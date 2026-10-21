// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independent authored tally records and literal bytes only. No corpus,
//! authenticated fixture loader, grading operation or producer is invoked.

use super::*;
use crate::stages::conformance::{encode_agreement_tallies, parse_agreement_tallies};

fn record() -> TallyRecord {
    TallyRecord {
        lane: "a".to_owned(),
        cases: 1,
        agree: 1,
        corpus_only: 0,
        dl_gap: 0,
        gap_shapes: BTreeMap::new(),
    }
}

fn one_tally(member: &str) -> String {
    format!(r#"{{"tiny":{{"lane":"a","cases":1,"agree":1,"corpus_only":0,"dl_gap":0{member}}}}}"#)
}

#[test]
fn json_agreement_tally_literal_pretty_order_escaping_and_lf() {
    let mut decorated = record();
    decorated.lane = "a\"\n😀".to_owned();
    decorated.gap_shapes = BTreeMap::from([("z".to_owned(), 2), ("é".to_owned(), 3)]);
    let records = BTreeMap::from([("z".to_owned(), record()), ("é".to_owned(), decorated)]);
    let expected = r#"{
  "z": {
    "lane": "a",
    "cases": 1,
    "agree": 1,
    "corpus_only": 0,
    "dl_gap": 0
  },
  "é": {
    "lane": "a\"\n😀",
    "cases": 1,
    "agree": 1,
    "corpus_only": 0,
    "dl_gap": 0,
    "gap_shapes": {
      "z": 2,
      "é": 3
    }
  }
}
"#;
    assert_eq!(encode_agreement_tallies(&records), expected.as_bytes());
    assert_eq!(
        parse_agreement_tallies(expected.as_bytes()).unwrap(),
        records
    );
    assert_eq!(encode_agreement_tallies(&BTreeMap::new()), b"{}\n");
}

