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
//! Sūkti — a Sanskrit-native, Devanagari-first general-purpose language.
//!
//! The name lives here as a single constant so it can be changed in one place
//! (see lexicon/lexicon.toml for the data-driven vocabulary).

pub const NAME: &str = "Sūkti";
pub const VERSION: &str = "0.1.0";

use std::sync::Mutex;
static TIER1_FACTORY: Mutex<Option<fn(usize) -> Box<dyn value::Tier1>>> = Mutex::new(None);
static JIT_CACHE_PATH: Mutex<Option<String>> = Mutex::new(None);

/// Install the Tier-1 factory (called once by the CLI binary, which links both
/// the runtime and the Cranelift JIT without creating a dependency cycle).
pub fn set_tier1_factory(f: fn(usize) -> Box<dyn value::Tier1>) {
    *TIER1_FACTORY.lock().unwrap() = Some(f);
}
pub(crate) fn tier1_factory() -> Option<fn(usize) -> Box<dyn value::Tier1>> {
    *TIER1_FACTORY.lock().unwrap()
}

/// Set the optional path for the Tier-1 compilation-plan cache. Called by the
/// pipeline from `Options::jit_cache`; the JIT factory reads it when building
/// an engine. Native code is process-local, so the cache stores only the
/// compilation plans and precise deopt-site metadata.
pub fn set_jit_cache_path(path: Option<String>) {
    *JIT_CACHE_PATH.lock().unwrap() = path;
}
pub fn jit_cache_path() -> Option<String> {
    JIT_CACHE_PATH.lock().unwrap().clone()
}

pub mod ast;
pub mod cli;
pub mod diag;
pub mod docgen;
pub mod fmt;
pub mod lexer;
pub mod lexicon;
pub mod parser;
pub mod pipeline;
pub mod resolve;
pub mod stdlib;
pub mod value;
pub mod vm;
