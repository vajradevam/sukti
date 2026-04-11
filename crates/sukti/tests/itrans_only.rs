//! Regression guard: Sūkti source and user-facing output are ITRANS (ASCII).

use std::path::Path;
use sukti::pipeline::{self, Options};

fn assert_ascii(text: &str, what: &str) {
    assert!(
        text.is_ascii(),
        "{what} contains non-ASCII characters: {:?}",
        text.chars()
            .filter(|c| !c.is_ascii())
            .take(8)
            .collect::<String>()
    );
}

#[test]
fn shipped_examples_and_goldens_are_ascii() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut n = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if ext == "sukti" || ext == "expected" {
            assert_ascii(
                &std::fs::read_to_string(&p).unwrap(),
                &p.display().to_string(),
            );
            n += 1;
        }
    }
    assert!(n >= 28, "only {n} example/expected files found");
}

#[test]
fn runtime_output_and_diagnostics_are_ascii() {
    let src = r#"kriyA preshaya(kartA ka, karma pa) { phalam ka + ":" + pa; }
vada(preshaya(karma: "patra", kartA: "rAma"));
vada(satya); vada(shUnya);
"#;
    let out = pipeline::run_source(src, "itrans", &Options::default()).unwrap();
    assert_ascii(&out.output, "program output");
    assert_ascii(&out.diagnostics, "diagnostics");
    assert!(out.output.contains("rAma:patra"));
    assert!(out.output.contains("satya"));
    assert!(out.output.contains("shUnya"));
}

#[test]
fn bad_source_diagnostics_are_ascii() {
    let out = pipeline::run_source("kriyA {", "bad", &Options::default()).unwrap();
    assert_ascii(&out.diagnostics, "parse diagnostics");
    assert!(out.diagnostics.contains("E000"));
}
