// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn schema_publication_carries_located_losses_and_exact_artifacts() {
    use std::borrow::Cow;
    let mut losses = purrdf::LossLedger::new();
    for subject in ["urn:constraint:a", "urn:constraint:b"] {
        losses.record(purrdf::LossEntry {
            code: Cow::Borrowed("schema.unrepresentable"),
            from: Cow::Borrowed("shacl"),
            to: Cow::Borrowed("json-schema"),
            note: Cow::Borrowed("cross-node constraint is not representable"),
            location: Some(Box::new(
                purrdf::RdfLocation::logical("authored.ttl").with_subject(subject),
            )),
        });
    }
    let output = publish_schema(
        "stage-export-json-schema",
        purrdf::shapes::json_schema::CompiledSchema {
            schema_json: "{\"$defs\":{}}\n".into(),
            openapi_json: "{\"openapi\":\"3.1.0\"}\n".into(),
            losses,
        },
    )
    .unwrap();
    assert_eq!(
        output.product.artifacts()[JSON_SCHEMA_PATH],
        b"{\"$defs\":{}}\n"
    );
    assert_eq!(
        output.product.artifacts()[OPENAPI_PATH],
        b"{\"openapi\":\"3.1.0\"}\n"
    );
    assert_eq!(output.diags.len(), 2);
    assert_eq!(
        serde_json::to_vec(&output.diags).unwrap(),
        serde_json::to_vec(&output.product.diag_nodes()).unwrap()
    );
    let mut replay = gmeow_errors::DiagLedger::new();
    replay.replay(output.product.diag_nodes());
    assert_ne!(replay.verdict(), gmeow_errors::GateVerdict::Fatal);
    let report = replay.project_report("json-schema");
    let subjects: std::collections::BTreeSet<_> = report
        .findings
        .iter()
        .map(|finding| finding.source_focus.as_deref().unwrap())
        .collect();
    assert_eq!(
        subjects,
        std::collections::BTreeSet::from(["urn:constraint:a", "urn:constraint:b"])
    );
    for finding in &report.findings {
        assert!(
            finding
                .detail
                .as_ref()
                .unwrap()
                .contains("shacl → json-schema")
        );
    }
    let rdf = gmeow_errors::render::to_gmeow_rdf(&report);
    let quads: Vec<_> = output.product.dataset().owned_quads().collect();
    assert!(!quads.is_empty());
    for subject in ["urn:constraint:a", "urn:constraint:b"] {
        let value = purrdf::RdfTerm::literal(purrdf::RdfLiteral::simple(subject));
        assert!(quads.iter().any(|quad| {
            quad.predicate == "https://blackcatinformatics.ca/gmeow/findingSourceFocusIdentity"
                && quad.object == value
        }));
    }
    assert!(quads.iter().any(|quad| {
        quad.predicate == "https://blackcatinformatics.ca/gmeow/findingLocationLogical"
            && quad.object == purrdf::RdfTerm::literal(purrdf::RdfLiteral::simple("authored.ttl"))
    }));
    assert!(rdf.contains("schema.unrepresentable"));
    assert!(rdf.contains("shacl → json-schema"));
    let message = purrdf::RdfTerm::literal(purrdf::RdfLiteral::simple(
        "shacl → json-schema: cross-node constraint is not representable",
    ));
    assert!(quads.iter().any(|quad| {
        quad.predicate == "https://blackcatinformatics.ca/gmeow/findingMessage"
            && quad.object == message
    }));
    assert!(quads.iter().all(|quad| quad.graph_name.as_ref()
        == Some(&purrdf::RdfTerm::iri(
            crate::stages::carrier::GRAPH_DIAGNOSTICS
        ))));
}

