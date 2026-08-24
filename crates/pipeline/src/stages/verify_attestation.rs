// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native reasoned-graph verification as a dedicated downstream DAG stage.
//!
//! `stage-reason` is the sole closure constructor. This stage reassembles the exact
//! object-level EDB, applies the same transport-independent canonicalization boundary,
//! and evaluates the embedded verify battery against the already-built typed
//! [`gmeow_logic::result::ReasoningResult`]. Its persistent product is deliberately
//! bounded: `graph/verify`, a normalized JSON report, and the forward diagnostics-node
//! blob. A cache hit therefore avoids query evaluation without hydrating the reasoner's
//! large cumulative carrier.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Instant;

use purrdf::{ContentDigest, RdfDataset};

use crate::bundle::PipelineHandle;
use crate::node::{CachePolicy, Stage, StageInput, StageOutput, StageProduct, StageRunTiming};

/// Committed normalized verification receipt. It is also folded into the generated
/// fanout archive so the shipped bundle and the filesystem projection carry the same
/// evidence.
pub const VERIFY_JSON_PATH: &str = "generated/diagnostics/verify.json";

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const QUALITY_ASSESSMENT: &str = "https://blackcatinformatics.ca/gmeow/QualityAssessment";

/// Deterministic evidence returned by the independent shipped-attestation grader.
///
/// These are work identities, not observations: the same snapshot/result/query/record
/// tuple must yield the same values on every host and cache state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttestationFreshness {
    /// Digest of the freshly projected `graph/verify` bytes.
    pub graph_digest: String,
    /// Digest of the freshly rendered normalized JSON record.
    pub record_digest: String,
    /// Number of verify queries independently evaluated by the caller.
    pub query_count: usize,
    /// This grader consumes a typed result and must never construct a closure.
    pub closure_constructions: usize,
}

/// The dedicated verify-attestation transform.
pub struct VerifyAttestationStage {
    consumes: Vec<String>,
    entities: Vec<(String, Vec<String>)>,
}

impl VerifyAttestationStage {
    /// Construct the stage over the three object-EDB producers and the reason stage's
    /// typed result. The compile and reason dependencies are narrowed to the exact
    /// named-graph entities read by the transform.
    #[must_use]
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-reason".to_string(),
                "stage-source-load".to_string(),
                "stage-statements".to_string(),
            ],
            entities: vec![
                (
                    "stage-compile-logic".to_string(),
                    crate::stages::compile_logic::object_level_entity_list(),
                ),
                (
                    "stage-reason".to_string(),
                    vec![gmeow_logic::result_rdf::GRAPH_REASONING.to_string()],
                ),
            ],
        }
    }
}

impl Default for VerifyAttestationStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for VerifyAttestationStage {
    fn id(&self) -> &str {
        "stage-verify-attestation"
    }

    fn consumes(&self) -> &[String] {
        &self.consumes
    }

    fn consumed_entities(&self) -> &[(String, Vec<String>)] {
        &self.entities
    }

    fn cache_policy(&self) -> CachePolicy {
        CachePolicy::Persistent
    }

    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }

    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }

    fn impl_version(&self) -> &str {
        "verify-attestation.v1-reuse-reasoning-result"
    }

    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let mut timings = Vec::with_capacity(3);
        let edb_started = Instant::now();
        let edb = crate::stages::carrier::assemble_object_level_edb(input.upstream)?;
        let edb_quads = edb.quad_count();
        timings.push(StageRunTiming {
            phase: "assemble-object-edb".to_string(),
            elapsed_ms: edb_started.elapsed().as_millis(),
            metadata: Some(format!("edb-quads={edb_quads}")),
        });
        let canonical_started = Instant::now();
        let canonical = crate::stages::reason::canonicalize_edb(edb.as_ref(), self.id())?;
        let canonical_quads = canonical.quad_count();
        timings.push(StageRunTiming {
            phase: "canonicalize-object-edb".to_string(),
            elapsed_ms: canonical_started.elapsed().as_millis(),
            metadata: Some(format!("canonical-quads={canonical_quads}")),
        });
        let reason_product = input.upstream.get("stage-reason").ok_or_else(|| {
            stage_err("missing stage-reason product for the typed ReasoningResult")
        })?;
        let entry = reason_product
            .bundle()
            .handle(gmeow_logic::result_rdf::GRAPH_REASONING)
            .ok_or_else(|| stage_err("stage-reason product carries no Reasoning handle"))?;
        let PipelineHandle::Reasoning(reasoning) = &entry.payload else {
            return Err(stage_err(
                "stage-reason graph/reasoning handle is not the Reasoning arm",
            ));
        };

        let queries = gmeow_logic::verify::embedded_verify_queries();
        let evaluate_started = Instant::now();
        let mut output = build_output(self.id(), canonical.as_ref(), reasoning.as_ref(), &queries)?;
        let output_bytes = output
            .product
            .artifact(VERIFY_JSON_PATH)
            .map_or(0, |bytes| bytes.len());
        timings.push(StageRunTiming {
            phase: "evaluate-verify-attestation".to_string(),
            elapsed_ms: evaluate_started.elapsed().as_millis(),
            metadata: Some(format!(
                "closure-constructions=0;queries={};edb-quads={canonical_quads};\
                 inferred-axioms={};artifact-bytes={output_bytes}",
                queries.len(),
                reasoning.inferred().len(),
            )),
        });
        output.timings = timings;
        Ok(output)
    }
}

