# The vendor tables

`forge` reads one vocabulary: the native `x-forge-*` extensions, in the readers
of `src/link/`. The extensions other tools write
(Cloudflare's `x-sensitive`, Fern's `x-fern-*`, and Stainless's
`x-stainless-*`) are ordinary extensions to it, kept for plugins like any
other it does not read. This folder is the only place that knows them, their
source, and what they say: `forge normalize` uses it to rewrite them into
native keys, before `forge` reads the document.

`deprecated` is also treated as a vendor here, though it is not literally one:
its adapter holds old Forge keys that have a current replacement. This keeps
renames out of the parser by giving them the same explicit migration path.

## Rules

- **Only this folder names a vendor extension.** That includes the readers,
  the linker, the plugin host, the WIT, the `packages/forge` npm package, and
  their comments and docs. `tests/boundary.rs` fails, with file and line, if
  one appears elsewhere (`just boundary`; it also runs with `cargo test`).
  The tests of vendor-keyed documents live in `tests/normalize/`.
- **Mapping, not semantics.** A table says which native key a key becomes,
  and whether its value is written as the native key's is, as data.
  Anything structural, such as how aliases become methods, belongs to the
  builder, not here. The one value that is written again is a group path, which
  becomes the names joined by dots (`walk.rs`).
- **One adapter per source.** `--from` selects the `cloudflare`, `deprecated`,
  `fern`, and `stainless` adapters, and no `--from` selects them all. The
  temporary in-memory compatibility bridge always uses all adapters, so it
  preserves what the old parser read.
- **A key is read only at the level `forge` reads its native key at**: an
  operation or an alias, a parameter, or a schema. A key of an object `forge`
  does not read it at is left alone, since rewriting it would give it a
  meaning it did not have.
- **Native wins.** When a document has the native key, the vendor one is
  dropped, whatever order they appear in. When a vendor key has no native form, it is left as it is written: on a
  property, `x-stainless-skip: true` and `false` become `x-forge-skip` with the
  same value, but a list of targets such as `["terraform"]` says "skip for
  these targets", which `x-forge-skip` cannot, as it is for every target. On an
  operation, `x-stainless-skip` is not read at all.

## Adding a vendor key

1. Find the native key it stands for in the readers of `src/link/`. If there
   is none, add a native `x-forge-*` key to the reader first: a vendor key
   never introduces a primitive.
2. Map it in the table of the adapter for its source, or add an adapter next to
   `cloudflare.rs`, `deprecated.rs`, `fern.rs` and `stainless.rs` and ask it
   from `mod.rs`. A test fails if the table names a key that `forge` does not
   read at that kind of object. A renamed Forge key goes in `deprecated.rs`
   and its exact old name goes in the table for that object kind.
3. Add a test in `tests/normalize/` showing that the vendor-keyed document,
   after `forge normalize`, builds the same `Forge` as the native-keyed one.

`forge` itself needs no change: it never learns the key.
