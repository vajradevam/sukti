//! Minimal bilingual doc-comment extractor. Lines beginning with `///` are
//! doc comments; a line `/// @sa:` supplies the Sanskrit gloss.

pub fn generate(src: &str) -> String {
    let mut out = String::from("# Sūkti API documentation\n\n");
    let mut current: Vec<String> = Vec::new();
    let mut sa: Option<String> = None;
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("/// @sa:") {
            sa = Some(rest.trim().to_string());
        } else if let Some(rest) = t.strip_prefix("///") {
            current.push(rest.trim().to_string());
        } else {
            if !current.is_empty() {
                out.push_str(&current.join("\n"));
                out.push('\n');
                if let Some(s) = sa.take() {
                    out.push_str(&format!("- saMskRRitam: {s}\n"));
                }
                out.push('\n');
                current.clear();
            }
            if t.starts_with("kriyA ") || t.starts_with("varga ") {
                out.push_str(&format!("### `{}`\n\n", t.trim_end_matches('{').trim()));
            }
        }
    }
    if !current.is_empty() {
        out.push_str(&current.join("\n"));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn extracts() {
        let s = "/// Adds two numbers.\n/// @sa: doni a~nkAni yojayati;\nkriyA yoga(a, ba) { }\n";
        let d = super::generate(s);
        assert!(d.contains("Adds two numbers"));
        assert!(d.contains("doni a~nkAni"));
    }
}
