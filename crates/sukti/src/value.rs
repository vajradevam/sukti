//! Runtime values, environments, objects, tasks and channels.

use crate::ast::{BinOp, Expr, ExprKind, Literal, Param, Stmt, StmtKind, UnOp};
use crate::diag::Span;
use num_bigint::BigInt;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct Fault {
    pub kind: String,
    pub message: String,
    pub value: Option<Value>,
    pub span: Option<Span>,
}
impl Fault {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            value: None,
            span: None,
        }
    }
    pub fn with_value(mut self, v: Value) -> Self {
        self.value = Some(v);
        self
    }
}
pub type VResult<T> = Result<T, Fault>;

#[derive(Clone)]
pub struct Binding {
    pub value: Value,
    pub mutable: bool,
}

pub type EnvRef = Rc<RefCell<Env>>;

#[derive(Clone)]
pub struct Env {
    pub vars: HashMap<String, Binding>,
    pub parent: Option<EnvRef>,
}
impl Env {
    pub fn root() -> EnvRef {
        Rc::new(RefCell::new(Self {
            vars: HashMap::new(),
            parent: None,
        }))
    }
    pub fn child(parent: EnvRef) -> EnvRef {
        Rc::new(RefCell::new(Self {
            vars: HashMap::new(),
            parent: Some(parent),
        }))
    }
    pub fn define(&mut self, name: &str, value: Value, mutable: bool) {
        self.vars
            .insert(name.to_string(), Binding { value, mutable });
    }
    pub fn get(&self, name: &str) -> Option<Value> {
        self.vars
            .get(name)
            .map(|b| b.value.clone())
            .or_else(|| self.parent.as_ref().and_then(|p| p.borrow().get(name)))
    }
    /// Assign to an existing binding, walking up the chain. Returns false if
    /// the name is unknown or the binding is constant.
    pub fn assign(&mut self, name: &str, value: Value) -> bool {
        if let Some(b) = self.vars.get_mut(name) {
            if !b.mutable {
                return false;
            }
            b.value = value;
            return true;
        }
        if let Some(p) = &self.parent {
            return p.borrow_mut().assign(name, value);
        }
        false
    }
    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

#[derive(Clone)]
pub struct Func {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub closure: EnvRef,
    /// Compile-time kāraka rewrite: for each parameter index, which argument
    /// index supplies it. Empty means plain positional.
    pub arg_order: Vec<usize>,
    /// Cheap, conservative hint computed once when the function is created.
    /// The VM uses it to skip the Tier-1 hook entirely for functions that can
    /// never be JIT-compiled (methods, string/list code, ...). This keeps the
    /// compiled tier from adding measurable overhead to non-numeric programs.
    pub jit_hint: bool,
}

/// A conservative scan used to set `Func::jit_hint`. It never claims a
/// function is eligible when it uses a construct the JIT cannot compile, but
/// it may reject a function that a more precise analysis could accept.
pub fn jit_candidate(body: &[Stmt]) -> bool {
    fn expr(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Lit(Literal::Int(_)) | ExprKind::Var(_) => true,
            ExprKind::Unary(UnOp::Neg, x) | ExprKind::Unary(UnOp::Not, x) => expr(x),
            ExprKind::Binary(op, a, b) => {
                matches!(
                    op,
                    BinOp::Add
                        | BinOp::Sub
                        | BinOp::Mul
                        | BinOp::Div
                        | BinOp::Mod
                        | BinOp::Eq
                        | BinOp::Ne
                        | BinOp::Lt
                        | BinOp::Le
                        | BinOp::Gt
                        | BinOp::Ge
                ) && expr(a)
                    && expr(b)
            }
            ExprKind::Assign(a, b) => matches!(a.kind, ExprKind::Var(_)) && expr(a) && expr(b),
            ExprKind::Call { callee, args } => {
                matches!(callee.kind, ExprKind::Var(_)) && args.iter().all(|a| expr(&a.expr))
            }
            _ => false,
        }
    }
    fn block(b: &[Stmt]) -> bool {
        b.iter().all(|s| match &s.kind {
            StmtKind::Let { init, .. } => init.as_ref().map_or(true, expr),
            StmtKind::Expr(e) => expr(e),
            StmtKind::Return(Some(e)) => expr(e),
            StmtKind::Return(None) => true,
            StmtKind::If { cond, then, els } => expr(cond) && block(then) && block(els),
            StmtKind::While { cond, body } => expr(cond) && block(body),
            StmtKind::For { iter, body, .. } => {
                matches!(iter.kind, ExprKind::Range { .. }) && expr(iter) && block(body)
            }
            StmtKind::Break | StmtKind::Continue => true,
            _ => false,
        })
    }
    block(body)
}

