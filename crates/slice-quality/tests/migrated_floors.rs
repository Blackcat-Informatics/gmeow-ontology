// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Value-preservation guard for the floor migration.
//!
//! The committed per-axis and per-slice-tier floors moved out of the governance
//! TSVs into ontology-resident `gmeow:AxisFloorCommitment` / `gmeow:SliceTierFloor`
//! individuals. This test freezes the historical pre-migration values (the golden
//! TSVs under `tests/fixtures/`) and asserts the loaded rubric reproduces every one
//! of them EXACTLY — same slice, same axis/tier local name, and (for axis floors)
//! a bit-identical `f64`. It is permanent and self-contained: it depends on the
//! golden copies, never on the soon-to-be-removed `governance/*.tsv`, so a
//! migration that dropped or perturbed a committed value can never pass silently.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

/// The local name of an IRI (the tail after the last `/` or `#`).
fn local_name(iri: &str) -> &str {
    iri.rsplit(['/', '#']).next().unwrap_or(iri)
}

/// The non-comment, non-blank rows of a golden TSV fixture.
fn golden_rows(name: &str) -> Vec<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden fixture {} must read: {e}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_pre_migration_axis_floor_is_reproduced_bit_exactly() {
    let rubric = gmeow_slice_quality::load_repo_rubric(&repo_root())
        .expect("the committed rubric slice must load");
    let rows = golden_rows("migrated-axis-floors.golden.tsv");
    assert_eq!(rows.len(), 164, "the frozen axis-floor golden has 164 rows");

    for row in &rows {
        let cols: Vec<&str> = row.split('\t').collect();
        assert_eq!(
            cols.len(),
            3,
            "golden axis-floor row is <slice-iri>\\t<axis-local>\\t<f64>: {row:?}"
        );
        let (slice, axis_local, floor_str) = (cols[0], cols[1], cols[2]);
        let want_bits = floor_str
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("golden floor {floor_str:?} parses as f64"))
            .to_bits();

        let found = rubric
            .floors
            .commitments
            .iter()
            .find(|c| c.slice == slice && local_name(&c.axis) == axis_local)
            .unwrap_or_else(|| {
                panic!("no AxisFloorCommitment for slice {slice} axis {axis_local}")
            });
        assert_eq!(
            found.floor.to_bits(),
            want_bits,
            "slice {slice} axis {axis_local}: loaded floor {} must bit-match the frozen golden {floor_str}",
            found.floor
        );
    }
}

#[test]
fn every_pre_migration_tier_floor_is_reproduced() {
    let rubric = gmeow_slice_quality::load_repo_rubric(&repo_root())
        .expect("the committed rubric slice must load");
    let rows = golden_rows("migrated-tier-floors.golden.tsv");
    assert_eq!(rows.len(), 5, "the frozen tier-floor golden has 5 rows");

    for row in &rows {
        let cols: Vec<&str> = row.split('\t').collect();
        assert_eq!(
            cols.len(),
            2,
            "golden tier-floor row is <slice-iri>\\t<tier-local>: {row:?}"
        );
        let (slice, tier_local) = (cols[0], cols[1]);
        let found = rubric
            .floors
            .tier_floors
            .iter()
            .find(|f| f.slice == slice)
            .unwrap_or_else(|| panic!("no SliceTierFloor for slice {slice}"));
        // The current rubric ladder (slices/core/slice-quality-rubric): Registered,
        // Grounded, Linked, Exemplified, Maximal. (An earlier ladder called rank 2
        // "tierRich"; it was renamed "tierLinked" and Exemplified inserted.)
        let rank = |tier: &str| match tier {
            "tierRegistered" => 0,
            "tierGrounded" => 1,
            "tierLinked" => 2,
            "tierExemplified" => 3,
            "tierMaximal" => 4,
            other => panic!("unknown tier local name {other}"),
        };
        let loaded = local_name(&found.tier);
        assert!(
            rank(loaded) >= rank(tier_local),
            "slice {slice}: loaded tier floor {loaded} must reproduce or ratchet the frozen pre-migration floor {tier_local}"
        );
    }
}
