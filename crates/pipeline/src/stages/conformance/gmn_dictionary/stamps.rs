// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Record version stamps without materializing a second dataset for two output quads.

use std::collections::BTreeMap;

use gmeow_lang_bridge::{
    GmnDictionary, PRED_GMN_SCHEMA_VERSION, resolved_schema_version, tag_schema_version,
};
use purrdf::RdfTerm;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(in crate::stages::conformance) struct Stamps {
    pub resolved: String,
    pub dictionary_major: String,
    pub acceptance_major: String,
    pub values: BTreeMap<String, Vec<String>>,
    pub repeated_quad_equal: bool,
}

pub(super) fn record(dictionary: &GmnDictionary, acceptance_major: String) -> Stamps {
    let records = [
        "https://blackcatinformatics.ca/gmeow/examples/lang/metricRowA",
        "https://blackcatinformatics.ca/gmeow/examples/lang/verbalizationRowB",
    ];
    let stamped = records.map(|record| tag_schema_version(record, dictionary));
    let repeated_quad_equal = stamped[0] == tag_schema_version(records[0], dictionary);
    let values = records
        .into_iter()
        .map(|record| {
            let values = stamped
                .iter()
                .filter(|quad| {
                    quad.predicate == PRED_GMN_SCHEMA_VERSION
                        && matches!(&quad.subject, RdfTerm::Iri(subject) if subject == record)
                })
                .filter_map(|quad| match &quad.object {
                    RdfTerm::Literal(literal) => Some(literal.lexical_form.clone()),
                    _ => None,
                })
                .collect();
            (record.to_owned(), values)
        })
        .collect();
    Stamps {
        resolved: resolved_schema_version(dictionary),
        dictionary_major: dictionary.schema_major(),
        acceptance_major,
        values,
        repeated_quad_equal,
    }
}
