#![deny(unsafe_code)]
#![allow(clippy::result_large_err)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::double_ended_iterator_last)]
#![allow(clippy::type_complexity)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::explicit_counter_loop)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::manual_is_ascii_check)]
fn main() {
    // Link the Tier-1 factory into the runtime without a dependency cycle.
    sukti::set_tier1_factory(|threshold| {
        let mut jit = sukti_jit::Jit::new(threshold);
        if let Some(path) = sukti::jit_cache_path() {
            jit = jit.with_cache(path);
        }
        Box::new(jit)
    });
    // The CLI's clap definition lives in `crates/sukti` and is intentionally
    // left untouched. Intercept the two stdio-protocol subcommands here so
    // they get real servers instead of the historical stub.
    if let Some(command) = std::env::args().nth(1) {
        match command.as_str() {
            "lsp" => {
                sukti_lsp::run_lsp();
                return;
            }
            "dap" => {
                sukti_lsp::run_dap();
                return;
            }
            _ => {}
        }
    }
    let code = sukti::cli::main();
    if code != 0 {
        std::process::exit(code);
    }
}
