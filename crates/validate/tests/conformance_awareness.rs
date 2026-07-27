// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from `slices/core/awareness/tests/test_awareness.py`.
//!
//! Two exact-set-equality invariants the module-scoped SPARQL-`ASK` structural-cell
//! harness cannot express — an `ASK` asserts a value EXISTS, never that a set is
//! EXACTLY `{…}` — one of which reads `manifest.ttl`, a file the structural harness
//! never loads (its store is `module.ttl` + `examples/` only). Both were the residue
//! left in Python; they now live natively here, strengthened to pin the set in BOTH
//! directions.
//!
//! * `level_ranks_are_exactly_zero_through_five` — the six `gmeow:AwarenessLevel`
//!   value-vocabulary individuals each carry exactly one `gmeow:levelRank`, and the six
//!   ranks are EXACTLY `{0,1,2,3,4,5}` (high arousal → low). Hardened beyond the Python
//!   original: exactly those six subjects carry `gmeow:levelRank` in the whole merged
//!   ontology, so a stray future rank on another subject is caught.
//! * `slice_depends_on_is_exactly_kernel_logic_and_temporal` — the awareness `manifest.ttl`
//!   declares `gmeow:sliceDependsOn` EXACTLY `{kernel, logic, temporal}` (dependency hygiene:
//!   mentation / metacognition / imagination are consumed by reference, never declared).

mod conformance_support;
use conformance_support::*;

use std::collections::BTreeSet;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// The six seeded `gmeow:AwarenessLevel` value-vocabulary individuals (high → low).
const LEVELS: &[&str] = &[
    "levelHyperalert",
    "levelAlert",
    "levelRelaxed",
    "levelDrowsy",
    "levelObtunded",
    "levelUnresponsive",
];

/// Twin of `test_level_ranks_are_zero_through_five`: each level individual carries a
/// single `gmeow:levelRank`, and the six ranks are EXACTLY `{0,1,2,3,4,5}`.
#[test]
fn level_ranks_are_exactly_zero_through_five() {
    let g = GraphStore::ontology();
    let rank_pred = gm("levelRank");

    let mut ranks: BTreeSet<String> = BTreeSet::new();
    for level in LEVELS {
        let values = g.objects_lex(&gm(level), &rank_pred);
        assert_eq!(
            values.len(),
            1,
            "{} must carry exactly one gmeow:levelRank, got {values:?}",
            gm(level)
        );
        ranks.extend(values);
    }

    let expected: BTreeSet<String> = ["0", "1", "2", "3", "4", "5"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    assert_eq!(
        ranks, expected,
        "the six gmeow:levelRank values must be exactly {{0,1,2,3,4,5}}, got {ranks:?}"
    );

    // Hardening: EXACTLY these six subjects carry gmeow:levelRank in the whole merged
    // ontology — the superset half the Python original (which iterated only the known
    // six) could not assert. A stray future rank on any other subject reds this test.
    let (_vars, rows) = g.select(
        &[],
        &format!("SELECT DISTINCT ?s WHERE {{ ?s <{rank_pred}> ?r }}"),
    );
    let subjects: BTreeSet<String> = rows
        .iter()
        .filter_map(|row| {
            row.first()
                .and_then(|t| t.as_ref())
                .and_then(|t| t.as_iri())
                .map(|iri| iri.to_owned())
        })
        .collect();
    let expected_subjects: BTreeSet<String> = LEVELS.iter().map(|l| gm(l)).collect();
    assert_eq!(
        subjects, expected_subjects,
        "exactly the six level individuals may carry gmeow:levelRank; got {subjects:?}"
    );
}

/// Twin of `test_manifest_depends_only_on_kernel_and_temporal`: the awareness
/// `manifest.ttl` declares `gmeow:sliceDependsOn` EXACTLY `{kernel, logic, temporal}`.
///
/// `logic` is the grounding vocabulary the slice's own `logic:PropertyCharacteristicAssertion`
/// carriers are written in (`gmeow:awarenessScalarFunctionality`): a characteristic of one of
/// THIS slice's properties is authored in THIS slice, so the slice consumes the `logic:`
/// assertion vocabulary. Domain → grounding is the sanctioned direction; the reverse
/// (a grounding slice carrying the record) is what `docs/GROUNDING.md`'s tier rule forbids.
#[test]
fn slice_depends_on_is_exactly_kernel_logic_and_temporal() {
    let manifest = repo_root().join("slices/core/awareness/manifest.ttl");
    let m = GraphStore::parse_ttl_file(&manifest);

    let deps = m.objects(&gm("slices/awareness"), &gm("sliceDependsOn"));
    let expected: BTreeSet<String> = [
        gm("slices/kernel"),
        gm("slices/logic"),
        gm("slices/temporal"),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        deps, expected,
        "awareness sliceDependsOn must be exactly {{kernel, logic, temporal}}, got {deps:?}"
    );
}
