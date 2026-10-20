// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Wire adapter for native absolute IRIs used as diagnostic graph identities.
//! No reference resolution or normalization is performed.

use purrdf_core::BaseIri;
use serde::{Deserialize, Deserializer, Serializer};

pub(crate) fn serialize<S: Serializer>(
    graph: &Option<BaseIri>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match graph {
        Some(graph) => serializer.serialize_some(graph.as_str()),
        None => serializer.serialize_none(),
    }
}

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<BaseIri>, D::Error> {
    Option::<String>::deserialize(deserializer)?
        .map(|graph| BaseIri::parse(&graph).map_err(serde::de::Error::custom))
        .transpose()
}
