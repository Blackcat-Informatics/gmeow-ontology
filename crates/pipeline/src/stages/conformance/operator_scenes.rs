// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Isolated authored operator scenes, evaluated only by the explicit producer.
//! Grounding examples reuse their existing parse and action. Other examples have
//! bounded source-local actions; no scene joins the global ontology implicitly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_logic::operator::{
    derive,
    refinement::RefinementRecord,
    scene::{ExampleSelection, LabelMutation, Observations, OcrObservation, SourceObservation},
};
use gmeow_logic::operator_rules::{OPERATOR_SOURCE_PATH, PreparedOperatorRules};
use purrdf::{
    DatasetMut, DatasetView, GraphMatch, MutableDataset, QuadValues, RdfDataset, TermRef,
    TermValue, parse_dataset,
};

pub(super) use gmeow_logic::operator::scene::CHANNEL;
const COUNTER: &str = "slices/grounding/logic/tests/counter-examples/unknown-outcome-retried-on-a-borrowed-licence.ttl";
const OCR_ABSENT: &str = "slices/core/work-orchestration/examples/ocr-capability-absent.ttl";
const OCR_PRESENT: &str = "slices/core/work-orchestration/examples/ocr-capability-present.ttl";
const REQUIRED: &[&str] = &[
    COUNTER,
    OCR_ABSENT,
    OCR_PRESENT,
    "slices/core/work-orchestration/examples/contextual-recommendation.ttl",
    "slices/core/work-orchestration/examples/effect-boundary-unknown.ttl",
];
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const OCR_ENTRY: &str =
    "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-absent/ocrStepEntry";
const OCR_ACTION: &str =
    "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-absent/ocrStep";
const FRAGMENT: &str = "https://blackcatinformatics.ca/logic/FragmentAcyclicMethod";

fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}

/// Preserve the existing sweep's literal source applicability, including a marker
/// used only in commentary or a predicate census. Such a scene must still retain
/// and check the operator's explicit no-label refusal.
pub(super) fn has_marker(bytes: &[u8]) -> bool {
    bytes
        .windows(b"logic:entryLabel".len())
        .any(|part| part == b"logic:entryLabel")
}

fn collect_examples(
    dir: &Path,
    in_examples: bool,
    files: &mut Vec<PathBuf>,
) -> gmeow_errors::Result<()> {
    for entry in std::fs::read_dir(dir).map_err(fail)? {
        let entry = entry.map_err(fail)?;
        let kind = entry.file_type().map_err(fail)?;
        let path = entry.path();
        if kind.is_dir() {
            collect_examples(&path, in_examples || entry.file_name() == "examples", files)?;
        } else if kind.is_file() {
            if in_examples && path.extension().is_some_and(|extension| extension == "ttl") {
                files.push(path);
            }
        } else {
            return Err(fail(format!(
                "unsupported operator source {}",
                path.display()
            )));
        }
    }
    Ok(())
}

pub(super) fn input_files(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_examples(&root.join("slices"), false, &mut files)?;
    if files.is_empty() {
        return Err(fail("operator sweep selected no authored examples"));
    }
    files.extend([root.join(COUNTER), root.join(OPERATOR_SOURCE_PATH)]);
    files.sort();
    files.dedup();
    Ok(files)
}

fn raw(path: &str, digest: String) -> ActionInput {
    ActionInput::Raw {
        logical_path: path.to_owned(),
        file_kind: FileKind::File,
        executable: false,
        digest,
    }
}

pub(super) fn record(
    root: &Path,
    rules: &PreparedOperatorRules,
    mut grounding: BTreeMap<String, SourceObservation>,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let mut examples = BTreeMap::new();
    let mut scenes = BTreeMap::new();
    for path in input_files(root)? {
        let relative = path
            .strip_prefix(root)
            .map_err(fail)?
            .to_str()
            .ok_or_else(|| fail("non-UTF-8 operator source path"))?;
        if relative == OPERATOR_SOURCE_PATH {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(fail)?;
        let digest = bytes_digest(&bytes);
        let marker = has_marker(&bytes);
        if relative != COUNTER {
            examples.insert(
                relative.to_owned(),
                ExampleSelection {
                    source_digest: digest.clone(),
                    contains_entry_label_marker: marker,
                },
            );
        }
        if !marker && !REQUIRED.contains(&relative) {
            continue;
        }
        let observed = if relative.starts_with("slices/grounding/")
            && relative.contains("/examples/")
        {
            let observed = grounding.remove(relative).ok_or_else(|| {
                fail(format!(
                    "missing shared original grounding operator observation for {relative}"
                ))
            })?;
            if observed.source_digest != digest {
                return Err(fail(format!(
                    "operator observation source identity drift for {relative}"
                )));
            }
            observed
        } else {
            super::execution::cached_inputs(
                &store,
                "native-operator-scene-v1:isolated-default-graph:label-audit:ocr-label-overlay",
                vec![
                    raw(relative, digest.clone()),
                    raw(OPERATOR_SOURCE_PATH, rules.source_digest().to_owned()),
                ],
                || {
                    let dataset = parse_dataset(&bytes, "text/turtle", None).map_err(|error| {
                        gmeow_errors::Diag::from(error).with_context(format!("parse {relative}"))
                    })?;
                    observe(relative, &digest, &dataset, rules)
                },
            )?
        };
        scenes.insert(relative.to_owned(), observed);
    }
    if !grounding.is_empty() {
        return Err(fail(
            "operator grounding observations exceed the declared example selection",
        ));
    }
    for &required in REQUIRED {
        if !scenes.contains_key(required) {
            return Err(fail(format!(
                "required operator source missing: {required}"
            )));
        }
    }
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&Observations { examples, scenes }).map_err(fail)?,
    );
    Ok(())
}

