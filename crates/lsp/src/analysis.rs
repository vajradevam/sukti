//! Shared front-end analysis for both servers plus the UTF-16 position and
//! Devanagari word-boundary helpers required by the LSP spec.

use serde_json::{json, Value};
use sukti::ast::Stmt;
use sukti::diag::{Reporter, Source};
use sukti::lexer::Lexer;
use sukti::lexicon::Lexicon;
use sukti::parser::Parser;
use sukti::resolve;

/// Parse and resolve a Devanagari source string without executing it.
///
/// `Source::new` performs NFC normalization, which is the same canonical form
/// the pipeline uses for Devanagari input (`fmt::to_canonical` is the identity
/// for that script), so spans line up with the original document bytes.
pub struct Parsed {
    pub source: Source,
    pub reporter: Reporter,
    pub stmts: Vec<Stmt>,
}

pub fn parse(name: &str, text: &str) -> Parsed {
    let source = Source::new(name, text);
    let lex = Lexicon::load();
    let mut reporter = Reporter::new("en");
    let toks = Lexer::new(&source, &lex, &mut reporter).tokenize();
    let mut parser = Parser::new(&source, &lex, &mut reporter, toks);
    let stmts = parser.parse_program();
    if reporter.has_errors() {
        return Parsed {
            source,
            reporter,
            stmts,
        };
    }
    let resolved = resolve::resolve(stmts, &lex, &mut reporter);
    Parsed {
        source,
        reporter,
        stmts: resolved.stmts,
    }
}

/// Byte offset in a UTF-8 `Source` to an LSP `Position` (UTF-16 code units).
pub fn byte_to_position(text: &str, off: usize) -> Value {
    let mut off = off.min(text.len());
    while off > 0 && !text.is_char_boundary(off) {
        off -= 1;
    }
    let before = &text[..off];
    let line = before.bytes().filter(|b| *b == b'\n').count() as u32;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let character = text[line_start..off].encode_utf16().count() as u32;
    json!({ "line": line, "character": character })
}

/// Inverse of [`byte_to_position`]: LSP `Position` to a UTF-8 byte offset.
pub fn position_to_byte(text: &str, line: u32, character: u32) -> usize {
    let mut base = 0usize;
    for (idx, segment) in text.split('\n').enumerate() {
        if idx == line as usize {
            let mut units = 0u32;
            for (byte, ch) in segment.char_indices() {
                if units >= character {
                    return base + byte;
                }
                units += ch.len_utf16() as u32;
            }
            return base + segment.len();
        }
        base += segment.len() + 1;
    }
    text.len()
}

/// True for characters that belong to a Sūkti identifier. The entire
/// Devanagari block is included so vowel signs and combining marks travel
/// with their base letter (grapheme-cluster-safe for our purposes).
pub fn is_word_char(c: char) -> bool {
    let u = c as u32;
    (0x0900..=0x097f).contains(&u) // Devanagari (letters, marks, digits)
        || (0x0300..=0x036f).contains(&u) // combining diacritics
        || c.is_ascii_alphanumeric()
        || c == '_'
}

/// Locate the word containing `off`, returning byte offsets and the slice.
pub fn word_at(text: &str, off: usize) -> (usize, usize, &str) {
    let mut start = off.min(text.len());
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = start;
    while start > 0 {
        let prev = text[..start].chars().next_back().unwrap();
        if is_word_char(prev) {
            start -= prev.len_utf8();
        } else {
            break;
        }
    }
    while end < text.len() {
        let next = text[end..].chars().next().unwrap();
        if is_word_char(next) {
            end += next.len_utf8();
        } else {
            break;
        }
    }
    (start, end, &text[start..end])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclKind {
    Function,
    Class,
    Sutra,
}

impl DeclKind {
    /// LSP `SymbolKind`.
    pub fn symbol_kind(self) -> i64 {
        match self {
            Self::Function => 12,
            Self::Class => 5,
            Self::Sutra => 2,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Declaration {
    pub name: String,
    pub kind: DeclKind,
    pub start: usize,
    pub end: usize,
}

/// Word tokens on one line, with byte offsets relative to that line.
fn word_tokens(line: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if !is_word_char(c) {
            continue;
        }
        let mut end = start + c.len_utf8();
        while let Some(&(next, nc)) = chars.peek() {
            if is_word_char(nc) {
                end = next + nc.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        out.push((start, end, &line[start..end]));
    }
    out
}

/// Scan a document for `kriyA` / `vidhi`, `varga` and `sUtra` declarations.
/// A plain text scan is used deliberately: it works even on partially broken
/// documents where the parser cannot produce an AST.
pub fn declarations(text: &str) -> Vec<Declaration> {
    let lex = Lexicon::load();
    let mut keywords: Vec<(String, DeclKind)> = Vec::new();
    for (id, kind) in [
        ("fn", DeclKind::Function),
        ("class", DeclKind::Class),
        ("sutra", DeclKind::Sutra),
    ] {
        if let Some(dev) = lex.canonical(id) {
            keywords.push((dev.to_string(), kind));
        }
        if let Some(entry) = lex.entry(id) {
            for alias in &entry.aliases {
                keywords.push((alias.clone(), kind));
            }
        }
    }

    let mut out = Vec::new();
    let mut line_start = 0usize;
    for line in text.split('\n') {
        let tokens = word_tokens(line);
        for pair in tokens.windows(2) {
            let head = pair[0].2;
            let kind = keywords.iter().find(|(kw, _)| kw == head).map(|(_, k)| *k);
            if let Some(kind) = kind {
                let (s, e, name) = pair[1];
                out.push(Declaration {
                    name: name.to_string(),
                    kind,
                    start: line_start + s,
                    end: line_start + e,
                });
            }
        }
        line_start += line.len() + 1;
    }
    out
}

/// Human-readable token kind for the word covering `[start, end)`.
pub fn token_kind_at(text: &str, start: usize, end: usize) -> String {
    let source = Source::new("hover", text);
    let lex = Lexicon::load();
    let mut reporter = Reporter::new("en");
    let tokens = Lexer::new(&source, &lex, &mut reporter).tokenize();
    for token in tokens {
        if token.span.start < end && token.span.end > start {
            return match &token.kind {
                sukti::lexer::Tok::Keyword(id) => format!("keyword `{id}`"),
                other => format!("{other:?}"),
            };
        }
    }
    "unknown".into()
}
