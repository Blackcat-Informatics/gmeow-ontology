// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Receipts for actual GMN projection products and explicitly scoped native controls.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_lang_bridge::TokenMetrics;
use gmeow_lang_bridge::gmn_metrics::MeasuredTokenCorpus;
use gmeow_lang_bridge::gmn1_digest::{EcosystemLeaves, codebook_digest, grammar_leaf, pack_root};
use gmeow_lang_bridge::registry::{LangEmission, LangEmissionBatch, LangProjectionInput};
use gmeow_logic_compile::ir::{Correspondence, LegPath};
use purrdf::slice::SliceCatalog;
use serde::{Deserialize, Serialize};

mod controls;

pub(crate) const CHANNEL: &str = "pipeline/gmn-pack-observations.json";
const PRODUCTS: [&str; 3] = [
    "conformance-pack.ttl",
    "token-metrics.ttl",
    "verbalizations.ttl",
];

#[derive(Serialize, Deserialize)]
pub(crate) struct Observations {
    pub major: String,
    pub paths: Vec<String>,
    pub emissions: Vec<EmissionWitness>,
    pub metrics: Metrics,
    pub grounding_metrics: Metrics,
    pub grounding_sources: Vec<String>,
    pub expected_codebook_digest: String,
    pub expected_pack_root: String,
    pub expected_grammar_leaf: String,
    pub ecosystem_leaves: Vec<(String, String, String)>,
    pub controls: controls::Controls,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct EmissionWitness {
    pub path: String,
    pub is_rdf: bool,
    pub correspondence: Correspondence,
    pub round_trip: bool,
    pub leg_pair: Option<(LegPath, LegPath)>,
    pub source_matches_artifact: bool,
}

impl EmissionWitness {
    fn from_emission(emission: &LangEmission, path: &str) -> gmeow_errors::Result<Self> {
        let artifact = emission
            .artifacts
            .iter()
            .find(|artifact| artifact.path_suffix == path)
            .ok_or_else(|| {
                super::stage_err(format!("GMN emission lacks selected artifact {path}"))
            })?;
        Ok(Self {
            path: path.to_owned(),
            is_rdf: artifact.is_rdf,
            correspondence: emission.correspondence.clone(),
            round_trip: emission.round_trip_holds,
            leg_pair: emission.leg_pair.clone(),
            source_matches_artifact: emission.source_rdf == artifact.bytes,
        })
    }
}

/// All seven rates and every integer witness, before RDF formatting rounds decimal values.
#[derive(Serialize, Deserialize)]
pub(crate) struct Metrics {
    pub bytes_on_disk: u64,
    pub tokens_in_context: u64,
    pub ast_validity_rate: f64,
    pub roundtrip_loss: f64,
    pub compression_ratio: f64,
    pub glyph_density: f64,
    pub dictionary_hit_rate: f64,
    pub gmn_worst_case_tokens: u64,
    pub gmn_realistic_tokens: u64,
    pub turtle_best_case_tokens: u64,
    pub gmn_ascii_bytes: u64,
    pub gmn_nonascii_bytes: u64,
    pub turtle_bytes_on_disk: u64,
    pub jsonld_bytes_on_disk: u64,
    pub total_sources: u64,
    pub measured_sources: u64,
    pub compression_gate_holds: bool,
}

impl From<TokenMetrics> for Metrics {
    fn from(metrics: TokenMetrics) -> Self {
        Self {
            bytes_on_disk: metrics.bytes_on_disk,
            tokens_in_context: metrics.tokens_in_context,
            ast_validity_rate: metrics.ast_validity_rate,
            roundtrip_loss: metrics.roundtrip_loss,
            compression_ratio: metrics.compression_ratio,
            glyph_density: metrics.glyph_density,
            dictionary_hit_rate: metrics.dictionary_hit_rate,
            gmn_worst_case_tokens: metrics.gmn_worst_case_tokens,
            gmn_realistic_tokens: metrics.gmn_realistic_tokens,
            turtle_best_case_tokens: metrics.turtle_best_case_tokens,
            gmn_ascii_bytes: metrics.gmn_ascii_bytes,
            gmn_nonascii_bytes: metrics.gmn_nonascii_bytes,
            turtle_bytes_on_disk: metrics.turtle_bytes_on_disk,
            jsonld_bytes_on_disk: metrics.jsonld_bytes_on_disk,
            total_sources: metrics.total_sources,
            measured_sources: metrics.measured_sources,
            compression_gate_holds: metrics.compression_gate_holds(),
        }
    }
}

#[derive(Default)]
pub(super) struct Recorder {
    selected: Option<Recorded>,
    grammar_views: BTreeMap<String, Option<String>>,
}

struct Recorded {
    measurements: Option<MeasuredTokenCorpus>,
    paths: Vec<String>,
    emissions: Vec<EmissionWitness>,
}

impl Recorder {
    pub(super) fn record(
        &mut self,
        target: &str,
        batch: &LangEmissionBatch,
    ) -> gmeow_errors::Result<()> {
        if matches!(target, "gbnf" | "lark") {
            let path = batch
                .emissions
                .iter()
                .flat_map(|emission| &emission.artifacts)
                .find(|artifact| {
                    artifact
                        .path_suffix
                        .ends_with(&format!("/{target}/gmn.{target}"))
                })
                .map(|artifact| artifact.path_suffix.clone());
            self.grammar_views.insert(target.to_owned(), path);
        }
        if target != "gmn1" {
            return Ok(());
        }
        if self.selected.is_some() {
            return Err(super::stage_err(
                "GMN pack observations received a repeated production batch",
            ));
        }
        let mut paths = Vec::new();
        let mut emissions = Vec::new();
        for emission in &batch.emissions {
            for artifact in &emission.artifacts {
                paths.push(artifact.path_suffix.clone());
                if PRODUCTS
                    .iter()
                    .any(|product| artifact.path_suffix.ends_with(product))
                {
                    emissions.push(EmissionWitness::from_emission(
                        emission,
                        &artifact.path_suffix,
                    )?);
                }
            }
        }
        self.selected = Some(Recorded {
            measurements: batch.gmn_metrics.clone(),
            paths,
            emissions,
        });
        Ok(())
    }

