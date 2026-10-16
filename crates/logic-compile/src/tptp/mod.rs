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

mod writer;

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
