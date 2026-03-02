//! Name/kāraka resolution and static lints.
//!
//! Layer A (explicit labels) and Layer B (vibhakti-inflected a-stem singular
//! forms) are rewritten to positional argument order here, at compile time,
//! so the VM and JIT see ordinary positional calls (zero runtime cost).

use crate::ast::*;
use crate::diag::{Diagnostic, Reporter, Span};
use crate::lexicon::Lexicon;
use std::collections::{HashMap, HashSet};
use sukti_sanskrit::declension::{parse_inflected, Case, Number};

pub struct Resolved {
    pub stmts: Vec<Stmt>,
    pub layer_b_rewrites: usize,
    pub layer_a_rewrites: usize,
}

/// Resolve a whole program. Mutates the AST in place and returns metrics.
pub fn resolve(mut stmts: Vec<Stmt>, lex: &Lexicon, rep: &mut Reporter) -> Resolved {
    let mut sigs = HashMap::new();
    collect_signatures(&stmts, &mut sigs);
    let mut stats = Resolved {
        stmts: Vec::new(),
        layer_b_rewrites: 0,
        layer_a_rewrites: 0,
    };
    for s in &mut stmts {
        resolve_stmt(s, lex, rep, &sigs, &mut stats);
    }
    lint_sutras(&stmts, rep);
    stats.stmts = stmts;
    stats
}

type Signatures = HashMap<String, Vec<Param>>;

