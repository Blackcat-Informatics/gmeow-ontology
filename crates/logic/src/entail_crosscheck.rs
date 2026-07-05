// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native EL/DL ↔ entail-oracle divergence cross-check.
//!
//! This is the Docker-free replacement for the ELK/HermiT container cross-check:
//! it drives gmeow's OWN native reasoner ([`crate::reason::el`] for subsumption,
//! [`crate::reason::dl`] for consistency) against the independent
//! [`crate::entail_oracle`] (OWL-RL subsumption + OWL-Direct-tableau consistency,
//! a 70/70 W3C-entailment-conformance-tested `purrdf::entail` engine) and folds
//! the comparison into the structured [`DivergenceLedger`] the classic lane
//! already speaks. No Java, no Docker, no network — it runs entirely in-process
//! and is therefore on-gate.
//!
//! # World alignment (the load-bearing modelling choice)
//!
//! gmeow's native chase ([`crate::reason::run_reasoning`]) is **world-scoped**:
//! every derived subsumption carries the named-graph IRI it was asserted in, and
//! [`crate::store::WorldStore::worlds`] enumerates ONLY named graphs (default-graph
//! triples are dropped from the chase). The committed bundle spreads its asserted
//! axioms across *many* named graphs — `graph/base`, `graph/statements`,
//! `graph/logic`, `graph/alignments`, … — so there is **no single canonical
//! "base world"** the asserted TBox subsumptions carry.
//!
//! The entail oracle, by contrast, is world-**agnostic**: `purrdf::entail`
//! materializes over the dataset's **default graph** only (documented in
//! `purrdf-entail`, and confirmed by its `// entailment operates over the default
//! graph` skips). It cannot see named graphs and does not distinguish worlds.
//!
//! Because the base world is genuinely ambiguous, we compare the **UNION** of the
//! native subsumptions (every world folded to one canonical tag) against the
//! **UNION** of the oracle's subsumptions, tagged with that same canonical world
//! so the `(sub, sup, world)` tuples line up. Both engines reason **per world**,
//! preserving gmeow's world isolation on both sides:
//!
//! * the **native** subsumptions come from a single [`crate::reason::reason_closure`]
//!   over the bundle as-is — the production, world-partitioned DL chase (the EL
//!   calculus PLUS the DL post-pass, i.e. gmeow's complete native authority
//!   surface) — with each axiom's own world dropped to [`CROSSCHECK_WORLD`] and the
//!   result unioned; and
//! * the **oracle** subsumptions come from running [`entail_oracle`] once per
//!   world, each world projected into its own default graph via
//!   [`RdfDataset::project_named_graph`] (the only graph `purrdf::entail` reads),
//!   with all worlds' pairs unioned.
//!
//! Reasoning per world (rather than collapsing every world into one) is both the
//! honest choice — it never invents a cross-world subsumption neither engine
//! would derive in isolation — and the tractable one: gmeow's native DL post-pass
//! is world-partitioned, so a single-world collapse of the whole bundle is
//! quadratic and blows up, whereas the per-world union matches production cost.
//! Any residual divergence between the two unions is therefore a genuine
//! reasoner/profile difference (native EL/DL vs oracle OWL-RL / OWL-Direct), not a
//! graph-scoping artifact.

use purrdf::RdfDataset;

use crate::entail_oracle;
use crate::reason::InferredAxiom;
use crate::reason::ledger::{
    DivergenceLedger, LedgerVerdict, build_ledger, compare_consistency, compare_subsumption,
    dl_gap_rows, enforce,
};

/// The single canonical world every native subsumption's own world is folded to
/// (the union tag; see the module world-alignment note). The oracle's world-less
/// per-world pairs are tagged with this same IRI so the compared tuples align.
pub const CROSSCHECK_WORLD: &str =
    "https://blackcatinformatics.ca/gmeow/graph/entail-crosscheck-world";

/// `rdfs:subClassOf`.
const SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
/// `owl:Thing` — the trivial top; `X ⊑ owl:Thing` carries no hierarchy info.
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
/// `owl:Nothing` — the bottom; a named `X ⊑ owl:Nothing` is a class-unsatisfiability
/// signal, routed through the *consistency* comparison, NOT the subsumption one.
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
/// `rdfs:Resource` — the RDFS universal; `X ⊑ rdfs:Resource` carries no info.
const RDFS_RESOURCE: &str = "http://www.w3.org/2000/01/rdf-schema#Resource";

/// Strip a decoded Nemo object display form (`<iri>`) back to a bare IRI.
fn unbracket(display: &str) -> &str {
    display
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(display)
}

