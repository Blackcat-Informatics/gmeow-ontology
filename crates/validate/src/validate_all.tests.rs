// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::collections::{BTreeSet, HashSet};

use purrdf::{DatasetView, GraphMatch, parse_dataset};

/// Write `contents` to `name` inside a fresh RAII temp directory.
///
/// The returned [`tempfile::TempDir`] owns the directory: it is removed on
/// drop, including on panic and early return. Bind it to a named `_tmp`
/// (never a bare `_`, which would drop it immediately) so it outlives the
/// path. The file *name* is preserved because the validation run dispatches
/// on the `.ttl` extension.
fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(name);
    std::fs::write(&path, contents).unwrap();
    (dir, path)
}

/// The per-example `base ∪ example` merge dedups shared base quads and adds the
/// example-only quads, leaving the base unaffected (each example merges into a
/// fresh dataset; there is no shared mutable store to leak into).
#[test]
fn native_validation_cache_key_preserves_sorted_decoded_segment_heads() {
    let low = [0x01; 32];
    let high = [0xab; 32];
    let mut lookaside = purrdf::RdfLookaside::default();
    for (index, head) in [high, low].into_iter().enumerate() {
        lookaside.segments.push(purrdf::RdfSegmentRecord {
            index,
            head: Some(purrdf::gts::wire::hex(&head)),
            profile: None,
            claimed_streamable: false,
            covered: 0,
            tail: 0,
        });
    }
    let mut envelope = purrdf::RdfEnvelope::new(lookaside);
    let expected = ValidationCache::cache_key(&[&low, &high]);
    assert_eq!(native_segment_heads_cache_key(&envelope).unwrap(), expected);
    envelope.lookaside.segments.reverse();
    assert_eq!(native_segment_heads_cache_key(&envelope).unwrap(), expected);
    envelope.lookaside.segments[0].head = Some("malformed".to_owned());
    assert!(native_segment_heads_cache_key(&envelope).is_err());
}

#[test]
fn example_merge_unions_base_and_example() {
    let base = parse_dataset(
        b"@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
        "text/turtle",
        None,
    )
    .unwrap();
    let base_quads: Vec<purrdf::RdfQuad> = base.owned_quads().collect();

    // An example carrying one duplicate of the base quad plus one new quad.
    let (_tmp, example_path) = write_tmp(
        "gmeow_validate_example_merge.ttl",
        "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\nex:c ex:p ex:d .\n",
    );
    let example = store::parse_file_dataset(&example_path).unwrap();

    let mut builder = RdfDatasetBuilder::new();
    for q in &base_quads {
        builder.push_owned_quad(q);
    }
    builder.push_dataset(&example);
    let merged = builder.freeze().unwrap();
    // The duplicate base quad collapses; the example-only quad is added → 2 total.
    assert_eq!(merged.quad_count(), 2, "duplicate base quad must dedup");
    // The base dataset is unchanged by the merge.
    assert_eq!(base.quad_count(), 1, "base dataset must be untouched");
    assert_eq!(
        merged
            .quads_for_pattern(None, None, None, GraphMatch::Default)
            .count(),
        2
    );
}

