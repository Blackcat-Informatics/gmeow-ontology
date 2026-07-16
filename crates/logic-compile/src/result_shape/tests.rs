// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_INT: &str = "http://www.w3.org/2001/XMLSchema#integer";

fn iri(var: &str) -> ObservedBinding {
    ObservedBinding::new(var, ObservedTerm::Iri)
}
fn lit(var: &str, dt: &str) -> ObservedBinding {
    ObservedBinding::new(
        var,
        ObservedTerm::Literal {
            datatype: dt.to_owned(),
        },
    )
}
fn triple(var: &str) -> ObservedBinding {
    ObservedBinding::new(var, ObservedTerm::TripleTerm)
}

#[test]
fn wire_and_local_roundtrip_for_every_enum() {
    for &k in TermKind::ALL {
        assert_eq!(TermKind::from_wire(k.wire()), Some(k));
        assert_eq!(TermKind::from_local(k.local_name()), Some(k));
    }
    for &b in ColumnBinding::ALL {
        assert_eq!(ColumnBinding::from_wire(b.wire()), Some(b));
        assert_eq!(ColumnBinding::from_local(b.local_name()), Some(b));
    }
    // RowCardinality has no `from_*` (the count payload makes it asymmetric); the
    // names are still pinned for the cross-check.
    assert_eq!(RowCardinality::Exact.local_name(), "RowsExact");
    assert_eq!(RowCardinality::Contains.local_name(), "RowsContains");
    assert_eq!(RowCardinality::Count(7).local_name(), "RowsCount");
    assert_eq!(RowCardinality::Count(7).count(), Some(7));
}

#[test]
fn conforming_rows_validate() {
    let shape = ResultShape::new(
        vec![
            ResultColumn::required("agent", ColumnKind::Iri),
            ResultColumn::required(
                "kind",
                ColumnKind::Literal {
                    datatype: Some(XSD_STRING.to_owned()),
                },
            ),
        ],
        RowCardinality::Contains,
    );
    let rows = vec![vec![iri("agent"), lit("kind", XSD_STRING)]];
    assert!(shape.validate_bindings(&rows).is_ok());
}

#[test]
fn term_kind_mismatch_is_hard_fail() {
    let shape = ResultShape::new(
        vec![ResultColumn::required("agent", ColumnKind::Iri)],
        RowCardinality::Contains,
    );
    // `agent` declared IRI, bound a literal.
    let rows = vec![vec![lit("agent", XSD_STRING)]];
    assert_eq!(
        shape.validate_bindings(&rows),
        Err(ContractViolation::TermKindMismatch {
            var: "agent".to_owned(),
            expected: TermKind::Iri,
            found: TermKind::Literal,
        })
    );
}

#[test]
fn datatype_mismatch_is_hard_fail() {
    let shape = ResultShape::new(
        vec![ResultColumn::required(
            "n",
            ColumnKind::Literal {
                datatype: Some(XSD_INT.to_owned()),
            },
        )],
        RowCardinality::Contains,
    );
    let rows = vec![vec![lit("n", XSD_STRING)]];
    assert_eq!(
        shape.validate_bindings(&rows),
        Err(ContractViolation::DatatypeMismatch {
            var: "n".to_owned(),
            expected: XSD_INT.to_owned(),
            found: XSD_STRING.to_owned(),
        })
    );
}

#[test]
fn any_literal_column_accepts_any_datatype() {
    let shape = ResultShape::new(
        vec![ResultColumn::required(
            "v",
            ColumnKind::Literal { datatype: None },
        )],
        RowCardinality::Contains,
    );
    assert!(shape.validate_bindings(&[vec![lit("v", XSD_INT)]]).is_ok());
    assert!(
        shape
            .validate_bindings(&[vec![lit("v", XSD_STRING)]])
            .is_ok()
    );
}

#[test]
fn rdf12_triple_term_is_a_first_class_closed_kind() {
    let shape = ResultShape::new(
        vec![ResultColumn::required("statement", ColumnKind::TripleTerm)],
        RowCardinality::Contains,
    );
    assert!(
        shape
            .validate_bindings(&[vec![triple("statement")]])
            .is_ok()
    );
    let inferred = ResultShape::from_observed(&[vec![triple("statement")]]);
    assert_eq!(
        inferred.column("statement").map(|column| &column.kind),
        Some(&ColumnKind::TripleTerm)
    );
    assert_eq!(
        shape.validate_bindings(&[vec![iri("statement")]]),
        Err(ContractViolation::TermKindMismatch {
            var: "statement".to_owned(),
            expected: TermKind::TripleTerm,
            found: TermKind::Iri,
        })
    );
}