/// The full outcome of one cross-check run: the classified [`DivergenceLedger`],
/// the strict [`LedgerVerdict`] over it, and calibration counts (how many
/// subsumptions each side produced, and how many worlds the source bundle had —
/// the evidence behind the union/collapse world-alignment choice).
#[derive(Debug, Clone)]
pub struct CrosscheckOutcome {
    /// The classified divergence ledger (subsumption + consistency + gap rows).
    pub ledger: DivergenceLedger,
    /// The strict native⊇oracle verdict over [`Self::ledger`].
    pub verdict: LedgerVerdict,
    /// Distinct named-graph worlds present in the source bundle (evidence that the
    /// base world is ambiguous, so the union alignment is the honest choice).
    pub source_worlds: usize,
    /// Native subsumption pairs compared (union over worlds, canonical tag).
    pub native_subsumptions: usize,
    /// Oracle (OWL-RL) subsumption pairs compared.
    pub oracle_subsumptions: usize,
}

/// True iff `iri` is a named class the subsumption comparison should carry: a
/// real IRI (not a chase Skolem witness for a blank restriction node) and not one
/// of the trivial/degenerate poles handled elsewhere.
fn is_comparable_class(iri: &str) -> bool {
    !iri.starts_with(crate::facts::SKOLEM_PREFIX)
        && iri != OWL_THING
        && iri != OWL_NOTHING
        && iri != RDFS_RESOURCE
}

/// Extract gmeow's NATIVE named-class `rdfs:subClassOf` subsumptions from an
/// already-computed native DL closure, as `(subclass, superclass, world)` tuples.
///
/// The `inferred` closure is the shared [`crate::reason::reason_closure`] payload
/// (the EL calculus PLUS the DL post-pass) — the full native DL authority surface,
/// NOT the EL-only fragment — so the comparison holds gmeow's *complete* native
/// subsumption verdict against the oracle. Reusing the shared closure also means
/// the whole bundle is reasoned exactly ONCE (the DL post-pass is the costly step;
/// a second EL-only pass would only reproduce a strict subset).
///
/// Each axiom's own world is dropped to [`CROSSCHECK_WORLD`] and the result is
/// unioned (deduplicated) so the tuples line up with the unioned world-less oracle
/// pairs. Trivial pairs are excluded to match [`entail_oracle::owlrl_subsumptions`]:
/// reflexive pairs, `owl:Thing` / `rdfs:Resource` superclasses, `owl:Nothing`
/// (routed through consistency), and Skolem-witness endpoints (blank restriction
/// nodes the oracle keeps as blanks and never reports as subsumptions).
fn native_subsumptions(inferred: &[InferredAxiom]) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    for ax in inferred {
        if ax.predicate != SUBCLASS_OF {
            continue;
        }
        let sub = unbracket(&ax.subject);
        let sup = unbracket(&ax.object);
        if sub == sup || !is_comparable_class(sub) || !is_comparable_class(sup) {
            continue;
        }
        out.push((sub.to_owned(), sup.to_owned(), CROSSCHECK_WORLD.to_owned()));
    }
    out.sort();
    out.dedup();
    out
}

