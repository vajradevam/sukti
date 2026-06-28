//! The LSP server. Every request is handled in-memory so the whole protocol
//! surface is also unit-testable through [`LspServer::handle`].

use crate::analysis::{
    byte_to_position, declarations, is_word_char, parse, position_to_byte, token_kind_at, word_at,
};
use crate::jsonrpc::{frame, unframe};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

/// Standard-library names offered by `completion`. Mirrors `sukti::stdlib`.
const STDLIB_NAMES: &[&str] = &[
    "vada",
    "paTha",
    "doSha",
    "asti",
    "rUpAntara",
    "gaNita",
    "vargamUla",
    "shakti",
    "nirapekSha",
    "nyUnatama",
    "adhikatama",
    "gola",
    "π",
    "e",
    "saMskRRita",
    "rUpAntaraNa",
    "sandhi",
    "sandhivichchheda",
    "pratyAhAra",
    "akSharANi",
    "mAtrA",
    "chhandaH",
    "vibhakti",
    "vibhaktiparichchheda",
    "su",
    "virAma",
    "chainala",
    "joDa",
    "yAdRRichchhika",
    "bIja",
    "samaya",
    "paThana",
    "lekhana",
    "astitva",
    "vishleShaNa",
    "nirmANa",
    "milAna",
    "parIkShA",
    "sama",
    "satya",
    "viphala",
];

/// A stateful LSP server over a set of open documents.
pub struct LspServer {
    docs: HashMap<String, String>,
    shutdown: bool,
    done: bool,
}

impl Default for LspServer {
    fn default() -> Self {
        Self::new()
    }
}

impl LspServer {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
            shutdown: false,
            done: false,
        }
    }

    pub fn with_docs(docs: HashMap<String, String>) -> Self {
        Self {
            docs,
            shutdown: false,
            done: false,
        }
    }

    pub fn into_docs(self) -> HashMap<String, String> {
        self.docs
    }

    /// True after the `exit` notification was received.
    pub fn is_done(&self) -> bool {
        self.done
    }

    fn text(&self, params: &Value) -> Option<String> {
        let uri = params
            .get("textDocument")
            .and_then(|d| d.get("uri"))
            .and_then(Value::as_str)?;
        self.docs.get(uri).cloned()
    }

    fn uri(params: &Value) -> String {
        params
            .get("textDocument")
            .and_then(|d| d.get("uri"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    }

    /// Handle one LSP message. Returns the single outgoing message, if any.
    /// Requests (with an `id`) produce a response; notifications produce only
    /// server-initiated notifications such as `publishDiagnostics`.
    pub fn handle(&mut self, value: Value) -> Option<Value> {
        let method = value.get("method").and_then(Value::as_str).unwrap_or("");
        let id = value.get("id").cloned();
        let params = value.get("params").cloned().unwrap_or_else(|| json!({}));

        match method {
            "initialize" => self.respond(id, initialize_result()),
            "initialized" => None,
            "shutdown" => {
                self.shutdown = true;
                self.respond(id, Value::Null)
            }
            "exit" => {
                self.done = true;
                None
            }
            "$/cancelRequest" => None,
            "textDocument/didOpen" => {
                let uri = Self::uri(&params);
                let text = params
                    .get("textDocument")
                    .and_then(|d| d.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                self.docs.insert(uri.clone(), text.clone());
                Some(publish_diagnostics(&uri, &text))
            }
            "textDocument/didChange" => {
                let uri = Self::uri(&params);
                let text = params
                    .get("contentChanges")
                    .and_then(Value::as_array)
                    .and_then(|changes| changes.last())
                    .and_then(|c| c.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                self.docs.insert(uri.clone(), text.clone());
                Some(publish_diagnostics(&uri, &text))
            }
            "textDocument/didClose" => {
                let uri = Self::uri(&params);
                self.docs.remove(&uri);
                Some(publish_diagnostics(&uri, ""))
            }
            "textDocument/formatting" => {
                let result = self
                    .text(&params)
                    .map(|text| formatting(&text))
                    .unwrap_or_else(|| json!([]));
                self.respond(id, result)
            }
            "textDocument/hover" => {
                let result = self
                    .text(&params)
                    .map(|text| hover(&text, &params))
                    .unwrap_or(Value::Null);
                self.respond(id, result)
            }
            "textDocument/completion" => self.respond(id, json!(completion())),
            "textDocument/definition" => {
                let result = self
                    .text(&params)
                    .map(|text| definition(&text, &params))
                    .unwrap_or(Value::Null);
                self.respond(id, result)
            }
            "textDocument/rename" => {
                let result = self
                    .text(&params)
                    .map(|text| rename(&text, &params, &Self::uri(&params)))
                    .unwrap_or(Value::Null);
                self.respond(id, result)
            }
            "textDocument/documentSymbol" => {
                let result = self
                    .text(&params)
                    .map(|text| document_symbols(&text))
                    .unwrap_or_default();
                self.respond(id, json!(result))
            }
            _ => {
                // Unknown notifications are ignored. Unknown requests get a
                // null result so the client never hangs waiting for an id.
                if value.get("id").is_some() {
                    self.respond(id, Value::Null)
                } else {
                    None
                }
            }
        }
    }

    fn respond(&self, id: Option<Value>, result: Value) -> Option<Value> {
        id.map(|id| json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
            "textDocumentSync": 1,
            "publishDiagnostics": {},
            "documentFormattingProvider": true,
            "hoverProvider": true,
            "completionProvider": { "triggerCharacters": [" "] },
            "definitionProvider": true,
            "renameProvider": true,
            "documentSymbolProvider": true
        },
        "serverInfo": { "name": "sukti-lsp", "version": "0.1.0" }
    })
}

fn publish_diagnostics(uri: &str, text: &str) -> Value {
    let parsed = parse(uri, text);
    let mut diagnostics = Vec::new();
    for d in &parsed.reporter.diags {
        let mut message = d.rendered.clone();
        if let Some(s) = &d.suggestion {
            message.push_str("\nsuggestion: ");
            message.push_str(s);
        }
        if let Some(n) = &d.note {
            message.push_str("\nnote: ");
            message.push_str(n);
        }
        let severity = if d.code.starts_with('E') { 1 } else { 2 };
        diagnostics.push(json!({
            "range": {
                "start": byte_to_position(&parsed.source.text, d.span.start),
                "end": byte_to_position(&parsed.source.text, d.span.end),
            },
            "severity": severity,
            "code": d.code.clone(),
            "source": "sukti",
            "message": message,
        }));
    }
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": { "uri": uri, "diagnostics": diagnostics }
    })
}

