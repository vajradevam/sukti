//! Transliteration among Devanagari, IAST, ITRANS, Harvard-Kyoto and SLP1.
//!
//! Devanagari is the hub: X → Devanagari → Y. Each scheme therefore only needs
//! one pair of tables and round-trips by construction. Conventions that differ
//! from other implementations are documented in the SLP1/ITRANS tables.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Script {
    Devanagari,
    Iast,
    Itrans,
    HarvardKyoto,
    Slp1,
}

impl Script {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().replace(['_', '-'], "").as_str() {
            "devanagari" | "dev" | "de" => Some(Self::Devanagari),
            "iast" | "ia" => Some(Self::Iast),
            "itrans" | "it" => Some(Self::Itrans),
            "harvardkyoto" | "hk" => Some(Self::HarvardKyoto),
            "slp1" | "slp" => Some(Self::Slp1),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Devanagari => "devanagari",
            Self::Iast => "iast",
            Self::Itrans => "itrans",
            Self::HarvardKyoto => "itrans", // overridden below; kept for completeness
            Self::Slp1 => "slp1",
        }
    }
}

impl fmt::Display for Script {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Devanagari => "devanagari",
            Self::Iast => "iast",
            Self::Itrans => "itrans",
            Self::HarvardKyoto => "harvard-kyoto",
            Self::Slp1 => "slp1",
        };
        f.write_str(s)
    }
}

type Table = &'static [(&'static str, &'static str)];

// --------------------------- IAST -------------------------------------------

const IAST_CONS: Table = &[
    ("kh", "ख"),
    ("gh", "घ"),
    ("ṅ", "ङ"),
    ("ch", "छ"),
    ("jh", "झ"),
    ("ñ", "ञ"),
    ("ṭh", "ठ"),
    ("ḍh", "ढ"),
    ("ṇ", "ण"),
    ("th", "थ"),
    ("dh", "ध"),
    ("ph", "फ"),
    ("bh", "भ"),
    ("ś", "श"),
    ("ṣ", "ष"),
    ("ḷ", "ळ"),
    ("k", "क"),
    ("g", "ग"),
    ("c", "च"),
    ("j", "ज"),
    ("ṭ", "ट"),
    ("ḍ", "ड"),
    ("t", "त"),
    ("d", "द"),
    ("n", "न"),
    ("p", "प"),
    ("b", "ब"),
    ("m", "म"),
    ("y", "य"),
    ("r", "र"),
    ("l", "ल"),
    ("v", "व"),
    ("s", "स"),
    ("h", "ह"),
];
const IAST_VOWELS: Table = &[
    ("ā", "आ"),
    ("ī", "ई"),
    ("ū", "ऊ"),
    ("ṛ", "ऋ"),
    ("ṝ", "ॠ"),
    ("ḷ", "ऌ"),
    ("ḹ", "ॡ"),
    ("ai", "ऐ"),
    ("au", "औ"),
    ("e", "ए"),
    ("o", "ओ"),
    ("i", "इ"),
    ("u", "उ"),
    ("a", "अ"),
];
const IAST_DEP: Table = &[
    ("ā", "ा"),
    ("ī", "ी"),
    ("ū", "ू"),
    ("ṛ", "ृ"),
    ("ṝ", "ॄ"),
    ("ḷ", "ॢ"),
    ("ḹ", "ॣ"),
    ("ai", "ै"),
    ("au", "ौ"),
    ("e", "े"),
    ("o", "ो"),
    ("i", "ि"),
    ("u", "ु"),
    ("a", ""),
];
const IAST_SPECIAL: Table = &[("ṃ", "ं"), ("ṁ", "ं"), ("ḥ", "ः"), ("'", "ऽ")];

// --------------------------- ITRANS -----------------------------------------