fn build_output(
    stage_id: &str,
    edb: &RdfDataset,
    reasoning: &gmeow_logic::result::ReasoningResult,
    queries: &[(String, String)],
) -> Result<StageOutput, gmeow_errors::Diag> {
    let report = gmeow_logic::verify::verify_with_reasoning_result(edb, reasoning, queries)
        .map_err(|e| stage_err(format!("native verify: {e}")))?;
    build_output_from_report(stage_id, edb, reasoning, queries, report)
}

/// Render the producer's graph and normalized record from an already-evaluated report.
/// Keeping evaluation outside this helper lets the independent CLI grader evaluate the
/// battery exactly once, then compare both shipped projections without a hidden second
/// query pass.
fn build_output_from_report(
    stage_id: &str,
    edb: &RdfDataset,
    reasoning: &gmeow_logic::result::ReasoningResult,
    queries: &[(String, String)],
    mut report: gmeow_errors::Report,
) -> Result<StageOutput, gmeow_errors::Diag> {
    let failed: BTreeSet<String> = report
        .findings
        .iter()
        .filter(|finding| {
            finding.severity == gmeow_errors::Severity::Error && finding.code.starts_with("verify.")
        })
        .map(|finding| finding.code["verify.".len()..].to_string())
        .collect();
    let turtle = emit_verify_attestation(queries, &failed);
    let dataset = crate::stages::carrier::parse_into_graph(
        turtle.as_bytes(),
        "text/turtle",
        crate::stages::carrier::GRAPH_VERIFY,
    )?;

    let canonical_nquads = purrdf::canonical_flat_nquads(edb)
        .map_err(|e| stage_err(format!("canonicalize verification input digest: {e}")))?;
    let reasoning_projection = gmeow_logic::result_rdf::project_reasoning_result(reasoning);
    let query_digest = query_set_digest(queries);
    let finding_count = report.findings.len();
    let error_count = report.error_count();
    let warning_count = report.warning_count();
    report.metadata.insert(
        "schemaVersion".to_string(),
        serde_json::json!("gmeow.verify-attestation.v1"),
    );
    report.metadata.insert(
        "verifyInputDigest".to_string(),
        serde_json::json!(ContentDigest::of(canonical_nquads.as_bytes()).to_hex()),
    );
    report.metadata.insert(
        "verifyReasoningDigest".to_string(),
        serde_json::json!(ContentDigest::of(reasoning_projection.as_bytes()).to_hex()),
    );
    report.metadata.insert(
        "verifyContractDigest".to_string(),
        serde_json::json!(gmeow_logic::reason::native_contract_hash()),
    );
    report.metadata.insert(
        "verifyQuerySetDigest".to_string(),
        serde_json::json!(query_digest),
    );
    report.metadata.insert(
        "verifyQueryCount".to_string(),
        serde_json::json!(queries.len()),
    );
    report.metadata.insert(
        "verifyEdbQuads".to_string(),
        serde_json::json!(edb.owned_quads().count()),
    );
    report.metadata.insert(
        "verifyInferredAxioms".to_string(),
        serde_json::json!(reasoning.inferred().len()),
    );
    report
        .metadata
        .insert("closureConstructions".to_string(), serde_json::json!(0));
    report.metadata.insert(
        "verifyFindingCount".to_string(),
        serde_json::json!(finding_count),
    );
    report.metadata.insert(
        "verifyErrorCount".to_string(),
        serde_json::json!(error_count),
    );
    report.metadata.insert(
        "verifyWarningCount".to_string(),
        serde_json::json!(warning_count),
    );
    report.normalize();

    let json = gmeow_errors::render::to_json(&report)
        .map_err(|e| stage_err(format!("render normalized verify report JSON: {e}")))?;
    let nodes = crate::stages::diag_render::finding_nodes(&report, stage_id);
    let diag_blob = serde_json::to_vec(&nodes)
        .map_err(|e| stage_err(format!("encode verify diagnostic nodes: {e}")))?;
    let artifacts = BTreeMap::from([(VERIFY_JSON_PATH.to_string(), json.into_bytes())]);
    let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
        dataset,
        artifacts,
        purrdf::provenance::DatasetProvenance::new(),
        crate::stages::carrier::REP_DIAG_NODES,
        "application/json",
        diag_blob,
    );
    Ok(StageOutput {
        product: StageProduct::from_bundle(stage_id, Arc::new(bundle)),
        diags: nodes,
        timings: Vec::new(),
    })
}

