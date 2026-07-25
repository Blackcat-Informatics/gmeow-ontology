// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! D5 abductive advice, proved on the SHIPPED production surface (the real
//! `gmeow_pipeline::stages::validate::ValidateStage`), not just the
//! `gmeow_validate::abductive::abductive_advisories` producer function.
//!
//! Task 3 wired the abductive producer into the validate stage's unconditional
//! advisory dual-projection loop (`crates/pipeline/src/stages/validate.rs`): each
//! engine-corroborated candidate is a WARRANT-as-Finding (attached to the advisory
//! `DiagLedger` first, so it earns a real `finding_iri` fingerprint) plus an advisory
//! whose diag carries a genuine finding→finding `gmeow:findingAntecedent` pointing at
//! that warrant's fingerprint. This test drives that stage END TO END over a fixture
//! base graph and asserts on the EMITTED CARRIER — the `graph/diagnostics` and
//! `graph/norm-claims` named graphs of the stage product's dataset.
//!
//! The base graph is the union of the canonical logic module (the four authored
//! `logic:AbductiveSchema`s + their completeness formulas AND — because the same
//! module carries them — the `gmeow:DiagnosticMetaRule` root-cause fold) and the
//! kernel module (the sortal disjointness the sortal warrant relies on), plus a tiny
//! four-subject A-Box: the four D5 gap cases. Because the logic module is in the base
//! graph, the stage's OWN meta-fold (`MetaProgram::from_source`) runs over the emitted
//! finding graph inside the shipped stage. We ALSO re-run the authored meta-fold
//! independently over the emitted diagnostics N-Quads to prove the warrant JOIN is
//! executable, not merely a string (see `warrant_edge_resolves_through_the_meta_fold`).
//!
//! The harness mirrors the crate-internal `run_full_stage` helper in
//! `crates/pipeline/src/stages/validate.rs` (which is `#[cfg(test)]`-only and so
//! unreachable from an integration test) using only the crate's PUBLIC surface: the
//! source-load product carries the `BASE_GRAPH_PATH` artifact + a `REP_SPAN_TABLE`
//! blob, and the four generated-shape producers arrive as header-only fresh members
//! (the stale-disk-fold fail-closed contract). The SHACL shape union is a benign
//! no-target NodeShape, so the run is genuinely conforming and the whole product is
//! the advisory + norm-claim dual projection — nothing else.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use gmeow_pipeline::bundle::bundle_from_artifacts_over_with_rep_blob;
use gmeow_pipeline::bundle_blobs::REP_SPAN_TABLE;
use gmeow_pipeline::ingest::SpanIndex;
use gmeow_pipeline::node::{Stage, StageInput, StageProduct};
use gmeow_pipeline::stages::meta_findings::MetaProgram;
use gmeow_pipeline::stages::source_load::BASE_GRAPH_PATH;
use gmeow_pipeline::stages::validate::{SHACL_RDF_PATH, ValidateStage};
use purrdf::provenance::DatasetProvenance;
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

// The carrier named-graph IRIs. `crate::stages::carrier::{GRAPH_DIAGNOSTICS,
// GRAPH_NORM_CLAIMS}` are `pub(crate)`, so — exactly like `product_routing.rs` — this
// integration test redeclares the two IRI strings locally (the workspace idiom).
const GRAPH_DIAGNOSTICS: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
const GRAPH_NORM_CLAIMS: &str = "https://blackcatinformatics.ca/gmeow/graph/norm-claims";

// Finding predicates the diagnostics graph carries (gmeow_errors::render::to_gmeow_rdf).
const FINDING_CODE: &str = "https://blackcatinformatics.ca/gmeow/findingCode";
const FINDING_SUGGESTION: &str = "https://blackcatinformatics.ca/gmeow/findingSuggestion";
const FINDING_SEVERITY: &str = "https://blackcatinformatics.ca/gmeow/findingSeverity";
const FINDING_STANDPOINT: &str = "https://blackcatinformatics.ca/gmeow/findingStandpoint";
const FINDING_ANTECEDENT: &str = "https://blackcatinformatics.ca/gmeow/findingAntecedent";
const FINDING_ROOT_CAUSE: &str = "https://blackcatinformatics.ca/gmeow/findingRootCause";
const SEVERITY_NOTE: &str = "https://blackcatinformatics.ca/gmeow/severityNote";
const STANDPOINT_ADVISORY: &str = "https://blackcatinformatics.ca/gmeow/standpointAdvisory";

// Norm-claims predicates (crates/validate/src/advisory.rs::project_compliance_assessment).
const COMPLIANCE_ASSESSMENT: &str = "https://blackcatinformatics.ca/gmeow/ComplianceAssessment";
const ASSESSED_NORM: &str = "https://blackcatinformatics.ca/gmeow/assessedNorm";
const DEONTIC_MODALITY: &str = "https://blackcatinformatics.ca/gmeow/deonticModality";
const DEONTIC_RECOMMENDATION: &str = "https://blackcatinformatics.ca/gmeow/deonticRecommendation";

// The four D5 gap subjects and their advice codes. The advice code is
// `advice.abductive.<discipline>.<digest>` where <discipline> = code_local(schema term):
// gmeow:Commitment → "Commitment", gmeow:Item → "Item", gmeow:Expression → "Expression",
// gmeow:Entity → "Entity" (see `crates/validate/src/abductive.rs::build_suggestion`).
const SUBJ_COMMITMENT: &str = "urn:c1";
const SUBJ_ITEM: &str = "urn:i1";
const SUBJ_EXPRESSION: &str = "urn:x1";
const SUBJ_ENTITY: &str = "urn:e1";

