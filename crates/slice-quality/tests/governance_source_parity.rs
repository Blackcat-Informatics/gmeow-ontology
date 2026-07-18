// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Completeness gate for the governance-source authority: the set of governance
//! individuals the `module.ttl` scan sees
//! (`gmeow_slice_quality::load_repo_floors` / `load_repo_ceilings`, unioned across
//! every slice by `governance_source_modules`) MUST equal the set the shipped
//! `gmeow.gts` bundle carries (`ceilings_from_gts`, which flattens the WHOLE bundle —
//! every triple of every slice, on every surface — and reloads the rubric).
//!
//! This makes the `module.ttl`-authoring scope a LOAD-BEARING contract rather than a
//! convention. If a future governance commitment is ever authored on a surface the scan
//! does not read (a slice's `shapes.ttl`, a `mappings/*.ttl`, an inline graph the
//! pipeline folds into the bundle but the loader's `module.ttl` sweep skips), the bundle
//! carries it while the scan does not — the two sets diverge and this test reds, closing
//! the floor-source-divergence bug CLASS (not just the rubric-module instance).

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

#[test]
fn module_scan_governance_set_equals_the_shipped_bundle_set() {
    let root = repo_root();
    let bundle_path = root.join("generated/dist/gmeow.gts");

    // The bundle is committed (`merge=ours`) and rewritten by `make sync`; its ABSENCE
    // (a partial checkout) is the only case where this parity is unknowable, so skip
    // loudly rather than red. A PRESENT-but-stale bundle is a legitimate red — it is the
    // same regenerate-required contract the migrated-floors golden test enforces, and
    // `make check` runs this test post-`make sync`.
    if !bundle_path.is_file() {
        eprintln!(
            "governance-source parity SKIPPED — {} is absent (partial checkout); the binding \
             assertion runs under `make check` on a synced tree",
            bundle_path.display()
        );
        return;
    }

    // What the module.ttl scan sees (the loader's segregated union across all slices).
    let scanned =
        gmeow_slice_quality::load_repo_floors(&root).expect("scan repo governance floors");

    // What the shipped bundle carries (the whole flattened dataset, every surface).
    let bundle_bytes = std::fs::read(&bundle_path).expect("read gmeow.gts bundle");
    let bundled =
        gmeow_slice_quality::ceilings_from_gts(&bundle_bytes).expect("load governance from bundle");

    // Both loaders sort every set by subject IRI, so equality is order-stable. Compare
    // field-by-field for a precise diagnostic on divergence.
    assert_eq!(
        scanned.commitments, bundled.commitments,
        "AxisFloorCommitment set diverges between the module.ttl scan and the shipped bundle — \
         a floor authored on a surface the scan does not read would appear only in the bundle"
    );
    assert_eq!(
        scanned.tier_floors, bundled.tier_floors,
        "SliceTierFloor set diverges between the module.ttl scan and the shipped bundle"
    );
    assert_eq!(
        scanned.ceilings, bundled.ceilings,
        "ProjectionCeilingCommitment set diverges between the module.ttl scan and the shipped bundle"
    );
    assert_eq!(
        scanned.vocabularies, bundled.vocabularies,
        "ProjectionVocabulary registry diverges between the module.ttl scan and the shipped bundle"
    );
    assert_eq!(
        scanned.exemptions, bundled.exemptions,
        "Exemption set diverges between the module.ttl scan and the shipped bundle"
    );
}