/// Independently grade the shipped verify graph and normalized record against an
/// already-evaluated EDB/result/query report.
///
/// This is the freshness half of `make reason-verify`: the CLI does not trust the
/// producer's positive attestation. It evaluates every query against the shipped typed
/// result, calls this function with that report, and requires the newly rendered graph
/// and JSON bytes to equal the materialized producer outputs exactly. The function has
/// no reasoner entry point and therefore constructs zero closures.
pub fn grade_shipped_attestation(
    snapshot: &RdfDataset,
    edb: &RdfDataset,
    reasoning: &gmeow_logic::result::ReasoningResult,
    queries: &[(String, String)],
    report: &gmeow_errors::Report,
    shipped_record: &[u8],
) -> Result<AttestationFreshness, gmeow_errors::Diag> {
    let fresh = build_output_from_report(
        "stage-verify-attestation",
        edb,
        reasoning,
        queries,
        report.clone(),
    )?;
    let expected_record = fresh
        .product
        .artifact(VERIFY_JSON_PATH)
        .ok_or_else(|| stage_err("fresh verifier emitted no normalized JSON record"))?;
    let expected_record_digest = ContentDigest::of(expected_record).to_hex();
    let shipped_record_digest = ContentDigest::of(shipped_record).to_hex();
    if expected_record != shipped_record {
        return Err(stage_err(format!(
            "shipped verify record is stale: expected {expected_record_digest}, found \
             {shipped_record_digest}"
        )));
    }

    let expected_graph = fresh
        .product
        .dataset()
        .project_named_graph(crate::stages::carrier::GRAPH_VERIFY);
    let shipped_graph = snapshot.project_named_graph(crate::stages::carrier::GRAPH_VERIFY);
    if shipped_graph.quad_count() == 0 {
        return Err(stage_err("snapshot carries no graph/verify attestation"));
    }
    let expected_graph_bytes = purrdf::canonical_flat_nquads(&expected_graph)
        .map_err(|error| stage_err(format!("canonicalize fresh graph/verify: {error}")))?;
    let shipped_graph_bytes = purrdf::canonical_flat_nquads(&shipped_graph)
        .map_err(|error| stage_err(format!("canonicalize shipped graph/verify: {error}")))?;
    let expected_graph_digest = ContentDigest::of(expected_graph_bytes.as_bytes()).to_hex();
    let shipped_graph_digest = ContentDigest::of(shipped_graph_bytes.as_bytes()).to_hex();
    if expected_graph_bytes != shipped_graph_bytes {
        return Err(stage_err(format!(
            "shipped graph/verify is stale: expected {expected_graph_digest}, found \
             {shipped_graph_digest}"
        )));
    }

    Ok(AttestationFreshness {
        graph_digest: expected_graph_digest,
        record_digest: expected_record_digest,
        query_count: queries.len(),
        closure_constructions: 0,
    })
}