const ADVICE_ABDUCTIVE_PREFIX: &str = "advice.abductive.";
const ADVICE_WARRANT_PREFIX: &str = "advice.abductive.warrant.";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The base graph the stage validates: kernel + logic modules (schemas, completeness
/// formulas, and the diagnostic meta-rules) + the four D5 gap subjects, serialized to
/// N-Quads (the format `ValidateStage` re-parses the `BASE_GRAPH_PATH` artifact as).
///
/// The four gap subjects mirror `crates/validate/tests/abductive.rs`:
///   * `urn:c1` — an under-mediated gmeow:Commitment (committedAgent + intentionGoal,
///     MISSING commitmentBeneficiary);
///   * `urn:i1` — a bare gmeow:Item (no gmeow:exemplifies);
///   * `urn:x1` — a bare gmeow:Expression (no gmeow:hasReferenceFrame);
///   * `urn:e1` — a gmeow:Entity that also carries a fixture-only class disjoint with
///     gmeow:SocialObject (F1: a genuinely BARE entity — nothing refuted — now suppresses
///     its sortal menu entirely as non-discriminating noise, so this fixture must carry a
///     REFUTATION to keep exercising the sortal wing's emitting path; the disjointness
///     lives only in this TEST fixture, never the shipped vocabulary).
fn base_nquads() -> Vec<u8> {
    let mut builder = RdfDatasetBuilder::new();
    for module in [
        "slices/core/kernel/module.ttl",
        "slices/grounding/logic/module.ttl",
    ] {
        let text = std::fs::read_to_string(repo_root().join(module)).expect("read module");
        let dataset =
            purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).expect("module parses");
        builder.push_dataset(dataset.as_ref());
    }
    let abox = format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         <{SUBJ_COMMITMENT}> a gmeow:Commitment ; gmeow:graphBoxRole gmeow:boxABox ; \
             gmeow:committedAgent <urn:agentA> ; gmeow:intentionGoal <urn:goalG> .\n\
         <urn:agentA> a gmeow:Agent .\n\
         <{SUBJ_ITEM}> a gmeow:Item ; gmeow:graphBoxRole gmeow:boxABox .\n\
         <{SUBJ_EXPRESSION}> a gmeow:Expression ; gmeow:graphBoxRole gmeow:boxABox .\n\
         <urn:notASocialObject> a owl:Class ; owl:disjointWith gmeow:SocialObject .\n\
         <{SUBJ_ENTITY}> a gmeow:Entity , <urn:notASocialObject> ; gmeow:graphBoxRole gmeow:boxABox .\n"
    );
    let abox_ds = purrdf::parse_dataset(abox.as_bytes(), "text/turtle", None).expect("abox parses");
    builder.push_dataset(abox_ds.as_ref());
    let dataset = builder.freeze().expect("merge base graph");
    // Plain (non-canonical) N-Quads: the abductive producer + meta-fold read the dataset
    // semantically, so blank-node relabelling is irrelevant and we skip RDFC-1.0.
    purrdf::serialize_dataset_to_format(dataset.as_ref(), purrdf::NativeRdfFormat::NQuads, None)
        .expect("serialize base graph to N-Quads")
        .bytes
}

/// A benign SHACL shape union member — a NodeShape targeting a node that does not exist,
/// so the run is genuinely conforming (`shacl.clean`) and the whole stage product is the
/// advisory dual projection, uncontaminated by hard SHACL findings.
const BENIGN_SHAPES: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://example.test/> .
ex:NoopShape a sh:NodeShape ; sh:targetNode ex:nothing .
"#;

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A mock repo whose authored shape half is the benign no-op shape (mirrors the
/// crate-internal `mock_repo` helper).
fn mock_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    write(&repo.path().join("shapes/gmeow-shapes.ttl"), BENIGN_SHAPES);
    write(
        &repo.path().join("generated/shapes/frame-shapes.ttl"),
        "# generated\n",
    );
    std::fs::create_dir_all(repo.path().join("slices")).unwrap();
    repo
}

/// The fully-owned emitted carrier — captured once (the stage run reasons the whole
/// kernel+logic base graph through the native conjecture engine, so it is cached across
/// every test via a `OnceLock`).
struct Emitted {
    /// Every quad of the product's `graph/diagnostics` named graph.
    diagnostics: Vec<RdfQuad>,
    /// Every quad of the product's `graph/norm-claims` named graph.
    norm_claims: Vec<RdfQuad>,
    /// Every quad of the WHOLE product dataset (for the R4 base-untouched proof).
    all: Vec<RdfQuad>,
    /// The rendered `generated/diagnostics/shacl.nq` bytes — the diagnostics finding
    /// graph the meta-fold re-derives over (carries the finding→finding antecedent edges).
    diagnostics_nq: String,
}

static EMITTED: OnceLock<Emitted> = OnceLock::new();

