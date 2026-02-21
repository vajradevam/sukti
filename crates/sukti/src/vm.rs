//! Tier-0 interpreter: frames (environments), closures, classes, exceptions,
//! modules and a deterministic cooperative task/channel scheduler.

use crate::ast::*;
use crate::diag::Span;
use crate::lexer::Lexer;
use crate::lexicon::Lexicon;
use crate::parser::Parser;
use crate::value::*;
use num_bigint::BigInt;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

pub struct VM {
    pub globals: EnvRef,
    pub output: String,
    pub lex: Lexicon,
    pub lang: String,
    pub modules: HashMap<String, Rc<Module>>,
    pub tasks: Vec<Rc<RefCell<Task>>>,
    pub channels: Vec<Rc<RefCell<Channel>>>,
    pub current_task: Option<usize>,
    pub trace_prakriya: bool,
    pub max_steps: usize,
    pub rng_state: u64,
    steps: usize,
    pub fault_class: Rc<Class>,
    pub tier1: Option<Box<dyn Tier1>>,
    /// Optional debugger hook invoked before each statement executes. Used by
    /// the DAP server to record deterministic execution traces.
    pub trace_hook: Option<Box<dyn FnMut(&mut VM, Span, &EnvRef)>>,
    source: Option<crate::diag::Source>,
}

impl VM {
    pub fn new(lang: &str) -> Self {
        let fault_class = Rc::new(Class {
            name: "doSha".into(),
            superclass: None,
            methods: HashMap::new(),
        });
        Self {
            globals: Env::root(),
            output: String::new(),
            lex: Lexicon::load(),
            lang: lang.into(),
            modules: HashMap::new(),
            tasks: Vec::new(),
            channels: Vec::new(),
            current_task: None,
            trace_prakriya: false,
            max_steps: 10_000_000,
            steps: 0,
            rng_state: 0x9e3779b97f4a7c15,
            fault_class,
            tier1: None,
            trace_hook: None,
            source: None,
        }
    }

    pub fn set_source(&mut self, s: crate::diag::Source) {
        self.source = Some(s);
    }

    pub fn add_native(&mut self, name: &str, n: Rc<dyn Native>) {
        self.globals
            .borrow_mut()
            .define(name, Value::Native(n), false);
    }
    pub fn add_native_fn<F>(&mut self, name: &str, label: &str, f: F)
    where
        F: Fn(&mut VM, Vec<Value>) -> VResult<Value> + 'static,
    {
        struct N<F>(String, F);
        impl<F> Native for N<F>
        where
            F: Fn(&mut VM, Vec<Value>) -> VResult<Value> + 'static,
        {
            fn name(&self) -> &str {
                &self.0
            }
            fn call(&self, vm: &mut VM, args: Vec<Value>) -> VResult<Value> {
                (self.1)(vm, args)
            }
        }
        self.add_native(name, Rc::new(N(label.to_string(), f)));
    }

    pub fn run(&mut self, stmts: &[Stmt]) -> VResult<()> {
        let env = self.globals.clone();
        match self.exec_block(stmts, &env)? {
            Flow::Return(_) | Flow::Normal => {}
            Flow::Break | Flow::Continue => {
                return Err(Fault::new("E0002", "break/continue outside a loop"))
            }
        }
        self.drain_tasks()
    }

    fn tick(&mut self, span: Option<Span>) -> VResult<()> {
        self.steps += 1;
        if self.steps > self.max_steps {
            let mut f = Fault::new("E0016", "step limit exceeded (--max-steps)");
            f.span = span;
            return Err(f);
        }
        Ok(())
    }

    // ------------------------------------------------------------- statements

