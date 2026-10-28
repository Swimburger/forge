# RFC 0001: Native metadata, and `forge normalize`

## Summary

`forge` reads a native `x-forge-*` vocabulary and nothing else. The extensions other tools write (`x-fern-*`, `x-stainless-skip`, `x-sensitive`) are ordinary extensions to it. A separate command, `forge normalize`, rewrites them into the native ones, so the spec that is produced is itself what gets transformed, and the compatibility knowledge is a layer of its own.

## Motivation

Vendor names were hardcoded in the readers, in value formats, in error messages, in the keys kept for plugins and in the public docs, so everything downstream inherited a vendor's vocabulary, and adding a vendor meant touching all of it. Reading them at runtime, even as a fallback, also means a ranking between keys and a vendor key that silently diverges from the native key beside it. Rewriting them in an explicit step makes the change visible as a diff, and reports a conflict.

## Scope

- `forge` reads native keys only: no tiers, no fallback, no feature to turn on.
- Nothing outside `src/normalize/vendor/` and `tests/normalize/` names a vendor extension, enforced by a test.
- A document, after `forge normalize`, builds what it built before.

Not in scope: Fern's structural behavior (allOf/anyOf, inline request types), operation ID prefixing, and the docs packages, which read vendor keys themselves. Vendor keys with no Forge primitive are neither read nor rewritten.

## The native vocabulary

| Native key               | Value                                                           | Meaning                                           | Rewritten from                                             |
| ------------------------ | --------------------------------------------------------------- | ------------------------------------------------- | ---------------------------------------------------------- |
| `x-forge-method-name`    | string                                                          | the method's name                                 | `x-fern-sdk-method-name`                                   |
| `x-forge-group-name`     | dotted string, or array of strings                              | the group path; an empty path makes no method     | `x-fern-sdk-group-name`                                    |
| `x-forge-skip`           | bool                                                            | operation or alias: no method; property: no field | `x-fern-ignore` (operation), `x-stainless-skip` (property) |
| `x-forge-availability`   | `alpha`, `beta`, `preview`, `generally-available`, `deprecated` | lifecycle stage                                   | `x-fern-availability`                                      |
| `x-forge-model-name`     | string                                                          | the model's name                                  | `x-fern-type-name`                                         |
| `x-forge-property-name`  | string                                                          | the property's field name                         | `x-fern-property-name`                                     |
| `x-forge-parameter-name` | string                                                          | the parameter's field name                        | `x-fern-parameter-name`                                    |
| `x-forge-sensitive`      | bool                                                            | the value is a secret, to mask                    | `x-sensitive`, which other systems write too               |

`x-forge-hidden`, `x-forge-require-confirmation`, `x-forge-aliases` and `x-forge-commands` are unchanged.

- Explicit keys, not one positional name key: tools that inline `$ref` targets would make a positional key silently rename fields.
- `skip`, not `ignore`: it says what happens, and stands apart from `x-forge-hidden`. One word covers operations and properties.
- Groups are a dotted string or an array, so a move from Cloudflare's dotted strings is a rename. A string is names separated by dots; an array is the names, which are not split. FORGE-3 expects to rename the key to `x-forge-namespace`.
- A native key at the wrong level is ignored. Validating later only tightens.

## What `forge normalize` does

- It changes nothing unless it is told to: one document is printed, rewritten, to standard output, and `--write` rewrites each file in place, only if something in it changes. `-` reads standard input and prints. `--check` writes nothing and fails if anything would change.
- A key is rewritten only at the kind of object `forge` reads its native key at: an operation or an alias, a parameter, or a schema.
- The document is edited as bytes, not written again: whitespace, key order and the text of every number stay as they were, so a diff is only the keys.
- A native key is never replaced. When a document says the same thing twice, the native key is kept, and the other is dropped, or reported if it says something else.
- A group path that is an array becomes the names joined by dots, unless a name has a dot in it.
- A skip on a property is rewritten to `x-forge-skip` with the same value: `x-stainless-skip: true` becomes `x-forge-skip: true`. `x-forge-skip` is for every target, so a skip for only some targets, written as a list (`x-stainless-skip: ["terraform"]`), has no native form and is left as it is written, in both modes.
- `--leave-old-extensions` keeps each key that is rewritten, beside the native one, for what still reads it.

## Architecture

```
src/
├─ metadata/        the native table, per kind of object: definition,
│                   parameter, schema, document. Pure: `&str` in, small
│                   values out.
├─ link/            reads native keys through `metadata`.
└─ normalize/
   ├─ vendor/       the tables of other tools' keys. The only code holding
   │                them, sectioned off, and per level.
   ├─ walk.rs       where a key is to be rewritten, and to what.
   └─ scan.rs       reading JSON as bytes, and editing it where it is read.
```

## Migration

- Run `forge normalize --write` before `forge` reads a document written with vendor keys, with `--leave-old-extensions` where something else still reads them. A pipeline is `cat spec.json | forge normalize - | forge - --target ts --out sdk`.
- Until the release that makes `forge normalize` a step to run first, the `forge` command does the same rewrite in memory when it reads a document, so one that has not been through it builds as before. It costs a pass over the document, and stops in that release; the core never reads anything but native keys.
- After that release, a document that is not normalized is not an error: `forge` finds no method names in it, and so no methods.
- Keys that are left are ordinary extensions, so plugins see them.

## Alternatives considered

- **A runtime fallback behind a feature**, native keys first and vendor keys for what has none: it costs tiers in the reader and a vendor key that diverges from the native one beside it. Rejected for an explicit rewrite.
- **Resolving after parsing, in the engine**: it must retain every extension of a large document.
- **Normalizing in memory before every `generate`, for good**: a pass over the document on every run, to save one in CI. Kept for now as a bridge, and not as the design.
- **A `Vendor` trait** per tool: no primitive overlaps across vendors, so the table only grows rows.
