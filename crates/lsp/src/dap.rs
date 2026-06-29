//! The DAP server. The program is executed to completion up front with a VM
//! `trace_hook`; stepping then navigates the recorded frames. This is
//! deterministic trace replay, not live pausing of native code.

use crate::analysis::parse;
use crate::jsonrpc::{read_message, write_message, ReadOutcome};
use num_bigint::BigInt;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::rc::Rc;
use sukti::value::{EnvRef, Value as SuValue};
use sukti::vm::VM;

/// Never keep more than this many trace frames in memory.
const MAX_FRAMES: usize = 2000;

#[derive(Clone, Debug)]
struct Variable {
    name: String,
    value: String,
    ty: String,
}

#[derive(Clone, Debug)]
struct TraceFrame {
    /// Zero-based source line, as required by DAP on the wire.
    line: u32,
    /// Call-stack depth. Uniform (0) for this deterministic replay.
    depth: u32,
    locals: Vec<Variable>,
    heap: Vec<Variable>,
}

fn int_string(i: &BigInt) -> String {
    i.to_string()
}

fn variable(name: String, value: &SuValue) -> Option<Variable> {
    let (display, ty) = match value {
        SuValue::Int(i) => (int_string(i), "Int"),
        SuValue::Bool(b) => (b.to_string(), "Bool"),
        SuValue::Str(s) => (s.clone(), "String"),
        _ => return None,
    };
    Some(Variable {
        name,
        value: display,
        ty: ty.into(),
    })
}

/// Snapshot of int/bool/string bindings in the environment chain, from the
/// innermost scope outwards. Nearest binding wins; names are sorted for
/// deterministic output.
fn snapshot_vars(env: &EnvRef) -> Vec<Variable> {
    let mut map: BTreeMap<String, Variable> = BTreeMap::new();
    let mut current = Some(env.clone());
    while let Some(scope) = current {
        let (vars, parent) = {
            let borrowed = scope.borrow();
            (borrowed.vars.clone(), borrowed.parent.clone())
        };
        for (name, binding) in vars {
            if let Some(v) = variable(name, &binding.value) {
                map.entry(v.name.clone()).or_insert(v);
            }
        }
        current = parent;
    }
    map.into_values().collect()
}

/// Counts of heap-allocated List / Map / Instance values in the chain.
fn heap_summary(env: &EnvRef) -> Vec<Variable> {
    let mut lists = 0usize;
    let mut maps = 0usize;
    let mut instances = 0usize;
    let mut current = Some(env.clone());
    while let Some(scope) = current {
        let (vars, parent) = {
            let borrowed = scope.borrow();
            (borrowed.vars.clone(), borrowed.parent.clone())
        };
        for binding in vars.values() {
            match &binding.value {
                SuValue::List(_) => lists += 1,
                SuValue::Map(_) => maps += 1,
                SuValue::Instance(_) => instances += 1,
                _ => {}
            }
        }
        current = parent;
    }
    vec![
        Variable {
            name: "lists".into(),
            value: lists.to_string(),
            ty: "count".into(),
        },
        Variable {
            name: "maps".into(),
            value: maps.to_string(),
            ty: "count".into(),
        },
        Variable {
            name: "instances".into(),
            value: instances.to_string(),
            ty: "count".into(),
        },
    ]
}

/// Parse, resolve and execute `source`, recording one frame before each
/// statement. Any front-end or runtime fault is returned as a human-readable
/// error; the frames recorded so far are intentionally discarded so the
/// debugger never presents a misleading partial trace.
fn collect_trace(source: &str) -> Result<Vec<TraceFrame>, String> {
    let parsed = parse("program", source);
    if parsed.reporter.has_errors() {
        return Err(parsed.reporter.render(&parsed.source));
    }

    let mut vm = VM::new("en");
    sukti::stdlib::register(&mut vm);
    vm.set_source(parsed.source.clone());

    let frames: Rc<RefCell<Vec<TraceFrame>>> = Rc::new(RefCell::new(Vec::new()));
    let hook_frames = frames.clone();
    let hook_source = parsed.source.clone();
    vm.trace_hook = Some(Box::new(move |_vm, span, env| {
        let mut recorded = hook_frames.borrow_mut();
        if recorded.len() >= MAX_FRAMES {
            return;
        }
        let (line, _) = hook_source.line_col(span.start);
        recorded.push(TraceFrame {
            line: line.saturating_sub(1) as u32,
            depth: 0,
            locals: snapshot_vars(env),
            heap: heap_summary(env),
        });
    }));

    if let Err(fault) = vm.run(&parsed.stmts) {
        return Err(format!("runtime {}: {}", fault.kind, fault.message));
    }
    let recorded = frames.borrow().clone();
    Ok(recorded)
}

