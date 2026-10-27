//! Tests for the compatibility layer: the keys other tools write, read as the
//! native extensions say the same things.
//!
//! Each primitive is tested the same way: a document written with a tool's
//! keys must build the same `Forge` as its native twin. The tests that only
//! the compatibility layer can pass, such as a value format native keys do
//! not have, or a key a tool writes that forge does not read, are written out.
//!
//! They only build with the `compat` feature. The suite of `tests/parse`,
//! which has no vendor names in it, runs without it.

mod common;
#[cfg(feature = "normalize")]
#[path = "../wasm/dump.rs"]
mod dump;
mod fern;
mod native_first;
#[cfg(feature = "normalize")]
mod normalize;
#[cfg(all(feature = "normalize", feature = "cli"))]
mod normalize_cli;
mod stainless;
