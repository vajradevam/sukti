//! Source spans, Unicode-correct columns and bilingual diagnostics.

use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize, // byte offset
    pub end: usize,   // byte offset, exclusive
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    pub fn to(&self, other: Span) -> Self {
        Self {
            start: self.start,
            end: other.end,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Source {
    pub name: String,
    pub text: String,
}

impl Source {
    /// NFC-normalize the input once; all later offsets refer to this text.
    pub fn new(name: impl Into<String>, text: &str) -> Self {
        Self {
            name: name.into(),
            text: text.nfc().collect(),
        }
    }

    /// (line, grapheme-column) of a byte offset, both 1-based for the user.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let off = offset.min(self.text.len());
        let before = &self.text[..off];
        let line = before.chars().filter(|c| *c == '\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        let line_text = &self.text[line_start..off];
        let col = line_text.graphemes(true).count() + 1;
        (line, col)
    }

    pub fn line_text(&self, line: usize) -> &str {
        self.text.lines().nth(line.saturating_sub(1)).unwrap_or("")
    }

    /// A source snippet with a caret under the offending grapheme cluster.
    pub fn snippet(&self, span: Span) -> String {
        let (line, _col) = self.line_col(span.start);
        let lt = self.line_text(line);
        let (_, col_start) = self.line_col(span.start);
        let (_, col_end) = self.line_col(span.end);
        let width = col_end.saturating_sub(col_start).max(1);
        let mut caret = String::new();
        // Columns are grapheme counts; prefix width in display cells is close
        // enough for Devanagari (one cell per cluster in monospace terminals).
        for _ in 1..col_start {
            caret.push(' ');
        }
        for _ in 0..width {
            caret.push('^');
        }
        format!("{:>4} | {}\n     | {}", line, lt, caret)
    }
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub code: String,
    pub span: Span,
    pub rendered: String, // already in the chosen language
    pub suggestion: Option<String>,
    pub note: Option<String>,
}

impl Diagnostic {
    pub fn new(code: &str, span: Span, rendered: String) -> Self {
        Self {
            code: code.to_string(),
            span,
            rendered,
            suggestion: None,
            note: None,
        }
    }
    pub fn with_suggestion(mut self, s: impl Into<String>) -> Self {
        self.suggestion = Some(s.into());
        self
    }
    pub fn with_note(mut self, s: impl Into<String>) -> Self {
        self.note = Some(s.into());
        self
    }
}

/// Collects diagnostics so a run can report several parse errors at once.
#[derive(Clone, Debug, Default)]
pub struct Reporter {
    pub diags: Vec<Diagnostic>,
    pub lang: String,
}

impl Reporter {
    pub fn new(lang: &str) -> Self {
        Self {
            diags: Vec::new(),
            lang: lang.to_string(),
        }
    }
    pub fn push(&mut self, d: Diagnostic) {
        self.diags.push(d);
    }
    pub fn has_errors(&self) -> bool {
        !self.diags.is_empty()
    }
    pub fn render(&self, src: &Source) -> String {
        let mut out = String::new();
        for d in &self.diags {
            let (line, col) = src.line_col(d.span.start);
            out.push_str(&format!(
                "{}:{}:{}: {}: {}\n",
                src.name, line, col, d.code, d.rendered
            ));
            out.push_str(&src.snippet(d.span));
            out.push('\n');
            if let Some(s) = &d.suggestion {
                out.push_str(&format!("  = sujhAva / suggestion: {s}\n"));
            }
            if let Some(n) = &d.note {
                out.push_str(&format!("  = TippaNI / note: {n}\n"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nfc_and_columns() {
        let s = Source::new("t.sukti", "vada(\"namaste\");\nkSha = 1;");
        assert_eq!(s.line_col(0), (1, 1));
        let off = s.text.find("kSha").unwrap();
        assert_eq!(s.line_col(off), (2, 1));
    }
    #[test]
    fn snippet_has_caret() {
        let s = Source::new("t", "abc(def)");
        let sn = s.snippet(Span::new(4, 5));
        assert!(sn.contains('^'));
    }
}
