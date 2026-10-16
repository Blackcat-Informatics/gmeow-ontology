// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::frontend::{AdmittedSemanticUnit, SemanticUnitDisposition, SourceClassDefinition};
use crate::ir::{Formula, Term};

/// Exercise the public source-admission/projection path with one object bound.
fn bound_source(constraints: &str) -> String {
    format!(
        "@prefix logic: <https://blackcatinformatics.ca/logic/> . \
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> . \
        @prefix ex: <https://example.org/> . \
        ex:Bound logic:onProperty ex:p ; {constraints} ."
    )
}

/// Inspect the actual admitted class law, including its source root and scope.
fn condition(admission: &SourceAdmission) -> (&SourceClassDefinition, &Formula) {
    let definition = admission
        .admitted()
        .find_map(|unit| {
            if let SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::ClassDefinition(
                definition,
            )) = &unit.disposition
            {
                Some(definition)
            } else {
                None
            }
        })
        .expect("certified bound law");
    let Formula::Forall { vars, body } = &definition.formula else {
        panic!("universally scoped class law")
    };
    assert_eq!(vars, &["class_member"]);
    let Formula::Iff(member, condition) = body.as_ref() else {
        panic!("class membership is defined by equivalence")
    };
    let Formula::Atom { relation, args } = member.as_ref() else {
        panic!("class membership")
    };
    assert_eq!(
        relation,
        &Term::Iri("https://blackcatinformatics.ca/logic/instanceOf".into())
    );
    assert_eq!(args[0], Term::Var("class_member".into()));
    let Term::Var(root) = &args[1] else {
        panic!("source-bound class")
    };
    assert_eq!(
        definition.constants[root].as_iri(),
        Some("https://example.org/Bound")
    );
    (definition, condition)
}

/// Every witness must satisfy both the property and qualifier, and every pair
/// must be denotationally unequal. Source-name distinctness is insufficient.
fn minimum_law(definition: &SourceClassDefinition, formula: &Formula, count: usize) {
    if count == 0 {
        let Formula::Atom { relation, args } = formula else {
            panic!("zero minimum is true")
        };
        assert_eq!(
            relation,
            &Term::Iri("https://blackcatinformatics.ca/logic/sameAs".into())
        );
        assert_eq!(
            args,
            &vec![
                Term::Var("class_member".into()),
                Term::Var("class_member".into())
            ]
        );
        return;
    }
    let Formula::Exists { vars, body } = formula else {
        panic!("minimum needs witnesses")
    };
    assert_eq!(vars.len(), count);
    let names: BTreeSet<_> = vars.iter().cloned().collect();
    assert_eq!(names.len(), count);
    assert!(!names.contains("class_member"));
    let Formula::And(conditions) = body.as_ref() else {
        panic!("witnesses require all conditions")
    };
    let mut properties = BTreeSet::new();
    let mut qualifiers = BTreeSet::new();
    let mut unequal = BTreeSet::new();
    for condition in conditions {
        match condition {
            Formula::Atom {
                relation: Term::Iri(relation),
                args,
            } if relation == "https://example.org/p" => {
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], Term::Var("class_member".into()));
                let Term::Var(value) = &args[1] else {
                    panic!("property witness")
                };
                properties.insert(value.clone());
            }
            Formula::Atom {
                relation: Term::Iri(relation),
                args,
            } if relation == "https://blackcatinformatics.ca/logic/instanceOf" => {
                assert_eq!(args.len(), 2);
                let (Term::Var(value), Term::Var(class)) = (&args[0], &args[1]) else {
                    panic!("qualified witness")
                };
                assert_eq!(
                    definition.constants[class].as_iri(),
                    Some("https://example.org/Part")
                );
                qualifiers.insert(value.clone());
            }
            Formula::Not(equality) => {
                let Formula::Atom { relation, args } = equality.as_ref() else {
                    panic!("logical inequality")
                };
                assert_eq!(
                    relation,
                    &Term::Iri("https://blackcatinformatics.ca/logic/sameAs".into())
                );
                assert_eq!(args.len(), 2);
                let (Term::Var(left), Term::Var(right)) = (&args[0], &args[1]) else {
                    panic!("witness pair")
                };
                assert_ne!(left, right);
                assert!(names.contains(left) && names.contains(right));
                unequal.insert(BTreeSet::from([left.clone(), right.clone()]));
            }
            _ => panic!("unexpected count condition: {condition:?}"),
        }
    }
    assert_eq!(properties, names);
    assert_eq!(qualifiers, names);
    assert_eq!(unequal.len(), count * (count - 1) / 2);
    assert_eq!(conditions.len(), count * 2 + unequal.len());
}

/// Lower, upper and exact zero/nonzero bounds retain their actual quantifiers.
#[test]
fn qualified_object_bounds_preserve_witnesses_qualification_and_logical_inequality() {
    for count in [0, 2] {
        for field in [
            "minQualifiedCardinality",
            "maxQualifiedCardinality",
            "qualifiedCardinality",
        ] {
            let (admission, projection) = project(&bound_source(&format!(
                "logic:{field} {count} ; logic:onClass ex:Part"
            )));
            assert_eq!(
                projection.status,
                FofProjectionStatus::Complete,
                "{projection:?}"
            );
            let (definition, formula) = condition(&admission);
            match field {
                "minQualifiedCardinality" => minimum_law(definition, formula, count),
                "maxQualifiedCardinality" => {
                    let Formula::Not(excess) = formula else {
                        panic!("upper bound excludes an excessive witness set")
                    };
                    minimum_law(definition, excess, count + 1);
                }
                _ => {
                    let Formula::And(bounds) = formula else {
                        panic!("exact bound retains lower and upper bounds")
                    };
                    assert_eq!(bounds.len(), 2);
                    minimum_law(definition, &bounds[0], count);
                    let Formula::Not(excess) = &bounds[1] else {
                        panic!("exact upper bound")
                    };
                    minimum_law(definition, excess, count + 1);
                }
            }
        }
    }
}

