//! Property tests: kāraka permutation determinism and formatter round-trip.

use proptest::prelude::*;
use sukti::diag::{Reporter, Source};
use sukti::fmt::{self, View};
use sukti::lexer::Lexer;
use sukti::lexicon::Lexicon;
use sukti::parser::Parser;
use sukti::resolve;

fn parse(src: &str) -> Vec<sukti::ast::Stmt> {
    let s = Source::new("prop", src);
    let l = Lexicon::load();
    let mut r = Reporter::new("en");
    let toks = Lexer::new(&s, &l, &mut r).tokenize();
    let mut p = Parser::new(&s, &l, &mut r, toks);
    let st = p.parse_program();
    assert!(!r.has_errors(), "{}", r.render(&s));
    st
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Permuting role-labelled arguments must produce the same positional call.
    #[test]
    fn karaka_permutation_is_deterministic(perm in prop::collection::vec(0usize..4, 4)) {
        let labels = ["kartA", "karma", "karaNa", "sampradAna"];
        let values = ["rAma", "patra", "DAka", "shyAma"];
        let mut idx = perm;
        idx.sort(); idx.dedup();
        for k in 0..4 { if !idx.contains(&k) { idx.push(k); } }
        idx.truncate(4);
        let args: Vec<String> = idx.iter().map(|i| format!("{}: \"{}\"", labels[*i], values[*i])).collect();
        let src = format!(
            "kriyA preShaya(kartA ka, karma pa, karaNa ma, sampradAna ga) {{ phalam ka; }}\npreShaya({});",
            args.join(", "));
        let st = parse(&src);
        let l = Lexicon::load();
        let mut r = Reporter::new("en");
        let res = resolve::resolve(st, &l, &mut r);
        prop_assert!(r.diags.is_empty(), "{:?}", r.diags);
        prop_assert_eq!(res.layer_a_rewrites, 1);
        // After rewriting, every call argument must be positional and in the
        // declared parameter order: rAma, patra, DAka, shyAma.
        let mut seen = Vec::new();
        fn walk(stmts: &[sukti::ast::Stmt], out: &mut Vec<String>) {
            for s in stmts {
                if let sukti::ast::StmtKind::Expr(e) = &s.kind {
                    if let sukti::ast::ExprKind::Call { args, .. } = &e.kind {
                        for a in args { out.push(format!("{:?}", a.expr.kind)); }
                    }
                }
            }
        }
        walk(&res.stmts, &mut seen);
        prop_assert_eq!(seen.len(), 4);
        prop_assert_eq!(&seen[0], &format!("Template([Text({:?})])", values[0]));
        prop_assert_eq!(&seen[1], &format!("Template([Text({:?})])", values[1]));
        prop_assert_eq!(&seen[2], &format!("Template([Text({:?})])", values[2]));
        prop_assert_eq!(&seen[3], &format!("Template([Text({:?})])", values[3]));
    }

    /// Formatter idempotence over generated arithmetic programs.
    #[test]
    fn formatter_idempotent(a in 0i64..1000, b in 0i64..1000) {
        let src = format!("vada({} + {} * 2);", a, b);
        let st = parse(&src);
        let l = Lexicon::load();
        let once = fmt::format(&st, View::Itrans, &l);
        let twice = fmt::format(&parse(&once), View::Itrans, &l);
        prop_assert_eq!(once, twice);
    }

    /// ITRANS source is canonical: formatting yields ASCII ITRANS unchanged.
    #[test]
    fn itrans_source_is_canonical(n in 0i64..500) {
        let src = format!("vada(\"namaste {}\");", n);
        let st = parse(&src);
        let l = Lexicon::load();
        let out = fmt::format(&st, View::Itrans, &l);
        prop_assert!(out.is_ascii());
        prop_assert_eq!(out, format!("{src}\n"));
    }
}