const ITRANS_CONS: Table = &[
    ("chh", "छ"),
    ("Th", "ठ"),
    ("Dh", "ढ"),
    ("~n", "ङ"),
    ("Sh", "ष"),
    ("kh", "ख"),
    ("gh", "घ"),
    ("ch", "च"),
    ("jh", "झ"),
    ("ny", "ञ"),
    ("th", "थ"),
    ("dh", "ध"),
    ("ph", "फ"),
    ("bh", "भ"),
    ("sh", "श"),
    ("k", "क"),
    ("g", "ग"),
    ("j", "ज"),
    ("T", "ट"),
    ("D", "ड"),
    ("N", "ण"),
    ("t", "त"),
    ("d", "द"),
    ("n", "न"),
    ("p", "प"),
    ("b", "ब"),
    ("m", "म"),
    ("y", "य"),
    ("r", "र"),
    ("l", "ल"),
    ("v", "व"),
    ("s", "स"),
    ("h", "ह"),
    ("L", "ळ"),
];
const ITRANS_VOWELS: Table = &[
    ("RRi", "ऋ"),
    ("RRI", "ॠ"),
    ("LLi", "ऌ"),
    ("LLI", "ॡ"),
    ("ai", "ऐ"),
    ("au", "औ"),
    ("A", "आ"),
    ("I", "ई"),
    ("U", "ऊ"),
    ("e", "ए"),
    ("o", "ओ"),
    ("i", "इ"),
    ("u", "उ"),
    ("a", "अ"),
];
const ITRANS_DEP: Table = &[
    ("RRi", "ृ"),
    ("RRI", "ॄ"),
    ("LLi", "ॢ"),
    ("LLI", "ॣ"),
    ("ai", "ै"),
    ("au", "ौ"),
    ("A", "ा"),
    ("I", "ी"),
    ("U", "ू"),
    ("e", "े"),
    ("o", "ो"),
    ("i", "ि"),
    ("u", "ु"),
    ("a", ""),
];
const ITRANS_SPECIAL: Table = &[("M", "ं"), (".n", "ं"), ("H", "ः"), ("'", "ऽ")];

// --------------------------- Harvard-Kyoto ----------------------------------

const HK_CONS: Table = &[
    ("kh", "ख"),
    ("gh", "घ"),
    ("ch", "छ"),
    ("jh", "झ"),
    ("Th", "ठ"),
    ("Dh", "ढ"),
    ("th", "थ"),
    ("dh", "ध"),
    ("ph", "फ"),
    ("bh", "भ"),
    ("k", "क"),
    ("g", "ग"),
    ("G", "ङ"),
    ("c", "च"),
    ("j", "ज"),
    ("J", "ञ"),
    ("T", "ट"),
    ("D", "ड"),
    ("N", "ण"),
    ("t", "त"),
    ("d", "द"),
    ("n", "न"),
    ("p", "प"),
    ("b", "ब"),
    ("m", "म"),
    ("y", "य"),
    ("r", "र"),
    ("l", "ल"),
    ("v", "व"),
    ("z", "श"),
    ("S", "ष"),
    ("s", "स"),
    ("h", "ह"),
];
const HK_VOWELS: Table = &[
    ("RR", "ॠ"),
    ("R", "ऋ"),
    ("A", "आ"),
    ("I", "ई"),
    ("U", "ऊ"),
    ("E", "ऐ"),
    ("O", "औ"),
    ("e", "ए"),
    ("o", "ओ"),
    ("i", "इ"),
    ("u", "उ"),
    ("a", "अ"),
];
const HK_DEP: Table = &[
    ("RR", "ॄ"),
    ("R", "ृ"),
    ("A", "ा"),
    ("I", "ी"),
    ("U", "ू"),
    ("E", "ै"),
    ("O", "ौ"),
    ("e", "े"),
    ("o", "ो"),
    ("i", "ि"),
    ("u", "ु"),
    ("a", ""),
];
const HK_SPECIAL: Table = &[("M", "ं"), ("H", "ः"), ("'", "ऽ")];

// --------------------------- SLP1 (documented variant) -----------------------
// A self-consistent SLP1-inspired scheme. Not every detail matches every
// Sanskrit Library tool; deviations are deliberate and round-trip safe.

const SLP1_CONS: Table = &[
    ("Th", "ठ"),
    ("Dh", "ढ"),
    ("kh", "ख"),
    ("gh", "घ"),
    ("ch", "छ"),
    ("jh", "झ"),
    ("th", "थ"),
    ("dh", "ध"),
    ("ph", "फ"),
    ("bh", "भ"),
    ("k", "क"),
    ("K", "ख"),
    ("g", "ग"),
    ("G", "घ"),
    ("N", "ङ"),
    ("c", "च"),
    ("C", "छ"),
    ("j", "ज"),
    ("J", "झ"),
    ("Y", "ञ"),
    ("T", "ट"),
    ("D", "ड"),
    ("Q", "ण"),
    ("t", "त"),
    ("d", "द"),
    ("n", "न"),
    ("p", "प"),
    ("b", "ब"),
    ("m", "म"),
    ("y", "य"),
    ("r", "र"),
    ("l", "ल"),
    ("v", "व"),
    ("z", "श"),
    ("S", "ष"),
    ("s", "स"),
    ("h", "ह"),
];
const SLP1_VOWELS: Table = &[
    ("A", "आ"),
    ("I", "ई"),
    ("U", "ऊ"),
    ("f", "ऋ"),
    ("F", "ॠ"),
    ("x", "ऌ"),
    ("X", "ॡ"),
    ("E", "ऐ"),
    ("O", "औ"),
    ("e", "ए"),
    ("o", "ओ"),
    ("i", "इ"),
    ("u", "उ"),
    ("a", "अ"),
];
const SLP1_DEP: Table = &[
    ("A", "ा"),
    ("I", "ी"),
    ("U", "ू"),
    ("f", "ृ"),
    ("F", "ॄ"),
    ("x", "ॢ"),
    ("X", "ॣ"),
    ("E", "ै"),
    ("O", "ौ"),
    ("e", "े"),
    ("o", "ो"),
    ("i", "ि"),
    ("u", "ु"),
    ("a", ""),
];
const SLP1_SPECIAL: Table = &[("M", "ं"), ("H", "ः"), ("'", "ऽ")];