/// Drive the REAL `ValidateStage::run` once over the D5 fixture base graph, using only
/// the crate's public surface, and capture the emitted carrier.
fn emitted() -> &'static Emitted {
    EMITTED.get_or_init(|| {
        let repo = mock_repo();

        // ── the source-load product: BASE_GRAPH_PATH artifact + REP_SPAN_TABLE blob ──
        let empty = RdfDatasetBuilder::new().freeze().expect("empty dataset");
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(BASE_GRAPH_PATH.to_string(), base_nquads());
        let span_blob = serde_json::to_vec(&SpanIndex::new()).expect("encode span index");
        let bundle = bundle_from_artifacts_over_with_rep_blob(
            empty,
            artifacts,
            DatasetProvenance::new(),
            REP_SPAN_TABLE,
            "application/json",
            span_blob,
        );
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-source-load".to_string(),
            StageProduct::from_bundle("stage-source-load", Arc::new(bundle)),
        );

        // ── the four generated-shape producers, header-only fresh members ───────────
        use gmeow_pipeline::stages::{
            compile_logic::{PROCEDURAL_CONSTRAINTS_PATH, VALIDATION_SHAPES_TTL_PATH},
            constraint_shapes::CONSTRAINT_SHAPES_PATH,
            frame_shapes::FRAME_SHAPES_PATH,
            result_shapes::RESULT_SHAPES_PATH,
        };
        for (producer, rels) in [
            (
                "stage-compile-logic",
                &[VALIDATION_SHAPES_TTL_PATH, PROCEDURAL_CONSTRAINTS_PATH][..],
            ),
            (
                "stage-export-constraint-shapes",
                &[CONSTRAINT_SHAPES_PATH][..],
            ),
            ("stage-export-frame-shapes", &[FRAME_SHAPES_PATH][..]),
            ("stage-export-result-shapes", &[RESULT_SHAPES_PATH][..]),
        ] {
            let members: BTreeMap<String, Vec<u8>> = rels
                .iter()
                .map(|rel| ((*rel).to_string(), b"# generated\n".to_vec()))
                .collect();
            upstream.insert(
                producer.to_string(),
                StageProduct::from_artifacts(producer, members),
            );
        }

        // The D5 abductive tier consumes stage-reason's reasoned closure. This fixture drives
        // the advisory wiring, not entailment, so the reasoned upstream is an empty-EDB reason
        // product (an empty closure ⇒ the reasoned union is exactly the authored base graph).
        upstream.insert(
            "stage-reason".to_string(),
            gmeow_pipeline::stages::reason::reason_product(b"")
                .expect("stage-reason fixture product"),
        );

        let output = ValidateStage::new()
            .run(StageInput {
                root: repo.path(),
                upstream: &upstream,
            })
            .expect("validate stage run over the D5 fixture");

        let dataset = output.product.dataset();
        let diagnostics = dataset
            .project_named_graph(GRAPH_DIAGNOSTICS)
            .owned_quads()
            .collect();
        let norm_claims = dataset
            .project_named_graph(GRAPH_NORM_CLAIMS)
            .owned_quads()
            .collect();
        let all = dataset.owned_quads().collect();
        let diagnostics_nq = String::from_utf8(
            output
                .product
                .artifact(SHACL_RDF_PATH)
                .expect("shacl.nq artifact on the stage product")
                .to_vec(),
        )
        .expect("shacl.nq is UTF-8");

        Emitted {
            diagnostics,
            norm_claims,
            all,
            diagnostics_nq,
        }
    })
}

/// Build a base graph = kernel + logic modules (schemas + meta-rules) + `abox_ttl`, and
/// drive the REAL `ValidateStage::run` with `reason` as the `stage-reason` upstream.
/// Returns every quad of the emitted `graph/diagnostics`. Mirrors `emitted()`'s upstream
/// wiring but is parameterized on the A-Box and the reasoned upstream so a test can vary
/// the closure the abductive tier reads.
fn run_with_reason(abox_ttl: &str, reason: StageProduct) -> Vec<RdfQuad> {
    let repo = mock_repo();

    let mut builder = RdfDatasetBuilder::new();
    for module in [
        "slices/core/kernel/module.ttl",
        "slices/grounding/logic/module.ttl",
    ] {
        let text = std::fs::read_to_string(repo_root().join(module)).expect("read module");
        let dataset =
            purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).expect("module parses");
        builder.push_dataset(dataset.as_ref());
    }
    let abox_ds =
        purrdf::parse_dataset(abox_ttl.as_bytes(), "text/turtle", None).expect("abox parses");
    builder.push_dataset(abox_ds.as_ref());
    let base = builder.freeze().expect("merge base graph");
    let base_nq =
        purrdf::serialize_dataset_to_format(base.as_ref(), purrdf::NativeRdfFormat::NQuads, None)
            .expect("serialize base graph")
            .bytes;

    let empty = RdfDatasetBuilder::new().freeze().expect("empty dataset");
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(BASE_GRAPH_PATH.to_string(), base_nq);
    let span_blob = serde_json::to_vec(&SpanIndex::new()).expect("encode span index");
    let bundle = bundle_from_artifacts_over_with_rep_blob(
        empty,
        artifacts,
        DatasetProvenance::new(),
        REP_SPAN_TABLE,
        "application/json",
        span_blob,
    );
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert(
        "stage-source-load".to_string(),
        StageProduct::from_bundle("stage-source-load", Arc::new(bundle)),
    );
    use gmeow_pipeline::stages::{
        compile_logic::{PROCEDURAL_CONSTRAINTS_PATH, VALIDATION_SHAPES_TTL_PATH},
        constraint_shapes::CONSTRAINT_SHAPES_PATH,
        frame_shapes::FRAME_SHAPES_PATH,
        result_shapes::RESULT_SHAPES_PATH,
    };
    for (producer, rels) in [
        (
            "stage-compile-logic",
            &[VALIDATION_SHAPES_TTL_PATH, PROCEDURAL_CONSTRAINTS_PATH][..],
        ),
        (
            "stage-export-constraint-shapes",
            &[CONSTRAINT_SHAPES_PATH][..],
        ),
        ("stage-export-frame-shapes", &[FRAME_SHAPES_PATH][..]),
        ("stage-export-result-shapes", &[RESULT_SHAPES_PATH][..]),
    ] {
        let members: BTreeMap<String, Vec<u8>> = rels
            .iter()
            .map(|rel| ((*rel).to_string(), b"# generated\n".to_vec()))
            .collect();
        upstream.insert(
            producer.to_string(),
            StageProduct::from_artifacts(producer, members),
        );
    }
    upstream.insert("stage-reason".to_string(), reason);

    let output = ValidateStage::new()
        .run(StageInput {
            root: repo.path(),
            upstream: &upstream,
        })
        .expect("validate stage run");
    output
        .product
        .dataset()
        .project_named_graph(GRAPH_DIAGNOSTICS)
        .owned_quads()
        .collect()
}

