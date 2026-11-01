// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native gate evidence transport; decoding does not recompute or classify reports.
use super::{Observations, ShippedWitness};
use gmeow_logic_compile::ir::LegPath;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record!(impl ToJson for Observations {
    "codebook" => codebook,
    "codebook_sources" => codebook_sources,
    "negative" => negative,
    "negative_source_digest" => negative_source_digest,
    "pack" => pack,
    "pack_artifact_digest" => pack_artifact_digest,
    "shipped" => shipped,
});
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            codebook: fields.required("codebook")?,
            codebook_sources: fields.required("codebook_sources")?,
            negative: fields.required("negative")?,
            negative_source_digest: fields.required("negative_source_digest")?,
            pack: fields.required("pack")?,
            pack_artifact_digest: fields.required("pack_artifact_digest")?,
            shipped: fields.required("shipped")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for ShippedWitness {
    "artifact_digest" => artifact_digest,
    "source_iri" => source_iri,
    "correspondence" => correspondence,
    "leg_pair" => leg_pair,
    "round_trip_holds" => round_trip_holds,
});
impl FromJson for ShippedWitness {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ShippedWitness")?;
        Ok(Self {
            artifact_digest: fields.required("artifact_digest")?,
            source_iri: fields.required("source_iri")?,
            correspondence: fields.required("correspondence")?,
            leg_pair: fields.optional_with("leg_pair", LegPath::pair_from_json)?,
            round_trip_holds: fields.required("round_trip_holds")?,
        })
    }
}
