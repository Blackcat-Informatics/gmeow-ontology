// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored JSON only: no timing collection, filesystem, engine, corpus or producer.

use super::*;

fn authored(expected: &str) -> Command {
    read_command(
        format!(r#"{{"command":"{expected}","deterministic_work":{{}},"observations":{{}}}}"#)
            .as_bytes(),
        expected,
    )
    .unwrap()
}

#[test]
fn json_perf_gate_whole_literal_output_preserves_envelope_and_pretty_lf() {
    let commands = FILES
        .into_iter()
        .map(|(_, label)| authored(label))
        .collect();
    let actual = merged_json(commands);
    assert_eq!(
        actual,
        r#"{
  "command": "perf-gate",
  "commands": [
    {
      "command": "validate",
      "deterministic_work": {},
      "observations": {}
    },
    {
      "command": "sync",
      "deterministic_work": {},
      "observations": {}
    },
    {
      "command": "reason-verify",
      "deterministic_work": {},
      "observations": {}
    }
  ],
  "deterministic_work": [
    {
      "command": "validate",
      "work": {}
    },
    {
      "command": "sync",
      "work": {}
    },
    {
      "command": "reason-verify",
      "work": {}
    }
  ],
  "observations": [
    {
      "command": "validate",
      "observations": {}
    },
    {
      "command": "sync",
      "observations": {}
    },
    {
      "command": "reason-verify",
      "observations": {}
    }
  ],
  "schema_version": 1
}
"#
    );
    assert!(actual.ends_with("}\n"));
    assert!(!actual.ends_with("\n\n"));
}

