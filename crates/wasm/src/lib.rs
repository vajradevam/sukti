#![allow(unsafe_code)] // raw WASM exports and linear-memory buffers
//! WASM build of Sūkti's deterministic Tier-0 interpreter.
//!
//! The browser cannot use Cranelift's native JIT, so this build runs the real
//! interpreter and exposes a **replay debugger**: the program is executed with
//! a statement-level trace hook, and the debugger steps through the recorded
//! frames. This is honest — it never pretends native code is running in WASM.
//!
//! ABI (all strings are UTF-8 in linear memory):
//!   `sukti_alloc(len) -> ptr`  allocate a scratch buffer
//!   `sukti_load(ptr, len)`     parse + run source, build the trace
//!   `sukti_step() -> i32`      advance one frame (1 = more, 0 = done)
//!   `sukti_continue() -> i32`  advance to the next breakpoint/end
//!   `sukti_set_breakpoint(line)`
//!   `sukti_state_ptr()/len()`  JSON state for the debugger panes

use serde_json::json;
use std::cell::RefCell;
use sukti::ast::StmtKind;
use sukti::diag::{Reporter, Source};
use sukti::fmt;
use sukti::lexer::Lexer;
use sukti::lexicon::Lexicon;
use sukti::parser::Parser;
use sukti::value::Value;
use sukti::vm::VM;

#[derive(Clone, Debug)]
struct Frame {
    line: usize,
    depth: usize,
    vars: Vec<(String, String)>,
    heap: usize,
}

#[derive(Default)]
struct Session {
    output: String,
    diagnostics: String,
    frames: Vec<Frame>,
    index: usize,
    breakpoints: Vec<usize>,
    tasks: Vec<String>,
    layer_a: usize,
    layer_b: usize,
    sutra: Vec<String>,
    source_lines: Vec<String>,
}

