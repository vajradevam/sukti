//! Unicode-aware, lexicon-driven lexer with spans and bilingual diagnostics.

use crate::diag::{Diagnostic, Reporter, Source, Span};
use crate::lexicon::Lexicon;
use num_bigint::BigInt;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Keyword(String), // vocabulary id, e.g. "let", "fn", "return"
    Role(String),    // e.g. "role.karta"
    Ident(String),
    Int(BigInt),
    Float(f64),
    StrSeg(String), // one literal segment of a (possibly interpolated) string
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    Range,
    RangeInclusive,
    Danda,
    DoubleDanda,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    StarStar,
    Eq,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Bang,
    AndAnd,
    OrOr,
    Arrow,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Tok,
    pub span: Span,
    pub text: String,
}

enum Mode {
    Normal,
    Str,
    Interp(usize),
}

pub struct Lexer<'a> {
    src: &'a Source,
    lex: &'a Lexicon,
    reporter: &'a mut Reporter,
    chars: Vec<char>,
    pos: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a Source, lex: &'a Lexicon, reporter: &'a mut Reporter) -> Self {
        Self {
            src,
            lex,
            reporter,
            chars: src.text.chars().collect(),
            pos: 0,
            tokens: Vec::new(),
        }
    }

    fn cur(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn peek(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.cur();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }
    fn span_from(&self, start: usize) -> Span {
        Span::new(
            byte_off(&self.chars, start),
            byte_off(&self.chars, self.pos),
        )
    }

    fn err(&mut self, code: &str, start: usize, args: &[(&str, String)]) {
        let rendered = self.lex.message(code, &self.reporter.lang, args);
        self.reporter
            .push(Diagnostic::new(code, self.span_from(start), rendered));
    }

    pub fn tokenize(mut self) -> Vec<Token> {
        let mut mode = Mode::Normal;
        let mut str_buf = String::new();
        let mut str_start = 0usize;
        let mut raw = false;
        let mut triple = false;

        while let Some(c) = self.cur() {
            match mode {
                Mode::Normal => {
                    // whitespace
                    if c.is_whitespace() {
                        self.bump();
                        continue;
                    }
                    // // line comment
                    if c == '/' && self.peek(1) == Some('/') {
                        while self.cur().map_or(false, |x| x != '\n') {
                            self.bump();
                        }
                        continue;
                    }
                    // /* block comment */ (nested)
                    if c == '/' && self.peek(1) == Some('*') {
                        self.bump();
                        self.bump();
                        let mut depth = 1;
                        while let Some(x) = self.bump() {
                            if x == '/' && self.cur() == Some('*') {
                                self.bump();
                                depth += 1;
                            } else if x == '*' && self.cur() == Some('/') {
                                self.bump();
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                        }
                        continue;
                    }
                    // string (raw / triple)
                    raw = c == 'r' && (self.peek(1) == Some('"'));
                    triple = self.peek(1) == Some('"') && self.peek(2) == Some('"');
                    if c == '"' || raw {
                        if raw {
                            self.bump();
                        }
                        let st = self.pos;
                        self.bump(); // first quote
                        if triple {
                            self.bump();
                            self.bump();
                        }
                        str_buf.clear();
                        str_start = st;
                        mode = Mode::Str;
                        continue;
                    }
                    // statement terminator (ITRANS `;`)
                    if c == ';' {
                        self.push(Tok::Danda, ";");
                        self.bump();
                        continue;
                    }
                    // numbers
                    if c.is_ascii_digit() {
                        self.lex_number();
                        continue;
                    }
                    // identifiers / keywords
                    if is_ident_start(c) {
                        self.lex_ident();
                        continue;
                    }
                    // operators and punctuation
                    let start = self.pos;
                    let two: String = [c, self.peek(1).unwrap_or('\0')].iter().collect();
                    let one = match c {
                        '{' => Some(Tok::LBrace),
                        '}' => Some(Tok::RBrace),
                        '(' => Some(Tok::LParen),
                        ')' => Some(Tok::RParen),
                        '[' => Some(Tok::LBracket),
                        ']' => Some(Tok::RBracket),
                        ',' => Some(Tok::Comma),
                        ':' => Some(Tok::Colon),
                        '.' if [self.peek(1), self.peek(2)] == [Some('.'), Some('=')] => {
                            Some(Tok::RangeInclusive)
                        }
                        '.' if self.peek(1) == Some('.') => Some(Tok::Range),
                        '.' => Some(Tok::Dot),
                        ';' => Some(Tok::Danda),
                        '+' => Some(Tok::Plus),
                        '-' if two == "->" => Some(Tok::Arrow),
                        '-' => Some(Tok::Minus),
                        '*' if two == "**" => Some(Tok::StarStar),
                        '*' => Some(Tok::Star),
                        '/' => Some(Tok::Slash),
                        '%' => Some(Tok::Percent),
                        '=' if two == "==" => Some(Tok::EqEq),
                        '=' => Some(Tok::Eq),
                        '!' if two == "!=" => Some(Tok::Ne),
                        '!' => Some(Tok::Bang),
                        '<' if two == "<=" => Some(Tok::Le),
                        '<' => Some(Tok::Lt),
                        '>' if two == ">=" => Some(Tok::Ge),
                        '>' => Some(Tok::Gt),
                        '&' if two == "&&" => Some(Tok::AndAnd),
                        '|' if two == "||" => Some(Tok::OrOr),
                        _ => None,
                    };
                    if let Some(tok) = one {
                        let n = if matches!(tok, Tok::RangeInclusive) {
                            3
                        } else if matches!(
                            tok,
                            Tok::StarStar
                                | Tok::EqEq
                                | Tok::Ne
                                | Tok::Le
                                | Tok::Ge
                                | Tok::AndAnd
                                | Tok::OrOr
                                | Tok::Arrow
                                | Tok::Range
                        ) {
                            2
                        } else {
                            1
                        };
                        for _ in 0..n {
                            self.bump();
                        }
                        let span = self.span_from(start);
                        self.tokens.push(Token {
                            kind: tok,
                            span,
                            text: self.src.text[span.start..span.end].to_string(),
                        });
                        continue;
                    }
                    // invisible / confusable characters
                    if is_confusable(c) {
                        self.err(
                            "E0015",
                            self.pos,
                            &[("a", c.to_string()), ("b", "adRRishyam".to_string())],
                        );
                    } else {
                        self.err("E0001", self.pos, &[("char", c.to_string())]);
                    }
                    self.bump();
                }
                Mode::Str => {
                    if c == '"' {
                        if triple {
                            if self.peek(1) == Some('"') && self.peek(2) == Some('"') {
                                self.emit_seg(&mut str_buf, str_start);
                                self.bump();
                                self.bump();
                                self.bump();
                                mode = Mode::Normal;
                            } else {
                                str_buf.push(c);
                                self.bump();
                            }
                        } else {
                            self.emit_seg(&mut str_buf, str_start);
                            self.bump();
                            mode = Mode::Normal;
                        }
                        continue;
                    }
                    if c == '{' && !raw {
                        self.emit_seg(&mut str_buf, str_start);
                        let st = self.pos;
                        self.bump();
                        let span = self.span_from(st);
                        self.tokens.push(Token {
                            kind: Tok::LBrace,
                            span,
                            text: "{".into(),
                        });
                        mode = Mode::Interp(0);
                        continue;
                    }
                    if c == '\\' && !raw {
                        self.bump();
                        if let Some(e) = self.bump() {
                            str_buf.push(match e {
                                'n' => '\n',
                                't' => '\t',
                                'r' => '\r',
                                '0' => '\0',
                                '\\' => '\\',
                                '"' => '"',
                                '{' => '{',
                                '}' => '}',
                                other => other,
                            });
                        }
                        continue;
                    }
                    str_buf.push(c);
                    self.bump();
                }
                Mode::Interp(depth) => {
                    if c == '{' {
                        let st = self.pos;
                        self.bump();
                        let span = self.span_from(st);
                        self.tokens.push(Token {
                            kind: Tok::LBrace,
                            span,
                            text: "{".into(),
                        });
                        mode = Mode::Interp(depth + 1);
                        continue;
                    }
                    if c == '}' {
                        let st = self.pos;
                        self.bump();
                        let span = self.span_from(st);
                        self.tokens.push(Token {
                            kind: Tok::RBrace,
                            span,
                            text: "}".into(),
                        });
                        if depth == 0 {
                            mode = Mode::Str;
                            str_buf.clear();
                            str_start = self.pos;
                        } else {
                            mode = Mode::Interp(depth - 1);
                        }
                        continue;
                    }
                    // Reuse normal-mode tokenization for everything else.
                    // (Handled here by a small inline delegate.)
                    if c.is_whitespace() {
                        self.bump();
                        continue;
                    }
                    if c.is_ascii_digit() || ('0'..='9').contains(&c) {
                        self.lex_number();
                        continue;
                    }
                    if is_ident_start(c) {
                        self.lex_ident();
                        continue;
                    }
                    let start = self.pos;
                    let two: String = [c, self.peek(1).unwrap_or('\0')].iter().collect();
                    let tok = match c {
                        '(' => Some(Tok::LParen),
                        ')' => Some(Tok::RParen),
                        '[' => Some(Tok::LBracket),
                        ']' => Some(Tok::RBracket),
                        ',' => Some(Tok::Comma),
                        ':' => Some(Tok::Colon),
                        '.' if [self.cur(), self.peek(1), self.peek(2)]
                            == [Some('.'), Some('.'), Some('=')] =>
                        {
                            Some(Tok::RangeInclusive)
                        }
                        '.' if self.peek(1) == Some('.') => Some(Tok::Range),
                        '.' => Some(Tok::Dot),
                        '+' => Some(Tok::Plus),
                        '-' => Some(Tok::Minus),
                        '*' if two == "**" => Some(Tok::StarStar),
                        '*' => Some(Tok::Star),
                        '/' => Some(Tok::Slash),
                        '%' => Some(Tok::Percent),
                        '=' if two == "==" => Some(Tok::EqEq),
                        '=' => Some(Tok::Eq),
                        '!' if two == "!=" => Some(Tok::Ne),
                        '!' => Some(Tok::Bang),
                        '<' if two == "<=" => Some(Tok::Le),
                        '<' => Some(Tok::Lt),
                        '>' if two == ">=" => Some(Tok::Ge),
                        '>' => Some(Tok::Gt),
                        '&' if two == "&&" => Some(Tok::AndAnd),
                        '|' if two == "||" => Some(Tok::OrOr),
                        _ => None,
                    };
                    if let Some(t) = tok {
                        let n = if matches!(t, Tok::RangeInclusive) {
                            3
                        } else if matches!(
                            t,
                            Tok::StarStar
                                | Tok::EqEq
                                | Tok::Ne
                                | Tok::Le
                                | Tok::Ge
                                | Tok::AndAnd
                                | Tok::OrOr
                                | Tok::Range
                        ) {
                            2
                        } else {
                            1
                        };
                        for _ in 0..n {
                            self.bump();
                        }
                        let span = self.span_from(start);
                        self.tokens.push(Token {
                            kind: t,
                            span,
                            text: self.src.text[span.start..span.end].to_string(),
                        });
                        continue;
                    }
                    self.err("E0001", self.pos, &[("char", c.to_string())]);
                    self.bump();
                }
            }
        }
        if !str_buf.is_empty() {
            self.emit_seg(&mut str_buf, str_start);
        }
        self.tokens.push(Token {
            kind: Tok::Eof,
            span: Span::new(self.src.text.len(), self.src.text.len()),
            text: String::new(),
        });
        self.tokens
    }

    fn emit_seg(&mut self, s: &mut String, start: usize) {
        if !s.is_empty() {
            let span = Span::new(
                byte_off(&self.chars, start),
                byte_off(&self.chars, self.pos),
            );
            self.tokens.push(Token {
                kind: Tok::StrSeg(s.clone()),
                span,
                text: String::new(),
            });
            s.clear();
        }
    }

    fn push(&mut self, k: Tok, text: &str) {
        let span = self.span_from(self.pos);
        self.tokens.push(Token {
            kind: k,
            span,
            text: text.to_string(),
        });
    }

    fn lex_number(&mut self) {
        let start = self.pos;
        let mut s = String::new();
        while let Some(c) = self.cur() {
            if c == '_' {
                self.bump();
                continue;
            }
            if c.is_ascii_digit() {
                s.push(c);
                self.bump();
            } else if ('0'..='9').contains(&c) {
                s.push((b'0' + (c as u32 - 0x0966) as u8) as char);
                self.bump();
            } else {
                break;
            }
        }
        // optional decimal fraction
        let mut is_float = false;
        if self.cur() == Some('.')
            && self
                .peek(1)
                .map_or(false, |c| c.is_ascii_digit() || ('0'..='9').contains(&c))
        {
            is_float = true;
            s.push('.');
            self.bump();
            while let Some(c) = self.cur() {
                if c == '_' {
                    self.bump();
                    continue;
                }
                if c.is_ascii_digit() {
                    s.push(c);
                    self.bump();
                } else if ('0'..='9').contains(&c) {
                    s.push((b'0' + (c as u32 - 0x0966) as u8) as char);
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let span = self.span_from(start);
        let kind = if is_float {
            Tok::Float(s.parse().unwrap_or(0.0))
        } else {
            Tok::Int(s.parse::<BigInt>().unwrap_or_else(|_| BigInt::from(0)))
        };
        self.tokens.push(Token {
            kind,
            span,
            text: s,
        });
    }

    fn lex_ident(&mut self) {
        let start = self.pos;
        let mut s = String::new();
        while let Some(c) = self.cur() {
            if is_ident_continue(c) {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        // Trailing ZWJ/ZWNJ is invalid.
        if s.ends_with('\u{200d}') || s.ends_with('\u{200c}') {
            self.err(
                "E0001",
                self.pos.saturating_sub(1),
                &[("char", "ZWJ/ZWNJ".to_string())],
            );
            s.pop();
        }
        let span = self.span_from(start);
        let kind = if let Some(id) = self.lex.id_of(&s) {
            if id.starts_with("role.") {
                Tok::Role(id.to_string())
            } else {
                Tok::Keyword(id.to_string())
            }
        } else {
            Tok::Ident(s.clone())
        };
        self.tokens.push(Token {
            kind,
            span,
            text: s,
        });
    }
}

fn byte_off(chars: &[char], idx: usize) -> usize {
    chars.iter().take(idx).map(|c| c.len_utf8()).sum()
}

/// ITRANS identifiers are plain ASCII: letters, digits and `_`.
fn is_ident_start(c: char) -> bool {
    c == '_' || c == '~' || c.is_ascii_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

fn is_confusable(c: char) -> bool {
    matches!(
        c,
        '\u{200b}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{2060}'
            | '\u{feff}'
            | '\u{00a0}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lex(s: &str) -> Vec<Tok> {
        let src = Source::new("t", s);
        let lex = Lexicon::load();
        let mut rep = Reporter::new("en");
        let toks = Lexer::new(&src, &lex, &mut rep).tokenize();
        assert!(!rep.has_errors(), "lex errors: {}", rep.render(&src));
        toks.into_iter().map(|t| t.kind).collect()
    }
    #[test]
    fn hello() {
        let t = lex("vada(\"namaste, vishva!\");");
        assert!(matches!(t[0], Tok::Keyword(ref x) if x == "print"));
        assert!(matches!(t[1], Tok::LParen));
        assert!(matches!(t[2], Tok::StrSeg(_)));
    }
    #[test]
    fn numerals_and_roles() {
        assert_eq!(lex("120"), vec![Tok::Int(BigInt::from(120)), Tok::Eof]);
        assert!(matches!(lex("kartA")[0], Tok::Role(ref r) if r == "role.karta"));
    }
    #[test]
    fn interpolation() {
        let t = lex("vada(\"namaste {nAma}!\");");
        assert!(t.iter().any(|x| matches!(x, Tok::LBrace)));
        assert!(t.iter().any(|x| matches!(x, Tok::Ident(s) if s == "nAma")));
    }
}
