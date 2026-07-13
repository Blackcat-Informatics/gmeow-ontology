// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Logic-compiler diagnostic kinds.
//!
//! The logic compiler parses the RDF 1.2 carrier into the intermediate
//! representation, validates it, and projects it into the committed lossy
//! surfaces (CGIF, CLIF, XCL, SPARQL, SSSOM, EDOAL, FnO, and the textual
//! renderings) — each a HARD failure surface (no-optionality): a malformed
//! carrier node, an out-of-fragment construct, a projection that cannot be
//! emitted, or a round-trip that does not close must surface as a typed
//! diagnostic rather than a bare string. Each defect is a
//! [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind) under the
//! `logic-compile.*` code namespace, so the compiler reports on the shared
//! substrate.
//!
//! Every kind carries a single `detail` string that preserves the authored
//! condition text verbatim; discrimination is by code + grade, and the message
//! is the preserved detail. The area codes track the compiler's stage
//! boundaries so a downstream reader can key on where a defect arose.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

/// The grade every compiler diagnostic carries: a hard modeling-discipline
/// violation at the binding standpoint. The compiler admits no degraded
/// fallback, so each condition is an `Error`.
macro_rules! compile_grade {
    () => {
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        )
    };
}

define_diag_kind! {
    /// An intermediate-representation constructor rejected its arguments: an
    /// empty term name/IRI, a non-IRI relation that would break first-orderness,
    /// or any other well-formedness precondition on the AST nodes.
    pub struct Ir { detail: String }
    code = "logic-compile.ir";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A validation-shape constructor or the shape normalizer rejected its input:
    /// a malformed constraint, an empty required field, or a normalization that
    /// could not close.
    pub struct Validation { detail: String }
    code = "logic-compile.validation";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The CGIF dialect surface failed: a lex/parse error, a form that is not a
    /// well-formed conceptual graph, or a predication that could not be emitted.
    pub struct Cgif { detail: String }
    code = "logic-compile.cgif";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The CLIF dialect surface failed: a lex/parse error, a sentence that is not
    /// a well-formed first-order form, or a predication that could not be emitted.
    pub struct Clif { detail: String }
    code = "logic-compile.clif";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The XCL (XML Common Logic) dialect surface failed: a malformed XML
    /// sentence, or a meta triple that could not be serialized.
    pub struct Xcl { detail: String }
    code = "logic-compile.xcl";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The FnO (Function Ontology) projection failed: a function/cell that could
    /// not be extracted, an unparsable pattern or bind, or a bind order that
    /// could not be resolved.
    pub struct Fno { detail: String }
    code = "logic-compile.fno";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The SSSOM projection failed: a mapping row that could not be lowered, an
    /// invalid TSV cell, or a metadata block that could not be rendered.
    pub struct Sssom { detail: String }
    code = "logic-compile.sssom";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The EDOAL alignment projection failed: an unsupported mapping pattern, a
    /// missing attribute, or a path that could not be rendered.
    pub struct Edoal { detail: String }
    code = "logic-compile.edoal";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The SPARQL projection (query or put) failed: an atom/term that could not be
    /// lowered, an unrenderable where-block, or a reified claim that could not be
    /// emitted.
    pub struct Sparql { detail: String }
    code = "logic-compile.sparql";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The correspondence frontend/reader failed: a malformed correspondence node,
    /// an absent caveat/binding, or a typed-relation lookup that did not resolve.
    pub struct Correspondence { detail: String }
    code = "logic-compile.correspondence";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A `put` derivation failed: a correspondence whose get-leg does not invert,
    /// or a derived-put outcome that could not be assembled.
    pub struct Put { detail: String }
    code = "logic-compile.put";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The get-leg projection failed: an unparsable pattern/atom/bind/expr, an
    /// unresolved profile binding, or a path/expression that could not be rendered.
    pub struct GetLeg { detail: String }
    code = "logic-compile.get-leg";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A textual projection failed because its rendering could not be emitted.
    pub struct Text { detail: String }
    code = "logic-compile.text";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The RDF-carrier frontend failed to lower a node into the IR: a malformed
    /// axiom node, or a validation-shape set that could not be derived.
    pub struct Frontend { detail: String }
    code = "logic-compile.frontend";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A Common-Logic dialect round-trip did not close: a projection or re-parse
    /// failed, or two dialects disagreed on the recovered program.
    pub struct Roundtrip { detail: String }
    code = "logic-compile.roundtrip";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A graph-utility step failed: blank-node canonicalization could not close
    /// over the dataset.
    pub struct Graph { detail: String }
    code = "logic-compile.graph";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// An openEHR-OPT lift/recover step failed: a constraint that could not be
    /// lifted into a validation shape, or a shape that could not be recovered back
    /// into a constraint.
    pub struct OptLift { detail: String }
    code = "logic-compile.opt-lift";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A reasoning-compat resolution failed: an unknown contract name, or a
    /// dataset that does not resolve to a known reasoning contract.
    pub struct Compat { detail: String }
    code = "logic-compile.compat";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The relational-core projection/parse failed: an axiom/rule that could not
    /// be lowered, or an RDF term that could not be read into a core term.
    pub struct RelationalCore { detail: String }
    code = "logic-compile.relational-core";
    grade = compile_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The top-level projection driver failed to assemble the compiled artifacts
    /// from an upstream projection surface.
    pub struct Projection { detail: String }
    code = "logic-compile.projection";
    grade = compile_grade!();
    message = "{}", detail;
}

/// The complete logic-compiler diagnostic-code catalog, in registration order.
/// Every [`DiagKind`](gmeow_errors::DiagKind) minted in the crate appears here
/// exactly once — [`register_all`] seeds them and the collision test proves the
/// code strings are distinct.
pub const LOGIC_COMPILE_DIAG_CODES: &[&str] = &[
    Ir::CODE,
    Validation::CODE,
    Cgif::CODE,
    Clif::CODE,
    Xcl::CODE,
    Fno::CODE,
    Sssom::CODE,
    Edoal::CODE,
    Sparql::CODE,
    Correspondence::CODE,
    Put::CODE,
    GetLeg::CODE,
    Text::CODE,
    Frontend::CODE,
    Roundtrip::CODE,
    Graph::CODE,
    OptLift::CODE,
    Compat::CODE,
    RelationalCore::CODE,
    Projection::CODE,
];

/// Eagerly intern every logic-compiler diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        Ir::register(),
        Validation::register(),
        Cgif::register(),
        Clif::register(),
        Xcl::register(),
        Fno::register(),
        Sssom::register(),
        Edoal::register(),
        Sparql::register(),
        Correspondence::register(),
        Put::register(),
        GetLeg::register(),
        Text::register(),
        Frontend::register(),
        Roundtrip::register(),
        Graph::register(),
        OptLift::register(),
        Compat::register(),
        RelationalCore::register(),
        Projection::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_logic_compile_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            LOGIC_COMPILE_DIAG_CODES.len(),
            "register_all() and LOGIC_COMPILE_DIAG_CODES must enumerate the same kinds"
        );
        for code in LOGIC_COMPILE_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "logic-compile code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = LOGIC_COMPILE_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            LOGIC_COMPILE_DIAG_CODES.len(),
            "duplicate logic-compile diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
