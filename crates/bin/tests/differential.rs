//! Tier-0 vs Tier-1 differential tests: identical output in every JIT mode.

use sukti::pipeline::{self, Options};

const PROGRAMS: &[&str] = &[
    "vada(\"namaste\");",
    "kriyA phaika(na) { yadi (na <= 1) { phalam 1; } phalam na * phaika(na - 1); } vada(phaika(8));",
    "kriyA phiba(na) { yadi (na < 2) { phalam na; } phalam phiba(na - 1) + phiba(na - 2); } vada(phiba(18));",
    "astu sa = 0; yAvat (sa < 10) { sa = sa + 1; } vada(sa);",
    "pratyekam i antaH 1..5 { vada(i); }",
    "sUtra shulka(va) { phalam 10; } apavAda sUtra shulka(va) yadi (va == 1) { phalam 0; } vada(shulka(1)); vada(shulka(2));",
    "kriyA preShaya(kartA ka, karma pa) { phalam ka + \":\" + pa; } vada(preShaya(karma: \"patra\", kartA: \"rAma\"));",
    "vada(saMskRRita.pratyAhAra(\"ach\"));",
    "vada(saMskRRita.rUpAntaraNa(\"namaste\", \"devanagari\", \"iast\"));",
];

#[test]
fn all_jit_modes_agree() {
    sukti::set_tier1_factory(|t| Box::new(sukti_jit::Jit::new(t)));
    for (i, src) in PROGRAMS.iter().enumerate() {
        let mut outputs = Vec::new();
        for mode in ["off", "on", "aggressive"] {
            let opts = Options {
                jit: mode.into(),
                ..Options::default()
            };
            let out = pipeline::run_source(src, &format!("diff#{i}"), &opts)
                .unwrap_or_else(|e| panic!("{mode} failed to run program {i}: {e}"));
            assert!(
                out.diagnostics.is_empty(),
                "program {i} mode {mode}: {}",
                out.diagnostics
            );
            // Strip the JIT stats line added when a Tier-1 engine is present.
            let clean = out
                .output
                .lines()
                .filter(|l| !l.starts_with("calls="))
                .collect::<Vec<_>>()
                .join("\n");
            outputs.push(clean);
        }
        assert!(
            outputs.windows(2).all(|w| w[0] == w[1]),
            "program {i} disagrees across JIT modes: {outputs:#?}"
        );
    }
}
