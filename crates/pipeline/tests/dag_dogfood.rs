// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The build pipeline is authored as data in `slices/core/pipeline/module.ttl`, and that
//! file is what the producer executes: the spec is parsed from it, with no second
//! description in Rust to keep in step.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gmeow_pipeline::registry::registry_for;
use gmeow_pipeline::run::{BuildProfile, RunOutputScope, spec_for_scope};
use gmeow_pipeline::{PipelineSpec, bind, default_registry, full_spec};
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{SparqlEngine, SparqlRequest, SparqlResult, TermValue, parse_dataset};

const OUTPUT_SCOPE_QUERY: &str =
    include_str!("../../../slices/core/pipeline/queries/competency/output-scope-stages.rq");

fn query_scope_rows(ttl: &str) -> BTreeSet<(String, String)> {
    // gmeow-test-input: synthetic-only
    let dataset = parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .expect("the caller-owned synthetic plan parses");
    let result = NativeSparqlEngine::new()
        .query(
            &dataset,
            SparqlRequest {
                query: OUTPUT_SCOPE_QUERY,
                base_iri: None,
                substitutions: &[],
            },
        )
        .expect("the authored scope query executes natively");
    let SparqlResult::Solutions {
        variables, rows, ..
    } = result
    else {
        panic!("the scope query must return solutions");
    };
    let scope = variables
        .iter()
        .position(|name| name == "scope")
        .expect("scope column");
    let stage = variables
        .iter()
        .position(|name| name == "stage")
        .expect("stage column");
    rows.into_iter()
        .map(|row| {
            let Some(TermValue::Literal { lexical_form, .. }) = &row[scope] else {
                panic!("scope must be a bound literal");
            };
            let Some(TermValue::Iri(iri)) = &row[stage] else {
                panic!("stage must be a bound IRI");
            };
            (
                lexical_form.clone(),
                iri.strip_prefix("https://blackcatinformatics.ca/gmeow/")
                    .expect("the build stage belongs to the declared namespace")
                    .to_owned(),
            )
        })
        .collect()
}

fn native_scope_rows(spec: &PipelineSpec) -> BTreeSet<(String, String)> {
    let mut rows = None;
    for profile in [BuildProfile::Ordinary, BuildProfile::Release] {
        let registry = registry_for(profile);
        let mut selected = BTreeSet::new();
        for (label, scope) in [
            ("all", RunOutputScope::All),
            ("committed", RunOutputScope::Committed),
        ] {
            let scoped = spec_for_scope(spec.clone(), scope, &registry)
                .expect("every family agrees with the actual selected profile");
            scoped
                .validate()
                .expect("scope retains its complete producer closure");
            selected.extend(
                scoped
                    .stages
                    .into_iter()
                    .map(|stage| (label.to_owned(), stage.id)),
            );
        }
        if let Some(previous) = &rows {
            assert_eq!(
                &selected, previous,
                "both executable profiles select the same declared families"
            );
        }
        rows = Some(selected);
    }
    rows.expect("both supported build profiles were admitted")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The executed pipeline is the authored slice: the spec the producer runs parses from
/// the slice file on disk to the same value, validates as a DAG with one sink, and binds
/// every stage to a registered implementation.
#[test]
fn the_executed_pipeline_is_the_authored_slice() {
    let root = repo_root();
    let ttl = std::fs::read_to_string(root.join("slices/core/pipeline/module.ttl"))
        .expect("read pipeline slice");
    let authored = PipelineSpec::from_turtle(&[&ttl]).expect("parse the authored pipeline");
    let executed = full_spec();
    assert_eq!(
        authored, executed,
        "the producer executes the pipeline slice it was compiled from"
    );

    let graph = executed
        .validate()
        .expect("the authored pipeline validates");
    let bound = bind(&executed, &graph, &default_registry()).expect("the authored pipeline binds");
    assert_eq!(
        bound.len(),
        executed.stages.len(),
        "every authored stage is bound"
    );
}

/// A runtime-family dependency remains selected for a generated root, while an
/// independent runtime exporter and stages from another pipeline stay outside it.
#[test]
fn output_scope_query_keeps_required_runtime_producers_within_its_pipeline() {
    let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:pipeline-build a gmeow:Pipeline ;
    gmeow:hasStage gmeow:stage-gts-sink, gmeow:stage-export-export, gmeow:stage-export-okf .
gmeow:stage-gts-sink a gmeow:PipelineStage ;
    gmeow:stageImpl "gts_sink" ;
    gmeow:hasCapability gmeow:sinkCapability ;
    gmeow:stageStability gmeow:stabilityStablePrefix ;
    gmeow:stageCacheDisposition gmeow:cachePersistentContribution ;
    gmeow:stageOutputFamily gmeow:outputGenerated ;
    gmeow:dataflowConsumes gmeow:stage-export-export .
gmeow:stage-export-export a gmeow:PipelineStage ;
    gmeow:stageImpl "export" ;
    gmeow:stageStability gmeow:stabilityStablePrefix ;
    gmeow:stageCacheDisposition gmeow:cacheRecomputeAggregate ;
    gmeow:stageOutputFamily gmeow:outputRuntimeProjection .
gmeow:stage-export-okf a gmeow:PipelineStage ;
    gmeow:stageImpl "okf" ;
    gmeow:stageStability gmeow:stabilityStablePrefix ;
    gmeow:stageCacheDisposition gmeow:cacheRecomputeAggregate ;
    gmeow:stageOutputFamily gmeow:outputRuntimeProjection .
"#;
    let spec = PipelineSpec::from_turtle(&[ttl]).expect("source-only closure boundary");
    let expected: BTreeSet<_> = [
        ("all", "stage-gts-sink"),
        ("all", "stage-export-export"),
        ("all", "stage-export-okf"),
        ("committed", "stage-gts-sink"),
        ("committed", "stage-export-export"),
    ]
    .into_iter()
    .map(|(scope, stage)| (scope.to_owned(), stage.to_owned()))
    .collect();
    assert_eq!(native_scope_rows(&spec), expected);
    let with_unrelated = format!(
        "{ttl}\n\
        gmeow:unrelatedPipeline a gmeow:Pipeline ; gmeow:hasStage gmeow:unrelatedStage .\n\
        gmeow:unrelatedStage gmeow:stageOutputFamily gmeow:outputGenerated .\n"
    );
    assert_eq!(query_scope_rows(&with_unrelated), expected);
}
