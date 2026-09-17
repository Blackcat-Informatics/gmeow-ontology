// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers kept outside the selected production source closure.

use super::*;

impl MetaDerivation {
    #[cfg(test)]
    pub(super) fn to_dataset(&self, graph_iri: &str) -> Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        self.append_to(&mut builder, graph_iri);
        builder.freeze().expect("native meta-findings")
    }

    /// Only terminal assertions in tests need a textual view of this native result.
    #[cfg(test)]
    pub(super) fn to_nquads(&self, graph_iri: &str) -> String {
        let dataset = self.to_dataset(graph_iri);
        String::from_utf8(
            crate::stages::superset::canonical_ntriples(&dataset).expect("canonical meta-findings"),
        )
        .expect("RDF text is UTF-8")
    }
}
