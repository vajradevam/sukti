//! Executable specification: every ```sukti-test block in tests/spec_blocks.txt is a test.

use sukti::pipeline::{self, Options};

#[test]
fn spec_blocks() {
    let spec = include_str!("../../../tests/spec_blocks.txt");
    let mut blocks = Vec::new();
    let mut cur: Option<(String, Vec<String>)> = None;
    for line in spec.lines() {
        if line.trim() == "```sukti-test" {
            cur = Some((String::new(), Vec::new()));
        } else if line.trim() == "```" {
            if let Some(b) = cur.take() {
                blocks.push(b);
            }
        } else if let Some((src, exp)) = &mut cur {
            if let Some(e) = line.trim().strip_prefix("// =>") {
                exp.push(e.trim().to_string());
            } else {
                src.push_str(line);
                src.push('\n');
            }
        }
    }
    assert!(!blocks.is_empty(), "no spec blocks found");
    let opts = Options::default();
    for (i, (src, expected)) in blocks.iter().enumerate() {
        let out = pipeline::run_source(src, &format!("spec#{i}"), &opts)
            .unwrap_or_else(|e| panic!("spec block {i} failed to run: {e}"));
        assert!(
            out.diagnostics.is_empty(),
            "spec block {i} diagnostics:\n{}",
            out.diagnostics
        );
        let actual: Vec<&str> = out.output.lines().collect();
        assert_eq!(
            actual,
            expected.iter().map(String::as_str).collect::<Vec<_>>(),
            "spec block {i} output mismatch\nsource:\n{src}"
        );
    }
    eprintln!("ran {} executable spec blocks", blocks.len());
}
