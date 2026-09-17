// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete-admission TPTP FOF projection.
//!
//! The writer consumes an exact [`frontend::SourceAdmission`](crate::frontend::SourceAdmission),
//! not a detached [`LogicProgram`](crate::ir::LogicProgram). This prevents ordinary
//! selected domain assertions, graph placement, owned formulas, compiler failures or
//! unsupported procedural relations from disappearing before the proof problem is built.

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
