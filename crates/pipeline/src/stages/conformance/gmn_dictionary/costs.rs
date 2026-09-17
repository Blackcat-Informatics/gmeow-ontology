// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Raw authored cost coordinates and native writer observations over the shared dictionary.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_lang_bridge::{Gmn0Model, Gmn1Error, GmnDictionary, gmn_glyph_token_cost, gmn1_write};
use purrdf::{DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, TermRef, TermValue};
use serde::{Deserialize, Serialize};

const COST: &str = "https://blackcatinformatics.ca/gmeow/gmnGlyphTokenCost";
const CODEPOINTS: &str = "https://blackcatinformatics.ca/gmeow/gmnCodepoints";
const VALUE: &str = "https://blackcatinformatics.ca/math/quantityValue";
const ENTRY_TERM: &str = "https://blackcatinformatics.ca/gmeow/gmnDictionaryEntryTerm";
const ENTRY_ALIAS: &str = "https://blackcatinformatics.ca/gmeow/gmnDictionaryEntryAlias";
const DENOTATION: &str = "https://blackcatinformatics.ca/gmeow/gmnFormDenotation";

#[derive(Debug, Serialize, Deserialize)]
pub(in crate::stages::conformance) struct Costs {
    pub rows: Vec<CostRow>,
    pub spellings: BTreeSet<String>,
    pub denotation_form_present: bool,
    pub aliases: BTreeMap<String, BTreeSet<String>>,
    pub emitted: BTreeMap<String, Result<String, Gmn1Error>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(in crate::stages::conformance) struct CostRow {
    pub codepoints: Vec<String>,
    pub values: Vec<String>,
    pub measured: Result<usize, gmeow_errors::RecordedDiag>,
}

fn literals(dataset: &RdfDataset, subject: purrdf::TermId, predicate: &str) -> Vec<String> {
    let Some(predicate) = dataset.term_id_by_value(&TermValue::iri(predicate)) else {
        return Vec::new();
    };
    let mut values: Vec<_> = dataset
        .quads_for_pattern(Some(subject), Some(predicate), None, GraphMatch::Any)
        .filter_map(|quad| match dataset.resolve(quad.o) {
            TermRef::Literal { lexical, .. } => Some(lexical.to_owned()),
            _ => None,
        })
        .collect();
    values.sort();
    values.dedup();
    values
}

pub(super) fn record(
    dataset: &RdfDataset,
    dictionary: &GmnDictionary,
) -> gmeow_errors::Result<Costs> {
    let mut rows = Vec::new();
    if let Some(predicate) = dataset.term_id_by_value(&TermValue::iri(COST)) {
        for quad in dataset.quads_for_pattern(None, Some(predicate), None, GraphMatch::Any) {
            let codepoints = literals(dataset, quad.s, CODEPOINTS);
            let measured = match codepoints.as_slice() {
                [spelling] => gmeow_lang_bridge::gmn1_codec::decode_codepoint_sequence(spelling)
                    .map(|glyph| gmn_glyph_token_cost(&glyph))
                    .map_err(|error| {
                        super::super::record_failure(super::super::stage_err(&error.0))
                    }),
                _ => Err(super::super::record_failure(super::super::stage_err(
                    "a glyph cost needs exactly one codepoint spelling",
                ))),
            };
            rows.push(CostRow {
                codepoints,
                values: literals(dataset, quad.o, VALUE),
                measured,
            });
        }
    }
    rows.sort_by(|left, right| {
        (&left.codepoints, &left.values).cmp(&(&right.codepoints, &right.values))
    });
    let mut spellings = BTreeSet::new();
    if let Some(predicate) = dataset.term_id_by_value(&TermValue::iri(CODEPOINTS)) {
        for quad in dataset.quads_for_pattern(None, Some(predicate), None, GraphMatch::Any) {
            if let TermRef::Literal { lexical, .. } = dataset.resolve(quad.o) {
                spellings.insert(lexical.to_owned());
            }
        }
    }
    let mut aliases: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if let Some(predicate) = dataset.term_id_by_value(&TermValue::iri(ENTRY_TERM)) {
        for quad in dataset.quads_for_pattern(None, Some(predicate), None, GraphMatch::Any) {
            if let TermRef::Iri(iri) = dataset.resolve(quad.o) {
                aliases.entry(iri.to_owned()).or_default().extend(literals(
                    dataset,
                    quad.s,
                    ENTRY_ALIAS,
                ));
            }
        }
    }
    let mut emitted = BTreeMap::new();
    for term in dictionary.alias_entries().keys() {
        let mut builder = RdfDatasetBuilder::new();
        let subject = builder.intern_iri("https://blackcatinformatics.ca/gmeow/probeSubject");
        let predicate = builder.intern_iri("https://blackcatinformatics.ca/gmeow/probePredicate");
        let object = builder.intern_iri(term);
        builder.push_quad(subject, predicate, object, None);
        let dataset = builder
            .freeze()
            .map_err(|error| super::super::stage_err(&error.to_string()))?;
        let model = Gmn0Model::from_dataset(&dataset);
        emitted.insert(
            term.clone(),
            gmn1_write(&model, dictionary).map(|document| document.text),
        );
    }
    Ok(Costs {
        rows,
        spellings,
        denotation_form_present: dataset
            .term_id_by_value(&TermValue::iri(DENOTATION))
            .is_some(),
        aliases,
        emitted,
    })
}
