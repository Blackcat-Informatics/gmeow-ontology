// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Ingestion diagnostic kinds.
//!
//! Every unliftable input in this crate is a HARD fail. `MATHEMATICS-RUNTIME.md`'s
//! ingestion rules are explicit about why:
//!
//! > No silent fallback from a structured expression to a string. A parse that cannot
//! > produce a well-formed AST fails; it does not emit a string-valued placeholder.
//! > […] No optional parser backends.
//!
//! So there is no `Option<Ast>` return, no "best effort" mode, and no partial lift: a
//! source this crate cannot fully structure raises a typed
//! [`DiagKind`](gmeow_errors::DiagKind) under the `math.lift.*` namespace and emits
//! nothing.
//!
//! # Why [`EmptyCodomain`] exists
//!
//! The shipped native lint `math:UnliftableIngest`
//! (`crates/validate/src/lint.rs::check_unliftable_ingest`) fires when a run carrying a
//! `math:parseSource` has nothing `gmeow:wasGeneratedBy` it. That lint is a downstream
//! backstop over an already-emitted graph. Emitting a codomain-free run and letting the
//! validator catch it would mean shipping a known-bad graph and relying on a later pass —
//! so the condition is rejected HERE, in Rust, before a single triple is serialized.

// `define_diag_kind!` generates each kind's `detail` field without a doc comment — the
// kind's own doc comment above the macro carries the meaning, and there is no hook to
// document the generated field. The crate-wide `deny(missing_docs)` is relaxed HERE only,
// so every hand-written item elsewhere still has to be documented.
#![allow(missing_docs)]

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// The source bytes are not valid UTF-8, so a text front-end cannot read them.
    pub struct SourceNotUtf8 { detail: String }
    code = "math.lift.source.not-utf8";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An R script is not syntactically well-formed over the statistical subset this
    /// front-end parses: an unterminated string, an unbalanced delimiter, an unexpected
    /// token, or a malformed model formula.
    pub struct RParse { detail: String }
    code = "math.lift.r.parse";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An R script parses, but its content cannot be structured into the `math:`
    /// codomain: general computation or control flow with no statistical content, a
    /// model call whose formula argument is absent, or a construct that would only
    /// survive the lift as an opaque string.
    pub struct RUnliftable { detail: String }
    code = "math.lift.r.unliftable";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// The `.onnx` byte stream is not a well-formed protobuf message: a truncated
    /// varint or length-delimited field, an unknown wire type, a length that runs past
    /// the end of the buffer, or a nested message that does not close.
    pub struct OnnxWire { detail: String }
    code = "math.lift.onnx.wire";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An ONNX model decodes as protobuf, but its graph cannot be structured into the
    /// `math:` codomain: no graph, no computation node, or a node referencing a tensor
    /// the graph never declares.
    pub struct OnnxUnliftable { detail: String }
    code = "math.lift.onnx.unliftable";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A TSTP derivation is not syntactically well-formed: a malformed annotated
    /// formula, an unterminated `inference(...)` record, an unquoted atom where a
    /// term is required, a connective mixed without parentheses where TPTP requires
    /// them, or a dialect outside the first-order fragment (`tff`, `thf`, `tcf`,
    /// `include`).
    ///
    /// `cnf` AND `fof` both parse — a real prover writes either, and a `fof`
    /// conclusion is carried as a formula rather than coerced into a clause shape.
    ///
    /// A structurally well-formed derivation whose DEPENDENCIES do not resolve is
    /// [`ProofUnliftable`], not this — there is nothing wrong with the syntax.
    pub struct TstpParse { detail: String }
    code = "math.lift.proof.parse";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A TSTP derivation parses, but carries no well-founded proof to lift: no derived
    /// step, a parent name the derivation never introduces, or a cycle in the
    /// dependency graph.
    ///
    /// It is NOT raised for a construct the lift can carry at a weaker rung. A
    /// `file(...)`/`theory(...)`/`introduced(...)` source becomes an external warrant;
    /// a non-`thm` SZS status, an `unknown` role, and a `<useful_info>` field are
    /// enumerated as `math:unmappedConstruct` residue and DOWNGRADE that run's
    /// correspondence off `logic:SectionRetraction`. Refusing them outright would
    /// have been a scope narrowing dressed as rigour; the rung is decided per run,
    /// before a triple is written, so the declared law stays true of what shipped.
    pub struct ProofUnliftable { detail: String }
    code = "math.lift.proof.unliftable";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// A lift produced an ingest run with an EMPTY structured codomain. Emitting it
    /// would ship a graph that the native `math:UnliftableIngest` lint is guaranteed to
    /// reject, so the lift declines instead of serializing a known-bad run.
    pub struct EmptyCodomain { detail: String }
    code = "math.lift.empty-codomain";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

/// The complete `math-lift` diagnostic-code catalog, in registration order.
pub const MATH_LIFT_DIAG_CODES: &[&str] = &[
    SourceNotUtf8::CODE,
    RParse::CODE,
    RUnliftable::CODE,
    OnnxWire::CODE,
    OnnxUnliftable::CODE,
    TstpParse::CODE,
    ProofUnliftable::CODE,
    EmptyCodomain::CODE,
];

/// Eagerly intern every `math-lift` diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        SourceNotUtf8::register(),
        RParse::register(),
        RUnliftable::register(),
        OnnxWire::register(),
        OnnxUnliftable::register(),
        TstpParse::register(),
        ProofUnliftable::register(),
        EmptyCodomain::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_math_lift_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            MATH_LIFT_DIAG_CODES.len(),
            "register_all() and MATH_LIFT_DIAG_CODES must enumerate the same kinds"
        );
        for code in MATH_LIFT_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "math-lift code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = MATH_LIFT_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            MATH_LIFT_DIAG_CODES.len(),
            "duplicate math-lift diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }

    #[test]
    fn every_code_is_namespaced_under_math_lift() {
        for code in MATH_LIFT_DIAG_CODES {
            assert!(
                code.starts_with("math.lift."),
                "`{code}` escapes the math.lift.* namespace"
            );
        }
    }
}
