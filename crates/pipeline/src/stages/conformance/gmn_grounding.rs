// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-local GMN judgments produced once for reconciliation and corpus readers.
//! These observations describe the selected sources; they are not rewrite certificates.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_lang_bridge::{
    ConstructCoverageTally, Gmn0Model, Gmn1ConstructCategory, Gmn1Error, GmnDictionary,
    QuadCoverage, classify_model, round_trip_with_claims_check,
};
use purrdf::{RdfDataset, parse_dataset};
use serde::{Deserialize, Serialize};

use crate::stages::gmn1_gate::{
    Gmn1ConstructCoverageReport, Gmn1RoundTripFailure, Gmn1RoundTripReport,
    collect_grounding_sources,
};
use crate::stages::parse_sources::SourceCatalog;

pub(crate) const CHANNEL: &str = "pipeline/gmn-grounding-observations.json";
const LANG: &str = "slices/grounding/lang/module.ttl";

#[derive(Serialize, Deserialize)]
pub(crate) struct Observations {
    pub sources: BTreeMap<String, SourceObservation>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SourceObservation {
    pub quads: usize,
    pub roundtrip: Result<(), Gmn1Error>,
    pub coverage: ConstructCoverageTally,
    pub without_decimal: ConstructCoverageTally,
    pub retained_quads: usize,
    pub(super) migration:
        Option<Result<super::gmn_migration::Observation, gmeow_lang_bridge::GmnMigrateError>>,
    pub(super) consume: Option<super::gmn_consume::Observation>,
    pub(super) native_scene: Option<Result<super::native_scene::Scene, gmeow_errors::RecordedDiag>>,
    pub(super) gufo_superset: Option<super::gufo_superset::SourceObservation>,
    pub(super) math_lowering: Option<super::math_lowering::SourceObservation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    operator_scene: Option<gmeow_logic::operator::scene::SourceObservation>,
}

pub(crate) struct Reports {
    pub roundtrip: Gmn1RoundTripReport,
    pub coverage: Gmn1ConstructCoverageReport,
}

impl Observations {
    pub(crate) fn reports(&self) -> Reports {
        let failures = self
            .sources
            .iter()
            .filter_map(|(path, source)| {
                source
                    .roundtrip
                    .as_ref()
                    .err()
                    .map(|error| Gmn1RoundTripFailure {
                        path: path.clone(),
                        error: error.clone(),
                    })
            })
            .collect();
        let mut tally = ConstructCoverageTally::default();
        for source in self.sources.values() {
            tally.merge(&source.coverage);
        }
        Reports {
            roundtrip: Gmn1RoundTripReport { failures },
            coverage: Gmn1ConstructCoverageReport {
                unexercised: tally.unexercised_categories(),
                uncovered_quad_count: tally.uncovered.len(),
            },
        }
    }
}

pub(super) fn input_files(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let sources = collect_grounding_sources(root)?;
    for slice in ["logic", "lang", "math"] {
        let module = format!("slices/grounding/{slice}/module.ttl");
        let examples = format!("slices/grounding/{slice}/examples/");
        if !sources.contains(&module) || !sources.iter().any(|path| path.starts_with(&examples)) {
            return Err(super::stage_err(&format!(
                "required GMN grounding source domain is incomplete for {slice}"
            )));
        }
    }
    Ok(sources.into_iter().map(|path| root.join(path)).collect())
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
    catalog: &SourceCatalog,
    operators: &gmeow_logic::operator_rules::PreparedOperatorRules,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<BTreeMap<String, gmeow_logic::operator::scene::SourceObservation>> {
    let fail = |error: String| super::stage_err(&error);
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| fail(error.to_string()))?;
    let dictionary_input = raw(LANG, catalog.document_digest(LANG)?.to_owned());
    let dictionary = || {
        catalog
            .language()
            .map(|language| language.dictionary.as_ref())
    };
    let mut sources = BTreeMap::new();
    let mut operator_scenes = BTreeMap::new();
    for path in input_files(root)? {
        let relative = path
            .strip_prefix(root)
            .map_err(|error| fail(error.to_string()))?
            .to_str()
            .ok_or_else(|| fail("non-UTF-8 GMN source path".into()))?;
        // Modules are already in the admitted native source catalog. Examples are
        // distinct documents outside that assertion catalog and are parsed only on
        // an observation miss, never merged into its logical authoring scope.
        let mut observation = if relative.ends_with("/module.ttl") {
            super::execution::cached_inputs(
                &store,
                "gmn-grounding-v5-datatype-inventory",
                vec![
                    dictionary_input.clone(),
                    raw(relative, catalog.document_digest(relative)?.to_owned()),
                ],
                || {
                    let dataset = catalog.document(relative)?;
                    let mut observed = observe(dataset, dictionary()?);
                    observed.native_scene = super::native_scene::observe(relative, dataset);
                    observed.gufo_superset = super::gufo_superset::observe(relative, dataset);
                    observed.math_lowering = super::math_lowering::observe(relative, dataset);
                    Ok(observed)
                },
            )?
        } else {
            let bytes = std::fs::read(&path).map_err(|error| fail(error.to_string()))?;
            let digest = bytes_digest(&bytes);
            let operator_selected =
                relative.contains("/examples/") && super::operator_scenes::has_marker(&bytes);
            let mut inputs = vec![dictionary_input.clone(), raw(relative, digest.clone())];
            if operator_selected {
                inputs.push(raw(
                    gmeow_logic::operator_rules::OPERATOR_SOURCE_PATH,
                    operators.source_digest().to_owned(),
                ));
            }
            super::execution::cached_inputs(
                &store,
                "gmn-grounding-v5-datatype-inventory",
                inputs,
                || {
                    let dataset = parse_dataset(&bytes, "text/turtle", None).map_err(|error| {
                        gmeow_errors::Diag::from(error).with_context(format!("parse {relative}"))
                    })?;
                    let dictionary = dictionary()?;
                    let model = Gmn0Model::from_dataset(&dataset);
                    let mut observed = observe_model(&model, dictionary);
                    observed.native_scene = super::native_scene::observe(relative, &dataset);
                    observed.gufo_superset = super::gufo_superset::observe(relative, &dataset);
                    observed.math_lowering = super::math_lowering::observe(relative, &dataset);
                    if operator_selected {
                        observed.operator_scene = Some(super::operator_scenes::observe(
                            relative, &digest, &dataset, operators,
                        )?);
                    }
                    if relative == super::gmn_migration::SOURCE {
                        observed.migration = Some(super::gmn_migration::observe(
                            &dataset,
                            catalog.document(LANG)?,
                            dictionary,
                        ));
                    }
                    if relative == super::gmn_consume::SOURCE {
                        observed.consume = Some(super::gmn_consume::observe(&model, catalog)?);
                    }
                    Ok(observed)
                },
            )?
        };
        if let Some(scene) = observation.operator_scene.take() {
            operator_scenes.insert(relative.to_owned(), scene);
        }
        sources.insert(relative.to_owned(), observation);
    }
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&Observations { sources }).map_err(|error| fail(error.to_string()))?,
    );
    Ok(operator_scenes)
}