pub trait Native {
    fn name(&self) -> &str;
    fn call(&self, vm: &mut crate::vm::VM, args: Vec<Value>) -> VResult<Value>;
}

/// A loop header encountered by the interpreter. The Tier-1 engine may compile
/// a native kernel for it and take over execution (on-stack replacement).
pub struct LoopSite<'a> {
    /// Stable identity: the byte offset of the `yAvat` statement.
    pub id: u64,
    pub cond: &'a Expr,
    pub body: &'a [Stmt],
}

/// Tier-1 engine hook. Implemented by `sukti-jit`; kept as a trait here so the
/// runtime crate does not depend on Cranelift.
pub trait Tier1 {
    /// Called on every (JIT-eligible) function call. Returns `Some(result)`
    /// when the call was handled in compiled code, `None` to fall back to the
    /// interpreter. A `None` result is also how overflow/yield deopts are
    /// signalled: the interpreter then re-executes the call with bigint values.
    fn observe(
        &mut self,
        f: &Func,
        args: &[Value],
        vm: &mut crate::vm::VM,
    ) -> Option<VResult<Value>>;
    /// Called at every `yAvat` (while) loop backedge. `None` means "keep
    /// interpreting"; `Some(Ok(()))` means the native kernel ran the loop to
    /// completion and the interpreter should continue after the loop.
    fn observe_loop(
        &mut self,
        _site: &LoopSite<'_>,
        _env: &EnvRef,
        _vm: &mut crate::vm::VM,
    ) -> Option<VResult<()>> {
        None
    }
    fn dump(&self) -> String;
    fn stats(&self) -> String;
}

#[derive(Clone)]
pub struct Class {
    pub name: String,
    pub superclass: Option<Rc<Class>>,
    pub methods: HashMap<String, Value>,
}
impl Class {
    pub fn find_method(&self, name: &str) -> Option<Value> {
        self.methods
            .get(name)
            .cloned()
            .or_else(|| self.superclass.as_ref().and_then(|s| s.find_method(name)))
    }
    pub fn is_subclass_of(&self, other: &Rc<Class>) -> bool {
        std::ptr::eq(self, other.as_ref())
            || self
                .superclass
                .as_ref()
                .map_or(false, |s| s.is_subclass_of(other))
    }
}

#[derive(Clone)]
pub struct Instance {
    pub class: Rc<Class>,
    pub fields: HashMap<String, Value>,
}

#[derive(Clone)]
pub struct Module {
    pub name: String,
    pub env: EnvRef,
}

#[derive(Clone)]
pub struct CompiledClause {
    pub kind: crate::ast::SutraKind,
    pub guard: Option<crate::ast::Expr>,
    pub body: Vec<Stmt>,
    pub order: usize,
    pub specificity: usize,
}

#[derive(Clone)]
pub struct SutraGroup {
    pub name: String,
    pub params: Vec<Param>,
    pub clauses: Vec<CompiledClause>,
    pub closure: EnvRef,
}

#[derive(Clone)]
pub struct Channel {
    pub messages: VecDeque<Value>,
    pub closed: bool,
    pub blocked_task: Option<usize>,
}

#[derive(Clone)]
pub struct Task {
    pub id: usize,
    pub name: String,
    pub body: Vec<Stmt>,
    pub env: EnvRef,
    pub finished: bool,
    pub blocked_on: Option<usize>, // channel id
}