#[test]
fn schema_publication_attests_empty_diagnostics_without_synthetic_findings() {
    let output = publish_schema(
        "stage-export-json-schema",
        purrdf::shapes::json_schema::CompiledSchema {
            schema_json: "{}\n".into(),
            openapi_json: "{}\n".into(),
            losses: purrdf::LossLedger::new(),
        },
    )
    .unwrap();
    assert!(output.diags.is_empty());
    assert!(output.product.diag_nodes().is_empty());
    assert!(
        crate::bundle::bundle_rep_blob(
            output.product.bundle(),
            crate::stages::carrier::REP_DIAG_NODES
        )
        .is_some()
    );
    assert!(
        output
            .product
            .dataset()
            .owned_named_graphs()
            .any(|graph| graph == purrdf::RdfTerm::iri(crate::stages::carrier::GRAPH_DIAGNOSTICS))
    );
}

#[test]
fn schema_publication_matches_authored_dataflow_and_attachments() {
    let declared = crate::loader::PipelineSpec::from_turtle(&[include_str!(
        "../../../../slices/core/pipeline/module.ttl"
    )])
    .unwrap();
    let declared = declared.stage("stage-export-json-schema").unwrap();
    let implementation = JsonSchemaStage::new();
    let mut actual = declared.dataflow_entities.clone();
    let mut expected = implementation.consumed_entities().to_vec();
    for (_, entities) in actual.iter_mut().chain(expected.iter_mut()) {
        entities.sort();
    }
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
    assert_eq!(
        declared.attaches_graphs.as_slice(),
        implementation.attaches_graphs()
    );
    assert_eq!(
        declared.attaches_blob_reps.as_slice(),
        implementation.attaches_blob_reps()
    );
}

#[test]
fn schema_publication_preserves_coalesced_loss_observations_in_rdf() {
    let mut losses = purrdf::LossLedger::new();
    for target in ["json-schema", "openapi"] {
        losses.record(purrdf::LossEntry {
            code: "schema.unrepresentable".into(),
            from: "shacl".into(),
            to: target.into(),
            note: "cross-node constraint is not representable".into(),
            location: Some(Box::new(
                purrdf::RdfLocation::logical("authored.ttl").with_subject("urn:constraint"),
            )),
        });
    }
    let output = publish_schema(
        "stage-export-json-schema",
        purrdf::shapes::json_schema::CompiledSchema {
            schema_json: "{}\n".into(),
            openapi_json: "{}\n".into(),
            losses,
        },
    )
    .unwrap();
    assert_eq!(output.diags.len(), 1, "one shared source witness");
    assert_eq!(output.diags[0].observations.len(), 2);
    let quads: Vec<_> = output.product.dataset().owned_quads().collect();
    for target in ["json-schema", "openapi"] {
        let message = purrdf::RdfTerm::literal(purrdf::RdfLiteral::simple(format!(
            "shacl → {target}: cross-node constraint is not representable"
        )));
        assert!(quads.iter().any(|quad| {
            matches!(
                quad.predicate.as_str(),
                "https://blackcatinformatics.ca/gmeow/findingMessage"
                    | "https://blackcatinformatics.ca/gmeow/labelMessage"
            ) && quad.object == message
        }));
    }
}