/// The reasoned closure is GENUINELY threaded into the abductive tier: an Item-completion
/// advisory fires on a subject whose `gmeow:Item` guard type is ENTAILED-ONLY (present in
/// stage-reason's closure, absent from the authored source graph). This proves the fix is
/// not a silent no-op — the union with the derived closure is what surfaces the advice.
///
/// `<urn:esub>` is authored `a ex:Widget` with `ex:Widget rdfs:subClassOf gmeow:Item`, but
/// NOT `a gmeow:Item`. The reasoner derives `<urn:esub> a gmeow:Item` (EL type
/// propagation); only when that closure is unioned in does the Item schema (missing
/// `gmeow:exemplifies`) fire for `<urn:esub>`. The contrast run with an EMPTY reasoned
/// closure emits NO advice for `<urn:esub>` — the authored graph alone cannot see the type.
#[test]
fn entailed_only_guard_type_surfaces_abductive_advice() {
    const ESUB: &str = "urn:esub";
    let abox = format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix ex: <https://example.test/> .\n\
         ex:Widget rdfs:subClassOf gmeow:Item .\n\
         <{ESUB}> a ex:Widget ; gmeow:graphBoxRole gmeow:boxABox .\n"
    );
    // The reasoned EDB that derives `<urn:esub> a gmeow:Item` by type propagation.
    let reason_edb = format!(
        "<{ESUB}> <{RDF_TYPE}> <https://example.test/Widget> .\n\
         <https://example.test/Widget> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <{GMEOW}Item> .\n"
    );
    let reason = gmeow_pipeline::stages::reason::reason_product(reason_edb.as_bytes())
        .expect("stage-reason product over the derivation EDB");

    let diagnostics = run_with_reason(&abox, reason);
    // The Item-completion advice for `<urn:esub>` (its suggestion names gmeow:exemplifies)
    // is present ONLY because the entailed `a gmeow:Item` was unioned in from the closure.
    let item_advice_for_esub: Vec<&str> = diagnostics
        .iter()
        .filter(|q| q.predicate.as_str() == FINDING_SUGGESTION)
        .filter_map(|q| {
            let subject = as_iri(&q.subject)?;
            let text = as_literal(&q.object)?;
            (text.contains(ESUB) && text.contains("gmeow:exemplifies")).then_some(subject)
        })
        .collect();
    assert!(
        !item_advice_for_esub.is_empty(),
        "the entailed-only gmeow:Item type must surface an Item-completion advisory for \
         <{ESUB}> (via the reasoned-closure union); diagnostics carried none"
    );

    // Contrast: with an EMPTY reasoned closure the union is the authored graph alone, which
    // never carries `<urn:esub> a gmeow:Item` — so NO Item advice for `<urn:esub>` fires.
    let empty_reason =
        gmeow_pipeline::stages::reason::reason_product(b"").expect("empty stage-reason product");
    let diagnostics_authored_only = run_with_reason(&abox, empty_reason);
    let item_advice_authored_only = diagnostics_authored_only
        .iter()
        .filter(|q| q.predicate.as_str() == FINDING_SUGGESTION)
        .any(|q| {
            as_literal(&q.object)
                .is_some_and(|text| text.contains(ESUB) && text.contains("gmeow:exemplifies"))
        });
    assert!(
        !item_advice_authored_only,
        "with an empty reasoned closure the authored graph alone must NOT surface Item advice \
         for <{ESUB}> — proving the closure union is load-bearing"
    );
}

// ── Case 1b: measurement-frame (PROPERTY guard) on the shipped stage ─────────────────

/// The measurement-frame abductive schema fires through the REAL `ValidateStage`: a
/// subject carrying `logic:unit` (the IRI-valued witness a framed value exists) but no
/// `logic:referenceFrame` — the `logic:MeasurementFrameMissing` gap — surfaces exactly one
/// "declare a reference frame" advisory whose `gmeow:findingSuggestion` names
/// `logic:referenceFrame`. This proves the PROPERTY-presence guard (not a class type) fires
/// on the production surface, and a subject already declaring its frame stays silent.
#[test]
fn unit_bearing_value_surfaces_measurement_frame_advice_through_the_stage() {
    const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
    const SUBJ_MEASUREMENT: &str = "urn:m1";
    const SUBJ_FRAMED: &str = "urn:m2";
    let abox = format!(
        "@prefix logic: <{LOGIC}> .\n@prefix gmeow: <{GMEOW}> .\n\
         <{SUBJ_MEASUREMENT}> logic:unit <urn:degreeCelsius> ; gmeow:graphBoxRole gmeow:boxABox .\n\
         <{SUBJ_FRAMED}> logic:unit <urn:degreeCelsius> ; gmeow:graphBoxRole gmeow:boxABox ; logic:referenceFrame <urn:celsiusFrame> .\n"
    );
    let empty_reason =
        gmeow_pipeline::stages::reason::reason_product(b"").expect("empty stage-reason product");
    let diagnostics = run_with_reason(&abox, empty_reason);

    // The unit-bearing, frame-less subject gets exactly one frame advisory naming
    // logic:referenceFrame.
    let for_m1: Vec<&str> = diagnostics
        .iter()
        .filter(|q| q.predicate.as_str() == FINDING_SUGGESTION)
        .filter_map(|q| as_literal(&q.object))
        .filter(|text| text.contains(SUBJ_MEASUREMENT) && text.contains("logic:referenceFrame"))
        .collect();
    assert!(
        !for_m1.is_empty(),
        "a unit-bearing value with no logic:referenceFrame must surface a measurement-frame \
         advisory naming logic:referenceFrame through the shipped ValidateStage; diagnostics \
         carried none"
    );

    // The already-framed subject surfaces no measurement-frame advice (honest absence).
    let for_m2 = diagnostics
        .iter()
        .filter(|q| q.predicate.as_str() == FINDING_SUGGESTION)
        .filter_map(|q| as_literal(&q.object))
        .any(|text| text.contains(SUBJ_FRAMED) && text.contains("logic:referenceFrame"));
    assert!(
        !for_m2,
        "a value already declaring its logic:referenceFrame must NOT surface measurement-frame \
         advice (honest absence)"
    );
}

