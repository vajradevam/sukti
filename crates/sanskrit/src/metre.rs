//! Akṣara (syllable) splitting, laghu/guru weights, and basic metre detection.

const VOWEL_SIGNS: &[char] = &['ा', 'ि', 'ी', 'ु', 'ू', 'ृ', 'ॄ', 'ॢ', 'ॣ', 'े', 'ै', 'ो', 'ौ'];
const INDEPENDENT_VOWELS: &[char] = &[
    'अ', 'आ', 'इ', 'ई', 'उ', 'ऊ', 'ऋ', 'ॠ', 'ऌ', 'ॡ', 'ए', 'ऐ', 'ओ', 'औ',
];
const LONG_VOWELS: &[char] = &[
    'ा', 'ई', 'ी', 'ू', 'ृ', 'ॄ', 'ॢ', 'ॣ', 'े', 'ै', 'ो', 'ौ', 'आ', 'ई', 'ऊ', 'ऋ', 'ॠ', 'ऌ', 'ॡ', 'ए',
    'ऐ', 'ओ', 'औ',
];
const JOINERS: &[char] = &['ं', 'ः'];

fn is_consonant(c: char) -> bool {
    ('क'..='ह').contains(&c) || ['ङ', 'ञ', 'ण', 'श', 'ष', 'ळ', '़'].contains(&c)
}

/// Split Devanagari text into akṣaras (syllables).
pub fn akshara_split(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if is_consonant(c) {
            cur.push(c);
            let next = chars.get(i + 1).copied();
            let stays = match next {
                Some('\u{094d}') => true,
                Some(x) if VOWEL_SIGNS.contains(&x) || JOINERS.contains(&x) => true,
                _ => false,
            };
            if !stays {
                out.push(std::mem::take(&mut cur));
            }
        } else if c == '\u{094d}' {
            cur.push(c);
        } else if VOWEL_SIGNS.contains(&c) {
            cur.push(c);
            out.push(std::mem::take(&mut cur));
        } else if INDEPENDENT_VOWELS.contains(&c) {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.push(c.to_string());
        } else if JOINERS.contains(&c) {
            cur.push(c);
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
        i += 1;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Laghu,
    Guru,
}

/// Laghu (light) or guru (heavy) weight of an akṣara.
pub fn weight(a: &str) -> Weight {
    let chars: Vec<char> = a.chars().collect();
    let consonants = chars.iter().filter(|c| is_consonant(**c)).count();
    if chars.iter().any(|c| LONG_VOWELS.contains(c))
        || chars.iter().any(|c| JOINERS.contains(c))
        || consonants >= 2
    {
        Weight::Guru
    } else {
        Weight::Laghu
    }
}

/// A compact laghu/guru notation: `L` for laghu, `G` for guru.
pub fn laghu_guru(s: &str) -> String {
    akshara_split(s)
        .iter()
        .map(|a| if weight(a) == Weight::Laghu { 'L' } else { 'G' })
        .collect()
}

/// Detect a (simplified, Pathyā) Anuṣṭubh verse: four pādas of eight akṣaras
/// with laghu, guru, laghu at positions 5, 6, 7 of each pāda. Returns None if
/// the text does not fit this pattern; this is a teaching-level detector.
pub fn detect_metre(s: &str) -> Option<&'static str> {
    let padas: Vec<&str> = s
        .split(|c: char| ['।', '॥', '\n'].contains(&c))
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .collect();
    if padas.len() != 4 {
        return None;
    }
    for p in padas {
        let a = akshara_split(p);
        if a.len() != 8 {
            return None;
        }
        if weight(&a[4]) != Weight::Laghu
            || weight(&a[5]) != Weight::Guru
            || weight(&a[6]) != Weight::Laghu
        {
            return None;
        }
    }
    Some("अनुष्टुभ्")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aksharas() {
        assert_eq!(akshara_split("रामः"), vec!["रा", "मः"]);
        assert_eq!(akshara_split("नमस्ते"), vec!["न", "म", "स्ते"]);
    }
    #[test]
    fn weights() {
        assert_eq!(weight("रा"), Weight::Guru);
        assert_eq!(weight("म"), Weight::Laghu);
        assert_eq!(weight("स्ते"), Weight::Guru);
    }
}
