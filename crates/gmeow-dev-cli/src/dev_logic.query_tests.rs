// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::resolve_query;

const HORN_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

#[test]
fn counterfactual_depth_refusal_does_not_build_an_unused_base_snapshot() {
    // The quoted-triple object is deliberately outside the snapshot reifier
    // contract. A depth-zero counterfactual returns before it needs any base
    // snapshot; the plain-query preparation path must not run speculatively.
    let nquads = "<https://ex/s> <https://ex/meta> \
                      <<( <https://ex/qs> <https://ex/qp> <https://ex/qo> )>> \
                      <http://world/base> .\n";
    let program = ":- prefix(ex, 'https://ex/').\n\
                       :- counterfactual('http://world/cf', 'http://world/base').\n\
                       :- depth_budget(0).\n\
                       :- assume(ex:p2(ex:s, ex:o2)).\n\
                       ?- ex:p(ex:s, Z).\n";

    let (answers, status) = resolve_query(nquads, program, HORN_PROFILE, None, None, None).unwrap();
    assert!(answers.is_empty());
    assert_eq!(status, "incomplete");
}

/// Preserve complete response packets through the actual serializer without query execution.
#[test]
fn query_json_preserves_complete_authored_response_packets() {
    use super::Answer;
    let binding = |probability| Answer {
        vars: vec![
            ("z".into(), "first".into()),
            ("a".into(), "é中😀\n\"\\".into()),
            ("z".into(), "last".into()),
            ("probability".into(), "authored-binding".into()),
        ],
        probability,
    };
    let cases = [
        ("empty", Vec::new(), "completed é中😀"),
        (
            "collisions",
            vec![
                binding(None),
                binding(Some(0.25)),
                binding(Some(f64::NAN)),
                Answer {
                    vars: vec![],
                    probability: Some(f64::INFINITY),
                },
                Answer {
                    vars: vec![],
                    probability: Some(f64::NEG_INFINITY),
                },
            ],
            "budget-exhausted",
        ),
        (
            "finite-weights",
            [
                0.0,
                -0.0,
                1.0,
                -1.5,
                1e-6,
                1e-7,
                1e16,
                1e20,
                5e-324,
                f64::MAX,
            ]
            .into_iter()
            .map(|probability| Answer {
                vars: vec![("value".into(), "authored".into())],
                probability: Some(probability),
            })
            .collect(),
            "completed",
        ),
    ];
    let references = include_str!("dev_logic/fixtures/query-json-v1.txt");
    assert_eq!(references.lines().count(), 3);
    for (name, answers, status) in cases {
        let bytes = format!("{}\n", super::query_json(&answers, status).unwrap()).into_bytes();
        let row = references
            .lines()
            .find(|row| row.split_whitespace().next() == Some(name))
            .unwrap();
        let mut fields = row.split_whitespace();
        assert_eq!(fields.next(), Some(name));
        let digest = fields.next().unwrap();
        let original = purrdf_hash::hex::decode(fields.next().unwrap()).unwrap();
        assert!(fields.next().is_none());
        assert_eq!(purrdf::ContentDigest::of(&bytes).to_hex(), digest, "{name}");
        assert_eq!(bytes, original, "{name}");
    }
}
