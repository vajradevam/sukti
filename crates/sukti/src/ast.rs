//! Abstract syntax tree shared by the parser, formatter, resolver and VM.

use crate::diag::Span;
use num_bigint::BigInt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Mod => "%",
            Self::Pow => "**",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::And => "cha",
            Self::Or => "vA",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Clone, Debug)]
pub enum Literal {
    Int(BigInt),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
}

#[derive(Clone, Debug)]
pub enum StrPart {
    Text(String),
    Expr(Expr, Span),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KarakaRole {
    Karta,
    Karma,
    Karana,
    Sampradana,
    Apadana,
    Adhikarana,
    Sambandha,
}
impl KarakaRole {
    pub fn id(self) -> &'static str {
        match self {
            Self::Karta => "role.karta",
            Self::Karma => "role.karma",
            Self::Karana => "role.karana",
            Self::Sampradana => "role.sampradana",
            Self::Apadana => "role.apadana",
            Self::Adhikarana => "role.adhikarana",
            Self::Sambandha => "role.sambandha",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "role.karta" => Some(Self::Karta),
            "role.karma" => Some(Self::Karma),
            "role.karana" => Some(Self::Karana),
            "role.sampradana" => Some(Self::Sampradana),
            "role.apadana" => Some(Self::Apadana),
            "role.adhikarana" => Some(Self::Adhikarana),
            "role.sambandha" => Some(Self::Sambandha),
            _ => None,
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Self::Karta => "kartA",
            Self::Karma => "karma",
            Self::Karana => "karaNa",
            Self::Sampradana => "sampradAna",
            Self::Apadana => "apAdAna",
            Self::Adhikarana => "adhikaraNa",
            Self::Sambandha => "sambandha",
        }
    }
    pub fn case(self) -> &'static str {
        match self {
            Self::Karta => "nAma",
            Self::Karma => "dvitIyA",
            Self::Karana => "tRRitIyA",
            Self::Sampradana => "chaturthI",
            Self::Apadana => "panychamI",
            Self::Adhikarana => "saptamI",
            Self::Sambandha => "ShaShThI",
        }
    }
    pub fn all() -> [Self; 7] {
        [
            Self::Karta,
            Self::Karma,
            Self::Karana,
            Self::Sampradana,
            Self::Apadana,
            Self::Adhikarana,
            Self::Sambandha,
        ]
    }
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub role: Option<KarakaRole>,
    pub ty: Option<String>,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ArgKind {
    Positional,
    Named(String),
    Role(KarakaRole),
    Omitted,
}

#[derive(Clone, Debug)]
pub struct Arg {
    pub kind: ArgKind,
    pub expr: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Lit(Literal),
    Var(String),
    Template(Vec<StrPart>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Assign(Box<Expr>, Box<Expr>),
    Call {
        callee: Box<Expr>,
        args: Vec<Arg>,
    },
    Get {
        obj: Box<Expr>,
        name: String,
    },
    Set {
        obj: Box<Expr>,
        name: String,
        value: Box<Expr>,
    },
    Index {
        obj: Box<Expr>,
        index: Box<Expr>,
    },
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    Tuple(Vec<Expr>),
    Lambda {
        params: Vec<Param>,
        body: Vec<Stmt>,
    },
    IfExpr {
        cond: Box<Expr>,
        then: Box<Expr>,
        els: Box<Expr>,
    },
    Block(Vec<Stmt>),
    Range {
        from: Box<Expr>,
        to: Box<Expr>,
        inclusive: bool,
    },
}

#[derive(Clone, Debug)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret_ty: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SutraKind {
    Utsarga,
    Apavada,
}

#[derive(Clone, Debug)]
pub struct SutraClause {
    pub kind: SutraKind,
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
    pub order: usize,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Member {
    Field {
        name: String,
        init: Option<Expr>,
        span: Span,
    },
    Method(FnDecl),
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Expr(Expr),
    Let {
        name: String,
        ty: Option<String>,
        mutable: bool,
        init: Option<Expr>,
    },
    Fn(FnDecl),
    Return(Option<Expr>),
    If {
        cond: Expr,
        then: Vec<Stmt>,
        els: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    For {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    Try {
        body: Vec<Stmt>,
        catch_var: String,
        catch_body: Vec<Stmt>,
        finally: Vec<Stmt>,
    },
    Raise(Expr),
    Class {
        name: String,
        superclass: Option<String>,
        members: Vec<Member>,
    },
    Import {
        path: String,
        alias: Option<String>,
    },
    Export(Vec<String>),
    Sutra {
        name: String,
        params: Vec<Param>,
        clauses: Vec<SutraClause>,
    },
    Task {
        name: String,
        body: Vec<Stmt>,
    },
    Channel {
        name: String,
    },
    Send {
        channel: String,
        value: Expr,
    },
    Receive {
        var: String,
        channel: String,
    },
    Join {
        names: Vec<String>,
    },
    Assert(Expr),
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}
impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self { kind, span }
    }
}
