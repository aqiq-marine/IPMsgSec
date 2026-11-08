# Toolchain

## Pinned baseline

- Verus release: 0.2026.11.02.8bf68ee
- Verus tag: release/0.2026.11.02.8bf68ee
- Required Rust toolchain from that release's rust-toolchain.toml: 1.99.1
- Initial CI platform: Ubuntu 24.04 x86_64
- Archive: verus-0.2026.11.02.8bf68ee-x86-linux.zip
- SHA-256: f3a0119292df8855e4435da8130bee1873b0e6a60c306accf81a08ea687bba3c

CI checks the archive digest before extraction. Verus's installation instructions require the Rust toolchain version corresponding to the release. The pinned release's rust-toolchain.toml specifies Rust 1.99.1.

References:
- https://github.com/verus-lang/verus/releases/tag/release/0.2026.11.02.8bf68ee
- https://github.com/verus-lang/verus/blob/main/INSTALL.md
- https://verus-lang.github.io/verus/guide/getting_started_cmd_line.html

## Local setup (Ubuntu 24.04 x86_64)

~~~bash
rustup toolchain install 1.99.1 --profile minimal \
  --component rustfmt --component clippy \
  --component rustc-dev --component llvm-tools
rustup default 1.99.1

VERUS_VERSION=0.2026.11.02.8bf68ee
VERUS_ARCHIVE="verus-$VERUS_VERSION-x86-linux.zip"
curl --fail --location \
  "https://github.com/verus-lang/verus/releases/download/release/$VERUS_VERSION/$VERUS_ARCHIVE" \
  --output "$VERUS_ARCHIVE"
echo "f3a0119292df8855e4435da8130bee1873b0e6a60c306accf81a08ea687bba3c  $VERUS_ARCHIVE" | sha256sum --check
unzip "$VERUS_ARCHIVE" -d .verus
VERUS_BIN="$(find .verus -type f -name verus -perm -u+x -print -quit)"
"$VERUS_BIN" verification/smoke.rs
~~~

The Verus binary path inside the archive is discovered rather than assumed.

## Commands

~~~bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings

# After downloading and verifying the pinned Verus release:
"$VERUS_BIN" verification/smoke.rs
~~~

The smoke proof checks that the verifier is installed and can discharge a small checked-arithmetic theorem. It is not a proof of the IP Messenger parser or the running application.

## Upgrade policy

Upgrade Verus and its required Rust toolchain together. A toolchain update must:
1. Change the release version, archive digest, and Rust toolchain pin.
2. Run the smoke proof and all project proofs.
3. Record the change in an ADR or update this document with rationale and evidence.
4. Never silently switch CI to a rolling Verus build.