#[test]
fn missing_required_column_is_hard_fail() {
    let shape = ResultShape::new(
        vec![
            ResultColumn::required("a", ColumnKind::Iri),
            ResultColumn::required("b", ColumnKind::Iri),
        ],
        RowCardinality::Contains,
    );
    let rows = vec![vec![iri("a")]]; // ?b missing
    assert_eq!(
        shape.validate_bindings(&rows),
        Err(ContractViolation::MissingRequired {
            var: "b".to_owned()
        })
    );
}

#[test]
fn optional_column_may_be_absent() {
    let shape = ResultShape::new(
        vec![
            ResultColumn::required("a", ColumnKind::Iri),
            ResultColumn {
                var: "b".to_owned(),
                kind: ColumnKind::Iri,
                binding: ColumnBinding::Optional,
            },
        ],
        RowCardinality::Contains,
    );
    assert!(shape.validate_bindings(&[vec![iri("a")]]).is_ok());
}

#[test]
fn undeclared_column_is_hard_fail() {
    let shape = ResultShape::new(
        vec![ResultColumn::required("a", ColumnKind::Iri)],
        RowCardinality::Contains,
    );
    let rows = vec![vec![iri("a"), iri("rogue")]];
    assert_eq!(
        shape.validate_bindings(&rows),
        Err(ContractViolation::UndeclaredColumn {
            var: "rogue".to_owned()
        })
    );
}

#[test]
fn count_mode_pins_row_count() {
    let shape = ResultShape::new(
        vec![ResultColumn::required("a", ColumnKind::Iri)],
        RowCardinality::Count(2),
    );
    assert!(
        shape
            .validate_bindings(&[vec![iri("a")], vec![iri("a")]])
            .is_ok()
    );
    assert_eq!(
        shape.validate_bindings(&[vec![iri("a")]]),
        Err(ContractViolation::RowCount {
            expected: 2,
            found: 1
        })
    );
}

#[test]
fn satisfiable_when_producer_covers_required_columns() {
    let consumer = ResultShape::new(
        vec![
            ResultColumn::required("agent", ColumnKind::Iri),
            ResultColumn::required(
                "kind",
                ColumnKind::Literal {
                    datatype: Some(XSD_STRING.to_owned()),
                },
            ),
        ],
        RowCardinality::Contains,
    );
    let producer = ResultShape::from_observed(&[vec![iri("agent"), lit("kind", XSD_STRING)]]);
    assert!(consumer.is_satisfiable_by(&producer).is_ok());
}

#[test]
fn unsatisfiable_when_producer_missing_required_column() {
    let consumer = ResultShape::new(
        vec![ResultColumn::required("agent", ColumnKind::Iri)],
        RowCardinality::Contains,
    );
    let producer = ResultShape::from_observed(&[vec![iri("other")]]);
    assert_eq!(
        consumer.is_satisfiable_by(&producer),
        Err(Mismatch::MissingColumn {
            var: "agent".to_owned()
        })
    );
}

#[test]
fn unsatisfiable_on_kind_and_datatype_incompatibility() {
    let consumer = ResultShape::new(
        vec![ResultColumn::required("v", ColumnKind::Iri)],
        RowCardinality::Contains,
    );
    let producer = ResultShape::from_observed(&[vec![lit("v", XSD_STRING)]]);
    assert_eq!(
        consumer.is_satisfiable_by(&producer),
        Err(Mismatch::IncompatibleKind {
            var: "v".to_owned(),
            required: TermKind::Iri,
            provided: TermKind::Literal,
        })
    );

    let consumer = ResultShape::new(
        vec![ResultColumn::required(
            "v",
            ColumnKind::Literal {
                datatype: Some(XSD_INT.to_owned()),
            },
        )],
        RowCardinality::Contains,
    );
    let producer = ResultShape::from_observed(&[vec![lit("v", XSD_STRING)]]);
    assert_eq!(
        consumer.is_satisfiable_by(&producer),
        Err(Mismatch::IncompatibleDatatype {
            var: "v".to_owned(),
            required: XSD_INT.to_owned(),
            provided: Some(XSD_STRING.to_owned()),
        })
    );
}

#[test]
fn from_observed_marks_sometimes_absent_columns_optional() {
    let shape = ResultShape::from_observed(&[vec![iri("a"), iri("b")], vec![iri("a")]]);
    assert_eq!(shape.column("a").unwrap().binding, ColumnBinding::Required);
    assert_eq!(shape.column("b").unwrap().binding, ColumnBinding::Optional);
}