// ── quad helpers ─────────────────────────────────────────────────────────────────────

/// The IRI string of a term, or `None` for a literal / blank node.
fn as_iri(term: &RdfTerm) -> Option<&str> {
    match term {
        RdfTerm::Iri(iri) => Some(iri.as_str()),
        _ => None,
    }
}

/// The lexical form of a literal term, or `None` otherwise.
fn as_literal(term: &RdfTerm) -> Option<&str> {
    match term {
        RdfTerm::Literal(lit) => Some(lit.lexical_form.as_str()),
        _ => None,
    }
}

/// Every object of `(subject, predicate, _)` in `quads`.
fn objects<'a>(quads: &'a [RdfQuad], subject: &str, predicate: &str) -> Vec<&'a RdfTerm> {
    quads
        .iter()
        .filter(|q| as_iri(&q.subject) == Some(subject) && q.predicate.as_str() == predicate)
        .map(|q| &q.object)
        .collect()
}

/// Every subject (IRI) that carries `predicate` with a literal object satisfying `pred`.
fn subjects_by_literal<'a>(
    quads: &'a [RdfQuad],
    predicate: &str,
    pred: impl Fn(&str) -> bool,
) -> Vec<(&'a str, &'a str)> {
    quads
        .iter()
        .filter(|q| q.predicate.as_str() == predicate)
        .filter_map(|q| {
            let subject = as_iri(&q.subject)?;
            let lit = as_literal(&q.object)?;
            pred(lit).then_some((subject, lit))
        })
        .collect()
}

/// The single advisory finding for `discipline` (its `advice.abductive.<discipline>.*`
/// finding code, excluding the `.warrant.` twin): `(finding_iri, findingCode)`.
fn advisory_finding<'a>(diagnostics: &'a [RdfQuad], discipline: &str) -> (&'a str, &'a str) {
    let want = format!("{ADVICE_ABDUCTIVE_PREFIX}{discipline}.");
    let mut hits: Vec<(&str, &str)> = subjects_by_literal(diagnostics, FINDING_CODE, |code| {
        code.starts_with(&want) && !code.starts_with(ADVICE_WARRANT_PREFIX)
    });
    hits.sort();
    hits.dedup();
    assert!(
        !hits.is_empty(),
        "expected an abductive advisory finding for discipline `{discipline}` \
         (code {want}*) in graph/diagnostics"
    );
    // The bare-Entity case emits four (one per consistent sortal); the caller that needs
    // a single finding uses a single-candidate discipline. For those, exactly one.
    hits[0]
}

// ── Case 1: four cases on the shipped stage output (advisory + dual projection) ────────

