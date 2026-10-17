// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete-admission TPTP FOF projection.
//!
//! The writer consumes an exact [`frontend::SourceAdmission`](crate::frontend::SourceAdmission),
//! not a detached [`LogicProgram`](crate::ir::LogicProgram). This prevents ordinary
//! selected domain assertions, graph placement, owned formulas, compiler failures or
//! unsupported procedural relations from disappearing before the proof problem is built.
//! Projection additionally requires the unchanged live compiler decision. Serialized
//! admissions are observation receipts and must be re-admitted from the selected
//! source; editing refusals, profile or symbol data cannot authorize execution.
//!
//! Relation objects occupy the first argument of an application family at each
//! source arity. Their denotational equality therefore propagates through facts,
//! rules, formulas and class laws by ordinary first-order congruence. A fixed
//! semantic-profile prelude links applications to membership, identity and the
//! canonical schema constraints. Unary interpretation does not depend on whether
//! a class declaration happens to accompany the selected source. Profile laws
//! cannot authorize an empty or refused source selection.
//!
//! Certified source-class laws cover object-property restrictions, Boolean
//! membership and resource enumeration. Nonempty union/intersection lists lower
//! to disjunction/conjunction, complement to classical negation, and resource
//! enumeration to denotational equality without a unique-name assumption.
//! Disjoint unions carry both the covering law and every pairwise disjointness
//! law, with repeated source members interpreted as a set. Exact
//! source constants and selected list support remain attached to these laws;
//! unsupported dependencies, value interpretations and contexts refuse admission.
//! Local self restrictions use an admitted true native XSD boolean and define
//! membership by the property's self-edge; they do not assert global reflexivity.
//! Qualified object counts require their explicit class. Minima use distinct
//! denoting witnesses, maxima exclude an excessive witness set, and exact counts
//! retain both bounds. Independent bounds remain conjunctive.
//!
//! Reasoning-contract declarations retain their native configuration and authored
//! support facts as typed metatheory evidence, outside the object-theory axioms.
//! Declaring a nonclassical configuration does not select it for this classical
//! proof operation. Referenced closure entries and
//! facet-type support must belong to the selected source; compiler errors and
//! scoped declarations still refuse admission.
//!
//! The source profile retains explicit `inContext` and typed modal standard
//! translations already carried by the formula IR. Other scope annotations on
//! selected formulas or their owned descendants block the complete problem;
//! atomic routing must not discard them. A graph-local source index propagates
//! these requirements once rather than rescanning shared trees per formula.

mod writer;

mod canon_admission;
pub use canon_admission::{
    CANON_ADMISSION_CHANNEL, CANON_ADMISSION_MEMBER, CanonAdmissionPublication,
};

mod protocol;
pub use protocol::{
    FofTask, SzsAdmission, SzsOutcome, SzsProtocolError, SzsStatusObservation,
    SzsTranscriptChannel, admit_szs_transcript, problem_name_matches,
};

pub use writer::{
    FofProjection, FofProjectionBlocker, FofProjectionStatus, FofSentence, project_tptp_fof,
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