#[derive(Clone)]
pub enum Value {
    Int(BigInt),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
    List(Rc<RefCell<Vec<Value>>>),
    Tuple(Rc<Vec<Value>>),
    Map(Rc<RefCell<Vec<(Value, Value)>>>),
    Range(BigInt, BigInt),
    Func(Rc<Func>),
    Native(Rc<dyn Native>),
    Class(Rc<Class>),
    Instance(Rc<RefCell<Instance>>),
    Super(Rc<Class>, Rc<RefCell<Instance>>),
    Module(Rc<Module>),
    Sutra(Rc<SutraGroup>),
    Channel(Rc<RefCell<Channel>>, usize),
    Task(Rc<RefCell<Task>>),
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.display())
    }
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(b) => *b,
            Self::Int(i) => i != &BigInt::from(0),
            Self::Float(f) => *f != 0.0,
            Self::Str(s) => !s.is_empty(),
            Self::List(l) => !l.borrow().is_empty(),
            Self::Tuple(t) => !t.is_empty(),
            Self::Map(m) => !m.borrow().is_empty(),
            _ => true,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Int(i) => i.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Str(s) => s.clone(),
            Self::Bool(b) => {
                if *b {
                    "satya".into()
                } else {
                    "asatya".into()
                }
            }
            Self::Null => "shUnya".into(),
            Self::List(l) => format!(
                "[{}]",
                l.borrow()
                    .iter()
                    .map(|v| v.display())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Tuple(t) => format!(
                "({})",
                t.iter().map(|v| v.display()).collect::<Vec<_>>().join(", ")
            ),
            Self::Map(m) => format!(
                "{{{}}}",
                m.borrow()
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.display(), v.display()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Range(a, b) => format!("{}..{}", a, b),
            Self::Func(f) => format!("<kriyA {}>", f.name),
            Self::Native(n) => format!("<kriyA {}>", n.name()),
            Self::Class(c) => format!("<varga {}>", c.name),
            Self::Instance(i) => format!("< instance {} >", i.borrow().class.name),
            Self::Super(c, _) => format!("<mUla {}>", c.name),
            Self::Module(m) => format!("<AyAta {}>", m.name),
            Self::Sutra(s) => format!("<sUtra {}>", s.name),
            Self::Channel(_, id) => format!("<sandeshavAhaka #{id}>"),
            Self::Task(t) => format!("<tantu {}>", t.borrow().name),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Int(_) => "pUrNA~nka",
            Self::Float(_) => "dashamalava",
            Self::Str(_) => "shabda",
            Self::Bool(_) => "bUliyana",
            Self::Null => "shUnya",
            Self::List(_) => "sUchI",
            Self::Tuple(_) => "Tupala",
            Self::Map(_) => "kosha",
            Self::Range(..) => "shreNI",
            Self::Func(_) | Self::Native(_) => "kriyA",
            Self::Class(_) => "varga",
            Self::Instance(_) => "instance",
            Self::Module(_) => "AyAta",
            Self::Sutra(_) => "sUtra",
            Self::Channel(..) => "sandeshavAhaka",
            Self::Task(_) => "tantu",
            Self::Super(..) => "mUla",
        }
    }
}

pub fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => {
            let f: f64 = x.to_string().parse().unwrap_or(f64::NAN);
            f == *y
        }
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::List(x), Value::List(y)) => {
            let (x, y) = (x.borrow(), y.borrow());
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| value_eq(a, b))
        }
        (Value::Tuple(x), Value::Tuple(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| value_eq(a, b))
        }
        (Value::Map(x), Value::Map(y)) => {
            let (x, y) = (x.borrow(), y.borrow());
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.iter().any(|(k2, v2)| value_eq(k, k2) && value_eq(v, v2)))
        }
        (Value::Class(x), Value::Class(y)) => Rc::ptr_eq(x, y),
        (Value::Instance(x), Value::Instance(y)) => Rc::ptr_eq(x, y),
        (Value::Channel(x, _), Value::Channel(y, _)) => Rc::ptr_eq(x, y),
        (Value::Func(x), Value::Func(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

pub fn hash_key(v: &Value) -> String {
    match v {
        Value::Int(i) => format!("i{i}"),
        Value::Float(f) => format!("f{f}"),
        Value::Str(s) => format!("s{s}"),
        Value::Bool(b) => format!("b{b}"),
        Value::Null => "n".into(),
        _ => format!("o{}", v.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn env_and_truthiness() {
        let e = Env::root();
        e.borrow_mut().define("na", Value::Int(5.into()), true);
        assert!(e.borrow().has("na"));
        assert!(Value::Int(1.into()).truthy());
        assert!(!Value::Null.truthy());
        assert!(Value::Str(String::new()).display().is_empty());
    }
    #[test]
    fn digits_are_ascii() {
        assert_eq!(Value::Int(120.into()).display(), "120");
    }
}
