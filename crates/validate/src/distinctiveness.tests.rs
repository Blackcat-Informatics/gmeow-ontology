// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn skeleton_lowercases_and_collapses_whitespace() {
    assert_eq!(
        skeleton("Assert  in the\tnatural   direction. "),
        "assert in the natural direction."
    );
}

#[test]
fn skeleton_keeps_curies_so_distinct_ranges_stay_distinct() {
    // Two usage coats sharing a frame but naming their own distinct range are NOT
    // near-duplicates — the load-bearing CURIE is kept, so they do not collide.
    let a = skeleton("Set it on a math:Sample with range math:ObservationUnit.");
    let b = skeleton("Set it on a math:Sample with range math:StatisticalVariable.");
    assert_ne!(a, b, "distinct ranges must stay distinct: {a:?} vs {b:?}");
    // A byte-identical coat (modulo case/space) DOES collide.
    let c = skeleton("Assert in the natural direction and read as its inverse.");
    let d = skeleton("assert in the natural direction and read as its inverse. ");
    assert_eq!(c, d);
}

#[test]
fn collisions_flags_n2_and_ignores_singletons_and_empty() {
    let items = vec![
        ("ex:A".to_owned(), skeleton("Avoid a partial quaternion.")),
        ("ex:B".to_owned(), skeleton("Avoid a partial quaternion.")),
        ("ex:C".to_owned(), skeleton("Set exactly one geocode.")),
        ("ex:D".to_owned(), "   ".to_owned()), // empty skeleton — skipped
        ("ex:E".to_owned(), String::new()),    // empty — skipped
    ];
    let got = collisions(items);
    assert_eq!(got.len(), 1, "one N=2 group: {got:#?}");
    assert_eq!(got[0].skeleton, "avoid a partial quaternion.");
    assert_eq!(got[0].members, vec!["ex:A".to_owned(), "ex:B".to_owned()]);
}

#[test]
fn collisions_same_key_twice_is_not_a_collision() {
    // One term carrying the same value twice is not a cross-term near-duplicate.
    let items = vec![
        ("ex:A".to_owned(), skeleton("same text.")),
        ("ex:A".to_owned(), skeleton("same text.")),
    ];
    assert!(collisions(items).is_empty());
}

#[test]
fn distinctiveness_passes_twins_flags_collapsed_distinction() {
    // Two twin sources (identical msgid skeleton) sharing one translation → PASS.
    let twins = vec![
        (
            skeleton("p-value"),
            skeleton("p值"),
            "math:PValue|rdfs:label".to_owned(),
        ),
        (
            skeleton("p-value"),
            skeleton("p值"),
            "math:pValue|rdfs:label".to_owned(),
        ),
    ];
    assert!(
        distinctiveness_violations(twins).is_empty(),
        "identical source → shared translation is legitimate"
    );
    // Two DISTINCT sources collapsed to one translation → FLAG.
    let collapsed = vec![
        (
            skeleton("read"),
            skeleton("lire"),
            "rights:read|rdfs:label".to_owned(),
        ),
        (
            skeleton("play"),
            skeleton("lire"),
            "rights:play|rdfs:label".to_owned(),
        ),
    ];
    let got = distinctiveness_violations(collapsed);
    assert_eq!(got.len(), 1, "the collapsed distinction reds: {got:#?}");
    assert_eq!(got[0].skeleton, "lire");
    assert_eq!(
        got[0].members,
        vec![
            "rights:play|rdfs:label".to_owned(),
            "rights:read|rdfs:label".to_owned()
        ]
    );
}

#[test]
fn distinctiveness_skips_empty_target() {
    let empties = vec![
        (skeleton("read"), String::new(), "a".to_owned()),
        (skeleton("play"), "   ".to_owned(), "b".to_owned()),
    ];
    assert!(distinctiveness_violations(empties).is_empty());
}