    fn exec_block(&mut self, stmts: &[Stmt], env: &EnvRef) -> VResult<Flow> {
        for s in stmts {
            let f = self.exec_stmt(s, env)?;
            if !matches!(f, Flow::Normal) {
                return Ok(f);
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_stmt(&mut self, st: &Stmt, env: &EnvRef) -> VResult<Flow> {
        if self.trace_hook.is_some() {
            let mut hook = self.trace_hook.take();
            if let Some(h) = &mut hook {
                h(self, st.span, env);
            }
            self.trace_hook = hook;
        }
        self.tick(Some(st.span))?;
        match &st.kind {
            StmtKind::Expr(e) => {
                self.eval(e, env)?;
                Ok(Flow::Normal)
            }
            StmtKind::Let {
                name,
                init,
                mutable,
                ..
            } => {
                let v = match init {
                    Some(e) => self.eval(e, env)?,
                    None => Value::Null,
                };
                env.borrow_mut().define(name, v, *mutable);
                Ok(Flow::Normal)
            }
            StmtKind::Fn(f) => {
                let fun = self.make_func(f, env);
                env.borrow_mut()
                    .define(&f.name, Value::Func(Rc::new(fun)), false);
                Ok(Flow::Normal)
            }
            StmtKind::Return(e) => {
                let v = match e {
                    Some(x) => self.eval(x, env)?,
                    None => Value::Null,
                };
                Ok(Flow::Return(v))
            }
            StmtKind::If { cond, then, els } => {
                if self.eval(cond, env)?.truthy() {
                    self.exec_block(then, &Env::child(env.clone()))
                } else {
                    self.exec_block(els, &Env::child(env.clone()))
                }
            }
            StmtKind::While { cond, body } => {
                // On-stack replacement: the profiler may compile a native loop
                // kernel after the loop becomes hot and take over the rest of
                // the iteration sequence. The kernel writes carried variables
                // back to `env`, so execution resumes after the loop.
                // Compute eligibility once per loop entry (not per backedge)
                // so interpreted-only loops never pay the profiler cost.
                let osr_possible = self.tier1.is_some()
                    && jit_candidate(std::slice::from_ref(&Stmt {
                        kind: StmtKind::While {
                            cond: cond.clone(),
                            body: body.clone(),
                        },
                        span: st.span,
                    }));
                let site = LoopSite {
                    id: st.span.start as u64,
                    cond,
                    body,
                };
                loop {
                    self.tick(Some(st.span))?;
                    if !self.eval(cond, env)?.truthy() {
                        break;
                    }
                    if osr_possible {
                        let mut tier = self.tier1.take();
                        let taken = tier.as_mut().and_then(|t| t.observe_loop(&site, env, self));
                        self.tier1 = tier;
                        if let Some(res) = taken {
                            res?;
                            break;
                        }
                    }
                    match self.exec_block(body, &Env::child(env.clone()))? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::For { var, iter, body } => {
                let iter_val = self.eval(iter, env)?;
                let items = self.iterable(iter_val, st.span)?;
                for item in items {
                    self.tick(Some(st.span))?;
                    let child = Env::child(env.clone());
                    child.borrow_mut().define(var, item, true);
                    match self.exec_block(body, &child)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::Break => Ok(Flow::Break),
            StmtKind::Continue => Ok(Flow::Continue),
            StmtKind::Try {
                body,
                catch_var,
                catch_body,
                finally,
            } => {
                let result = self.exec_block(body, &Env::child(env.clone()));
                let mut flow = match result {
                    Ok(f) => Ok(f),
                    Err(fault) => {
                        let ce = Env::child(env.clone());
                        let err = self.fault_value(&fault);
                        ce.borrow_mut().define(catch_var, err, true);
                        self.exec_block(catch_body, &ce)
                    }
                };
                let fin = self.exec_block(finally, &Env::child(env.clone()))?;
                if !matches!(fin, Flow::Normal) {
                    flow = Ok(fin);
                }
                flow
            }
            StmtKind::Raise(e) => {
                let v = self.eval(e, env)?;
                Err(self.value_to_fault(v, Some(st.span)))
            }
            StmtKind::Class {
                name,
                superclass,
                members,
            } => {
                let sup = superclass
                    .as_ref()
                    .and_then(|s| env.borrow().get(s))
                    .and_then(|v| match v {
                        Value::Class(c) => Some(c),
                        _ => None,
                    });
                let mut methods: HashMap<String, Value> = HashMap::new();
                // Field initializers become default parameters of a synthetic
                // constructor, so instances get the declared defaults.
                let field_defaults: Vec<Param> = members
                    .iter()
                    .filter_map(|m| match m {
                        Member::Field { name, init, span } => Some(Param {
                            name: name.clone(),
                            role: None,
                            ty: None,
                            default: init.clone(),
                            span: *span,
                        }),
                        _ => None,
                    })
                    .collect();
                if !field_defaults.is_empty() {
                    methods.insert(
                        "__fields__".into(),
                        Value::Func(Rc::new(Func {
                            name: "__fields__".into(),
                            params: field_defaults,
                            body: Vec::new(),
                            closure: env.clone(),
                            arg_order: Vec::new(),
                            jit_hint: true,
                        })),
                    );
                }
                for m in members {
                    if let Member::Method(f) = m {
                        let fun = self.make_func(f, env);
                        methods.insert(f.name.clone(), Value::Func(Rc::new(fun)));
                    }
                }
                let cls = Rc::new(Class {
                    name: name.clone(),
                    superclass: sup,
                    methods,
                });
                env.borrow_mut().define(name, Value::Class(cls), false);
                Ok(Flow::Normal)
            }
            StmtKind::Import { path, alias } => self.import(path, alias.as_deref(), env),
            StmtKind::Export(_) => Ok(Flow::Normal),
            StmtKind::Sutra {
                name,
                params,
                clauses,
            } => {
                let mut existing = match env.borrow().get(name) {
                    Some(Value::Sutra(s)) => Some((*s).clone()),
                    _ => None,
                };
                for (i, c) in clauses.iter().enumerate() {
                    let cc = CompiledClause {
                        kind: c.kind,
                        guard: c.guard.clone(),
                        body: c.body.clone(),
                        order: i + existing.as_ref().map_or(0, |g| g.clauses.len()),
                        specificity: c.guard.as_ref().map_or(0, |e| count_comparisons(e)),
                    };
                    if let Some(g) = &mut existing {
                        g.clauses.push(cc);
                    } else {
                        existing = Some(SutraGroup {
                            name: name.clone(),
                            params: params.clone(),
                            clauses: vec![cc],
                            closure: env.clone(),
                        });
                    }
                }
                if let Some(g) = existing {
                    env.borrow_mut()
                        .define(name, Value::Sutra(Rc::new(g)), false);
                }
                Ok(Flow::Normal)
            }
            StmtKind::Task { name, body } => {
                let id = self.tasks.len();
                let tenv = Env::child(env.clone());
                let task = Rc::new(RefCell::new(Task {
                    id,
                    name: name.clone(),
                    body: body.clone(),
                    env: tenv,
                    finished: false,
                    blocked_on: None,
                }));
                self.tasks.push(task.clone());
                env.borrow_mut().define(name, Value::Task(task), false);
                Ok(Flow::Normal)
            }
            StmtKind::Channel { name } => {
                let id = self.channels.len();
                let c = Rc::new(RefCell::new(Channel {
                    messages: VecDeque::new(),
                    closed: false,
                    blocked_task: None,
                }));
                self.channels.push(c.clone());
                env.borrow_mut().define(name, Value::Channel(c, id), false);
                Ok(Flow::Normal)
            }
            StmtKind::Send { channel, value } => {
                let v = self.eval(value, env)?;
                let ch = env
                    .borrow()
                    .get(channel)
                    .and_then(|x| match x {
                        Value::Channel(c, _) => Some(c),
                        _ => None,
                    })
                    .ok_or_else(|| Fault::new("E0007", channel))?;
                ch.borrow_mut().messages.push_back(v);
                Ok(Flow::Normal)
            }
            StmtKind::Receive { var, channel } => {
                let ch = env
                    .borrow()
                    .get(channel)
                    .and_then(|x| match x {
                        Value::Channel(c, _) => Some(c),
                        _ => None,
                    })
                    .ok_or_else(|| Fault::new("E0007", channel))?;
                let msg = ch.borrow_mut().messages.pop_front();
                match msg {
                    Some(v) => {
                        env.borrow_mut().define(var, v, true);
                        Ok(Flow::Normal)
                    }
                    None => Err(Fault::new(
                        "E0011",
                        format!("task {} blocked on channel", self.current_task.unwrap_or(0)),
                    )),
                }
            }
            StmtKind::Join { .. } => Ok(Flow::Normal),
            StmtKind::Assert(e) => {
                let v = self.eval(e, env)?;
                if v.truthy() {
                    Ok(Flow::Normal)
                } else {
                    Err(Fault::new("E0017", "assertion failed"))
                }
            }
        }
    }

    fn make_func(&self, f: &FnDecl, env: &EnvRef) -> Func {
        Func {
            name: f.name.clone(),
            params: f.params.clone(),
            body: f.body.clone(),
            closure: env.clone(),
            arg_order: Vec::new(),
            jit_hint: jit_candidate(&f.body),
        }
    }

    pub fn fault_value(&self, f: &Fault) -> Value {
        let inst = Rc::new(RefCell::new(Instance {
            class: self.fault_class.clone(),
            fields: HashMap::from([
                ("sandesha".to_string(), Value::Str(f.message.clone())),
                ("kind".to_string(), Value::Str(f.kind.clone())),
            ]),
        }));
        Value::Instance(inst)
    }

    fn value_to_fault(&self, v: Value, span: Option<Span>) -> Fault {
        let kind = match &v {
            Value::Instance(i) => i
                .borrow()
                .fields
                .get("kind")
                .map(|x| x.display())
                .unwrap_or_else(|| "doSha".into()),
            _ => "doSha".into(),
        };
        let message = match &v {
            Value::Instance(i) => i
                .borrow()
                .fields
                .get("sandesha")
                .map(|x| x.display())
                .unwrap_or_else(|| v.display()),
            _ => v.display(),
        };
        let mut f = Fault::new(kind, message).with_value(v);
        f.span = span;
        f
    }

    fn import(&mut self, path: &str, alias: Option<&str>, env: &EnvRef) -> VResult<Flow> {
        if let Some(m) = self.modules.get(path).cloned() {
            let name = alias
                .unwrap_or(path)
                .split('/')
                .last()
                .unwrap_or(path)
                .to_string();
            env.borrow_mut().define(&name, Value::Module(m), false);
            return Ok(Flow::Normal);
        }
        // File module: read, parse, run into a fresh environment.
        let text = std::fs::read_to_string(path)
            .map_err(|_| Fault::new("E0007", format!("module {path} not found")))?;
        let src = crate::diag::Source::new(path, &text);
        let mut rep = crate::diag::Reporter::new(&self.lang);
        let toks = Lexer::new(&src, &self.lex, &mut rep).tokenize();
        let mut p = Parser::new(&src, &self.lex, &mut rep, toks);
        let stmts = p.parse_program();
        if rep.has_errors() {
            return Err(Fault::new("E0002", rep.render(&src)));
        }
        let menv = Env::child(self.globals.clone());
        match self.exec_block(&stmts, &menv)? {
            Flow::Normal | Flow::Return(_) => {}
            _ => return Err(Fault::new("E0002", "invalid module")),
        }
        let module = Rc::new(Module {
            name: path.to_string(),
            env: menv,
        });
        self.modules.insert(path.to_string(), module.clone());
        let name = alias
            .unwrap_or(path)
            .split('/')
            .last()
            .unwrap_or(path)
            .to_string();
        env.borrow_mut().define(&name, Value::Module(module), false);
        Ok(Flow::Normal)
    }