fn observe(dataset: &RdfDataset, dictionary: &GmnDictionary) -> SourceObservation {
    let model = Gmn0Model::from_dataset(dataset);
    observe_model(&model, dictionary)
}

fn observe_model(model: &Gmn0Model, dictionary: &GmnDictionary) -> SourceObservation {
    let roundtrip = round_trip_with_claims_check(model, dictionary);
    let mut coverage = ConstructCoverageTally::default();
    let classifications = classify_model(model, dictionary);
    let decimal = |classified: &QuadCoverage| {
        matches!(classified,
        QuadCoverage::Covered { subject, predicate, object }
            if [*subject, *predicate, *object].contains(&Gmn1ConstructCategory::LiteralDecimal))
    };
    // If removal selects nothing, the original classifications already describe
    // the exact resulting model. Materialize and reclassify only changed sources:
    // removing a quad can change a remaining quad's record context.
    let filtered = classifications.iter().any(decimal).then(|| Gmn0Model {
        quads: model
            .quads
            .iter()
            .zip(&classifications)
            .filter(|(_, classified)| !decimal(classified))
            .map(|(quad, _)| quad.clone())
            .collect(),
    });
    coverage.absorb_classifications(classifications);
    let (without_decimal, retained_quads) = match filtered {
        Some(filtered) => {
            let mut tally = ConstructCoverageTally::default();
            tally.absorb(&filtered, dictionary);
            (tally, filtered.quads.len())
        }
        None => (coverage.clone(), model.quads.len()),
    };
    SourceObservation {
        quads: model.quads.len(),
        roundtrip,
        coverage,
        without_decimal,
        retained_quads,
        migration: None,
        consume: None,
        native_scene: None,
        gufo_superset: None,
        math_lowering: None,
        operator_scene: None,
    }
}

pub(crate) fn from_bytes(bytes: &[u8]) -> gmeow_errors::Result<Observations> {
    serde_json::from_slice(bytes).map_err(|error| super::stage_err(&error.to_string()))
}

#[path = "gmn_grounding.tests.rs"]
#[cfg(test)]
mod tests;