fn formatting(text: &str) -> Value {
    let opts = sukti::pipeline::Options::default();
    match sukti::pipeline::format_source(text, sukti::fmt::View::Itrans, &opts) {
        Ok(formatted) => json!([{
            "range": {
                "start": byte_to_position(text, 0),
                "end": byte_to_position(text, text.len()),
            },
            "newText": formatted,
        }]),
        Err(_) => json!([]),
    }
}

fn position_from_params(params: &Value) -> (u32, u32) {
    let pos = params.get("position").unwrap_or(&Value::Null);
    let line = pos.get("line").and_then(Value::as_u64).unwrap_or(0) as u32;
    let character = pos.get("character").and_then(Value::as_u64).unwrap_or(0) as u32;
    (line, character)
}

fn hover(text: &str, params: &Value) -> Value {
    let (line, character) = position_from_params(params);
    let off = position_to_byte(text, line, character);
    let (start, end, word) = word_at(text, off);
    if word.is_empty() {
        return Value::Null;
    }
    let lex = sukti::lexicon::Lexicon::load();
    let value = match lex.id_of(word).and_then(|id| lex.entry(id)) {
        Some(entry) => format!(
            "**{}** (`{}`)\n\n{}\n\n*{}*",
            entry.canonical, entry.id, entry.gloss_en, entry.gloss_sa
        ),
        None => format!("`{word}` — {}", token_kind_at(text, start, end)),
    };
    json!({
        "contents": { "kind": "markdown", "value": value },
        "range": {
            "start": byte_to_position(text, start),
            "end": byte_to_position(text, end),
        }
    })
}

fn completion() -> Vec<Value> {
    let lex = sukti::lexicon::Lexicon::load();
    let mut seen: HashSet<String> = HashSet::new();
    let mut items = Vec::new();
    for entry in &lex.entries {
        for spelling in std::iter::once(&entry.canonical).chain(entry.aliases.iter()) {
            if seen.insert(spelling.clone()) {
                items.push(json!({
                    "label": spelling,
                    "kind": 14, // Keyword
                    "detail": entry.id,
                    "documentation": entry.gloss_en,
                }));
            }
        }
    }
    for name in STDLIB_NAMES {
        if seen.insert((*name).to_string()) {
            items.push(json!({
                "label": name,
                "kind": 3, // Function
                "detail": "stdlib",
            }));
        }
    }
    items
}

fn definition(text: &str, params: &Value) -> Value {
    let (line, character) = position_from_params(params);
    let (_, _, word) = word_at(text, position_to_byte(text, line, character));
    if word.is_empty() {
        return Value::Null;
    }
    match declarations(text).into_iter().find(|d| d.name == word) {
        Some(d) => json!({
            "uri": LspServer::uri(params),
            "range": {
                "start": byte_to_position(text, d.start),
                "end": byte_to_position(text, d.end),
            }
        }),
        None => Value::Null,
    }
}

fn rename(text: &str, params: &Value, uri: &str) -> Value {
    let new_name = params.get("newName").and_then(Value::as_str).unwrap_or("");
    let (line, character) = position_from_params(params);
    let (_, _, word) = word_at(text, position_to_byte(text, line, character));
    if word.is_empty() || new_name.is_empty() {
        return Value::Null;
    }
    let mut edits = Vec::new();
    let mut i = 0usize;
    while i < text.len() {
        let ch = text[i..].chars().next().unwrap();
        if is_word_char(ch) {
            let (start, end, candidate) = word_at(text, i);
            if candidate == word {
                edits.push(json!({
                    "range": {
                        "start": byte_to_position(text, start),
                        "end": byte_to_position(text, end),
                    },
                    "newText": new_name,
                }));
            }
            i = end;
        } else {
            i += ch.len_utf8();
        }
    }
    let mut changes = Map::new();
    changes.insert(uri.to_string(), json!(edits));
    json!({ "changes": changes })
}

fn document_symbols(text: &str) -> Vec<Value> {
    declarations(text)
        .into_iter()
        .map(|d| {
            let range = json!({
                "start": byte_to_position(text, d.start),
                "end": byte_to_position(text, d.end),
            });
            json!({
                "name": d.name,
                "kind": d.kind.symbol_kind(),
                "range": range,
                "selectionRange": range,
            })
        })
        .collect()
}

/// Convenience helper used by the integration tests and handy for piping a
/// single framed message through the server from a shell.
///
/// Accepts either a `Content-Length` framed message or a bare JSON body and
/// returns a framed response (or `None` for notifications / malformed input).
pub fn handle_lsp_request_line(text: &str, docs: &mut HashMap<String, String>) -> Option<String> {
    let body = unframe(text)?;
    let value: Value = serde_json::from_str(body).ok()?;
    let mut server = LspServer::with_docs(std::mem::take(docs));
    let response = server.handle(value);
    *docs = server.into_docs();
    response.map(|v| frame(&v.to_string()))
}
