//! Māheśvara-sūtra pratyāhāra expansion.
//!
//! The fourteen sūtras are stored with their it (marker) letters. A
//! pratyāhāra such as अच् or हल् denotes the continuous span from its first
//! letter to its marker, markers excluded. This is a dev-time-verifiable,
//! pure-data implementation (no large linguistic files).

/// (sūtra text, letters, marker character)
const SUTRAS: &[(&str, &str, char)] = &[
    ("अइउण्", "अइउ", 'ण'),
    ("ऋऌक्", "ऋऌ", 'क'),
    ("एओङ्", "एओ", 'ङ'),
    ("ऐऔच्", "ऐऔ", 'च'),
    ("हयवरट्", "हयवर", 'ट'),
    ("लण्", "ल", 'ण'),
    ("ञमङणनम्", "ञमङणन", 'म'),
    ("झभञ्", "झभ", 'ञ'),
    ("घढधष्", "घढध", 'ष'),
    ("जबगडदश्", "जबगडद", 'श'),
    ("खफछठथचटतव्", "खफछठथचटतव", 'व'),
    ("कपय्", "कप", 'य'),
    ("शषसर्", "शषस", 'र'),
    ("हल्", "ह", 'ल'),
];

/// Expand a pratyāhāra (e.g. "अच्", "हल्", "अण्") to its letters.
/// Returns `None` if the first letter or the marker is not found.
pub fn expand(pratyahara: &str) -> Option<Vec<char>> {
    let chars: Vec<char> = pratyahara.chars().collect();
    if chars.len() < 2 {
        return None;
    }
    let first = chars[0];
    // The last *letter* is the marker; ignore a trailing virāma if present.
    let marker = *chars.iter().rev().find(|c| **c != '\u{094d}')?;

    // Build the continuous stream: letters, then their marker (as a position).
    let mut stream: Vec<(char, bool)> = Vec::new(); // (char, is_marker)
    for (_txt, letters, mk) in SUTRAS {
        for c in letters.chars() {
            stream.push((c, false));
        }
        stream.push((*mk, true));
    }
    let start = stream.iter().position(|(c, _)| *c == first)?;
    // The marker that closes this pratyāhāra is the first marker occurrence
    // at or after `start` that matches `marker`.
    let end = stream
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, (c, is_mk))| *is_mk && *c == marker)
        .map(|(i, _)| i)?;
    // Duplicate letters occur in the sūtras (e.g. ह and व); a pratyāhāra is a
    // set of phonemes, so preserve first-occurrence order and deduplicate.
    let mut seen = std::collections::HashSet::new();
    Some(
        stream[start..=end]
            .iter()
            .filter(|(_, m)| !*m)
            .map(|(c, _)| *c)
            .filter(|c| seen.insert(*c))
            .collect(),
    )
}

/// Convenience: expand to a string.
pub fn expand_string(p: &str) -> Option<String> {
    expand(p).map(|v| v.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vowels() {
        assert_eq!(expand_string("अच्").as_deref(), Some("अइउऋऌएओऐऔ"));
    }
    #[test]
    fn consonants() {
        let h = expand("हल्").unwrap();
        assert_eq!(h.len(), 33); // traditional count of Sanskrit consonants
        assert!(h.contains(&'क'));
        assert!(h.contains(&'ह'));
    }
    #[test]
    fn first_sutra() {
        assert_eq!(expand_string("अण्").as_deref(), Some("अइउ"));
    }
}