thread_local! {
    static SESSION: RefCell<Session> = RefCell::new(Session::default());
    static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn env_snapshot(env: &sukti::value::EnvRef) -> (Vec<(String, String)>, usize) {
    let mut vars = Vec::new();
    let mut heap = 0usize;
    let mut cur = Some(env.clone());
    while let Some(e) = cur {
        let b = e.borrow();
        for (name, binding) in &b.vars {
            let v = &binding.value;
            match v {
                Value::List(_) | Value::Map(_) | Value::Instance(_) => heap += 1,
                _ => {}
            }
            if vars.len() < 24 {
                vars.push((name.clone(), v.display()));
            }
        }
        cur = b.parent.clone();
    }
    (vars, heap)
}

fn depth_of(env: &sukti::value::EnvRef) -> usize {
    let mut d = 0;
    let mut cur = Some(env.clone());
    while let Some(e) = cur {
        d += 1;
        cur = e.borrow().parent.clone();
    }
    d
}

fn collect_sutras(stmts: &[sukti::ast::Stmt], out: &mut Vec<String>) {
    for s in stmts {
        match &s.kind {
            StmtKind::Sutra { name, clauses, .. } => {
                out.push(format!(
                    "{name}: {} clauses ({})",
                    clauses.len(),
                    clauses
                        .iter()
                        .map(|c| match c.kind {
                            sukti::ast::SutraKind::Utsarga => "utsarga",
                            sukti::ast::SutraKind::Apavada => "apavāda",
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            StmtKind::If { then, els, .. } => {
                collect_sutras(then, out);
                collect_sutras(els, out);
            }
            StmtKind::While { body, .. } | StmtKind::For { body, .. } => collect_sutras(body, out),
            StmtKind::Fn(f) => collect_sutras(&f.body, out),
            _ => {}
        }
    }
}

fn run_source(source: &str) -> Session {
    SESSION.with(|c| c.borrow_mut().frames.clear());
    let mut sess = Session {
        source_lines: source.lines().map(str::to_string).collect(),
        ..Session::default()
    };
    let canonical = fmt::to_canonical(source, sukti_sanskrit::Script::Itrans);
    let src = Source::new("playground", &canonical);
    let lex = Lexicon::load();
    let mut rep = Reporter::new("en");
    let toks = Lexer::new(&src, &lex, &mut rep).tokenize();
    let parsed = Parser::new(&src, &lex, &mut rep, toks).parse_program();
    if rep.has_errors() {
        sess.diagnostics = rep.render(&src);
        return sess;
    }
    let resolved = sukti::resolve::resolve(parsed, &lex, &mut rep);
    if rep.has_errors() {
        sess.diagnostics = rep.render(&src);
        return sess;
    }
    sess.layer_a = resolved.layer_a_rewrites;
    sess.layer_b = resolved.layer_b_rewrites;
    collect_sutras(&resolved.stmts, &mut sess.sutra);

    let mut vm = VM::new("en");
    sukti::stdlib::register(&mut vm);
    vm.set_source(src.clone());
    let trace_src = src.clone();
    vm.trace_hook = Some(Box::new(move |vm: &mut VM, span, env| {
        let line = trace_src.line_col(span.start).0;
        let (vars, heap) = env_snapshot(env);
        let _ = vm;
        SESSION.with(|cell| {
            let mut s = cell.borrow_mut();
            if s.frames.len() < 4000 {
                s.frames.push(Frame {
                    line,
                    depth: depth_of(env),
                    vars,
                    heap,
                });
            }
        });
    }));
    match vm.run(&resolved.stmts) {
        Ok(()) => sess.output = vm.output.clone(),
        Err(f) => {
            sess.diagnostics = if let Some(sp) = f.span {
                format!("runtime {}: {}\n{}", f.kind, f.message, src.snippet(sp))
            } else {
                format!("runtime {}: {}", f.kind, f.message)
            };
            sess.output = vm.output.clone();
        }
    }
    sess.frames = SESSION.with(|c| c.borrow().frames.clone());
    sess.tasks = vm
        .tasks
        .iter()
        .map(|t| {
            let t = t.borrow();
            format!(
                "{}: {} ({})",
                t.name,
                if t.finished { "finished" } else { "running" },
                if t.blocked_on.is_some() {
                    "blocked"
                } else {
                    "ready"
                }
            )
        })
        .collect();
    sess
}

fn state_json(s: &Session) -> String {
    let frame = s.frames.get(s.index);
    let stack: Vec<_> = match frame {
        Some(f) => (0..f.depth)
            .map(|d| json!({ "scope": format!("scope #{d}"), "note": "environment frame" }))
            .collect(),
        None => vec![],
    };
    let vars: Vec<_> = frame
        .map(|f| {
            f.vars
                .iter()
                .map(|(k, v)| json!([k, v]))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let roles: Vec<_> = [
        "kartA",
        "karma",
        "karaNa",
        "sampradAna",
        "apAdAna",
        "adhikaraNa",
        "sambandha",
    ]
    .iter()
    .map(|r| json!(r))
    .collect();
    json!({
        "line": frame.map_or(0, |f| f.line),
        "index": s.index,
        "total": s.frames.len(),
        "done": s.index >= s.frames.len(),
        "output": s.output,
        "diagnostics": s.diagnostics,
        "stack": stack,
        "variables": vars,
        "heap": { "objects": frame.map_or(0, |f| f.heap) },
        "tasks": s.tasks,
        "karaka": { "layer_a": s.layer_a, "layer_b": s.layer_b, "roles": roles },
        "sutra": s.sutra,
        "tier": {
            "mode": "Tier 0",
            "note": "WASM build runs the deterministic interpreter; the native Cranelift JIT is not available in a browser."
        },
        "source": s.source_lines,
        "breakpoints": s.breakpoints,
    })
    .to_string()
}

/// Allocate a scratch buffer of `len` bytes in WASM linear memory.
#[no_mangle]
pub extern "C" fn sukti_alloc(len: usize) -> *mut u8 {
    SCRATCH.with(|s| {
        let mut b = s.borrow_mut();
        b.resize(len, 0);
        b.as_mut_ptr()
    })
}

/// Parse, resolve and run a UTF-8 Sūkti program, building the replay trace.
///
/// # Safety
/// `ptr` must point to `len` initialized bytes returned by `sukti_alloc`.
#[no_mangle]
pub unsafe extern "C" fn sukti_load(ptr: *const u8, len: usize) {
    let bytes = std::slice::from_raw_parts(ptr, len);
    let source = String::from_utf8_lossy(bytes).into_owned();
    let session = run_source(&source);
    SESSION.with(|s| *s.borrow_mut() = session);
}

/// Advance the replay cursor by one recorded statement.
#[no_mangle]
pub extern "C" fn sukti_step() -> i32 {
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        if s.index < s.frames.len() {
            s.index += 1;
        }
        i32::from(s.index < s.frames.len())
    })
}

/// Advance until a breakpoint line (or the end of the trace).
#[no_mangle]
pub extern "C" fn sukti_continue() -> i32 {
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        while s.index < s.frames.len() {
            s.index += 1;
            if let Some(f) = s.frames.get(s.index) {
                if s.breakpoints.contains(&f.line) {
                    break;
                }
            }
        }
        i32::from(s.index < s.frames.len())
    })
}

#[no_mangle]
pub extern "C" fn sukti_set_breakpoint(line: u32) {
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        if !s.breakpoints.contains(&(line as usize)) {
            s.breakpoints.push(line as usize);
        }
    });
}

#[no_mangle]
pub extern "C" fn sukti_clear_breakpoints() {
    SESSION.with(|s| s.borrow_mut().breakpoints.clear());
}

#[no_mangle]
pub extern "C" fn sukti_reset() {
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        s.index = 0;
    });
}

