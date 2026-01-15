//! A small, clearly-labelled *educational* subset of sandhi.
//!
//! Sandhi split is inherently ambiguous: a string may have several valid
//! analyses. `split` therefore returns candidates; `join` returns candidates
//! too (primary first). Nothing here is used to segment source tokens — Sūkti
//! tokens are always delimited by whitespace/punctuation and never sandhied.

/// Join two words, returning candidate joined forms (primary first).
pub fn join(a: &str, b: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut primary = a.to_string();
    let bc = b.chars().next();

    // Simple vowel sandhi (dīrgha) for the common अ/आ and इ/ई and उ/ू pairs.
    let last = primary.chars().last();
    if let (Some(x), Some(y)) = (last, bc) {
        let merged = match (x, y) {
            ('अ' | 'आ', 'अ' | 'आ') => Some('आ'),
            ('इ' | 'ई', 'इ' | 'ई') => Some('ई'),
            ('उ' | 'ऊ', 'उ' | 'ऊ') => Some('ऊ'),
            ('ऋ' | 'ॠ', 'ऋ' | 'ॠ') => Some('ॠ'),
            _ => None,
        };
        if let Some(m) = merged {
            primary.pop();
            primary.push(m);
            let b_rest: String = b.chars().skip(1).collect();
            out.push(primary + &b_rest);
            return out;
        }
    }

    // Visarga before a vowel: documented simplification — we keep the visarga
    // as a candidate and also offer the common "ोऽ" form before अ/आ.
    if a.ends_with('ः') {
        let rest: String = a.chars().take(a.chars().count() - 1).collect();
        match bc {
            Some('अ' | 'आ') => out.push(format!("{rest}ोऽ{b}")),
            Some('इ' | 'ई' | 'उ' | 'ऊ' | 'ए' | 'ओ') => out.push(format!("{rest}ो{b}")),
            _ => {}
        }
    }
    out.push(format!("{a}{b}"));
    out
}

/// Candidate ways to split a joined string at every position whose boundary
/// matches a known sandhi rule. Returns empty if no rule-based split exists.
pub fn split(s: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    for i in 1..chars.len() {
        let a: String = chars[..i].iter().collect();
        let b: String = chars[i..].iter().collect();
        let last = a.chars().last().unwrap();
        let first = b.chars().next().unwrap();
        // Reverse of dīrgha sandhi: a long vowel may stand for two shorts.
        let candidates: Option<&str> = match (last, first) {
            ('आ', _) => Some("अ"),
            ('ई', _) => Some("इ"),
            ('ऊ', _) => Some("उ"),
            ('ॠ', _) => Some("ऋ"),
            ('ो', 'ऽ') => Some("ः"),
            _ => None,
        };
        if let Some(short) = candidates {
            let mut left = a.clone();
            left.pop();
            left.push_str(short);
            out.push((left, b.clone()));
        }
        // A visarga boundary is always a plausible analysis.
        if first == 'ः' || last == 'ः' {
            out.push((a.clone(), b.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vowel_sandhi() {
        assert_eq!(join("अ", "अ"), vec!["आ"]);
        assert_eq!(join("अ", "अस्ति").first().unwrap(), "आस्ति");
        assert_eq!(join("राम", "अस्ति").first().unwrap(), "रामअस्ति");
    }
    #[test]
    fn split_is_candidates() {
        let c = split("आस्ति");
        assert!(c.iter().any(|(a, b)| a == "अ" && b == "स्ति"));
        assert!(split("कख").is_empty()); // no rule applies to a plain cluster
    }
}