/// Each independently authored bound survives; numerically equal count lexicals
/// retain their distinct source identity while describing one semantic bound.
#[test]
fn qualified_bounds_are_conjunctive_and_keep_count_lexical_evidence() {
    let (admission, projection) = project(&bound_source(
        "logic:minQualifiedCardinality 1, 2 ; logic:maxQualifiedCardinality 3 ; logic:onClass ex:Part",
    ));
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{projection:?}"
    );
    let (definition, formula) = condition(&admission);
    let Formula::And(bounds) = formula else {
        panic!("independent bounds are conjunctive")
    };
    assert_eq!(bounds.len(), 3);
    minimum_law(definition, &bounds[0], 1);
    minimum_law(definition, &bounds[1], 2);
    let Formula::Not(excess) = &bounds[2] else {
        panic!("upper bound")
    };
    minimum_law(definition, excess, 4);

    let aliases = bound_source(
        "logic:minQualifiedCardinality 2, \"02\"^^xsd:integer ; logic:onClass ex:Part",
    );
    let theory = compile(&aliases);
    assert_eq!(
        theory
            .program()
            .axioms
            .iter()
            .filter(|axiom| axiom.predicate
                == "https://blackcatinformatics.ca/logic/minQualifiedCardinality")
            .count(),
        2
    );
    let alias_admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    assert_eq!(
        project_tptp_fof(&theory, &alias_admission).status,
        FofProjectionStatus::Complete
    );
    let (single, _) = project(&bound_source(
        "logic:minQualifiedCardinality 2 ; logic:onClass ex:Part",
    ));
    assert_ne!(single.source_digest, alias_admission.source_digest);
    assert_eq!(
        condition(&single).0.formula,
        condition(&alias_admission).0.formula
    );
}

/// Counts use native integer interpretation and the existing lexical-count
/// convention; malformed or unrepresentable values never weaken a constraint.
#[test]
fn qualified_count_value_admission_fails_closed() {
    for count in ["2", "\"2\"", "\"02\"^^xsd:unsignedByte"] {
        let (admission, projection) = project(&bound_source(&format!(
            "logic:minQualifiedCardinality {count} ; logic:onClass ex:Part"
        )));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Complete,
            "{count}: {projection:?}"
        );
        let (definition, formula) = condition(&admission);
        minimum_law(definition, formula, 2);
    }
    for count in [
        "-1",
        "1.5",
        "\"1.0\"^^xsd:integer",
        "\"1\"@en",
        "true",
        "ex:notACount",
        "\"bad\"",
    ] {
        let (_, projection) = project(&bound_source(&format!(
            "logic:minQualifiedCardinality {count} ; logic:onClass ex:Part"
        )));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Blocked,
            "{count}: {projection:?}"
        );
        assert!(projection.problem.is_none());
        assert!(
            projection
                .blockers
                .iter()
                .any(|row| row.code == "MALFORMED_CLASS_CARDINALITY")
        );
    }
    let (_, projection) = project(&bound_source(&format!(
        "logic:minQualifiedCardinality \"{}\"^^xsd:nonNegativeInteger ; logic:onClass ex:Part",
        u128::MAX
    )));
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(
        projection
            .blockers
            .iter()
            .any(|row| row.code == "CLASS_CARDINALITY_NOT_REPRESENTABLE")
    );
}

/// A qualifier is mandatory, and data ranges cannot be relabelled object classes.
#[test]
fn qualified_count_roles_cannot_be_erased_or_invented() {
    for (constraints, code) in [
        ("logic:minQualifiedCardinality 2", "MISSING_CLASS_QUALIFIER"),
        ("logic:onClass ex:Part", "ORPHAN_CLASS_QUALIFIER"),
        (
            "logic:minQualifiedCardinality 0 ; logic:onClass \"not a class\"",
            "UNSUPPORTED_CLASS_EXPRESSION",
        ),
        (
            "logic:minQualifiedCardinality 2 ; logic:onClass xsd:integer",
            "UNSUPPORTED_CLASS_EXPRESSION",
        ),
        (
            "logic:minQualifiedCardinality 2 ; logic:onDataRange xsd:integer",
            "UNSUPPORTED_CLASS_EXPRESSION",
        ),
    ] {
        let (_, projection) = project(&bound_source(constraints));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Blocked,
            "{projection:?}"
        );
        assert!(projection.problem.is_none());
        assert!(
            projection.blockers.iter().any(|row| row.code == code),
            "{projection:?}"
        );
    }
}

/// A source class used as a qualifier must retain its selected definition.
#[test]
fn qualified_count_dependencies_require_the_selected_definition() {
    let theory = compile_documents(&[
        (
            "bound.ttl",
            &bound_source("logic:minQualifiedCardinality 2 ; logic:onClass ex:Part"),
            None,
        ),
        (
            "part.ttl",
            "@prefix logic: <https://blackcatinformatics.ca/logic/> . <https://example.org/Part> logic:oneOf (<https://example.org/a> <https://example.org/b>) .",
            None,
        ),
    ]);
    let all =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    assert_eq!(
        project_tptp_fof(&theory, &all).status,
        FofProjectionStatus::Complete
    );
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.documents = DocumentSelection::Exact(BTreeSet::from(["bound.ttl".into()]));
    let selected = SourceAdmission::classical_fof(&theory, selection);
    let projection = project_tptp_fof(&theory, &selected);
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(
        projection
            .blockers
            .iter()
            .any(|row| row.code == "UNADMITTED_CLASS_DEPENDENCY")
    );
}
