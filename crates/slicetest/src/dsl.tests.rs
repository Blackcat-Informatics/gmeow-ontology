// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::paths;

fn epistemics_tests_dir() -> std::path::PathBuf {
    paths::slices_root().join("core/epistemics/tests")
}

#[test]
fn parses_competency_exemplars() {
    let spec = load_spec(&epistemics_tests_dir().join("competency.ttl"))
        .expect("competency.ttl must parse");
    assert_eq!(spec.competency.len(), 2, "two competency questions");

    let agents = spec
        .competency
        .iter()
        .find(|c| c.iri.ends_with("cqAgentKinds"))
        .expect("cqAgentKinds present");
    assert_eq!(
        agents.query_file.as_deref(),
        Some("queries/competency/agents.rq")
    );
    assert!(agents.exact_rows);
    assert_eq!(
        agents.expected_rows.len(),
        8,
        "eight fully-enumerated agent kinds: the six rigid Kinds (Agent, Builder, \
             Organization, Person, Sensor, SoftwareAgent) plus the inhabitation-slice \
             role-mixins DigitalSubject and Inhabitant, both grounded rdfs:subClassOf gmeow:Agent"
    );
    // Each row has exactly one cell binding ?agentKind to a gmeow: IRI.
    for row in &agents.expected_rows {
        assert_eq!(row.cells.len(), 1);
        assert_eq!(row.cells[0].var, "agentKind");
        assert!(matches!(row.cells[0].value, TermValue::Iri(_)));
    }

    let roles = spec
        .competency
        .iter()
        .find(|c| c.iri.ends_with("cqContributionRoles"))
        .expect("cqContributionRoles present");
    // T3 finished the enumeration: the coarse row-count escape hatch is gone,
    // every one of the 48 roles is pinned, and cqExactRows is set.
    assert_eq!(roles.expect_row_count, None);
    assert!(roles.exact_rows);
    assert_eq!(roles.expected_rows.len(), 48, "all 48 roles enumerated");
}

#[test]
fn parses_structural_exemplars() {
    let spec = load_spec(&epistemics_tests_dir().join("structural.ttl"))
        .expect("structural.ttl must parse");
    // The T3 migration expanded the two T2 keystones to the full set of
    // module invariants lifted from test_epistemics.py.
    assert!(
        spec.structural.len() >= 13,
        "the migrated structural invariants are present, got {}",
        spec.structural.len()
    );

    let must = spec
        .structural
        .iter()
        .find(|s| s.iri.ends_with("saKnowsThatSubPropertyOfBelieves"))
        .expect("keystone entailment present");
    assert_eq!(must.polarity, Polarity::Must);
    assert_eq!(must.scope, Scope::Module);
    assert!(must.pattern.as_deref().unwrap().contains("subPropertyOf"));

    let must_not = spec
        .structural
        .iter()
        .find(|s| s.iri.ends_with("saNoTruthBit"))
        .expect("no-truth-bit present");
    assert_eq!(must_not.polarity, Polarity::MustNot);

    // A migrated MUST-NOT over a VALUES set (the open-range invariant) parses.
    let open_range = spec
        .structural
        .iter()
        .find(|s| s.iri.ends_with("saSpineOpenRange"))
        .expect("migrated spine-open-range present");
    assert_eq!(open_range.polarity, Polarity::MustNot);
}

#[test]
fn parses_conformance_exemplars() {
    let spec = load_spec(&epistemics_tests_dir().join("example-conformance.ttl"))
        .expect("example-conformance.ttl must parse");
    assert_eq!(spec.conformance.len(), 2);

    let conforms = spec
        .conformance
        .iter()
        .find(|c| c.outcome == Outcome::Conforms)
        .expect("conforming fixture present");
    assert_eq!(conforms.file, "examples/justification-and-defeat.ttl");
    assert!(conforms.violation_code.is_none());

    let violates = spec
        .conformance
        .iter()
        .find(|c| c.outcome == Outcome::Violates)
        .expect("violating fixture present");
    assert_eq!(
        violates.violation_code.as_deref(),
        Some("shacl.MinCountConstraintComponent")
    );
}

