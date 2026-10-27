# RFC 0001: The metadata resolution layer

## Summary

The engine reads vendor metadata extensions (`x-fern-*`, `x-stainless-skip`) directly, so everything downstream of the linked model inherits the vendors' vocabulary and formats. This RFC gives the engine a native `x-forge-*` vocabulary and moves all vendor knowledge into one quarantined, feature-gated **compatibility layer** (`src/compat/`) that maps vendor keys onto native primitives. The layer is designed to be deleted as a whole.

Output for existing specs is unchanged, and the release is non-breaking.

## Motivation

Vendor names are hardcoded in the serde key enums in `src/link/openapi.rs`, in value formats (Fern's availability words, Stainless's skip list), in validation messages, in the list of keys kept from plugins, and in the public docs, the WIT and the JS runtime. Anything downstream, including the cf codebase, depends on a vendor's extension format, and adding another vendor means touching all of it again.

The released aggregate uses the keys forge reads about 11,400 times (`x-fern-sdk-*` 3,638 each, `x-fern-availability` 3,444, `x-fern-ignore` 449, `x-stainless-skip` 114, and about 150 name keys). It also carries keys forge does not read, such as `x-fern-enum` (3,652) and `x-stainless-terraform-configurability` (407).

## Scope

Goals:

- Nothing in the engine (`src/`, `tests/`, `packages/forge`) outside `src/compat/` and `tests/compat/` names a vendor extension, enforced by a test.
- Output for existing specs is byte-identical; no meaningful parse-time regression.
- Native keys are applied first; the compatibility layer is only a fallback, and can be removed without changing what any `x-forge-*` key means.

Non-goals: Fern's structural behavior (allOf/anyOf, inline request types), operation ID prefixing, the cf codebase's own Fern-to-Forge rename, dotted `operationId` derivation, and the docs packages in this repository, which read vendor keys themselves. Vendor keys with no Forge primitive (`x-fern-enum`, `x-fern-audiences`, `x-stability` and others) stay unread.

## The native vocabulary

| Native key | Value | Meaning | Compat source |
|---|---|---|---|
| `x-forge-method-name` | string | the method's name | `x-fern-sdk-method-name` |
| `x-forge-group-name` | dotted string, or array of strings, top level first | the group path; an empty path makes no method, as today | `x-fern-sdk-group-name`, read the same way |
| `x-forge-skip` | bool | operation or alias: no method; property schema: no field | `x-fern-ignore` (operation), `x-stainless-skip` (property; a list of targets counts as `false`) |
| `x-forge-availability` | `alpha`, `beta`, `preview`, `generally-available`, `deprecated` | lifecycle stage | `x-fern-availability` |
| `x-forge-model-name` | string | the model's name | `x-fern-type-name` |
| `x-forge-property-name` | string | the property's field name | `x-fern-property-name` |
| `x-forge-parameter-name` | string | the parameter's field name | `x-fern-parameter-name` |
| `x-forge-sensitive` | bool | the value is a secret, to mask | none; replaces `x-sensitive` |
| `x-sensitive` | bool | deprecated name of `x-forge-sensitive`, read only where that is not written | none |

`x-forge-hidden`, `x-forge-require-confirmation`, `x-forge-aliases` and `x-forge-commands` are unchanged.

Design notes:

- **Explicit keys, not one positional name key.** Tools that inline `$ref` targets would make a positional `x-forge-name` silently rename fields. A misplaced explicit key can be ignored or flagged instead.
- **`model-name`** sets `Model::name`, the word the Rust API, the WIT and the JS runtime use. `type-name` would promise more than it does, `schema-name` clashes with `Model::schema_name`, and `class-name` and `title` are a language's word and OpenAPI's display string.
- **`skip`**, not `ignore`: it says what happens, and stands apart from `x-forge-hidden` (generated, but not listed). One word covers operations and properties.
- **Groups as a dotted string or an array.** Cloudflare's specs write dotted strings, so moving to the native key is a rename with the value unchanged. Fern reads a string as one group and never splits on dots, so for a string with a dot the adapter and Fern disagree. That is deliberate. FORGE-3 expects to rename the key to `x-forge-namespace`.
- **Every native key begins with `x-forge-`.** `x-sensitive` predates that rule: it stays readable (155 production uses) as a deprecated alias, which is part of the native vocabulary and keeps working with compat off.
- **No misplacement validation.** A native key at the wrong level is silently ignored, as Fern's keys are today. Validating later only tightens.

