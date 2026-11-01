// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete pack evidence transport without recomputing measurements or verdicts.
use super::controls::{Controls, Pair, VerbalizerControl};
use super::{EmissionWitness, Metrics, Observations};
use gmeow_logic_compile::ir::LegPath;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, items_with},
};

::purrdf_lex::json_record!(impl ToJson for Observations {
    "major" => major,
    "paths" => paths,
    "emissions" => emissions,
    "metrics" => metrics,
    "grounding_metrics" => grounding_metrics,
    "grounding_sources" => grounding_sources,
    "expected_codebook_digest" => expected_codebook_digest,
    "expected_pack_root" => expected_pack_root,
    "expected_grammar_leaf" => expected_grammar_leaf,
    "ecosystem_leaves" => ecosystem_leaves,
    "controls" => controls,
});
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            major: fields.required("major")?,
            paths: fields.required("paths")?,
            emissions: fields.required("emissions")?,
            metrics: fields.required("metrics")?,
            grounding_metrics: fields.required("grounding_metrics")?,
            grounding_sources: fields.required("grounding_sources")?,
            expected_codebook_digest: fields.required("expected_codebook_digest")?,
            expected_pack_root: fields.required("expected_pack_root")?,
            expected_grammar_leaf: fields.required("expected_grammar_leaf")?,
            ecosystem_leaves: fields.required_with("ecosystem_leaves", |value| {
                items_with(value, |row| -> Result<_, DecodeError> {
                    let [subject, predicate, digest] =
                        gmeow_errors::json::read_string_row::<3>(row)?;
                    Ok((subject, predicate, digest))
                })
            })?,
            controls: fields.required("controls")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for EmissionWitness {
    "path" => path,
    "is_rdf" => is_rdf,
    "correspondence" => correspondence,
    "round_trip" => round_trip,
    "leg_pair" => leg_pair,
    "source_matches_artifact" => source_matches_artifact,
});
impl FromJson for EmissionWitness {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct EmissionWitness")?;
        Ok(Self {
            path: fields.required("path")?,
            is_rdf: fields.required("is_rdf")?,
            correspondence: fields.required("correspondence")?,
            round_trip: fields.required("round_trip")?,
            leg_pair: fields.optional_with("leg_pair", LegPath::pair_from_json)?,
            source_matches_artifact: fields.required("source_matches_artifact")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for Metrics {
    "bytes_on_disk" => bytes_on_disk,
    "tokens_in_context" => tokens_in_context,
    "ast_validity_rate" => ast_validity_rate,
    "roundtrip_loss" => roundtrip_loss,
    "compression_ratio" => compression_ratio,
    "glyph_density" => glyph_density,
    "dictionary_hit_rate" => dictionary_hit_rate,
    "gmn_worst_case_tokens" => gmn_worst_case_tokens,
    "gmn_realistic_tokens" => gmn_realistic_tokens,
    "turtle_best_case_tokens" => turtle_best_case_tokens,
    "gmn_ascii_bytes" => gmn_ascii_bytes,
    "gmn_nonascii_bytes" => gmn_nonascii_bytes,
    "turtle_bytes_on_disk" => turtle_bytes_on_disk,
    "jsonld_bytes_on_disk" => jsonld_bytes_on_disk,
    "total_sources" => total_sources,
    "measured_sources" => measured_sources,
    "compression_gate_holds" => compression_gate_holds,
});
impl FromJson for Metrics {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Metrics")?;
        Ok(Self {
            bytes_on_disk: fields.required_with("bytes_on_disk", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 bytes_on_disk")
            })?,
            tokens_in_context: fields.required_with("tokens_in_context", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 tokens_in_context")
            })?,
            ast_validity_rate: fields.required("ast_validity_rate")?,
            roundtrip_loss: fields.required("roundtrip_loss")?,
            compression_ratio: fields.required("compression_ratio")?,
            glyph_density: fields.required("glyph_density")?,
            dictionary_hit_rate: fields.required("dictionary_hit_rate")?,
            gmn_worst_case_tokens: fields.required_with("gmn_worst_case_tokens", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 gmn_worst_case_tokens")
            })?,
            gmn_realistic_tokens: fields.required_with("gmn_realistic_tokens", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 gmn_realistic_tokens")
            })?,
            turtle_best_case_tokens: fields.required_with("turtle_best_case_tokens", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 turtle_best_case_tokens")
            })?,
            gmn_ascii_bytes: fields.required_with("gmn_ascii_bytes", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 gmn_ascii_bytes")
            })?,
            gmn_nonascii_bytes: fields.required_with("gmn_nonascii_bytes", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 gmn_nonascii_bytes")
            })?,
            turtle_bytes_on_disk: fields.required_with("turtle_bytes_on_disk", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 turtle_bytes_on_disk")
            })?,
            jsonld_bytes_on_disk: fields.required_with("jsonld_bytes_on_disk", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 jsonld_bytes_on_disk")
            })?,
            total_sources: fields.required_with("total_sources", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 total_sources")
            })?,
            measured_sources: fields.required_with("measured_sources", |value| {
                gmeow_errors::json::read_unsigned(value, "u64 measured_sources")
            })?,
            compression_gate_holds: fields.required("compression_gate_holds")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for Controls {
    "bumped_major" => bumped_major,
    "bumped_paths" => bumped_paths,
    "verbalizer" => verbalizer,
    "perturbed_text" => perturbed_text,
    "scoped" => scoped,
    "erased_scope_error" => erased_scope_error,
});
impl FromJson for Controls {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Controls")?;
        Ok(Self {
            bumped_major: fields.required("bumped_major")?,
            bumped_paths: fields.required("bumped_paths")?,
            verbalizer: fields.required("verbalizer")?,
            perturbed_text: fields.required("perturbed_text")?,
            scoped: fields.required("scoped")?,
            erased_scope_error: fields.required("erased_scope_error")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for VerbalizerControl {
    "emission" => emission,
    "text" => text,
    "pairs" => pairs,
    "round_trip" => round_trip,
});
impl FromJson for VerbalizerControl {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct VerbalizerControl")?;
        Ok(Self {
            emission: fields.required("emission")?,
            text: fields.required("text")?,
            pairs: fields.required("pairs")?,
            round_trip: fields.required("round_trip")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for Pair {
    "label" => label,
    "nl" => nl,
    "surface" => surface,
    "inverse_recovers_form" => inverse_recovers_form,
});
impl FromJson for Pair {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Pair")?;
        Ok(Self {
            label: fields.required("label")?,
            nl: fields.required("nl")?,
            surface: fields.required("surface")?,
            inverse_recovers_form: fields.required("inverse_recovers_form")?,
        })
    }
}
