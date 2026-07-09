// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Slice-test-harness diagnostic kinds.
//!
//! Every genuine failure surface in the harness — a Turtle source that will not
//! read or parse, a SPARQL query that will not evaluate or returns the wrong form,
//! a merged/closed graph that cannot be built, a test-DSL spec cell that names an
//! unrecognized controlled-vocabulary value or is missing a required field, a
//! `logic:ResultShape` that will not parse, a typed binding that is malformed, a
//! competency/structural/example-conformance cell that fails its expectation, or a
//! per-file aggregate of failing cells — is a HARD fail (no-optionality). Each is a
//! [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind) under the `slicetest.*`
//! code namespace, so the harness reports on the shared substrate rather than a bare
//! string. Message text that varies per site rides in a free-form `detail: String`
//! field carrying the full formatted string, so the head message (the text the
//! per-file aggregator renders) is preserved verbatim.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// A Turtle source (a spec file, module, example, or fixture) could not be read
    /// off disk or parsed through the native codec.
    pub struct DatasetRead { detail: String }
    code = "slicetest.dataset.read";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A SPARQL query failed to parse or evaluate over its dataset (an introspection
    /// query, a competency/structural query, a projection CONSTRUCT, or an RDFS rule).
    pub struct SparqlEval { detail: String }
    code = "slicetest.sparql.eval";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A SPARQL query returned the wrong result form for its call site (e.g. a
    /// non-SELECT where SELECT is required, or a non-CONSTRUCT RDFS rule).
    pub struct UnexpectedResultForm { detail: String }
    code = "slicetest.sparql.unexpected-form";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// Building the merged ontology dataset failed: a required source is missing, a
    /// directory could not be enumerated, or a source file failed to read/parse.
    pub struct MergedGraph { detail: String }
    code = "slicetest.store.merged-graph";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The RDFS closure could not be computed (a rule error, or the fixpoint safety
    /// bound was exceeded — a signal of a bug, not slow data).
    pub struct RdfsClosure { detail: String }
    code = "slicetest.store.rdfs-closure";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The native `logic:`-reasoned closure could not be built: the algebra-law
    /// sources would not load, the program would not compile cleanly, the graph
    /// could not be re-scoped into the reasoner world, or the reasoner failed.
    pub struct LogicReasoning { detail: String }
    code = "slicetest.store.logic-reasoning";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A `tests/*.ttl` spec file could not be loaded into a native dataset for
    /// introspection.
    pub struct SpecLoad { detail: String }
    code = "slicetest.spec.load";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A test-DSL spec cell is malformed: an unrecognized controlled-vocabulary value,
    /// a missing required field, a conflicting duplicate definition, or a binding that
    /// names both/neither of a mutually-exclusive pair.
    pub struct SpecCell { detail: String }
    code = "slicetest.spec.cell";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A `logic:ResultShape` individual could not be introspected into the canonical
    /// type: an empty shape, a column missing a required field, an unknown term-kind /
    /// binding / cardinality value, or a `logic:RowsCount` missing its row count.
    pub struct ResultShapeParse { detail: String }
    code = "slicetest.spec.result-shape";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A bound introspection term was not the kind the DSL requires: an expected IRI
    /// slot that bound something else, or a malformed `xsd:boolean` / non-negative
    /// integer literal that must never silently coerce.
    pub struct TypedBinding { detail: String }
    code = "slicetest.spec.typed-binding";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A competency question's query could not be resolved: an unreadable
    /// `gmeow:cqQueryFile`, or a question naming both/neither of `cqQuery`/`cqQueryFile`.
    pub struct QueryLoad { detail: String }
    code = "slicetest.exec.query-load";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A competency question's result did not match its expectation: an ASK/row-count
    /// mismatch, missing/extra enumerated rows, a result-shape or composition contract
    /// violation, or a lane/form misuse.
    pub struct CompetencyMismatch { detail: String }
    code = "slicetest.exec.competency";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A structural assertion cell failed: an unsupported/misconfigured pattern, or a
    /// polarity the ASK result contradicts.
    pub struct StructuralCell { detail: String }
    code = "slicetest.exec.structural";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An example-conformance cell failed: an unexpected conformance/violation outcome,
    /// or a `violates` cell missing its expected code.
    pub struct ConformanceCell { detail: String }
    code = "slicetest.exec.conformance";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The native SHACL surface failed while validating an example-conformance cell:
    /// the slice shapes would not parse, or `validate_dataset` errored.
    pub struct ShapeValidation { detail: String }
    code = "slicetest.exec.shape-validation";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// Enumerating a slice's `examples/` directory failed for a reason other than the
    /// directory being absent (a permissions / I/O error that must not masquerade as
    /// "no examples").
    pub struct ExampleDiscovery { detail: String }
    code = "slicetest.exec.example-discovery";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// One or more cells failed in a spec file; `detail` is the per-file aggregate
    /// report naming each failing cell by its IRI.
    pub struct CellAggregate { detail: String }
    code = "slicetest.exec.aggregate";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

/// The complete slice-test diagnostic-code catalog, in registration order.
pub const SLICETEST_DIAG_CODES: &[&str] = &[
    DatasetRead::CODE,
    SparqlEval::CODE,
    UnexpectedResultForm::CODE,
    MergedGraph::CODE,
    RdfsClosure::CODE,
    LogicReasoning::CODE,
    SpecLoad::CODE,
    SpecCell::CODE,
    ResultShapeParse::CODE,
    TypedBinding::CODE,
    QueryLoad::CODE,
    CompetencyMismatch::CODE,
    StructuralCell::CODE,
    ConformanceCell::CODE,
    ShapeValidation::CODE,
    ExampleDiscovery::CODE,
    CellAggregate::CODE,
];

/// Eagerly intern every slice-test diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        DatasetRead::register(),
        SparqlEval::register(),
        UnexpectedResultForm::register(),
        MergedGraph::register(),
        RdfsClosure::register(),
        LogicReasoning::register(),
        SpecLoad::register(),
        SpecCell::register(),
        ResultShapeParse::register(),
        TypedBinding::register(),
        QueryLoad::register(),
        CompetencyMismatch::register(),
        StructuralCell::register(),
        ConformanceCell::register(),
        ShapeValidation::register(),
        ExampleDiscovery::register(),
        CellAggregate::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_slicetest_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            SLICETEST_DIAG_CODES.len(),
            "register_all() and SLICETEST_DIAG_CODES must enumerate the same kinds"
        );
        for code in SLICETEST_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "slicetest code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = SLICETEST_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            SLICETEST_DIAG_CODES.len(),
            "duplicate slicetest diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