/// Evaluate every selected bad-example query against the exact EDB/result pair and
/// project one deterministic quality assessment per query. This function never invokes
/// the reasoner.
#[cfg(test)]
fn evaluate_attestation(
    edb: &RdfDataset,
    reasoning: &gmeow_logic::result::ReasoningResult,
    queries: &[(String, String)],
) -> Result<(Arc<RdfDataset>, gmeow_errors::Report), gmeow_errors::Diag> {
    let report = gmeow_logic::verify::verify_with_reasoning_result(edb, reasoning, queries)
        .map_err(|e| stage_err(format!("native verify: {e}")))?;
    let failed: BTreeSet<String> = report
        .findings
        .iter()
        .filter(|finding| {
            finding.severity == gmeow_errors::Severity::Error && finding.code.starts_with("verify.")
        })
        .map(|finding| finding.code["verify.".len()..].to_string())
        .collect();
    let turtle = emit_verify_attestation(queries, &failed);
    let dataset = crate::stages::carrier::parse_into_graph(
        turtle.as_bytes(),
        "text/turtle",
        crate::stages::carrier::GRAPH_VERIFY,
    )?;
    Ok((dataset, report))
}

fn query_set_digest(queries: &[(String, String)]) -> String {
    let mut framed = Vec::new();
    framed.extend_from_slice(b"gmeow.verify-query-set.v1\0");
    for (name, query) in queries {
        framed.extend_from_slice(&(name.len() as u64).to_le_bytes());
        framed.extend_from_slice(name.as_bytes());
        framed.extend_from_slice(&(query.len() as u64).to_le_bytes());
        framed.extend_from_slice(query.as_bytes());
    }
    ContentDigest::of(&framed).to_hex()
}

/// Emit the verify-attestation Turtle. This is assertional generated data, with one
/// `gmeow:QualityAssessment` per selected query.
fn emit_verify_attestation(queries: &[(String, String)], failed: &BTreeSet<String>) -> String {
    let mut body = String::new();
    writeln!(body, "@prefix gmeow: <{GMEOW_NS}> .").unwrap();
    writeln!(body, "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .").unwrap();
    writeln!(
        body,
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> ."
    )
    .unwrap();
    writeln!(body).unwrap();

    let ontology_iri = GMEOW_NS.trim_end_matches('/');
    writeln!(
        body,
        "<{GMEOW_NS}activity/native-verify> a <{GMEOW_NS}Activity> ;"
    )
    .unwrap();
    writeln!(body, "    rdfs:label \"Native verify activity\" ;").unwrap();
    writeln!(body, "    rdfs:isDefinedBy <{GMEOW_NS}graph/verify> ;").unwrap();
    writeln!(body, "    gmeow:graphBoxRole gmeow:boxABox ;").unwrap();
    writeln!(
        body,
        "    <{GMEOW_NS}wasAssociatedWith> <{GMEOW_NS}agent/native-verify> ."
    )
    .unwrap();
    writeln!(body).unwrap();

    for (name, _) in queries {
        let stem = query_stem(name);
        let passed = !failed.contains(stem);
        writeln!(body, "<{GMEOW_NS}verify-attestation/{stem}>").unwrap();
        writeln!(body, "    a <{QUALITY_ASSESSMENT}> ;").unwrap();
        writeln!(body, "    rdfs:label \"Verify attestation: {stem}\" ;").unwrap();
        writeln!(body, "    rdfs:isDefinedBy <{GMEOW_NS}graph/verify> ;").unwrap();
        writeln!(body, "    gmeow:graphBoxRole gmeow:boxABox ;").unwrap();
        writeln!(body, "    <{GMEOW_NS}assessedEntity> <{ontology_iri}> ;").unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}qualityDimension> <{GMEOW_NS}qualityDimensionLogicalConsistency> ;"
        )
        .unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}observationResult> \"{}\"^^xsd:boolean ;",
            if passed { "true" } else { "false" }
        )
        .unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}wasDerivedFrom> <{GMEOW_NS}verify-query/{stem}> ;"
        )
        .unwrap();
        writeln!(
            body,
            "    <{GMEOW_NS}wasGeneratedBy> <{GMEOW_NS}activity/native-verify> ."
        )
        .unwrap();
        writeln!(body).unwrap();
    }
    body
}

fn query_stem(name: &str) -> &str {
    name.rsplit('/')
        .next()
        .unwrap_or(name)
        .strip_suffix(".rq")
        .unwrap_or(name)
}

