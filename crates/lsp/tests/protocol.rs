//! End-to-end protocol tests: framing, capabilities, diagnostics and formatting.

use serde_json::{json, Value};
use std::collections::HashMap;
use sukti_lsp::{frame, handle_lsp_request_line, DapServer, LspServer};

fn body_of(framed: &str) -> &str {
    &framed[framed.find("\r\n\r\n").unwrap() + 4..]
}

fn parse_response(framed: &str) -> Value {
    serde_json::from_str(body_of(framed)).expect("response body must be JSON")
}

#[test]
fn framing_roundtrip_and_single_request_helper() {
    let body = r#"{"jsonrpc":"2.0","id":7,"method":"initialize","params":{}}"#;
    let framed = frame(body);
    assert!(framed.starts_with(&format!("Content-Length: {}\r\n\r\n", body.len())));
    assert_eq!(body_of(&framed), body);

    let mut docs = HashMap::new();
    let response = handle_lsp_request_line(&framed, &mut docs).expect("initialize responds");
    let value = parse_response(&response);
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 7);
    assert!(value["result"]["capabilities"].is_object());
}

#[test]
fn initialize_advertises_all_capabilities() {
    let mut server = LspServer::new();
    let response = server
        .handle(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }))
        .expect("initialize responds");
    let caps = &response["result"]["capabilities"];
    assert_eq!(caps["textDocumentSync"], 1);
    assert_eq!(caps["publishDiagnostics"], json!({}));
    for key in [
        "documentFormattingProvider",
        "hoverProvider",
        "definitionProvider",
        "renameProvider",
        "documentSymbolProvider",
    ] {
        assert_eq!(caps[key], true, "missing capability {key}");
    }
    assert!(caps["completionProvider"].is_object());

    // Notifications never produce responses.
    assert!(server
        .handle(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}))
        .is_none());
    server.handle(json!({"jsonrpc": "2.0", "method": "shutdown", "id": 2}));
    assert!(server
        .handle(json!({"jsonrpc": "2.0", "method": "exit"}))
        .is_none());
    assert!(server.is_done());
}

#[test]
fn did_open_publishes_diagnostics_for_broken_program() {
    let mut server = LspServer::new();
    let message = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///broken.sukti",
                "languageId": "sukti",
                "version": 1,
                "text": "yadi ("
            }
        }
    });
    let outgoing = server
        .handle(message)
        .expect("didOpen publishes diagnostics");
    assert_eq!(outgoing["method"], "textDocument/publishDiagnostics");
    assert_eq!(outgoing["params"]["uri"], "file:///broken.sukti");
    let diagnostics = outgoing["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(!diagnostics.is_empty(), "expected at least one diagnostic");
    let first = &diagnostics[0];
    assert_eq!(first["source"], "sukti");
    assert_eq!(first["severity"], 1);
    assert!(first["code"].as_str().unwrap().starts_with('E'));
    assert!(!first["message"].as_str().unwrap().is_empty());
    assert!(first["range"]["start"]["line"].is_number());
}

#[test]
fn formatting_returns_one_full_document_text_edit() {
    let mut server = LspServer::new();
    server.handle(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///ok.sukti",
                "languageId": "sukti",
                "version": 1,
                "text": "astu ga = 1;"
            }
        }
    }));
    let response = server
        .handle(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/formatting",
            "params": {
                "textDocument": { "uri": "file:///ok.sukti" },
                "options": { "tabSize": 4, "insertSpaces": true }
            }
        }))
        .expect("formatting responds");
    let edits = response["result"].as_array().expect("array of edits");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["range"]["start"]["line"], 0);
    assert_eq!(edits[0]["range"]["start"]["character"], 0);
    let new_text = edits[0]["newText"].as_str().unwrap();
    assert!(
        new_text.contains("astu"),
        "formatter dropped the keyword: {new_text}"
    );
}

