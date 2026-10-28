# Temporary dependency security upgrades

These copies preserve AWS login credentials and LanceDB compression while their
upstream releases still require affected dependencies. They are a workspace build
fix: Cargo ignores a dependency's `[patch.crates-io]` section when publishing or
consuming Swiftide from another workspace. Published Swiftide consumers are **not
protected** by these patches. Those consumers need the same overrides in their
application workspace, or fixed upstream releases. Do not advertise this as a
published-library security fix.

## Source provenance

Each directory is the corresponding crates.io release payload, without Cargo's
local cache markers or release lockfile. Original manifests, tests, generated
source required for compilation, license notices, and readmes are retained.

| Release | crates.io archive SHA-256 | Release source revision |
| --- | --- | --- |
| arrow-ipc 58.4.0 | `29a908a11fcfb3fb2f6730f4ac15e367bc644e419155e96238f68cf3adde572b` | `unavailable in release archive` |
| aws-config 1.12.0 | `b8d7b388a9fc3a6db15a5ec778c38b354eff1364882c94d08e0252f7a47dcaa4` | `653085fbbc4a50138cf955445b65431bea3599d4` |
| lance-encoding 9.0.0 | `4725f14fe6cc1b2a5644786b4afa828d0c243064e208fa17378f839243d049c0` | `7653c2065d6951e0ef236ba9587c6832f7456947` |

Lance's registry payload omits its license file. Its `LICENSE` is copied from
https://github.com/lance-format/lance/blob/7653c2065d6951e0ef236ba9587c6832f7456947/LICENSE.
Arrow's archive does not include Git metadata; its release archive checksum is
the provenance reference. AWS and Arrow license files are unchanged.

## Local changes

`security-upgrades.patch` contains every edit to files from the release payloads:

- Arrow IPC 58.4.0 and Lance encoding 9.0.0 require zstd 0.14.3, which uses
  zstd-safe 8.2.0. This removes RUSTSEC-2026-0391, -0398, and -0403.
- AWS config 1.12.0 uses p256 0.14 and rand 0.10, resolving crypto-bigint 0.7.10
  instead of affected 0.5.5 (RUSTSEC-2026-0387). DPoP uses the new SEC1 and RNG
  APIs and reports entropy failure. Its existing valid-key test now verifies the
  signature and rejects a changed message.
- Empty workspace sections keep these third-party packages outside Swiftide's
  workspace formatting and lint scope and allow standalone upstream tests.

The package versions stay unchanged so existing parent requirements select these
copies. No advisory exclusions or affected-package version numbers are changed.

## Removal

Remove each patch and its directory when the parent release adopts the fixed
published dependency. Refresh the lockfile, run `cargo deny --workspace check`,
and verify Bedrock login, Arrow compression, and Lance compression. AWS config's
latest 1.13.0, Arrow IPC 58.4.1, and LanceDB 0.40.0 still require affected versions
as of 2026-10-28; upgrading them alone does not remove these paths.

## Verification

Run the commands from the repository root, with a writable advisory cache if
needed:

```sh
cargo deny --workspace check
cargo check -p swiftide-integrations --no-default-features --features aws-bedrock,lancedb
cp Cargo.lock vendor/aws-config-1.12.0/Cargo.lock
cargo test --manifest-path vendor/aws-config-1.12.0/Cargo.toml --features credentials-login login::dpop::tests
cp Cargo.lock vendor/arrow-ipc-58.4.0/Cargo.lock
cargo test --manifest-path vendor/arrow-ipc-58.4.0/Cargo.toml --features zstd compression::tests
cp Cargo.lock vendor/lance-encoding-9.0.0/Cargo.lock
cargo test --manifest-path vendor/lance-encoding-9.0.0/Cargo.toml --lib encodings::physical::block::tests::zstd
```

Seed standalone tests from the workspace lockfile to preserve its compatible AWS
SDK versions. Standalone upstream test runs generate local Cargo.lock files, which are not
part of these copies. `cargo package -p swiftide-integrations --list --allow-dirty`
confirms vendor files are absent from the published integration package.

Observed results on 2026-10-28: all cargo-deny checks pass; the integration
compile passes; AWS DPoP tests pass (4 tests, including signature verification
and changed-message rejection); Arrow compression tests pass (2 tests,
including zstd). The Lance runtime-test build was stopped as shared disk free
space fell to 18 GB; its round-trip tests have not run. Lance's patched library
compiled successfully as part of the integration check.
