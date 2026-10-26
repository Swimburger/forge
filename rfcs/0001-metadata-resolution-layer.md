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
2. **Vendor value formats** — Fern's five-value availability vocabulary, Stainless's `x-stainless-skip` (where a list of target names means *false*).
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

- Nothing in the engine outside the resolution layer references `x-fern-*` or `x-stainless-*` — enforced mechanically. The engine is `src/`, `tests/` and the `packages/forge` npm package; the docs packages in this repository (`astro-fern`, `fern-forge`, `cloudflare-api-site`) read vendor keys themselves and are outside this RFC (see Non-goals).
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
- The docs packages' own `x-fern-*` reading (`fern-forge`, `astro-fern`, `cloudflare-api-site`: downstream consumers, like cf) — follow-up.

## The native vocabulary

| Native key | Form | Meaning | Compat source (the adapter translates) |
|---|---|---|---|
| `x-forge-method-name` | string | the method's name | `x-fern-sdk-method-name` |
| `x-forge-group-name` | **string of names separated by dots**, or **array of strings**, top level first (array elements are not split) | the group path; `""` and `[]` are meant to be the root, but are not yet (see [Known gaps](#known-gaps)) | `x-fern-sdk-group-name` (read the same way as the native key) |
| `x-forge-skip` | bool, positional | operation/alias → no method; property schema → no field | `x-fern-ignore` (operation) · `x-stainless-skip` (property; a list of targets counts as `false` — inside the adapter) |
| `x-forge-availability` | `alpha` \| `beta` \| `preview` \| `generally-available` \| `deprecated` | lifecycle stage. The values are Forge's shipped vocabulary (public enum across Rust, WIT, JS); Fern merely coincides | `x-fern-availability` |
| `x-forge-model-name` | string | the model's name (components / `$defs` schema) | `x-fern-type-name` |
| `x-forge-property-name` | string | the property's field name | `x-fern-property-name` |
| `x-forge-parameter-name` | string | the parameter's field name | `x-fern-parameter-name` |
| `x-forge-sensitive` | bool | the schema's value is a secret, to mask | — (replaces `x-sensitive`, below) |
| `x-sensitive` | bool | **deprecated** name of `x-forge-sensitive`, read only where `x-forge-sensitive` is not written | — (a native key, not a vendor one) |
| `x-forge-hidden`, `x-forge-require-confirmation`, `x-forge-aliases`, `x-forge-commands` | *(existing, unchanged)* | | — no vendor source; now owned by the native table |

Design notes:

- **Explicit position keys over one positional name key.** Fern used three name keys because its reader architecture had one visitor per OpenAPI object kind. We use explicit keys for a different, real reason: tools in the wild mangle `$ref` structure (inlining targets, dropping siblings), and when placement lies, the key name still carries the role — `x-forge-model-name` on a property schema can be ignored or flagged, where a positional `x-forge-name` would silently become a field name. Explicitness here is corruption-detection, not verbosity.
- **`model-name`, not `type-name` or `schema-name`.** The key sets `Model::name`, and the Rust API, the WIT (`model-info`, `get-model`) and the JS runtime all call the thing a model, so the key uses the word a plugin author sees. `type-name`, Fern's word, would promise more than the key does: a `Type` is any type, scalars, arrays and unions included, of which `Type::Model` is one kind, and only a schema that becomes a model is named by the key. `schema-name` is taken: `Model::schema_name` is the key under `components.schemas`, which is the very name this key replaces. `class-name` and `struct-name` are words of particular target languages, and `title` is OpenAPI's own display string. `method-name` and `model-name` are named after what they set in the model; `property-name` and `parameter-name` both set a field's name, so they are named after their position instead, for the reason above.
- **One exclusion word, two positions.** "Excluded from generated surfaces" is one concept at two scopes; two words would be a memory tax for no disambiguation. The word is `skip` rather than `ignore`: it says what happens (the thing is not generated), where `ignore` reads as though the reader ignores the key; it stands further apart from `x-forge-hidden` (generated, but not listed); and it reads naturally should the key one day take a list of targets. That Stainless uses the same word, and Fern the other, played no part.
- **Groups as a dotted string or an array.** The dotted string is how Cloudflare's specs write a group path today, and Forge CI's lint requires it, so moving a team to `x-forge-group-name` is a rename of the key, with the value unchanged. The array is JSON's own way to write a path, and the way to write a group name that has a dot in it. Both are read exactly as forge has always read groups.
- **The dotted string is Cloudflare's, not Fern's.** Fern reads a string `x-fern-sdk-group-name` as *one* group (`"a.b"` is a group named `a.b`) and an array as a path; it never splits on dots. The release pipeline converts Cloudflare's dotted strings to arrays before Fern sees them (the released aggregate has 3,591 arrays and no strings). Forge has always split them, and the adapter reads the Fern key the same way as the native one, so for a string with a dot the adapter and Fern disagree. That is deliberate: the specs forge reads write Fern's key in Cloudflare's convention, and reading the two keys alike keeps a key rename from changing a value's meaning.
- **Every native key begins with `x-forge-`.** The `-name` keys (`method-name`, `group-name`, `model-name`, `property-name`, `parameter-name`) say the same kind of thing and are spelled alike. `x-forge-group-name` holds a path rather than one name, as `x-fern-sdk-group-name` does; keeping the suffix makes moving a team a rename of the key's prefix, and keeps it apart from the legacy `x-forge-group-info`, which Forge CI bans. FORGE-3 expects to rename it to `x-forge-namespace`. `x-sensitive` predates the rule; it becomes `x-forge-sensitive`, and the old name stays readable as a deprecated alias (155 production uses), so no spec breaks. A deprecated alias is part of the native vocabulary, not of the compatibility layer: it keeps working with compat off, and deleting compat does not remove it.
- **No misplacement validation in v1.** A known native key in a wrong position is silently ignored, exactly as Fern's keys are today. Adding validation later is a pure tightening that breaks no valid spec.

## Resolution rules

1. **Native applied, compat fallback** — per primitive, independently (a spec mid-migration may take the group from native and availability from Fern). Keys rank in three tiers: a native key under its current name, then a deprecated native alias (`x-sensitive`), then a compat key. The strongest tier present wins.
2. **Silent native-wins** — when keys of two tiers are present (a native key and a deprecated alias or compat key), the weaker value is ignored without comparison machinery. Only the winning value is validated: an invalid compat `availability` does not error when a native one wins. A value of the wrong JSON type (a number where a string is expected) is still a document-shape error at read time, whichever key it is on and in whatever order the keys are written, so errors never depend on key order. Consequence, to be documented in the metadata docs: a stale compat key silently diverges, and editing the compat key when a native key is present appears to do nothing.
3. **Same key twice** → error (today's `duplicate field` behavior, unchanged).
4. **Empty or punctuation-only values** behave as today, which depends on the key:
   - `x-forge-property-name`, `x-forge-parameter-name` and `x-forge-model-name`: a value with no letters or digits, empty included, is ignored, and the field or model is named after its wire name (the property, parameter or schema name).
   - `x-forge-method-name` and `x-forge-group-name`: an empty method name, or an empty group path (`""` or `[]`), means the operation or alias is not a method. For the empty group path this is the current behavior, not the intended one: see [Known gaps](#known-gaps). A method name, or any group name in the path, that is not empty but has no letters or digits is an error.
   - Validation errors name the key the author actually wrote (provenance), so existing Fern-keyed error expectations hold.
5. **Dual-writing** (native and compat keys carrying the same value) is the supported pattern for keeping specs portable across engine versions during the transition: old engines read the compat key, new engines apply native.

## Known gaps

### An empty group path

- **Intended:** `x-forge-group-name: []` (or `""`) attaches the method to the root, in no group. An operation without `x-forge-group-name` at all is still not a method.
- **Today:** an empty group path makes no method, exactly like a missing key. `add_method` in `src/link.rs` returns before building the method when the path is empty; `Method::group`'s docs in `src/lib.rs` say so; and `tests/parse/groups.rs::empty_group_name_is_no_group` and the empty-group alias case in `tests/parse/aliases.rs` assert it.
- **Why it is not fixed here:** the model has no place for a root method. Methods exist only inside groups (`Forge::groups` → `Group::methods`); `Forge` has top-level models but no top-level methods, and neither does the WIT. The fix needs a root method list in the model, a way for plugins to read it (a new WIT function is additive; a new field in an existing record is not), and generators that render root methods. That is beyond FORGE-1, which leaves the model unchanged.
- **Impact:** none today. The released aggregate has no empty group path and no method name without a group, so the fix will not change its output.
- **To decide with the fix:** whether an empty `x-fern-sdk-group-name` (`""` or `[]`) should also mean the root, or keep meaning "not a method" in the compatibility layer. Fern itself puts a method with no group, or an empty array, at the root of the client, so meaning the root would bring the adapter closer to Fern.

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
│  │              each vendor in turn, and one name lookup per kind, for
│  │              errors. The only things link imports from here.
│  │              The designed end state: delete this folder + the feature.
│  ├─ fern.rs      every x-fern-* literal, mapped onto metadata's primitives;
│  │               the value formats of those keys, as data; Fern unit
│  │               tests.
│  └─ stainless.rs the x-stainless-skip mapping and its format-as-data
│                  (a list of targets counts as false); unit tests.
└─ link/
   ├─ resolve.rs  the orchestrator: native first, then compat behind
   │              #[cfg(feature = "compat")] — the only place link calls
   │              compat. (The feature's other cfg points: `mod compat` in
   │              lib.rs, and the compat-only items in metadata.)
   ├─ openapi.rs  OpenAPI's own keys only; every x- key asks resolve.rs.
   └─ read.rs     serde helpers, as today.
```

Properties, all deliberate:

- **Dependencies**: `link → metadata` and `link → compat` (behind the feature), and `compat → metadata` (an adapter produces metadata's primitives). `metadata` imports neither of the others, so compat sits *beside* the native table rather than under it. `link/resolve.rs` asks native first, compat second — that fallback arm *is* resolution rule 2.
- **Purity**: the library's contract is `&str` keys in and small values out: primitives, key names, and *format descriptors as data* (for example "a string splits on `.`" or "a list of targets counts as `false`"). The serde reading and the arena writes stay in `link` and are generic, driven by those descriptors. This is what makes the library pluggable, standalone-consumable (a future `forge-ci validate` needs the key table, not the engine), and mechanically liftable into a real separate crate when the first standalone consumer appears (FORGE-5's work).
- **The availability vocabulary is native.** The five values live in the native table (they are shipped public API); `fern.rs` only maps `x-fern-availability` onto that primitive.
- **Deletion is the designed end state.** `cargo build --no-default-features --features cli` proves the native vocabulary stands alone. When vendor keys reach end-of-life, deleting the layer means removing `src/compat/` and `tests/compat/`, `mod compat` in `src/lib.rs`, the `compat` cfg arms in `link/resolve.rs` (a resolver and a name lookup for each kind of object the layer reads: operations and aliases, parameters, schemas), and the `compat` feature and its `[[test]]` entry in `Cargo.toml`. No `x-forge-*` extension changes, in behavior, docs, or WIT.
  - *What a deletion leaves behind.* Some native-side code exists only for the layer and compiles, unused, without it: `Hit::compat` and `Hit::after`, `Tier::Compat`, `COMPAT_SLOTS`, the value format only a vendor uses (`BoolFormat::BoolOrTargets`) and its reader in `link/openapi.rs`, and the sentence in `Method::extensions`'s docs about the keys the compatibility layer reads. Removing them is a mechanical follow-on step, not a prerequisite.
- **With compat off, vendor keys are ordinary extensions.** Built without the feature, the engine does not drop `x-fern-*` and `x-stainless-*` keys: like any other `x-` key it does not read, an operation's vendor keys are kept and given to plugins by `Method::extensions`. Native behavior is the same either way; what plugins see in `extensions()` is not.
- **Dispatch, not semantics**: the layer maps keys to primitives and normalizes simple values. Structural behavior — `x-forge-aliases` becoming methods, `x-forge-commands` describing groups — stays in the builder.

## Enforcement

- **Structural boundary — compiler-enforced (built-in Rust):** every vendor key literal is private to its adapter; the only things outside `compat/` that can touch vendor knowledge are the aggregator's per-object resolvers and name lookups, which `link/resolve.rs` calls. The name lookups are how a vendor key's name reaches an error message (rule 4's provenance) without `link` holding the literal: it reaches `link` as data, never as code that matches on it. The feature gate makes compat removable, and builds without the `compat` feature keep proving it.
- **Textual boundary — `tests/boundary.rs`:** scans `src/`, `tests/`, and `packages/forge/` (including the shipped `plugin/forge.wit`) for `x-fern` / `x-stainless` outside `src/compat/` and `tests/compat/`. Doc comments count — moving the WIT's "from `x-fern-availability`" attribution to `x-forge-availability` is part of the work. The test runs wherever `cargo test` runs; a `just boundary` alias targets it.
- **Compat off is compiled in CI, not tested — on purpose.** The `compat` feature is a tool for organizing the code: it keeps the vendor knowledge in one place that can be deleted, rather than being a build that anyone ships. CI's clippy step already builds the crate without it (`--no-default-features`), so the code can't quietly come to depend on the layer. But CI runs the tests only with `--all-features`. A second test run with compat off would compile the crate and every test binary again, for 30–60 s on each push, to cover a configuration nothing ships. The compat-off run in *Verification* was done locally.
- **Criterion mapping:** (1) nothing outside the layer references `x-fern-*` → the two mechanisms above; (2) output unchanged → zero model-shape changes, Fern-keyed specs resolve identically (equivalence tests + the full-aggregate native-vs-wasm byte-equality bench); (3) tests per primitive → below.

## Test plan

- `tests/parse/` — every fixture rewrites to native keys. Not find-and-replace: group values `"a.b"` become `["a","b"]`; the three Fern name keys become their explicit native keys; error assertions naming Fern keys move out.
- `tests/compat/` — the vendor suite, mirroring `tests/parse`'s harness: for each primitive, parse(Fern-keyed fixture) == parse(native-keyed fixture); format specifics (dotted vs array groups, duplicate-key errors, provenance error strings, Stainless's skip-list semantics).
- `tests/boundary.rs` — the guard.
- Existing tripwires keep working: `wit_is_current` (WIT regeneration is mechanical via `wit.mjs`), the wasm dump equality, native-vs-wasm generated-output byte equality.
- Performance: divan parse benches before/after on the real aggregate (`FORGE_SPEC`); `tests/wasm.rs::bench` re-run; forge-bench re-pinned upstream after merge.

## Verification

The implementation was checked against the unmodified engine on the released aggregate (`openapi.forge.json`, 27.6 MB, sha `99544617…`):

- **Fern-keyed behavior.** The original, untouched suite — 379 parse tests written with Fern keys — passed against the new engine before any fixture was migrated.
- **Generated output.** `forge --target ts` on the aggregate writes the same 3,381 files, byte for byte (`diff -r`). The aggregate's 155 `x-sensitive` keys are read through the deprecated alias; the dump's 210 sensitive fields are unchanged.
- **The model.** The full model dump (the repo's own `tests/wasm/dump.rs`: every group, method, availability, hidden flag, confirmation, extension and field name) is identical between the old and new engines: 197,702 lines, covering 3,021 availability values, 1,259 hidden flags, 217 confirmations and 4,695 extension entries. The generated TypeScript consumes none of those four, which is why the dump, not the output diff, is what proves them.
- **Across the plugin boundary.** `tests/wasm.rs`, including the ignored `bench` on the aggregate, reports identical output and dump, native versus WASM, for 1,317 groups and 3,381 files; `wit_is_current` holds with the regenerated WIT.
- **Independence.** `cargo test --no-default-features --features cli` (compat off) passes all 445 tests of the native suite, including the deprecated `x-sensitive` alias; with compat on, 498 pass. Clippy is clean in the library-alone, compat-off and all-features configurations.
- **The checks can fail.** Breaking the precedence rule (keeping the last value written, whatever its tier) fails 11 compat tests, and 1 `tests/parse` test, the deprecated `x-sensitive` one, with compat on and off; planting a vendor name in a doc comment fails the boundary test, naming the file and line.
- **Performance.** Measured with the divan `parse` bench (parse and link of the aggregate), paired against the unmodified engine: each pair ran base–new–new–base so that drift in the machine's speed cancels, took the fastest sample of each run, and left out pairs whose four runs differed by more than 10%. On the final code, over 12 pairs, it is 0.8% slower: +0.34 ms of 42.0 ms, standard error 0.18 ms (t = 1.9), slower in 9 of 12 pairs. Earlier runs during the work put it between −0.1% and +1.1%, depending on the machine's speed state. Peak memory is 68.80 MB against 68.75 MB (+0.07%), with the same peak allocation count. Parsing is under a tenth of a whole `forge` run on the aggregate (about 0.5 s), so the end-to-end change is under 0.1%.
  - *Where it went.* Profiling (samply, with line tables, attributing inlined frames) found no hot spot of the layer's own: most of the difference is LLVM inlining parts of serde_json's key loop into `SchemaObject::read_object` and laying the code out differently, which moves with any change. Three things were measured and kept: each table turns away keys outside its prefix (`x-forge-`, `x-fern-`, `x-stainless-`) before matching, and only `x-` keys are asked about, so a skipped key costs one compare; and a `Hit` carries only its field and slot, the key's name looked up by slot when an error needs it, so the reader's per-key result fits in registers (16 bytes, as before, not 24).
  - *Ruling things out.* Unpaired, sequential runs had suggested +1.5–1.9%; that was the machine warming up through the session, which the ABBA order removes. A single run of either binary differs from the next by about as much as the change measured here, so a smaller difference needs more pairs, not a better single run.

## Release shape

Non-breaking. `x-sensitive` keeps working, deprecated in favor of `x-forge-sensitive`; when it stops being read is FORGE-4's policy question. The WIT changes are doc comments only — `forge:plugin@0.3.0` stays, bindings regenerate identically, no plugin rebuilds anywhere; npm gets an additive release: the engine binary it ships accepts the new native keys and, built with the default `compat` feature, still reads the vendor keys, and the JS runtime changes only in doc comments; the spec surface grows append-only; generated output is byte-identical. Vocabulary renames that *are* breaking (group→namespace, the availability word) are deliberately deferred to FORGE-3 so one breaking release absorbs all of them; the versioning policy that governs this is FORGE-4's RFC.

## Alternatives considered

- **Resolve-after-parse** (keep all `x-` keys as raw JSON, normalize in a second pass): most literal reading of "a resolution step", trivially provenance-aware — but it must retain every schema-level extension on the 27 MB aggregate (5,366 `x-auditable` alone) unless the reader consults a key table anyway, at which point it collapses into the chosen design with extra memory and a reimplementation of duplicate-key errors. Rejected on performance and diff size.
- **Vendor-trait machinery** (a `Vendor` trait with Fern/Stainless/Scalar impls): no primitive overlaps across vendors today; the key table grows rows when one does. Rejected as over-engineering for now.
- **One positional `x-forge-name`**: mirrors the model's single `Name` concept and is cleaner in the abstract — but when toolchains inline `$ref` targets, the model's name key rides into property positions and silently renames fields. Explicit keys turn that corruption into something ignorable or flaggable. Rejected on robustness.
- **Separate words for an operation and a property** (`x-forge-ignore` and `x-forge-skip`, as Fern and Stainless split them): two words for one concept. Rejected.
- **Arrays only for `x-forge-group-name`**: one form, which every tool reads the same way, and loosening later would break nothing. Rejected: Cloudflare's specs write dotted strings, so every migration would reshape values as well as rename keys, and Forge CI's lint would need an array rule.
- **Reading a string `x-fern-sdk-group-name` as one group, as Fern does**: faithful to Fern, but the native and Fern keys would then read the same value differently, and the specs forge reads use dots. Rejected.
- **Error on native/compat disagreement**: the strictest conflict rule, and consistent with the engine's general strictness — deferred as an open question in FORGE-4 in favor of silent native-wins (zero comparison machinery, no cost for correct specs; the drift risk is documented).
- **Full crate split now** (a separate `metadata` member crate): delivers "separate library" literally today. The root `Cargo.toml` is already a `[workspace]`, so adding a member does not mean moving the engine; the real cost is that a separate crate cannot depend on the engine, so `Availability` and the other primitive value types would move into it and be re-exported by `forge`. Deferred: the pure contract keeps that move mechanical for when the first standalone consumer exists.
- **Renaming `Group`/`Availability` now**: both are Fern-inherited words, but both renames break the plugin contract. Deferred to FORGE-3's single breaking wave.

## Follow-ups

| Ticket | What |
|---|---|
| FORGE-3 | Vocabulary renames in one breaking release: `group` → `namespace`, plus the availability word review; `x-forge-group-name`/`x-forge-availability` become deprecated aliases (one table row each, the same tier `x-sensitive` uses). |
| FORGE-4 | The versioning RFC: the four contracts, the rules this RFC assumes, and open questions (1.0 timing, warning channel, vendor-key EoL policy). |
| FORGE-5 | Informational Forge CI lint flagging vendor keys the engine doesn't read, sourced from this layer's key table. |
| (FORGE-1 follow-ups) | cf codebase Fern→Forge rename; dotted `operationId` derivation behind an opt-in flag. |
