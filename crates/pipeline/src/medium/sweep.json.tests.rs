// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored winner-table values and literal JSON only. No committed baseline,
//! corpus, registry discovery, compression, sweep or training is invoked.

use super::*;
use purrdf_lex::json::{Value, record::DecodeError};

const WIRE: &str = r#"{
  "schema": "gmeow.medium-baseline.v2",
  "codec_sweep": {
    "mandated_codec": "zstd-rsyncable",
    "mandated_level": 12,
    "corpus_frame_count": 2,
    "corpus_bytes": 7,
    "excluded_reps": [
      "z",
      "é\"\n"
    ],
    "rows": [
      {
        "codec": "z",
        "level": 19,
        "bytes": 9
      },
      {
        "codec": "a",
        "level": -3,
        "bytes": 7
      }
    ],
    "mandated_is_argmin": false
  },
  "dictionaries": [
    {
      "id": "é",
      "population": "authored",
      "declared_strategy": "trained",
      "declared_target_length": 3,
      "winning_strategy": "term-table",
      "winning_target_length": 2,
      "declared_is_argmin": false,
      "bytes_on_disk": 4,
      "bytes_on_disk_baseline": 7,
      "dictionary_in_band_bytes": 1,
      "two_part_code_bytes": 5,
      "dictionary_gain_fraction": "0.285714",
      "corpus_sample_count": 2,
      "held_out_sample_count": 1,
      "corpus_digest": "blake3:authored",
      "evaluated_frame_count": 2,
      "grid": [
        {
          "strategy": "term-table",
          "target_length": 2,
          "dictionary_bytes": 1,
          "two_part_code_bytes": 5
        },
        {
          "strategy": "trained",
          "target_length": 1,
          "dictionary_bytes": 1,
          "two_part_code_bytes": 6
        }
      ]
    }
  ]
}
"#;

fn authored() -> MediumBaseline {
    MediumBaseline {
        schema: MEDIUM_BASELINE_SCHEMA.to_owned(),
        codec_sweep: CodecSweep {
            mandated_codec: "zstd-rsyncable".to_owned(),
            mandated_level: 12,
            corpus_frame_count: 2,
            corpus_bytes: 7,
            excluded_reps: vec!["z".to_owned(), "é\"\n".to_owned()],
            rows: vec![
                CodecRow {
                    codec: "z".to_owned(),
                    level: 19,
                    bytes: 9,
                },
                CodecRow {
                    codec: "a".to_owned(),
                    level: -3,
                    bytes: 7,
                },
            ],
            mandated_is_argmin: false,
        },
        dictionaries: vec![DictionaryBaseline {
            id: "é".to_owned(),
            population: "authored".to_owned(),
            declared_strategy: "trained".to_owned(),
            declared_target_length: 3,
            winning_strategy: "term-table".to_owned(),
            winning_target_length: 2,
            declared_is_argmin: false,
            bytes_on_disk: 4,
            bytes_on_disk_baseline: 7,
            dictionary_in_band_bytes: 1,
            two_part_code_bytes: 5,
            dictionary_gain_fraction: "0.285714".to_owned(),
            corpus_sample_count: 2,
            held_out_sample_count: 1,
            corpus_digest: "blake3:authored".to_owned(),
            evaluated_frame_count: 2,
            grid: vec![
                GridRow {
                    strategy: "term-table".to_owned(),
                    target_length: 2,
                    dictionary_bytes: 1,
                    two_part_code_bytes: 5,
                },
                GridRow {
                    strategy: "trained".to_owned(),
                    target_length: 1,
                    dictionary_bytes: 1,
                    two_part_code_bytes: 6,
                },
            ],
        }],
    }
}

#[test]
fn json_medium_baseline_literal_pretty_order_unicode_and_roundtrip() {
    let original = authored();
    assert_eq!(original.to_json().unwrap(), WIRE);
    assert_eq!(MediumBaseline::from_json(WIRE).unwrap(), original);
    assert_eq!(
        MediumBaseline::from_json(WIRE).unwrap().to_json().unwrap(),
        WIRE
    );
}

