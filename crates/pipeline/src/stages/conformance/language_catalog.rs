// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact source-catalog observations for reference-language coverage gates.

use crate::stages::parse_sources::SourceCatalog;
use purrdf::TermRef;
use std::collections::{BTreeMap, BTreeSet};

pub(super) const CHANNEL: &str = "pipeline/language-reference-index.bin";
const SOURCE: &str = "imports/languages-reference.ttl";

pub(super) fn record(
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let index = Index::build(catalog.document(SOURCE)?);
    let bytes = bincode::serialize(&index).map_err(|error| {
        super::stage_err(&format!("encode language catalog observations: {error}"))
    })?;
    artifacts.insert(CHANNEL.to_owned(), bytes);
    Ok(())
}

/// A flat, string-keyed projection of every triple in the dataset whose subject
/// and predicate are IRIs. The object is captured as either an IRI string or the
/// literal lexical form (no datatype/lang) — sufficient for the presence and
/// set-coverage assertions these audits perform.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Index {
    /// `(subject, predicate) -> set of object IRIs`.
    pub(super) obj_iris: BTreeMap<(String, String), BTreeSet<String>>,
    /// `(subject, predicate) -> set of object literal lexical forms`.
    pub(super) obj_lits: BTreeMap<(String, String), BTreeSet<String>>,
}

impl Index {
    fn build(dataset: &purrdf::RdfDataset) -> Self {
        let mut obj_iris: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        let mut obj_lits: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for qr in dataset.quad_refs() {
            let (TermRef::Iri(s), TermRef::Iri(p)) = (qr.s, qr.p) else {
                continue;
            };
            let key = (s.to_owned(), p.to_owned());
            match qr.o {
                TermRef::Iri(o) => {
                    obj_iris.entry(key).or_default().insert(o.to_owned());
                }
                TermRef::Literal { lexical, .. } => {
                    obj_lits.entry(key).or_default().insert(lexical.to_owned());
                }
                _ => {}
            }
        }
        Self { obj_iris, obj_lits }
    }
}