fn collect_signatures(stmts: &[Stmt], out: &mut Signatures) {
    for s in stmts {
        match &s.kind {
            StmtKind::Fn(f) => {
                out.entry(f.name.clone())
                    .or_insert_with(|| f.params.clone());
            }
            StmtKind::Sutra { name, clauses, .. } => {
                if let Some(c) = clauses.first() {
                    // Sūtra signatures are learned from calls conservatively:
                    // no params declared → use arity of first clause guard use.
                    out.entry(name.clone()).or_default();
                    let _ = c;
                }
            }
            StmtKind::If { then, els, .. } => {
                collect_signatures(then, out);
                collect_signatures(els, out);
            }
            StmtKind::While { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Task { body, .. } => collect_signatures(body, out),
            StmtKind::Try {
                body,
                catch_body,
                finally,
                ..
            } => {
                collect_signatures(body, out);
                collect_signatures(catch_body, out);
                collect_signatures(finally, out);
            }
            StmtKind::Class { members, .. } => {
                for m in members {
                    if let Member::Method(f) = m {
                        out.entry(f.name.clone())
                            .or_insert_with(|| f.params.clone());
                    }
                }
            }
            _ => {}
        }
    }
}

fn resolve_stmt(
    s: &mut Stmt,
    lex: &Lexicon,
    rep: &mut Reporter,
    sigs: &Signatures,
    stats: &mut Resolved,
) {
    match &mut s.kind {
        StmtKind::Expr(e)
        | StmtKind::Return(Some(e))
        | StmtKind::Raise(e)
        | StmtKind::Assert(e)
        | StmtKind::Send { value: e, .. } => resolve_expr(e, lex, rep, sigs, stats),
        StmtKind::Let { init, .. } => {
            if let Some(e) = init {
                resolve_expr(e, lex, rep, sigs, stats);
            }
        }
        StmtKind::Fn(f) => {
            for p in &mut f.params {
                if let Some(d) = &mut p.default {
                    resolve_expr(d, lex, rep, sigs, stats);
                }
            }
            resolve_block(&mut f.body, lex, rep, sigs, stats);
        }
        StmtKind::If { cond, then, els } => {
            resolve_expr(cond, lex, rep, sigs, stats);
            resolve_block(then, lex, rep, sigs, stats);
            resolve_block(els, lex, rep, sigs, stats);
        }
        StmtKind::While { cond, body } => {
            resolve_expr(cond, lex, rep, sigs, stats);
            resolve_block(body, lex, rep, sigs, stats);
        }
        StmtKind::For { iter, body, .. } => {
            resolve_expr(iter, lex, rep, sigs, stats);
            resolve_block(body, lex, rep, sigs, stats);
        }
        StmtKind::Try {
            body,
            catch_body,
            finally,
            ..
        } => {
            resolve_block(body, lex, rep, sigs, stats);
            resolve_block(catch_body, lex, rep, sigs, stats);
            resolve_block(finally, lex, rep, sigs, stats);
        }
        StmtKind::Class { members, .. } => {
            for m in members {
                if let Member::Method(f) = m {
                    resolve_block(&mut f.body, lex, rep, sigs, stats);
                }
            }
        }
        StmtKind::Sutra { clauses, .. } => {
            for c in clauses {
                if let Some(g) = &mut c.guard {
                    resolve_expr(g, lex, rep, sigs, stats);
                }
                resolve_block(&mut c.body, lex, rep, sigs, stats);
            }
        }
        StmtKind::Task { body, .. } => resolve_block(body, lex, rep, sigs, stats),
        _ => {}
    }
}

fn resolve_block(
    b: &mut [Stmt],
    lex: &Lexicon,
    rep: &mut Reporter,
    sigs: &Signatures,
    stats: &mut Resolved,
) {
    for s in b {
        resolve_stmt(s, lex, rep, sigs, stats);
    }
}

fn resolve_expr(
    e: &mut Expr,
    lex: &Lexicon,
    rep: &mut Reporter,
    sigs: &Signatures,
    stats: &mut Resolved,
) {
    match &mut e.kind {
        ExprKind::Call { callee, args } => {
            resolve_expr(callee, lex, rep, sigs, stats);
            for a in args.iter_mut() {
                resolve_expr(&mut a.expr, lex, rep, sigs, stats);
            }
            let name = match &callee.kind {
                ExprKind::Var(n) => Some(n.clone()),
                _ => None,
            };
            if let Some(n) = name {
                if let Some(params) = sigs.get(&n) {
                    if params.iter().any(|p| p.role.is_some()) {
                        reorder(&n, params, args, lex, rep, stats);
                    }
                }
            }
        }
        ExprKind::Unary(_, x) => resolve_expr(x, lex, rep, sigs, stats),
        ExprKind::Assign(l, r) => {
            resolve_expr(l, lex, rep, sigs, stats);
            resolve_expr(r, lex, rep, sigs, stats);
        }
        ExprKind::Binary(_, a, b) => {
            resolve_expr(a, lex, rep, sigs, stats);
            resolve_expr(b, lex, rep, sigs, stats);
        }
        ExprKind::Get { obj, .. } => resolve_expr(obj, lex, rep, sigs, stats),
        ExprKind::Index { obj, index } => {
            resolve_expr(obj, lex, rep, sigs, stats);
            resolve_expr(index, lex, rep, sigs, stats);
        }
        ExprKind::Set { obj, value, .. } => {
            resolve_expr(obj, lex, rep, sigs, stats);
            resolve_expr(value, lex, rep, sigs, stats);
        }
        ExprKind::List(v) | ExprKind::Tuple(v) => {
            for x in v {
                resolve_expr(x, lex, rep, sigs, stats);
            }
        }
        ExprKind::Map(v) => {
            for (k, x) in v {
                resolve_expr(k, lex, rep, sigs, stats);
                resolve_expr(x, lex, rep, sigs, stats);
            }
        }
        ExprKind::Template(parts) => {
            for p in parts {
                if let StrPart::Expr(x, _) = p {
                    resolve_expr(x, lex, rep, sigs, stats);
                }
            }
        }
        ExprKind::Lambda { body, .. } => resolve_block(body, lex, rep, sigs, stats),
        ExprKind::Block(b) => resolve_block(b, lex, rep, sigs, stats),
        ExprKind::IfExpr { cond, then, els } => {
            resolve_expr(cond, lex, rep, sigs, stats);
            resolve_expr(then, lex, rep, sigs, stats);
            resolve_expr(els, lex, rep, sigs, stats);
        }
        ExprKind::Range { from, to, .. } => {
            resolve_expr(from, lex, rep, sigs, stats);
            resolve_expr(to, lex, rep, sigs, stats);
        }
        _ => {}
    }
}

fn case_to_role(c: Case) -> KarakaRole {
    match c {
        Case::Nom => KarakaRole::Karta,
        Case::Acc => KarakaRole::Karma,
        Case::Ins => KarakaRole::Karana,
        Case::Dat => KarakaRole::Sampradana,
        Case::Abl => KarakaRole::Apadana,
        Case::Gen => KarakaRole::Sambandha,
        Case::Loc => KarakaRole::Adhikarana,
        Case::Voc => KarakaRole::Karta,
    }
}

/// Rewrite a call's labelled arguments into positional order.
fn reorder(
    name: &str,
    params: &[Param],
    args: &mut Vec<Arg>,
    lex: &Lexicon,
    rep: &mut Reporter,
    stats: &mut Resolved,
) {
    if params.is_empty() {
        return;
    }
    let param_roles: Vec<Option<KarakaRole>> = params.iter().map(|p| p.role).collect();
    let mut slots: Vec<Option<Arg>> = vec![None; params.len()];
    let mut used_roles: HashSet<KarakaRole> = HashSet::new();
    let mut plain = 0usize;
    let mut saw_layer_b = false;
    let mut saw_layer_a = false;

    for a in args.drain(..) {
        let mut role: Option<KarakaRole> = None;
        let mut named_slot: Option<usize> = None;
        match &a.kind {
            ArgKind::Role(r) => {
                role = Some(*r);
                saw_layer_a = true;
            }
            ArgKind::Named(n) => {
                named_slot = params.iter().position(|p| &p.name == n);
                saw_layer_a = true;
            }
            ArgKind::Positional | ArgKind::Omitted => {}
        }
        // Layer B: inflected bare identifier resolved against callee expectancy.
        if role.is_none() && named_slot.is_none() && matches!(a.kind, ArgKind::Positional) {
            if let ExprKind::Var(w) = &a.expr.kind {
                // Program text is ITRANS; the inflection tables are stored in
                // Devanagari. Convert at this boundary, not in user-visible I/O.
                let inflected = sukti_sanskrit::transliterate(
                    w,
                    sukti_sanskrit::Script::Itrans,
                    sukti_sanskrit::Script::Devanagari,
                );
                if let Some(forms) = parse_inflected(&inflected, Number::Sg) {
                    let mut cases: HashSet<Case> = forms.iter().map(|f| f.case).collect();
                    cases.retain(|c| param_roles.contains(&Some(case_to_role(*c))));
                    if cases.len() == 1 {
                        role = Some(case_to_role(*cases.iter().next().unwrap()));
                        saw_layer_b = true;
                    } else if cases.len() > 1 {
                        rep.push(
                            Diagnostic::new(
                                "E0005",
                                a.span,
                                lex.message(
                                    "E0005",
                                    &rep.lang,
                                    &[(
                                        "detail",
                                        format!("{w} has {} plausible cases", cases.len()),
                                    )],
                                ),
                            )
                            .with_suggestion("use an explicit kartA/karma label (Layer A)"),
                        );
                        if plain < slots.len() {
                            slots[plain] = Some(a);
                            plain += 1;
                        }
                        continue;
                    }
                }
            }
        }
        let slot = if let Some(i) = named_slot {
            Some(i)
        } else if let Some(r) = role {
            if !used_roles.insert(r) {
                rep.push(Diagnostic::new(
                    "E0003",
                    a.span,
                    lex.message("E0003", &rep.lang, &[("role", r.short().to_string())]),
                ));
                continue;
            }
            param_roles.iter().position(|p| *p == Some(r))
        } else {
            None
        };
        match slot {
            Some(i) => slots[i] = Some(a),
            None => {
                if let Some(r) = role {
                    if param_roles.iter().all(|p| *p != Some(r)) {
                        rep.push(Diagnostic::new(
                            "E0004",
                            a.span,
                            lex.message("E0004", &rep.lang, &[("role", r.short().to_string())]),
                        ));
                        continue;
                    }
                }
                {
                    while plain < slots.len() && slots[plain].is_some() {
                        plain += 1;
                    }
                    if plain < slots.len() {
                        slots[plain] = Some(a);
                        plain += 1;
                    }
                }
            }
        }
    }
    for (i, p) in params.iter().enumerate() {
        if slots[i].is_none() && p.default.is_none() {
            if let Some(r) = p.role {
                rep.push(Diagnostic::new(
                    "E0004",
                    Span::default(),
                    lex.message("E0004", &rep.lang, &[("role", r.short().to_string())]),
                ));
            }
        }
    }
    let mut out = Vec::new();
    for (i, slot) in slots.into_iter().enumerate() {
        let mut a = match slot {
            Some(a) => a,
            None => Arg {
                kind: if params[i].default.is_some() {
                    ArgKind::Omitted
                } else {
                    ArgKind::Positional
                },
                expr: Expr::new(ExprKind::Lit(Literal::Null), Span::default()),
                span: Span::default(),
            },
        };
        if !matches!(a.kind, ArgKind::Omitted) {
            a.kind = ArgKind::Positional;
        }
        out.push(a);
    }
    if saw_layer_b {
        stats.layer_b_rewrites += 1;
    } else if saw_layer_a {
        stats.layer_a_rewrites += 1;
    }
    *args = out;
    let _ = name;
}

/// Lint: flag sūtra clauses shadowed by a later equally-general clause.
fn lint_sutras(stmts: &[Stmt], rep: &mut Reporter) {
    for s in stmts {
        if let StmtKind::Sutra { clauses, .. } = &s.kind {
            for (i, a) in clauses.iter().enumerate() {
                if a.kind == SutraKind::Utsarga && a.guard.is_none() {
                    for b in clauses.iter().skip(i + 1) {
                        if b.kind == SutraKind::Utsarga && b.guard.is_none() {
                            rep.push(Diagnostic::new("W0001", b.span,
                                "clause shadows an earlier ungated utsarga; the earlier clause is unreachable".into())
                                .with_note("keep only the later clause, or add a guard"));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Source;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn resolve_src(src: &str) -> (Resolved, Reporter, Source) {
        let s = Source::new("t", src);
        let l = Lexicon::load();
        let mut r = Reporter::new("en");
        let toks = Lexer::new(&s, &l, &mut r).tokenize();
        let mut p = Parser::new(&s, &l, &mut r, toks);
        let st = p.parse_program();
        let res = resolve(st, &l, &mut r);
        (res, r, s)
    }
    #[test]
    fn layer_a_reorders() {
        let (res, r, s) =
            resolve_src("kriyA preShaya(kartA preShaka, karma patra) { phalam 1; }\npreShaya(karma: patra, kartA: rAma);");
        assert!(!r.has_errors(), "{}", r.render(&s));
        assert_eq!(res.layer_a_rewrites, 1);
    }
    #[test]
    fn layer_b_ambiguous() {
        let (_, r, _) = resolve_src(
            "kriyA preShaya(kartA preShaka, karma patra) { phalam 1; }\npreShaya(patram, rAmam);",
        );
        assert!(r.diags.iter().any(|d| d.code == "E0005"));
    }
}
