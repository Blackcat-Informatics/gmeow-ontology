// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned diagnostic observations over shared authored rule compilations.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_errors::grade::{FindingCategory, Severity, Standpoint};
use gmeow_errors::{Finding, Report};
use gmeow_logic_compile::frontend::default_source_statements;
use gmeow_ns::GMEOW_NS;
use purrdf::{
    NativeRdfFormat, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermRef, parse_dataset,
};
use serde::{Deserialize, Serialize};

use crate::stages::gate_verdict::GateProgram;
use crate::stages::meta_findings::{MetaDerivation, MetaProgram};
use crate::stages::parse_sources::SourceCatalog;

mod inputs;

pub(super) const CHANNEL: &str = "pipeline/diagnostic-observations.json";
const LOGIC: &str = crate::stages::compile_logic::SOURCE_PATH;
const DIAGNOSTICS: &str = "slices/core/diagnostics/module.ttl";
const FIXTURES: &str = "slices/core/diagnostics/tests";
const WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics-gate-conformance";
const META_CASES: &[&str] = &[
    "conformance-fixtures/finding-root-cause-present.ttl",
    "counter-examples/finding-root-cause-absent.ttl",
    "conformance-fixtures/cross-node-glut-present.ttl",
    "counter-examples/cross-node-glut-same-polarity.ttl",
    "counter-examples/cross-node-glut-different-anchor.ttl",
    "counter-examples/cross-node-glut-trivial-anchor.ttl",
];