#[test]
fn json_agreement_tally_missing_empty_map_and_unknown_fields() {
    let missing = one_tally("");
    let empty = one_tally(r#", "gap_shapes":{}"#);
    let unknown = one_tally(r#", "future":[null,1], "future":false"#);
    for text in [&missing, &empty, &unknown] {
        assert_eq!(
            parse_agreement_tallies(text.as_bytes()).unwrap()["tiny"],
            record()
        );
    }
    assert!(
        !String::from_utf8(encode_agreement_tallies(
            &parse_agreement_tallies(empty.as_bytes()).unwrap()
        ))
        .unwrap()
        .contains("gap_shapes")
    );
    for text in [
        one_tally(r#", "gap_shapes":null"#),
        one_tally(r#", "gap_shapes":[]"#),
    ] {
        assert!(parse_agreement_tallies(text.as_bytes()).is_err(), "{text}");
    }
}

#[test]
fn json_agreement_tally_known_duplicates_missing_types_and_malformed_refuse() {
    for extra in [
        r#", "lane":"b""#,
        r#", "cases":1"#,
        r#", "agree":1"#,
        r#", "corpus_only":0"#,
        r#", "dl_gap":0"#,
        r#", "gap_shapes":{}, "gap_shapes":{}"#,
    ] {
        assert!(
            parse_agreement_tallies(one_tally(extra).as_bytes()).is_err(),
            "{extra}"
        );
    }
    for text in [
        "",
        "{",
        "[]",
        "null",
        r#"{"tiny":[]}"#,
        r#"{"tiny":{"lane":"a","cases":1,"agree":1,"corpus_only":0}}"#,
        r#"{"tiny":{"cases":1,"agree":1,"corpus_only":0,"dl_gap":0}}"#,
    ] {
        assert!(parse_agreement_tallies(text.as_bytes()).is_err(), "{text}");
    }
    for field in ["lane", "cases", "agree", "corpus_only", "dl_gap"] {
        let original = if field == "lane" {
            r#""lane":"a""#.to_owned()
        } else {
            format!(
                "\"{field}\":{}",
                if field == "cases" || field == "agree" {
                    1
                } else {
                    0
                }
            )
        };
        for wrong in ["null", "[]", "{}", "true"] {
            let text = one_tally("").replace(&original, &format!("\"{field}\":{wrong}"));
            let error = parse_agreement_tallies(text.as_bytes()).unwrap_err();
            assert!(error.to_string().contains(field), "{error}");
        }
        let missing = if field == "dl_gap" {
            one_tally("").replace(&format!(",{original}"), "")
        } else {
            one_tally("").replace(&format!("{original},"), "")
        };
        let error = parse_agreement_tallies(missing.as_bytes()).unwrap_err();
        assert!(error.to_string().contains(field), "{error}");
    }
    assert!(parse_agreement_tallies(format!("{} null", one_tally("")).as_bytes()).is_err());
}

#[test]
fn json_agreement_tally_unsigned_tokens_and_nested_pointers() {
    for token in ["0".to_owned(), usize::MAX.to_string()] {
        for field in ["cases", "agree", "corpus_only", "dl_gap"] {
            let value = if field == "cases" || field == "agree" {
                1
            } else {
                0
            };
            let text = one_tally("").replace(
                &format!("\"{field}\":{value}"),
                &format!("\"{field}\":{token}"),
            );
            assert!(parse_agreement_tallies(text.as_bytes()).is_ok(), "{text}");
        }
        let shapes = one_tally(&format!(r#", "gap_shapes":{{"shape":{token}}}"#));
        assert_eq!(
            parse_agreement_tallies(shapes.as_bytes()).unwrap()["tiny"].gap_shapes["shape"]
                .to_string(),
            token
        );
    }
    let overflow = ((usize::MAX as u128) + 1).to_string();
    for token in [
        "-0",
        "-1",
        "1.0",
        "1e0",
        "true",
        "\"1\"",
        "18446744073709551616",
        &overflow,
    ] {
        for field in ["cases", "agree", "corpus_only", "dl_gap"] {
            let value = if field == "cases" || field == "agree" {
                1
            } else {
                0
            };
            let text = one_tally("").replace(
                &format!("\"{field}\":{value}"),
                &format!("\"{field}\":{token}"),
            );
            let error = parse_agreement_tallies(text.as_bytes()).unwrap_err();
            assert!(error.to_string().contains(field), "{error}");
        }
        let error = parse_agreement_tallies(
            one_tally(&format!(r#", "gap_shapes":{{"role/assertion":{token}}}"#)).as_bytes(),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("gap_shapes") && message.contains("role~1assertion"),
            "{message}"
        );
    }
}

#[test]
fn json_agreement_tally_map_duplicates_last_win_without_hiding_invalid_records() {
    let text = r#"{"tiny":{"lane":"b","cases":2,"agree":2,"corpus_only":0,"dl_gap":0},"tiny":{"lane":"a","cases":1,"agree":1,"corpus_only":0,"dl_gap":0}}"#;
    assert_eq!(
        parse_agreement_tallies(text.as_bytes()).unwrap()["tiny"],
        record()
    );
    let nested = one_tally(r#", "gap_shapes":{"shape":1,"shape":2}"#);
    assert_eq!(
        parse_agreement_tallies(nested.as_bytes()).unwrap()["tiny"].gap_shapes["shape"],
        2
    );
    assert!(
        parse_agreement_tallies(one_tally(r#", "gap_shapes":{"shape":-0,"shape":2}"#).as_bytes())
            .is_err()
    );
    assert!(
        parse_agreement_tallies(text.replace("\"cases\":2", "\"cases\":-0").as_bytes()).is_err()
    );
}

#[test]
fn json_agreement_tally_rate_handles_unsigned_boundary_without_changing_rounding() {
    assert_eq!(agree_rate(usize::MAX, usize::MAX).unwrap(), "100.0%");
    assert_eq!(agree_rate(usize::MAX - 1, usize::MAX).unwrap(), "99.9%");
    assert_eq!(agree_rate(usize::MAX / 2, usize::MAX).unwrap(), "50.0%");
    // The migration does not invent a partition or agree<=cases admission rule.
    assert_eq!(agree_rate(2, 1).unwrap(), "200.0%");
    assert!(agree_rate(1, 0).is_err());
}

#[test]
fn json_agreement_tally_all_displayed_aggregates_refuse_overflow_with_identity() {
    for field in ["cases", "agree", "corpus_only", "dl_gap"] {
        let mut first = record();
        match field {
            "cases" => first.cases = usize::MAX,
            "agree" => first.agree = usize::MAX,
            "corpus_only" => first.corpus_only = usize::MAX,
            _ => first.dl_gap = usize::MAX,
        }
        let mut second = record();
        second.corpus_only = 1;
        second.dl_gap = 1;
        let records = BTreeMap::from([("first".to_owned(), first), ("second".to_owned(), second)]);
        let error = render_agreement_matrix(&encode_agreement_tallies(&records)).unwrap_err();
        let message = format!("{error:?}");
        assert!(
            message.contains(field) && message.contains("second"),
            "{message}"
        );
    }
    let mut first = record();
    first.gap_shapes.insert("shape".to_owned(), usize::MAX);
    let mut second = record();
    second.gap_shapes.insert("shape".to_owned(), 1);
    let error = render_agreement_matrix(&encode_agreement_tallies(&BTreeMap::from([
        ("first".to_owned(), first),
        ("second".to_owned(), second),
    ])))
    .unwrap_err();
    let message = format!("{error:?}");
    assert!(
        message.contains("gap shape shape") && message.contains("second"),
        "{message}"
    );
}

#[test]
fn json_agreement_tally_matrix_literal_bytes_and_real_refusals() {
    let wire = encode_agreement_tallies(&BTreeMap::from([("tiny".to_owned(), record())]));
    let artifacts = render_agreement_matrix(&wire).unwrap();
    let expected = r#"<!-- GENERATED by `gmeow-dev sync --mode update --outputs generated` (agreement) — DO NOT EDIT. -->

# gmeow external-corpus agreement matrix

The native reasoner graded against published external ground truth — TPTP SZS
problems, W3C `otest:`/`mf:` entailment manifests, and OntoUML foundation
disciplines — over the committed `conformance/logic/cases/external/` corpus. Each
case's native verdict (frozen at vendor time, the golden the conformance harness
re-asserts) is compared to the corpus's published expected verdict.

* **agree** — the native verdict matched the published expected.
* **corpus-only** — the native path DECIDED but disagreed with the published
  expected (a real defect for an agreement-expected corpus; DOCUMENTED and
  intended for a `divergence`-lane corpus, see below).
* **dl-gap** — the native path could not decide the case (an honest coverage gap).

## Agreement (agreement-expected corpora)

| corpus | lane | cases | agree | corpus-only | dl-gap | agree rate |
|---|---|---|---|---|---|---|
| tiny | a | 1 | 1 | 0 | 0 | 100.0% |
| **TOTAL** | — | 1 | 1 | 0 | 0 | **100.0%** |

## Documented divergences (native EL intentionally differs — not defects)

These `divergence`-lane corpora are vendored to PROVE the native engine correctly
declines (a dl-gap) or differs (a corpus-only row) from the published DL/Full answer;
both are documented, intended, and excluded from the headline agreement rate above.

_(none in the committed corpus)_

## Capability gaps (by shape)

The structured `gmeow:gapShape` of every divergence case the native reasoner honestly
declined to grade (an out-of-fragment conclusion shape), tallied across corpora carrying
structured gap shapes.

_(none in the committed corpus)_

1 agreement-expected corpus(es), 0 documented-divergence corpus(es).
"#;
    assert_eq!(artifacts[AGREEMENT_MATRIX_PATH], expected.as_bytes());
    for text in [b"{}".as_slice(), b"null".as_slice(), b"{".as_slice()] {
        assert!(render_agreement_matrix(text).is_err());
    }
    assert!(render_agreement_matrix(one_tally(",\"cases\":2").as_bytes()).is_err());
}