#[test]
fn json_medium_baseline_empty_vectors_and_literal_fraction_string() {
    let mut original = authored();
    original.dictionaries[0].id = "z".to_owned();
    let mut second = original.dictionaries[0].clone();
    second.id = "a".to_owned();
    original.dictionaries.push(second);
    let ordered = original.to_json().unwrap();
    assert!(ordered.find("\"id\": \"z\"").unwrap() < ordered.find("\"id\": \"a\"").unwrap());
    assert_eq!(MediumBaseline::from_json(&ordered).unwrap(), original);
    let _ = original.dictionaries.pop();
    original.codec_sweep.excluded_reps.clear();
    original.codec_sweep.rows.clear();
    original.dictionaries[0].grid.clear();
    // This field is a retained string, not a floating point parse/normalization.
    original.dictionaries[0].dictionary_gain_fraction = "0.000000".to_owned();
    let text = original.to_json().unwrap();
    assert!(text.contains("\"excluded_reps\": []"));
    assert!(text.contains("\"rows\": []"));
    assert!(text.contains("\"grid\": []"));
    assert!(text.contains("\"dictionary_gain_fraction\": \"0.000000\""));
    assert_eq!(MediumBaseline::from_json(&text).unwrap(), original);
    original.dictionaries.clear();
    let text = original.to_json().unwrap();
    assert!(text.contains("\"dictionaries\": []"));
    assert_eq!(MediumBaseline::from_json(&text).unwrap(), original);
}

#[test]
fn json_medium_baseline_schema_first_and_malformed_refusals() {
    let error = MediumBaseline::from_json(r#"{"schema":"unknown","codec_sweep":null}"#)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("unknown") && error.contains("maint-medium-sweep"),
        "{error}"
    );
    assert!(!error.contains("missing field"), "{error}");
    for text in [
        "",
        "{",
        "null",
        "[]",
        "{}",
        r#"{"schema":null}"#,
        r#"{"schema":"gmeow.medium-baseline.v2","schema":"unknown"}"#,
    ] {
        assert!(MediumBaseline::from_json(text).is_err(), "{text}");
    }
    assert!(MediumBaseline::from_json(&format!("{WIRE} null")).is_err());
}

fn root_value() -> Value {
    json::read(WIRE).unwrap()
}

fn record_values() -> Vec<(&'static str, Value)> {
    let original = authored();
    vec![
        ("baseline", ToJson::to_json(&original)),
        ("codec_sweep", ToJson::to_json(&original.codec_sweep)),
        ("codec_row", ToJson::to_json(&original.codec_sweep.rows[0])),
        ("dictionary", ToJson::to_json(&original.dictionaries[0])),
        ("grid", ToJson::to_json(&original.dictionaries[0].grid[0])),
    ]
}

fn decode_record(kind: &str, value: &Value) -> Result<(), DecodeError> {
    match kind {
        "baseline" => <MediumBaseline as FromJson>::from_json(value).map(|_| ()),
        "codec_sweep" => CodecSweep::from_json(value).map(|_| ()),
        "codec_row" => CodecRow::from_json(value).map(|_| ()),
        "dictionary" => DictionaryBaseline::from_json(value).map(|_| ()),
        "grid" => GridRow::from_json(value).map(|_| ()),
        _ => unreachable!("authored record kind"),
    }
}

#[test]
fn json_medium_baseline_each_record_missing_null_duplicates_and_unknowns() {
    for (kind, value) in record_values() {
        let fields: Vec<_> = value
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| (name.to_owned(), value.clone()))
            .collect();
        for (name, member) in fields {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(&name);
            let error = decode_record(kind, &missing).unwrap_err();
            assert!(error.to_string().contains(name.as_str()), "{kind}: {error}");
            let mut null = value.clone();
            null.as_object_mut()
                .unwrap()
                .insert(name.clone(), Value::Null);
            assert!(decode_record(kind, &null).is_err(), "{kind}.{name}");
            let mut duplicate = value.clone();
            duplicate
                .as_object_mut()
                .unwrap()
                .push(name.clone(), member);
            let error = decode_record(kind, &duplicate).unwrap_err();
            assert!(error.to_string().contains(name.as_str()), "{kind}: {error}");
        }
        let mut unknown = value.clone();
        unknown.as_object_mut().unwrap().push("future", Value::Null);
        unknown
            .as_object_mut()
            .unwrap()
            .push("future", Value::Bool(true));
        assert!(decode_record(kind, &unknown).is_ok(), "{kind}");
        for wrong in [Value::Array(Vec::new()), Value::Null, Value::Bool(true)] {
            assert!(decode_record(kind, &wrong).is_err(), "{kind}");
        }
    }
}

