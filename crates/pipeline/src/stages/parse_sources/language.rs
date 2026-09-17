// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native language analyses shared for the lifetime of one admitted source catalog.

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_lang_bridge::{CurrentCodebook, GmnDictionary, GmnOperatorForm, RingLattice};
use purrdf::{RdfDataset, RdfTerm};

pub(crate) const LANG: &str = "slices/grounding/lang/module.ttl";
pub(crate) const MODULES: [&str; 3] = [
    "slices/grounding/logic/module.ttl",
    LANG,
    "slices/grounding/math/module.ttl",
];

pub(crate) struct LanguageContext {
    pub dictionary: Arc<GmnDictionary>,
    pub codebook: Arc<CurrentCodebook>,
    pub dialect: gmeow_lang_bridge::gmn1_codec::DialectAcceptance,
    pub operator_forms: Arc<[GmnOperatorForm]>,
    pub lattice: Arc<RingLattice>,
}

impl LanguageContext {
    pub(super) fn compile(catalog: &super::SourceCatalog) -> gmeow_errors::Result<Self> {
        let lang = catalog.document(LANG)?;
        let (dictionary, codebook) = gmeow_lang_bridge::gmn1_codec::compile_gmn_codebook(lang)
            .map_err(|error| super::stage_error(error.0))?;
        let dialect = gmeow_lang_bridge::resolve_dialect_acceptance(lang)
            .map_err(|error| super::stage_error(error.0))?
            .ok_or_else(|| {
                super::stage_error("selected language source has no GMN dialect lineage".to_owned())
            })?;
        let modules: Vec<_> = MODULES
            .iter()
            .map(|path| catalog.document(path))
            .collect::<gmeow_errors::Result<_>>()?;
        let labels = labels(modules);
        let forms = gmeow_lang_bridge::resolve_operator_forms(dictionary.glyph_registry(), &labels)
            .map_err(|error| super::stage_error(error.to_string()))?;
        Ok(Self {
            dictionary: Arc::new(dictionary),
            codebook: Arc::new(codebook),
            dialect,
            operator_forms: forms.into(),
            lattice: Arc::new(RingLattice::from_dataset(lang)),
        })
    }
}

/// Select the authored rendering label deterministically. This index is a
/// presentation coordinate and never changes coexisting semantic claims.
fn labels<'a>(modules: impl IntoIterator<Item = &'a RdfDataset>) -> BTreeMap<String, String> {
    let mut best: BTreeMap<String, (bool, String)> = BTreeMap::new();
    for dataset in modules {
        for quad in dataset.owned_quads() {
            if quad.predicate != "http://www.w3.org/2000/01/rdf-schema#label" {
                continue;
            }
            let (RdfTerm::Iri(subject), RdfTerm::Literal(literal)) = (quad.subject, quad.object)
            else {
                continue;
            };
            let candidate = (
                literal.language.as_deref() == Some("x-gmeow-english"),
                literal.lexical_form,
            );
            let replace = best.get(&subject).is_none_or(|current| {
                (candidate.0, std::cmp::Reverse(&candidate.1))
                    > (current.0, std::cmp::Reverse(&current.1))
            });
            if replace {
                best.insert(subject, candidate);
            }
        }
    }
    best.into_iter()
        .map(|(subject, (_, lexical))| (subject, lexical))
        .collect()
}

#[path = "language.tests.rs"]
#[cfg(test)]
mod tests;
