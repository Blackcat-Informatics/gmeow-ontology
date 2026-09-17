// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer observations of the shipped primer and independently authored emission tasks.

use std::collections::BTreeSet;

use gmeow_lang_bridge::{Gmn1Document, GmnDictionary, gmn1_read, resolve_operator_forms};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue};
use serde::{Deserialize, Serialize};

use super::{Gmn1Primer, PrimerError, Reader, build_primer_with_dictionary};

/// Original held-out task source, preserved as its own document in the examples archive.
pub const SOURCE: &str = "slices/grounding/lang/examples/gmn-heldout-emission-tasks.ttl";
/// Compact bundle-bound action consumed by teachability tests.
pub const ARTIFACT: &str = "gmn-primer-teachability.json";

/// The exact shipped card and each held-out document's observed reader outcome.
#[derive(Debug, Serialize, Deserialize)]
pub struct Observations {
    pub primer: Gmn1Primer,
    pub tasks: Vec<HeldoutTask>,
}

/// Constructs are derived from the document and the full operator alphabet, independently
/// of the budget-limited primer. An omitted primer row cannot hide an exercised operator.
#[derive(Debug, Serialize, Deserialize)]
pub struct HeldoutTask {
    pub label: String,
    pub document: String,
    pub sigils: BTreeSet<String>,
    pub operator_glyphs: BTreeSet<String>,
    pub ast_error: Option<String>,
}

/// Observe the actual reader against the selected bundle's prepared native dictionary,
/// shared by the primer and every held-out document without repeated source lowering.
/// This is an explicit producer operation, never a corpus-test loader.
///
/// # Errors
/// Refuses missing task identities/documents or malformed dictionary and primer inputs.
pub fn observe(
    bundle: &RdfDataset,
    heldout: &RdfDataset,
    dictionary: &GmnDictionary,
) -> Result<Observations, PrimerError> {
    let primer = build_primer_with_dictionary(bundle, dictionary)?;
    let forms = resolve_operator_forms(dictionary.glyph_registry(), &Reader::new(bundle).labels())
        .map_err(|error| PrimerError(error.to_string()))?;
    let alphabet: BTreeSet<_> = forms.into_iter().map(|form| form.gmn_glyph).collect();
    let id = |iri: &str| heldout.term_id_by_value(&TermValue::iri(iri));
    let (Some(rdf_type), Some(task_class)) = (
        id("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
        id("https://blackcatinformatics.ca/gmeow/examples/gmn-heldout/GmnHeldoutEmissionTask"),
    ) else {
        return Err(PrimerError(
            "held-out corpus declares no emission tasks".into(),
        ));
    };
    let subjects: BTreeSet<_> = heldout
        .quads_for_pattern(None, Some(rdf_type), Some(task_class), GraphMatch::Any)
        .filter_map(|quad| match heldout.resolve(quad.s) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    if subjects.is_empty() {
        return Err(PrimerError("held-out corpus is empty".into()));
    }
    let literal = |subject: &str, predicate: &str| {
        let (Some(subject), Some(predicate)) = (id(subject), id(predicate)) else {
            return None;
        };
        heldout
            .quads_for_pattern(Some(subject), Some(predicate), None, GraphMatch::Any)
            .find_map(|quad| match heldout.resolve(quad.o) {
                TermRef::Literal { lexical, .. } => Some(lexical.to_owned()),
                _ => None,
            })
    };
    let tasks = subjects
        .into_iter()
        .map(|subject| {
            let label = literal(&subject, "http://www.w3.org/2000/01/rdf-schema#label")
                .unwrap_or_else(|| subject.clone());
            let document = literal(&subject, "http://www.w3.org/2004/02/skos/core#example")
                .ok_or_else(|| {
                    PrimerError(format!("held-out task {subject} carries no skos:example"))
                })?;
            let sigils = document
                .lines()
                .filter(|line| !line.starts_with("@gmn{"))
                .filter_map(|line| {
                    let line = line.trim_start();
                    if !line.starts_with('@') {
                        return None;
                    }
                    line.find('{').map(|index| line[..index].to_owned())
                })
                .collect();
            let operator_glyphs = alphabet
                .iter()
                .filter(|glyph| document.contains(glyph.as_str()))
                .cloned()
                .collect();
            let ast_error = gmn1_read(&Gmn1Document::from_text(&document), dictionary)
                .err()
                .map(|error| error.to_string());
            Ok(HeldoutTask {
                label,
                document,
                sigils,
                operator_glyphs,
                ast_error,
            })
        })
        .collect::<Result<_, PrimerError>>()?;
    Ok(Observations { primer, tasks })
}
