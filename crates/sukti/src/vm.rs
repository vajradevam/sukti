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

    // -------------------------------------------------------------- expressions

    fn eval(&mut self, e: &Expr, env: &EnvRef) -> VResult<Value> {
        self.tick(Some(e.span))?;
        match &e.kind {
            ExprKind::Lit(l) => Ok(match l {
                Literal::Int(i) => Value::Int(i.clone()),
                Literal::Float(f) => Value::Float(*f),
                Literal::Str(s) => Value::Str(s.clone()),
                Literal::Bool(b) => Value::Bool(*b),
                Literal::Null => Value::Null,
            }),
            ExprKind::Var(name) => {
                // Keyword spellings (vada) and their internal ids (print) both
                // resolve to the same binding, so source and AST views agree.
                let direct = env
                    .borrow()
                    .get(name)
                    .or_else(|| self.globals.borrow().get(name));
                let via_dev = self
                    .lex
                    .canonical(name)
                    .and_then(|d| env.borrow().get(d).or_else(|| self.globals.borrow().get(d)));
                direct
                    .or(via_dev)
                    .ok_or_else(|| self.fault("E0007", &[("name", name.clone())], Some(e.span)))
            }
            ExprKind::Template(parts) => {
                let mut s = String::new();
                for p in parts {
                    match p {
                        StrPart::Text(t) => s.push_str(t),
                        StrPart::Expr(x, _) => s.push_str(&self.eval(x, env)?.display()),
                    }
                }
                Ok(Value::Str(s))
            }
            ExprKind::Unary(op, x) => {
                let v = self.eval(x, env)?;
                match op {
                    UnOp::Neg => match v {
                        Value::Int(i) => Ok(Value::Int(-i)),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        _ => Err(self.fault(
                            "E0008",
                            &[
                                ("expected", "number".into()),
                                ("found", v.type_name().into()),
                            ],
                            Some(e.span),
                        )),
                    },
                    UnOp::Not => Ok(Value::Bool(!v.truthy())),
                }
            }
            ExprKind::Binary(op, l, r) => {
                if *op == BinOp::And {
                    let lv = self.eval(l, env)?;
                    return if lv.truthy() {
                        self.eval(r, env)
                    } else {
                        Ok(lv)
                    };
                }
                if *op == BinOp::Or {
                    let lv = self.eval(l, env)?;
                    return if lv.truthy() {
                        Ok(lv)
                    } else {
                        self.eval(r, env)
                    };
                }
                let a = self.eval(l, env)?;
                let b = self.eval(r, env)?;
                self.binary(*op, a, b, e.span)
            }
            ExprKind::Assign(target, value) => {
                let v = self.eval(value, env)?;
                match &target.kind {
                    ExprKind::Var(name) => {
                        if !env.borrow_mut().assign(name, v.clone()) {
                            return Err(self.fault(
                                "E0007",
                                &[("name", name.clone())],
                                Some(e.span),
                            ));
                        }
                    }
                    ExprKind::Get { obj, name } => {
                        let o = self.eval(obj, env)?;
                        match o {
                            Value::Instance(i) => {
                                i.borrow_mut().fields.insert(name.clone(), v.clone());
                            }
                            _ => {
                                return Err(self.fault(
                                    "E0008",
                                    &[
                                        ("expected", "object".into()),
                                        ("found", o.type_name().into()),
                                    ],
                                    Some(e.span),
                                ))
                            }
                        }
                    }
                    ExprKind::Index { obj, index } => {
                        let o = self.eval(obj, env)?;
                        let i = self.eval(index, env)?;
                        self.index_set(&o, &i, v.clone(), e.span)?;
                    }
                    _ => {
                        return Err(self.fault(
                            "E0002",
                            &[
                                ("expected", "assignable".into()),
                                ("found", "expression".into()),
                            ],
                            Some(e.span),
                        ))
                    }
                }
                Ok(v)
            }
            ExprKind::Call { callee, args } => {
                let f = self.eval(callee, env)?;
                let mut vals = Vec::with_capacity(args.len());
                let mut kinds = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(self.eval(&a.expr, env)?);
                    kinds.push(a.kind.clone());
                }
                self.call(f, vals, Some(kinds), e.span)
            }
            ExprKind::Get { obj, name } => {
                let o = self.eval(obj, env)?;
                self.get(&o, name, e.span)
            }
            ExprKind::Index { obj, index } => {
                let o = self.eval(obj, env)?;
                let i = self.eval(index, env)?;
                self.index_get(&o, &i, e.span)
            }
            ExprKind::List(v) => {
                let mut out = Vec::new();
                for x in v {
                    out.push(self.eval(x, env)?);
                }
                Ok(Value::List(Rc::new(RefCell::new(out))))
            }
            ExprKind::Tuple(v) => {
                let mut out = Vec::new();
                for x in v {
                    out.push(self.eval(x, env)?);
                }
                Ok(Value::Tuple(Rc::new(out)))
            }
            ExprKind::Map(v) => {
                let mut out = Vec::new();
                for (k, x) in v {
                    let key = self.eval(k, env)?;
                    let val = self.eval(x, env)?;
                    out.push((key, val));
                }
                Ok(Value::Map(Rc::new(RefCell::new(out))))
            }
            ExprKind::Range {
                from,
                to,
                inclusive,
            } => {
                let av = self.eval(from, env)?;
                let a = self.as_int(&av, e.span)?;
                let bv = self.eval(to, env)?;
                let b = self.as_int(&bv, e.span)?;
                Ok(Value::Range(
                    a,
                    if *inclusive { b + BigInt::from(1) } else { b },
                ))
            }
            ExprKind::Lambda { params, body } => Ok(Value::Func(Rc::new(Func {
                name: "<kriyA>".into(),
                params: params.clone(),
                body: body.clone(),
                closure: env.clone(),
                arg_order: Vec::new(),
                jit_hint: jit_candidate(body),
            }))),
            ExprKind::IfExpr { cond, then, els } => {
                if self.eval(cond, env)?.truthy() {
                    self.eval(then, env)
                } else {
                    self.eval(els, env)
                }
            }
            ExprKind::Block(stmts) => {
                let child = Env::child(env.clone());
                match self.exec_block(stmts, &child)? {
                    Flow::Return(v) => Ok(v),
                    Flow::Normal => Ok(Value::Null),
                    _ => Ok(Value::Null),
                }
            }
            ExprKind::Set { obj, name, value } => {
                let o = self.eval(obj, env)?;
                let v = self.eval(value, env)?;
                if let Value::Instance(i) = o {
                    i.borrow_mut().fields.insert(name.clone(), v.clone());
                    return Ok(v);
                }
                Err(self.fault(
                    "E0008",
                    &[("expected", "object".into()), ("found", "?".into())],
                    Some(e.span),
                ))
            }
        }
    }

    fn fault(&self, code: &str, args: &[(&str, String)], span: Option<Span>) -> Fault {
        let mut f = Fault::new(code, self.lex.message(code, &self.lang, args));
        f.span = span;
        f
    }

    fn as_int(&self, v: &Value, span: Span) -> VResult<BigInt> {
        match v {
            Value::Int(i) => Ok(i.clone()),
            _ => Err(self.fault(
                "E0008",
                &[
                    ("expected", "pUrNA~nka".into()),
                    ("found", v.type_name().into()),
                ],
                Some(span),
            )),
        }
    }

    fn binary(&self, op: BinOp, a: Value, b: Value, span: Span) -> VResult<Value> {
        use BinOp::*;
        let err = || {
            self.fault(
                "E0008",
                &[
                    ("expected", "number".into()),
                    ("found", format!("{} and {}", a.type_name(), b.type_name())),
                ],
                Some(span),
            )
        };
        match op {
            Add => match (&a, &b) {
                (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x + y)),
                (Value::Str(x), Value::Str(y)) => Ok(Value::Str(format!("{x}{y}"))),
                (Value::Str(x), _) => Ok(Value::Str(format!("{}{}", x, b.display()))),
                (_, Value::Str(y)) => Ok(Value::Str(format!("{}{}", a.display(), y))),
                _ => {
                    let x = num(&a).ok_or_else(err)?;
                    let y = num(&b).ok_or_else(err)?;
                    Ok(Value::Float(x + y))
                }
            },
            Sub | Mul | Div | Mod | Pow => {
                if let (Value::Int(x), Value::Int(y)) = (&a, &b) {
                    return match op {
                        Sub => Ok(Value::Int(x - y)),
                        Mul => Ok(Value::Int(x * y)),
                        Div => {
                            if *y == BigInt::from(0) {
                                return Err(self.fault("E0012", &[], Some(span)));
                            }
                            Ok(Value::Int(x / y))
                        }
                        Mod => {
                            if *y == BigInt::from(0) {
                                return Err(self.fault("E0012", &[], Some(span)));
                            }
                            Ok(Value::Int(x % y))
                        }
                        Pow => {
                            let e: u32 = y.to_string().parse().map_err(|_| {
                                self.fault(
                                    "E0008",
                                    &[
                                        ("expected", "nonnegative exponent".into()),
                                        ("found", y.to_string()),
                                    ],
                                    Some(span),
                                )
                            })?;
                            Ok(Value::Int(x.pow(e)))
                        }
                        _ => unreachable!(),
                    };
                }
                let x = num(&a).ok_or_else(err)?;
                let y = num(&b).ok_or_else(err)?;
                Ok(Value::Float(match op {
                    Sub => x - y,
                    Mul => x * y,
                    Div => {
                        if y == 0.0 {
                            return Err(self.fault("E0012", &[], Some(span)));
                        }
                        x / y
                    }
                    Mod => x % y,
                    Pow => x.powf(y),
                    _ => unreachable!(),
                }))
            }
            Eq => Ok(Value::Bool(value_eq(&a, &b))),
            Ne => Ok(Value::Bool(!value_eq(&a, &b))),
            Lt | Le | Gt | Ge => {
                if let (Value::Str(x), Value::Str(y)) = (&a, &b) {
                    return Ok(Value::Bool(match op {
                        Lt => x < y,
                        Le => x <= y,
                        Gt => x > y,
                        Ge => x >= y,
                        _ => false,
                    }));
                }
                let x = num(&a).ok_or_else(err)?;
                let y = num(&b).ok_or_else(err)?;
                Ok(Value::Bool(match op {
                    Lt => x < y,
                    Le => x <= y,
                    Gt => x > y,
                    Ge => x >= y,
                    _ => false,
                }))
            }
            And | Or => unreachable!(),
        }
    }

    fn iterable(&self, v: Value, span: Span) -> VResult<Vec<Value>> {
        match v {
            Value::List(l) => Ok(l.borrow().clone()),
            Value::Tuple(t) => Ok((*t).clone()),
            Value::Range(a, b) => {
                let mut out = Vec::new();
                let mut i = a;
                while i < b {
                    out.push(Value::Int(i.clone()));
                    i += BigInt::from(1);
                }
                Ok(out)
            }
            Value::Str(s) => Ok(s.chars().map(|c| Value::Str(c.to_string())).collect()),
            Value::Map(m) => Ok(m.borrow().iter().map(|(k, _)| k.clone()).collect()),
            _ => Err(self.fault(
                "E0008",
                &[("expected", "iterable".into()), ("found", "?".into())],
                Some(span),
            )),
        }
    }

    fn index_get(&self, o: &Value, i: &Value, span: Span) -> VResult<Value> {
        match o {
            Value::List(l) => {
                let n = self.as_int(i, span)?;
                let l = l.borrow();
                let idx = norm_index(&n, l.len()).ok_or_else(|| {
                    self.fault(
                        "E0013",
                        &[
                            ("index", n.to_string()),
                            ("container", "sUchI".into()),
                            ("len", l.len().to_string()),
                        ],
                        Some(span),
                    )
                })?;
                Ok(l[idx].clone())
            }
            Value::Tuple(t) => {
                let n = self.as_int(i, span)?;
                let idx = norm_index(&n, t.len()).ok_or_else(|| {
                    self.fault(
                        "E0013",
                        &[
                            ("index", n.to_string()),
                            ("container", "Tupala".into()),
                            ("len", t.len().to_string()),
                        ],
                        Some(span),
                    )
                })?;
                Ok(t[idx].clone())
            }
            Value::Map(m) => m
                .borrow()
                .iter()
                .find(|(k, _)| value_eq(k, i))
                .map(|(_, v)| v.clone())
                .ok_or_else(|| {
                    self.fault(
                        "E0013",
                        &[
                            ("index", i.display()),
                            ("container", "kosha".into()),
                            ("len", "?".into()),
                        ],
                        Some(span),
                    )
                }),
            Value::Str(s) => {
                let n = self.as_int(i, span)?;
                let chars: Vec<char> = s.chars().collect();
                let idx = norm_index(&n, chars.len()).ok_or_else(|| {
                    self.fault(
                        "E0013",
                        &[
                            ("index", n.to_string()),
                            ("container", "shabda".into()),
                            ("len", chars.len().to_string()),
                        ],
                        Some(span),
                    )
                })?;
                Ok(Value::Str(chars[idx].to_string()))
            }
            _ => Err(self.fault(
                "E0008",
                &[
                    ("expected", "indexable".into()),
                    ("found", o.type_name().into()),
                ],
                Some(span),
            )),
        }
    }

    fn index_set(&self, o: &Value, i: &Value, v: Value, span: Span) -> VResult<()> {
        match o {
            Value::List(l) => {
                let n = self.as_int(i, span)?;
                let mut l = l.borrow_mut();
                let idx = norm_index(&n, l.len()).ok_or_else(|| {
                    self.fault(
                        "E0013",
                        &[
                            ("index", n.to_string()),
                            ("container", "sUchI".into()),
                            ("len", l.len().to_string()),
                        ],
                        Some(span),
                    )
                })?;
                l[idx] = v;
                Ok(())
            }
            Value::Map(m) => {
                let mut m = m.borrow_mut();
                if let Some(p) = m.iter_mut().find(|(k, _)| value_eq(k, i)) {
                    p.1 = v;
                } else {
                    m.push((i.clone(), v));
                }
                Ok(())
            }
            _ => Err(self.fault(
                "E0008",
                &[
                    ("expected", "indexable".into()),
                    ("found", o.type_name().into()),
                ],
                Some(span),
            )),
        }
    }

    fn get(&mut self, o: &Value, name: &str, span: Span) -> VResult<Value> {
        match o {
            Value::Instance(inst) => {
                let inst = inst.clone();
                if let Some(v) = inst.borrow().fields.get(name).cloned() {
                    return Ok(v);
                }
                if let Some(m) = inst.borrow().class.find_method(name) {
                    return Ok(bind_method(&m, o.clone(), &inst.borrow().class));
                }
                Err(self.fault("E0007", &[("name", name.into())], Some(span)))
            }
            Value::Class(c) => {
                if let Some(m) = c.find_method(name) {
                    Ok(m)
                } else {
                    Err(self.fault("E0007", &[("name", name.into())], Some(span)))
                }
            }
            Value::Super(c, inst) => {
                if let Some(m) = c.find_method(name) {
                    Ok(bind_method(&m, Value::Instance(inst.clone()), c))
                } else {
                    Err(self.fault("E0007", &[("name", name.into())], Some(span)))
                }
            }
            Value::Module(m) => m
                .env
                .borrow()
                .get(name)
                .ok_or_else(|| self.fault("E0007", &[("name", name.into())], Some(span))),
            Value::List(_)
            | Value::Str(_)
            | Value::Map(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::Tuple(_)
            | Value::Channel(..) => crate::stdlib::get_method(o.clone(), name)
                .ok_or_else(|| self.fault("E0007", &[("name", name.into())], Some(span))),
            _ => Err(self.fault("E0007", &[("name", name.into())], Some(span))),
        }
    }

    pub fn call(
        &mut self,
        f: Value,
        args: Vec<Value>,
        kinds: Option<Vec<ArgKind>>,
        span: Span,
    ) -> VResult<Value> {
        match f {
            Value::Native(n) => n.call(self, args),
            Value::Func(fun) => {
                // Tier-1: only functions whose body is a possible integer JIT
                // candidate reach the profiler hook. This keeps method- and
                // list-heavy programs at exactly Tier-0 speed when the JIT is
                // enabled (the "never slower" clause of the DoD).
                if fun.jit_hint {
                    let mut tier = self.tier1.take();
                    let handled = tier.as_mut().and_then(|t| t.observe(&fun, &args, self));
                    self.tier1 = tier;
                    if let Some(r) = handled {
                        return r;
                    }
                }
                let env = Env::child(fun.closure.clone());
                let mut pos = 0usize;
                for (i, p) in fun.params.iter().enumerate() {
                    let supplied = if !fun.arg_order.is_empty() {
                        fun.arg_order.get(i).and_then(|j| args.get(*j)).cloned()
                    } else if let Some(ks) = &kinds {
                        if matches!(ks.get(i), Some(ArgKind::Omitted)) {
                            None
                        } else {
                            let named = ks.iter().position(|k| match k {
                                ArgKind::Named(n) => n == &p.name,
                                ArgKind::Role(r) => p.role == Some(*r),
                                _ => false,
                            });
                            match named {
                                Some(j) => args.get(j).cloned(),
                                None => {
                                    while matches!(ks.get(pos), Some(ArgKind::Omitted)) {
                                        pos += 1;
                                    }
                                    let v = args.get(pos).cloned();
                                    if v.is_some() {
                                        pos += 1;
                                    }
                                    v
                                }
                            }
                        }
                    } else {
                        args.get(i).cloned()
                    };
                    let val = match supplied {
                        Some(v) => v,
                        None => match &p.default {
                            Some(d) => self.eval(d, &env)?,
                            None => {
                                return Err(self.fault(
                                    "E0004",
                                    &[(
                                        "role",
                                        p.role
                                            .map(|r| r.short().to_string())
                                            .unwrap_or_else(|| p.name.clone()),
                                    )],
                                    Some(span),
                                ))
                            }
                        },
                    };
                    env.borrow_mut().define(&p.name, val, true);
                }
                match self.exec_block(&fun.body, &env)? {
                    Flow::Return(v) => Ok(v),
                    Flow::Normal => Ok(Value::Null),
                    _ => Ok(Value::Null),
                }
            }
            Value::Class(c) => {
                let inst = Rc::new(RefCell::new(Instance {
                    class: c.clone(),
                    fields: HashMap::new(),
                }));
                // Copy defaults from the class's field table (stored as a synthetic method name).
                if let Some(Value::Func(f)) = c.methods.get("__fields__") {
                    for p in &f.params {
                        if let Some(d) = &p.default {
                            let ge = self.globals.clone();
                            let v = self.eval(d, &ge)?;
                            inst.borrow_mut().fields.insert(p.name.clone(), v);
                        }
                    }
                }
                if let Some(init) = c.find_method("nirmANa") {
                    let bound = bind_method(&init, Value::Instance(inst.clone()), &c);
                    self.call(bound, args, kinds, span)?;
                }
                Ok(Value::Instance(inst))
            }
            Value::Sutra(g) => self.call_sutra(&g, args, kinds, span),
            _ => Err(self.fault(
                "E0008",
                &[
                    ("expected", "kriyA".into()),
                    ("found", f.type_name().into()),
                ],
                Some(span),
            )),
        }
    }

    fn call_sutra(
        &mut self,
        g: &Rc<SutraGroup>,
        args: Vec<Value>,
        kinds: Option<Vec<ArgKind>>,
        span: Span,
    ) -> VResult<Value> {
        let env = Env::child(g.closure.clone());
        let mut pos = 0;
        for (i, p) in g.params.iter().enumerate() {
            let v = args
                .get(if kinds.is_some() { pos } else { i })
                .cloned()
                .unwrap_or(Value::Null);
            pos += 1;
            env.borrow_mut().define(&p.name, v, true);
        }
        let mut applicable: Vec<(usize, &CompiledClause)> = Vec::new();
        for c in &g.clauses {
            let ok = match &c.guard {
                None => true,
                Some(x) => self.eval(x, &env)?.truthy(),
            };
            if ok {
                applicable.push((c.order, c));
            }
        }
        if applicable.is_empty() {
            return Ok(Value::Null);
        }
        // apavāda beats utsarga; among equals, the later clause wins.
        let best_spec = applicable
            .iter()
            .map(|(_, c)| c.specificity)
            .max()
            .unwrap_or(0);
        let apavadas: Vec<_> = applicable
            .iter()
            .filter(|(_, c)| c.kind == SutraKind::Apavada && c.specificity == best_spec)
            .collect();
        let winner = if !apavadas.is_empty() {
            if apavadas.len() > 1 {
                return Err(self.fault(
                    "E0006",
                    &[
                        ("a", format!("clause#{}", apavadas[0].1.order)),
                        ("b", format!("clause#{}", apavadas[1].1.order)),
                    ],
                    Some(span),
                ));
            }
            apavadas[0]
        } else {
            applicable.iter().max_by_key(|(o, _)| *o).unwrap()
        };
        if self.trace_prakriya {
            self.output.push_str(&format!(
                "[prakriyA] sUtra {} → clause#{} ({}) specificity={}\n",
                g.name,
                winner.1.order,
                if winner.1.kind == SutraKind::Apavada {
                    "apavAda"
                } else {
                    "utsarga"
                },
                winner.1.specificity
            ));
        }
        match self.exec_block(&winner.1.body, &Env::child(env.clone()))? {
            Flow::Return(v) => Ok(v),
            Flow::Normal => Ok(Value::Null),
            _ => Ok(Value::Null),
        }
    }

    // ------------------------------------------------------------ task scheduler

    fn drain_tasks(&mut self) -> VResult<()> {
        let mut progress = true;
        while progress {
            progress = false;
            let ids: Vec<usize> = self.tasks.iter().enumerate().map(|(i, _)| i).collect();
            for id in ids {
                let t = self.tasks[id].clone();
                if t.borrow().finished || t.borrow().blocked_on.is_some() {
                    continue;
                }
                self.current_task = Some(id);
                let body = t.borrow().body.clone();
                let env = t.borrow().env.clone();
                let res = self.exec_block(&body, &Env::child(env.clone()));
                match res {
                    Ok(_) => {
                        t.borrow_mut().finished = true;
                        progress = true;
                    }
                    Err(f) if f.kind == "E0011" => {
                        t.borrow_mut().blocked_on = Some(0);
                    }
                    Err(f) => return Err(f),
                }
            }
        }
        let blocked: Vec<String> = self
            .tasks
            .iter()
            .filter(|t| t.borrow().blocked_on.is_some())
            .map(|t| t.borrow().name.clone())
            .collect();
        if !blocked.is_empty() {
            return Err(self.fault("E0011", &[("tasks", blocked.join(", "))], None));
        }
        self.current_task = None;
        Ok(())
    }
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => i.to_string().parse().ok(),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn norm_index(n: &BigInt, len: usize) -> Option<usize> {
    let v: i64 = n.to_string().parse().ok()?;
    let l = len as i64;
    let idx = if v < 0 { l + v } else { v };
    if idx >= 0 && idx < l {
        Some(idx as usize)
    } else {
        None
    }
}

fn count_comparisons(e: &Expr) -> usize {
    match &e.kind {
        ExprKind::Binary(op, a, b) => {
            let n = if matches!(
                op,
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
            ) {
                1
            } else {
                0
            };
            n + count_comparisons(a) + count_comparisons(b)
        }
        ExprKind::Unary(_, a) => count_comparisons(a),
        _ => 0,
    }
}

pub fn bind_method(m: &Value, recv: Value, class: &Rc<Class>) -> Value {
    match m {
        Value::Func(f) => {
            let env = Env::child(f.closure.clone());
            env.borrow_mut().define("svayam", recv.clone(), false);
            if let Some(sup) = &class.superclass {
                env.borrow_mut().define(
                    "mUla",
                    Value::Super(
                        sup.clone(),
                        match &recv {
                            Value::Instance(i) => i.clone(),
                            _ => unreachable!(),
                        },
                    ),
                    false,
                );
            }
            Value::Func(Rc::new(Func {
                name: f.name.clone(),
                params: f.params.clone(),
                body: f.body.clone(),
                closure: env,
                arg_order: f.arg_order.clone(),
                jit_hint: f.jit_hint,
            }))
        }
        _ => m.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(src: &str) -> (String, Option<Fault>) {
        let mut vm = VM::new("en");
        crate::stdlib::register(&mut vm);
        let s = crate::diag::Source::new("t", src);
        let mut rep = crate::diag::Reporter::new("en");
        let toks = Lexer::new(&s, &vm.lex, &mut rep).tokenize();
        let mut p = Parser::new(&s, &vm.lex, &mut rep, toks);
        let st = p.parse_program();
        assert!(!rep.has_errors(), "{}", rep.render(&s));
        let r = vm.run(&st);
        (vm.output, r.err())
    }
    #[test]
    fn hello_and_arithmetic() {
        let (o, e) = run("vada(\"namaste, vishva!\"); vada(2 + 3);");
        assert!(e.is_none(), "{:?}", e);
        assert_eq!(o, "namaste, vishva!\n5\n");
    }
    #[test]
    fn factorial() {
        let (o, e) = run(
            "kriyA kramaguNita(na) { yadi (na <= 1) { phalam 1; } phalam na * kramaguNita(na - 1); } vada(kramaguNita(5));",
        );
        assert!(e.is_none(), "{:?}", e);
        assert_eq!(o, "120\n");
    }
}