/// D5 fires all four disciplines on the shipped `ValidateStage` output, and each emits
/// BOTH wings: (a) a Note advisory finding at the Advisory standpoint whose
/// `gmeow:findingSuggestion` names the specific missing element, in `graph/diagnostics`;
/// and (b) a `deonticRecommendation` `gmeow:ComplianceAssessment` for the SAME advisory
/// code in `graph/norm-claims`.
#[test]
fn four_cases_emit_the_note_advisory_and_the_deontic_recommendation() {
    let emitted = emitted();
    let diagnostics = &emitted.diagnostics;
    let norm_claims = &emitted.norm_claims;

    // (subject, discipline, a substring the suggestion must name).
    let cases: [(&str, &str, &[&str]); 4] = [
        (
            SUBJ_COMMITMENT,
            "Commitment",
            &["gmeow:commitmentBeneficiary"],
        ),
        (
            SUBJ_ITEM,
            "Item",
            &["gmeow:exemplifies", "gmeow:Manifestation"],
        ),
        (SUBJ_EXPRESSION, "Expression", &["gmeow:hasReferenceFrame"]),
        (SUBJ_ENTITY, "Entity", &["gmeow:Agent"]),
    ];

    for (subject, discipline, needles) in cases {
        // Every advisory finding of this discipline whose suggestion carries every needle.
        let want = format!("{ADVICE_ABDUCTIVE_PREFIX}{discipline}.");
        let finding_codes: Vec<(&str, &str)> =
            subjects_by_literal(diagnostics, FINDING_CODE, |code| {
                code.starts_with(&want) && !code.starts_with(ADVICE_WARRANT_PREFIX)
            });
        assert!(
            !finding_codes.is_empty(),
            "discipline `{discipline}` must emit at least one advisory finding: none in \
             graph/diagnostics"
        );

        // (a) the matching Note advisory finding — the one whose suggestion names the
        // specific missing element AND whose diagnostic location resolves the subject.
        let matched: Vec<(&str, &str)> = finding_codes
            .iter()
            .copied()
            .filter(|(finding_iri, _)| {
                objects(diagnostics, finding_iri, FINDING_SUGGESTION)
                    .iter()
                    .any(|obj| {
                        as_literal(obj).is_some_and(|s| needles.iter().all(|n| s.contains(n)))
                    })
            })
            .collect();
        assert!(
            !matched.is_empty(),
            "discipline `{discipline}` (subject {subject}) must emit an advisory whose \
             gmeow:findingSuggestion names {needles:?}; codes seen: {finding_codes:?}"
        );
        let (finding_iri, code) = matched[0];

        // Note severity + Advisory standpoint (a never-gating recommendation).
        assert!(
            objects(diagnostics, finding_iri, FINDING_SEVERITY)
                .iter()
                .any(|o| as_iri(o) == Some(SEVERITY_NOTE)),
            "advisory {code} must be gmeow:severityNote"
        );
        assert!(
            objects(diagnostics, finding_iri, FINDING_STANDPOINT)
                .iter()
                .any(|o| as_iri(o) == Some(STANDPOINT_ADVISORY)),
            "advisory {code} must carry gmeow:findingStandpoint gmeow:standpointAdvisory"
        );

        // (b) the dual projection — a gmeow:ComplianceAssessment for the SAME code whose
        // assessedNorm carries gmeow:deonticModality gmeow:deonticRecommendation.
        let assessment = norm_claims
            .iter()
            .find(|q| {
                q.predicate.as_str() == RDF_TYPE
                    && as_iri(&q.object) == Some(COMPLIANCE_ASSESSMENT)
                    && as_iri(&q.subject).is_some_and(|s| s.contains(code))
            })
            .map(|q| as_iri(&q.subject).unwrap())
            .unwrap_or_else(|| {
                panic!(
                    "graph/norm-claims must carry a gmeow:ComplianceAssessment embedding the \
                     advisory code `{code}` (the reified wing of the paired advice event)"
                )
            });
        let norm = objects(norm_claims, assessment, ASSESSED_NORM)
            .into_iter()
            .find_map(as_iri)
            .unwrap_or_else(|| panic!("assessment {assessment} must carry a gmeow:assessedNorm"));
        assert!(
            objects(norm_claims, norm, DEONTIC_MODALITY)
                .iter()
                .any(|o| as_iri(o) == Some(DEONTIC_RECOMMENDATION)),
            "the assessedNorm {norm} of advisory {code} must carry gmeow:deonticModality \
             gmeow:deonticRecommendation (the D4 dual projection)"
        );
    }

    // The Entity discipline: `urn:e1` also carries a fixture-only class disjoint with
    // gmeow:SocialObject (F1: a genuinely bare entity — nothing refuted — suppresses its
    // sortal menu entirely, so the fixture is built to REFUTE exactly one offered sortal),
    // so it emits a specialization suggestion for the three CORROBORATED remainder sortals
    // and excludes the refuted gmeow:SocialObject.
    let entity_suggestions: Vec<&str> = subjects_by_literal(diagnostics, FINDING_CODE, |code| {
        code.starts_with(&format!("{ADVICE_ABDUCTIVE_PREFIX}Entity."))
            && !code.starts_with(ADVICE_WARRANT_PREFIX)
    })
    .iter()
    .flat_map(|(finding_iri, _)| {
        objects(diagnostics, finding_iri, FINDING_SUGGESTION)
            .into_iter()
            .filter_map(as_literal)
    })
    .collect();
    for sortal in [
        "gmeow:Agent",
        "gmeow:InformationObject",
        "gmeow:PhysicalObject",
    ] {
        assert!(
            entity_suggestions.iter().any(|s| s.contains(sortal)),
            "the gmeow:Entity case must emit a specialization suggestion naming the \
             corroborated sortal {sortal}: {entity_suggestions:?}"
        );
    }
    assert!(
        !entity_suggestions
            .iter()
            .any(|s| s.contains("gmeow:SocialObject")),
        "the REFUTED sortal gmeow:SocialObject must be excluded from the Entity \
         specialization suggestions (F1: non-discriminating menu suppression keeps only the \
         corroborated remainder): {entity_suggestions:?}"
    );
}

// ── Case 2: the warrant edge CLOSES (I3 — the ledger-identity check) ──────────────────

/// The abductive advisory's warrant edge closes as a REAL finding→finding join, not a
/// bare `warrant:` string. For the relator-mediation case:
///   * a warrant `gmeow:Finding` (code `advice.abductive.warrant.Commitment.*`) is
///     present in `graph/diagnostics`; AND
///   * the advisory finding carries a `gmeow:findingAntecedent` whose object IS that
///     warrant finding's own subject (its ledger fingerprint IRI) — the edge closes to a
///     present warrant, so the projected finding graph is walkable by the meta-rules.
#[test]
fn warrant_edge_closes_finding_to_finding_on_a_present_fingerprint() {
    let diagnostics = &emitted().diagnostics;

    let (advisory_iri, advisory_code) = advisory_finding(diagnostics, "Commitment");
    // The warrant twin shares the discipline + candidate digest: advisory code
    // `advice.abductive.Commitment.<digest>` ⇒ warrant `advice.abductive.warrant.Commitment.<digest>`.
    let digest = advisory_code
        .strip_prefix(&format!("{ADVICE_ABDUCTIVE_PREFIX}Commitment."))
        .expect("advisory code carries the discipline prefix");
    let warrant_code = format!("{ADVICE_WARRANT_PREFIX}Commitment.{digest}");

    let warrant_iri = diagnostics
        .iter()
        .find(|q| {
            q.predicate.as_str() == FINDING_CODE
                && as_literal(&q.object) == Some(warrant_code.as_str())
        })
        .map(|q| as_iri(&q.subject).unwrap())
        .unwrap_or_else(|| {
            panic!(
                "a warrant gmeow:Finding with code `{warrant_code}` must be present in \
                 graph/diagnostics"
            )
        });

    // The load-bearing assertion: the antecedent object IS the warrant's own fingerprint
    // subject IRI — a genuine, closed finding→finding edge (not merely the presence of a
    // `warrant:` tag string somewhere).
    let antecedents: Vec<&str> = objects(diagnostics, advisory_iri, FINDING_ANTECEDENT)
        .into_iter()
        .filter_map(as_iri)
        .collect();
    assert!(
        antecedents.contains(&warrant_iri),
        "the advisory finding {advisory_iri} must carry gmeow:findingAntecedent → the warrant \
         finding's fingerprint {warrant_iri}; antecedents = {antecedents:?}"
    );
    // The warrant fingerprint really is a present finding subject (the graph closes).
    assert!(
        diagnostics.iter().any(
            |q| as_iri(&q.subject) == Some(warrant_iri) && q.predicate.as_str() == FINDING_CODE
        ),
        "the antecedent fingerprint {warrant_iri} must be a present warrant finding subject"
    );
}

