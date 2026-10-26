// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native wire ownership for the complete medium winner-table family.
//! Known record fields are strict; unknown fields retain their historical policy.

use super::{CodecRow, CodecSweep, DictionaryBaseline, GridRow, MediumBaseline};
use purrdf_lex::json::Value;
use purrdf_lex::json::record::{DecodeError, FromJson, Record};

/// Count/byte/length evidence uses exact unsigned u64 tokens, including at the
/// maximum boundary. Native generic integer admission would also accept -0.
struct SweepUnsigned(u64);

impl FromJson for SweepUnsigned {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        value.as_u64().map(Self).ok_or_else(|| {
            DecodeError::invalid_value(
                value,
                "an unsigned u64 integer token in a medium winner table",
            )
        })
    }
}

/// Signed zstd levels preserve the original i32 token contract: -0, fractions
/// and exponent notation are not integer levels. No codec/grid range is invented.
struct SweepLevel(i32);

impl FromJson for SweepLevel {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_signed(value, "a signed i32 integer level token, excluding -0")
            .map(Self)
    }
}

::purrdf_lex::json_record!(impl ToJson for MediumBaseline {
    "schema" => schema,
    "codec_sweep" => codec_sweep,
    "dictionaries" => dictionaries,
});

impl FromJson for MediumBaseline {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "MediumBaseline object")?;
        Ok(Self {
            schema: fields.required("schema")?,
            codec_sweep: fields.required("codec_sweep")?,
            dictionaries: fields.required("dictionaries")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for CodecSweep {
    "mandated_codec" => mandated_codec,
    "mandated_level" => mandated_level,
    "corpus_frame_count" => corpus_frame_count,
    "corpus_bytes" => corpus_bytes,
    "excluded_reps" => excluded_reps,
    "rows" => rows,
    "mandated_is_argmin" => mandated_is_argmin,
});

impl FromJson for CodecSweep {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "CodecSweep object")?;
        Ok(Self {
            mandated_codec: fields.required("mandated_codec")?,
            mandated_level: fields.required::<SweepLevel>("mandated_level")?.0,
            corpus_frame_count: fields.required::<SweepUnsigned>("corpus_frame_count")?.0,
            corpus_bytes: fields.required::<SweepUnsigned>("corpus_bytes")?.0,
            excluded_reps: fields.required("excluded_reps")?,
            rows: fields.required("rows")?,
            mandated_is_argmin: fields.required("mandated_is_argmin")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for CodecRow {
    "codec" => codec,
    "level" => level,
    "bytes" => bytes,
});

impl FromJson for CodecRow {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "CodecRow object")?;
        Ok(Self {
            codec: fields.required("codec")?,
            level: fields.required::<SweepLevel>("level")?.0,
            bytes: fields.required::<SweepUnsigned>("bytes")?.0,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for DictionaryBaseline {
    "id" => id,
    "population" => population,
    "declared_strategy" => declared_strategy,
    "declared_target_length" => declared_target_length,
    "winning_strategy" => winning_strategy,
    "winning_target_length" => winning_target_length,
    "declared_is_argmin" => declared_is_argmin,
    "bytes_on_disk" => bytes_on_disk,
    "bytes_on_disk_baseline" => bytes_on_disk_baseline,
    "dictionary_in_band_bytes" => dictionary_in_band_bytes,
    "two_part_code_bytes" => two_part_code_bytes,
    "dictionary_gain_fraction" => dictionary_gain_fraction,
    "corpus_sample_count" => corpus_sample_count,
    "held_out_sample_count" => held_out_sample_count,
    "corpus_digest" => corpus_digest,
    "evaluated_frame_count" => evaluated_frame_count,
    "grid" => grid,
});

impl FromJson for DictionaryBaseline {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "DictionaryBaseline object")?;
        Ok(Self {
            id: fields.required("id")?,
            population: fields.required("population")?,
            declared_strategy: fields.required("declared_strategy")?,
            declared_target_length: fields
                .required::<SweepUnsigned>("declared_target_length")?
                .0,
            winning_strategy: fields.required("winning_strategy")?,
            winning_target_length: fields.required::<SweepUnsigned>("winning_target_length")?.0,
            declared_is_argmin: fields.required("declared_is_argmin")?,
            bytes_on_disk: fields.required::<SweepUnsigned>("bytes_on_disk")?.0,
            bytes_on_disk_baseline: fields
                .required::<SweepUnsigned>("bytes_on_disk_baseline")?
                .0,
            dictionary_in_band_bytes: fields
                .required::<SweepUnsigned>("dictionary_in_band_bytes")?
                .0,
            two_part_code_bytes: fields.required::<SweepUnsigned>("two_part_code_bytes")?.0,
            dictionary_gain_fraction: fields.required("dictionary_gain_fraction")?,
            corpus_sample_count: fields.required::<SweepUnsigned>("corpus_sample_count")?.0,
            held_out_sample_count: fields.required::<SweepUnsigned>("held_out_sample_count")?.0,
            corpus_digest: fields.required("corpus_digest")?,
            evaluated_frame_count: fields.required::<SweepUnsigned>("evaluated_frame_count")?.0,
            grid: fields.required("grid")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for GridRow {
    "strategy" => strategy,
    "target_length" => target_length,
    "dictionary_bytes" => dictionary_bytes,
    "two_part_code_bytes" => two_part_code_bytes,
});

impl FromJson for GridRow {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "GridRow object")?;
        Ok(Self {
            strategy: fields.required("strategy")?,
            target_length: fields.required::<SweepUnsigned>("target_length")?.0,
            dictionary_bytes: fields.required::<SweepUnsigned>("dictionary_bytes")?.0,
            two_part_code_bytes: fields.required::<SweepUnsigned>("two_part_code_bytes")?.0,
        })
    }
}