#[test]
fn hover_completion_definition_rename_symbols() {
    let mut server = LspServer::new();
    let source = "kriyA vadana() { phalam 1; }\nastu ga = 1;\nvadana();";
    server.handle(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///symbols.sukti",
                "languageId": "sukti",
                "version": 1,
                "text": source
            }
        }
    }));

    let hover = server
        .handle(json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": "file:///symbols.sukti" },
                "position": { "line": 0, "character": 2 }
            }
        }))
        .unwrap();
    assert!(hover["result"]["contents"]["value"]
        .as_str()
        .unwrap()
        .contains("function"));

    let completion = server
        .handle(json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "textDocument/completion",
            "params": { "textDocument": { "uri": "file:///symbols.sukti" }, "position": { "line": 0, "character": 2 } }
        }))
        .unwrap();
    assert!(completion["result"].as_array().unwrap().len() > 10);

    let definition = server
        .handle(json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": "file:///symbols.sukti" },
                "position": { "line": 2, "character": 1 }
            }
        }))
        .unwrap();
    assert_eq!(definition["result"]["uri"], "file:///symbols.sukti");
    assert_eq!(definition["result"]["range"]["start"]["line"], 0);

    let rename = server
        .handle(json!({
            "jsonrpc": "2.0",
            "id": 13,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": "file:///symbols.sukti" },
                "position": { "line": 2, "character": 1 },
                "newName": "nayA"
            }
        }))
        .unwrap();
    let edits = rename["result"]["changes"]["file:///symbols.sukti"]
        .as_array()
        .unwrap();
    assert_eq!(edits.len(), 2, "declaration + call should both be renamed");

    let symbols = server
        .handle(json!({
            "jsonrpc": "2.0",
            "id": 14,
            "method": "textDocument/documentSymbol",
            "params": { "textDocument": { "uri": "file:///symbols.sukti" } }
        }))
        .unwrap();
    let symbols = symbols["result"].as_array().unwrap();
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0]["name"], "vadana");
}

#[test]
fn dap_trace_replay_navigates_recorded_frames() {
    let mut server = DapServer::new();
    let source = "astu ga = 1;\nvada(ga);\nastu kha = [1, 2];";

    let init = server.handle(json!({
        "seq": 1, "type": "request", "command": "initialize", "arguments": {}
    }));
    assert_eq!(init[0]["body"]["supportsEvaluateForHovers"], false);
    assert_eq!(init[0]["body"]["exceptionBreakpointFilters"], json!([]));

    let launched = server.handle(json!({
        "seq": 2, "type": "request", "command": "launch",
        "arguments": { "source": source }
    }));
    assert_eq!(launched[0]["success"], true);

    let bps = server.handle(json!({
        "seq": 3, "type": "request", "command": "setBreakpoints",
        "arguments": { "breakpoints": [{ "line": 2 }] }
    }));
    assert_eq!(bps[0]["body"]["breakpoints"][0]["verified"], true);

    let started = server.handle(json!({
        "seq": 4, "type": "request", "command": "configurationDone", "arguments": {}
    }));
    assert_eq!(started[0]["request_seq"], 4);
    assert!(started.iter().any(|m| m["event"] == "initialized"));
    let stopped = started
        .iter()
        .find(|m| m["event"] == "stopped")
        .expect("stopped event");
    assert_eq!(stopped["body"]["reason"], "breakpoint");

    let stack = server.handle(json!({
        "seq": 5, "type": "request", "command": "stackTrace",
        "arguments": { "threadId": 1 }
    }));
    assert_eq!(stack[0]["body"]["stackFrames"][0]["line"], 2);

    let scopes = server.handle(json!({
        "seq": 6, "type": "request", "command": "scopes", "arguments": { "frameId": 1 }
    }));
    assert_eq!(scopes[0]["body"]["scopes"][0]["name"], "Locals");
    assert_eq!(scopes[0]["body"]["scopes"][1]["name"], "Heap");

    let locals = server.handle(json!({
        "seq": 7, "type": "request", "command": "variables",
        "arguments": { "variablesReference": 1 }
    }));
    assert_eq!(locals[0]["body"][0]["name"], "ga");

    let stepped = server.handle(json!({
        "seq": 8, "type": "request", "command": "next", "arguments": { "threadId": 1 }
    }));
    assert_eq!(stepped[1]["body"]["reason"], "step");

    let continued = server.handle(json!({
        "seq": 9, "type": "request", "command": "continue", "arguments": { "threadId": 1 }
    }));
    assert!(continued.iter().any(|m| m["event"] == "terminated"));

    let disconnect = server.handle(json!({
        "seq": 10, "type": "request", "command": "disconnect", "arguments": {}
    }));
    assert_eq!(disconnect[0]["success"], true);
    assert!(server.is_done());
}