fn write_result(s: String) {
    SCRATCH.with(|buf| {
        let mut b = s.into_bytes();
        b.push(0);
        *buf.borrow_mut() = b;
    });
}

/// Materialize the current debugger state as JSON in the scratch buffer.
#[no_mangle]
pub extern "C" fn sukti_state() {
    SESSION.with(|s| write_result(state_json(&s.borrow())));
}

/// Materialize a literal string into the scratch buffer (helper for tests).
#[no_mangle]
pub extern "C" fn sukti_echo() {
    write_result("sukti-wasm ready".into());
}

#[no_mangle]
pub extern "C" fn sukti_state_ptr() -> *const u8 {
    SCRATCH.with(|s| s.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn sukti_state_len() -> usize {
    SCRATCH.with(|s| s.borrow().len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scratch_json() -> serde_json::Value {
        let ptr = sukti_state_ptr();
        let len = sukti_state_len();
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        serde_json::from_slice(bytes).unwrap()
    }

    #[test]
    fn raw_abi_loads_steps_and_reports() {
        let src = "astu sa = 0; sa = 42; vada(sa);";
        let bytes = src.as_bytes();
        let ptr = sukti_alloc(bytes.len());
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            sukti_load(ptr, bytes.len());
        }
        sukti_state();
        let state = scratch_json();
        assert_eq!(state["output"], "42\n");
        assert!(state["total"].as_u64().unwrap() >= 3);
        assert_eq!(state["index"], 0);
        sukti_step();
        sukti_state();
        assert_eq!(scratch_json()["index"], 1);
        sukti_reset();
        sukti_state();
        assert_eq!(scratch_json()["index"], 0);
        sukti_set_breakpoint(3);
        sukti_continue();
        sukti_state();
        assert!(scratch_json()["breakpoints"]
            .as_array()
            .unwrap()
            .contains(&json!(3)));
    }

    #[test]
    fn state_json_is_well_formed() {
        let s = Session {
            output: "42\n".into(),
            frames: vec![Frame {
                line: 1,
                depth: 1,
                vars: vec![("na".into(), "42".into())],
                heap: 0,
            }],
            ..Session::default()
        };
        let v: serde_json::Value = serde_json::from_str(&state_json(&s)).unwrap();
        assert_eq!(v["line"], 1);
        assert_eq!(v["output"], "42\n");
        assert!(v["tier"]["note"]
            .as_str()
            .unwrap()
            .contains("not available"));
    }
}
