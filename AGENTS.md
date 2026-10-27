# Forge Docs Monorepo

API documentation site for Forge OpenAPI specs, built on Astro.

## Architecture

- **`packages/astro-fern`**: Generic Astro engine for rendering Fern-based API documentation.
- **`packages/fern-forge`**: `astro-fern` extension that validates and adapts Forge OpenAPI metadata.
- **`packages/docs-site`**: API documentation website consuming `astro-fern` and `fern-forge`. The OpenAPI spec is fetched from the `openapi.forge.json` GitHub release asset (or a local file via `FORGE_OPENAPI_SPEC`).
- **`packages/cloudflare-terraform-docs`**: Generator for the site's Terraform documentation data, derived from the released Cloudflare Terraform provider. Temporary: removed once Forge supports Terraform natively (see the site's README).
- **`src/`**: The Rust engine: OpenAPI to linked model to plugin host. It reads only native `x-forge-*` extensions (`src/metadata/`); other tools' extensions are translated in `src/compat/`.

## Vendor compatibility

Only `src/compat/` knows vendor extensions (`x-fern-*`, `x-stainless-*`). It is sectioned off so it can be deleted as a whole. See [`src/compat/README.md`](src/compat/README.md).

- Never name a vendor extension outside `src/compat/` and `tests/compat/`, comments and docs included. `tests/boundary.rs` enforces this (`just boundary`).
- Compat is behind the default-on `compat` feature and is only a fallback: native `x-forge-*` keys always win.
- Support for a new vendor key means mapping it onto an existing native primitive in an adapter. Add the native key first if there is none.
- `forge normalize` rewrites vendor keys to native ones using the same adapter tables, so it needs no list of its own.
- The docs packages (`astro-fern`, `fern-forge`, `cloudflare-api-site`) read vendor keys themselves and are not covered by the boundary.

## Conventions

- **Open Source Separation**: Generic packages (`astro-fern`, `fern-forge`) must remain free of Cloudflare-specific dependencies.
- **Formatting & Linting**: `oxfmt` and `oxlint`, configured only at the root (`.oxfmtrc.json`, `.oxlintrc.json`). `pnpm check` verifies; `pnpm format` rewrites.
- **Type Checking**: Strict TypeScript compiler settings across all packages via `pnpm typecheck`.
- **Tests**: Node-native test runner via `pnpm test` (or `pnpm -r run test`).
- **RFCs**: Design proposals and specifications live in `rfcs/`.

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
