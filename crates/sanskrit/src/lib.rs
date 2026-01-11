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
//! Self-contained Sanskrit text utilities for Sūkti.
//!
//! No large linguistic data files are loaded at runtime. The tables here are
//! small and baked in; a development-time tool cross-checks them against
//! Vidyut where conventions agree.
//!
//! # Caveats (honest)
//! - Transliteration conventions differ between schools. All schemes here are
//!   self-consistent (X → Devanagari → Y round-trips); where they differ from
//!   Vidyut-lipi we document it rather than claiming exact conformance.
//! - Sandhi *split* is ambiguous by nature: `split()` returns candidates.

pub mod declension;
pub mod metre;
pub mod pratyahara;
pub mod sandhi;
pub mod translit;

pub use translit::{transliterate, Script};
