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
        33,
        "the frozen projection-ceiling golden has 33 rows"
    );
    assert_eq!(
        ceilings.ceilings.len(),
        rows.len(),
        "the loaded ProjectionCeilingCommitment set must have exactly the frozen row count"
    );

    for row in &rows {
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
        10,
        "the frozen projection-vocabulary golden has 10 rows"
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
