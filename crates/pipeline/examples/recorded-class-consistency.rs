// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit consumer of recorded pipeline products. Never executes a DAG or
//! fixture producer, derives a current build identity, or repairs a cache miss.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use gmeow_errors::{Diag, Result};
use gmeow_logic::reasoner_services::DlReasoner;
use gmeow_pipeline::cache::{CACHE_CODEC_IDENTITY, PipelineCache, StageReceipt};
use gmeow_pipeline::error::{InvalidDeclaration, RecordedInput};
use gmeow_pipeline::node::StageProduct;
use gmeow_pipeline::scheduler::ActionIdentity;
use gmeow_pipeline::stages::carrier::assemble_object_level_edb;
use purrdf::TermRef;
use purrdf::entail::Question;
use serde::Deserialize;
use serde_json::{Value, json};

/// Select only a stage receipt; the cache independently verifies its actual
/// common envelope and product before this expected payload is compared.
#[derive(Deserialize)]
struct Selector {
    receipt: Payload,
}

#[derive(Deserialize)]
struct Payload {
    payload: StageReceipt,
}

/// Report an invalid recorded-input contract or consumer invocation.
fn invalid(message: impl Into<String>) -> Diag {
    Diag::of_kind(InvalidDeclaration {
        message: message.into(),
    })
}

/// Retain the original typed failure through the shared diagnostic cause chain.
fn foreign(operation: &'static str, cause: impl std::error::Error + Send + Sync + 'static) -> Diag {
    Diag::of_kind(RecordedInput {
        operation,
        cause: Arc::new(cause),
    })
}

/// Write one JSON observation, retaining encoding and output failures.
fn emit(value: &Value) -> Result<()> {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, value).map_err(|error| foreign("report encoding", error))?;
    stdout
        .write_all(b"\n")
        .map_err(|error| foreign("report writing", error))
}

/// Read an expected payload; the native cache authenticates the actual receipt.
fn selected_receipt(path: &Path) -> Result<StageReceipt> {
    let bytes = std::fs::read(path).map_err(|error| {
        foreign("receipt reading", error).with_focus(path.display().to_string())
    })?;
    let selected: Selector = serde_json::from_slice(&bytes).map_err(|error| {
        foreign("receipt decoding", error).with_focus(path.display().to_string())
    })?;
    let receipt = selected.receipt.payload;
    if receipt.context.build.profile != "pipeline" || receipt.context.codec != CACHE_CODEC_IDENTITY
    {
        return Err(invalid(
            "recorded inputs require the pipeline profile and the current native cache codec",
        )
        .with_focus(path.display().to_string()));
    }
    Ok(receipt)
}

/// Preflight a coherent selection before admitting its three products read-only.
fn load_inputs(cache_root: &Path, paths: &[PathBuf]) -> Result<BTreeMap<String, StageProduct>> {
    let selections = paths
        .iter()
        .map(|path| selected_receipt(path).map(|receipt| (path, receipt)))
        .collect::<Result<Vec<_>>>()?;
    let mut receipts = BTreeMap::new();
    let mut raw_inputs = BTreeMap::new();
    let mut build = None;
    for (_, expected) in &selections {
        let stage = &expected.context.stage_id;
        if !matches!(
            stage.as_str(),
            "stage-source-load" | "stage-statements" | "stage-compile-logic"
        ) {
            return Err(invalid(format!(
                "unexpected recorded reasoning input {stage}"
            )));
        }
        if let Some(previous) = &build {
            if previous != &expected.context.build {
                return Err(invalid(
                    "recorded reasoning inputs must share one producer build identity",
                ));
            }
        } else {
            build = Some(expected.context.build.clone());
        }
        for raw in &expected.context.raw_inputs {
            if let Some(previous) = raw_inputs.insert(raw.path.clone(), raw.digest.clone())
                && previous != raw.digest
            {
                return Err(invalid(format!(
                    "recorded products disagree about source {}",
                    raw.path
                )));
            }
        }
        if receipts.insert(stage.clone(), expected).is_some() {
            return Err(invalid(format!(
                "duplicate recorded reasoning input {stage}"
            )));
        }
    }
    if receipts.len() != 3 {
        return Err(invalid(
            "exactly source-load, statements and compile-logic products are required",
        ));
    }
    for receipt in receipts.values() {
        for dependency in &receipt.context.upstream {
            if let Some(producer) = receipts.get(&dependency.producer) {
                let actual = match &dependency.entity {
                    Some(entity) => producer.entity_digest(entity)?.ok_or_else(|| {
                        invalid(format!("missing recorded dependency entity {entity}"))
                    })?,
                    None => producer.product_digest.clone(),
                };
                if actual != dependency.digest {
                    return Err(invalid(format!(
                        "incoherent recorded dependency from {} to {}",
                        receipt.context.stage_id, dependency.producer
                    )));
                }
            }
        }
    }
    // Selection metadata is only a preflight. Every product is still admitted
    // exclusively by the native read-only cache and exact verified receipt match.
    let cache = PipelineCache::open_existing_default_read_only(cache_root)?;
    let mut inputs = BTreeMap::new();
    for (path, expected) in selections {
        let stage = &expected.context.stage_id;
        let started = Instant::now();
        let hit = cache.get(&expected.context)?.ok_or_else(|| {
            invalid(format!(
                "missing recorded reasoning input {stage}; recomputation is forbidden"
            ))
            .with_focus(path.display().to_string())
        })?;
        if hit.receipt != expected {
            return Err(
                invalid("selected stage receipt differs from the verified store receipt")
                    .with_focus(path.display().to_string()),
            );
        }
        emit(&json!({
            "phase": "hydrated", "stage": stage, "action_key": expected.action_key,
            "receipt_digest": expected.digest(), "product_digest": expected.product_digest,
            "product_blob_digest": expected.product_blob_digest,
            "product_bytes": expected.product_blob_bytes,
            "elapsed_ms": started.elapsed().as_millis(),
        }))?;
        inputs.insert(stage.clone(), hit.product);
    }
    Ok(inputs)
}

