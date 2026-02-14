//! Canonical formatter/printer. Sūkti source is ITRANS (pure ASCII); this
//! module prints and normalizes that single canonical script.

use crate::ast::*;
use crate::lexicon::Lexicon;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Itrans,
}

/// Format a program in the canonical ITRANS script.
pub fn format(stmts: &[Stmt], _view: View, lex: &Lexicon) -> String {
    let mut p = Printer {
        lex,
        out: String::new(),
        indent: 0,
    };
    p.program(stmts);
    p.out
}

/// Normalize source to canonical ITRANS. ITRANS is already canonical; the
/// other schemes are only accepted by the independent `sukti translit` tool.
pub fn to_canonical(src: &str, _script: sukti_sanskrit::Script) -> String {
    src.to_string()
}

struct Printer<'a> {
    lex: &'a Lexicon,
    out: String,
    indent: usize,
}

impl<'a> Printer<'a> {
    fn kw(&self, id: &str) -> String {
        if let Some(d) = self.lex.canonical(id) {
            d.to_string()
        } else {
            id.to_string()
        }
    }
    fn name(&self, s: &str) -> String {
        // A variable whose name is a vocabulary id prints as its Devanagari form.
        self.lex.canonical(s).unwrap_or(s).to_string()
    }
    fn pad(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }
    fn line(&mut self, s: &str) {
        self.pad();
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn program(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            self.stmt(s);
        }
    }

