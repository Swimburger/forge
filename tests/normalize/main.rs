//! Tests for `forge normalize`: the keys other tools write, rewritten into the
//! native extensions that say the same things.
//!
//! `forge` reads the native keys and no others, so each primitive is tested
//! the same way: a document written with a tool's keys, after `forge
//! normalize`, must build the same `Forge` as its native twin. The tests of
//! what a rewrite does to the document itself, byte for byte, and of the
//! command, are written out.
//!
//! The suite of `tests/parse`, which has no vendor names in it, runs without
//! the `normalize` feature.

mod common;
#[path = "../wasm/dump.rs"]
mod dump;
mod fern;
mod native_first;
mod normalize;
#[cfg(feature = "cli")]
mod normalize_cli;
mod stainless;
