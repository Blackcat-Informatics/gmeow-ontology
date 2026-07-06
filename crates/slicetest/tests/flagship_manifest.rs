// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The flagship acceptance-manifest cross-check (math grounding capstone).
//!
//! The `math:FlagshipScenario` manifest is gated on three surfaces. SHACL
//! (`math:FlagshipScenarioShape`) and a structural ASK (`ex:saFlagshipCoverage`)
//! prove the five scenarios are present and fully linked to a real
//! `math:MathConformanceFailure` subclass — but neither can resolve the
//! `math:demonstratedByCompetency` reference, because the `gmeow:CompetencyQuestion`
//! individuals live in `tests/competency.ttl`, which the module/examples-scoped
//! validators never load (the dataset split, documented in the manifest).
//!
//! This test is that missing surface. It unions the flagship manifest with the
//! competency corpus and asserts, for each scenario:
//!
//! * its competency reference resolves to a real `gmeow:CompetencyQuestion` that
//!   carries a `gmeow:cqExpectRow` expectation (i.e. it is a GREEN, pinned gate,
//!   not a dangling IRI), and its `gmeow:cqQueryFile` exists on disk;
//! * its worked example and its counter-example both exist on disk.
//!
//! It also pins the coverage set: exactly the five canonical scenarios, no more, no
//! fewer. Flip any competency IRI to a non-registered value, delete a referenced
//! file, or add/drop a scenario, and this test fails.

use std::collections::BTreeSet;
use std::path::Path;

use gmeow_slicetest::native_query::{dataset_from_file, render_term, select, union};
use gmeow_slicetest::paths::{example_file, query_file, slices_root};

/// The five canonical flagship-scenario IRIs the epic's depth bar requires.
const CANONICAL: [&str; 5] = [
    "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/e8Symmetry",
    "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/homomorphicEncryption",
    "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/proofAsProcess",
    "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/rBridge",
    "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/aiSelfStructure",
];

fn math_slice_dir() -> std::path::PathBuf {
    slices_root().join("grounding").join("math")
}

/// Render one bound cell to its N-Triples lexical form, hard-failing on an unbound
/// slot — every column projected below is required, so an unbound cell is a bug in
/// the manifest, not an expected optional.
fn rendered(cell: &Option<purrdf::TermValue>) -> String {
    render_term(cell.as_ref().expect("required column was unbound"))
}

/// Strip the `<...>` of a rendered IRI, panicking if the term is not an IRI.
fn as_iri(rendered: &str) -> &str {
    rendered
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or_else(|| panic!("expected an IRI term, got {rendered}"))
}

/// Extract the lexical form of a rendered string literal (`"lexical"^^<datatype>`).
/// The manifest's path literals contain no quotes or escapes, so the first closing
/// quote is the lexical boundary.
fn as_literal(rendered: &str) -> String {
    let inner = rendered
        .strip_prefix('"')
        .unwrap_or_else(|| panic!("expected a literal term, got {rendered}"));
    let end = inner
        .find('"')
        .unwrap_or_else(|| panic!("literal term missing closing quote: {rendered}"));
    inner[..end].to_owned()
}

#[test]
fn every_flagship_scenario_is_wired_to_a_green_competency_and_real_artifacts() {
    let slice = math_slice_dir();
    let manifest_path = slice.join("examples").join("flagship-acceptance.ttl");
    let competency_path = slice.join("tests").join("competency.ttl");

    let manifest = dataset_from_file(&manifest_path).expect("parse flagship-acceptance.ttl");
    let competency = dataset_from_file(&competency_path).expect("parse competency.ttl");
    let dataset = union(&[manifest, competency]);

    // Every scenario and its four links, in one pass.
    let scenarios = select(
        &dataset,
        r"
        PREFIX math: <https://blackcatinformatics.ca/math/>
        SELECT ?s ?ex ?cq ?ce ?fc WHERE {
            ?s a math:FlagshipScenario ;
               math:demonstratedByExample   ?ex ;
               math:demonstratedByCompetency ?cq ;
               math:guardedByCounterExample ?ce ;
               math:enforcesFailureClass    ?fc .
        }",
    )
    .expect("scenario query runs");

    let mut seen = BTreeSet::new();
    for row in &scenarios.rows {
        let scenario = as_iri(&rendered(&row[0])).to_owned();
        let example = as_literal(&rendered(&row[1]));
        let competency_iri = as_iri(&rendered(&row[2])).to_owned();
        let counter_example = as_literal(&rendered(&row[3]));
        // ?fc must be an IRI; the subclass check is the structural/SHACL gate's job.
        let _failure_class = as_iri(&rendered(&row[4]));

        // The worked example and the counter-example must exist on disk.
        let example_abs = example_file(&slice, &example);
        assert!(
            example_abs.exists(),
            "{scenario}: math:demonstratedByExample {example} does not exist at {}",
            example_abs.display()
        );
        let counter_abs = example_file(&slice, &counter_example);
        assert!(
            counter_abs.exists(),
            "{scenario}: math:guardedByCounterExample {counter_example} does not exist at {}",
            counter_abs.display()
        );

        // The competency reference must resolve to a real, pinned (cqExpectRow)
        // competency question whose query file exists.
        assert_competency_is_green(&dataset, &scenario, &competency_iri);

        assert!(
            seen.insert(scenario.clone()),
            "duplicate scenario {scenario}"
        );
    }

    let expected: BTreeSet<String> = CANONICAL.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(
        seen, expected,
        "the flagship coverage set must be EXACTLY the five canonical scenarios"
    );
}

/// Assert `competency_iri` names a `gmeow:CompetencyQuestion` carrying at least one
/// `gmeow:cqExpectRow` and a `gmeow:cqQueryFile` that exists on disk.
fn assert_competency_is_green(
    dataset: &std::sync::Arc<purrdf::RdfDataset>,
    scenario: &str,
    competency_iri: &str,
) {
    let q = format!(
        r"
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        SELECT ?qf ?row WHERE {{
            <{competency_iri}> a gmeow:CompetencyQuestion ;
                gmeow:cqQueryFile ?qf ;
                gmeow:cqExpectRow ?row .
        }}"
    );
    let hits = select(dataset, &q).expect("competency resolution query runs");
    assert!(
        !hits.rows.is_empty(),
        "{scenario}: math:demonstratedByCompetency {competency_iri} does not resolve to a \
         gmeow:CompetencyQuestion carrying gmeow:cqQueryFile + gmeow:cqExpectRow \
         (a dangling or unpinned competency reference)"
    );
    // Every row shares the same cqQueryFile; check the first exists on disk.
    let query_rel = as_literal(&rendered(&hits.rows[0][0]));
    let query_abs = query_file(&query_rel);
    assert!(
        Path::new(&query_abs).exists(),
        "{scenario}: cqQueryFile {query_rel} for {competency_iri} does not exist at {}",
        query_abs.display()
    );
}
