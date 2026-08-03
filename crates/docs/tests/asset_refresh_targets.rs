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
//! the claim gets an assertion. The descriptor table is the extension point: a fifth
//! engine cannot reintroduce a phantom target without reddening here.

use gmeow_docs::vendored_asset::{
    GMN_ASSET, QUERY_ASSET, REASON_ASSET, VALIDATE_ASSET, VendoredWasmAsset,
};
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

const ASSETS: &[&VendoredWasmAsset] =
    &[&QUERY_ASSET, &VALIDATE_ASSET, &REASON_ASSET, &GMN_ASSET];

#[test]
fn every_asset_refresh_target_is_a_real_makefile_rule() {
    let makefile = makefile();
    for asset in ASSETS {
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

/// Each asset's `bless_env` must be distinct, so one asset's bless cannot silently
/// rewrite another's digest manifest.
#[test]
fn every_asset_bless_env_is_distinct() {
    let mut seen: Vec<&str> = ASSETS.iter().map(|a| a.bless_env).collect();
    seen.sort_unstable();
    let before = seen.len();
    seen.dedup();
    assert_eq!(
        seen.len(),
        before,
        "two assets share a bless environment variable: {seen:?}"
    );
}
