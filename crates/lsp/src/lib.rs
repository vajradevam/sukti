#![deny(unsafe_code)]
//! Real LSP and DAP servers for Sūkti, speaking `Content-Length` framed
//! JSON-RPC 2.0 over stdio. Both servers are also usable in-process through
//! [`LspServer`] and [`DapServer`], which is how the integration tests drive
//! them without spawning a subprocess.

pub mod analysis;
pub mod dap;
pub mod jsonrpc;
pub mod lsp;

use std::io;

pub use crate::dap::{run_dap, DapServer};
pub use crate::jsonrpc::frame;
use crate::jsonrpc::{read_message, write_message, ReadOutcome};
pub use crate::lsp::{handle_lsp_request_line, LspServer};

/// Run the LSP server until the client sends `exit` or closes stdin.
pub fn run_lsp() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut server = LspServer::new();
    loop {
        match read_message(&mut reader) {
            ReadOutcome::Message(body) => {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
                    continue;
                };
                if let Some(message) = server.handle(value) {
                    if let Err(e) = write_message(&mut writer, &message.to_string()) {
                        eprintln!("lsp: write failed: {e}");
                        return;
                    }
                }
                if server.is_done() {
                    break;
                }
            }
            ReadOutcome::Eof => break,
            ReadOutcome::Error(msg) => eprintln!("lsp: {msg}"),
        }
    }
}
