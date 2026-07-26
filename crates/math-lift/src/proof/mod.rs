// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The proof → `math:` bridge: the complex-proofs flagship.
//!
//! `MATHEMATICS-BRIDGES.md` states the bar:
//!
//! > **Flagship — complex proofs as process.** Answerable when a proof-assistant dependency
//! > DAG lifts into the `math:` proof layer bound to `logic:` teleology/transaction, so a
//! > complex proof is a goal-decomposed process whose steps, axiom dependencies, and
//! > verification claim are all first-class.
//!
//! The bridge is split into the two tiers the crate doc mandates:
//!
//! | tier | module | contract |
//! |---|---|---|
//! | parse | [`tstp`] | bytes → a typed derivation AST, no RDF, no ontology in the loop |
//! | lift | [`mod@lift`] | AST → `math:` triples, no parsing |
//!
//! # Why the reader lives here and not in `gmeow-conformance`
//!
//! `crates/conformance` also parses TPTP/TSTP, and it must NOT be a dependency of a shipped
//! lifter: it is the repo-maintenance crate that grades external corpora, and it links the
//! reasoning runtime this crate exists to stay clear of (`Cargo.toml`'s `crate-type =
//! ["rlib"]` note). The two readers answer different questions, too — the conformance
//! parser lowers a TPTP *problem* into the first-order `Formula` IR for the engine, while
//! this one reads a TSTP *derivation* as a dependency DAG. Sharing one reader would force
//! the lifter to carry a `Formula` IR it never lowers, and the conformance grader to carry
//! a derivation AST it never walks.
//!
//! The committed fixture `fixtures/theorem-subclass.tstp` is the seam: it is a PRODUCT of
//! our own reasoner (`gmeow_logic::proof_tree::ProofTree::to_tstp`), byte-pinned by
//! `gmeow_conformance::external::tptp::lower_fol`'s
//! `the_committed_tstp_fixture_is_exactly_what_our_reasoner_produces`, and read back here.
//! Both crates `include_str!` the one file, so a drift on either side is caught.
//!
//! # The rung this bridge claims
//!
//! [`Rung::section_retraction`](crate::frame::Rung::section_retraction) — the only bridge
//! that claims it, and the one `math:ProofDependencyGraph`'s own definition names ("the DAG
//! recovers from the lift and witness"). The claim is EARNED by
//! `lift`'s `the_lift_is_a_section_the_derivation_reconstructs_from_the_graph_alone`,
//! which rebuilds every step name, inference rule, parent set, and rendered conclusion from
//! the emitted Turtle and nothing else.
//!
//! That rung is also what makes the reader's refusals mandatory rather than fussy: at
//! `logic:ExactPreservation` **everything the source states must cross**, so any TSTP
//! construct this bridge does not structure — a `file(…)`/`theory(…)` source, a
//! `<useful_info>` field, a role other than `axiom`/`plain`, a `fof` formula — is a typed
//! hard failure instead of a silently dropped field. See [`tstp`] for the full list.

pub mod lift;
pub mod tstp;

pub use lift::lift;
pub use tstp::{Clause, Derivation, Literal, Role, Step, Term, parse};