## Resolution rules

1. Keys rank in three tiers: native, then deprecated native alias, then compat. The strongest tier present wins, per primitive and whatever order the keys are written in. Only the winning value is validated.
2. The same key written twice is an error. A value of the wrong JSON type is an error whichever key it is on.
3. Names with no letters or digits behave as they always have: a property, parameter or model name is ignored, a method name or group path that is empty makes no method, and a non-empty one that has none is an error.
4. Errors name the key the author actually wrote.
5. Writing native and compat keys with the same value is the supported way to stay portable across engine versions during the transition.

A consequence of rule 1: a stale compat key silently diverges from a native key written beside it.

## Architecture

```
src/
├─ metadata/   the native table, per kind of object (definition, parameter,
│              schema, document): keys, values, formats. Pure: `&str` in,
│              small values out. Imports neither compat nor link.
├─ compat/     the adapters (fern, stainless) and their aggregator. The only
│              code holding vendor keys. Behind the default-on `compat` feature.
└─ link/
   ├─ resolve.rs  native first, then compat; the only place link calls compat
   └─ openapi.rs  OpenAPI's own keys; every `x-` key asks resolve.rs
```

- Dependencies: `link` uses `metadata` and, behind the feature, `compat`; `compat` uses `metadata`.
- The layer maps keys to primitives and normalizes simple values. Structural behavior, such as aliases becoming methods, stays in the builder.
- Without the `compat` feature, vendor keys are ordinary extensions and are passed to plugins.
- Deleting the layer means removing `src/compat/`, `tests/compat/`, `mod compat` in `src/lib.rs`, the compat arms in `link/resolve.rs`, and the feature in `Cargo.toml`. No native key changes. A little native-side code that only the layer used is left over and can be removed afterwards.

## Enforcement

- Every vendor key literal is private to its adapter, and the compiler enforces that.
- `tests/boundary.rs` searches `src/`, `tests/` and `packages/forge/` (docs and WIT included) for `x-fern` and `x-stainless` outside the layer.
- CI compiles the crate without `compat` through clippy, but runs tests only with all features, to avoid a second full compile for a configuration nothing ships.

## Verification

Checked against the unmodified engine on the released aggregate (27.6 MB):

- The original 379 Fern-keyed parse tests passed before any fixture was migrated.
- `forge --target ts` writes the same 3,381 files byte for byte, and the full model dump (197,702 lines) is identical, natively and through the WASM plugin boundary.
- `cargo test` passes with compat on, and with it off when run locally; clippy is clean in the library-only, compat-off and all-features configurations.
- Parse time (divan `parse` bench, paired base-new-new-base runs) is 0.8% slower (+0.34 ms of 42.0 ms, t = 1.9). Peak memory is +0.07%, with the same allocation count.

## Alternatives considered

- **Resolve after parse** (keep every `x-` key as raw JSON, normalize in a second pass): it must retain every extension on the 27 MB aggregate unless the reader consults a key table anyway, which collapses into this design. Rejected on memory and diff size.
- **A `Vendor` trait** with an implementation per tool: no primitive overlaps across vendors today, so the table just grows rows. Over-engineering for now.
- **Separate words for operations and properties** (`ignore` and `skip`): two words for one concept.
- **Arrays only for `x-forge-group-name`**: every migration would reshape values as well as rename keys.
- **Reading a string group as one group, as Fern does**: the native and Fern keys would read the same value differently.
- **Erroring when native and compat keys disagree**: stricter, but costs comparison machinery for no gain on correct specs. Silent native-wins was chosen.
- **A separate `metadata` crate now**: it would pull `Availability` and the other primitive types out of the engine. Deferred until a standalone consumer exists; the pure contract keeps that move mechanical.
- **Renaming `Group` and `Availability` now**: both break the plugin contract, so both wait for FORGE-3's single breaking release.