#[test]
fn json_medium_baseline_unsigned_fields_keep_exact_tokens_and_bounds() {
    for (kind, value) in record_values() {
        for (name, member) in value.as_object().unwrap() {
            if member.as_number().is_none() || name == "level" || name == "mandated_level" {
                continue;
            }
            for token in ["0", "18446744073709551615"] {
                let mut changed = value.clone();
                changed
                    .as_object_mut()
                    .unwrap()
                    .insert(name, json::read(token).unwrap());
                assert!(
                    decode_record(kind, &changed).is_ok(),
                    "{kind}.{name}: {token}"
                );
            }
            for token in [
                "-0",
                "-1",
                "1.0",
                "1e0",
                "18446744073709551616",
                "true",
                "\"1\"",
            ] {
                let mut changed = value.clone();
                changed
                    .as_object_mut()
                    .unwrap()
                    .insert(name, json::read(token).unwrap());
                let error = decode_record(kind, &changed).unwrap_err();
                assert!(error.to_string().contains(name.as_str()), "{kind}: {error}");
            }
        }
    }
}

#[test]
fn json_medium_baseline_signed_levels_preserve_i32_token_admission() {
    for (kind, value, field) in [
        ("codec_sweep", record_values().remove(1).1, "mandated_level"),
        ("codec_row", record_values().remove(2).1, "level"),
    ] {
        for token in ["0", "-1", "-2147483648", "2147483647"] {
            let mut changed = value.clone();
            changed
                .as_object_mut()
                .unwrap()
                .insert(field, json::read(token).unwrap());
            assert!(decode_record(kind, &changed).is_ok(), "{kind}: {token}");
        }
        for token in [
            "-0",
            "1.0",
            "1e0",
            "-2147483649",
            "2147483648",
            "true",
            "\"12\"",
        ] {
            let mut changed = value.clone();
            changed
                .as_object_mut()
                .unwrap()
                .insert(field, json::read(token).unwrap());
            assert!(decode_record(kind, &changed).is_err(), "{kind}: {token}");
        }
    }
}

#[test]
fn json_medium_baseline_full_decoder_keeps_nested_error_paths_and_types() {
    let bad = WIRE.replace("\"target_length\": 2", "\"target_length\": -0");
    let error = MediumBaseline::from_json(&bad).unwrap_err().to_string();
    assert!(
        error.contains("/dictionaries/0/grid/0/target_length"),
        "{error}"
    );
    for (old, new) in [
        ("\"mandated_is_argmin\": false", "\"mandated_is_argmin\": 0"),
        (
            "\"dictionary_gain_fraction\": \"0.285714\"",
            "\"dictionary_gain_fraction\": 0.285714",
        ),
        (
            "\"winning_target_length\": 2",
            "\"winning_target_length\": 2, \"winning_target_length\": 2",
        ),
    ] {
        assert!(
            MediumBaseline::from_json(&WIRE.replace(old, new)).is_err(),
            "{old}"
        );
    }
    let mut wrong_vector = root_value();
    *wrong_vector
        .get_mut("codec_sweep")
        .unwrap()
        .get_mut("rows")
        .unwrap() = Value::Object(Default::default());
    assert!(MediumBaseline::from_json(&json::write_compact(&wrong_vector)).is_err());
    let mut unknown = root_value();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("future", Value::Bool(true));
    assert_eq!(
        MediumBaseline::from_json(&json::write_compact(&unknown)).unwrap(),
        authored()
    );
}

#[test]
fn json_medium_baseline_winner_target_never_saturates_or_changes_strategy() {
    let mut row = authored().dictionaries.remove(0);
    assert_eq!(row.winner().unwrap(), (DictionaryStrategy::TermTable, 2));
    row.winning_target_length = u64::MAX;
    match usize::try_from(u64::MAX) {
        Ok(target) => assert_eq!(
            row.winner().unwrap(),
            (DictionaryStrategy::TermTable, target)
        ),
        Err(_) => {
            let error = row.winner().unwrap_err().to_string();
            assert!(
                error.contains("é")
                    && error.contains("usize")
                    && error.contains(&u64::MAX.to_string()),
                "{error}"
            );
        }
    }
    row.winning_strategy = "unknown".to_owned();
    assert!(row.winner().is_err());
}
