// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Every wasm-asset descriptor's `refresh_target` must name a REAL `Makefile` rule.
//!
//! This gate exists because one did not. The playground engine's descriptor carried
//! `refresh_target: "maint-refresh-purrdf-asset"` — a target that was never defined
//! anywhere in the `Makefile`. Nothing checked it, so the failure message told every
//! reader to run a command that did not exist, and the asset it named could not in
//! fact be refreshed from this repository at all.
//!
//! A `refresh_target` is a claim about the build. Per `docs/GATE-AND-PIPELINE.md`, a
//! false claim in help text is a defect of the same kind as a broken assertion — so
//! the claim gets an assertion. It iterates `vendored_asset::ALL_ASSETS`, the same
//! registry the renderer emits from — not a copy of it, which would let a fifth engine
//! ship while this gate still asserted over four.

use gmeow_docs::vendored_asset::ALL_ASSETS;
use std::path::PathBuf;

fn makefile() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate is under <repo>/crates")
        .to_path_buf();
    std::fs::read_to_string(root.join("Makefile")).expect("read the workspace Makefile")
}

/// Whether `target` is defined as a rule (`^<target>:`) in the Makefile.
fn defines_target(makefile: &str, target: &str) -> bool {
    let prefix = format!("{target}:");
    makefile.lines().any(|line| line.starts_with(&prefix))
}

#[test]
fn every_asset_refresh_target_is_a_real_makefile_rule() {
    let makefile = makefile();
    for asset in ALL_ASSETS {
        assert!(
            defines_target(&makefile, asset.refresh_target),
            "asset `{}` names refresh_target `{}`, which is NOT defined in the Makefile — \
             its failure messages would tell a reader to run a command that does not exist",
            asset.name,
            asset.refresh_target
        );
    }
}

/// Prove the gate has teeth (`docs/GATE-AND-PIPELINE.md` P8): a bogus target name must
/// be rejected by the very predicate the test above trusts. Without this, a
/// `defines_target` that always returned `true` would pass the suite silently.
#[test]
fn the_target_check_rejects_a_name_the_makefile_does_not_define() {
    let makefile = makefile();
    assert!(
        !defines_target(&makefile, "maint-refresh-purrdf-asset"),
        "the retired phantom target is defined again — the defect this gate exists for"
    );
    assert!(
        !defines_target(&makefile, "maint-refresh-a-target-that-does-not-exist"),
        "defines_target accepted a name the Makefile does not define; the gate is vacuous"
    );
}

/// Each asset's `refresh_target` must be distinct AND must actually refresh THAT asset.
///
/// Existence alone is too weak: all four descriptors could name one real target and the
/// existence gate would stay green, so three engines would advertise a command that
/// re-vendors a different one.
#[test]
fn every_asset_refresh_target_is_distinct_and_refreshes_that_asset() {
    let mut seen: Vec<&str> = ALL_ASSETS.iter().map(|a| a.refresh_target).collect();
    seen.sort_unstable();
    let before = seen.len();
    seen.dedup();
    assert_eq!(
        seen.len(),
        before,
        "two assets share a refresh target, so one of them advertises a command that \
         re-vendors a different engine: {seen:?}"
    );

    // The recipe must name the asset it claims to refresh. The rules are generated from a
    // canned recipe keyed by the engine's short name, so the call carries that name.
    let makefile = makefile();
    for asset in ALL_ASSETS {
        let recipe = makefile
            .split(&format!("\n{}:", asset.refresh_target))
            .nth(1)
            .unwrap_or_else(|| panic!("no recipe body for {}", asset.refresh_target));
        let body = recipe.split("\n\n").next().unwrap_or(recipe);
        assert!(
            body.contains(asset.name),
            "`{}` never names `{}` in its recipe, so it does not demonstrably refresh the \
             asset whose failure message advertises it",
            asset.refresh_target,
            asset.name
        );
    }
}