#[test]
fn json_perf_gate_full_documents_and_opaque_versions_are_retained() {
    let command = read_command(br#"{"command":"validate","schema_version":"future","extra":{"z":null,"a":[true,"\u732b"]},"deterministic_work":{"x":[]},"observations":{"y":false}}"#, "validate").unwrap();
    let text = merged_json(vec![command]);
    let value = json::record::read_document(text.as_bytes()).unwrap();
    assert_eq!(
        value
            .pointer("/commands/0/schema_version")
            .and_then(Value::as_str),
        Some("future")
    );
    assert_eq!(
        value
            .pointer("/commands/0/extra/a/1")
            .and_then(Value::as_str),
        Some("猫")
    );
    assert_eq!(
        value.pointer("/commands/0/deterministic_work"),
        value.pointer("/deterministic_work/0/work")
    );
    assert_eq!(
        value.pointer("/commands/0/observations"),
        value.pointer("/observations/0/observations")
    );
    assert!(text.contains("\"z\": null"));
}

#[test]
fn json_perf_gate_duplicates_resolve_last_recursively_before_admission() {
    let command = read_command(br#"{"command":"wrong","command":"validate","deterministic_work":null,"deterministic_work":{"z":0,"z":1,"a":{"x":false,"x":true}},"observations":[],"observations":{}}"#, "validate").unwrap();
    assert_eq!(
        json::write_compact(&command.work),
        r#"{"a":{"x":true},"z":1}"#
    );
    assert!(matches!(read_command(br#"{"command":"validate","command":"wrong","deterministic_work":{},"observations":{}}"#, "validate"), Err(InputError::Contract(_))));
    assert!(matches!(read_command(br#"{"command":"validate","deterministic_work":{},"deterministic_work":null,"observations":{}}"#, "validate"), Err(InputError::Contract(_))));
}

#[test]
fn json_perf_gate_document_root_must_be_an_object() {
    for raw in ["null", "false", "1", "\"document\"", "[]", "[{}]"] {
        assert!(
            matches!(read_command(raw.as_bytes(), "validate"), Err(InputError::Contract(message)) if message.contains("document must be an object")),
            "{raw}"
        );
    }
}

#[test]
fn json_perf_gate_required_projection_fields_refuse_missing_and_every_wrong_kind() {
    for field in ["deterministic_work", "observations"] {
        let other = if field == "deterministic_work" {
            "observations"
        } else {
            "deterministic_work"
        };
        let missing = format!(r#"{{"command":"validate","{other}":{{}}}}"#);
        assert!(
            matches!(read_command(missing.as_bytes(), "validate"), Err(InputError::Contract(message)) if message.contains(field) && message.contains("object"))
        );
        for wrong in ["null", "false", "1", "\"object\"", "[]"] {
            let raw = format!(r#"{{"command":"validate","{other}":{{}},"{field}":{wrong}}}"#);
            assert!(
                matches!(read_command(raw.as_bytes(), "validate"), Err(InputError::Contract(message)) if message.contains(field)),
                "{raw}"
            );
        }
    }
}

#[test]
fn json_perf_gate_command_is_required_string_bound_to_each_filename() {
    for (file, expected) in FILES {
        assert_eq!(file, format!("{expected}.json"));
        for wrong in ["null", "false", "1", "[]", "{}", "\"wrong\"", "\"\""] {
            let raw =
                format!(r#"{{"command":{wrong},"deterministic_work":{{}},"observations":{{}}}}"#);
            assert!(
                matches!(read_command(raw.as_bytes(), expected), Err(InputError::Contract(message)) if message.contains("command") && message.contains(expected)),
                "{file}: {raw}"
            );
        }
        assert!(matches!(
            read_command(br#"{"deterministic_work":{},"observations":{}}"#, expected),
            Err(InputError::Contract(_))
        ));
    }
}

#[test]
fn json_perf_gate_native_numbers_preserve_opaque_lexemes_without_identities() {
    let command = read_command(br#"{"command":"validate","deterministic_work":{"large":18446744073709551616,"precise":9007199254740993,"fraction":1.00},"observations":{"exp":1e400,"negative_zero":-0},"unused":-1E-400}"#, "validate").unwrap();
    let text = merged_json(vec![command]);
    for token in [
        "18446744073709551616",
        "9007199254740993",
        "1.00",
        "1e400",
        "-0",
        "-1E-400",
    ] {
        assert!(text.contains(token), "{token}: {text}");
    }
    let value = json::record::read_document(text.as_bytes()).unwrap();
    assert_eq!(
        value
            .pointer("/commands/0/deterministic_work/fraction")
            .and_then(Value::as_number)
            .unwrap()
            .lexeme(),
        "1.00"
    );
    assert!(value.get("identity").is_none());
}

#[test]
fn json_perf_gate_cache_hit_and_nullable_profiles_remain_opaque() {
    let sync = read_command(br#"{"command":"sync","cache_hit":true,"deterministic_work":{"stage_receipts":[],"input_digest":"authored"},"observations":{"manifest_hit":true,"pipeline_runs":0}}"#, "sync").unwrap();
    let reason = read_command(br#"{"command":"reason-verify","deterministic_work":{"budget_limit":null},"observations":{"timings":[{"metadata":null}]}}"#, "reason-verify").unwrap();
    let value = json::record::read_document(merged_json(vec![sync, reason]).as_bytes()).unwrap();
    assert!(value.pointer("/commands/0/observations/timings").is_none());
    assert_eq!(
        value
            .pointer("/commands/0/observations/pipeline_runs")
            .and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(
        value.pointer("/deterministic_work/1/work/budget_limit"),
        Some(&Value::Null)
    );
    assert_eq!(
        value.pointer("/observations/1/observations/timings/0/metadata"),
        Some(&Value::Null)
    );
}

#[test]
fn json_perf_gate_malformed_json_remains_a_parse_failure() {
    for raw in [
        &b""[..],
        &b"{"[..],
        &b"{} {}"[..],
        &b"{\"command\":NaN}"[..],
        &b"\xff"[..],
        &br#"{"command":"\uD800"}"#[..],
    ] {
        assert!(matches!(
            read_command(raw, "validate"),
            Err(InputError::Parse(_))
        ));
    }
}
