//! Diagnostic snapshot coverage.
//!
//! Every documented code in `lexicon/lexicon.toml` is exercised in all
//! three diagnostic languages (`en`, `sa`, `hi`). Most codes are produced by
//! a real erroneous Sūkti program through `pipeline::run_source`; the few
//! codes that have no producer in the Tier-0 pipeline (E0009, E0010, E0014,
//! W0001) are validated by rendering their translated message through the
//! same `Reporter`/`Diagnostic` machinery used by the pipeline.

use sukti::diag::{Diagnostic, Reporter, Source, Span};
use sukti::lexicon::Lexicon;
use sukti::pipeline::{self, Options};

// (name, source, expected code, max_steps; 0 means the default)
type Snap = (&'static str, &'static str, &'static str, usize);

static CASES: &[Snap] = &[
    // ---- E0001
    ("e0001_at", "@", "E0001", 0),
    ("e0001_hash", "vada(1); #", "E0001", 0),
    ("e0001_dollar", "vada(1); $", "E0001", 0),
    ("e0001_greek", "vada(π);", "E0001", 0),
    ("e0001_backtick", "vada(1); `", "E0001", 0),
    ("e0001_question", "vada(1); ?", "E0001", 0),
    ("e0001_caret", "vada(1); ^", "E0001", 0),
    // ---- E0002
    ("e0002_missing_operand", "vada(1 + );", "E0002", 0),
    ("e0002_bad_params", "kriyA pha( {", "E0002", 0),
    ("e0002_no_parens", "yadi satya { }", "E0002", 0),
    ("e0002_unclosed_call", "vada(1 + 2", "E0002", 0),
    ("e0002_missing_ident", "astu = 5;", "E0002", 0),
    ("e0002_break_outside", "virama;", "E0002", 0),
    ("e0002_continue_outside", "anuvarta;", "E0002", 0),
    ("e0002_double_op", "vada(1 + * 2);", "E0002", 0),
    // ---- E0003
    ("e0003_dup_karta", "kriyA pha(kartA ka) { phalam ka; } pha(kartA: 1, kartA: 2);", "E0003", 0),
    ("e0003_dup_karma", "kriyA pha(karma ka) { phalam ka; } pha(karma: 1, karma: 2);", "E0003", 0),
    ("e0003_dup_third", "kriyA pha(kartA ka, karma pa) { phalam 1; } pha(kartA: 1, karma: 2, kartA: 3);", "E0003", 0),
    ("e0003_dup_both", "kriyA pha(kartA ka, karma pa) { phalam 1; } pha(kartA: 1, karma: 2, kartA: 3, karma: 4);", "E0003", 0),
    // ---- E0004
    ("e0004_no_args", "kriyA pha(kartA ka) { phalam ka; } pha();", "E0004", 0),
    ("e0004_wrong_role", "kriyA pha(kartA ka) { phalam ka; } pha(karma: 1);", "E0004", 0),
    ("e0004_second_missing", "kriyA pha(kartA ka, karma pa) { phalam 1; } pha(kartA: 1);", "E0004", 0),
    ("e0004_role_unknown", "kriyA pha(kartA ka) { phalam ka; } pha(sampradAna: 1);", "E0004", 0),
    // ---- E0005
    ("e0005_two_nouns", "kriyA preShaya(kartA preShaka, karma patra) { phalam 1; } preShaya(patram, rAmam);", "E0005", 0),
    ("e0005_three_nouns", "kriyA bheja(kartA ka, karma kha, karaNa ga) { phalam 1; } bheja(pustakam, rAmam, DAkam);", "E0005", 0),
    ("e0005_neuter", "kriyA da(kartA ka, karma kha) { phalam 1; } da(phalam, pustakam);", "E0005", 0),
    // ---- E0006
    ("e0006_two_apavadas", "sUtra pha(va) { phalam 0; } apavAda sUtra pha(va) yadi (va > 0) { phalam 1; } apavAda sUtra pha(va) yadi (va > 1) { phalam 2; } vada(pha(5));", "E0006", 0),
    ("e0006_eq_variants", "sUtra ga(va) { phalam 0; } apavAda sUtra ga(va) yadi (va >= 0) { phalam 1; } apavAda sUtra ga(va) yadi (va <= 10) { phalam 2; } vada(ga(5));", "E0006", 0),
    ("e0006_string_guards", "sUtra ha(sa) { phalam 0; } apavAda sUtra ha(sa) yadi (sa != \"x\") { phalam 1; } apavAda sUtra ha(sa) yadi (sa != \"y\") { phalam 2; } vada(ha(\"ka\"));", "E0006", 0),
    // ---- E0007
    ("e0007_var", "vada(aparichita);", "E0007", 0),
    ("e0007_ascii", "vada(missing);", "E0007", 0),
    ("e0007_assign", "vada(1); aparichita = 5;", "E0007", 0),
    ("e0007_inside_fn", "kriyA pha() { phalam aparichita; } vada(pha());", "E0007", 0),
    ("e0007_module_member", "vada(gaNita.asti());", "E0007", 0),
    ("e0007_call_undefined", "aparichita();", "E0007", 0),
    // ---- E0008
    ("e0008_neg_str", "vada(-\"a\");", "E0008", 0),
    ("e0008_add_bool", "vada(1 + satya);", "E0008", 0),
    ("e0008_range_str", "vada(\"ka\"..3);", "E0008", 0),
    ("e0008_call_int", "vada(5(1));", "E0008", 0),
    ("e0008_index_str", "vada([1, 2][\"ka\"]);", "E0008", 0),
    ("e0008_set_field_int", "vada(1); 5.nAma = 2;", "E0008", 0),
    // ---- E0011
    ("e0011_empty_channel", "astu cha = su.chainala(); vada(cha.prApta());", "E0011", 0),
    ("e0011_task_blocked", "astu cha = su.chainala(); tantu ka { vada(cha.prApta()); }", "E0011", 0),
    ("e0011_two_recv", "astu cha = su.chainala(); cha.preShaya(1); vada(cha.prApta()); vada(cha.prApta());", "E0011", 0),
    // ---- E0012
    ("e0012_div_zero", "vada(1 / 0);", "E0012", 0),
    ("e0012_mod_zero", "vada(1 % 0);", "E0012", 0),
    ("e0012_float_zero", "vada(1.5 / 0.0);", "E0012", 0),
    ("e0012_in_fn", "kriyA pha(na) { phalam 1 / na; } vada(pha(0));", "E0012", 0),
    // ---- E0013
    ("e0013_index_far", "vada([1][5]);", "E0013", 0),
    ("e0013_index_get", "vada([1, 2].prApta(9));", "E0013", 0),
    ("e0013_index_assign", "astu la = [1]; la[4] = 2;", "E0013", 0),
    ("e0013_tuple_index", "vada((1, 2)[7]);", "E0013", 0),
    ("e0013_empty_index", "vada([][0]);", "E0013", 0),
    // ---- E0015
    ("e0015_zwsp_end", "vada(1);\u{200b}", "E0015", 0),
    ("e0015_zwsp_start", "\u{200b}vada(1);", "E0015", 0),
    ("e0015_zwsp_mid", "vada(1)\u{200b};", "E0015", 0),
    ("e0015_word_joiner", "\u{2060}vada(1);", "E0015", 0),
    ("e0015_lrm", "vada(1);\u{200e}", "E0015", 0),
    ("e0015_bom", "\u{feff}vada(1);", "E0015", 0),
    // ---- E0016
    ("e0016_infinite_while", "astu na = 0; yAvat (satya) { na = na + 1; }", "E0016", 5),
    ("e0016_deep_recursion", "kriyA pha(na) { phalam pha(na + 1); } vada(pha(0));", "E0016", 50),
    ("e0016_long_loop", "astu na = 0; yAvat (na < 100) { na = na + 1; }", "E0016", 10),
    // ---- E0017
    ("e0017_fail", "parIkShA.viphala(\"boom\");", "E0017", 0),
    ("e0017_eq_int", "parIkShA.sama(1, 2);", "E0017", 0),
    ("e0017_eq_str", "parIkShA.sama(\"ka\", \"kha\");", "E0017", 0),
    ("e0017_fail_no_arg", "parIkShA.viphala();", "E0017", 0),
];

/// Codes that are documented in the lexicon but have no producer in the
/// current Tier-0 pipeline. We still verify their trilingual rendering.
static MESSAGE_ONLY_CODES: &[&str] = &["E0009", "E0010", "E0014", "W0001"];

fn check_snapshot((name, src, code, max_steps): &Snap) {
    for lang in ["en", "sa", "hi"] {
        let opts = Options {
            lang: lang.to_string(),
            max_steps: if *max_steps == 0 {
                200_000_000
            } else {
                *max_steps
            },
            ..Options::default()
        };
        let out = pipeline::run_source(src, name, &opts)
            .unwrap_or_else(|e| panic!("[{name}/{lang}] run_source returned Err: {e}"));
        assert!(
            !out.diagnostics.is_empty(),
            "[{name}/{lang}] expected a diagnostic, got none; output:\n{}",
            out.output
        );
        assert!(
            out.diagnostics.contains(code),
            "[{name}/{lang}] expected code {code} in:\n{}",
            out.diagnostics
        );
        let first = out.diagnostics.lines().next().unwrap_or("");
        assert!(
            first.len() > code.len() + 5,
            "[{name}/{lang}] diagnostic has no rendered message: {first:?}"
        );
    }
}

fn check_message_code(code: &str) {
    let lex = Lexicon::load();
    let args: &[(&str, String)] = &[
        ("name", "rAma".into()),
        ("char", "@".into()),
        ("expected", "X".into()),
        ("found", "Y".into()),
        ("role", "kartA".into()),
        ("detail", "ambiguous".into()),
        ("a", "clause#1".into()),
        ("b", "clause#2".into()),
        ("index", "9".into()),
        ("container", "sUchI".into()),
        ("len", "3".into()),
        ("cycle", "a→b".into()),
        ("tasks", "t1".into()),
    ];
    let src = Source::new("snippet.sukti", "vada(1);");
    for lang in ["en", "sa", "hi"] {
        let msg = lex.message(code, lang, args);
        assert!(
            !msg.is_empty() && !msg.starts_with('['),
            "{code}/{lang}: {msg}"
        );
        let mut rep = Reporter::new(lang);
        rep.push(Diagnostic::new(code, Span::new(0, 3), msg));
        let rendered = rep.render(&src);
        assert!(rendered.contains(code), "{code}/{lang}: {rendered}");
        let first = rendered.lines().next().unwrap_or("");
        assert!(first.len() > code.len() + 5, "{code}/{lang}: {first:?}");
    }
}

#[test]
fn diagnostic_snapshots() {
    assert!(CASES.len() >= 60, "only {} snippets", CASES.len());
    let mut names = std::collections::HashSet::new();
    for case in CASES {
        assert!(names.insert(case.0), "duplicate case name {}", case.0);
        check_snapshot(case);
    }
    for code in MESSAGE_ONLY_CODES {
        check_message_code(code);
    }
    eprintln!(
        "ran {} diagnostic snippets x 3 languages + {} message-only codes",
        CASES.len(),
        MESSAGE_ONLY_CODES.len()
    );
}

#[test]
fn translations_differ_across_languages() {
    let lex = Lexicon::load();
    let args: &[(&str, String)] = &[
        ("name", "rAma".into()),
        ("char", "@".into()),
        ("detail", "ambiguous".into()),
    ];
    // Codes whose messages come from the data-driven lexicon (not raw runtime strings).
    for code in [
        "E0001", "E0002", "E0003", "E0004", "E0005", "E0007", "E0008", "E0013", "E0015",
    ] {
        let en = lex.message(code, "en", args);
        let sa = lex.message(code, "sa", args);
        let hi = lex.message(code, "hi", args);
        assert_ne!(en, sa, "{code}: en == sa");
        assert_ne!(en, hi, "{code}: en == hi");
        assert_ne!(sa, hi, "{code}: sa == hi");
    }
}
