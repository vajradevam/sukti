//! Diagnostic catalogue snapshots: every error code renders in all three
//! languages and includes a source snippet with a caret where applicable.

use sukti::diag::{Reporter, Source};
use sukti::lexer::Lexer;
use sukti::lexicon::Lexicon;
use sukti::parser::Parser;

#[test]
fn all_codes_trilingual() {
    let l = Lexicon::load();
    let args = [
        ("char", "@".to_string()),
        ("expected", "x".into()),
        ("found", "y".into()),
        ("role", "kartA".into()),
        ("detail", "ambiguous".into()),
        ("a", "1".into()),
        ("b", "2".into()),
        ("name", "rAma".into()),
        ("index", "9".into()),
        ("container", "sUchI".into()),
        ("len", "3".into()),
        ("cycle", "a→b".into()),
        ("tasks", "t1".into()),
    ];
    let count = l.all_ids().count();
    assert!(count > 30);
    // Collect codes from the TOML via the public message API by probing the
    // documented set; each must render non-empty and differ from the code.
    for code in (1..=17)
        .map(|n| format!("E{n:04}"))
        .chain(std::iter::once("W0001".into()))
    {
        for lang in ["sa", "en", "hi"] {
            let m = l.message(&code, lang, &args);
            assert!(!m.is_empty(), "{code}/{lang} empty");
            assert!(!m.starts_with('['), "{code}/{lang} unresolved: {m}");
        }
    }
}

#[test]
fn parse_error_has_caret() {
    let src = Source::new("snippet.sukti", "vada(1 + );");
    let l = Lexicon::load();
    let mut r = Reporter::new("en");
    let toks = Lexer::new(&src, &l, &mut r).tokenize();
    let mut p = Parser::new(&src, &l, &mut r, toks);
    p.parse_program();
    assert!(r.has_errors());
    let rendered = r.render(&src);
    assert!(rendered.contains('^'), "no caret in:\n{rendered}");
    assert!(
        rendered.contains("snippet.sukti:1:"),
        "no location in:\n{rendered}"
    );
}
