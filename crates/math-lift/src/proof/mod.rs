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
//! # What this bridge reads
//!
//! The TSTP a REAL prover writes, not only what our own reasoner emits: `cnf` and `fof`
//! annotated formulas, the full fifteen-value TPTP role set, equality literals, the
//! quantifiers and every binary connective, `inference(…)` records with any SZS status,
//! `file(…)`/`theory(…)`/`introduced(…)`/`creator(…)`/`unknown` external sources, bare
//! `<name>` DAG sources, and the `<useful_info>` 5th field. The refusals that remain are
//! the ones no reading could honour — a typed (`tff`/`thf`/`tcf`) body, an `include` whose
//! document is absent, a `<sources>` list, an inline nested parent derivation.
//!
//! # The rung this bridge claims
//!
//! [`Rung::section_retraction`](crate::frame::Rung::section_retraction) — the only bridge
//! that claims it, and the one `math:ProofDependencyGraph`'s own definition names ("the DAG
//! recovers from the lift and witness"). The claim is EARNED by
//! `lift`'s `the_lift_is_a_section_the_derivation_reconstructs_from_the_graph_alone`,
//! which rebuilds every step name, formula ROLE, inference rule, parent set, and rendered
//! conclusion from the emitted Turtle and nothing else.
//!
//! It is claimed **per run**, not per bridge. At `logic:ExactPreservation` everything the
//! source states must cross, and three constructs a real prover writes have no `math:`
//! codomain to cross into: an SZS inference status other than `status(thm)`, the role
//! `unknown`, and a `<useful_info>` field. A derivation carrying any of them enumerates it
//! through `RunFrame::record_unmapped` and travels at `logic:LossyLens` instead — the
//! honest alternative to either refusing real prover output or silently keeping the strong
//! rung over a dropped fact. See [`mod@lift`] for the full residue rule.

pub mod lift;
pub mod tstp;

pub use lift::lift;
pub use tstp::{
    Clause, Conclusion, Connective, Derivation, ExternalSource, Formula, Literal, Quantifier, Role,
    Source, Step, Term, parse,
};
