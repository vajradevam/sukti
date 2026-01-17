//! Declension tables for a documented subset: masculine/neuter a-stems and
//! feminine ā-stems, singular and plural. Generated from explicit endings;
//! cross-checked against Vidyut in the dev-time tool (not at runtime).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Case {
    Nom,
    Acc,
    Ins,
    Dat,
    Abl,
    Gen,
    Loc,
    Voc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Number {
    Sg,
    Pl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gender {
    Masc,
    Neut,
    Fem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StemClass {
    A,
    Aa,
}

/// Decline a stem. For class A pass the stem without its final अ (e.g. राम).
/// For class Aa pass the stem without its final आ (e.g. सेन).
pub fn decline(stem: &str, class: StemClass, gender: Gender, number: Number, case: Case) -> String {
    let ending = match (class, gender, number, case) {
        (StemClass::A, Gender::Masc, Number::Sg, Case::Nom) => "ः",
        (StemClass::A, Gender::Masc, Number::Sg, Case::Acc) => "म्",
        (StemClass::A, Gender::Masc, Number::Sg, Case::Ins) => "ेण",
        (StemClass::A, Gender::Masc, Number::Sg, Case::Dat) => "ाय",
        (StemClass::A, Gender::Masc, Number::Sg, Case::Abl) => "ात्",
        (StemClass::A, Gender::Masc, Number::Sg, Case::Gen) => "स्य",
        (StemClass::A, Gender::Masc, Number::Sg, Case::Loc) => "े",
        (StemClass::A, Gender::Masc, Number::Sg, Case::Voc) => "",
        (StemClass::A, Gender::Masc, Number::Pl, Case::Nom | Case::Voc) => "ाः",
        (StemClass::A, Gender::Masc, Number::Pl, Case::Acc) => "ान्",
        (StemClass::A, Gender::Masc, Number::Pl, Case::Ins) => "ैः",
        (StemClass::A, Gender::Masc, Number::Pl, Case::Dat | Case::Abl) => "ेभ्यः",
        (StemClass::A, Gender::Masc, Number::Pl, Case::Gen) => "ानाम्",
        (StemClass::A, Gender::Masc, Number::Pl, Case::Loc) => "ेषु",

        (StemClass::A, Gender::Neut, Number::Sg, Case::Nom | Case::Acc) => "म्",
        (StemClass::A, Gender::Neut, Number::Sg, Case::Ins) => "ेण",
        (StemClass::A, Gender::Neut, Number::Sg, Case::Dat) => "ाय",
        (StemClass::A, Gender::Neut, Number::Sg, Case::Abl) => "ात्",
        (StemClass::A, Gender::Neut, Number::Sg, Case::Gen) => "स्य",
        (StemClass::A, Gender::Neut, Number::Sg, Case::Loc) => "े",
        (StemClass::A, Gender::Neut, Number::Sg, Case::Voc) => "",
        (StemClass::A, Gender::Neut, Number::Pl, Case::Nom | Case::Acc | Case::Voc) => "ानि",
        (StemClass::A, Gender::Neut, Number::Pl, Case::Ins) => "ैः",
        (StemClass::A, Gender::Neut, Number::Pl, Case::Dat | Case::Abl) => "ेभ्यः",
        (StemClass::A, Gender::Neut, Number::Pl, Case::Gen) => "ानाम्",
        (StemClass::A, Gender::Neut, Number::Pl, Case::Loc) => "ेषु",

        (StemClass::Aa, Gender::Fem, Number::Sg, Case::Nom) => "ा",
        (StemClass::Aa, Gender::Fem, Number::Sg, Case::Acc) => "ाम्",
        (StemClass::Aa, Gender::Fem, Number::Sg, Case::Ins) => "या",
        (StemClass::Aa, Gender::Fem, Number::Sg, Case::Dat) => "ायै",
        (StemClass::Aa, Gender::Fem, Number::Sg, Case::Abl | Case::Gen) => "ायाः",
        (StemClass::Aa, Gender::Fem, Number::Sg, Case::Loc) => "ायाम्",
        (StemClass::Aa, Gender::Fem, Number::Sg, Case::Voc) => "",
        (StemClass::Aa, Gender::Fem, Number::Pl, Case::Nom | Case::Acc | Case::Voc) => "ाः",
        (StemClass::Aa, Gender::Fem, Number::Pl, Case::Ins) => "ाभिः",
        (StemClass::Aa, Gender::Fem, Number::Pl, Case::Dat | Case::Abl) => "ाभ्यः",
        (StemClass::Aa, Gender::Fem, Number::Pl, Case::Gen) => "ानाम्",
        (StemClass::Aa, Gender::Fem, Number::Pl, Case::Loc) => "ासु",

        _ => return String::new(),
    };
    format!("{stem}{ending}")
}

/// One candidate analysis of an inflected word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    pub stem: String,
    pub class: StemClass,
    pub gender: Gender,
    pub number: Number,
    pub case: Case,
}

const A_ENDINGS: &[(&str, Gender, Case)] = &[
    ("ः", Gender::Masc, Case::Nom),
    ("म्", Gender::Masc, Case::Acc),
    ("म्", Gender::Neut, Case::Nom),
    ("म्", Gender::Neut, Case::Acc),
    ("ेण", Gender::Masc, Case::Ins),
    ("ेण", Gender::Neut, Case::Ins),
    ("ाय", Gender::Masc, Case::Dat),
    ("ाय", Gender::Neut, Case::Dat),
    ("ात्", Gender::Masc, Case::Abl),
    ("ात्", Gender::Neut, Case::Abl),
    ("स्य", Gender::Masc, Case::Gen),
    ("स्य", Gender::Neut, Case::Gen),
    ("े", Gender::Masc, Case::Loc),
    ("े", Gender::Neut, Case::Loc),
];

/// Strip a closed set of singular endings and return every candidate analysis.
/// This is deliberately closed and documented; unknown forms return `None`.
pub fn parse_inflected(word: &str, number: Number) -> Option<Vec<Form>> {
    let mut out = Vec::new();
    for (ending, gender, case) in A_ENDINGS {
        if let Some(stem) = word.strip_suffix(ending) {
            if !stem.is_empty()
                && stem.chars().last().map_or(false, |c| {
                    ('क'..='ह').contains(&c) || c == 'ङ' || c == 'ञ' || c == 'ण'
                })
            {
                out.push(Form {
                    stem: stem.to_string(),
                    class: StemClass::A,
                    gender: *gender,
                    number,
                    case: *case,
                });
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_stem_table() {
        assert_eq!(
            decline("राम", StemClass::A, Gender::Masc, Number::Sg, Case::Nom),
            "रामः"
        );
        assert_eq!(
            decline("राम", StemClass::A, Gender::Masc, Number::Sg, Case::Ins),
            "रामेण"
        );
        assert_eq!(
            decline("पत्र", StemClass::A, Gender::Neut, Number::Sg, Case::Acc),
            "पत्रम्"
        );
        assert_eq!(
            decline("श्याम", StemClass::A, Gender::Masc, Number::Sg, Case::Dat),
            "श्यामाय"
        );
    }
    #[test]
    fn parse_forms() {
        let f = parse_inflected("रामेण", Number::Sg).unwrap();
        assert!(f
            .iter()
            .any(|x| x.case == Case::Ins && x.gender == Gender::Masc && x.stem == "राम"));
        let f = parse_inflected("पत्रम्", Number::Sg).unwrap();
        assert!(f
            .iter()
            .any(|x| x.case == Case::Acc && x.gender == Gender::Neut));
    }
}
