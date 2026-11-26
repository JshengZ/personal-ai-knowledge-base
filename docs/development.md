# Development and releases

[English](development.md) · [简体中文](development.zh-CN.md) · [Home](../README.md)

## Prerequisites

- Node.js 24 or later; the initial release was built with Node 26.3.0.
- Rust through rustup. `rust-toolchain.toml` pins 1.100.0.
- A native compiler: Xcode Command Line Tools on Intel macOS, or Visual Studio C++ Build Tools / Windows SDK on Windows x64.
- CMake for rebuilding the Windows embedding runtime. Cross-compiling it on a Mac also requires Zig; cross-compiling the Rust engine requires `cargo-zigbuild` and the Windows GNU Rust target.

These are developer requirements. Installer users do not install development tools.

## Check and run

```sh
npm ci
npm run assets -- --small-only
npm run check
npm run build:web
npm run build:desktop
cargo build --locked --manifest-path engine/Cargo.toml
npm test
npm run test:integration
npm run test:resilience
npm run test:desktop
npm run dev
```

`--small-only` prepares local OCR resources and icons without downloading the embedding model. The integration suite uses a synthetic local model API to exercise provider protocols and MCP. Full local vector retrieval requires the complete assets.

Development knowledge is stored in ignored `.local/dev`, separate from the installed application's per-user database. The desktop launcher uses `.local/desktop`. The Vite preview is bound to `127.0.0.1:5173` and proxies to the local owner service; it does not expose data files as static assets.

## Build an Intel Mac installer

```sh
npm ci
npm run assets
npm run dist:mac
node scripts/verify-package.mjs mac
node --test tests/packaged.test.mjs
```

The DMG is written under `release/mac-x64/`. The current resource preparation supports the Intel Mac runtime used by this release; an ARM64 package requires adding and validating its runtime before distribution.

## Build Windows on Windows

In a Visual Studio developer environment with CMake and the Windows SDK:

```sh
npm ci
npm run assets
npm run assets:win-runtime
npm run dist:win
node scripts/verify-package.mjs win
node --test tests/packaged.test.mjs
```

The embedding runtime is built from checksum-pinned llama.cpp source with static C++ linkage, OpenSSL/OpenMP disabled, and baseline x64 CPU settings. The NSIS installer is written under `release/win-x64/`.

## Cross-build Windows on a Mac

```sh
rustup target add x86_64-pc-windows-gnu
cargo install cargo-zigbuild --locked
```

Set `PKB_ZIG` to your Zig executable and `PKB_CMAKE` if CMake is not on PATH, then run:

```sh
npm run assets
npm run assets:win-runtime
export CARGO_ZIGBUILD_ZIG_PATH="$PKB_ZIG"
npm run dist:win
node scripts/verify-package.mjs win
```

The initial Windows runtime used Zig 0.17.1. Cross-building cannot prove target-workstation GUI behavior; perform the packaged smoke test on Windows before claiming that validation.

## Resource and privacy checks

`scripts/prepare-assets.mjs` verifies upstream archive/model hashes. `prepare-platform.mjs` verifies the target canvas package using the npm lockfile integrity. Windows runtime provenance is recorded in `resources/runtime/win-x64/build-manifest.json`. Compiler paths are remapped in public builds.

The package audit checks runtime selection, native modules, the model digest, a blank organization template, and absence of knowledge databases, backups, or tokens. Resource directories, credentials, local databases, and build output are ignored by Git.

An optional build-time Cargo mirror is shown in `.cargo/config.example.toml`. Set `ELECTRON_MIRROR` only if a download mirror is needed. Neither setting chooses a runtime model API.

For organization-specific installers, set `PKB_COMPANY_CONFIG` to a JSON template following `resources/company-config.json`. The packager allows organization/API/model/protocol fields and strips credential fields. Do not publish an organization-specific package as the project's generic release.

## Release procedure

1. Update versions in `package.json`, `package-lock.json`, `engine/Cargo.toml`, and `engine/Cargo.lock`, then update both languages and the changelog.
2. Run CI and the platform-specific packaged smoke test. Keep validation claims tied to actual evidence.
3. Rebuild installers and run both package audits. If signing identities are available, configure `CSC_LINK` and the signing provider's required secrets in the build environment; never commit them. Apple notarization requires separate platform validation.
4. Generate SHA-256 checksums and public build metadata with `node scripts/release-info.mjs`.
5. Create a Git tag, then a **draft** GitHub Release. Upload only the installers, `SHA256SUMS.txt`, and `BUILD-INFO.json`. Verify uploaded digests before publishing.
6. Verify repository visibility and anonymous download access. Never upload `.local/`, knowledge backups, developer logs, signing keys, or user configuration.

In GitHub Actions, run **Build installers** on `main`. After both package jobs, the matching source CI and secret scan succeed, run **Publish verified release** and enter that build's numeric run ID as `installer_run`. It downloads both artifacts in the cloud, checks their source, version, size and SHA-256, creates a draft, uploads and verifies the four release assets, then publishes. The tag points to the tested installer source; `BUILD-INFO.json` also records the publishing-tool commit. Add bilingual notes under `docs/releases/v<version>.md` before publishing. Build artifacts expire after seven days; rebuild if they have expired.

The publication workflow rejects failed builds, modified binaries, mismatched platform versions and an existing published release. GitHub automatically provides source archives for the tag; large installers belong in Releases. The manual procedure above remains available.

Update third-party notices when dependencies change. `node scripts/collect-licenses.mjs` refreshes JavaScript records, including bundled UI dependencies; Rust license records must be updated alongside lockfile changes. Use only properly licensed contributions and preserve upstream notices.

For an offline Electron packaging step, set `PKB_ELECTRON_DIST` to an unpacked Electron distribution or a directory containing the exact pinned `electron-v<version>-<platform>-<arch>.zip`. This uses a local runtime cache; it does not change the pinned Electron version.
