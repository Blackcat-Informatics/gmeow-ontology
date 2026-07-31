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
//!    against the declared floor here;
//! 3. the STATED LICENSE of every file in the vendored directory, against what that file
//!    actually is. `PROVENANCE.md` is the licensing authority for this directory and it said
//!    every sidecar states `MIT OR Apache-2.0` while two of the six said `AGPL-3.0-only` —
//!    correctly, because those two are records this repository generates, not bytes it
//!    vendored. Prose nobody checks is prose that ends up contradicting the files it
//!    describes, so the split is asserted here instead of asserted by a sentence.

use std::path::{Path, PathBuf};

use gmeow_docs::vendored_asset::{
    DIGEST_MANIFEST, PURRDF_ASSET, UPSTREAM_RECORD, check_vendored_lower_bound,
};

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

/// The SPDX identifier upstream purrdf's own bytes carry. Vendoring does not relicense.
const UPSTREAM_LICENSE: &str = "MIT OR Apache-2.0";

/// The SPDX identifier a record THIS repository generates carries, wherever it sits.
const RECORD_LICENSE: &str = "AGPL-3.0-only";

/// The files in the vendored directory that this repository writes rather than vendors.
///
/// Derived from the descriptor's own vocabulary — the upstream-release record and the digest
/// manifest — so a third generated record cannot be added without being named here.
const GENERATED_RECORDS: &[&str] = &[UPSTREAM_RECORD, DIGEST_MANIFEST];

/// The `SPDX-License-Identifier` a file states: inline if it carries one, otherwise from its
/// `.license` REUSE sidecar.
///
/// Returns `None` for a file that states neither, which is itself a failure — an unlicensed
/// file in a vendored directory is the case the sidecar convention exists to prevent.
fn stated_license(path: &Path) -> Option<String> {
    const TAG: &str = "SPDX-License-Identifier:";
    let read = |candidate: &Path| -> Option<String> {
        // Read as BYTES and scan lossily: the wasm blob has no text header at all, and
        // decoding it as UTF-8 would fail rather than answer "no inline identifier".
        let bytes = std::fs::read(candidate).ok()?;
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();
        head.lines()
            .find_map(|line| line.split_once(TAG))
            .map(|(_, value)| value.trim().to_string())
    };
    read(path).or_else(|| {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(".license");
        read(Path::new(&sidecar))
    })
}

#[test]
fn every_vendored_file_states_the_license_of_what_it_actually_is() {
    let dir = PURRDF_ASSET.asset_dir();
    let mut checked = 0usize;
    let mut records = 0usize;
    let mut problems = Vec::new();

    let mut names: Vec<String> = PURRDF_ASSET
        .vendored_files
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    names.push(DIGEST_MANIFEST.to_string());
    names.sort();

    for name in &names {
        let path = dir.join(name);
        assert!(path.is_file(), "vendored {name} must exist");
        let generated = GENERATED_RECORDS.contains(&name.as_str());
        let expected = if generated {
            RECORD_LICENSE
        } else {
            UPSTREAM_LICENSE
        };
        records += usize::from(generated);
        checked += 1;
        match stated_license(&path) {
            None => problems.push(format!(
                "{name} states no SPDX identifier, inline or in a .license sidecar"
            )),
            Some(stated) if stated != expected => problems.push(format!(
                "{name} states `{stated}`, but it is {} and must state `{expected}`",
                if generated {
                    "a record this repository generates"
                } else {
                    "upstream purrdf's own bytes"
                }
            )),
            Some(_) => {}
        }
    }

    assert!(
        problems.is_empty(),
        "the vendored purrdf directory's stated licenses do not match what its files are:\n{}",
        problems.join("\n")
    );
    // Non-vacuity: BOTH sides of the split are actually exercised. A directory that turned
    // out to be all-upstream or all-generated would pass the loop above while proving
    // nothing about the distinction PROVENANCE.md draws.
    assert!(
        records > 0 && checked > records,
        "the split must have members on both sides: {records} generated of {checked} files"
    );

    // …and the licensing authority states the same split it is the authority for.
    let provenance = std::fs::read_to_string(dir.join("PROVENANCE.md"))
        .expect("the vendored purrdf directory carries PROVENANCE.md");
    for required in [UPSTREAM_LICENSE, RECORD_LICENSE] {
        assert!(
            provenance.contains(required),
            "PROVENANCE.md does not state `{required}`, which files beside it do"
        );
    }
    for record in GENERATED_RECORDS {
        assert!(
            provenance.contains(record),
            "PROVENANCE.md does not name {record} as a record this repository generates"
        );
    }
}
