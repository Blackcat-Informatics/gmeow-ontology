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

/// Slices RETIRED since the golden was frozen.
///
/// The golden TSVs are a permanent historical record and are never edited — a
/// dropped or perturbed value must stay detectable forever. But a slice that no
/// longer exists has no commitment to reproduce, so its frozen rows are skipped
/// here rather than deleted there. Each entry names the slice and why it went,
/// so the exemption is a recorded decision rather than a silent hole: every row
/// of every LIVE slice keeps its full teeth.
const RETIRED_SLICES: &[(&str, &str)] = &[(
    "https://blackcatinformatics.ca/gmeow/slices/procedures",
    "process-model slice superseded by the canonical logic: prescription/enactment \
     spine; its terms were removed rather than kept as a second source of truth",
)];

/// True when the row belongs to a slice retired since the freeze.
fn is_retired(slice: &str) -> bool {
    RETIRED_SLICES.iter().any(|(iri, _)| *iri == slice)
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
    let floors = gmeow_slice_quality::load_repo_floors(&repo_root())
        .expect("the committed rubric floors must load");
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
        if is_retired(slice) {
            continue;
        }
        let want_bits = floor_str
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("golden floor {floor_str:?} parses as f64"))
            .to_bits();

        let found = floors
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
    let floors = gmeow_slice_quality::load_repo_floors(&repo_root())
        .expect("the committed rubric floors must load");
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
        let found = floors
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

/// The escape hatch must never be able to silence a LIVE slice.
///
/// `RETIRED_SLICES` skips frozen golden rows, and the golden is a permanent record whose
/// whole value is that a dropped or perturbed commitment stays detectable forever. A slice
/// that no longer exists genuinely has no commitment to reproduce; a slice that still ships
/// does, and adding it here would silently drain the teeth from every one of its frozen
/// rows while the suite kept reporting green.
///
/// So the list is pinned to the one thing that makes an entry legitimate: the slice is
/// GONE. Liveness is read from the repository itself — the same
/// `discover_slice_dirs` + `slice_iri_of_dir` pair the sweep, the ratchet gate and the
/// pipeline carrier producer use — not from a second hand-maintained roster that could
/// drift from it.
#[test]
fn no_retired_slice_entry_names_a_slice_that_still_exists() {
    let root = repo_root();
    let live: Vec<String> = gmeow_slice_quality::discover_slice_dirs(&root.join("slices"))
        .iter()
        .map(|dir| {
            gmeow_slice_quality::slice_iri_of_dir(dir)
                .unwrap_or_else(|e| panic!("{} must declare a gmeow:Slice: {e}", dir.display()))
        })
        .collect();
    assert!(
        !live.is_empty(),
        "slice discovery found no slices at all, so this guard would pass vacuously"
    );
    for (iri, reason) in RETIRED_SLICES {
        assert!(
            !live.contains(&(*iri).to_owned()),
            "{iri} is still a LIVE slice in the repository, so its frozen floor rows must \
             keep their full teeth; RETIRED_SLICES exempts only slices that are gone (the \
             recorded reason was: {reason})"
        );
        assert!(
            !reason.trim().is_empty(),
            "{iri} must carry a recorded reason, or the exemption is a silent hole"
        );
    }
}