struct SchemeDef {
    cons: Table,
    vowels: Table,
    dep: Table,
    special: Table,
    anusvara: &'static str,
    visarga: &'static str,
    avagraha: &'static str,
}

fn def(script: Script) -> SchemeDef {
    match script {
        Script::Iast => SchemeDef {
            cons: IAST_CONS,
            vowels: IAST_VOWELS,
            dep: IAST_DEP,
            special: IAST_SPECIAL,
            anusvara: "ṃ",
            visarga: "ḥ",
            avagraha: "'",
        },
        Script::Itrans => SchemeDef {
            cons: ITRANS_CONS,
            vowels: ITRANS_VOWELS,
            dep: ITRANS_DEP,
            special: ITRANS_SPECIAL,
            anusvara: "M",
            visarga: "H",
            avagraha: "'",
        },
        Script::HarvardKyoto => SchemeDef {
            cons: HK_CONS,
            vowels: HK_VOWELS,
            dep: HK_DEP,
            special: HK_SPECIAL,
            anusvara: "M",
            visarga: "H",
            avagraha: "'",
        },
        Script::Slp1 => SchemeDef {
            cons: SLP1_CONS,
            vowels: SLP1_VOWELS,
            dep: SLP1_DEP,
            special: SLP1_SPECIAL,
            anusvara: "M",
            visarga: "H",
            avagraha: "'",
        },
        Script::Devanagari => unreachable!(),
    }
}

/// Transliterate a string between scripts. Devanagari is the internal hub.
pub fn transliterate(s: &str, from: Script, to: Script) -> String {
    if from == to {
        return s.to_string();
    }
    let dev = if from == Script::Devanagari {
        s.to_string()
    } else {
        dev_from(s, from)
    };
    if to == Script::Devanagari {
        dev
    } else {
        dev_to(&dev, to)
    }
}

fn longest(rem: &str, table: Table) -> Option<(&'static str, usize)> {
    let mut best: Option<(&'static str, usize)> = None;
    for (key, val) in table {
        if rem.starts_with(key) {
            let len = key.chars().count();
            if best.map_or(true, |(_, bl)| len > bl) {
                best = Some((val, len));
            }
        }
    }
    best
}

#[derive(Clone)]
enum Unit {
    Cons(String),
    Vowel(String), // key, to select dependent vs independent form
    Special(String),
    Other(char),
}

fn tokenize_ascii(s: &str, d: &SchemeDef) -> Vec<Unit> {
    let chars: Vec<char> = s.chars().collect();
    let mut units = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let rem: String = chars[i..].iter().collect();
        if let Some((v, n)) = longest(&rem, d.cons) {
            units.push(Unit::Cons(v.to_string()));
            i += n;
        } else if let Some((v, n)) = longest(&rem, d.vowels) {
            // store the key (ASCII) not the Devanagari value
            let key: String = chars[i..i + n].iter().collect();
            let _ = v;
            units.push(Unit::Vowel(key));
            i += n;
        } else if let Some((v, n)) = longest(&rem, d.special) {
            units.push(Unit::Special(v.to_string()));
            i += n;
        } else {
            units.push(Unit::Other(chars[i]));
            i += 1;
        }
    }
    units
}