    fn block(&mut self, stmts: &[Stmt]) {
        self.out.push_str(" {\n");
        self.indent += 1;
        self.program(stmts);
        self.indent -= 1;
        self.pad();
        self.out.push_str("}\n");
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Expr(e) => {
                self.pad();
                self.expr(e, 0);
                self.out.push_str(";\n");
            }
            StmtKind::Let {
                name,
                ty,
                mutable,
                init,
            } => {
                self.pad();
                self.out
                    .push_str(&self.kw(if *mutable { "let" } else { "const" }));
                self.out.push(' ');
                self.out.push_str(&self.name(name));
                if let Some(t) = ty {
                    self.out.push_str(": ");
                    self.out.push_str(&self.name(t));
                }
                if let Some(e) = init {
                    self.out.push_str(" = ");
                    self.expr(e, 0);
                }
                self.out.push_str(";\n");
            }
            StmtKind::Fn(f) => {
                self.pad();
                self.out.push_str(&self.kw("fn"));
                self.out.push(' ');
                self.out.push_str(&self.name(&f.name));
                self.params(&f.params);
                if let Some(t) = &f.ret_ty {
                    self.out.push_str(" -> ");
                    self.out.push_str(&self.name(t));
                }
                self.block(&f.body);
            }
            StmtKind::Return(e) => {
                self.pad();
                self.out.push_str(&self.kw("return"));
                if let Some(e) = e {
                    self.out.push(' ');
                    self.expr(e, 0);
                }
                self.out.push_str(";\n");
            }
            StmtKind::If { cond, then, els } => {
                self.pad();
                self.out.push_str(&self.kw("if"));
                self.out.push_str(" (");
                self.expr(cond, 0);
                self.out.push(')');
                self.block(then);
                if !els.is_empty() {
                    self.pad();
                    self.out.push_str(&self.kw("else"));
                    if els.len() == 1 && matches!(els[0].kind, StmtKind::If { .. }) {
                        // `else if` is rendered by the nested statement.
                        self.out.push(' ');
                        self.stmt(&els[0]);
                    } else {
                        self.block(els);
                    }
                }
            }
            StmtKind::While { cond, body } => {
                self.pad();
                self.out.push_str(&self.kw("while"));
                self.out.push_str(" (");
                self.expr(cond, 0);
                self.out.push(')');
                self.block(body);
            }
            StmtKind::For { var, iter, body } => {
                self.pad();
                self.out.push_str(&self.kw("for"));
                self.out.push(' ');
                self.out.push_str(&self.name(var));
                self.out.push(' ');
                self.out.push_str(&self.kw("in"));
                self.out.push(' ');
                self.expr(iter, 0);
                self.block(body);
            }
            StmtKind::Break => self.line(&format!("{};", self.kw("break"))),
            StmtKind::Continue => self.line(&format!("{};", self.kw("continue"))),
            StmtKind::Try {
                body,
                catch_var,
                catch_body,
                finally,
            } => {
                self.pad();
                self.out.push_str(&self.kw("try"));
                self.block(body);
                self.pad();
                self.out.push_str(&self.kw("catch"));
                self.out.push_str(" (");
                self.out.push_str(&self.name(catch_var));
                self.out.push(')');
                self.block(catch_body);
                if !finally.is_empty() {
                    self.pad();
                    self.out.push_str(&self.kw("finally"));
                    self.block(finally);
                }
            }
            StmtKind::Raise(e) => {
                self.pad();
                self.out.push_str(&self.kw("raise"));
                self.out.push(' ');
                self.expr(e, 0);
                self.out.push_str(";\n");
            }
            StmtKind::Class {
                name,
                superclass,
                members,
            } => {
                self.pad();
                self.out.push_str(&self.kw("class"));
                self.out.push(' ');
                self.out.push_str(&self.name(name));
                if let Some(s) = superclass {
                    self.out.push_str(": ");
                    self.out.push_str(&self.name(s));
                }
                self.out.push_str(" {\n");
                self.indent += 1;
                for m in members {
                    match m {
                        Member::Field { name, init, .. } => {
                            self.pad();
                            self.out.push_str(&self.name(name));
                            if let Some(e) = init {
                                self.out.push_str(" = ");
                                self.expr(e, 0);
                            }
                            self.out.push_str(";\n");
                        }
                        Member::Method(f) => {
                            self.stmt(&Stmt::new(StmtKind::Fn(f.clone()), Default::default()))
                        }
                    }
                }
                self.indent -= 1;
                self.pad();
                self.out.push_str("}\n");
            }
            StmtKind::Import { path, alias } => {
                self.pad();
                self.out.push_str(&self.kw("import"));
                self.out.push_str(" \"");
                self.out.push_str(path);
                self.out.push('"');
                if let Some(a) = alias {
                    self.out.push_str(" -> ");
                    self.out.push_str(&self.name(a));
                }
                self.out.push_str(";\n");
            }
            StmtKind::Export(names) => {
                self.pad();
                self.out.push_str(&self.kw("export"));
                self.out.push(' ');
                self.out.push_str(
                    &names
                        .iter()
                        .map(|n| self.name(n))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                self.out.push_str(";\n");
            }
            StmtKind::Sutra { name, clauses, .. } => {
                for c in clauses {
                    self.pad();
                    if c.kind == SutraKind::Apavada {
                        self.out.push_str(&self.kw("apavada"));
                        self.out.push(' ');
                    }
                    self.out.push_str(&self.kw("sutra"));
                    self.out.push(' ');
                    self.out.push_str(&self.name(name));
                    self.out.push_str("()");
                    if let Some(g) = &c.guard {
                        self.out.push(' ');
                        self.out.push_str(&self.kw("if"));
                        self.out.push_str(" (");
                        self.expr(g, 0);
                        self.out.push(')');
                    }
                    self.out.push_str(" {\n");
                    self.indent += 1;
                    self.program(&c.body);
                    self.indent -= 1;
                    self.pad();
                    self.out.push_str("}\n");
                }
            }
            StmtKind::Task { name, body } => {
                self.pad();
                self.out.push_str(&self.kw("task"));
                self.out.push(' ');
                self.out.push_str(&self.name(name));
                self.block(body);
            }
            StmtKind::Channel { name } => {
                self.pad();
                self.out.push_str(" channel ");
                self.out.push_str(&self.name(name));
                self.out.push_str(";\n");
            }
            StmtKind::Send { channel, value } => {
                self.pad();
                self.out.push_str("send ");
                self.out.push_str(&self.name(channel));
                self.out.push(' ');
                self.expr(value, 0);
                self.out.push_str(";\n");
            }
            StmtKind::Receive { var, channel } => {
                self.pad();
                self.out.push_str("receive ");
                self.out.push_str(&self.name(var));
                self.out.push_str(" from ");
                self.out.push_str(&self.name(channel));
                self.out.push_str(";\n");
            }
            StmtKind::Join { names } => {
                self.pad();
                self.out.push_str("join ");
                self.out.push_str(
                    &names
                        .iter()
                        .map(|n| self.name(n))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                self.out.push_str(";\n");
            }
            StmtKind::Assert(e) => {
                self.pad();
                self.out.push_str("assert ");
                self.expr(e, 0);
                self.out.push_str(";\n");
            }
        }
    }

    fn params(&mut self, ps: &[Param]) {
        self.out.push('(');
        let parts: Vec<String> = ps
            .iter()
            .map(|p| {
                let mut s = String::new();
                if let Some(r) = p.role {
                    s.push_str(r.short());
                    s.push(' ');
                }
                s.push_str(&self.name(&p.name));
                if let Some(t) = &p.ty {
                    s.push_str(": ");
                    s.push_str(&self.name(t));
                }
                if let Some(d) = &p.default {
                    let mut sub = Printer {
                        lex: self.lex,
                        out: String::new(),
                        indent: 0,
                    };
                    sub.expr(d, 0);
                    s.push_str(" = ");
                    s.push_str(&sub.out);
                }
                s
            })
            .collect();
        self.out.push_str(&parts.join(", "));
        self.out.push(')');
    }

    fn expr(&mut self, e: &Expr, prec: u8) {
        match &e.kind {
            ExprKind::Lit(l) => match l {
                Literal::Int(i) => self.out.push_str(&i.to_string()),
                Literal::Float(f) => self.out.push_str(&f.to_string()),
                Literal::Str(s) => {
                    self.out.push('"');
                    self.out.push_str(s);
                    self.out.push('"');
                }
                Literal::Bool(b) => self
                    .out
                    .push_str(&self.kw(if *b { "true" } else { "false" })),
                Literal::Null => self.out.push_str(&self.kw("null")),
            },
            ExprKind::Var(s) => self.out.push_str(&self.name(s)),
            ExprKind::Template(parts) => {
                self.out.push('"');
                for p in parts {
                    match p {
                        StrPart::Text(t) => self.out.push_str(t),
                        StrPart::Expr(e, _) => {
                            self.out.push('{');
                            self.expr(e, 0);
                            self.out.push('}');
                        }
                    }
                }
                self.out.push('"');
            }
            ExprKind::Unary(op, x) => {
                let sym = match op {
                    UnOp::Neg => "-".to_string(),
                    UnOp::Not => self.kw("not"),
                };
                self.out.push_str(&sym);
                self.expr(x, 9);
            }
            ExprKind::Binary(op, a, b) => {
                let p = binop_prec(*op);
                let paren = p < prec;
                if paren {
                    self.out.push('(');
                }
                self.expr(a, p);
                self.out.push(' ');
                self.out.push_str(op.symbol());
                self.out.push(' ');
                self.expr(b, p + 1);
                if paren {
                    self.out.push(')');
                }
            }
            ExprKind::Assign(l, r) => {
                self.expr(l, 1);
                self.out.push_str(" = ");
                self.expr(r, 1);
            }
            ExprKind::Call { callee, args } => {
                self.expr(callee, 10);
                self.out.push('(');
                let parts: Vec<String> = args
                    .iter()
                    .map(|a| {
                        let mut sub = Printer {
                            lex: self.lex,
                            out: String::new(),
                            indent: 0,
                        };
                        sub.expr(&a.expr, 0);
                        match &a.kind {
                            ArgKind::Role(r) => format!("{}: {}", r.short(), sub.out),
                            ArgKind::Named(n) => format!("{}: {}", sub.name(n), sub.out),
                            ArgKind::Positional | ArgKind::Omitted => sub.out,
                        }
                    })
                    .collect();
                self.out.push_str(&parts.join(", "));
                self.out.push(')');
            }
            ExprKind::Get { obj, name } => {
                self.expr(obj, 10);
                self.out.push('.');
                self.out.push_str(&self.name(name));
            }
            ExprKind::Set { obj, name, value } => {
                self.expr(obj, 10);
                self.out.push('.');
                self.out.push_str(&self.name(name));
                self.out.push_str(" = ");
                self.expr(value, 0);
            }
            ExprKind::Index { obj, index } => {
                self.expr(obj, 10);
                self.out.push('[');
                self.expr(index, 0);
                self.out.push(']');
            }
            ExprKind::List(v) => {
                self.out.push('[');
                self.comma_exprs(v);
                self.out.push(']');
            }
            ExprKind::Tuple(v) => {
                self.out.push('(');
                self.comma_exprs(v);
                self.out.push(')');
            }
            ExprKind::Map(v) => {
                self.out.push('{');
                let parts: Vec<String> = v
                    .iter()
                    .map(|(k, x)| {
                        let mut a = Printer {
                            lex: self.lex,
                            out: String::new(),
                            indent: 0,
                        };
                        a.expr(k, 0);
                        let mut b = Printer {
                            lex: self.lex,
                            out: String::new(),
                            indent: 0,
                        };
                        b.expr(x, 0);
                        format!("{}: {}", a.out, b.out)
                    })
                    .collect();
                self.out.push_str(&parts.join(", "));
                self.out.push('}');
            }
            ExprKind::Range {
                from,
                to,
                inclusive,
            } => {
                self.expr(from, 0);
                self.out.push_str(if *inclusive { "..=" } else { ".." });
                self.expr(to, 0);
            }
            ExprKind::Lambda { params, body } => {
                self.out.push_str(&self.kw("fn"));
                self.params(params);
                self.out.push_str(" {");
                self.indent += 1;
                self.out.push('\n');
                self.program(body);
                self.indent -= 1;
                self.pad();
                self.out.push('}');
            }
            ExprKind::IfExpr { cond, then, els } => {
                self.out.push('(');
                self.expr(cond, 0);
                self.out.push_str(" ? ");
                self.expr(then, 0);
                self.out.push_str(" : ");
                self.expr(els, 0);
                self.out.push(')');
            }
            ExprKind::Block(b) => {
                self.out.push('{');
                self.indent += 1;
                self.out.push('\n');
                self.program(b);
                self.indent -= 1;
                self.pad();
                self.out.push('}');
            }
        }
    }

    fn comma_exprs(&mut self, v: &[Expr]) {
        let parts: Vec<String> = v
            .iter()
            .map(|e| {
                let mut p = Printer {
                    lex: self.lex,
                    out: String::new(),
                    indent: 0,
                };
                p.expr(e, 0);
                p.out
            })
            .collect();
        self.out.push_str(&parts.join(", "));
    }
}

fn binop_prec(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 2,
        BinOp::And => 3,
        BinOp::Eq | BinOp::Ne => 4,
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 5,
        BinOp::Add | BinOp::Sub => 6,
        BinOp::Mul | BinOp::Div | BinOp::Mod => 7,
        BinOp::Pow => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Source;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn parse(s: &str) -> Vec<Stmt> {
        let src = Source::new("t", &to_canonical(s, sukti_sanskrit::Script::Itrans));
        let l = Lexicon::load();
        let mut r = crate::diag::Reporter::new("en");
        let toks = Lexer::new(&src, &l, &mut r).tokenize();
        let mut p = Parser::new(&src, &l, &mut r, toks);
        let st = p.parse_program();
        assert!(!r.has_errors(), "{}", r.render(&src));
        st
    }
    #[test]
    fn idempotent() {
        let st =
            parse("kriyA phaika(na) { yadi (na <= 1) { phalam 1; } phalam na * phaika(na - 1); }");
        let a = format(&st, View::Itrans, &Lexicon::load());
        let b = format(&parse(&a), View::Itrans, &Lexicon::load());
        assert_eq!(a, b);
        assert!(a.is_ascii());
    }
    #[test]
    fn itrans_roundtrip() {
        let st = parse("vada(\"namaste\");");
        let out = format(&st, View::Itrans, &Lexicon::load());
        assert_eq!(out, "vada(\"namaste\");\n");
        assert_eq!(format(&parse(&out), View::Itrans, &Lexicon::load()), out);
    }
}
