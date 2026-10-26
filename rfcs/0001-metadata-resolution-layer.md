# RFC 0001: The metadata resolution layer

- **Status:** Draft
- **Date:** 2026-10-23
- **Ticket:** FORGE-1 — *section off the fern stuff into a compatibility layer*
- **Related:** FORGE-3 (vocabulary renames) · FORGE-4 (versioning RFC) · FORGE-5 (unread vendor key lint)

## Summary

Forge's engine reads vendor metadata extensions (`x-fern-*`, `x-stainless-skip`) directly, so everything downstream of the linked model inherits the vendors' vocabulary, formats, and limitations. This RFC adds a **metadata resolution layer** with three parts:

1. a native `x-forge-*` vocabulary, designed on Forge's own terms rather than mirroring any vendor's;
2. a pure library that resolves keys to primitives — native first, vendor adapters as fallback;
3. a quarantined, feature-gated **compatibility layer** that is the only code holding vendor knowledge, and whose designed end state is wholesale deletion.

After the boundary, the engine — reader, builder, linked model, plugin host, npm runtime — acts only on Forge primitives. Output for existing specs is unchanged, and the release is non-breaking.

## Motivation

Today the engine couples to vendors in five places:

1. **Wire keys** — vendor names are hardcoded in the serde key enums (`DefinitionKey`, `ParameterKey::FieldName`, `SchemaKey::{ModelName, FieldName, Skip}` in `src/link/openapi.rs`).
2. **Vendor value formats** — Fern's dotted-or-array group strings, Fern's five-value availability vocabulary, Stainless's `x-stainless-skip` (where a list of target names means *false*).
3. **Error strings** — validation messages name vendor keys ("`x-fern-availability "ga" is not …`").
4. **The consumed-key set** — the seven names excluded from the plugin passthrough are hand-listed in the deserializers and re-documented in the model docs.
5. **Documentation** — the public model, the WIT, and the JS runtime attribute primitives to vendor keys; the WIT's doc comments ship in the npm package.

Anything downstream — including the cf codebase — therefore depends on a vendor's extension format. Adding another vendor (Stainless, Scalar) means touching the parser, the linker, the docs, and the filter again.

### What the production specs actually use

Audit of the released aggregate (`openapi.forge.json`, 27.6 MB, sha `99544617…`):

| Read by forge today | Uses | Present, **not** read | Uses |
|---|---|---|---|
| `x-fern-sdk-method-name` | 3,638 | `x-fern-enum` | 3,652 |
| `x-fern-sdk-group-name` | 3,638 | `x-stainless-terraform-configurability` | 407 |
| `x-fern-availability` | 3,444 | `x-stainless-deprecation-message` | 281 |
| `x-fern-ignore` | 449 | `x-stability` | 240 |
| `x-fern-parameter-name` | 108 | `x-stainless-collection-type` | 110 |
| `x-fern-property-name` | 38 | `x-fern-audiences` | 80 |
| `x-fern-type-name` | 2 | ~10 further Stainless / Fern keys | ~30 |
| `x-stainless-skip` | 114 | `x-fern-sdk-variables` | 0 |

## Goals

- Nothing outside the resolution layer references `x-fern-*` or `x-stainless-*` — enforced mechanically.
- Output for existing specs is unchanged, proven by byte-equality tests on the real aggregate.
- Tests cover each supported primitive, from each accepted key.
- No meaningful performance regression; parse throughput is benchmarked before and after.
- All Fern (and Stainless) compat code and tests live in one folder, imported only through an aggregating library.
- Native metadata is applied first; the compatibility layer is a fallback.
- The compatibility layer can be completely deleted or turned off without affecting any `x-forge-*` extension.
- The native vocabulary is not constrained by vendor naming, structure, or limitations — native keys are designed for Forge's model, and adapters do the translating.

## Non-goals (this ticket)

