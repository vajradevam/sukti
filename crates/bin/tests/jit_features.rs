//! Feature tests for the full tiered engine: overflow deopt, precise metadata,
//! repeated-deopt blacklisting, OSR, inline caches and suspendable compiled code.
//!
//! These are structural assertions on the profiler counters (deterministic)

use std::sync::{Mutex, MutexGuard};
use sukti::pipeline::{self, Options};

static SERIAL: Mutex<()> = Mutex::new(());

fn run(_guard: &MutexGuard<'_, ()>, src: &str, dump: bool) -> String {
    let opts = Options {
        jit: "on".into(),
        jit_threshold: 1,
        dump_jit: dump,
        ..Options::default()
    };
    let out = pipeline::run_source(src, "jit-feature", &opts).expect("run_source");
    assert!(
        out.diagnostics.is_empty(),
        "unexpected diagnostics: {}",
        out.diagnostics
    );
    out.output
}

fn stat(output: &str, key: &str) -> usize {
    output
        .lines()
        .find(|l| l.starts_with("calls="))
        .and_then(|l| l.split_whitespace().find(|kv| kv.starts_with(key)))
        .and_then(|kv| kv.split('=').nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn install() {
    sukti::set_tier1_factory(|t| {
        let mut jit = sukti_jit::Jit::new(t);
        if let Some(path) = sukti::jit_cache_path() {
            jit = jit.with_cache(path);
        }
        Box::new(jit)
    });
}

#[test]
fn overflow_deopts_to_bigint_tier0_with_precise_metadata() {
    let g = SERIAL.lock().unwrap();
    install();
    let src = "kriyA sq(a) { phalam a * a; }\nvada(sq(5000000000));";
    let out = run(&g, src, true);
    assert!(out.starts_with("25000000000000000000\n"), "{out}");
    assert!(stat(&out, "deopt_overflow=") >= 1, "{out}");
    assert!(
        out.contains("deopt-site"),
        "missing precise site table: {out}"
    );
    assert!(out.contains("overflow"), "missing overflow reason: {out}");
}

#[test]
fn repeated_deopts_blacklist_the_function() {
    let g = SERIAL.lock().unwrap();
    install();
    let src =
        "kriyA sq(a) { phalam a * a; }\nastu i = 0; yAvat (i < 5) { vada(sq(5000000000)); i = i + 1; }";
    let out = run(&g, src, false);
    assert_eq!(out.matches("25000000000000000000").count(), 5, "{out}");
    assert_eq!(stat(&out, "repeated_deopt_blacklist="), 1, "{out}");
}

#[test]
fn hot_loop_is_replaced_by_a_native_kernel() {
    let g = SERIAL.lock().unwrap();
    install();
    let src = "astu sa = 0; astu i = 0; yAvat (i < 100) { sa = sa + i; i = i + 1; } vada(sa);";
    let out = run(&g, src, true);
    assert!(out.starts_with("4950\n"), "{out}");
    assert!(stat(&out, "osr_compiled=") >= 1, "{out}");
    assert!(stat(&out, "osr_runs=") >= 1, "{out}");
    assert!(out.contains("osr-kernel"), "{out}");
}

#[test]
fn compiled_tier_transition_uses_inline_cache() {
    let g = SERIAL.lock().unwrap();
    install();
    let src =
        "kriyA dbl(na) { phalam na * 2; }\nastu i = 0; yAvat (i < 20) { vada(dbl(i)); i = i + 1; }";
    let out = run(&g, src, false);
    assert!(out.starts_with("0\n2\n4\n"), "{out}");
    assert!(stat(&out, "icache_hits=") >= 15, "{out}");
}

#[test]
fn compiled_loops_are_suspendable_at_backedges() {
    let g = SERIAL.lock().unwrap();
    install();
    sukti_jit::arm_yield_after(10);
    let src = "astu sa = 0; astu i = 0; yAvat (i < 200) { sa = sa + i; i = i + 1; } vada(sa);";
    let out = run(&g, src, false);
    sukti_jit::arm_yield_after(0);
    assert!(out.starts_with("19900\n"), "{out}");
    assert!(stat(&out, "deopt_yield=") >= 1, "{out}");
}

#[test]
fn compilation_plan_cache_roundtrips_across_engines() {
    let g = SERIAL.lock().unwrap();
    install();
    let path = std::env::temp_dir().join(format!("sukti-jit-plan-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let cache = path.to_string_lossy().into_owned();
    let opts = Options {
        jit: "on".into(),
        jit_threshold: 1,
        jit_cache: Some(cache.clone()),
        ..Options::default()
    };
    let src = "kriyA dbl(na) { phalam na * 2; } vada(dbl(21));";
    let first = pipeline::run_source(src, "cache-1", &opts).expect("first run");
    assert!(first.output.starts_with("42\n"), "{}", first.output);
    assert!(path.exists(), "compilation plan was not written");
    let second = pipeline::run_source(src, "cache-2", &opts).expect("second run");
    sukti::set_jit_cache_path(None);
    let _ = std::fs::remove_file(&path);
    assert!(second.output.starts_with("42\n"), "{}", second.output);
    assert!(
        stat(&second.output, "code_cache_hits=") >= 1,
        "{}",
        second.output
    );
    drop(g);
}

#[test]
fn non_jit_workloads_do_no_profiler_work() {
    let g = SERIAL.lock().unwrap();
    install();
    // Method-heavy: every call is rejected by the conservative Func hint.
    let methods = include_str!("../../../bench/methods.sukti");
    let out = run(&g, methods, false);
    assert_eq!(stat(&out, "calls="), 0, "{out}");
    assert_eq!(stat(&out, "compiled="), 0, "{out}");
    assert_eq!(stat(&out, "osr_backedges="), 0, "{out}");
    // List-heavy: the loop body uses list methods, so no OSR kernel either.
    let sieve = include_str!("../../../bench/sieve.sukti");
    let out = run(&g, sieve, false);
    assert_eq!(stat(&out, "calls="), 0, "{out}");
    assert_eq!(stat(&out, "osr_backedges="), 0, "{out}");
}
