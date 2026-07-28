// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Every vendored asset's printed refresh instruction must be followable.
//!
//! Each [`VendoredWasmAsset`] carries a `refresh_target`, and every failure message the
//! harness prints ends with "run make `<target>`". Nothing forced that name to be a real
//! `make` target: the vendored purrdf engine spent its whole life telling the reader to run
//! `make maint-refresh-purrdf-asset`, a target the Makefile never declared. A descriptor
//! that prints an instruction nobody can follow is worse than one that prints none — it
//! sends the reader somewhere that does not exist, and it hides the fact that the asset had
//! no supported refresh path at all, which is exactly how a pinned blob becomes
//! unrefreshable and then permanently stale.
//!
//! The target set is DERIVED from the descriptors, so a new vendored asset cannot be added
//! without its refresh target and a target cannot be renamed out from under a descriptor.
//!
//! [`VendoredWasmAsset`]: gmeow_docs::vendored_asset::VendoredWasmAsset

use std::path::PathBuf;

use gmeow_docs::vendored_asset::{VENDORED_ASSETS, check_refresh_targets};

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
fn every_vendored_asset_names_a_real_make_target() {
    let makefile = repo_root().join("Makefile");
    let text = std::fs::read_to_string(&makefile)
        .unwrap_or_else(|e| panic!("read {}: {e}", makefile.display()));

    let errors = check_refresh_targets(&text);
    assert!(
        errors.is_empty(),
        "a vendored asset descriptor prints a refresh instruction the Makefile cannot \
         satisfy:\n{}",
        errors.join("\n")
    );

    // The gate is only meaningful if it is actually looking at something.
    assert!(
        !VENDORED_ASSETS.is_empty(),
        "no vendored assets are declared — the refresh-target gate would pass vacuously"
    );
}

#[test]
fn every_refresh_target_is_maintainer_scoped_and_documented() {
    // The repo's convention: a maintainer-only target is `maint-`-prefixed and carries a
    // `## …` help string, which is what `make help` lists. A refresh target that is neither
    // is reachable by accident and invisible to the person looking for it.
    let makefile = repo_root().join("Makefile");
    let text = std::fs::read_to_string(&makefile).expect("read the Makefile");

    for asset in VENDORED_ASSETS {
        let target = asset.refresh_target;
        assert!(
            target.starts_with("maint-"),
            "vendored asset '{}' names refresh target `{target}`, which is not \
             maintainer-scoped",
            asset.name
        );
        let rule = text
            .lines()
            .find(|line| line.starts_with(&format!("{target}:")))
            .unwrap_or_else(|| panic!("no `{target}:` rule in the Makefile"));
        assert!(
            rule.contains("##"),
            "the `{target}` rule carries no `## …` help string, so `make help` cannot \
             list it: {rule}"
        );
    }
}