/// The warrant JOIN is EXECUTABLE and non-DARK — proved TWO ways over the SHIPPED stage's
/// emitted diagnostics finding graph:
///
///  1. **Materialized on the carrier.** The stage's OWN `gmeow:DiagnosticMetaRule` meta-fold
///     (`MetaProgram::from_source` over the base graph, run inside `ValidateStage::run`)
///     already reasoned the finding graph and FOLDED `gmeow:findingRootCause(advisory,
///     warrant)` into `graph/diagnostics`. We assert that materialized edge is present —
///     the shipped stage derived it, naming the abductive warrant as the advisory's root.
///
///  2. **Re-derivable by the authored rules independently.** We rebuild the fold from the
///     ACTUAL authored logic + diagnostics modules (discovered BY TYPE, exactly as
///     `crates/conformance/tests/diagnostics_meta_findings.rs` does) and re-reason the
///     emitted finding graph with the already-materialized meta rows STRIPPED (they arrive
///     as EDB otherwise, and `MetaDerivation` drops `is_edb` atoms). The authored
///     `ruleFindingTracesBase/Step` + `ruleFindingHasAntecedent` + `ruleFindingRootCause`
///     then re-derive `gmeow:findingRootCause(advisory, warrant)` FRESH from the raw
///     `gmeow:findingAntecedent` edge — proving the join is an executable rule inference,
///     not merely a materialized string.
#[test]
fn warrant_edge_resolves_through_the_meta_fold() {
    let emitted = emitted();

    let (advisory_iri, _) = advisory_finding(&emitted.diagnostics, "Commitment");
    let antecedents: Vec<&str> = objects(&emitted.diagnostics, advisory_iri, FINDING_ANTECEDENT)
        .into_iter()
        .filter_map(as_iri)
        .collect();
    assert_eq!(
        antecedents.len(),
        1,
        "the mediation advisory must carry exactly one warrant antecedent: {antecedents:?}"
    );
    let warrant_iri = antecedents[0];

    // (1) The shipped stage's own meta-fold materialized the root-cause edge into the carrier.
    assert!(
        objects(&emitted.diagnostics, advisory_iri, FINDING_ROOT_CAUSE)
            .iter()
            .any(|o| as_iri(o) == Some(warrant_iri)),
        "the shipped stage's meta-fold must materialize gmeow:findingRootCause({advisory_iri}, \
         {warrant_iri}) into graph/diagnostics"
    );

    // (2) The authored rules re-derive it FRESH from the raw findingAntecedent edge. Strip
    // the stage's already-materialized meta rows (findingRootCause / findingCluster /
    // clusterRoot / the cluster+root type markers) so they are not re-ingested as EDB
    // (MetaDerivation reports only `is_edb == false` atoms).
    let stripped: String = emitted
        .diagnostics_nq
        .lines()
        .filter(|line| {
            !line.contains("findingRootCause")
                && !line.contains("findingCluster")
                && !line.contains("clusterRoot")
                && !line.contains("FindingCluster")
                && !line.contains("RootFinding")
        })
        .map(|line| format!("{line}\n"))
        .collect();
    let derivation = authored_meta_program()
        .derive(&stripped)
        .expect("the authored meta-fold must reason the emitted diagnostics graph");
    assert!(
        derivation
            .root_cause
            .contains(&(advisory_iri.to_owned(), warrant_iri.to_owned())),
        "the authored meta-rules must RE-DERIVE gmeow:findingRootCause({advisory_iri}, \
         {warrant_iri}) from the raw findingAntecedent edge; derived root_cause = {:?}",
        derivation.root_cause
    );
}

/// Build the diagnostic meta-fold from the authored logic + diagnostics modules —
/// the class-based discovery (`?r a gmeow:DiagnosticMetaRule`) the production
/// `MetaProgram::from_source` performs, over the real ontology text.
fn authored_meta_program() -> MetaProgram {
    let root = repo_root();
    let mut combined =
        std::fs::read(root.join("slices/grounding/logic/module.ttl")).expect("read logic module");
    combined.push(b'\n');
    combined.extend_from_slice(
        &std::fs::read(root.join("slices/core/diagnostics/module.ttl"))
            .expect("read diagnostics module"),
    );
    let dataset: Arc<RdfDataset> =
        purrdf::parse_dataset(&combined, "text/turtle", None).expect("modules parse");
    MetaProgram::from_source_dataset(&dataset)
        .expect("the authored slices parse for the diagnostic meta-fold")
        .expect("the authored slices carry gmeow:DiagnosticMetaRule rules")
}

