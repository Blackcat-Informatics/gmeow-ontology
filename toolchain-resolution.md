# Managed Cargo resolution

Initial make clippy-prebuild stopped before Cargo execution because the Stage wrapper default active-toolchain/bin/cargo path disappeared (EXIT2 from Make, wrapper127). This was not a code diagnostic. Managed rustup/settings.toml still selects nightly-2026-10-01-x86_64-unknown-linux-gnu; its Cargo binary exists, and the deployed rustc, cargo-clippy and rustfmt links resolve to that same dated managed directory.

The Stage cargo wrapper explicitly supports CARGO_REAL. Qualification uses CARGO_REAL=/home/paudley/stage/packages/rustup/toolchains/nightly-2026-10-01-x86_64-unknown-linux-gnu/bin/cargo with the unchanged Make target and wrapper. No compiler substitution, wrapper bypass, global installation, host deployment, configuration or symlink modification. The original resolution failure remains in clippy-toolchain-resolution-failed.log. Rerun result pending; no pass inferred.
