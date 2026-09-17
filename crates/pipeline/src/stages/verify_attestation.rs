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
                crate::stages::parse_sources::STAGE_ID.to_string(),
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
        "verify-attestation.v4-shared-native-laws"
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
        let prepare_started = Instant::now();
        let gates = crate::stages::parse_sources::catalog(&input)?.prepared_reasoned_gates()?;
        let verification =
            gmeow_logic::verify::PreparedVerification::new(&queries, gates.as_ref())?;
        timings.push(StageRunTiming {
            phase: "prepare-native-verification".into(),
            elapsed_ms: prepare_started.elapsed().as_millis(),
            metadata: Some(format!(
                "source-modules={};queries={}",
                gates.source_digests().len(),
                queries.len()
            )),
        });
        let evaluate_started = Instant::now();
        let mut output = build_output(
            self.id(),
            canonical.as_ref(),
            reasoning.as_ref(),
            &queries,
            &verification,
        )?;
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
    verification: &gmeow_logic::verify::PreparedVerification<'_>,
) -> Result<StageOutput, gmeow_errors::Diag> {
    let report = verification
        .verify_with_reasoning_result(edb, reasoning)
        .map_err(|e| stage_err(format!("native verify: {e}")))?;
    build_output_from_report(
        stage_id,
        edb,
        reasoning,
        queries,
        report,
        verification.gates(),
    )
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
    gates: &gmeow_logic::verify::PreparedReasonedGates,
) -> Result<StageOutput, gmeow_errors::Diag> {
    gates.validate_source_identity()?;
    let prepared_gates = serde_json::to_vec(gates)
        .map_err(|error| stage_err(format!("encode prepared native laws: {error}")))?;
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
    // The shipped CLI reconstructs the verdict-and-provenance handle from
    // `graph/reasoning`. That projection deliberately omits EDB payload rows, so grade
    // the transport-normalized value the consumer can actually recover rather than the
    // richer in-process handle. Query evaluation is unchanged: it reads only non-EDB
    // inferred rows, all of which the projection carries.
    let live_projection = gmeow_logic::result_rdf::project_reasoning_dataset(reasoning)?;
    let transported_reasoning = gmeow_logic::result_rdf::parse_reasoning_dataset(
        &live_projection,
        purrdf::GraphMatch::Default,
    )
    .map_err(|error| stage_err(format!("read native graph/reasoning summary: {error}")))?;
    let reasoning_projection =
        gmeow_logic::result_rdf::project_reasoning_result(&transported_reasoning)?;
    let query_digest = query_set_digest(queries);
    let finding_count = report.findings.len();
    let error_count = report.error_count();
    let warning_count = report.warning_count();
    report.metadata.insert(
        "schemaVersion".to_string(),
        serde_json::json!("gmeow.verify-attestation.v2"),
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
        "verifyPreparedLawsDigest".to_owned(),
        serde_json::json!(ContentDigest::of(&prepared_gates).to_hex()),
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
        serde_json::json!(transported_reasoning.inferred().len()),
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
    let artifacts = BTreeMap::from([
        (VERIFY_JSON_PATH.to_string(), json.into_bytes()),
        (
            gmeow_logic::verify::PREPARED_GATES_CHANNEL.to_owned(),
            prepared_gates,
        ),
    ]);
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
    gates: &gmeow_logic::verify::PreparedReasonedGates,
) -> Result<AttestationFreshness, gmeow_errors::Diag> {
    let fresh = build_output_from_report(
        "stage-verify-attestation",
        edb,
        reasoning,
        queries,
        report.clone(),
        gates,
    )?;
    let expected_record = fresh
        .product
        .artifact(VERIFY_JSON_PATH)
        .ok_or_else(|| stage_err("fresh verifier emitted no normalized JSON record"))?;
    let expected_record_digest = ContentDigest::of(expected_record).to_hex();
    let shipped_record_digest = ContentDigest::of(shipped_record).to_hex();
    if expected_record != shipped_record {
        let differences = json_difference_summary(expected_record, shipped_record);
        return Err(stage_err(format!(
            "shipped verify record is stale: expected {expected_record_digest}, found \
             {shipped_record_digest}; {differences}"
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

/// Describe the first deterministic JSON-value differences without dumping a whole
/// report into a diagnostic. Byte-only drift remains visible as such.
fn json_difference_summary(expected: &[u8], shipped: &[u8]) -> String {
    let expected: serde_json::Value = match serde_json::from_slice(expected) {
        Ok(value) => value,
        Err(error) => return format!("fresh record is not JSON: {error}"),
    };
    let shipped: serde_json::Value = match serde_json::from_slice(shipped) {
        Ok(value) => value,
        Err(error) => return format!("shipped record is not JSON: {error}"),
    };
    let mut differences = Vec::new();
    collect_json_differences("", &expected, &shipped, &mut differences, 12);
    if differences.is_empty() {
        "JSON values are equal; byte encoding differs".to_string()
    } else {
        format!("JSON differences: {}", differences.join("; "))
    }
}

fn collect_json_differences(
    pointer: &str,
    expected: &serde_json::Value,
    shipped: &serde_json::Value,
    differences: &mut Vec<String>,
    limit: usize,
) {
    if differences.len() >= limit || expected == shipped {
        return;
    }
    match (expected, shipped) {
        (serde_json::Value::Object(expected), serde_json::Value::Object(shipped)) => {
            let keys: BTreeSet<&str> = expected
                .keys()
                .chain(shipped.keys())
                .map(String::as_str)
                .collect();
            for key in keys {
                if differences.len() >= limit {
                    break;
                }
                let child = format!("{pointer}/{}", json_pointer_token(key));
                match (expected.get(key), shipped.get(key)) {
                    (Some(expected), Some(shipped)) => {
                        collect_json_differences(&child, expected, shipped, differences, limit)
                    }
                    (Some(expected), None) => differences.push(format!(
                        "{child}: expected {}, shipped <missing>",
                        short_json(expected)
                    )),
                    (None, Some(shipped)) => differences.push(format!(
                        "{child}: expected <missing>, shipped {}",
                        short_json(shipped)
                    )),
                    (None, None) => {}
                }
            }
        }
        (serde_json::Value::Array(expected), serde_json::Value::Array(shipped)) => {
            if expected.len() != shipped.len() {
                differences.push(format!(
                    "{pointer}/length: expected {}, shipped {}",
                    expected.len(),
                    shipped.len()
                ));
            }
            for (index, (expected, shipped)) in expected.iter().zip(shipped).enumerate() {
                if differences.len() >= limit {
                    break;
                }
                collect_json_differences(
                    &format!("{pointer}/{index}"),
                    expected,
                    shipped,
                    differences,
                    limit,
                );
            }
        }
        _ => differences.push(format!(
            "{}: expected {}, shipped {}",
            if pointer.is_empty() { "/" } else { pointer },
            short_json(expected),
            short_json(shipped)
        )),
    }
}

fn json_pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

fn short_json(value: &serde_json::Value) -> String {
    const MAX_CHARS: usize = 160;
    let rendered = value.to_string();
    if rendered.chars().count() <= MAX_CHARS {
        rendered
    } else {
        let mut prefix: String = rendered.chars().take(MAX_CHARS).collect();
        prefix.push('…');
        prefix
    }
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

#[path = "verify_attestation.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "verify_attestation_test_support.rs"]
mod test_support;
#[cfg(test)]
use test_support::evaluate_attestation;
#[cfg(test)]
use test_support::test_gates;