// ── Case 3: not auto-asserted (R4) at the stage level ─────────────────────────────────

/// The abductive additions live ONLY in `graph/diagnostics` + `graph/norm-claims`, never
/// the base A-Box. The stage product's dataset carries quads exclusively in those two
/// carrier graphs, and NONE of the base A-Box typing triples (nor any witness/scenario
/// individual the producer minted internally) is re-asserted into the product.
#[test]
fn abductive_additions_never_touch_the_base_abox() {
    let emitted = emitted();

    // Every product quad is in one of the two advisory carrier graphs — the stage neither
    // re-emits the source graph nor auto-asserts the hypothetical addition into a base graph.
    for quad in &emitted.all {
        let graph = quad.graph_name.as_ref().and_then(as_iri);
        assert!(
            graph == Some(GRAPH_DIAGNOSTICS) || graph == Some(GRAPH_NORM_CLAIMS),
            "every stage-product quad must ride graph/diagnostics or graph/norm-claims; \
             found one in {graph:?}: {quad:?}"
        );
    }

    // The base A-Box typing triples the producer READ are never re-asserted. The producer
    // proposes `<urn:c1> gmeow:commitmentBeneficiary <witness>` only inside an isolated
    // scenario world; it must not surface in the product as an assertion.
    let commitment_class = format!("{GMEOW}Commitment");
    assert!(
        !emitted.all.iter().any(|q| {
            as_iri(&q.subject) == Some(SUBJ_COMMITMENT)
                && q.predicate.as_str() == RDF_TYPE
                && as_iri(&q.object) == Some(commitment_class.as_str())
        }),
        "the base typing triple <{SUBJ_COMMITMENT}> a gmeow:Commitment must NOT be re-asserted \
         into the stage product"
    );
    let beneficiary = format!("{GMEOW}commitmentBeneficiary");
    assert!(
        !emitted.all.iter().any(|q| {
            as_iri(&q.subject) == Some(SUBJ_COMMITMENT) && q.predicate.as_str() == beneficiary
        }),
        "the abductive candidate <{SUBJ_COMMITMENT}> gmeow:commitmentBeneficiary … must NOT be \
         auto-asserted (R4): it lives only in the borrowed scenario world"
    );
}

// ── Case 4: a one-party Commitment (G4 — two missing relata) ──────────────────────────

/// G4's own canonical example, proved through the REAL `ValidateStage`: a
/// `gmeow:Commitment` with only ONE party present (`gmeow:committedAgent`) — TWO of its
/// three declared relata (`gmeow:commitmentBeneficiary`, `gmeow:intentionGoal`) missing —
/// still emits advice through the shipped stage. Per-conjunct completeness (G4) warrants
/// each missing relatum independently, so this fixture yields TWO Commitment advisory
/// findings (one per missing relatum), neither of which re-suggests the already-present
/// `gmeow:committedAgent`.
#[test]
fn one_party_commitment_emits_advice_for_both_missing_relata() {
    const SUBJ_ONE_PARTY: &str = "urn:c-one-party";
    let abox = format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         <{SUBJ_ONE_PARTY}> a gmeow:Commitment ; gmeow:graphBoxRole gmeow:boxABox ; gmeow:committedAgent <urn:soloAgent> .\n\
         <urn:soloAgent> a gmeow:Agent .\n"
    );
    let empty_reason =
        gmeow_pipeline::stages::reason::reason_product(b"").expect("empty stage-reason product");
    let diagnostics = run_with_reason(&abox, empty_reason);

    let want = format!("{ADVICE_ABDUCTIVE_PREFIX}Commitment.");
    let mine: Vec<(&str, &str)> = subjects_by_literal(&diagnostics, FINDING_CODE, |code| {
        code.starts_with(&want) && !code.starts_with(ADVICE_WARRANT_PREFIX)
    })
    .into_iter()
    .filter(|(finding_iri, _)| {
        objects(&diagnostics, finding_iri, FINDING_SUGGESTION)
            .iter()
            .any(|o| as_literal(o).is_some_and(|s| s.contains(SUBJ_ONE_PARTY)))
    })
    .collect();

    assert_eq!(
        mine.len(),
        2,
        "a one-party Commitment (two missing relata) must emit exactly two advisory \
         findings through the shipped ValidateStage — one per missing relatum: {mine:?}"
    );

    let suggestion_text: Vec<&str> = mine
        .iter()
        .flat_map(|(finding_iri, _)| {
            objects(&diagnostics, finding_iri, FINDING_SUGGESTION)
                .into_iter()
                .filter_map(as_literal)
        })
        .collect();
    assert!(
        suggestion_text
            .iter()
            .any(|s| s.contains("gmeow:commitmentBeneficiary")),
        "one advisory must suggest the missing gmeow:commitmentBeneficiary relatum: {suggestion_text:?}"
    );
    assert!(
        suggestion_text
            .iter()
            .any(|s| s.contains("gmeow:intentionGoal")),
        "one advisory must suggest the missing gmeow:intentionGoal relatum: {suggestion_text:?}"
    );
    assert!(
        !suggestion_text
            .iter()
            .any(|s| s.contains("gmeow:committedAgent")),
        "the already-present gmeow:committedAgent relatum must NOT be re-suggested: {suggestion_text:?}"
    );

    // Distinct advisory codes — the D4 claim emitter never clashes on the same code.
    let mut codes: Vec<&str> = mine.iter().map(|(_, code)| *code).collect();
    codes.sort_unstable();
    let n = codes.len();
    codes.dedup();
    assert_eq!(
        codes.len(),
        n,
        "advisory codes are injective per missing relatum"
    );
}
