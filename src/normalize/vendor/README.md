# The vendor tables

`forge` reads one vocabulary: the native `x-forge-*` extensions in
[`src/metadata/`](../../metadata/mod.rs). The extensions other tools write
(Fern's `x-fern-*`, Stainless's `x-stainless-*`) are ordinary extensions to
it, kept for plugins like any other it does not read. This folder is the only
place that knows them, and what they say: `forge normalize` uses it to
rewrite them into native keys, before `forge` reads the document.

## Rules

- **Only this folder names a vendor extension.** That includes the readers,
  the linker, the plugin host, the WIT, the `packages/forge` npm package, and
  their comments and docs. `tests/boundary.rs` fails, with file and line, if
  one appears elsewhere (`just boundary`; it also runs with `cargo test`).
  The tests of vendor-keyed documents live in `tests/normalize/`.
- **Mapping, not semantics.** A table says which native field a key stands
  for, and whether its value is written as the native key's is, as data.
  Anything structural, such as how aliases become methods, belongs to the
  builder, not here. The one value that is written again is a group path, which
  becomes the names joined by dots (`walk.rs`).
- **A key is read only at the level `forge` reads its native key at**: an
  operation or an alias, a parameter, or a schema. A key of an object `forge`
  does not read it at is left alone, since rewriting it would give it a
  meaning it did not have.
- **Native wins.** When a document has the native key, the vendor one is
  dropped, or reported if it says something else, whatever order they appear
  in. When a vendor key has no native form, it is left as it is written: on a
  property, `x-stainless-skip: true` and `false` become `x-forge-skip` with the
  same value, but a list of targets such as `["terraform"]` says "skip for
  these targets", which `x-forge-skip` cannot, as it is for every target. On an
  operation, `x-stainless-skip` is not read at all.

## Adding a vendor key

1. Find the native primitive it stands for in `src/metadata/`. If there is
   none, add a native `x-forge-*` key first: a vendor key never introduces a
   primitive.
2. Map it in the adapter for its tool, or add an adapter next to `fern.rs` and
   `stainless.rs` and ask it from `mod.rs`.
3. Add a test in `tests/normalize/` showing that the vendor-keyed document,
   after `forge normalize`, builds the same `Forge` as the native-keyed one.

`forge` itself needs no change: it never learns the key.
