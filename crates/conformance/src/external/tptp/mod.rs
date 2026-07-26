// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! TPTP FOF/CNF problem-body ingestion for the external FOL correctness oracle.
//!
//! [`szs`](crate::external::szs) reads only the `% SZS status` *result* comment.
//! This module reads the *problem itself*: a native, dependency-free FOF/CNF
//! parser ([`parser`]) lowers each annotated formula into the full-FOL
//! [`Formula`](gmeow_logic_compile::ir::Formula) IR, and the FOL lowerer
//! (`lower_fol`) applies the refutation reduction (`premises ∧ ¬conjecture`)
//! and projects the EL/DL-expressible fragment onto a world-scoped OWL-RDF EDB
//! that the native DL engine decides. Its verdict is then graded against the
//! problem's SZS ground truth.
//!
//! Fragment boundary (no-optionality / hard-fail):
//!
//! * A **malformed** problem is a [`TptpError::Syntax`] — a hard parse failure.
//! * A **well-formed but out-of-fragment** construct (function symbols in
//!   argument position, genuine `∃`-under-`∀` needing Skolem functions, n-ary
//!   predicates the binary DL projection cannot carry, equality) is a
//!   [`TptpError::Unsupported`] — a *capability gap* the caller records as a
//!   DlGap ledger row, **never** a silently-swallowed `incomplete`.
//!
//! The two are distinct on purpose: a capability gap is honest ("our engine
//! cannot express this"), whereas `incomplete` means the oracle itself was
//! undecided. Conflating them would hide gaps behind a green verdict.

pub mod lower_fol;
pub mod parser;

pub use lower_fol::{
    LoweredProblem, LoweringGap, lower_and_decide, lower_problem, lower_to_fol_program,
};
pub use parser::{
    AnnotatedFormula, TPTP_NS, TptpError, TptpRole, TptpSource, TstpTerm, parse_tptp,
};
