// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Value-preservation guard for the projection-vocabulary ratchet.
//!
//! The committed per-(slice, vocabulary) ungrounded-residue ceilings and the
//! guarded projection-vocabulary registry live as ontology-resident
//! `gmeow:ProjectionCeilingCommitment` / `gmeow:ProjectionVocabulary` individuals
//! in `slices/core/slice-quality-rubric/module.ttl`. This test freezes the
//! grandfathered values (the golden TSVs under `tests/fixtures/`) and asserts the
//! loaded rubric reproduces every one of them EXACTLY — same slice, same vocab
//! prefix, and an integer-exact count. It is permanent and self-contained: it
//! depends on the golden copies, never on the generated `governance/*.tsv`, so a
//! migration that dropped or perturbed a committed ceiling can never pass
//! silently.

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
/// dropped or perturbed ceiling must stay detectable forever. But a slice that no
/// longer exists has no commitment to reproduce, so its frozen rows are skipped
/// here rather than deleted there. Each entry names the slice and why it went,
/// so the exemption is a recorded decision rather than a silent hole.
const RETIRED_SLICES: &[(&str, &str)] = &[(
    "https://blackcatinformatics.ca/gmeow/slices/procedures",
    "process-model slice superseded by the canonical logic: prescription/enactment \
     spine; its terms were removed rather than kept as a second source of truth",
)];

/// True when the row belongs to a slice retired since the freeze.
fn is_retired(slice: &str) -> bool {
    RETIRED_SLICES.iter().any(|(iri, _)| *iri == slice)
}

/// The frozen rows that still name a live slice.
fn live_rows(rows: &[String]) -> Vec<&String> {
    rows.iter()
        .filter(|r| !is_retired(r.split('\t').next().unwrap_or("")))
        .collect()
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
fn every_committed_projection_ceiling_is_reproduced_integer_exactly() {
    let ceilings = gmeow_slice_quality::load_repo_ceilings(&repo_root())
        .expect("the committed rubric ceilings must load");
    let rows = golden_rows("projection-ceilings.golden.tsv");
    assert_eq!(
        rows.len(),
        141,
        // 141, not 140: the math/obi row was ADDED when OBI's catalog ownership moved to
        // the logic: grounding slice, where its planned-process backbone belongs and where
        // this vocabulary was already declared subsumed. math: keeps its one pre-existing
        // OBI_0200000 data-transformation bridge, which is now an off-owner residue of
        // exactly one and is bounded here. Extending the record is what a newly bounded
        // residue looks like; no committed historical value moved.
        "the frozen projection-ceiling golden has 141 rows"
    );
    let live = live_rows(&rows);
    assert_eq!(
        ceilings.ceilings.len(),
        live.len(),
        "the loaded ProjectionCeilingCommitment set must have exactly the frozen row count \
         for every slice that still exists"
    );

    for row in live {
        let cols: Vec<&str> = row.split('\t').collect();
        assert_eq!(
            cols.len(),
            3,
            "golden ceiling row is <slice-iri>\\t<vocab-prefix>\\t<count>: {row:?}"
        );
        let (slice, vocab_prefix, count_str) = (cols[0], cols[1], cols[2]);
        let want_count = count_str
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("golden count {count_str:?} parses as u64"));

        let found = ceilings
            .ceilings
            .iter()
            .find(|c| c.slice == slice && c.vocab_prefix == vocab_prefix)
            .unwrap_or_else(|| {
                panic!("no ProjectionCeilingCommitment for slice {slice} vocab {vocab_prefix}")
            });
        assert_eq!(
            found.count, want_count,
            "slice {slice} vocab {vocab_prefix}: loaded ceiling {} must integer-match the frozen golden {want_count}",
            found.count
        );
    }
}

#[test]
fn every_guarded_projection_vocabulary_is_reproduced_exactly() {
    let ceilings = gmeow_slice_quality::load_repo_ceilings(&repo_root())
        .expect("the committed rubric ceilings must load");
    let rows = golden_rows("projection-vocabularies.golden.tsv");
    assert_eq!(
        rows.len(),
        37,
        "the frozen projection-vocabulary golden has 37 rows"
    );
    assert_eq!(
        ceilings.vocabularies.len(),
        rows.len(),
        "the loaded ProjectionVocabulary set must have exactly the frozen row count"
    );

    for row in &rows {
        let cols: Vec<&str> = row.split('\t').collect();
        assert_eq!(
            cols.len(),
            5,
            "golden vocabulary row is <prefix>\\t<namespaces>\\t<count-kind-local>\\t<default-ceiling>\\t<preservation-local>: {row:?}"
        );
        let (prefix, namespaces_str, count_kind_local, default_ceiling_str, preservation_local) =
            (cols[0], cols[1], cols[2], cols[3], cols[4]);
        let want_default_ceiling = default_ceiling_str.parse::<u64>().unwrap_or_else(|_| {
            panic!("golden default ceiling {default_ceiling_str:?} parses as u64")
        });

        let found = ceilings
            .vocabularies
            .iter()
            .find(|v| v.prefix == prefix)
            .unwrap_or_else(|| panic!("no ProjectionVocabulary for prefix {prefix}"));

        let got_namespaces = found.namespaces.join(",");
        assert_eq!(
            got_namespaces, namespaces_str,
            "prefix {prefix}: loaded namespaces {got_namespaces:?} must match the frozen golden {namespaces_str:?}"
        );
        assert_eq!(
            found.count_kind.as_local(),
            count_kind_local,
            "prefix {prefix}: loaded count-kind must match the frozen golden {count_kind_local}"
        );
        assert_eq!(
            found.default_ceiling, want_default_ceiling,
            "prefix {prefix}: loaded default ceiling {} must match the frozen golden {want_default_ceiling}",
            found.default_ceiling
        );
        assert_eq!(
            local_name(&found.preservation),
            preservation_local,
            "prefix {prefix}: loaded preservation {} must match the frozen golden {preservation_local}",
            found.preservation
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
            "{iri} is still a LIVE slice in the repository, so its frozen ceiling rows must \
             keep their full teeth; RETIRED_SLICES exempts only slices that are gone (the \
             recorded reason was: {reason})"
        );
        assert!(
            !reason.trim().is_empty(),
            "{iri} must carry a recorded reason, or the exemption is a silent hole"
        );
    }
}