/// Run the oracle OWL-RL subsumption closure once **per world** and union the
/// pairs, tagging each with [`CROSSCHECK_WORLD`].
///
/// Each `world` is projected into its own default graph — the only graph
/// `purrdf::entail` reads — via [`RdfDataset::project_named_graph`], so the oracle
/// reasons over exactly the same per-world axiom set the native chase does. Skolem
/// endpoints are dropped defensively so the two sides share one comparable universe.
fn oracle_subsumptions(bundle: &RdfDataset, worlds: &[String]) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    for world in worlds {
        let world_ds = bundle.project_named_graph(world);
        for (sub, sup) in entail_oracle::owlrl_subsumptions(&world_ds) {
            if is_comparable_class(&sub) && is_comparable_class(&sup) {
                out.push((sub, sup, CROSSCHECK_WORLD.to_owned()));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Run the oracle OWL-Direct consistency check once **per world** and fold the
/// verdicts into `(global_consistent, unsat_classes)`.
///
/// Each world is projected into its own default graph so the tableau reasons over
/// exactly the same per-world axiom set as native. Two per-world folds:
///
/// * **unsat classes** are unioned across worlds; and
/// * **global consistency** is the AND across worlds — the bundle is globally
///   inconsistent iff *some* world is.
///
/// The oracle's boolean is reconciled to the SAME meaning native's `consistent`
/// carries — GLOBAL consistency (no individual forced into `owl:Nothing`) — before
/// folding. Comparing the raw flags would be apples-to-oranges: native keeps class
/// unsatisfiability (`unsatisfiable_classes`, an empty-but-unpopulated class)
/// SEPARATE from global inconsistency, whereas the oracle's `consistency()` returns
/// the boolean `unsat.is_empty()` and so reports `false` for a *consistent* ontology
/// that merely has empty classes. The oracle's own three cases are explicit:
///
/// * `(true,  [])`   — fully consistent, no empty classes,
/// * `(false, [X…])` — CONSISTENT ontology WITH empty classes X…,
/// * `(false, [])`   — globally inconsistent (an ABox clash).
///
/// So a world is globally inconsistent iff its flag is `false` AND its unsat list
/// is empty. The empty-class disagreement is not lost — it surfaces through the
/// (separately compared) unsatisfiable-class sets.
fn oracle_consistency(bundle: &RdfDataset, worlds: &[String]) -> (bool, Vec<String>) {
    let mut global_consistent = true;
    let mut unsat: Vec<String> = Vec::new();
    for world in worlds {
        let world_ds = bundle.project_named_graph(world);
        let (flag, world_unsat_raw) = entail_oracle::consistency(&world_ds);
        let world_unsat: Vec<String> = world_unsat_raw
            .into_iter()
            .filter(|c| is_comparable_class(c))
            .collect();
        // This world is globally inconsistent only when the flag is false AND no
        // class-unsatisfiability explains the false (an ABox clash), per the docs.
        if !flag && world_unsat.is_empty() {
            global_consistent = false;
        }
        unsat.extend(world_unsat);
    }
    unsat.sort();
    unsat.dedup();
    (global_consistent, unsat)
}

/// Run the native EL/DL ↔ entail-oracle divergence cross-check over `bundle`.
///
/// `bundle` is the whole committed dataset (all graphs); it is reshaped twice from
/// the same quad multiset (see the module world-alignment note) so the native
/// chase and the oracle reason over identical asserted axioms. The result folds
/// the subsumption comparison, the consistency comparison, and any native DL
/// coverage gap into a [`DivergenceLedger`], carries the strict [`enforce`]
/// verdict, and reports calibration counts.
///
/// # Errors
///
/// Returns `Err(String)` if either reshaped dataset cannot be built or the native
/// EL/DL chase fails. An oracle materialization error is a HARD FAIL inside
/// [`entail_oracle`] (it panics) — the cross-check never silently downgrades an
/// unclosable graph.
pub fn run_entail_crosscheck(bundle: &RdfDataset) -> Result<CrosscheckOutcome, String> {
    let worlds = {
        let store = crate::store::WorldStore::new();
        store.load_dataset(bundle)?;
        store.worlds()
    };
    let source_worlds = worlds.len();

    // The native subsumption surface is the EL closure (the class-level
    // rdfs:subClassOf chase); the consistency verdict + DL coverage gaps come from
    // the DL post-pass. Both run over the whole world-partitioned bundle.
    let inferred = crate::reason::el::el_closure(bundle)?.inferred;
    let native_verdict = crate::reason::dl::dl_consistency(bundle)?;

    // Subsumption: native DL closure (union over every world) vs the oracle OWL-RL
    // closure run once per world, both tagged with the canonical world.
    let native_subs = native_subsumptions(&inferred);
    let oracle_subs = oracle_subsumptions(bundle, &worlds);
    let subsumption_rows = compare_subsumption(&native_subs, &oracle_subs);

    // Consistency: native DL verdict (world-partitioned, whole bundle) vs the
    // oracle OWL-Direct tableau verdict run once per world and folded.
    let native_unsat: Vec<String> = native_verdict
        .unsatisfiable_classes
        .iter()
        .map(|u| unbracket(&u.class).to_owned())
        .filter(|c| is_comparable_class(c))
        .collect();
    let (oracle_global_consistent, oracle_unsat) = oracle_consistency(bundle, &worlds);
    let consistency_rows = compare_consistency(
        native_verdict.consistent,
        &native_unsat,
        Some(oracle_global_consistent),
        &oracle_unsat,
    );

    // Any native DL coverage gap is recorded honestly as a DlGap row (a construct
    // present in the bundle the native path could not decide).
    let gap_rows = dl_gap_rows(&native_verdict.gaps);

    let ledger = build_ledger(subsumption_rows, consistency_rows, gap_rows, Vec::new());
    let verdict = enforce(&ledger);

    Ok(CrosscheckOutcome {
        native_subsumptions: native_subs.len(),
        oracle_subsumptions: oracle_subs.len(),
        source_worlds,
        ledger,
        verdict,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reason::ledger::DivergenceKind;
    use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

    const W: &str = "http://gmeow.example/world";
    const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";

    fn iri(local: &str) -> String {
        format!("http://gmeow.example/{local}")
    }

    /// Build a single-named-world dataset from `(subject, predicate, object)` IRI
    /// triples, so the per-world chase and oracle both see them (default-graph
    /// triples are invisible to the world-scoped chase).
    fn world_dataset(triples: &[(&str, &str, &str)]) -> std::sync::Arc<RdfDataset> {
        // A local name (no scheme) is prefixed with the example namespace; a full
        // IRI object (e.g. owl:Class) is kept verbatim.
        fn obj(o: &str) -> RdfTerm {
            if o.contains("://") {
                RdfTerm::iri(o.to_owned())
            } else {
                RdfTerm::iri(iri(o))
            }
        }
        let mut builder = RdfDatasetBuilder::new();
        for (s, p, o) in triples {
            let quad = RdfQuad::new(RdfTerm::iri(iri(s)), *p, obj(o)).in_graph(RdfTerm::iri(W));
            builder.push_owned_quad(&quad);
        }
        builder.freeze().expect("valid test dataset")
    }

    #[test]
    fn native_and_oracle_agree_on_a_sub_b_sub_c() {
        // :A ⊑ :B ⊑ :C — both engines must derive the transitive :A ⊑ :C, and the
        // consistency verdict must agree (satisfiable), so the ledger is pure Agree
        // and the verdict passes.
        let ds = world_dataset(&[
            ("A", RDF_TYPE, OWL_CLASS),
            ("B", RDF_TYPE, OWL_CLASS),
            ("C", RDF_TYPE, OWL_CLASS),
            ("A", SUBCLASS_OF, "B"),
            ("B", SUBCLASS_OF, "C"),
        ]);
        let outcome = run_entail_crosscheck(ds.as_ref()).expect("cross-check runs");

        // Every subsumption row is an agreement — no NativeOnly / OracleOnly.
        assert_eq!(
            outcome.ledger.native_only, 0,
            "no native-only rows: {:#?}",
            outcome.ledger.rows
        );
        assert_eq!(
            outcome.ledger.oracle_only, 0,
            "no oracle-only rows: {:#?}",
            outcome.ledger.rows
        );
        assert_eq!(outcome.ledger.dl_gap, 0, "no native DL coverage gap");
        assert!(
            outcome.verdict.passed,
            "pure agreement must pass: {:?}",
            outcome.verdict
        );

        // The three subsumption tuples (asserted A⊑B, B⊑C and derived A⊑C) are all
        // present and classified Agree.
        let agrees: Vec<(&str, &str)> = outcome
            .ledger
            .rows
            .iter()
            .filter(|r| r.category == "subsumption" && r.kind == DivergenceKind::Agree)
            .map(|r| (r.subject.as_str(), r.object.as_str()))
            .collect();
        for (s, o) in [("A", "B"), ("B", "C"), ("A", "C")] {
            assert!(
                agrees.contains(&(iri(s).as_str(), iri(o).as_str())),
                "{s} ⊑ {o} must be an Agree row: {agrees:?}"
            );
        }

        // A consistency agreement row (both consistent) is present.
        assert!(
            outcome
                .ledger
                .rows
                .iter()
                .any(|r| r.category == "consistency"
                    && r.kind == DivergenceKind::Agree
                    && r.object == "consistent"),
            "both engines agree the TBox is consistent: {:#?}",
            outcome.ledger.rows
        );
    }

    #[test]
    fn both_engines_flag_a_disjointness_unsatisfiable_class() {
        // :X ⊑ :Y, :X ⊑ :Z, :Y disjointWith :Z makes :X provably empty. Both the
        // native DL post-pass and the oracle tableau report :X unsatisfiable while
        // the ontology as a whole stays consistent (no individual populates :X), so
        // the unsat-class comparison agrees and the verdict passes.
        let ds = world_dataset(&[
            ("X", RDF_TYPE, OWL_CLASS),
            ("Y", RDF_TYPE, OWL_CLASS),
            ("Z", RDF_TYPE, OWL_CLASS),
            ("X", SUBCLASS_OF, "Y"),
            ("X", SUBCLASS_OF, "Z"),
            ("Y", OWL_DISJOINT_WITH, "Z"),
        ]);
        let outcome = run_entail_crosscheck(ds.as_ref()).expect("cross-check runs");

        assert!(
            outcome
                .ledger
                .rows
                .iter()
                .any(|r| r.category == "consistency"
                    && r.kind == DivergenceKind::Agree
                    && r.subject == iri("X")
                    && r.object == "owl:Nothing"),
            "native and oracle agree :X is unsatisfiable: {:#?}",
            outcome.ledger.rows
        );
        assert_eq!(
            outcome.ledger.oracle_only, 0,
            "unsat verdicts must agree, no oracle-only: {:#?}",
            outcome.ledger.rows
        );
        assert!(
            outcome.verdict.passed,
            "agreeing unsat-class verdict passes: {:?}",
            outcome.verdict
        );
    }
}