/// Run the selected observation without executing a producer or repairing inputs.
fn run() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 5 || arguments.iter().any(|argument| argument.is_empty()) {
        return Err(invalid(
            "usage: recorded-class-consistency REPO_WITH_CACHE SOURCE_RECEIPT STATEMENTS_RECEIPT COMPILE_RECEIPT WORLD_OR_list",
        ));
    }
    let cache_root = PathBuf::from(&arguments[0]);
    let receipts: Vec<_> = arguments[1..4].iter().map(PathBuf::from).collect();
    let operation = arguments[4]
        .to_str()
        .ok_or_else(|| invalid("world selection must be UTF-8"))?;
    let inputs = load_inputs(&cache_root, &receipts)?;
    let started = Instant::now();
    let edb = assemble_object_level_edb(&inputs)?;
    drop(inputs);
    emit(&json!({"phase": "assembled", "elapsed_ms": started.elapsed().as_millis()}))?;
    if operation == "list" {
        let mut worlds = BTreeMap::from([("default".to_owned(), 0_usize)]);
        for graph in edb.named_graphs() {
            let name = match edb.resolve(graph) {
                TermRef::Iri(iri) => iri.to_owned(),
                other => format!("{other:?}"),
            };
            worlds.insert(name, 0);
        }
        for quad in edb.quads() {
            let graph = quad.g.map_or_else(
                || "default".to_owned(),
                |graph| match edb.resolve(graph) {
                    TermRef::Iri(iri) => iri.to_owned(),
                    other => format!("{other:?}"),
                },
            );
            *worlds.entry(graph).or_default() += 1;
        }
        let unselectable: BTreeMap<_, _> = worlds
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(world, _)| (world, "no-base-assertions"))
            .collect();
        return emit(&json!({
            "operation": "list", "world_assertions": worlds,
            "unselectable_worlds": unselectable,
        }));
    }
    let started = Instant::now();
    let graph = (operation != "default").then(|| purrdf::RdfTerm::iri(operation));
    let dataset = Arc::new(
        edb.project_graph(graph.as_ref())
            .map_err(|error| foreign("world selection", error))?,
    );
    drop(edb);
    let selected_ms = started.elapsed().as_millis();
    if dataset.quads().next().is_none() {
        return Err(invalid(format!(
            "selected recorded world {operation} has no base assertions"
        )));
    }
    let assertions = dataset.quads().count();
    emit(&json!({"phase": "prepare", "world": operation, "assertions": assertions}))?;
    let started = Instant::now();
    let mut reasoner = DlReasoner::with_proofs(&dataset)?;
    reasoner.prepare(&Question::Consistency)?;
    let prepared_ms = started.elapsed().as_millis();
    emit(&json!({"phase": "decide", "world": operation, "prepared_ms": prepared_ms}))?;
    let started = Instant::now();
    let result = reasoner.consistency();
    let decided_ms = started.elapsed().as_millis();
    let context = reasoner.proof_context()?;
    let proof = result
        .proof()
        .ok_or_else(|| invalid("recorded native DL service omitted its proof"))?;
    let started = Instant::now();
    let replay = proof
        .verify(
            &dataset,
            &Question::Consistency,
            Some(result.certificate()),
            &context,
            &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
        )
        .map_err(|error| foreign("native proof verification", error))?;
    emit(&json!({
        "operation": "consistency", "semantics": "owl2-direct",
        "gmeow_canon_admission": "not-evaluated",
        "world": operation, "source_graph": (operation != "default").then_some(operation),
        "assertions": assertions,
        "selected_ms": selected_ms,
        "prepared_ms": prepared_ms, "decided_ms": decided_ms,
        "verified_ms": started.elapsed().as_millis(),
        "answer": format!("{:?}", result.answer()),
        "completeness": format!("{:?}", result.certificate().completeness()),
        "certificate_debug": format!("{:?}", result.certificate()),
        "proof_runs": replay.runs(), "replayed_runs": replay.replayed(),
    }))
}

/// Return a failing exit status for any input, native-service or output refusal.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}