- Fern's structural behavior (allOf/anyOf handling, inline request types) — unchanged.
- Operation ID prefixing.
- Renaming Fern→Forge across the cf codebase (existing follow-up).
- Dotted `operationId` derivation (`a.b.c.d` → group `[a,b,c]` + method `d`) — a follow-up that will land *in this layer* behind an opt-in flag.
- Audiences. The ticket lists audiences as a lower-priority primitive; this RFC deliberately leaves `x-fern-audiences` unread (it needs a visibility concept Forge has not designed), flagged by FORGE-5. This is a scope deviation from the ticket text and needs the reviewers' agreement.
- New primitives for `x-fern-enum` (enum-value names — needs a model primitive that does not exist yet), `x-stability`, `x-forge-sunset`, `x-stainless-deprecation-message`. These are flagged (FORGE-5) and stay unread; each would be a one-row additive addition later.
- Treating the `fern-forge` docs package's own `x-fern-*` reading (a downstream consumer, like cf) — follow-up.

## The native vocabulary

| Native key | Form | Meaning | Compat source (the adapter translates) |
|---|---|---|---|
| `x-forge-method-name` | string | the method's name | `x-fern-sdk-method-name` |
| `x-forge-group` | **array of strings**, top level first | the group path | `x-fern-sdk-group-name` (dotted string is Fern's format; the split lives in the adapter. Native has no dot syntax) |
| `x-forge-ignore` | bool, positional | operation/alias → no method; property schema → no field | `x-fern-ignore` (operation) · `x-stainless-skip` (property; a list of targets counts as `false` — inside the adapter) |
| `x-forge-availability` | `alpha` \| `beta` \| `preview` \| `generally-available` \| `deprecated` | lifecycle stage. The values are Forge's shipped vocabulary (public enum across Rust, WIT, JS); Fern merely coincides | `x-fern-availability` |
| `x-forge-model-name` | string | the model's name (components / `$defs` schema) | `x-fern-type-name` |
| `x-forge-property-name` | string | the property's field name | `x-fern-property-name` |
| `x-forge-parameter-name` | string | the parameter's field name | `x-fern-parameter-name` |
| `x-forge-sensitive` | bool | the schema's value is a secret, to mask | — (replaces `x-sensitive`, below) |
| `x-sensitive` | bool | **deprecated** name of `x-forge-sensitive`, read only where `x-forge-sensitive` is not written | — (a native key, not a vendor one) |
| `x-forge-hidden`, `x-forge-require-confirmation`, `x-forge-aliases`, `x-forge-commands` | *(existing, unchanged)* | | — no vendor source; now owned by the native table |

Design notes:

- **Explicit position keys over one positional name key.** Fern used three name keys because its reader architecture had one visitor per OpenAPI object kind. We use explicit keys for a different, real reason: tools in the wild mangle `$ref` structure (inlining targets, dropping siblings), and when placement lies, the key name still carries the role — `x-forge-model-name` on a property schema can be ignored or flagged, where a positional `x-forge-name` would silently become a field name. Explicitness here is corruption-detection, not verbosity.
- **One exclusion word, two positions.** "Excluded from generated surfaces" is one concept at two scopes; two words (`ignore` + `skip`) would be a memory tax for no disambiguation, and `skip` is Stainless's word.
- **Arrays for groups.** JSON's native way to write a path. Accepting dotted strings natively would permanently import Fern's format into the native vocabulary; array-only now keeps the option to loosen later (the reverse — tightening later — would break specs). Fern's dotted strings keep working through the adapter for as long as Fern keys do.
- **Every native key begins with `x-forge-`.** The `-name` keys (`method-name`, `model-name`, `property-name`, `parameter-name`) say the same kind of thing and are spelled alike. `x-sensitive` predates the rule; it becomes `x-forge-sensitive`, and the old name stays readable as a deprecated alias (155 production uses), so no spec breaks. A deprecated alias is part of the native vocabulary, not of the compatibility layer: it keeps working with compat off, and deleting compat does not remove it.
- **No misplacement validation in v1.** A known native key in a wrong position is silently ignored, exactly as Fern's keys are today. Adding validation later is a pure tightening that breaks no valid spec.

## Resolution rules

1. **Native applied, compat fallback** — per primitive, independently (a spec mid-migration may take the group from native and availability from Fern). Keys rank in three tiers: a native key under its current name, then a deprecated native alias (`x-sensitive`), then a compat key. The strongest tier present wins.
2. **Silent native-wins** — when keys of two tiers are present (a native key and a deprecated alias or compat key), the weaker value is ignored without comparison machinery. Only the winning value is validated: an invalid compat `availability` does not error when a native one wins. A value of the wrong JSON type (a number where a string is expected) is still a document-shape error at read time, whichever key it is on and in whatever order the keys are written, so errors never depend on key order. Consequence, to be documented in the metadata docs: a stale compat key silently diverges, and editing the compat key when a native key is present appears to do nothing.
3. **Same key twice** → error (today's `duplicate field` behavior, unchanged).
4. **Empty or punctuation-only values** behave as today, which depends on the key:
   - `x-forge-property-name`, `x-forge-parameter-name` and `x-forge-model-name`: a value with no letters or digits, empty included, is ignored, and the field or model is named after its wire name (the property, parameter or schema name).
   - `x-forge-method-name` and `x-forge-group`: an empty method name, or an empty group path (`[]`, or Fern's `""`), means the operation or alias is not a method. A method name, or any group name in the path, that is not empty but has no letters or digits is an error.
   - Validation errors name the key the author actually wrote (provenance), so existing Fern-keyed error expectations hold.
5. **Dual-writing** (native and compat keys carrying the same value) is the supported pattern for keeping specs portable across engine versions during the transition: old engines read the compat key, new engines apply native.

## Architecture

```
src/
├─ metadata/      THE NATIVE LIBRARY — pure
│   │             The x-forge-* table (per kind of object: definition,
│   │             parameter, schema, document), deprecated native aliases,
│   │             the primitives and format descriptors, the availability
│   │             vocabulary, the consumed-key set. Imports neither compat nor link. No serde, no arena
│   │             coupling: its inputs are &str keys, its outputs small
│   │             pure values.
├─ compat/         THE COMPATIBILITY LAYER — feature "compat" (default on)
│  ├─ mod.rs      the aggregator: one resolver per kind of object, trying
│  │              each vendor in turn. The only thing link imports from here.
│  │              The designed end state: delete this folder + the feature.
│  ├─ fern.rs      every x-fern-* literal, mapped onto metadata's primitives;
│  │               Fern's value formats, as data (e.g. "a string splits on
│  │               dots"); Fern unit tests.
│  └─ stainless.rs the x-stainless-skip mapping and its format-as-data
│                  (a list of targets counts as false); unit tests.
└─ link/
   ├─ resolve.rs  the orchestrator: native first, then compat behind
   │              #[cfg(feature = "compat")] — the only cfg point.
   ├─ openapi.rs  OpenAPI's own keys only; every x- key asks resolve.rs.
   └─ read.rs     serde helpers, as today.
```

Properties, all deliberate:

- **Dependencies**: `link → metadata` and `link → compat` (behind the feature), and `compat → metadata` (an adapter produces metadata's primitives). `metadata` imports neither of the others, so compat sits *beside* the native table rather than under it. `link/resolve.rs` asks native first, compat second — that fallback arm *is* resolution rule 2.
- **Purity**: the library's contract is `&str` keys in and small values out: primitives, key names, and *format descriptors as data* (for example "a string splits on `.`" or "a list of targets counts as `false`"). The serde reading and the arena writes stay in `link` and are generic, driven by those descriptors. This is what makes the library pluggable, standalone-consumable (a future `forge-ci validate` needs the key table, not the engine), and mechanically liftable into a real separate crate when the first standalone consumer appears (FORGE-5's work).
- **The availability vocabulary is native.** The five values live in the native table (they are shipped public API); `fern.rs` only maps `x-fern-availability` onto that primitive.
- **Deletion is the designed end state.** `cargo build --no-default-features --features cli` proves the native vocabulary stands alone; when vendor keys reach end-of-life, deleting `compat/` and the cfg arms in `link/resolve.rs` (one for each kind of object) touches nothing else — no `x-forge-*` extension changes, in behavior, docs, or WIT.
- **Dispatch, not semantics**: the layer maps keys to primitives and normalizes simple values. Structural behavior — `x-forge-aliases` becoming methods, `x-forge-commands` describing groups — stays in the builder.

## Enforcement

- **Structural boundary — compiler-enforced (built-in Rust):** every vendor key literal is private to its adapter; the only thing outside `compat/` that can touch vendor knowledge is the aggregator's per-object resolvers, which `link/resolve.rs` calls. The feature gate makes compat removable, and builds without the `compat` feature keep proving it.
- **Textual boundary — `tests/boundary.rs`:** scans `src/`, `tests/`, and `packages/forge/` (including the shipped `plugin/forge.wit`) for `x-fern` / `x-stainless` outside `src/compat/` and `tests/compat/`. Doc comments count — moving the WIT's "from `x-fern-availability`" attribution to `x-forge-availability` is part of the work. The test runs wherever `cargo test` runs; a `just boundary` alias targets it.
- **Criterion mapping:** (1) nothing outside the layer references `x-fern-*` → the two mechanisms above; (2) output unchanged → zero model-shape changes, Fern-keyed specs resolve identically (equivalence tests + the full-aggregate native-vs-wasm byte-equality bench); (3) tests per primitive → below.

## Test plan

- `tests/parse/` — every fixture rewrites to native keys. Not find-and-replace: group values `"a.b"` become `["a","b"]`; the three Fern name keys become their explicit native keys; error assertions naming Fern keys move out.
- `tests/compat/` — the vendor suite, mirroring `tests/parse`'s harness: for each primitive, parse(Fern-keyed fixture) == parse(native-keyed fixture); Fern format specifics (dotted vs array groups, duplicate-key errors, provenance error strings, Stainless's skip-list semantics).
- `tests/boundary.rs` — the guard.
- Existing tripwires keep working: `wit_is_current` (WIT regeneration is mechanical via `wit.mjs`), the wasm dump equality, native-vs-wasm generated-output byte equality.
- Performance: divan parse benches before/after on the real aggregate (`FORGE_SPEC`); `tests/wasm.rs::bench` re-run; forge-bench re-pinned upstream after merge.

## Verification

The implementation was checked against the unmodified engine on the released aggregate (`openapi.forge.json`, 27.6 MB, sha `99544617…`):

- **Fern-keyed behavior.** The original, untouched suite — 379 parse tests written with Fern keys — passed against the new engine before any fixture was migrated.
- **Generated output.** `forge --target ts` on the aggregate writes the same 3,381 files, byte for byte (`diff -r`). The aggregate's 155 `x-sensitive` keys are read through the deprecated alias; the dump's 210 sensitive fields are unchanged.
- **The model.** The full model dump (the repo's own `tests/wasm/dump.rs`: every group, method, availability, hidden flag, confirmation, extension and field name) is identical between the old and new engines: 197,702 lines, covering 3,021 availability values, 1,259 hidden flags, 217 confirmations and 4,695 extension entries. The generated TypeScript consumes none of those four, which is why the dump, not the output diff, is what proves them.
- **Across the plugin boundary.** `tests/wasm.rs`, including the ignored `bench` on the aggregate, reports identical output and dump, native versus WASM, for 1,317 groups and 3,381 files; `wit_is_current` holds with the regenerated WIT.
- **Independence.** `cargo test --no-default-features --features cli` (compat off) passes all 445 tests of the native suite, including the deprecated `x-sensitive` alias; with compat on, 495 pass. Clippy is clean in the library-alone, compat-off and all-features configurations.
- **The checks can fail.** Breaking the native-wins rule fails 8 compat tests; planting a vendor name in a doc comment fails the boundary test, naming the file and line.
- **Performance.** Measured with `benches/compare.sh` (`just bench-compare`): parse and link of the aggregate, paired against the unmodified engine, each pair run base–new–new–base so that drift in the machine's speed cancels, and pairs in which the machine changed speed left out. The working tree is 0–1% slower (about +0.45 ms of 41.8 ms, standard error 0.17 ms, on a machine at full speed; −0.04 ms, standard error 0.28, on the same machine throttled). Peak memory is +0.55%, with the same peak allocation count. Parsing is under a tenth of a whole `forge` run on the aggregate (about 0.5 s), so the end-to-end change is under 0.1%.
  - *Where it went.* Profiling (samply, with line tables, attributing inlined frames) found no hot spot of the layer's own: most of the difference is LLVM inlining parts of serde_json's key loop into `SchemaObject::read_object` and laying the code out differently, which moves with any change. Three things were measured and kept: each table turns away keys outside its prefix (`x-forge-`, `x-fern-`, `x-stainless-`) before matching, and only `x-` keys are asked about, so a skipped key costs one compare; and a `Hit` carries only its field and slot, the key's name looked up by slot when an error needs it, so the reader's per-key result fits in registers (16 bytes, as before, not 24).
  - *Ruling things out.* Unpaired, sequential runs had suggested +1.5–1.9%; that was the machine warming up through the session, which the ABBA order removes. A single run of either binary differs from the next by about as much as the change measured here, so a smaller difference needs more pairs, not a better single run.

## Release shape

Non-breaking. `x-sensitive` keeps working, deprecated in favor of `x-forge-sensitive`; when it stops being read is FORGE-4's policy question. The WIT changes are doc comments only — `forge:plugin@0.3.0` stays, bindings regenerate identically, no plugin rebuilds anywhere; npm gets an additive release: the engine binary it ships accepts the new native keys and, built with the default `compat` feature, still reads the vendor keys, and the JS runtime changes only in doc comments; the spec surface grows append-only; generated output is byte-identical. Vocabulary renames that *are* breaking (group→namespace, the availability word) are deliberately deferred to FORGE-3 so one breaking release absorbs all of them; the versioning policy that governs this is FORGE-4's RFC.

## Alternatives considered

- **Resolve-after-parse** (keep all `x-` keys as raw JSON, normalize in a second pass): most literal reading of "a resolution step", trivially provenance-aware — but it must retain every schema-level extension on the 27 MB aggregate (5,366 `x-auditable` alone) unless the reader consults a key table anyway, at which point it collapses into the chosen design with extra memory and a reimplementation of duplicate-key errors. Rejected on performance and diff size.
- **Vendor-trait machinery** (a `Vendor` trait with Fern/Stainless/Scalar impls): no primitive overlaps across vendors today; the key table grows rows when one does. Rejected as over-engineering for now.
- **One positional `x-forge-name`**: mirrors the model's single `Name` concept and is cleaner in the abstract — but when toolchains inline `$ref` targets, the model's name key rides into property positions and silently renames fields. Explicit keys turn that corruption into something ignorable or flaggable. Rejected on robustness.
- **Separate `x-forge-skip` for properties**: two words for one concept, and `skip` is Stainless's word. Rejected.
- **Accepting dotted group strings natively**: ergonomic continuity, but it permanently imports Fern's format into the native vocabulary; array-only now is the reversible choice. Rejected.
- **Error on native/compat disagreement**: the strictest conflict rule, and consistent with the engine's general strictness — deferred as an open question in FORGE-4 in favor of silent native-wins (zero comparison machinery, no cost for correct specs; the drift risk is documented).
- **Full crate split now** (a separate `metadata` member crate): delivers "separate library" literally today. The root `Cargo.toml` is already a `[workspace]`, so adding a member does not mean moving the engine; the real cost is that a separate crate cannot depend on the engine, so `Availability` and the other primitive value types would move into it and be re-exported by `forge`. Deferred: the pure contract keeps that move mechanical for when the first standalone consumer exists.
- **Renaming `Group`/`Availability` now**: both are Fern-inherited words, but both renames break the plugin contract. Deferred to FORGE-3's single breaking wave.

## Follow-ups

| Ticket | What |
|---|---|
| FORGE-3 | Vocabulary renames in one breaking release: `group` → `namespace`, plus the availability word review; `x-forge-group`/`x-forge-availability` become deprecated aliases (one table row each, the same tier `x-sensitive` uses). |
| FORGE-4 | The versioning RFC: the four contracts, the rules this RFC assumes, and open questions (1.0 timing, warning channel, vendor-key EoL policy). |
| FORGE-5 | Informational Forge CI lint flagging vendor keys the engine doesn't read, sourced from this layer's key table. |
| (FORGE-1 follow-ups) | cf codebase Fern→Forge rename; dotted `operationId` derivation behind an opt-in flag. |
