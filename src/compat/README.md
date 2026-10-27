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

## `forge normalize`

`forge normalize` (`src/normalize/`) runs these tables as a rewrite instead
of a read: it finds a key with the same lookups the reader uses, at the same
kinds of object, and writes the native key that says the same thing. There is
no second list of keys, so a key added to an adapter is normalized too, and a
key the reader does not read on an object is not rewritten there. Its tests
are in `tests/compat/`, with the other vendor-keyed documents.

One thing it does is not a key becoming its native key: a group path that
is an array becomes the names joined by dots, unless a name has a dot in it.
And what no native key says is left exactly as it is written, in both modes,
and is not counted or reported: a skip that is a list of targets, which a
native skip, that is for every target, does not say. A rewrite does not drop
what only the old key says.

## Adding a vendor key

1. Find the native primitive it stands for in `src/metadata/`. If there is
   none, add a native `x-forge-*` key first: a vendor key never introduces a
   primitive.
2. Map it in the adapter for its tool, or add an adapter next to `fern.rs`
   and `stainless.rs` and register it in `mod.rs`.
3. Add a test in `tests/compat/` showing that the vendor-keyed document
   parses to the same `Forge` as the native-keyed one. `forge normalize`
   picks the key up without a change, and rewrites it to the native key.

## Removing the layer

Delete this folder and `tests/compat/`, `mod compat` in `src/lib.rs`, the
`compat` arms in `src/link/resolve.rs`, and the `compat` feature and its
`[[test]]` entry in `Cargo.toml`. `forge normalize` is built on these tables:
it goes too, which is `src/normalize/`, `mod normalize` in `src/lib.rs`,
`src/bin/forge/normalize.rs`, `Command` in `src/bin/forge/main.rs`, and the
`normalize` feature, the `glob` and `memchr` dependencies and the CLI tests
of `tests/compat/`. No native extension changes. A few
native-side items exist only for the layer and are left unused; removing them
is a mechanical follow-up.
