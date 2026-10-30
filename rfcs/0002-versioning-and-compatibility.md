# RFC 0002: Versioning and downstream compatibility

## Summary

Forge has one product version across its Git release, Rust crate, command-line
binary and `@cloudflare/forge` npm package. Those artifacts are built and released
together and use the same version number.

The WIT world is bundled with that release but has its own compatibility version.
It changes only when the plugin interface changes. A consumer selects a Forge
release; it does not assemble Rust, JavaScript and WIT versions independently.

This RFC also defines what Forge considers a breaking change across its plugin
API, accepted specifications and linked model. Its central rule is:

> A change is breaking when a supported consumer must change, rebuild, or
> receives a meaningfully different result from the same input.

## Compatibility contracts

Forge has four contracts with different consumers:

1. **The plugin API.** The Rust model API, its isomorphic JavaScript API and the
   WIT world expose one semantic contract. Compatibility includes their shape,
   object identity and the set of operation extensions passed through to a
   plugin.
2. **The specification input.** Service repositories and other producers depend
   on which OpenAPI documents and extensions Forge accepts and what they mean.
3. **The linked model.** Generators depend on the model Forge derives from a
   given specification, even when no public API signature changed.
4. **The specification artifact.** A release such as `openapi@<sha>` is an
   immutable input snapshot. It is versioned by its content SHA, independently
   of the Forge implementation that consumes it.

A change can be compatible on one contract and breaking on another. Release
notes identify every affected contract instead of treating a WIT diff as the
only kind of break.

## Forge releases

The Git tag, Cargo package, `forge --version` and `@cloudflare/forge` package use
the same Forge version. `Cargo.toml` is the current source of truth and the npm
package is synchronized from it.

All distributions advance even when only one implementation changed. The version
means "part of this tested Forge release", not "every artifact changed".

While Forge is below 1.0:

- a compatible release increments the patch version;
- a breaking release increments the minor version.

After 1.0, Forge follows normal semantic versioning: patch for fixes, minor for
compatible features and major for breaking changes.

## The WIT version

The WIT package version describes plugin-interface compatibility, so it does not
advance for an internal engine fix, documentation change or JavaScript helper
change that leaves the interface unchanged.

Before WIT 1.0:

- a compatible additive interface change increments its patch version;
- a breaking interface change increments its minor version;
- a documentation-only change does not increment it.

An additive change is called compatible only after a host/plugin test proves that
a plugin built against the previous WIT version still runs on the new host.
Until that test exists or passes, the change is treated as breaking.

Every Forge release contains exactly one WIT version. Its release notes state the
version and whether existing plugins must rebuild. For example:

| Forge release | WIT world | Plugin action |
| ------------- | --------- | ------------- |
| `0.4.2`       | `0.3.0`   | None          |
| `0.4.3`       | `0.3.1`   | None          |
| `0.5.0`       | `0.4.0`   | Rebuild       |

The numbers do not have to match. The Forge release is what a developer selects;
the WIT version is the compatibility identity carried inside it.

## Keeping the plugin API coherent

The Rust, JavaScript and WIT representations are one semantic API. A concept must
be added, changed, deprecated or removed across every representation that exposes
it in the same Forge release. An unaffected representation does not need an
artificial code change.

The same rule covers behavior that is not visible in a type signature. Changing
which object an ID names, or promoting an operation extension from pass-through
data to a Forge-owned key, can change plugin output and is therefore reviewed as
a plugin-contract change.

CI should enforce this contract with a Rust/WIT/JavaScript parity check and a
host/plugin compatibility test. Until those checks exist, the release author must
verify the three representations in the same pull request.

## Specifications and model behavior

Forge-native specification keys are stable input API. A rename introduces the
new key before the old key is removed. Removal is a breaking change and requires
evidence that supported specifications no longer use the old key plus notice in
an earlier breaking release.

During a migration, writing the native and compatibility keys with the same value
is supported so old and new engines interpret the specification the same way.
When tiers disagree, the strongest tier wins: native, deprecated native alias,
then compatibility key. Once Forge has a warning channel, disagreement should
warn. Changing this to an error requires a future breaking release.

A derivation change is breaking when the same supported specification produces a
meaningfully incompatible linked model. Compatible fixes and intentional output
changes are allowed, but release notes call them out so generator owners can
review their output. A pinned-model regression check will make those changes
visible in CI; it is a guardrail, not a promise that model output never changes.

Vendor compatibility keys may be retired. Retirement requires measured absence
from supported specifications and known consumers, a deprecation warning when a
warning channel exists, and announcement before the breaking release that removes
them. This is a default process rather than a promise to retain every vendor key
forever.

## Downstream use

A downstream consumer pins one Forge release, or an exact commit build until
versioned releases exist. That selection supplies the binary, Rust
implementation, JavaScript API and matching WIT world. A plugin is compiled
against the WIT bundled by that selection.

The `cf` repository is the production compatibility test today. Its build pins a
Forge package, builds the cf generator plugin against that package, and invokes
the package's Forge binary to run the plugin. It separately pins an immutable
`openapi.forge.json` release by SHA. The two pins serve different purposes:

- the Forge version selects generator behavior and the plugin contract;
- the artifact SHA selects the API snapshot to generate from.

Updating a breaking Forge release in cf includes rebuilding the plugin and making
any source migration in the same change. Updating only the API snapshot does not
change the Forge or WIT version.

## Release requirements

Every release records:

- the affected compatibility contracts;
- the WIT version and whether plugins must rebuild;
- any consumer-visible model or specification behavior change.

A breaking release also includes a migration note and demonstrates that the
current production cf plugin can be rebuilt and run. Planned breaking changes
should be batched when doing so avoids repeated downstream migrations.

## Reaching 1.0

Forge can declare 1.0 after the planned pre-1.0 breaking changes have shipped and
the compatibility checks above have exercised at least one production release
cycle. A second plugin consumer is useful evidence but is not a prerequisite.
The maintainers make the decision based on confidence in the contracts, rather
than an automatic date or consumer count.

## Follow-up enforcement

This RFC sets policy; the following checks are separate implementation work:

- reject a release when Cargo, npm, the Git tag and `forge --version` disagree;
- test the supported host/plugin WIT version combinations;
- check semantic parity across the Rust, JavaScript and WIT plugin APIs;
- compare a pinned linked-model fixture across releases;
- surface deprecated keys and conflicting metadata through a warning channel.
