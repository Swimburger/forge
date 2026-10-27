# The compatibility layer

The engine understands one vocabulary: the native `x-forge-*` extensions in
[`src/metadata/`](../metadata/mod.rs). This folder is the only place that
knows any other tool's extensions (Fern's `x-fern-*`, Stainless's
`x-stainless-*`). Each adapter maps its tool's keys onto native primitives.

The layer is meant to be deleted as a whole once nobody writes those keys.
Everything below follows from that.

## Rules

- **Only this folder names a vendor extension.** That includes the readers,
  the linker, the plugin host, the WIT, the `packages/forge` npm package, and
  their comments and docs. `tests/boundary.rs` fails, with file and line, if
  one appears elsewhere (`just boundary`; it also runs with `cargo test`).
  Vendor tests live in `tests/compat/`.
- **Native wins.** A vendor key is read only for a primitive the document
  has no native key for. When both are written, the native value is used and
  the vendor value is not even validated, whatever order they appear in.
- **Behind the `compat` feature** (on by default). Native keys must mean the
  same with it off. CI compiles that configuration (clippy with
  `--no-default-features`) but does not run its tests.
- **Mapping, not semantics.** An adapter says which native primitive a key
  stands for and how its value is written, as data. Anything structural, such
  as how aliases become methods, belongs to the builder, not here.
- **No vendor knowledge leaks out.** Keys are private to their adapter.
  `mod.rs` is the only way in: one resolver per kind of object, and one name
  lookup per kind so errors can name the key the author wrote.

## Adding a vendor key

1. Find the native primitive it stands for in `src/metadata/`. If there is
   none, add a native `x-forge-*` key first: a vendor key never introduces a
   primitive.
2. Map it in the adapter for its tool, or add an adapter next to `fern.rs`
   and `stainless.rs` and register it in `mod.rs`.
3. Add a test in `tests/compat/` showing that the vendor-keyed document
   parses to the same `Forge` as the native-keyed one.

## Removing the layer

Delete this folder and `tests/compat/`, `mod compat` in `src/lib.rs`, the
`compat` arms in `src/link/resolve.rs`, and the `compat` feature and its
`[[test]]` entry in `Cargo.toml`. No native extension changes. A few
native-side items exist only for the layer and are left unused; removing them
is a mechanical follow-up.