/// A stateful DAP server. Outgoing messages are returned by [`DapServer::handle`]
/// so the whole protocol is testable without touching stdio.
pub struct DapServer {
    seq: i64,
    source: Option<String>,
    source_path: Option<String>,
    breakpoints: HashSet<u32>,
    frames: Vec<TraceFrame>,
    index: usize,
    run_error: Option<String>,
    done: bool,
}

impl Default for DapServer {
    fn default() -> Self {
        Self::new()
    }
}

impl DapServer {
    pub fn new() -> Self {
        Self {
            seq: 0,
            source: None,
            source_path: None,
            breakpoints: HashSet::new(),
            frames: Vec::new(),
            index: 0,
            run_error: None,
            done: false,
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    fn next_seq(&mut self) -> i64 {
        self.seq += 1;
        self.seq
    }

    fn response(&mut self, request_seq: i64, command: &str, body: Value) -> Value {
        let seq = self.next_seq();
        json!({
            "seq": seq,
            "type": "response",
            "request_seq": request_seq,
            "success": true,
            "command": command,
            "body": body,
        })
    }

    fn error_response(&mut self, request_seq: i64, command: &str, message: &str) -> Value {
        let seq = self.next_seq();
        json!({
            "seq": seq,
            "type": "response",
            "request_seq": request_seq,
            "success": false,
            "command": command,
            "message": message,
            "body": {},
        })
    }

    fn event(&mut self, name: &str, body: Value) -> Value {
        let seq = self.next_seq();
        json!({
            "seq": seq,
            "type": "event",
            "event": name,
            "body": body,
        })
    }

    /// Next recorded frame at or after `from` whose zero-based line has a
    /// breakpoint.
    fn breakpoint_from(&self, from: usize) -> Option<usize> {
        if self.breakpoints.is_empty() {
            return None;
        }
        (from..self.frames.len()).find(|&i| self.breakpoints.contains(&self.frames[i].line))
    }

    fn load_source(&mut self, args: &Value) -> Result<(), String> {
        if let Some(text) = args.get("source").and_then(Value::as_str) {
            self.source = Some(text.to_string());
            self.source_path = args
                .get("sourcePath")
                .and_then(Value::as_str)
                .map(str::to_string);
            return Ok(());
        }
        if let Some(path) = args.get("program").and_then(Value::as_str) {
            let text = fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
            self.source = Some(text);
            self.source_path = Some(path.to_string());
            return Ok(());
        }
        Err("launch/attach requires 'program' or 'source'".into())
    }

    pub fn handle(&mut self, value: Value) -> Vec<Value> {
        let command = value
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let request_seq = value
            .get("seq")
            .or_else(|| value.get("id"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let args = value.get("arguments").cloned().unwrap_or_else(|| json!({}));

        match command.as_str() {
            "initialize" => vec![self.response(
                request_seq,
                &command,
                json!({
                    "supportsEvaluateForHovers": false,
                    "exceptionBreakpointFilters": [],
                    "supportsStepBack": false,
                }),
            )],
            "launch" | "attach" => match self.load_source(&args) {
                Ok(()) => vec![self.response(request_seq, &command, json!({}))],
                Err(msg) => vec![self.error_response(request_seq, &command, &msg)],
            },
            "setBreakpoints" => {
                let mut verified = Vec::new();
                self.breakpoints.clear();
                if let Some(bps) = args.get("breakpoints").and_then(Value::as_array) {
                    for bp in bps {
                        let line = bp.get("line").and_then(Value::as_u64).unwrap_or(0) as u32;
                        self.breakpoints.insert(line.saturating_sub(1));
                        verified.push(json!({ "verified": true, "line": line }));
                    }
                }
                vec![self.response(request_seq, &command, json!({ "breakpoints": verified }))]
            }
            "setExceptionBreakpoints" | "setFunctionBreakpoints" => {
                vec![self.response(request_seq, &command, json!({}))]
            }
            "configurationDone" => self.configuration_done(request_seq, &command),
            "threads" => vec![self.response(
                request_seq,
                &command,
                json!({ "threads": [{ "id": 1, "name": "main" }] }),
            )],
            "stackTrace" => {
                let stack = match self.frames.get(self.index) {
                    Some(frame) => {
                        let mut source = json!({ "name": "program.sukti" });
                        if let Some(path) = &self.source_path {
                            source["path"] = json!(path);
                        }
                        json!([{
                            "id": 1,
                            "name": format!("frame {}", self.index + 1),
                            "source": source,
                            "line": frame.line + 1,
                            "column": 1,
                            "presentationHint": "normal",
                            "depth": frame.depth,
                        }])
                    }
                    None => json!([]),
                };
                let total = stack.as_array().map_or(0, Vec::len);
                vec![self.response(
                    request_seq,
                    &command,
                    json!({ "stackFrames": stack, "totalFrames": total }),
                )]
            }
            "scopes" => vec![self.response(
                request_seq,
                &command,
                json!({
                    "scopes": [
                        { "name": "Locals", "variablesReference": 1, "expensive": false },
                        { "name": "Heap", "variablesReference": 2, "expensive": false },
                    ]
                }),
            )],
            "variables" => {
                let reference = args
                    .get("variablesReference")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let vars: Vec<Variable> = match reference {
                    1 => self
                        .frames
                        .get(self.index)
                        .map(|f| f.locals.clone())
                        .unwrap_or_default(),
                    2 => self
                        .frames
                        .get(self.index)
                        .map(|f| f.heap.clone())
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                let body: Vec<Value> = vars
                    .into_iter()
                    .map(|v| {
                        json!({
                            "name": v.name,
                            "value": v.value,
                            "type": v.ty,
                            "variablesReference": 0,
                        })
                    })
                    .collect();
                vec![self.response(request_seq, &command, json!(body))]
            }
            "next" | "stepIn" | "stepOut" => {
                let mut out = vec![self.response(request_seq, &command, json!({}))];
                self.index += 1;
                out.push(if self.index >= self.frames.len() {
                    self.event("terminated", json!({ "reason": "normal" }))
                } else {
                    self.event("stopped", json!({ "reason": "step", "threadId": 1 }))
                });
                out
            }
            "continue" => {
                let mut out = vec![self.response(request_seq, &command, json!({}))];
                match self.breakpoint_from(self.index.saturating_add(1)) {
                    Some(stop) => {
                        self.index = stop;
                        out.push(
                            self.event("stopped", json!({ "reason": "breakpoint", "threadId": 1 })),
                        );
                    }
                    None => {
                        self.index = self.frames.len();
                        out.push(self.event("terminated", json!({ "reason": "normal" })));
                    }
                }
                out
            }
            "disconnect" => {
                self.done = true;
                vec![self.response(request_seq, &command, json!({}))]
            }
            _ => vec![self.response(request_seq, &command, json!({}))],
        }
    }

    fn configuration_done(&mut self, request_seq: i64, command: &str) -> Vec<Value> {
        let mut out = vec![self.response(request_seq, command, json!({}))];
        self.run_error = None;
        match self.source.clone() {
            Some(source) => match collect_trace(&source) {
                Ok(frames) => {
                    self.frames = frames;
                    self.index = 0;
                }
                Err(msg) => self.run_error = Some(msg),
            },
            None => self.run_error = Some("no source loaded".into()),
        }

        // Announce initialization so clients may register listeners before the
        // first stopped event (this is the simplified replay handshake).
        out.push(self.event("initialized", json!({})));
        if let Some(msg) = self.run_error.clone() {
            out.push(self.event(
                "output",
                json!({ "category": "stderr", "output": format!("{msg}\n") }),
            ));
            out.push(self.event("terminated", json!({ "reason": "error" })));
        } else if self.frames.is_empty() {
            out.push(self.event("terminated", json!({ "reason": "normal" })));
        } else {
            let stop = self.breakpoint_from(0).unwrap_or(0);
            self.index = stop;
            let reason = if self.breakpoints.contains(&self.frames[stop].line) {
                "breakpoint"
            } else {
                "entry"
            };
            out.push(self.event("stopped", json!({ "reason": reason, "threadId": 1 })));
        }
        out
    }
}

/// Run the DAP server until the client disconnects or closes stdin.
pub fn run_dap() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut server = DapServer::new();
    loop {
        match read_message(&mut reader) {
            ReadOutcome::Message(body) => {
                let Ok(value) = serde_json::from_str::<Value>(&body) else {
                    continue;
                };
                for message in server.handle(value) {
                    if let Err(e) = write_message(&mut writer, &message.to_string()) {
                        eprintln!("dap: write failed: {e}");
                        return;
                    }
                }
                if server.is_done() {
                    break;
                }
            }
            ReadOutcome::Eof => break,
            ReadOutcome::Error(msg) => eprintln!("dap: {msg}"),
        }
    }
}
