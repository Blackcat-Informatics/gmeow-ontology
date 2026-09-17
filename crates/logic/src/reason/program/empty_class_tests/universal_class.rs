// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny synthetic controls for universal-class source roles in the joint closure.

use super::*;

const INSTANCE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const THING: &str = "https://blackcatinformatics.ca/logic/Thing";
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const RULE: &str = "dl:universal-class-subclass";

#[test]
fn mixed_universal_class_spellings_preserve_the_actual_two_source_premises() {
    for member in [INSTANCE, TYPE] {
        for member_class in [THING, OWL_THING] {
            for source_class in [THING, OWL_THING] {
                for subclass in ["https://blackcatinformatics.ca/logic/subClassOf", SUBCLASS] {
                    let rows = [
                        fact(S, member, member_class),
                        fact(source_class, subclass, C),
                    ];
                    let result = run(&rows, Vec::new()).unwrap();
                    let derived = result
                        .inferred
                        .iter()
                        .find(|row| {
                            row.subject == S
                                && row.predicate == TYPE
                                && row.object.as_iri() == Some(C)
                                && row.rule_name.as_deref() == Some(RULE)
                        })
                        .expect("the universal-class rule joins the two exact source roles");
                    assert_eq!(
                        derived.premises.iter().cloned().collect::<BTreeSet<_>>(),
                        BTreeSet::from([
                            (S.to_owned(), member.to_owned(), format!("<{member_class}>")),
                            (
                                source_class.to_owned(),
                                subclass.to_owned(),
                                format!("<{C}>")
                            ),
                        ])
                    );
                    assert_eq!(derived.premises.len(), 2);
                    assert!(
                        result.witnesses.is_empty(),
                        "an existing member needs no invented domain witness"
                    );
                }
            }
        }
    }
}

#[test]
fn late_universal_subclasses_wake_positive_consumers_before_absence() {
    let rows = [fact(S, INSTANCE, THING), fact(OWL_THING, SEED, NOTHING)];
    let result = run(
        &rows,
        vec![
            (atom("?x", SEED, "?y"), atom("?x", SUBCLASS, "?y")),
            (
                Formula::And(vec![
                    atom("?x", INSTANCE, THING),
                    Formula::Not(Box::new(atom("?x", TYPE, NOTHING))),
                ]),
                atom("?x", ABSENT, NOTHING),
            ),
        ],
    )
    .unwrap();
    assert!(contains(&result, S, CLASH));
    assert!(!contains(&result, S, ABSENT));
    let proof = result
        .inferred
        .iter()
        .find(|row| {
            row.subject == S
                && row.rule_name.as_deref() == Some(RULE)
                && row.object.as_iri() == Some(NOTHING)
        })
        .unwrap();
    assert!(proof.premises.contains(&(
        OWL_THING.to_owned(),
        SUBCLASS.to_owned(),
        format!("<{NOTHING}>")
    )));
}

#[test]
fn equivalent_universal_classes_reach_members_in_both_source_orientations() {
    for canonical in [false, true] {
        for reversed in [false, true] {
            let source_class = if canonical { THING } else { OWL_THING };
            let member_class = if canonical { OWL_THING } else { THING };
            let equivalent = vocabulary(canonical, "equivalentClass");
            let relation = if reversed {
                fact(C, &equivalent, source_class)
            } else {
                fact(source_class, &equivalent, C)
            };
            let result = run(&[fact(S, INSTANCE, member_class), relation], Vec::new()).unwrap();
            assert!(result.inferred.iter().any(|row| row.subject == S
                && row.predicate == TYPE
                && row.object.as_iri() == Some(C)));
        }
    }
}

#[test]
fn universal_roles_do_not_merge_worlds_rewrite_data_or_match_foreign_markers() {
    let member = fact(S, INSTANCE, THING).in_graph(RdfTerm::iri("urn:universal:member-world"));
    let relation =
        fact(OWL_THING, SUBCLASS, NOTHING).in_graph(RdfTerm::iri("urn:universal:other-world"));
    let result = run(&[member, relation], Vec::new()).unwrap();
    assert!(!contains(&result, S, CLASH));
    for rows in [
        [fact(S, SEED, THING), fact(OWL_THING, SUBCLASS, NOTHING)],
        [
            fact(S, TYPE, "urn:foreign:Thing"),
            fact(OWL_THING, SUBCLASS, NOTHING),
        ],
        [
            fact(S, INSTANCE, THING),
            fact("urn:foreign:Thing", SUBCLASS, NOTHING),
        ],
    ] {
        let result = run(&rows, Vec::new()).unwrap();
        assert!(!contains(&result, S, CLASH));
        assert!(
            !result
                .inferred
                .iter()
                .any(|row| row.rule_name.as_deref() == Some(RULE))
        );
    }
}
