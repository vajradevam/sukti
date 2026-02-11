//! Hand-written, error-recovering recursive-descent parser with a Pratt
//! expression parser. Produces the shared AST used by formatter and VM.

use crate::ast::*;
use crate::diag::{Diagnostic, Reporter, Source, Span};
use crate::lexer::{Tok, Token};
use crate::lexicon::Lexicon;

pub struct Parser<'a> {
    _src: &'a Source,
    lex: &'a Lexicon,
    reporter: &'a mut Reporter,
    tokens: Vec<Token>,
    pos: usize,
    last_span: Span,
}

impl<'a> Parser<'a> {
    pub fn new(
        src: &'a Source,
        lex: &'a Lexicon,
        reporter: &'a mut Reporter,
        tokens: Vec<Token>,
    ) -> Self {
        Self {
            _src: src,
            lex,
            reporter,
            tokens,
            pos: 0,
            last_span: Span::default(),
        }
    }

    fn cur(&self) -> &Tok {
        &self.tokens[self.pos.min(self.tokens.len() - 1)].kind
    }
    fn tok(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }
    fn peek(&self, n: usize) -> &Tok {
        &self.tokens[(self.pos + n).min(self.tokens.len() - 1)].kind
    }
    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        self.last_span = t.span;
        t
    }
    fn is_eof(&self) -> bool {
        matches!(self.cur(), Tok::Eof)
    }
    fn at_kw(&self, id: &str) -> bool {
        matches!(self.cur(), Tok::Keyword(k) if k == id)
    }
    fn eat_kw(&mut self, id: &str) -> bool {
        if self.at_kw(id) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn eat(&mut self, t: &Tok) -> bool {
        if self.cur() == t {
            self.advance();
            true
        } else {
            false
        }
    }

    fn err_expected(&mut self, expected: &str) {
        let found = match self.cur() {
            Tok::Eof => "anta / EOF".to_string(),
            t => format!("{t:?}"),
        };
        let rendered = self.lex.message(
            "E0002",
            &self.reporter.lang,
            &[("expected", expected.to_string()), ("found", found)],
        );
        self.reporter
            .push(Diagnostic::new("E0002", self.tok().span, rendered));
    }

    fn expect(&mut self, t: Tok, what: &str) -> bool {
        if self.cur() == &t {
            self.advance();
            true
        } else {
            self.err_expected(what);
            false
        }
    }

    fn end_stmt(&mut self) {
        if matches!(self.cur(), Tok::Danda | Tok::DoubleDanda) {
            self.advance();
        }
    }

    fn recover(&mut self) {
        while !self.is_eof() && !matches!(self.cur(), Tok::Danda | Tok::DoubleDanda | Tok::RBrace) {
            self.advance();
        }
        self.end_stmt();
    }

    // ---------------------------------------------------------------- program

    pub fn parse_program(&mut self) -> Vec<Stmt> {
        let mut out = Vec::new();
        let mut guard = 0;
        while !self.is_eof() && guard < 10000 {
            guard += 1;
            let before = self.pos;
            match self.statement() {
                Some(s) => out.push(s),
                None => {
                    self.recover();
                }
            }
            if self.pos == before {
                self.advance();
            }
        }
        out
    }

    fn statement(&mut self) -> Option<Stmt> {
        let start = self.tok().span;
        // else-if continuous phrase
        if self.at_kw("else") && matches!(self.peek(1), Tok::Keyword(k) if k == "if") {
            return self.if_stmt(start, true);
        }
        if self.at_kw("let") || self.at_kw("const") {
            let mutable = self.eat_kw("let");
            if !mutable {
                self.advance();
            }
            let name = self.ident_or("_")?;
            let ty = if self.eat(&Tok::Colon) {
                Some(self.type_name())
            } else {
                None
            };
            let init = if self.eat(&Tok::Eq) {
                Some(self.expr())
            } else {
                None
            };
            self.end_stmt();
            return Some(Stmt::new(
                StmtKind::Let {
                    name,
                    ty,
                    mutable,
                    init,
                },
                start.to(self.last_span),
            ));
        }
        if self.at_kw("fn") {
            return self.fn_decl(false);
        }
        if self.eat_kw("return") {
            let e = if matches!(
                self.cur(),
                Tok::Danda | Tok::DoubleDanda | Tok::RBrace | Tok::Eof
            ) {
                None
            } else {
                Some(self.expr())
            };
            self.end_stmt();
            return Some(Stmt::new(StmtKind::Return(e), start.to(self.last_span)));
        }
        if self.at_kw("if") {
            return self.if_stmt(start, false);
        }
        if self.eat_kw("while") {
            self.expect(Tok::LParen, "(");
            let cond = self.expr();
            self.expect(Tok::RParen, ")");
            let body = self.block();
            return Some(Stmt::new(
                StmtKind::While { cond, body },
                start.to(self.last_span),
            ));
        }
        if self.eat_kw("for") {
            let var = self.ident_or("_")?;
            if !self.eat_kw("in") {
                self.err_expected("antaH (in)");
            }
            let iter = self.expr();
            let body = self.block();
            return Some(Stmt::new(
                StmtKind::For { var, iter, body },
                start.to(self.last_span),
            ));
        }
        if self.eat_kw("break") {
            self.end_stmt();
            return Some(Stmt::new(StmtKind::Break, start.to(self.last_span)));
        }
        if self.eat_kw("continue") {
            self.end_stmt();
            return Some(Stmt::new(StmtKind::Continue, start.to(self.last_span)));
        }
        if self.eat_kw("try") {
            let body = self.block();
            let mut catch_var = "I".to_string();
            let mut catch_body = Vec::new();
            if self.eat_kw("catch") {
                if self.eat(&Tok::LParen) {
                    catch_var = self.ident_or("I")?;
                    self.expect(Tok::RParen, ")");
                }
                catch_body = self.block();
            }
            let mut finally = Vec::new();
            if self.eat_kw("finally") {
                finally = self.block();
            }
            return Some(Stmt::new(
                StmtKind::Try {
                    body,
                    catch_var,
                    catch_body,
                    finally,
                },
                start.to(self.last_span),
            ));
        }
        if self.eat_kw("raise") {
            let e = self.expr();
            self.end_stmt();
            return Some(Stmt::new(StmtKind::Raise(e), start.to(self.last_span)));
        }
        if self.eat_kw("class") {
            let name = self.ident_or("?")?;
            let superclass = if self.eat(&Tok::Colon) {
                Some(self.ident_or("?")?)
            } else {
                None
            };
            let members = self.members();
            return Some(Stmt::new(
                StmtKind::Class {
                    name,
                    superclass,
                    members,
                },
                start.to(self.last_span),
            ));
        }
        if self.eat_kw("import") {
            let path = match self.cur().clone() {
                Tok::StrSeg(s) => {
                    self.advance();
                    s
                }
                _ => {
                    self.err_expected("string");
                    self.advance();
                    String::new()
                }
            };
            let alias = if self.eat(&Tok::Arrow) {
                Some(self.ident_or("_")?)
            } else {
                None
            };
            self.end_stmt();
            return Some(Stmt::new(
                StmtKind::Import { path, alias },
                start.to(self.last_span),
            ));
        }
        if self.eat_kw("export") {
            let mut names = Vec::new();
            loop {
                names.push(self.ident_or("_")?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.end_stmt();
            return Some(Stmt::new(StmtKind::Export(names), start.to(self.last_span)));
        }
        if self.at_kw("sutra") || self.at_kw("apavada") || self.at_kw("utsarga") {
            return self.sutra_stmt(start);
        }
        if self.eat_kw("task") {
            let name = self.ident_or("_")?;
            let body = self.block();
            return Some(Stmt::new(
                StmtKind::Task { name, body },
                start.to(self.last_span),
            ));
        }
        if self.eat(&Tok::LBrace) {
            let body = self.block_rest();
            return Some(Stmt::new(
                StmtKind::Expr(Expr::new(ExprKind::Block(body), start.to(self.last_span))),
                start.to(self.last_span),
            ));
        }
        // expression statement
        let e = self.expr();
        self.end_stmt();
        Some(Stmt::new(StmtKind::Expr(e), start.to(self.last_span)))
    }

    fn ident_or(&mut self, fallback: &str) -> Option<String> {
        match self.cur().clone() {
            Tok::Ident(s) => {
                self.advance();
                Some(s)
            }
            Tok::Keyword(k) => {
                self.advance();
                Some(k)
            } // allow keywords as names where harmless
            _ => {
                self.err_expected("identifier");
                None.or(Some(fallback.to_string()))
            }
        }
    }

    fn type_name(&mut self) -> String {
        match self.cur().clone() {
            Tok::Ident(s) => {
                self.advance();
                s
            }
            Tok::Keyword(k) => {
                self.advance();
                k
            }
            _ => {
                self.err_expected("type");
                "?".to_string()
            }
        }
    }

    fn fn_decl(&mut self, _anonymous: bool) -> Option<Stmt> {
        let start = self.tok().span;
        self.advance(); // fn
        let name = self.ident_or("_")?;
        let params = self.params()?;
        let ret_ty = if self.eat(&Tok::Arrow) {
            Some(self.type_name())
        } else {
            None
        };
        let body = self.block();
        Some(Stmt::new(
            StmtKind::Fn(FnDecl {
                name,
                params,
                ret_ty,
                body,
            }),
            start.to(self.last_span),
        ))
    }

    fn params(&mut self) -> Option<Vec<Param>> {
        if !self.expect(Tok::LParen, "(") {
            return Some(Vec::new());
        }
        let mut out = Vec::new();
        while self.cur() != &Tok::RParen && !self.is_eof() {
            let st = self.tok().span;
            let role = match self.cur().clone() {
                Tok::Role(id) => {
                    self.advance();
                    KarakaRole::from_id(&id)
                }
                _ => None,
            };
            let name = self.ident_or("_")?;
            let ty = if self.eat(&Tok::Colon) {
                Some(self.type_name())
            } else {
                None
            };
            let default = if self.eat(&Tok::Eq) {
                Some(self.expr())
            } else {
                None
            };
            out.push(Param {
                name,
                role,
                ty,
                default,
                span: st,
            });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RParen, ")");
        Some(out)
    }

    fn if_stmt(&mut self, start: Span, already_ate_else: bool) -> Option<Stmt> {
        if !already_ate_else {
            self.advance();
        } // consume if/else
        if already_ate_else {
            self.advance();
        } // consume if after else
        self.expect(Tok::LParen, "(");
        let cond = self.expr();
        self.expect(Tok::RParen, ")");
        let then = self.block();
        let mut els = Vec::new();
        if self.at_kw("else") {
            self.advance();
            if self.at_kw("if") {
                let nested = self.if_stmt(self.tok().span, false)?;
                els.push(nested);
            } else {
                els = self.block();
            }
        }
        Some(Stmt::new(
            StmtKind::If { cond, then, els },
            start.to(self.last_span),
        ))
    }

    fn members(&mut self) -> Vec<Member> {
        let mut out = Vec::new();
        if !self.expect(Tok::LBrace, "{") {
            return out;
        }
        let mut guard = 0;
        while self.cur() != &Tok::RBrace && !self.is_eof() && guard < 10000 {
            guard += 1;
            let before = self.pos;
            let st = self.tok().span;
            if self.at_kw("fn") {
                if let Some(s) = self.fn_decl(false) {
                    if let StmtKind::Fn(f) = s.kind {
                        out.push(Member::Method(f));
                    }
                }
            } else if matches!(self.cur(), Tok::Ident(_) | Tok::Keyword(_))
                && matches!(self.peek(1), Tok::LParen)
            {
                // A bare `nirmANa(...) { }` method declaration inside a class.
                let name = self.ident_or("_").unwrap_or_default();
                let params = self.params().unwrap_or_default();
                let ret_ty = if self.eat(&Tok::Arrow) {
                    Some(self.type_name())
                } else {
                    None
                };
                let body = self.block();
                out.push(Member::Method(FnDecl {
                    name,
                    params,
                    ret_ty,
                    body,
                }));
            } else {
                let name = self.ident_or("_").unwrap_or_default();
                let init = if self.eat(&Tok::Eq) {
                    Some(self.expr())
                } else {
                    None
                };
                self.end_stmt();
                out.push(Member::Field {
                    name,
                    init,
                    span: st,
                });
            }
            if self.pos == before {
                self.advance();
            }
        }
        self.expect(Tok::RBrace, "}");
        out
    }

    fn block(&mut self) -> Vec<Stmt> {
        if !self.expect(Tok::LBrace, "{") {
            return Vec::new();
        }
        self.block_rest()
    }

    fn block_rest(&mut self) -> Vec<Stmt> {
        let mut out = Vec::new();
        let mut guard = 0;
        while self.cur() != &Tok::RBrace && !self.is_eof() && guard < 10000 {
            guard += 1;
            let before = self.pos;
            if let Some(s) = self.statement() {
                out.push(s);
            } else {
                self.recover();
            }
            if self.pos == before {
                self.advance();
            }
        }
        self.expect(Tok::RBrace, "}");
        out
    }

    fn sutra_stmt(&mut self, start: Span) -> Option<Stmt> {
        let first_kind = if self.eat_kw("apavada") {
            SutraKind::Apavada
        } else if self.eat_kw("utsarga") {
            SutraKind::Utsarga
        } else {
            SutraKind::Utsarga
        };
        if !self.eat_kw("sutra") {
            self.err_expected("sUtra");
        }
        let name = self.ident_or("_")?;
        let params = self.params()?;
        let guard = if self.eat_kw("if") {
            self.expect(Tok::LParen, "(");
            let g = self.expr();
            self.expect(Tok::RParen, ")");
            Some(g)
        } else {
            None
        };
        let body = self.block();
        let clause = SutraClause {
            kind: first_kind,
            guard,
            body,
            order: 0,
            span: start.to(self.last_span),
        };
        // If this is a plain `sUtra name(params) { }`, it is the base group.
        Some(Stmt::new(
            StmtKind::Sutra {
                name,
                params,
                clauses: vec![clause],
            },
            start.to(self.last_span),
        ))
    }

    // -------------------------------------------------------------- expressions

    pub fn expr(&mut self) -> Expr {
        self.assignment()
    }

    fn assignment(&mut self) -> Expr {
        let lhs = self.or_expr();
        if self.eat(&Tok::Range) || self.eat(&Tok::RangeInclusive) {
            let inclusive = matches!(self.tokens[self.pos - 1].kind, Tok::RangeInclusive);
            let rhs = self.or_expr();
            let sp = lhs.span.to(rhs.span);
            return Expr::new(
                ExprKind::Range {
                    from: Box::new(lhs),
                    to: Box::new(rhs),
                    inclusive,
                },
                sp,
            );
        }
        if self.eat(&Tok::Eq) {
            let rhs = self.assignment();
            let span = lhs.span.to(rhs.span);
            return Expr::new(ExprKind::Assign(Box::new(lhs), Box::new(rhs)), span);
        }
        lhs
    }

    fn or_expr(&mut self) -> Expr {
        let mut e = self.and_expr();
        loop {
            let op = if self.eat(&Tok::OrOr) || self.eat_kw("or") {
                BinOp::Or
            } else {
                break;
            };
            let r = self.and_expr();
            let sp = e.span.to(r.span);
            e = Expr::new(ExprKind::Binary(op, Box::new(e), Box::new(r)), sp);
        }
        e
    }

    fn and_expr(&mut self) -> Expr {
        let mut e = self.equality();
        loop {
            let op = if self.eat(&Tok::AndAnd) || self.eat_kw("and") {
                BinOp::And
            } else {
                break;
            };
            let r = self.equality();
            let sp = e.span.to(r.span);
            e = Expr::new(ExprKind::Binary(op, Box::new(e), Box::new(r)), sp);
        }
        e
    }

    fn equality(&mut self) -> Expr {
        let mut e = self.comparison();
        loop {
            let op = if self.eat(&Tok::EqEq) {
                BinOp::Eq
            } else if self.eat(&Tok::Ne) {
                BinOp::Ne
            } else {
                break;
            };
            let r = self.comparison();
            let sp = e.span.to(r.span);
            e = Expr::new(ExprKind::Binary(op, Box::new(e), Box::new(r)), sp);
        }
        e
    }

    fn comparison(&mut self) -> Expr {
        let mut e = self.additive();
        loop {
            let op = if self.eat(&Tok::Lt) {
                BinOp::Lt
            } else if self.eat(&Tok::Le) {
                BinOp::Le
            } else if self.eat(&Tok::Gt) {
                BinOp::Gt
            } else if self.eat(&Tok::Ge) {
                BinOp::Ge
            } else {
                break;
            };
            let r = self.additive();
            let sp = e.span.to(r.span);
            e = Expr::new(ExprKind::Binary(op, Box::new(e), Box::new(r)), sp);
        }
        e
    }

    fn additive(&mut self) -> Expr {
        let mut e = self.multiplicative();
        loop {
            let op = if self.eat(&Tok::Plus) {
                BinOp::Add
            } else if self.eat(&Tok::Minus) {
                BinOp::Sub
            } else {
                break;
            };
            let r = self.multiplicative();
            let sp = e.span.to(r.span);
            e = Expr::new(ExprKind::Binary(op, Box::new(e), Box::new(r)), sp);
        }
        e
    }

    fn multiplicative(&mut self) -> Expr {
        let mut e = self.power();
        loop {
            let op = if self.eat(&Tok::Star) {
                BinOp::Mul
            } else if self.eat(&Tok::Slash) {
                BinOp::Div
            } else if self.eat(&Tok::Percent) {
                BinOp::Mod
            } else {
                break;
            };
            let r = self.power();
            let sp = e.span.to(r.span);
            e = Expr::new(ExprKind::Binary(op, Box::new(e), Box::new(r)), sp);
        }
        e
    }

    fn power(&mut self) -> Expr {
        let mut e = self.unary();
        if self.eat(&Tok::StarStar) {
            let r = self.power();
            let sp = e.span.to(r.span);
            e = Expr::new(ExprKind::Binary(BinOp::Pow, Box::new(e), Box::new(r)), sp);
        }
        e
    }

    fn unary(&mut self) -> Expr {
        let st = self.tok().span;
        if self.eat(&Tok::Minus) {
            let e = self.unary();
            let sp = st.to(e.span);
            return Expr::new(ExprKind::Unary(UnOp::Neg, Box::new(e)), sp);
        }
        let not_op = self.eat(&Tok::Bang)
            || (self.at_kw("not") && can_start_primary(self.peek(1)) && {
                self.advance();
                true
            });
        if not_op {
            let e = self.unary();
            let sp = st.to(e.span);
            return Expr::new(ExprKind::Unary(UnOp::Not, Box::new(e)), sp);
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Expr {
        let mut e = self.primary();
        loop {
            if self.eat(&Tok::LParen) {
                let args = self.args();
                self.expect(Tok::RParen, ")");
                let sp = e.span.to(self.last_span);
                e = Expr::new(
                    ExprKind::Call {
                        callee: Box::new(e),
                        args,
                    },
                    sp,
                );
            } else if self.eat(&Tok::LBracket) {
                let idx = self.expr();
                self.expect(Tok::RBracket, "]");
                let sp = e.span.to(self.last_span);
                e = Expr::new(
                    ExprKind::Index {
                        obj: Box::new(e),
                        index: Box::new(idx),
                    },
                    sp,
                );
            } else if self.eat(&Tok::Dot) {
                let name = self.ident_or("_").unwrap_or_default();
                let sp = e.span.to(self.last_span);
                e = Expr::new(
                    ExprKind::Get {
                        obj: Box::new(e),
                        name,
                    },
                    sp,
                );
            } else {
                break;
            }
        }
        e
    }

    fn args(&mut self) -> Vec<Arg> {
        let mut out = Vec::new();
        while self.cur() != &Tok::RParen && !self.is_eof() {
            let st = self.tok().span;
            let kind = match self.cur().clone() {
                Tok::Role(id) => {
                    self.advance();
                    self.eat(&Tok::Colon);
                    ArgKind::Role(KarakaRole::from_id(&id).unwrap_or(KarakaRole::Karta))
                }
                Tok::Ident(name) if matches!(self.peek(1), Tok::Colon) => {
                    self.advance();
                    self.advance();
                    ArgKind::Named(name)
                }
                _ => ArgKind::Positional,
            };
            let e = self.expr();
            out.push(Arg {
                kind,
                expr: e,
                span: st.to(self.last_span),
            });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        out
    }

    fn primary(&mut self) -> Expr {
        let tok = self.tok().clone();
        let st = tok.span;
        match tok.kind {
            Tok::Int(i) => {
                self.advance();
                Expr::new(ExprKind::Lit(Literal::Int(i)), st)
            }
            Tok::Float(f) => {
                self.advance();
                Expr::new(ExprKind::Lit(Literal::Float(f)), st)
            }
            Tok::StrSeg(s) => {
                self.advance();
                let mut parts = vec![StrPart::Text(s)];
                while self.cur() == &Tok::LBrace {
                    self.advance();
                    let e = self.expr();
                    self.expect(Tok::RBrace, "}");
                    parts.push(StrPart::Expr(e, st));
                    if let Tok::StrSeg(s2) = self.cur().clone() {
                        self.advance();
                        parts.push(StrPart::Text(s2));
                    }
                }
                Expr::new(ExprKind::Template(parts), st.to(self.last_span))
            }
            Tok::Ident(s) => {
                self.advance();
                Expr::new(ExprKind::Var(s), st)
            }
            Tok::Keyword(k) if k == "true" || k == "false" => {
                self.advance();
                Expr::new(ExprKind::Lit(Literal::Bool(k == "true")), st)
            }
            Tok::Keyword(k) if k == "null" => {
                self.advance();
                Expr::new(ExprKind::Lit(Literal::Null), st)
            }
            Tok::Keyword(k) if k == "fn" => {
                self.advance();
                let params = self.params().unwrap_or_default();
                let body = self.block();
                Expr::new(ExprKind::Lambda { params, body }, st.to(self.last_span))
            }
            Tok::Keyword(k) if k == "task" => {
                self.advance();
                let body = self.block();
                Expr::new(
                    ExprKind::Lambda {
                        params: Vec::new(),
                        body,
                    },
                    st.to(self.last_span),
                )
            }
            Tok::Keyword(k) => {
                self.advance();
                Expr::new(ExprKind::Var(k), st)
            }
            Tok::LParen => {
                self.advance();
                if self.eat(&Tok::RParen) {
                    return Expr::new(ExprKind::Tuple(Vec::new()), st.to(self.last_span));
                }
                let first = self.expr();
                if self.eat(&Tok::Comma) {
                    let mut v = vec![first];
                    while self.cur() != &Tok::RParen && !self.is_eof() {
                        v.push(self.expr());
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(Tok::RParen, ")");
                    Expr::new(ExprKind::Tuple(v), st.to(self.last_span))
                } else {
                    self.expect(Tok::RParen, ")");
                    first
                }
            }
            Tok::LBrace => {
                // Map literal in expression position: { key: value, ... }
                self.advance();
                let mut pairs = Vec::new();
                while self.cur() != &Tok::RBrace && !self.is_eof() {
                    let k = self.expr();
                    self.expect(Tok::Colon, ":");
                    let v = self.expr();
                    pairs.push((k, v));
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::RBrace, "}");
                Expr::new(ExprKind::Map(pairs), st.to(self.last_span))
            }
            Tok::LBracket => {
                self.advance();
                let mut v = Vec::new();
                while self.cur() != &Tok::RBracket && !self.is_eof() {
                    v.push(self.expr());
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::RBracket, "]");
                Expr::new(ExprKind::List(v), st.to(self.last_span))
            }
            _ => {
                self.err_expected("expression");
                self.advance();
                Expr::new(ExprKind::Lit(Literal::Null), st)
            }
        }
    }
}

fn can_start_primary(t: &Tok) -> bool {
    match t {
        Tok::Int(_)
        | Tok::Float(_)
        | Tok::StrSeg(_)
        | Tok::Ident(_)
        | Tok::LParen
        | Tok::LBracket => true,
        Tok::Keyword(k) => !matches!(
            k.as_str(),
            "return"
                | "break"
                | "continue"
                | "if"
                | "while"
                | "for"
                | "class"
                | "try"
                | "raise"
                | "let"
                | "const"
                | "else"
                | "import"
                | "export"
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(src: &str) -> (Vec<Stmt>, Reporter, Source) {
        let s = Source::new("t", src);
        let l = Lexicon::load();
        let mut r = Reporter::new("en");
        let toks = Lexer::new(&s, &l, &mut r).tokenize();
        let mut p = Parser::new(&s, &l, &mut r, toks);
        let out = p.parse_program();
        (out, r, s)
    }
    #[test]
    fn parses_samples() {
        let (st, r, s) = parse("vada(\"namaste, vishva!\");");
        assert!(!r.has_errors(), "{}", r.render(&s));
        assert_eq!(st.len(), 1);
        let (st, r, s) =
            parse("kriyA kramaguNita(na) { yadi (na <= 1) { phalam 1; } phalam na * kramaguNita(na - 1); }");
        assert!(!r.has_errors(), "{}", r.render(&s));
        assert_eq!(st.len(), 1);
    }
    #[test]
    fn parses_karaka_call_and_sutra() {
        let (st, r, s) =
            parse("preShaya(kartA: rAma, karma: patra); apavAda sUtra shulka(vastu) yadi (vastu == 1) { phalam 0; }");
        assert!(!r.has_errors(), "{}", r.render(&s));
        assert_eq!(st.len(), 2);
    }
}
