//! Data-driven lexicon: keywords, kāraka roles, type names and trilingual
//! diagnostics are loaded from `lexicon/lexicon.toml`. No compiler change is
//! needed to correct an entry or add an alias.

use serde::Deserialize;
use std::collections::HashMap;

const TOML: &str = include_str!("../../../lexicon/lexicon.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Meta {
    pub name: String,
    pub name_itrans: String,
    pub cli: String,
    pub version: String,
    pub script: String,
    pub file_extensions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub id: String,
    pub canonical: String,
    pub iast: String,
    pub kind: String,
    pub pos: String,
    pub root: String,
    pub gloss_en: String,
    pub gloss_sa: String,
    pub aliases: Vec<String>,
    pub status: String,
    pub rationale: String,
    pub alternatives: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub code: String,
    pub en: String,
    pub sa: String,
    pub hi: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Raw {
    meta: Meta,
    entry: Vec<Entry>,
    message: Vec<Message>,
}

pub struct Lexicon {
    pub meta: Meta,
    pub entries: Vec<Entry>,
    messages: HashMap<String, Message>,
    by_canonical: HashMap<String, String>,
    by_alias: HashMap<String, String>,
}

impl Lexicon {
    pub fn load() -> Self {
        let raw: Raw = toml::from_str(TOML).expect("lexicon/lexicon.toml must be valid TOML");
        let mut by_canonical = HashMap::new();
        let mut by_alias = HashMap::new();
        for e in &raw.entry {
            by_canonical.insert(e.canonical.clone(), e.id.clone());
            for a in &e.aliases {
                by_alias.insert(a.clone(), e.id.clone());
            }
        }
        let messages = raw
            .message
            .into_iter()
            .map(|m| (m.code.clone(), m))
            .collect();
        Self {
            meta: raw.meta,
            entries: raw.entry,
            messages,
            by_canonical,
            by_alias,
        }
    }

    /// Resolve an ITRANS spelling (canonical or alias) to a vocabulary id.
    pub fn id_of(&self, s: &str) -> Option<&str> {
        self.by_canonical
            .get(s)
            .or_else(|| self.by_alias.get(s))
            .map(String::as_str)
    }
    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }
    pub fn canonical(&self, id: &str) -> Option<&str> {
        self.entry(id).map(|e| e.canonical.as_str())
    }

    /// Format a diagnostic message in the requested language, substituting
    /// `{placeholders}` from `args`.
    pub fn message(&self, code: &str, lang: &str, args: &[(&str, String)]) -> String {
        let Some(m) = self.messages.get(code) else {
            return format!("[{code}] unknown diagnostic");
        };
        let mut t = match lang {
            "sa" => &m.sa,
            "hi" => &m.hi,
            _ => &m.en,
        }
        .clone();
        for (k, v) in args {
            t = t.replace(&format!("{{{k}}}"), v);
        }
        t
    }

    pub fn all_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.id.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loads_and_resolves() {
        let l = Lexicon::load();
        assert_eq!(l.meta.name, "Sūkti");
        assert_eq!(l.id_of("astu"), Some("let"));
        assert_eq!(l.id_of("bhavatu"), Some("let")); // alias
        assert_eq!(l.id_of("kartA"), Some("role.karta"));
    }
    #[test]
    fn messages_are_trilingual() {
        let l = Lexicon::load();
        let a = [("role", "kartA".to_string())];
        assert!(l.message("E0003", "en", &a).contains("duplicate"));
        assert!(!l.message("E0003", "sa", &a).is_empty());
        assert!(!l.message("E0003", "hi", &a).is_empty());
    }
}
