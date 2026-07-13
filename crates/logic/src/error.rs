// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reasoning-core diagnostic kinds.
//!
//! The reasoning core loads the RDF 1.2 carrier into world-indexed stores,
//! lowers the compiled intermediate representation into the runtime rule form,
//! drives the forward and backward engines, and derives the teleology,
//! transaction, transition, obligation, counterfactual, and probabilistic
//! consequences — each a HARD failure surface (no-optionality): a malformed
//! carrier node, an ill-formed query program, an engine that refuses, or a
//! derivation precondition that does not hold must surface as a typed diagnostic
//! rather than a bare string. Each defect is a
//! [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind) under the `logic.*` code
//! namespace, so the reasoning core reports on the shared substrate.
//!
//! Every kind carries a single `detail` string that preserves the authored
//! condition text verbatim; discrimination is by code + grade, and the message
//! is the preserved detail. The area codes track the core's subsystem boundaries
//! so a downstream reader can key on where a defect arose.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

/// The grade every reasoning-core diagnostic carries: a hard modeling-discipline
/// violation at the binding standpoint. The core admits no degraded fallback, so
/// each condition is an `Error`.
macro_rules! logic_grade {
    () => {
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        )
    };
}

define_diag_kind! {
    /// A runtime intermediate-representation constructor rejected its input: a
    /// malformed rule/query IR node, an empty term name, or a well-formedness
    /// precondition on the runtime AST that does not hold.
    pub struct Ir { detail: String }
    code = "logic.ir";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The query-program frontend failed: an unparsable directive, atom, term, or
    /// rule clause, an unresolved prefix, or a goal that is not well formed.
    pub struct Query { detail: String }
    code = "logic.query";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The reasoning driver failed to assemble or fold a closure: a premise/row it
    /// could not decode, a term it could not re-materialize into an RDF term, or a
    /// contract it could not resolve.
    pub struct Reason { detail: String }
    code = "logic.reason";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A retained comparison engine refused a program or dataset: a load/parse
    /// failure or a chase that could not close.
    pub struct Engine { detail: String }
    code = "logic.engine";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The runtime pin's typed refusal when a live [`EngineContract`]
    /// descriptor differs from the descriptor a consumer pinned: answers minted
    /// under the pinned contract must not be trusted against a drifted engine, so
    /// the mismatch is a distinct hard failure rather than the generic reasoning
    /// [`Reason`] kind.
    ///
    /// [`EngineContract`]: crate::runtime::EngineContract
    pub struct ContractDrift { detail: String }
    code = "logic.contract-drift";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The native physical execution core failed: a chase, semi-naive, magic-sets,
    /// or parity step that could not evaluate.
    pub struct Physical { detail: String }
    code = "logic.physical";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A teleology derivation precondition does not hold: a goal, plan, outcome,
    /// action-schema, gate-probe, or deontic node missing a required property, an
    /// unrenderable reifier, or a malformed duration/instant.
    pub struct Teleology { detail: String }
    code = "logic.teleology";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A transaction derivation precondition does not hold: a malformed transaction
    /// program, an unresolved step/protocol/isolation node, a view classification
    /// that could not close, or a conflict edge that could not be assembled.
    pub struct Transaction { detail: String }
    code = "logic.transaction";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A transition derivation precondition does not hold: an elementary update
    /// that is not well formed, a transition fact that could not be keyed or
    /// reified, or a world-quad sort that could not close.
    pub struct Transition { detail: String }
    code = "logic.transition";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The native reasoning oracle / cross-check failed: an EL/DL entailment run or
    /// a divergence fold that could not complete.
    pub struct Oracle { detail: String }
    code = "logic.oracle";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A certificate/certifier step failed: a proof obligation that could not be
    /// assembled or a certificate that could not be emitted.
    pub struct Certify { detail: String }
    code = "logic.certify";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A provenance derivation failed: a derivation-graph edge or attribution that
    /// could not be assembled.
    pub struct Provenance { detail: String }
    code = "logic.provenance";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// An obligation derivation precondition does not hold: a deontic obligation or
    /// entrenchment node missing a required property, or a verdict that could not
    /// close.
    pub struct Obligation { detail: String }
    code = "logic.obligation";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A counterfactual derivation failed: an intervention world that could not be
    /// built, or a counterfactual query that could not be evaluated.
    pub struct Counterfactual { detail: String }
    code = "logic.counterfactual";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A probabilistic derivation failed: a probability/confidence fact that could
    /// not be lowered, or a probabilistic model that could not be evaluated.
    pub struct Probabilistic { detail: String }
    code = "logic.probabilistic";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The world-indexed store failed to load, index, or query the carrier: a
    /// malformed quad, an absent world, or a select that could not be evaluated.
    pub struct Store { detail: String }
    code = "logic.store";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The compiler-IR → runtime lowering failed: an axiom/rule that could not be
    /// bridged into the runtime rule form.
    pub struct Lower { detail: String }
    code = "logic.lower";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A verify-lane step failed: a governance/verify query or gap-finding fold
    /// that could not close.
    pub struct Verify { detail: String }
    code = "logic.verify";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A foundation multi-world chase step failed: a world that could not be chased
    /// or a foundation quad that could not be assembled.
    pub struct Foundation { detail: String }
    code = "logic.foundation";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// The relational-core waist failed: a formula/rule that could not be lowered
    /// into the relational core, or a core term that could not be read.
    pub struct RelationalCore { detail: String }
    code = "logic.relational-core";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A result-family projection failed: a result shape or result-RDF row that
    /// could not be assembled or rendered.
    pub struct Result { detail: String }
    code = "logic.result";
    grade = logic_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A reference-resolution step failed: a reference that could not be resolved
    /// to a known node or contract.
    pub struct Reference { detail: String }
    code = "logic.reference";
    grade = logic_grade!();
    message = "{}", detail;
}

/// The complete reasoning-core diagnostic-code catalog, in registration order.
/// Every [`DiagKind`](gmeow_errors::DiagKind) minted in the crate appears here
/// exactly once — [`register_all`] seeds them and the collision test proves the
/// code strings are distinct.
pub const LOGIC_DIAG_CODES: &[&str] = &[
    Ir::CODE,
    Query::CODE,
    Reason::CODE,
    Engine::CODE,
    ContractDrift::CODE,
    Physical::CODE,
    Teleology::CODE,
    Transaction::CODE,
    Transition::CODE,
    Oracle::CODE,
    Certify::CODE,
    Provenance::CODE,
    Obligation::CODE,
    Counterfactual::CODE,
    Probabilistic::CODE,
    Store::CODE,
    Lower::CODE,
    Verify::CODE,
    Foundation::CODE,
    RelationalCore::CODE,
    Result::CODE,
    Reference::CODE,
];

/// Eagerly intern every reasoning-core diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        Ir::register(),
        Query::register(),
        Reason::register(),
        Engine::register(),
        ContractDrift::register(),
        Physical::register(),
        Teleology::register(),
        Transaction::register(),
        Transition::register(),
        Oracle::register(),
        Certify::register(),
        Provenance::register(),
        Obligation::register(),
        Counterfactual::register(),
        Probabilistic::register(),
        Store::register(),
        Lower::register(),
        Verify::register(),
        Foundation::register(),
        RelationalCore::register(),
        Result::register(),
        Reference::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_logic_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            LOGIC_DIAG_CODES.len(),
            "register_all() and LOGIC_DIAG_CODES must enumerate the same kinds"
        );
        for code in LOGIC_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "logic code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = LOGIC_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            LOGIC_DIAG_CODES.len(),
            "duplicate logic diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
