# Releases

This document describes how a Kanjo release is built, what it contains, how to verify it and how reproducible it is. The workflow is `.github/workflows/release.yml`. It first ran for v0.1.0 and succeeded on that first run: all five targets were built on their native runners, packaged, signed and published as the [v0.1.0 release](https://github.com/Kyotu-Technology/kanjo/releases/tag/v0.1.0). The few steps that were also tried locally on Linux say so. The verification snippets take the repository URL from the `REPO` variable of the cosign snippet, so a fork changes it there. The install commands and the installers name the repository directly: `REPO_URL` in `install.sh` and `$RepoUrl` in `install.ps1`.

## Cutting a release

Set the new version in `[workspace.package]` of the root `Cargo.toml`, move the Unreleased entries of `CHANGELOG.md` under a `## [x.y.z] - YYYY-MM-DD` heading, merge that to main and push a tag `vx.y.z` on the merge commit. The first job refuses to continue when the tag and the workspace version differ. A version with a pre-release suffix (`v0.2.0-rc.1`) is published as a GitHub pre-release.

The workflow runs in six jobs. `meta` checks the version and takes the commit time as `SOURCE_DATE_EPOCH`. `web` builds the UI once with `npm ci` and `npm run build` and hands `web/dist` to every build job, so all five binaries embed byte-identical UI files. `build` compiles the binary per target, `package` creates the archives, SBOMs and `SHA256SUMS`, `sign` signs and attests, and `publish` creates the GitHub release. Only `sign` gets an OIDC token and write access to attestations, only `publish` can write repository contents, and every other job has read access only. The workflow uses no caches, so nothing a pull request workflow wrote can end up in a release.

The `sign` job runs in a GitHub environment named `release`. Configure that environment with required reviewers or a tag protection rule if releases should need a second person. The job uses no secret: cosign signs keylessly with the job's OIDC token, so there is no long-lived signing key to keep.

## Targets and toolchains

| Target | Runner | C toolchain |
| --- | --- | --- |
| `x86_64-unknown-linux-musl` | `ubuntu-24.04` | `musl-gcc` from `musl-tools` |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` | `musl-gcc` from `musl-tools` |
| `x86_64-apple-darwin` | `macos-15-intel` | Xcode clang |
| `aarch64-apple-darwin` | `macos-15` | Xcode clang |
| `x86_64-pc-windows-msvc` | `windows-2025` | MSVC, Strawberry Perl |

The binary links SQLCipher and OpenSSL, both compiled from C by build scripts (`libsqlite3-sys` with `bundled-sqlcipher-vendored-openssl`, and `openssl-src`). The OpenSSL build is driven by a Perl `Configure` script and the C compiler that the `cc` crate selects for the target. That makes the C side, not Rust, the hard part of cross-compiling, and it is why every target is built natively on a runner of its own architecture. GitHub provides native arm64 Linux and macOS runners, so no target needs a cross compiler.

For Linux the binary is static, linked against musl, so one archive runs on any distribution regardless of its libc. Ubuntu's `musl-tools` package provides a `musl-gcc` wrapper for the runner's own architecture; the workflow points `CC_<target>` at it, because the `cc` crate would otherwise look for an `aarch64-linux-musl-gcc` that Ubuntu does not ship. `musl-gcc` searches only musl's own headers, while OpenSSL's secure-heap code includes `<linux/mman.h>`, so the workflow also exposes the kernel UAPI headers (`linux`, `asm`, `asm-generic`, which do not depend on the libc) through `CFLAGS_<target>`, without adding the glibc include tree. `openssl-src` already disables the OpenSSL features that musl cannot build (the engine module and async jobs). After the build, `scripts/check-binary.sh` fails if `readelf -d` shows any `NEEDED` entry.

Two alternatives were considered. `cargo-zigbuild` would build every Linux target from one x86_64 runner with `zig cc`, but it adds Zig and another wrapper between `cargo auditable` and the C build, and the OpenSSL and SQLCipher builds under `zig cc` would then be one more thing to keep working. `cross` runs the build in Docker images that ship musl cross compilers and Perl, but cargo-auditable and the path remapping would have to be set up inside the container. Native runners have the fewest moving parts, which is why every target is built on one.

macOS binaries target macOS 11 (`MACOSX_DEPLOYMENT_TARGET=11.0`) and may only link system libraries under `/usr/lib` and `/System/Library`; the workflow checks that with `otool -L`. They are not signed with a Developer ID or notarized. The arm64 binary carries the ad-hoc signature that the Apple linker adds. Files downloaded with `curl` or by `install.sh` are not quarantined, so the CLI runs; a binary downloaded with a browser may need `xattr -d com.apple.quarantine kanjo`.

On Windows, `openssl-src` needs a native Perl, and Git's MSYS perl, which is often first on `PATH`, breaks its `Configure`. `scripts/ci-windows-perl.ps1` points `OPENSSL_SRC_PERL` at the runner's Strawberry Perl and installs it if the image lacks it. The workflow does not install NASM, and when `openssl-src` finds no NASM it configures OpenSSL with `no-asm`. AES then runs in portable C, which is slower but more than fast enough for a local database. The C runtime is linked statically (`-C target-feature=+crt-static`, set by `scripts/release-build.sh`), so `kanjo.exe` does not need the Visual C++ redistributable. With that flag the `cc` crate compiles SQLCipher and ring with `/MT`, and OpenSSL, configured with `no-shared`, builds its static libraries with `/MT /Zl`, so they carry no runtime reference of their own. `scripts/check-binary.sh` lists the DLLs the binary imports with `llvm-objdump` (from the toolchain's `llvm-tools` component) and fails on any DLL that is not part of Windows, which includes `vcruntime140.dll`, `msvcp140.dll`, `ucrtbase.dll` and the `api-ms-win-crt-*` sets of the dynamic UCRT. The CI job `windows-static` runs the same build script (with the dev profile) and check on every pull request, so a change that brings the dynamic runtime back fails long before a release. The binary is not Authenticode-signed, so SmartScreen may warn on first start.

Every target is built by `scripts/release-build.sh`, which runs `cargo auditable build --profile release --locked -p kanjo --target <target>` with a pinned Rust toolchain (`RUST_TOOLCHAIN` in the workflow) and sets the path remapping and, for Windows, the static C runtime. `--locked` builds exactly the dependency versions in `Cargo.lock`. cargo-auditable embeds the resolved dependency list in a `.dep-v0` section of the binary. `scripts/check-binary.sh` then checks the linkage described above, checks that the section exists and starts the binary once with `--version` on its own runner. `scripts/cargo-audit.sh bin` runs `cargo audit bin` on the result against a fresh shallow clone of the RustSec advisory database, never a cached copy, and refuses a clone with fewer than 1000 advisories, so neither a stale cache nor an empty database can let a binary with a known vulnerable dependency through. CI audits `Cargo.lock` with the same script, and the `msrv` job checks that the workspace still compiles (`cargo check`) with the `rust-version` declared in `Cargo.toml`.

## What a release contains

For every target there is an archive, `kanjo-<version>-<target>.tar.gz` (or `.zip` for Windows), containing one directory, `kanjo-<target>/`, with the binary, `LICENSE`, `NOTICE`, `CHANGELOG.md`, `SECURITY.md` and `README.md`. The directory name carries no version, so the path of the binary in an unpacked archive does not change from one release to the next; the v0.1.0 archives, packed before that, name it `kanjo-0.1.0-<target>/`. Next to each archive there is a CycloneDX 1.5 SBOM, `kanjo-<version>-<target>.cdx.json`, generated by cargo-cyclonedx for that target. The installers `install.sh` and `install.ps1` are attached as well. `SHA256SUMS` lists all of these files. `SHA256SUMS.sigstore.json` is the cosign bundle (signature, certificate and transparency log entry) for `SHA256SUMS`. A GitHub build provenance attestation (SLSA provenance, signed through Sigstore) covers every archive, SBOM and installer.

## Install channels

There are two install channels, one installer for Linux and macOS and one for Windows, and both install the archive from the GitHub release:

```sh
curl -fsSL https://github.com/Kyotu-Technology/kanjo/releases/latest/download/install.sh | sh
```

```powershell
irm https://github.com/Kyotu-Technology/kanjo/releases/latest/download/install.ps1 | iex
```

The `releases/latest/download` URLs point to the latest release that is not a pre-release. Both installers are written to be run this way. `install.sh` keeps all of its work in a function that is called on the last line, inside braces, so a download that is cut off part way defines functions and exits without doing anything; it does not use its own path and does not read standard input. Options go after `sh -s --`, as in `curl -fsSL .../install.sh | sh -s -- --version 0.1.0`. `install.ps1` runs everything inside a script block of its own, because `Invoke-Expression` runs code in the caller's scope: the user's session keeps its variables, functions, `$ErrorActionPreference` and strict mode setting, the TLS setting that Windows PowerShell 5.1 needs is put back afterwards, the script never calls `exit`, and a failed check ends as an ordinary error that leaves the window open. Only `$env:PATH` of the session changes, on purpose, when the install directory is added to the user PATH. Parameters cannot be given to `iex`; the documented form is `& ([scriptblock]::Create((irm .../install.ps1))) -Version 0.1.0`, with switches written without a value (`-NoPathUpdate`), and `powershell -ExecutionPolicy Bypass -File install.ps1 -Version 0.1.0` still works for a downloaded copy. `scripts/test-install.sh` feeds `install.sh` to `sh` through a pipe, with options and without, and cuts it after every line and after every byte of the last line to check that nothing is downloaded or installed. `scripts/test-install.ps1` runs `install.ps1` in a child session and checks that nothing of the installer is left in that session and that it keeps running afterwards: through `Invoke-Expression` with a failure that happens before any download (a run that succeeds would need the network and would change the real user PATH), and in the script block form with the download commands replaced by functions that serve a test release, once installing the latest release and once with `-Version` and a rejected signature. Both scripts run in the Installers CI job, `test-install.sh` on Linux and macOS and `test-install.ps1` under Windows PowerShell 5.1 and PowerShell 7 on Windows; locally they were run under dash, busybox sh and bash 3.2, and under PowerShell 7.4 on Linux.

The installers check the archive against `SHA256SUMS` and, when `cosign` is installed, check that `SHA256SUMS` was signed by the release workflow for that tag, as described under "Verifying a download". Kanjo is not published to a package manager. Building from source is documented in `docs/install.md`.

## Verifying a download

Check the archive against the checksum file first:

```sh
sha256sum --check --ignore-missing SHA256SUMS
```

Then check that `SHA256SUMS` itself was signed by this repository's release workflow, for a tag. The certificate identity pins both:

```sh
REPO=https://github.com/Kyotu-Technology/kanjo
cosign verify-blob \
  --bundle SHA256SUMS.sigstore.json \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity "$REPO/.github/workflows/release.yml@refs/tags/v0.1.0" \
  SHA256SUMS
```

The installers run this check themselves when `cosign` is on `PATH`, with the identity of the version being installed (for cosign 2 they add `--new-bundle-format`, because the release is signed by cosign 3; this cosign 2 path has been tested only with stub commands, not against a real bundle). They stop when the check fails or when the bundle is missing, and they print a summary that says which checks ran and why the others were skipped. Without cosign they install on the checksum alone and say so in a warning, because a checksum from the same download protects against a damaged file but not against a replaced release. `--skip-signature` (`-SkipSignature` in `install.ps1`) turns the cosign check off explicitly, for a machine where cosign is installed but cannot reach Sigstore's trust root. The `package` job checks with `scripts/installer-repo-url.sh` that both installers pin this repository, and the `sign` job runs the same `cosign verify-blob` right after signing, so a release that the installers would reject is never published.

The provenance attestation can be checked with the GitHub CLI, which verifies that the archive was built by a workflow in this repository:

```sh
gh attestation verify kanjo-0.1.0-x86_64-unknown-linux-musl.tar.gz --repo Kyotu-Technology/kanjo
```

The GitHub CLI has to be signed in (`gh auth login` or `GH_TOKEN`) and recent enough to have the `attestation` command; version 2.45 does not have it.

Finally, the dependency list inside an installed binary can be compared with the published SBOM or audited directly with `cargo audit bin "$(command -v kanjo)"`.

## Reproducibility

Several things are pinned so that two builds of the same tag come out as close as possible. `SOURCE_DATE_EPOCH` is the commit time of the tagged commit and is exported to every build and packaging step. Rust paths are remapped with `--remap-path-prefix`: the checkout becomes `/build/kanjo` and the cargo home becomes `/cargo`, so panic messages and debug paths do not contain runner-specific directories. The standard library paths are already `/rustc/<commit>`. The Rust toolchain version, the Node major version and every action are pinned, and dependencies come from `Cargo.lock` and `web/package-lock.json`. The cargo-auditable data is deterministic by design. `scripts/release-package.sh` creates the archives with sorted entries, owner and group 0, fixed permissions and every timestamp set to `SOURCE_DATE_EPOCH`, and gzip without a name or time stamp; packing the same binaries twice gives byte-identical archives (checked locally with synthetic binaries).

Not everything is reproducible, and we do not claim bit-for-bit reproducible binaries. The C code of SQLCipher and OpenSSL is compiled by the runner image's compiler (Ubuntu's gcc for musl, Xcode clang, MSVC), and those versions change when GitHub updates the images, independently of anything in this repository. The C build scripts can embed their own paths and flags, which the Rust path remapping does not touch; OpenSSL, for example, records its configured directories and compiler flags for `OpenSSL_version()`, and those contain the build directory. The Windows linker writes a link timestamp into the PE header unless told otherwise, which the workflow does not do. The web build depends on Vite and its plugins producing stable output for the same lockfile, which we have not verified. The SBOMs are not a source of drift: cargo-cyclonedx takes its timestamp from `SOURCE_DATE_EPOCH`, and in a local test during development two runs on the same commit produced identical files. `SHA256SUMS` therefore changes only when an archive changes. The signatures and attestations are inherently unique per run.

What you can rely on is that a release comes from a known commit, built by a public workflow, with locked dependencies, and that the provenance attestation and the cosign certificate tie each file to that run. A rebuild from the same tag on the same runner image should produce a functionally identical binary with the same embedded dependency list. Byte-for-byte equality would need the C compilers pinned in a container image and a Windows linker flag for deterministic output, and the workflow has neither.