/// Load inline Turtle into a native dataset via the canonical codec:
/// `parse_dataset` into the frozen IR — the same codec the rest of the stack uses
/// (and lenient on long private-use language tags, like the harness).
fn store_from_turtle(ttl: &str) -> Arc<RdfDataset> {
    native_query::dataset_from_turtle(ttl).expect("valid turtle")
}

#[test]
fn rejects_conflicting_duplicate_competency_questions() {
    // Two cqExpectRowCount values for the SAME question yield two ?cq solutions
    // with conflicting scalar fields. Silently keeping whichever the SPARQL
    // engine returned last would make the spec solution-order-dependent.
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cqDup a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqExpectRowCount 1, 2 .\n";
    let err = parse_competency(&store_from_turtle(ttl))
        .expect_err("conflicting duplicate must hard-fail");
    assert!(
        err.message().contains("conflicting duplicate"),
        "unexpected error: {err}"
    );
}

#[test]
fn identical_duplicate_competency_solutions_are_harmless() {
    // A repeated identical scalar (here a duplicated cqQueryFile triple) is not a
    // conflict — it must parse, collapsing to one question.
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cqSame a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\", \"q.rq\" .\n";
    let spec = parse_competency(&store_from_turtle(ttl)).expect("identical repeat is fine");
    assert_eq!(spec.len(), 1);
}

#[test]
fn rejects_malformed_boolean_literal() {
    // A typoed boolean must hard-fail, never silently coerce to false (which would
    // quietly flip cqExactRows from exact-match to subset).
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cqBad a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqExactRows \"ture\" .\n"; // codespell:ignore ture
    let err =
        parse_competency(&store_from_turtle(ttl)).expect_err("malformed boolean must hard-fail");
    assert!(
        err.message().contains("xsd:boolean"),
        "unexpected error: {err}"
    );
}