    pub(super) fn finish(
        self,
        input: &LangProjectionInput,
        catalog: Option<&SliceCatalog>,
        artifacts: &[(String, Vec<u8>)],
    ) -> gmeow_errors::Result<Option<Observations>> {
        let Some(dict) = input.gmn_dictionary.as_ref() else {
            return Ok(None);
        };
        let selected = self
            .selected
            .ok_or_else(|| super::stage_err("selected GMN target was not observed"))?;
        let measured = selected
            .measurements
            .ok_or_else(|| super::stage_err("selected GMN target omitted its native metrics"))?;
        let major = input
            .gmn_dialect_major
            .as_deref()
            .ok_or_else(|| super::stage_err("selected GMN target lacks dialect major"))?;
        let codebook = input
            .gmn_codebook
            .as_ref()
            .ok_or_else(|| super::stage_err("selected GMN pack lacks codebook"))?;
        let grammar = input
            .gmn_grammar_source
            .as_deref()
            .ok_or_else(|| super::stage_err("selected GMN pack lacks authored grammar"))?;
        let (grounding_indices, grounding_sources) = grounding_scope(input, catalog)?;
        let get = |suffix: &str| -> gmeow_errors::Result<&[u8]> {
            let path = format!("{}/{suffix}", super::LANG_PROJECTION_DIR);
            artifacts
                .iter()
                .find(|(key, _)| *key == path)
                .map(|(_, bytes)| bytes.as_slice())
                .ok_or_else(|| super::stage_err(format!("selected GMN product is absent: {path}")))
        };
        let grammar_view = |target: &str| -> gmeow_errors::Result<&[u8]> {
            match self.grammar_views.get(target) {
                Some(Some(path)) => get(path),
                // A target that actually emitted no surface contributes the lawful empty
                // leaf. A missing target or a missing emitted artifact still fails closed.
                Some(None) => Ok(&[]),
                None => Err(super::stage_err(format!(
                    "GMN pack lacks observed {target} target"
                ))),
            }
        };
        // These are the actual external target products, not a second grammar emission.
        let ecosystem = EcosystemLeaves::from_view_bytes(
            grammar_view("gbnf")?,
            grammar_view("lark")?,
            get(&format!("gmn1/v{major}/token-metrics.ttl"))?,
            get(&format!("gmn1/v{major}/verbalizations.ttl"))?,
        );
        let expected_codebook_digest = codebook_digest(codebook, dict);
        let observations = Observations {
            major: major.to_owned(),
            paths: selected.paths,
            emissions: selected.emissions,
            metrics: measured.aggregate().into(),
            grounding_metrics: measured
                .selected(|index| grounding_indices.contains(&index))
                .into(),
            grounding_sources,
            expected_pack_root: pack_root(&expected_codebook_digest, dict, grammar, &ecosystem),
            expected_codebook_digest,
            expected_grammar_leaf: grammar_leaf(grammar),
            ecosystem_leaves: vec![
                (
                    "gmnGbnf".to_owned(),
                    "gmnGbnfDigest".to_owned(),
                    ecosystem.gbnf,
                ),
                (
                    "gmnLark".to_owned(),
                    "gmnLarkDigest".to_owned(),
                    ecosystem.lark,
                ),
                (
                    "gmnTokenMetricsCurrent".to_owned(),
                    "gmnTokenMetricsDigest".to_owned(),
                    ecosystem.token_metrics,
                ),
                (
                    "gmnVerbalizationsCurrent".to_owned(),
                    "gmnVerbalizationsDigest".to_owned(),
                    ecosystem.verbalizations,
                ),
            ],
            controls: controls::record(input)?,
        };
        Ok(Some(observations))
    }
}

fn grounding_scope(
    input: &LangProjectionInput,
    catalog: Option<&SliceCatalog>,
) -> gmeow_errors::Result<(BTreeSet<usize>, Vec<String>)> {
    let catalog =
        catalog.ok_or_else(|| super::stage_err("selected GMN pack lacks its source catalog"))?;
    let mut indices = BTreeSet::new();
    let mut sources = Vec::new();
    for record in catalog.records() {
        let Some(slice) = ["lang", "math", "logic"]
            .into_iter()
            .find(|slice| record.slice_dir.ends_with(format!("grounding/{slice}")))
        else {
            continue;
        };
        for artifact in &record.artifacts {
            if !super::is_lang_model(&artifact.logical_path, &artifact.content) {
                continue;
            }
            let name = super::lang_model_stem(&artifact.logical_path);
            let matches: Vec<_> = input
                .lang_models
                .iter()
                .enumerate()
                .filter(|(_, source)| source.name == name && source.bytes == artifact.content)
                .map(|(index, _)| index)
                .collect();
            let [index] = matches.as_slice() else {
                return Err(super::stage_err(format!(
                    "grounding source {slice}/{} lacks a unique production measurement",
                    artifact.logical_path
                )));
            };
            indices.insert(*index);
            sources.push(format!(
                "slices/grounding/{slice}/{}",
                artifact.logical_path
            ));
        }
    }
    sources.sort();
    Ok((indices, sources))
}