#[derive(Serialize, Deserialize)]
pub(crate) struct Observations {
    pub meta_sources: BTreeSet<String>,
    pub meta: BTreeMap<String, Result<MetaDerivation, gmeow_errors::RecordedDiag>>,
    pub gate: Result<GateObservation, gmeow_errors::RecordedDiag>,
    pub reports: BTreeMap<String, Result<MetaReportObservation, gmeow_errors::RecordedDiag>>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct GateObservation {
    pub coordinates: BTreeMap<String, (String, String, String)>,
    pub category_blocking: BTreeMap<String, String>,
    pub fatal: BTreeSet<String>,
    pub projections: BTreeMap<String, ProjectionObservation>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ProjectionObservation {
    pub nquads: String,
    pub fatal: BTreeSet<String>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MetaReportObservation {
    pub nquads: String,
    pub derivation: MetaDerivation,
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    META_CASES
        .iter()
        .map(|path| root.join(FIXTURES).join(path))
        .chain([root.join(LOGIC), root.join(DIAGNOSTICS)])
        .collect()
}

fn raw(path: &str, digest: String) -> ActionInput {
    ActionInput::Raw {
        logical_path: path.to_owned(),
        file_kind: FileKind::File,
        executable: false,
        digest,
    }
}

fn source_iris(dataset: &RdfDataset) -> BTreeSet<String> {
    let (Some(predicate), Some(class)) = (
        dataset.term_id_by_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
        dataset.term_id_by_iri("https://blackcatinformatics.ca/gmeow/DiagnosticMetaRule"),
    ) else {
        return BTreeSet::new();
    };
    default_source_statements(dataset, None, Some(predicate), Some(class))
        .filter_map(|quad| match dataset.resolve(quad.s) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect()
}

struct Programs {
    meta: MetaProgram,
    gate: GateProgram,
}

impl Programs {
    fn load(catalog: &SourceCatalog) -> Result<Self, gmeow_errors::Diag> {
        let theory = catalog.compiled_document(
            LOGIC,
            Some(crate::stages::compile_logic::SOURCE_IRI.to_owned()),
        )?;
        let wiring = catalog.document(DIAGNOSTICS)?;
        Ok(Self {
            meta: MetaProgram::from_compiled_theory_with_wiring(&theory, wiring)?
                .ok_or_else(|| super::stage_err("required diagnostic meta-rules are absent"))?,
            gate: GateProgram::from_compiled_theory_with_wiring(&theory, wiring)?
                .ok_or_else(|| super::stage_err("required diagnostic gate rule is absent"))?,
        })
    }
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let fail = |error: String| super::stage_err(&error);
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| fail(error.to_string()))?;
    let shared = [LOGIC, DIAGNOSTICS]
        .into_iter()
        .map(|path| Ok(raw(path, catalog.document_digest(path)?.to_owned())))
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    // Even rule selection and query preparation happen only if some observation misses.
    let programs = OnceLock::new();
    let programs = || programs.get_or_try_init(|| Programs::load(catalog));
    let mut meta = BTreeMap::new();
    for case in META_CASES {
        let path = format!("{FIXTURES}/{case}");
        let bytes = std::fs::read(root.join(&path)).map_err(|error| fail(error.to_string()))?;
        let mut selected = shared.clone();
        selected.push(raw(&path, bytes_digest(&bytes)));
        let observed =
            super::execution::cached_inputs(&store, "diagnostic-meta-fixture", selected, || {
                let dataset = parse_dataset(&bytes, NativeRdfFormat::Turtle.media_type(), None)
                    .map_err(gmeow_errors::Diag::from)?;
                programs()?.meta.derive_dataset(&dataset)
            })
            .map_err(super::record_failure);
        meta.insert((*case).to_owned(), observed);
    }
    let gate =
        super::execution::cached_inputs(&store, "diagnostic-gate-matrix", shared.clone(), || {
            gate_observation(&programs()?.gate)
        })
        .map_err(super::record_failure);
    let mut reports = BTreeMap::new();
    for (key, report) in [
        ("root", inputs::root_report as fn() -> Report),
        ("glut", inputs::glut_report),
    ] {
        let observation = super::execution::cached_inputs(
            &store,
            &format!("diagnostic-meta-report-{key}"),
            shared.clone(),
            || {
                let nquads = gmeow_errors::render::to_gmeow_rdf(&report());
                let dataset = parse_dataset(
                    nquads.as_bytes(),
                    NativeRdfFormat::NQuads.media_type(),
                    None,
                )
                .map_err(gmeow_errors::Diag::from)?;
                let derivation = programs()?.meta.derive_dataset(&dataset)?;
                Ok(MetaReportObservation { nquads, derivation })
            },
        )
        .map_err(super::record_failure);
        reports.insert(key.to_owned(), observation);
    }
    let observation = Observations {
        meta_sources: source_iris(catalog.document(LOGIC)?),
        meta,
        gate,
        reports,
    };
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observation).map_err(|error| fail(error.to_string()))?,
    );
    Ok(())
}

fn gate_observation(gate: &GateProgram) -> Result<GateObservation, gmeow_errors::Diag> {
    let mut builder = RdfDatasetBuilder::new();
    let mut coordinates = BTreeMap::new();
    for (grade, iri) in inputs::all_grades() {
        coordinates.insert(
            iri.clone(),
            (
                inputs::severity_iri(grade.severity),
                inputs::category_iri(grade.category),
                inputs::standpoint_iri(grade.standpoint),
            ),
        );
        for (predicate, object) in [
            ("findingSeverity", inputs::severity_iri(grade.severity)),
            ("findingCategory", inputs::category_iri(grade.category)),
            (
                "findingStandpoint",
                inputs::standpoint_iri(grade.standpoint),
            ),
        ] {
            builder.push_owned_quad(
                &RdfQuad::new(
                    RdfTerm::iri(&iri),
                    format!("{GMEOW_NS}{predicate}"),
                    RdfTerm::iri(object),
                )
                .in_graph(RdfTerm::iri(WORLD)),
            );
        }
    }
    let dataset = builder.freeze().map_err(gmeow_errors::Diag::from)?;
    let fatal = gate.derived_fatal(&dataset)?;
    let mut projections = BTreeMap::new();
    for (key, code, message, standpoint) in [
        ("fatal", "x.upset", "up-set finding", Standpoint::Binding),
        (
            "advisory",
            "x.adv",
            "advisory never gates",
            Standpoint::Advisory,
        ),
    ] {
        let mut report = Report::new("validate");
        report.add_finding(
            Finding::new(Severity::Error, code, message)
                .with_category(FindingCategory::DataShapeViolation)
                .with_standpoint(standpoint),
        );
        let nquads = gmeow_errors::render::to_gmeow_rdf(&report);
        let dataset = parse_dataset(
            nquads.as_bytes(),
            NativeRdfFormat::NQuads.media_type(),
            None,
        )
        .map_err(gmeow_errors::Diag::from)?;
        let fatal = gate.derived_fatal(&dataset)?;
        projections.insert(key.to_owned(), ProjectionObservation { nquads, fatal });
    }
    Ok(GateObservation {
        coordinates,
        category_blocking: gate.category_blocking().clone(),
        fatal,
        projections,
    })
}
