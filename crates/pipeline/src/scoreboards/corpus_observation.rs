// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned worked claim audit; tests only restore its authenticated result.

use super::*;

pub const ARTIFACT: &str = "claim-audit-observation-v1.json";

const INPUT: &str = include_str!("../../../../tests/fixtures/coverage/hallucination-kg.ttl");
const QUERIES: &[(&str, &str)] = &[
    (
        "claims-without-evidence",
        include_str!("../../../../queries/audit/claims-without-evidence.rq"),
    ),
    (
        "claims-contradicted-by-higher-confidence",
        include_str!("../../../../queries/audit/claims-contradicted-by-higher-confidence.rq"),
    ),
    (
        "stale-source-claims",
        include_str!("../../../../queries/audit/stale-source-claims.rq"),
    ),
    (
        "evidence-coverage",
        include_str!("../../../../queries/audit/evidence-coverage.rq"),
    ),
    (
        "contradictions",
        include_str!("../../../../queries/audit/contradictions.rq"),
    ),
    (
        "claims-by-model",
        include_str!("../../../../queries/audit/claims-by-model.rq"),
    ),
];

#[derive(Serialize)]
struct Recorded<'a> {
    schema: u32,
    query_rows: &'a AuditRows,
    nodes: Vec<&'a gmeow_errors::DiagNode>,
    claims: &'a [FlatClaim],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    schema: u32,
    query_rows: AuditRows,
    nodes: Vec<gmeow_errors::DiagNode>,
    claims: Vec<FlatClaim>,
}

fn encode(report: &ClaimAuditReport) -> gmeow_errors::Result<Vec<u8>> {
    serde_json::to_vec(&Recorded {
        schema: 1,
        query_rows: &report.query_rows,
        nodes: report.ledger.emit_sorted(),
        claims: &report.claims,
    })
    .ctx("encode produced claim audit")
}

/// Run only in the explicit fixture producer, over its selected ontology and shapes.
/// The compiler's input inventory binds the original held-out input and queries.
pub fn produce_claim_audit_observation(
    ontology: &RdfDataset,
    shapes_text: &str,
) -> gmeow_errors::Result<Vec<u8>> {
    let data = parse_dataset(INPUT.as_bytes(), "text/turtle", None)
        .ctx("parse held-out claim audit input")?;
    let store = Arc::new(RdfDataset::union(&[ontology, data.as_ref()])?);
    let mut report = ClaimAuditReport::default();
    for (name, query) in QUERIES {
        let path = PathBuf::from("queries/audit").join(format!("{name}.rq"));
        report.query(&store, &path, query)?;
    }
    let (shapes, classes) = prepare_claim_shapes(shapes_text)?;
    report
        .ledger
        .union(&validate_claim_shapes(&store, &shapes, &classes)?);
    report.claims = flat_claims(&store, &report)?;
    encode(&report)
}

/// Restore native diagnostic nodes without parsing, assembling or producing corpus.
/// Callers must first authenticate the artifact through the selected fixture receipt.
pub fn restore(bytes: &[u8]) -> gmeow_errors::Result<ClaimAuditReport> {
    let stored: Stored = serde_json::from_slice(bytes).ctx("decode produced claim audit")?;
    if stored.schema != 1 {
        return Err(Diag::of_kind(crate::error::Scoreboard {
            message: format!("unsupported produced claim audit schema {}", stored.schema),
        }));
    }
    let mut ledger = DiagLedger::new();
    ledger.replay(stored.nodes);
    Ok(ClaimAuditReport {
        query_rows: stored.query_rows,
        ledger,
        claims: stored.claims,
    })
}

#[cfg(test)]
#[path = "corpus_observation.tests.rs"]
mod tests;
