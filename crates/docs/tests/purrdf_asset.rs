// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Anti-rot gate for the vendored purrdf browser engine.
//!
//! The site ships a copy of the published `@blackcatinformatics/purrdf` package under
//! `crates/docs/assets/purrdf/`, refreshed by `make maint-refresh-purrdf-asset`. The
//! pipeline never builds wasm and never reaches the npm registry, so nothing structurally
//! forces the vendored blob to stay in step with the package it was taken from. Two facts
//! are gated here, and each closes a way the vendored copy has previously been able to lie:
//!
//! 1. the SHARED vendored-wasm-asset harness ([`gmeow_docs::vendored_asset`]) over the
//!    [`PURRDF_ASSET`] descriptor — the `.wasm` is a real WebAssembly module of plausible
//!    size, the glue, the wrapper and BOTH type surfaces declare one export set, and
//!    `DIGESTS.blake3` describes the exact on-disk bytes. Integrity is a **content digest**:
//!    a length check passes a stale-but-still-functional engine and every hand edit that
//!    keeps the file size, which is precisely the drift a vendored blob accumulates;
//! 2. the **lower bound**. The refresh path is by floor, not by pin — always the newest
//!    published release satisfying the Makefile's `PURRDF_NPM_MIN`. A floor nothing checks
//!    is not a floor: it could be raised to require a new upstream capability while the
//!    vendored bytes stayed behind it, or the vendored copy could be downgraded, and the
//!    tree would look identical. `UPSTREAM.txt` records what was actually vendored, is
//!    digest-pinned by (1) so it cannot drift from the blob it describes, and is compared
//!    against the declared floor here.

use std::path::PathBuf;

use gmeow_docs::vendored_asset::{PURRDF_ASSET, UPSTREAM_RECORD, check_vendored_lower_bound};

/// The repository root — the ancestor of this crate's manifest dir that contains `crates/`.
fn repo_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while !dir.join("crates").is_dir() {
        assert!(
            dir.pop(),
            "no ancestor of CARGO_MANIFEST_DIR contains crates/"
        );
    }
    dir
}

#[test]
fn vendored_purrdf_engine_passes_the_anti_rot_gate() {
    PURRDF_ASSET.verify();
}

#[test]
fn the_vendored_purrdf_release_satisfies_the_declared_lower_bound() {
    let makefile = repo_root().join("Makefile");
    let text = std::fs::read_to_string(&makefile)
        .unwrap_or_else(|e| panic!("read {}: {e}", makefile.display()));
    let record_path = PURRDF_ASSET.asset_dir().join(UPSTREAM_RECORD);
    let record = std::fs::read_to_string(&record_path).unwrap_or_else(|e| {
        panic!(
            "read {} (run make {}): {e}",
            record_path.display(),
            PURRDF_ASSET.refresh_target
        )
    });

    let errors = check_vendored_lower_bound(&text, &record);
    assert!(
        errors.is_empty(),
        "the vendored purrdf engine does not satisfy the lower bound the Makefile \
         declares:\n{}",
        errors.join("\n")
    );
}