#[test]
fn example_validation_rechecks_remote_constraint_dependencies() {
    let base = parse_dataset(
        b"@prefix ex: <https://example.org/> . ex:agreement ex:hasPart ex:part .",
        "text/turtle",
        None,
    )
    .unwrap();
    let base = purrdf::shapes::engine::project_dataset(&base).unwrap();
    // These are projected constraints supplied to the GMEOW orchestrator.
    // This test owns example selection and finding attribution, not SPARQL
    // or SHACL conformance (which is tested in PurRDF).
    for constraint in [
        r#"sh:sparql [ sh:select '''
                SELECT $this WHERE {
                    <https://example.org/policy> <https://example.org/requiresReview> true .
                    FILTER NOT EXISTS { $this <https://example.org/reviewedBy> ?reviewer }
                }
            ''' ]"#,
        "sh:property [ sh:path (ex:hasPart ex:reviewer) ; sh:maxCount 1 ]",
    ] {
        let shapes = purrdf::shapes::engine::parse_shapes(
            &format!(
                "@prefix sh: <http://www.w3.org/ns/shacl#> . \
                     @prefix ex: <https://example.org/> . \
                     ex:AgreementShape a sh:NodeShape ; sh:targetNode ex:agreement ; {constraint} ."
            ),
            None,
        )
        .unwrap();
        let shapes = purrdf::shapes::engine::PreparedShapes::new(Arc::new(shapes));
        let (_control, control) = write_tmp("control.ttl", "");
        assert!(
            run_example_shacl(
                &base,
                &shapes,
                &FailureClassIndex::empty(),
                &control,
                "control"
            )
            .unwrap()
            .is_empty()
        );
        let (_changed, changed) = write_tmp(
            "changed.ttl",
            "@prefix ex: <https://example.org/> . \
                 ex:policy ex:requiresReview true . ex:part ex:reviewer ex:alice, ex:bob .",
        );
        let findings = run_example_shacl(
            &base,
            &shapes,
            &FailureClassIndex::empty(),
            &changed,
            "changed",
        )
        .unwrap();
        assert_eq!(
            findings.len(),
            1,
            "remote change must recheck agreement: {findings:?}"
        );
        assert_eq!(findings[0].severity, Severity::Error);
        assert_eq!(findings[0].locations[0].path.as_deref(), Some("changed"));
    }
}

#[test]
fn example_validation_includes_targets_added_by_canonical_subsumption() {
    let base = parse_dataset(
        b"@prefix ex: <https://example.org/> . ex:person a ex:Child .",
        "text/turtle",
        None,
    )
    .unwrap();
    let base = purrdf::shapes::engine::project_dataset(&base).unwrap();
    let shapes = purrdf::shapes::engine::parse_shapes(
        "@prefix sh: <http://www.w3.org/ns/shacl#> . \
             @prefix ex: <https://example.org/> . \
             ex:ParentShape a sh:NodeShape ; sh:targetClass ex:Parent ; \
             sh:property [ sh:path ex:required ; sh:minCount 1 ] .",
        None,
    )
    .unwrap();
    let shapes = purrdf::shapes::engine::PreparedShapes::new(Arc::new(shapes));
    let (_tmp, example) = write_tmp(
        "subclass.ttl",
        "@prefix ex: <https://example.org/> . \
             @prefix logic: <https://blackcatinformatics.ca/logic/> . \
             ex:Child logic:subClassOf ex:Parent .",
    );
    let findings = run_example_shacl(
        &base,
        &shapes,
        &FailureClassIndex::empty(),
        &example,
        "subclass",
    )
    .unwrap();
    assert_eq!(findings.len(), 1, "newly targeted parent must be checked");
    assert_eq!(findings[0].severity, Severity::Error);
}

fn minimal_gts_bytes() -> Vec<u8> {
    use purrdf::gts::model::{Term, TermKind};
    use purrdf::gts::writer::Writer;

    let mut graph = purrdf::gts::model::Graph::default();
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some("https://example.org/a".to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    });
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some("https://example.org/p".to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    });
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some("https://example.org/b".to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    });
    graph.quads.push((0, 1, 2, None));

    let writer = Writer::deterministic(&graph, "gmeow-validate-test")
        .expect("deterministic GTS writer must succeed");
    writer.to_bytes()
}

#[test]
fn deep_semantic_pass_flags_inconsistency_and_consistency() {
    // An inconsistent bundle: A⊑B, A⊑C, B disjointWith C, x:A forces x into
    // owl:Nothing — the shared ReasoningResult reports information=both.
    let inconsistent = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .
ex:x rdf:type ex:A .
";
    let bytes = gts_bytes_from_turtle(inconsistent);
    let mut report = Report::new("validate");
    deep_semantic_findings_prepared(&bytes, &mut report, &verification_fixture::verification())
        .expect("deep pass must run");
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == "validate.deep.inconsistent"),
        "the deep pass must flag the inconsistency: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );

    // A consistent bundle: A⊑B, x:A. No clash → a consistency note, no error.
    let consistent = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:x rdf:type ex:A .
";
    let bytes = gts_bytes_from_turtle(consistent);
    let mut report = Report::new("validate");
    deep_semantic_findings_prepared(&bytes, &mut report, &verification_fixture::verification())
        .expect("deep pass must run");
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == "validate.deep.consistent"),
        "a consistent bundle must record the consistency note"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.code == "validate.deep.inconsistent"),
        "a consistent bundle must NOT flag inconsistency"
    );
}

#[test]
fn fold_categorizes_permitted_versus_forbidden_glut() {
    // A real within-world glut, reasoned from an inconsistent fixture.
    let inconsistent = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .
ex:x rdf:type ex:A .
";
    let bytes = gts_bytes_from_turtle(inconsistent);
    let bundle = purrdf::import_gts_events(&bytes).expect("gts read");
    let result = gmeow_logic::reason::reason_all(
        gmeow_logic::reason::prepare_reasoning_input(bundle.dataset.as_ref())
            .expect("fixture ingress"),
        &gmeow_logic::reasoning_graphs::object_level_domains().expect("bundle role selection"),
    )
    .expect("reason");
    assert!(!result.is_consistent(), "the fixture must produce a glut");
    let explanations = gmeow_logic::explain::explanations_for_result(&result)
        .expect("explain skeletons must build for a real verdict");

    // FORBIDDEN (classical): an Error categorized ContradictionWitness; gate fails.
    let mut forbidden = Report::new("validate");
    fold_reasoning_result(
        &result,
        ContradictionPolicy::ForbidGapAndGlut,
        &explanations,
        &mut forbidden,
    )
    .expect("fold must locate every witness derivation");
    let f = forbidden
        .findings
        .iter()
        .find(|f| f.code == "validate.deep.inconsistent")
        .expect("forbidden glut must emit a deep.inconsistent error");
    assert_eq!(f.severity, Severity::Error);
    assert_eq!(f.category, Some(FindingCategory::ContradictionWitness));
    assert!(!forbidden.ok(), "a forbidden glut must fail the gate");

    // PERMITTED (glut-admitting): a Warning categorized PermittedEpistemicConflict;
    // the gate stays green — the load-bearing acceptance criterion (c).
    let mut permitted = Report::new("validate");
    fold_reasoning_result(
        &result,
        ContradictionPolicy::ForbidGap,
        &explanations,
        &mut permitted,
    )
    .expect("fold must locate every witness derivation");
    assert!(
        !permitted
            .findings
            .iter()
            .any(|f| f.code == "validate.deep.inconsistent"),
        "a permitted glut must NOT emit the forbidden inconsistency error"
    );
    let p = permitted
        .findings
        .iter()
        .find(|f| f.code == "validate.deep.permitted-conflict")
        .expect("permitted glut must emit a permitted-conflict warning");
    assert_eq!(p.severity, Severity::Warning);
    assert_eq!(
        p.category,
        Some(FindingCategory::PermittedEpistemicConflict)
    );
    assert!(
        permitted.ok(),
        "a permitted, disclosed contradiction must NOT fail the gate"
    );
}

/// The inconsistent bundle every derivation-attach test reasons over: `x : A`,
/// `A ⊑ B`, `A ⊑ C`, `B ⊐⊏ C` forces `x` into `owl:Nothing`.
const INCONSISTENT_TTL: &str = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .
ex:x rdf:type ex:A .
";

/// A forbidden contradiction finding carries the explain-skeleton
/// cited-quad-reifier derivation (`derived_from_quads`) of its clash quad, and
/// leaves the finding-fingerprint edges (`antecedents`/`root_cause`) untouched —
/// the namespace guard: the two edges are NEVER conflated.
#[test]
fn deep_inconsistent_finding_carries_derivation_not_antecedents() {
    let bytes = gts_bytes_from_turtle(INCONSISTENT_TTL);
    let bundle = purrdf::import_gts_events(&bytes).expect("gts read");
    let result = gmeow_logic::reason::reason_all(
        gmeow_logic::reason::prepare_reasoning_input(bundle.dataset.as_ref())
            .expect("fixture ingress"),
        &gmeow_logic::reasoning_graphs::object_level_domains().expect("bundle role selection"),
    )
    .expect("reason");
    assert!(!result.is_consistent(), "the fixture must produce a glut");
    let explanations = gmeow_logic::explain::explanations_for_result(&result)
        .expect("explain skeletons must build for a real verdict");

    let mut report = Report::new("validate");
    fold_reasoning_result(
        &result,
        ContradictionPolicy::ForbidGapAndGlut,
        &explanations,
        &mut report,
    )
    .expect("fold must locate every witness derivation");

    let finding = report
        .findings
        .iter()
        .find(|f| f.code == "validate.deep.inconsistent")
        .expect("a forbidden glut must emit a deep.inconsistent error");

    assert!(
        !finding.derived_from_quads.is_empty(),
        "the reasoned-quad verdict must carry its explain-skeleton derivation"
    );
    // The cited-IRI skeleton names the clash quad's own reifier and its world —
    // the load-bearing logic-world coordinates of the derivation.
    assert!(
        finding
            .derived_from_quads
            .iter()
            .any(|iri| iri.starts_with("https://blackcatinformatics.ca/gmeow/reifier/")),
        "derived_from_quads must cite at least one logic-world quad reifier; got {:?}",
        finding.derived_from_quads
    );
    assert!(
        finding
            .derived_from_quads
            .contains(&"https://blackcatinformatics.ca/gmeow/graph/rl-default".to_owned()),
        "the derivation is cited within its world; got {:?}",
        finding.derived_from_quads
    );
    // Namespace guard: the finding-fingerprint edges stay empty — a quad reifier
    // must NEVER be written into antecedents/root_cause.
    assert!(
        finding.antecedents.is_empty(),
        "antecedents (finding-fingerprint IRIs) must stay empty"
    );
    assert!(
        finding.root_cause.is_none(),
        "root_cause (finding-fingerprint IRI) must stay unset"
    );
}

/// The absent-witness invariant: a verdict names a witness whose clash quad has
/// no locatable explain skeleton → `fold_reasoning_result` HARD-FAILS with
/// [`WitnessDerivationMissing`], never a silent (or advisory-Note) attach. Here
/// the real inconsistent result is folded with an EMPTY explanation set, so no
/// witness can be located — the same shape as a verdict referencing a quad the
/// result does not carry.
#[test]
fn fold_hard_fails_when_witness_derivation_absent() {
    let bytes = gts_bytes_from_turtle(INCONSISTENT_TTL);
    let bundle = purrdf::import_gts_events(&bytes).expect("gts read");
    let result = gmeow_logic::reason::reason_all(
        gmeow_logic::reason::prepare_reasoning_input(bundle.dataset.as_ref())
            .expect("fixture ingress"),
        &gmeow_logic::reasoning_graphs::object_level_domains().expect("bundle role selection"),
    )
    .expect("reason");
    assert!(!result.is_consistent(), "the fixture must produce a glut");

    let mut report = Report::new("validate");
    let err = fold_reasoning_result(
        &result,
        ContradictionPolicy::ForbidGapAndGlut,
        &[],
        &mut report,
    )
    .expect_err("an unlocatable witness derivation must HARD-FAIL the fold");
    assert!(
        err.message.contains("contradiction witness")
            && err.message.contains("no explain skeleton"),
        "the invariant violation must name the unlocatable witness; got {:?}",
        err.message
    );
}

/// Build canonical GTS bytes from a Turtle string for the deep-pass test.
fn gts_bytes_from_turtle(ttl: &str) -> Vec<u8> {
    let dataset =
        purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse test turtle");
    // gmeow-test-input: synthetic-only
    purrdf::gts_write::to_gts(
        &dataset,
        &purrdf::RdfLookaside::default(),
        "gmeow-validate-deep-test",
    )
    .expect("encode GTS bytes")
}

/// A bundle that BOTH reasons to a within-world glut AND declares a
/// `logic:ReasoningContract` whose `logic:admissibleValuation` is the supplied
/// policy local name (e.g. `ForbidGap` admits a glut; `ForbidGapAndGlut` forbids
/// it). The contract is real RDF in the bundle, so `deep_semantic_findings`
/// resolves the governing policy off the bundle exactly as production does.
fn glut_bundle_with_contract(valuation_local: &str) -> Vec<u8> {
    let ttl = format!(
        "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .
ex:x rdf:type ex:A .
ex:governingContract rdf:type logic:ReasoningContract ;
    logic:admissibleValuation logic:{valuation_local} .
"
    );
    gts_bytes_from_turtle(&ttl)
}

/// H3(a)+(b)+(c): the FULL deep path on a real glut-admitting bundle. The policy
/// comes from the bundle's declared contract (proving the C1 wiring works on real
/// data): a `ForbidGap` (glut-admitting) contract turns the within-world glut into
/// a PERMITTED, disclosed conflict — a non-error finding that keeps the gate
/// GREEN — and a coherence certificate is attached to the report metadata.
#[test]
fn deep_pass_permitted_glut_stays_green_with_certificate() {
    let bytes = glut_bundle_with_contract("ForbidGap");
    let mut report = Report::new("validate");
    deep_semantic_findings_prepared(&bytes, &mut report, &verification_fixture::verification())
        .expect("deep pass must run");

    // (a) a PermittedEpistemicConflict finding at NON-error severity.
    let permitted = report
        .findings
        .iter()
        .find(|f| f.category == Some(FindingCategory::PermittedEpistemicConflict))
        .expect("a glut-admitting contract must emit a permitted-conflict finding");
    assert_ne!(
        permitted.severity,
        Severity::Error,
        "a permitted, disclosed conflict must NOT be an error"
    );

    // (b) NO error-severity finding from the conflict — the gate stays GREEN.
    assert!(
        report.ok(),
        "a permitted glut under its declared contract must keep the gate green: {:?}",
        report.legacy_errors()
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.code == "validate.deep.inconsistent"),
        "no forbidden inconsistency error must be emitted"
    );

    // (c) a coherence certificate is present in report metadata.
    assert!(
        report.metadata.contains_key("coherence_certificate"),
        "the deep pass must attach a coherence certificate"
    );
    let cert = report.metadata["coherence_certificate"]
        .as_str()
        .expect("certificate is serialized as N-Quads string");
    assert!(
        cert.contains("permittedConflictWitness"),
        "the certificate must disclose the permitted conflict: {cert}"
    );
}

/// H3 (forbidding side): the SAME glut under a glut-FORBIDDING declared contract
/// (`ForbidGapAndGlut`) yields an Error-severity ContradictionWitness (gate fails),
/// and the release-lane certificate build REFUSES (Err) on the same bundle.
#[test]
fn deep_pass_forbidden_glut_fails_and_release_refuses() {
    let bytes = glut_bundle_with_contract("ForbidGapAndGlut");
    let mut report = Report::new("validate");
    deep_semantic_findings_prepared(&bytes, &mut report, &verification_fixture::verification())
        .expect("deep pass must run");

    let witness = report
        .findings
        .iter()
        .find(|f| f.category == Some(FindingCategory::ContradictionWitness))
        .expect("a glut-forbidding contract must emit a contradiction witness");
    assert_eq!(witness.severity, Severity::Error);
    assert!(!report.ok(), "a forbidden glut must fail the gate");

    // The release lane reasons over the SAME bundle bytes under the SAME
    // bundle-resolved policy, then REFUSES to sign an incoherent bundle (hard-fail,
    // no DEFAULT papering-over). gmeow-pipeline cannot be imported here (it depends
    // on gmeow-validate — a cycle), so exercise the exact decision the release lane
    // makes: resolve the policy off the bundle, build the outcome, and assert it is
    // refused (release.rs returns Err on `outcome.is_refused()`).
    let bundle = purrdf::import_gts_events(&bytes).expect("gts read");
    let result = gmeow_logic::reason::reason_all(
        gmeow_logic::reason::prepare_reasoning_input(bundle.dataset.as_ref())
            .expect("fixture ingress"),
        &gmeow_logic::reasoning_graphs::object_level_domains().expect("bundle role selection"),
    )
    .expect("reason");
    let policy =
        ContradictionPolicy::resolve_from_dataset(bundle.dataset.as_ref()).expect("policy");
    assert_eq!(
        policy,
        ContradictionPolicy::ForbidGapAndGlut,
        "the bundle's declared contract must resolve to the glut-forbidding policy"
    );
    let projection_loss_codes: BTreeSet<String> = PROJECTION_CODECS
        .iter()
        .flat_map(|&to| {
            pair_loss_ledger("gts", to)
                .entries()
                .iter()
                .map(|e| e.code.to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    let outcome = gmeow_logic::certificate::CoherenceOutcome::from_reasoning_result(
        &result,
        purrdf::gts::writer::digest_string(&bytes),
        gmeow_logic::certificate::per_graph_axiom_hashes(
            bundle.dataset.as_ref(),
            purrdf::gts::writer::digest_string,
        ),
        policy,
        "2026-06-28T00:00:00Z",
        projection_loss_codes,
    )
    .expect("outcome build");
    assert!(
        outcome.is_refused(),
        "the release lane must refuse to sign a bundle carrying a forbidden integrity violation"
    );
}

fn minimal_lint_config() -> LintConfig {
    LintConfig {
        namespace: "https://blackcatinformatics.ca/gmeow/".to_owned(),
        ontology_iri: "https://blackcatinformatics.ca/gmeow".to_owned(),
        selector_tokens: BTreeSet::new(),
        core_slice_iris: HashSet::new(),
        annotation_predicates: HashSet::new(),
    }
}

#[test]
fn run_with_gts_bytes_succeeds_with_empty_source_paths() {
    let bytes = minimal_gts_bytes();
    let options = ValidateOptions {
        gts_bytes: Some(bytes),
        ..ValidateOptions::default()
    };

    let run = ValidationRun::run(&[], "", "", "", &minimal_lint_config(), &options)
        .expect("ValidationRun::run with gts_bytes must succeed");

    assert!(
        run.errors().is_empty(),
        "unexpected errors: {:?}",
        run.errors()
    );
    assert!(
        run.warnings().is_empty(),
        "unexpected warnings: {:?}",
        run.warnings()
    );
    // The canonical report is always present, even on a clean run.
    assert!(run.report.normalized().ok());
    assert_eq!(run.dataset.quad_count(), 1);

    // The single triple (s,p,o) is present in the shared dataset.
    let ds = &run.dataset;
    let s = ds.term_id_by_value(&purrdf::TermValue::iri("https://example.org/a"));
    let p = ds.term_id_by_value(&purrdf::TermValue::iri("https://example.org/p"));
    let o = ds.term_id_by_value(&purrdf::TermValue::iri("https://example.org/b"));
    assert!(
        ds.quads_for_pattern(s, p, o, GraphMatch::Any)
            .next()
            .is_some(),
        "the (a,p,b) triple must be present in the shared dataset"
    );
}

/// With the fixed demonstrator removed (greenfield), a normal-completion run
/// over a bundle carrying NO accepted recommendation candidates emits an EMPTY
/// advisory tier — honest absence, not a synthetic always-on Note. This proves the
/// unconditional demonstrator is gone. Harvested advisories surfacing on a real
/// candidate-bearing dataset is covered by the advisory-bridge unit tests
/// (`harvest_yields_note_with_subject_and_howtouse_suggestion` et al.) and the
/// pipeline stage test; the full `make check` over gmeow.gts (which ships the advisory
/// candidates) exercises the whole path end to end.
#[test]
fn clean_run_over_candidate_free_bundle_emits_no_advisory() {
    let bytes = minimal_gts_bytes();
    let options = ValidateOptions {
        gts_bytes: Some(bytes),
        ..ValidateOptions::default()
    };

    let run = ValidationRun::run(&[], "", "", "", &minimal_lint_config(), &options)
        .expect("ValidationRun::run must succeed");

    // No advisory contaminates the error/warning surfaces, and a clean run is ok.
    assert!(
        run.errors().is_empty(),
        "no errors on a clean run: {:?}",
        run.errors()
    );
    assert!(
        run.warnings().is_empty(),
        "no warnings on a clean run: {:?}",
        run.warnings()
    );
    assert!(
        run.report.normalized().ok(),
        "a clean report must still be ok"
    );

    // A candidate-free bundle harvests NOTHING — no claim hook, no advice.* finding.
    assert!(
        run.advisory_claims.is_empty(),
        "a candidate-free bundle must harvest no advisory claims; got: {:?}",
        run.advisory_claims
    );
    assert!(
        !run.report
            .findings
            .iter()
            .any(|f| f.code.starts_with(crate::codes::ADVICE_FAMILY)),
        "a candidate-free bundle must emit no advice.* finding"
    );
}

/// The syntax/sameAs short-circuit early return (a hard-failed run) must NOT
/// emit any advisory. Triggered with VALID Turtle carrying a banned
/// `owl:sameAs` to an external entity: the file parses (so build-store
/// succeeds), then Phase 2 records a sameAs-ban error, so `run` returns at the
/// `!errors.is_empty()` short-circuit (NOT via an Err) — exercising the real
/// Ok early-return path, not the vacuous build-store-failure path.
#[test]
fn early_return_path_emits_no_advisory() {
    let (_tmp, banned_ttl_path) = write_tmp(
        "gmeow_validate_advisory_early_return_sameas.ttl",
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:Foo owl:sameAs <https://external.example.org/bar> .\n",
    );
    let source = banned_ttl_path.to_string_lossy().to_string();

    let options = ValidateOptions::default();
    let run = ValidationRun::run(&[source], "", "", "", &minimal_lint_config(), &options)
        .expect("valid-but-banned Turtle must reach the Ok short-circuit, not Err");

    // The run hard-failed (the sameAs ban is an error), proving we hit the
    // short-circuit early-return path.
    assert!(
        !run.errors().is_empty(),
        "expected a sameAs-ban error to drive the short-circuit; got none"
    );
    // The hard-fail path emits NO advisory claim and NO advisory finding.
    assert!(
        run.advisory_claims.is_empty(),
        "early-return path must emit no advisory claims"
    );
    assert!(
        !run.report
            .findings
            .iter()
            .any(|f| f.code.starts_with(crate::codes::ADVICE_FAMILY)),
        "early-return path must emit no advice.* finding"
    );
}

// ── Category-assignment tests (H1) ──────────────────────────────────────

/// A SHACL constraint violation folded through `shacl_findings_from_report`
/// must carry `FindingCategory::DataShapeViolation`.
#[test]
fn shacl_violation_is_categorized_data_shape_violation() {
    use purrdf::shapes::report::{ValidationReport, ValidationResult};
    use purrdf::shapes::term::{Literal, NamedNode, Term};

    let result = ValidationResult {
        focus_node: Term::NamedNode(NamedNode::new_unchecked("https://example.org/FocusA")),
        result_path: None,
        path_structure: None,
        value: None,
        source_constraint_component: NamedNode::new_unchecked(
            "http://www.w3.org/ns/shacl#MinCountConstraintComponent",
        ),
        source_shape: Term::NamedNode(NamedNode::new_unchecked("https://example.org/ShapeA")),
        severity: purrdf::shapes::report::Severity::Violation,
        message: Some("must have at least one value".to_owned()),
        source_box_roles: vec![],
        path_box_roles: vec![],
        result_box_roles: vec![],
        attributions: vec![],
    };
    let _ = Literal::new_simple_literal("unused");
    let report = ValidationReport {
        conforms: false,
        results: vec![result],
    };

    let findings = shacl_findings_from_report(&report, None, &FailureClassIndex::empty());

    assert_eq!(findings.len(), 1, "expected exactly one finding");
    assert_eq!(
        findings[0].category,
        Some(FindingCategory::DataShapeViolation),
        "a SHACL violation must carry DataShapeViolation; got {:?}",
        findings[0].category
    );
    assert!(
        findings[0].code.starts_with("shacl."),
        "finding code must start with 'shacl.'; got {}",
        findings[0].code
    );
}

/// A non-conforming SHACL report with zero results (the `shacl.nonconforming`
/// guard) must also carry `FindingCategory::DataShapeViolation`.
#[test]
fn shacl_nonconforming_guard_is_categorized_data_shape_violation() {
    use purrdf::shapes::report::ValidationReport;

    let report = ValidationReport {
        conforms: false,
        results: vec![],
    };

    let findings = shacl_findings_from_report(&report, None, &FailureClassIndex::empty());

    assert_eq!(
        findings.len(),
        1,
        "expected the nonconforming guard finding"
    );
    assert_eq!(findings[0].code, "shacl.nonconforming");
    assert_eq!(
        findings[0].category,
        Some(FindingCategory::DataShapeViolation),
        "the nonconforming guard must carry DataShapeViolation"
    );
}

/// A `ReasoningResult` with `evaluation=BudgetExhausted` must cause
/// `fold_reasoning_result` to emit a finding categorized `IncompleteCheck`.
#[test]
fn budget_exhausted_result_emits_incomplete_check() {
    use gmeow_logic::result::{
        CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
        ReasoningResult, ResultPayload, ResultProvenance,
    };

    let result = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::BudgetExhausted,
        // BudgetExhausted → completeness must be Incomplete or Unknown (not CompleteForFragment
        // with BudgetExhausted, as that would mean conclusive). Use Incomplete.
        CompletenessStatus::Incomplete,
        PreservationClaim::exact(),
        // Neither requires conclusive, but BudgetExhausted + Incomplete is non-conclusive,
        // so the information state must be Undetermined (not Neither).
        InformationState::Undetermined,
        ResultProvenance::native("test-contract", "test-world"),
        ResultPayload::Empty,
    );

    let mut report = Report::new("validate");
    fold_reasoning_result(
        &result,
        ContradictionPolicy::ForbidGapAndGlut,
        &[],
        &mut report,
    )
    .expect("synthetic empty-payload result folds without witnesses");

    let incomplete_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.category == Some(FindingCategory::IncompleteCheck))
        .collect();

    assert!(
        !incomplete_findings.is_empty(),
        "a BudgetExhausted result must emit at least one IncompleteCheck finding; \
             got findings: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
    assert_eq!(
        incomplete_findings[0].code, "validate.deep.incomplete",
        "incomplete finding must carry the expected code"
    );
    assert_eq!(
        incomplete_findings[0].severity,
        Severity::Warning,
        "incomplete check must be a Warning, not an error"
    );
}

/// A `ReasoningResult` with `completeness=Incomplete` (but evaluation Completed)
/// must also trigger the `IncompleteCheck` category.
#[test]
fn completeness_incomplete_result_emits_incomplete_check() {
    use gmeow_logic::result::{
        CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
        ReasoningResult, ResultPayload, ResultProvenance,
    };

    // Completed + CompleteForFragment is conclusive → Neither is valid.
    // Completed + Incomplete is conclusive via Completed → Neither is still valid.
    // We want to fire the IncompleteCheck path: evaluation=Completed, completeness=Incomplete.
    let result = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::Incomplete,
        PreservationClaim::exact(),
        // Completed alone makes it conclusive, so Neither is valid.
        InformationState::Neither,
        ResultProvenance::native("test-contract", "test-world"),
        ResultPayload::Empty,
    );

    let mut report = Report::new("validate");
    fold_reasoning_result(
        &result,
        ContradictionPolicy::ForbidGapAndGlut,
        &[],
        &mut report,
    )
    .expect("synthetic empty-payload result folds without witnesses");

    let incomplete = report
        .findings
        .iter()
        .find(|f| f.category == Some(FindingCategory::IncompleteCheck))
        .expect("completeness=Incomplete must emit an IncompleteCheck finding");
    assert_eq!(incomplete.code, "validate.deep.incomplete");
    assert_eq!(incomplete.severity, Severity::Warning);
}

/// A fully-conclusive, consistent result (evaluation=Completed, completeness=CompleteForFragment)
/// must NOT emit any IncompleteCheck finding.
#[test]
fn conclusive_consistent_result_does_not_emit_incomplete_check() {
    use gmeow_logic::result::{
        CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
        ReasoningResult, ResultPayload, ResultProvenance,
    };

    let result = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        PreservationClaim::exact(),
        InformationState::Neither,
        ResultProvenance::native("test-contract", "test-world"),
        ResultPayload::Empty,
    );

    let mut report = Report::new("validate");
    fold_reasoning_result(
        &result,
        ContradictionPolicy::ForbidGapAndGlut,
        &[],
        &mut report,
    )
    .expect("synthetic empty-payload result folds without witnesses");

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.category == Some(FindingCategory::IncompleteCheck)),
        "a conclusive, consistent result must NOT emit IncompleteCheck; \
             got: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
}

/// `fold_reasoning_result` must emit at least one `FindingCategory::ProjectionLoss`
/// finding sourced from the genuine static loss ledger, distinct from any
/// `UnsupportedSemanticFeature` findings. The ledger has at least one entry for
/// `gts → owl-dl` (named-graph-dropped + owl-dl-projection), so the count is
/// deterministically at least 2.
#[test]
fn fold_emits_projection_loss_findings_from_ledger() {
    use gmeow_logic::result::{
        CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
        ReasoningResult, ResultPayload, ResultProvenance,
    };

    let result = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        PreservationClaim::exact(),
        InformationState::Neither,
        ResultProvenance::native("test-contract", "test-world"),
        ResultPayload::Empty,
    );

    let mut report = Report::new("validate");
    fold_reasoning_result(
        &result,
        ContradictionPolicy::ForbidGapAndGlut,
        &[],
        &mut report,
    )
    .expect("synthetic empty-payload result folds without witnesses");

    let projection_loss_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.category == Some(FindingCategory::ProjectionLoss))
        .collect();

    assert!(
        !projection_loss_findings.is_empty(),
        "fold_reasoning_result must emit at least one ProjectionLoss finding; \
             got findings: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );

    // All ProjectionLoss findings carry the expected code and severity.
    for f in &projection_loss_findings {
        assert_eq!(
            f.code, "validate.deep.projection-loss",
            "ProjectionLoss finding must carry the expected code"
        );
        assert_eq!(
            f.severity,
            Severity::Note,
            "ProjectionLoss must be a Note (informational), not a failure"
        );
    }

    // The ledger must contribute at least the owl-dl pair (named-graph-dropped +
    // owl-dl-projection), so we expect at least 2 findings.
    assert!(
        projection_loss_findings.len() >= 2,
        "must have at least 2 ProjectionLoss findings (owl-dl pair); got {}",
        projection_loss_findings.len()
    );

    // Messages must name the target codec and contain the loss code.
    let has_named_graph_dropped = projection_loss_findings
        .iter()
        .any(|f| f.message.contains("named-graph-dropped"));
    assert!(
        has_named_graph_dropped,
        "at least one ProjectionLoss finding must mention 'named-graph-dropped'"
    );

    // ProjectionLoss findings must NOT carry UnsupportedSemanticFeature category.
    for f in &projection_loss_findings {
        assert_ne!(
            f.category,
            Some(FindingCategory::UnsupportedSemanticFeature),
            "ProjectionLoss must not be conflated with UnsupportedSemanticFeature"
        );
    }
}

/// Verify that `deep_semantic_findings` (the full GTS path) also emits
/// ProjectionLoss findings — confirming they reach the report via the real bundle path.
#[test]
fn deep_semantic_findings_emits_projection_loss_on_consistent_bundle() {
    // A minimal consistent bundle is sufficient; the projection losses come from
    // the static ledger, not from bundle content.
    let consistent = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:x rdf:type ex:A .
";
    let bytes = gts_bytes_from_turtle(consistent);
    let mut report = Report::new("validate");
    deep_semantic_findings_prepared(&bytes, &mut report, &verification_fixture::verification())
        .expect("deep pass must run");

    let projection_loss_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.category == Some(FindingCategory::ProjectionLoss))
        .collect();

    assert!(
        !projection_loss_findings.is_empty(),
        "deep_semantic_findings must emit at least one ProjectionLoss finding; \
             got findings: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
}

#[test]
fn interned_shacl_finding_roundtrips_related_locations_and_detail() {
    use gmeow_errors::Location;

    // A SHACL-shaped finding: focus-node PRIMARY location, result-path + value
    // RELATED locations, and a "source shape: …" detail (exactly what
    // `finding_from_shacl` produces). Interning it onto the run ledger and
    // projecting back via `project_report` must carry ALL of them — the
    // fingerprint keys on the message-INDEPENDENT structural identity, but no
    // structural anchor is lost through the ledger round-trip.
    let mut finding = Finding::new(
        Severity::Error,
        "shacl.MinCountConstraintComponent",
        "missing required property",
    )
    .with_tool("shacl")
    .with_category(FindingCategory::DataShapeViolation);
    finding.add_location(Location {
        logical: Some("https://ex/a".to_owned()),
        ..Location::default()
    });
    finding.related_locations.push(Location {
        logical: Some("path https://ex/p".to_owned()),
        ..Location::default()
    });
    finding.related_locations.push(Location {
        logical: Some("value https://ex/bad".to_owned()),
        ..Location::default()
    });
    finding.detail = Some("source shape: https://ex/shape".to_owned());

    let mut ledger = DiagLedger::new();
    intern_finding(
        &mut ledger,
        StageId::new("validate.shacl"),
        Standpoint::Binding,
        &finding,
    );
    let report = ledger.project_report("validate");
    let projected = report
        .findings
        .iter()
        .find(|f| f.code == "shacl.MinCountConstraintComponent")
        .expect("interned SHACL finding must project back into the report");

    // The focus-node primary location survives.
    assert_eq!(
        projected
            .primary_location()
            .and_then(|l| l.logical.as_deref()),
        Some("https://ex/a"),
        "focus-node primary location must round-trip"
    );
    // The SHACL result-path and offending-value related locations survive
    // (carried as first-class Labels, re-emitted by `to_finding`).
    assert!(
        projected
            .related_locations
            .iter()
            .any(|l| l.logical.as_deref() == Some("path https://ex/p")),
        "result-path related location must round-trip; got {:?}",
        projected.related_locations
    );
    assert!(
        projected
            .related_locations
            .iter()
            .any(|l| l.logical.as_deref() == Some("value https://ex/bad")),
        "offending-value related location must round-trip; got {:?}",
        projected.related_locations
    );
    // The "source shape: …" detail survives (carried as a context frame,
    // folded back into the projected finding's detail).
    assert_eq!(
        projected.detail.as_deref(),
        Some("source shape: https://ex/shape"),
        "the source-shape detail must round-trip"
    );

    // Hard Invariant 6: two findings identical in structural identity but
    // differing only in message are the SAME witness — interning a
    // message-variant of the same finding must NOT add a second finding.
    let mut variant = finding.clone();
    variant.message = "a differently-worded violation".to_owned();
    intern_finding(
        &mut ledger,
        StageId::new("validate.shacl"),
        Standpoint::Binding,
        &variant,
    );
    let merged = ledger.project_report("validate");
    assert_eq!(
        merged
            .findings
            .iter()
            .filter(|f| f.code == "shacl.MinCountConstraintComponent")
            .count(),
        1,
        "a message-only variant must hash-cons-merge, not fork a new finding"
    );
}

#[test]
fn distinct_lines_of_same_constraint_do_not_hash_cons_merge() {
    use gmeow_errors::Location;

    // Two structurally-distinct violations of the SAME constraint at DIFFERENT
    // lines of one file: identical code / severity / path / detail, no `logical`
    // location, differing only by `line`. Because line/column are part of the
    // message-independent structural identity, these are genuinely different
    // witnesses and must NOT hash-cons-merge — both line numbers must survive.
    let make = |line: u32| {
        let mut finding = Finding::new(
            Severity::Error,
            "shacl.MinCountConstraintComponent",
            "missing required property",
        )
        .with_tool("shacl")
        .with_category(FindingCategory::DataShapeViolation);
        finding.add_location(Location {
            path: Some("ontology.ttl".to_owned()),
            line: Some(line),
            ..Location::default()
        });
        finding.detail = Some("source shape: https://ex/shape".to_owned());
        finding
    };

    let mut ledger = DiagLedger::new();
    for line in [10u32, 40u32] {
        intern_finding(
            &mut ledger,
            StageId::new("validate.shacl"),
            Standpoint::Binding,
            &make(line),
        );
    }
    let report = ledger.project_report("validate");

    let lines: std::collections::BTreeSet<u32> = report
        .findings
        .iter()
        .filter(|f| f.code == "shacl.MinCountConstraintComponent")
        .filter_map(|f| f.primary_location().and_then(|l| l.line))
        .collect();
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "shacl.MinCountConstraintComponent")
            .count(),
        2,
        "two distinct-line violations of one constraint must NOT merge; got \
             lines {lines:?}"
    );
    assert!(
        lines.contains(&10) && lines.contains(&40),
        "both violated line numbers must survive the ledger round-trip; got {lines:?}"
    );
}
