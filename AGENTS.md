# Forge Docs Monorepo

API documentation site for Forge OpenAPI specs, built on Astro.

## Architecture

- **`packages/astro-fern`**: Generic Astro engine for rendering Fern-based API documentation.
- **`packages/fern-forge`**: `astro-fern` extension that validates and adapts Forge OpenAPI metadata.
- **`packages/docs-site`**: API documentation website consuming `astro-fern` and `fern-forge`. The OpenAPI spec is fetched from the `openapi.forge.json` GitHub release asset (or a local file via `FORGE_OPENAPI_SPEC`).
- **`packages/cloudflare-terraform-docs`**: Generator for the site's Terraform documentation data, derived from the released Cloudflare Terraform provider. Temporary: removed once Forge supports Terraform natively (see the site's README).
- **`src/`**: The Rust engine: OpenAPI to linked model to plugin host. It reads only native `x-forge-*` extensions (`src/metadata/`); other tools' extensions are rewritten into them by `forge normalize` (`src/normalize/`).

## Vendor extensions

Compatibility with other tools' extensions (`x-fern-*`, `x-stainless-*`, and `x-sensitive`) is sectioned off into its own area, `src/normalize/vendor/`, and nowhere else in the codebase knows them. `forge` does not read them: they are ordinary extensions to it, and `forge normalize` is what rewrites them into native ones. Any new compatibility key belongs in that area. See [`src/normalize/vendor/README.md`](src/normalize/vendor/README.md).

- Never name a vendor extension outside `src/normalize/vendor/` and `tests/normalize/`, comments and docs included. `tests/boundary.rs` enforces this (`just boundary`).
- Run `forge normalize --write --leave-old-extensions` on a vendor-keyed document before `forge` reads it. The old keys stay for tools that still read them. Until the release that makes this step required, the `forge` command also does it in memory when it reads a document (`before_reading` in `src/bin/forge/normalize.rs`); that call is temporary, and the engine in `src/` never does it.
- Support for a new vendor key means mapping it onto an existing native primitive in an adapter. Add the native key first if there is none.
- The docs packages (`astro-fern`, `fern-forge`, `cloudflare-api-site`) read vendor keys themselves and are not covered by the boundary.

## Conventions

- **Open Source Separation**: Generic packages (`astro-fern`, `fern-forge`) must remain free of Cloudflare-specific dependencies.
- **Formatting & Linting**: `oxfmt` and `oxlint`, configured only at the root (`.oxfmtrc.json`, `.oxlintrc.json`). `pnpm check` verifies; `pnpm format` rewrites.
- **Type Checking**: Strict TypeScript compiler settings across all packages via `pnpm typecheck`.
- **Tests**: Node-native test runner via `pnpm test` (or `pnpm -r run test`).
- **RFCs**: Design proposals and specifications live in `rfcs/`.

## Public Repository

Never publish links to Cloudflare-internal systems (anything behind Cloudflare's network or login) in files, commits, branch names, or PR and issue text. Bare ticket IDs (`FORGE-1`) are fine. A pushed link stays in history even after a later commit removes it: tell the maintainers.

## Commands

```bash
# Install dependencies
pnpm install

# Run lint and format checks
pnpm check

# Typecheck all packages
pnpm typecheck

# Run all unit tests
pnpm test

# Build the docs site
pnpm --filter docs-site build
```