#[test]
fn schema_publication_preserves_opaque_locator_values_in_both_rdf_surfaces() {
    let graph = crate::stages::carrier::GRAPH_DIAGNOSTICS;
    let logical = "constraint role: <literal> \"quoted\"";
    let focus = "not an IRI [_:scope]";
    let mut finding = gmeow_errors::Finding::new(
        gmeow_errors::Severity::Note,
        "schema.locator",
        "source context",
    );
    finding.source_focus = Some(focus.into());
    finding.add_location(gmeow_errors::Location::new(
        None,
        Some(0),
        Some(0),
        Some(logical.into()),
    ));
    let mut report = gmeow_errors::Report::new("json-schema");
    report.add_finding(finding);
    let text = gmeow_errors::render::to_gmeow_rdf_in_graph(&report, graph);
    let parsed = purrdf::parse_dataset(text.as_bytes(), "application/n-quads", None).unwrap();
    let mut builder = purrdf::RdfDatasetBuilder::new();
    gmeow_errors::render::append_gmeow_findings(&report, graph, &mut builder).unwrap();
    let native = builder.freeze().unwrap();
    let rows: Vec<_> = native.owned_quads().collect();
    for (predicate, value) in [
        ("findingLocationLogical", logical),
        ("findingSourceFocusIdentity", focus),
    ] {
        assert!(rows.iter().any(|row| {
            row.predicate == format!("https://blackcatinformatics.ca/gmeow/{predicate}")
                && row.object == purrdf::RdfTerm::literal(purrdf::RdfLiteral::simple(value))
        }));
    }
    let parsed_rows: Vec<_> = parsed.owned_quads().collect();
    assert_eq!(rows.len(), parsed_rows.len());
    for row in rows {
        assert!(parsed_rows.contains(&row));
    }
}

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn json_schema_stage_emits_valid_authenticated_artifacts() {
    let root = repo_root();
    let first = crate::fixture::stage_artifacts(&root, 1, "stage-export-json-schema")
        .expect("authenticated JSON Schema projection");

    let schema_bytes = first
        .get(JSON_SCHEMA_PATH)
        .expect("schema artifact present");
    let openapi_bytes = first.get(OPENAPI_PATH).expect("openapi artifact present");

    // Both parse as JSON.
    let schema: serde_json::Value =
        serde_json::from_slice(schema_bytes).expect("schema is valid JSON");
    let _openapi: serde_json::Value =
        serde_json::from_slice(openapi_bytes).expect("openapi is valid JSON");

    // The schema has a non-empty `$defs` object.
    let defs = schema
        .get("$defs")
        .and_then(|v| v.as_object())
        .expect("schema has a $defs object");
    assert!(!defs.is_empty(), "$defs must be non-empty");

    for path in [CARD_SCHEMA_PATH, FINDING_SCHEMA_PATH] {
        assert!(first.contains_key(path), "missing {path}");
    }
}

/// Recursively collect every `#/$defs/<name>` ref reachable from a value.
fn collect_def_refs(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(r)) = map.get("$ref")
                && let Some(name) = r.strip_prefix("#/$defs/")
            {
                out.push(name.to_owned());
            }
            for child in map.values() {
                collect_def_refs(child, out);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                collect_def_refs(child, out);
            }
        }
        _ => {}
    }
}

/// Corpus self-consistency invariant: compiling over the REAL repo
/// shape union must produce ZERO dangling `$ref`s — every `#/$defs/<name>`
/// the schema references must resolve to an emitted `$def`. This guards the
/// real corpus against the dangling-ref bug a draft-2020-12 validator rejects
/// (`Pointer '/$defs/<name>' does not exist`).
#[test]
fn json_schema_corpus_has_no_dangling_refs() {
    let root = repo_root();
    let artifacts = crate::fixture::stage_artifacts(&root, 1, "stage-export-json-schema")
        .expect("authenticated JSON Schema projection");
    let schema: serde_json::Value = serde_json::from_slice(
        artifacts
            .get(JSON_SCHEMA_PATH)
            .expect("schema artifact present"),
    )
    .expect("schema is valid JSON");

    let defs: std::collections::BTreeSet<String> = schema
        .get("$defs")
        .and_then(|v| v.as_object())
        .expect("$defs object")
        .keys()
        .cloned()
        .collect();

    let mut refs = Vec::new();
    collect_def_refs(&schema, &mut refs);
    assert!(!refs.is_empty(), "expected refs in the real corpus schema");

    let dangling: Vec<&String> = refs.iter().filter(|r| !defs.contains(*r)).collect();
    assert!(
        dangling.is_empty(),
        "schema references {} dangling $defs over the real corpus: {:?}",
        dangling.len(),
        dangling
    );
}