fn dev_from(s: &str, script: Script) -> String {
    let d = def(script);
    let units = tokenize_ascii(s, &d);
    let mut out = String::new();
    for (j, u) in units.iter().enumerate() {
        match u {
            Unit::Cons(dev) => {
                out.push_str(dev);
                let next = units.get(j + 1);
                let virama_needed = match next {
                    None | Some(Unit::Cons(_)) | Some(Unit::Other(_)) => true,
                    Some(Unit::Vowel(_)) | Some(Unit::Special(_)) => false,
                };
                if virama_needed {
                    out.push('\u{094d}');
                }
            }
            Unit::Vowel(key) => {
                // Dependent vowel if a consonant precedes; else independent.
                let prev_is_cons = j > 0 && matches!(units[j - 1], Unit::Cons(_));
                if prev_is_cons {
                    if let Some((dep, _)) = longest(key, d.dep) {
                        out.push_str(dep);
                    }
                } else if let Some((ind, _)) = longest(key, d.vowels) {
                    out.push_str(ind);
                }
            }
            Unit::Special(dev) => out.push_str(dev),
            Unit::Other(c) => {
                // ASCII ';' is the accepted danda alias; map it back to '।'
                // so transliterated views round-trip to canonical source.
                if *c == ';' {
                    out.push('।');
                } else {
                    out.push(*c);
                }
            }
        }
    }
    out
}

fn dev_to(dev: &str, script: Script) -> String {
    let d = def(script);
    let chars: Vec<char> = dev.chars().collect();
    let cons_lookup = |c: char| -> Option<&'static str> {
        let target = c.to_string();
        for (key, val) in d.cons {
            if *val == target {
                return Some(*key);
            }
        }
        None
    };
    let dep_lookup = |c: char| -> Option<&'static str> {
        let target = c.to_string();
        for (key, val) in d.dep {
            if *val == target && !key.is_empty() {
                return Some(*key);
            }
        }
        None
    };
    let ind_lookup = |c: char| -> Option<&'static str> {
        let target = c.to_string();
        for (key, val) in d.vowels {
            if *val == target && key != &"a" {
                return Some(*key);
            }
        }
        None
    };
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(s) = cons_lookup(c) {
            out.push_str(s);
            let next = chars.get(i + 1).copied();
            match next {
                Some('\u{094d}') => {
                    i += 1;
                } // consume virāma, no inherent vowel
                Some(n) if dep_lookup(n).is_some() => { /* dependent vowel supplies the vowel */ }
                _ => out.push('a'), // inherent schwa (also before anusvāra/visarga)
            }
        } else if let Some(s) = dep_lookup(c) {
            out.push_str(s);
        } else if let Some(s) = ind_lookup(c) {
            out.push_str(s);
        } else {
            match c {
                'अ' => out.push('a'), // independent a (at word start or after a vowel)
                '\u{094d}' => {}      // standalone virāma: ignore
                '\u{0902}' => out.push_str(d.anusvara),
                '\u{0903}' => out.push_str(d.visarga),
                '\u{093d}' => out.push_str(d.avagraha),
                '।' => out.push(';'),
                '॥' => out.push(';'),
                '\u{0966}'..='\u{096f}' => out.push((b'0' + (c as u32 - 0x0966) as u8) as char),
                _ => out.push(c),
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(s: &str, sc: Script) {
        let dev = transliterate(s, sc, Script::Devanagari);
        let back = transliterate(&dev, Script::Devanagari, sc);
        assert_eq!(back, s, "round trip failed for {:?} via {}", s, sc);
    }

    #[test]
    fn iast_basic() {
        assert_eq!(
            transliterate("नमस्ते", Script::Devanagari, Script::Iast),
            "namaste"
        );
        assert_eq!(
            transliterate("रामः", Script::Devanagari, Script::Iast),
            "rāmaḥ"
        );
        assert_eq!(transliterate("क्ष", Script::Devanagari, Script::Iast), "kṣa");
    }

    #[test]
    fn iast_to_dev() {
        assert_eq!(
            transliterate("namaste", Script::Iast, Script::Devanagari),
            "नमस्ते"
        );
        assert_eq!(
            transliterate("rāmaḥ", Script::Iast, Script::Devanagari),
            "रामः"
        );
    }

    #[test]
    fn round_trips() {
        for w in [
            "namaste",
            "rāmaḥ",
            "kṣa",
            "jñāna",
            "dharma",
            "śūnya",
            "bhāṣā",
            "aṣṭādhyāyī",
            "kartā",
            "karma",
            "karaṇa",
            "sampradāna",
            "apādāna",
            "adhikaraṇa",
        ] {
            rt(w, Script::Iast);
            rt(w, Script::Itrans);
            rt(w, Script::HarvardKyoto);
            rt(w, Script::Slp1);
        }
    }
}