/// Called with the same original native dataset used by the grounding action.
/// The caller binds the exact source digest and prepared operator input in its key.
pub(super) fn observe(
    path: &str,
    digest: &str,
    dataset: &Arc<RdfDataset>,
    rules: &PreparedOperatorRules,
) -> Result<SourceObservation, gmeow_errors::Diag> {
    let ocr = if path == OCR_ABSENT {
        Some(observe_ocr(dataset, rules)?)
    } else {
        None
    };
    let task = match path {
        OCR_ABSENT => Some(OCR_ACTION),
        OCR_PRESENT => Some(
            "https://blackcatinformatics.ca/gmeow/examples/work-orchestration/ocr-present/ocrStep",
        ),
        _ => None,
    };
    let refinement = task.map(|task| {
        RefinementRecord::from(&gmeow_logic::refine(dataset, task, FRAGMENT, 1000, rules))
    });
    let attempt = dataset.term_id_by_value(&TermValue::iri(format!("{LOGIC}attemptOfIntent")));
    let has_attempt_of_intent = attempt.is_some_and(|predicate| {
        dataset
            .quads_for_pattern(None, Some(predicate), None, GraphMatch::Any)
            .next()
            .is_some()
    });
    Ok(SourceObservation {
        source_path: path.to_owned(),
        source_digest: digest.to_owned(),
        has_attempt_of_intent,
        original: derive(dataset, rules).map_err(super::record_failure),
        ocr,
        refinement,
    })
}

fn objects(dataset: &RdfDataset, subject: &str, predicate: &str) -> Vec<String> {
    let (Some(subject), Some(predicate)) = (
        dataset.term_id_by_value(&TermValue::iri(subject)),
        dataset.term_id_by_value(&TermValue::iri(predicate)),
    ) else {
        return Vec::new();
    };
    dataset
        .quads_for_pattern(Some(subject), Some(predicate), None, GraphMatch::Default)
        .filter_map(|quad| match dataset.resolve(quad.o) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect()
}

fn observe_ocr(
    dataset: &Arc<RdfDataset>,
    rules: &PreparedOperatorRules,
) -> Result<OcrObservation, gmeow_errors::Diag> {
    let predicate = format!("{LOGIC}entryLabel");
    let removed = format!("{LOGIC}FrontierBlockedCapabilityOrResource");
    let inserted = format!("{LOGIC}FrontierReadyAuthorized");
    let asserted_labels = objects(dataset, OCR_ENTRY, &predicate);
    let entry_actions = objects(dataset, OCR_ENTRY, &format!("{LOGIC}entryAction"));
    // Native overlay preserves every original term/scope and changes precisely the
    // contradictory assertion. It never serializes or reparses the authored scene.
    let mut edited = MutableDataset::new(dataset.clone());
    let row = |object: &str| QuadValues {
        s: TermValue::iri(OCR_ENTRY),
        p: TermValue::iri(predicate.clone()),
        o: TermValue::iri(object),
        g: None,
    };
    let removed_rows = usize::from(edited.remove(&row(&removed)));
    let inserted_rows = usize::from(
        edited
            .insert(row(&inserted))
            .map_err(gmeow_errors::Diag::from)?,
    );
    let changed = edited.freeze().map_err(gmeow_errors::Diag::from)?;
    Ok(OcrObservation {
        entry: OCR_ENTRY.to_owned(),
        action: OCR_ACTION.to_owned(),
        asserted_labels,
        entry_actions,
        mutation: LabelMutation {
            predicate,
            removed,
            inserted,
            removed_rows,
            inserted_rows,
            result: derive(&changed, rules).map_err(super::record_failure),
        },
    })
}
