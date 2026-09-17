// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Selected dictionary observations. Native validated tables remain in the producer.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_lang_bridge::{
    Gmn0Model, Gmn1Document, Gmn1Error, gmn1_read, per_claim_round_trip_check,
    per_claim_standalone_check,
};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, RdfTriple};
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::{SourceCatalog, language::LANG};

mod cases;
pub(super) mod costs;
mod notation;
mod stamps;

pub(super) const CHANNEL: &str = "pipeline/gmn-dictionary-observations.json";
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

#[derive(Serialize, Deserialize)]
pub(super) struct Observations {
    pub aliases: BTreeMap<String, String>,
    pub nec_reverse: Option<String>,
    pub acceptance: (u32, u32),
    pub resolved_acceptance: (u32, u32),
    pub current_header: Result<(), Gmn1Error>,
    pub future_header: Result<(), Gmn1Error>,
    pub per_claim: Result<(), Gmn1Error>,
    pub standalone: Result<gmeow_lang_bridge::gmn1_witness::StandaloneReport, Gmn1Error>,
    pub glyphs: BTreeMap<String, GlyphSource>,
    pub records: cases::Records,
    pub costs: costs::Costs,
    pub stamps: stamps::Stamps,
    pub notation: Result<notation::Notation, gmeow_lang_bridge::IngestDiagnostic>,
}

/// Raw authored coordinates, independent of the dictionary's validated registry.
#[derive(Default, Serialize, Deserialize)]
pub(super) struct GlyphSource {
    pub codepoints: BTreeSet<String>,
    pub scopes: BTreeSet<String>,
}

fn glyph_sources(source: &RdfDataset) -> BTreeMap<String, GlyphSource> {
    let mut denotations = BTreeSet::new();
    let mut bindings: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut glyphs: BTreeMap<String, GlyphSource> = BTreeMap::new();
    for quad in source.owned_quads() {
        let RdfTerm::Iri(subject) = quad.subject else {
            continue;
        };
        match (quad.predicate.as_str(), quad.object) {
            ("http://www.w3.org/1999/02/22-rdf-syntax-ns#type", RdfTerm::Iri(class))
                if class == "https://blackcatinformatics.ca/lang/Denotation" =>
            {
                denotations.insert(subject);
            }
            ("https://blackcatinformatics.ca/gmeow/gmnDenotationGrapheme", RdfTerm::Iri(glyph)) => {
                bindings.entry(subject).or_default().insert(glyph);
            }
            ("https://blackcatinformatics.ca/gmeow/gmnCodepoints", RdfTerm::Literal(value)) => {
                glyphs
                    .entry(subject)
                    .or_default()
                    .codepoints
                    .insert(value.lexical_form);
            }
            ("https://blackcatinformatics.ca/gmeow/gmnSigilScope", RdfTerm::Iri(scope)) => {
                glyphs.entry(subject).or_default().scopes.insert(scope);
            }
            _ => {}
        }
    }
    let selected: BTreeSet<_> = bindings
        .into_iter()
        .filter(|(denotation, _)| denotations.contains(denotation))
        .flat_map(|(_, glyphs)| glyphs)
        .collect();
    selected
        .into_iter()
        .map(|iri| {
            let source = glyphs.remove(&iri).unwrap_or_default();
            (iri, source)
        })
        .collect()
}

pub(super) fn record(
    sources: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let language = sources.language()?;
    let dict = &language.dictionary;
    let acceptance = dict.acceptance();
    let latest = acceptance.latest_major();
    let future = latest
        .checked_add(acceptance.accept_window())
        .and_then(|major| major.checked_add(1))
        .ok_or_else(|| {
            super::stage_err("GMN future-version probe exceeds its major-version domain")
        })?;
    let header = |major| {
        Gmn1Document::from_text(format!(
            "@gmn{{v: {major}, aliases: dict-v{}, glyphs: {}}}",
            dict.version(),
            dict.glyph_registry().version(),
        ))
    };
    let iri = |local| RdfTerm::iri(format!("{GMEOW}{local}"));
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&RdfQuad::new(
        iri("reifier1"),
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies",
        RdfTerm::triple(RdfTriple::new(
            iri("gate1"),
            format!("{GMEOW}hasState"),
            iri("open"),
        )),
    ));
    builder.push_owned_quad(&RdfQuad::new(
        iri("reifier1"),
        format!("{GMEOW}hasState"),
        iri("open"),
    ));
    let dataset = builder
        .freeze()
        .map_err(|error| super::stage_err(&error.to_string()))?;
    let model = Gmn0Model::from_dataset(&dataset);
    let observed = Observations {
        aliases: dict.alias_entries().clone(),
        nec_reverse: dict.term_for("nec").map(str::to_owned),
        acceptance: (latest, acceptance.accept_window()),
        resolved_acceptance: (
            language.dialect.latest_major(),
            language.dialect.accept_window(),
        ),
        current_header: gmn1_read(&header(latest), dict).map(|_| ()),
        future_header: gmn1_read(&header(future), dict).map(|_| ()),
        per_claim: per_claim_round_trip_check(&model, dict),
        standalone: per_claim_standalone_check(&model, dict),
        glyphs: glyph_sources(sources.document(LANG)?),
        records: cases::record(dict)?,
        costs: costs::record(sources.document(LANG)?, dict)?,
        stamps: stamps::record(dict, language.dialect.latest_major_key()),
        notation: notation::record(dict),
    };
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observed).map_err(|error| super::stage_err(&error.to_string()))?,
    );
    Ok(())
}

#[path = "gmn_dictionary.tests.rs"]
#[cfg(test)]
mod tests;
