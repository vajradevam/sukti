//! Minimal, dependency-free `Content-Length` framing shared by the LSP and
//! DAP servers. Both protocols use the same wire format:
//!
//! ```text
//! Content-Length: <byte count>\r\n
//! \r\n
//! <UTF-8 JSON body>
//! ```

use std::io::{self, BufRead, Write};

/// Wrap a JSON body in a `Content-Length` frame. The length is the number of
/// UTF-8 bytes (Devanagari is multi-byte), not the number of characters.
pub fn frame(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
}

/// Extract the JSON body from a framed message. Also accepts a bare JSON body,
/// which makes it convenient for tests and for manual `printf` piping.
pub fn unframe(text: &str) -> Option<&str> {
    let body = if let Some(pos) = text.find("\r\n\r\n") {
        &text[pos + 4..]
    } else if let Some(pos) = text.find("\n\n") {
        &text[pos + 2..]
    } else {
        text
    };
    let body = body.trim_end_matches(['\r', '\n']);
    if body.trim_start().starts_with(['{', '[']) {
        Some(body)
    } else {
        None
    }
}

/// Result of reading one message from a stream.
#[derive(Debug)]
pub enum ReadOutcome {
    /// A complete JSON body.
    Message(String),
    /// The peer closed the stream (clean EOF).
    Eof,
    /// A malformed header was encountered; the caller may log and continue.
    Error(String),
}

/// Read exactly one framed message from `reader`.
///
/// Header lines are consumed until the blank line; the body is then read by
/// its declared byte length. A truncated body at EOF is treated as EOF rather
/// than a panic, so malformed input can never crash the server.
pub fn read_message(reader: &mut impl BufRead) -> ReadOutcome {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => return ReadOutcome::Eof,
            Ok(_) => {}
            Err(e) => return ReadOutcome::Error(format!("read header: {e}")),
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                match value.trim().parse::<usize>() {
                    Ok(n) => length = Some(n),
                    Err(e) => return ReadOutcome::Error(format!("bad Content-Length: {e}")),
                }
            }
        }
    }
    let Some(len) = length else {
        return ReadOutcome::Error("missing Content-Length header".into());
    };
    // Guard against absurd allocations from a hostile peer; real LSP/DAP
    // messages are far below this bound.
    if len > 64 * 1024 * 1024 {
        return ReadOutcome::Error("Content-Length too large".into());
    }
    let mut buf = vec![0u8; len];
    match reader.read_exact(&mut buf) {
        Ok(()) => match String::from_utf8(buf) {
            Ok(s) => ReadOutcome::Message(s),
            Err(e) => ReadOutcome::Error(format!("body is not UTF-8: {e}")),
        },
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => ReadOutcome::Eof,
        Err(e) => ReadOutcome::Error(format!("read body: {e}")),
    }
}

/// Write one framed message to `writer`, flushing so the client sees it now.
pub fn write_message(writer: &mut impl Write, body: &str) -> io::Result<()> {
    writer.write_all(frame(body).as_bytes())?;
    writer.flush()
}