#[test]
fn parses_cq_result_shape_into_the_canonical_type() {
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn [ logic:columnVariable \"agent\" ; logic:columnTermKind logic:TermKindIri ; logic:columnBinding logic:BindingRequired ] ,\n\
                                     [ logic:columnVariable \"name\" ; logic:columnTermKind logic:TermKindLiteral ; logic:columnDatatype <http://www.w3.org/2001/XMLSchema#string> ; logic:columnBinding logic:BindingOptional ] ;\n\
                logic:shapeCardinality logic:RowsExact .\n";
    let spec = parse_competency(&store_from_turtle(ttl)).expect("shape parses");
    let cq = &spec[0];
    let shape = cq.result_shape.as_ref().expect("result_shape present");
    assert_eq!(shape.cardinality, RowCardinality::Exact);
    assert_eq!(shape.columns.len(), 2);
    // columns are canonicalised sorted-by-var: agent, name
    assert_eq!(shape.columns[0].var, "agent");
    assert_eq!(shape.columns[0].kind, ColumnKind::Iri);
    assert_eq!(shape.columns[0].binding, ColumnBinding::Required);
    assert_eq!(shape.columns[1].var, "name");
    assert_eq!(
        shape.columns[1].kind,
        ColumnKind::Literal {
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".to_owned())
        }
    );
    assert_eq!(shape.columns[1].binding, ColumnBinding::Optional);
}

#[test]
fn rejects_result_shape_with_unknown_term_kind() {
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn [ logic:columnVariable \"x\" ; logic:columnTermKind logic:TermKindBogus ; logic:columnBinding logic:BindingRequired ] ;\n\
                logic:shapeCardinality logic:RowsContains .\n";
    let err =
        parse_competency(&store_from_turtle(ttl)).expect_err("unknown term-kind must hard-fail");
    assert!(
        err.message().contains("columnTermKind"),
        "unexpected error: {err}"
    );
}

// ── ResultShape hard-fail discipline ─────────────────────────────
//
// A `logic:declaresColumn` node MISSING any of the three required fields
// (`columnVariable`, `columnTermKind`, `columnBinding`) must HARD-FAIL with
// a precise per-field error that names the missing predicate and the column
// node. The SPARQL query reads each required field independently, so a
// missing field reaches the explicit validation below instead of silently
// dropping the column's solution row and shrinking the contract.

#[test]
fn rejects_result_shape_column_missing_term_kind() {
    // columnTermKind is absent; the column must not be silently dropped.
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn [ logic:columnVariable \"x\" ;\n\
                                       logic:columnBinding logic:BindingRequired ] ;\n\
                logic:shapeCardinality logic:RowsExact .\n";
    let err = parse_competency(&store_from_turtle(ttl))
        .expect_err("missing columnTermKind must hard-fail");
    assert!(
        err.message().contains("columnTermKind"),
        "error must name the missing predicate; got: {err}"
    );
}

#[test]
fn rejects_result_shape_column_missing_column_variable() {
    // columnVariable is absent; the column must not be silently dropped.
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn [ logic:columnTermKind logic:TermKindIri ;\n\
                                       logic:columnBinding logic:BindingRequired ] ;\n\
                logic:shapeCardinality logic:RowsExact .\n";
    let err = parse_competency(&store_from_turtle(ttl))
        .expect_err("missing columnVariable must hard-fail");
    assert!(
        err.message().contains("columnVariable"),
        "error must name the missing predicate; got: {err}"
    );
}

#[test]
fn rejects_result_shape_column_missing_column_binding() {
    // columnBinding is absent; the column must not be silently dropped.
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn [ logic:columnVariable \"x\" ;\n\
                                       logic:columnTermKind logic:TermKindIri ] ;\n\
                logic:shapeCardinality logic:RowsExact .\n";
    let err = parse_competency(&store_from_turtle(ttl))
        .expect_err("missing columnBinding must hard-fail");
    assert!(
        err.message().contains("columnBinding"),
        "error must name the missing predicate; got: {err}"
    );
}

#[test]
fn well_formed_multi_column_shape_still_parses() {
    // A complete multi-column shape (all three required fields present on
    // every column) must parse to the expected canonical columns. Independent
    // field reads preserve both complete rows; this guards the positive path.
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn\n\
                    [ logic:columnVariable \"agent\" ;\n\
                      logic:columnTermKind logic:TermKindIri ;\n\
                      logic:columnBinding logic:BindingRequired ] ,\n\
                    [ logic:columnVariable \"score\" ;\n\
                      logic:columnTermKind logic:TermKindLiteral ;\n\
                      logic:columnBinding logic:BindingOptional ] ;\n\
                logic:shapeCardinality logic:RowsContains .\n";
    let spec = parse_competency(&store_from_turtle(ttl))
        .expect("well-formed multi-column shape must parse");
    let cq = &spec[0];
    let shape = cq.result_shape.as_ref().expect("result_shape present");
    assert_eq!(shape.cardinality, RowCardinality::Contains);
    // columns are canonically sorted by var: agent, score
    assert_eq!(shape.columns.len(), 2, "both columns present, none dropped");
    assert_eq!(shape.columns[0].var, "agent");
    assert_eq!(shape.columns[0].kind, ColumnKind::Iri);
    assert_eq!(shape.columns[0].binding, ColumnBinding::Required);
    assert_eq!(shape.columns[1].var, "score");
    assert_eq!(
        shape.columns[1].kind,
        ColumnKind::Literal { datatype: None }
    );
    assert_eq!(shape.columns[1].binding, ColumnBinding::Optional);
}

#[test]
fn rejects_expected_rows_for_unknown_competency_question() {
    // A gmeow:cqExpectRow whose ?cq matches no CompetencyQuestion is a spec
    // typo; the rows must not be silently dropped.
    let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cqReal a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" .\n\
            ex:cqTypo gmeow:cqExpectRow [ gmeow:rowCell [ gmeow:cellVar \"x\" ; gmeow:cellValueLiteral \"v\" ] ] .\n";
    let err =
        parse_competency(&store_from_turtle(ttl)).expect_err("orphan expected rows must hard-fail");
    assert!(
        err.message().contains("unknown competency question"),
        "unexpected error: {err}"
    );
}