fn stage_err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-verify-attestation".to_string(),
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::RdfTerm;

    fn fixture() -> (Arc<RdfDataset>, gmeow_logic::result::ReasoningResult) {
        let edb = purrdf::parse_dataset(
            b"<urn:s> <urn:p> <urn:o> <urn:world> .\n",
            "application/n-quads",
            None,
        )
        .expect("fixture EDB");
        let reasoning = gmeow_logic::reason::reason_all(edb.as_ref()).expect("fixture closure");
        (edb, reasoning)
    }

    fn assessments(dataset: &RdfDataset) -> usize {
        dataset
            .project_named_graph(crate::stages::carrier::GRAPH_VERIFY)
            .owned_quads()
            .filter(|quad| {
                quad.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                    && matches!(&quad.object, RdfTerm::Iri(iri) if iri == QUALITY_ASSESSMENT)
            })
            .count()
    }

    #[test]
    fn clean_and_poisoned_queries_have_attestation_teeth_without_re_reasoning() {
        let (edb, reasoning) = fixture();
        let clean = vec![(
            "clean.rq".to_string(),
            "SELECT ?s WHERE { ?s <urn:missing> <urn:o> . }".to_string(),
        )];
        let (clean_graph, clean_report) =
            evaluate_attestation(edb.as_ref(), &reasoning, &clean).expect("clean verify");
        assert_eq!(assessments(clean_graph.as_ref()), 1);
        assert!(clean_report.ok(), "clean query must pass: {clean_report:?}");

        let poisoned = vec![(
            "poisoned.rq".to_string(),
            "SELECT ?s WHERE { ?s <urn:p> <urn:o> . }".to_string(),
        )];
        let (poisoned_graph, poisoned_report) =
            evaluate_attestation(edb.as_ref(), &reasoning, &poisoned).expect("poisoned verify");
        assert_eq!(assessments(poisoned_graph.as_ref()), 1);
        assert!(
            poisoned_report
                .findings
                .iter()
                .any(|finding| finding.code == "verify.poisoned"
                    && finding.severity == gmeow_errors::Severity::Error),
            "the poisoned query must produce a hard verification finding: {poisoned_report:?}"
        );
    }

    #[test]
    fn output_receipt_records_zero_closure_constructions() {
        let (edb, reasoning) = fixture();
        let queries = vec![(
            "clean.rq".to_string(),
            "SELECT ?s WHERE { ?s <urn:missing> <urn:o> . }".to_string(),
        )];
        let output = build_output(
            "stage-verify-attestation",
            edb.as_ref(),
            &reasoning,
            &queries,
        )
        .expect("verify product");
        let report: serde_json::Value = serde_json::from_slice(
            output
                .product
                .artifact(VERIFY_JSON_PATH)
                .expect("verify JSON artifact"),
        )
        .expect("normalized report JSON");
        assert_eq!(report["metadata"]["closureConstructions"], 0);
        assert_eq!(report["metadata"]["verifyQueryCount"], 1);
        assert!(!output.product.diag_nodes().is_empty());
    }

    #[test]
    fn independent_grader_accepts_exact_outputs_and_rejects_stale_projections() {
        let (edb, reasoning) = fixture();
        let queries = vec![(
            "clean.rq".to_string(),
            "SELECT ?s WHERE { ?s <urn:missing> <urn:o> . }".to_string(),
        )];
        let report =
            gmeow_logic::verify::verify_with_reasoning_result(edb.as_ref(), &reasoning, &queries)
                .expect("evaluate exact report");
        let output = build_output_from_report(
            "stage-verify-attestation",
            edb.as_ref(),
            &reasoning,
            &queries,
            report.clone(),
        )
        .expect("render producer outputs");
        let snapshot = output.product.dataset();
        let record = output
            .product
            .artifact(VERIFY_JSON_PATH)
            .expect("verify record");

        let exact = grade_shipped_attestation(
            snapshot,
            edb.as_ref(),
            &reasoning,
            &queries,
            &report,
            record,
        )
        .expect("exact producer projections grade fresh");
        assert_eq!(exact.query_count, 1);
        assert_eq!(exact.closure_constructions, 0);

        let mut tampered_record = record.to_vec();
        tampered_record.push(b' ');
        let record_error = grade_shipped_attestation(
            snapshot,
            edb.as_ref(),
            &reasoning,
            &queries,
            &report,
            &tampered_record,
        )
        .expect_err("tampered normalized record must fail");
        assert!(record_error.to_string().contains("verify record is stale"));

        let empty_snapshot = purrdf::parse_dataset(b"", "application/n-quads", None)
            .expect("empty snapshot dataset");
        let graph_error = grade_shipped_attestation(
            empty_snapshot.as_ref(),
            edb.as_ref(),
            &reasoning,
            &queries,
            &report,
            record,
        )
        .expect_err("missing graph/verify must fail");
        assert!(graph_error.to_string().contains("no graph/verify"));
    }
}
