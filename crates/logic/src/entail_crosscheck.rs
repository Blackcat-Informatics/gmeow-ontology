// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native ↔ entail-oracle subsumption divergence cross-check.
//!
//! This is the Docker-free, on-gate replacement for the retired external container
//! subsumption cross-check: it drives gmeow's OWN native reasoner ([`crate::reason::reason_all`],
//! the same world-partitioned chase `reason-verify` runs on-gate) against the
//! independent [`crate::entail_oracle`] OWL-RL subsumption closure (a 70/70
//! W3C-entailment-conformance-tested `purrdf::entail` engine) and folds the
//! comparison into the structured `DivergenceLedger`. No Java, no Docker, no
//! network — it runs entirely in-process and is therefore on-gate.
//!
//! # Scope: subsumption superset, not consistency
//!
//! The on-gate cross-check compares the **subsumption hierarchy** only — the
//! independent anti-regression oracle confirming native derives every standard
//! OWL-RL subsumption (`native ⊇ oracle`). It does NOT run a consistency
//! comparison: purrdf's OWL-RL `materialize` is a positive-only forward closure
//! that cannot witness an inconsistency, and the OWL-Direct **tableau** that can
//! ([`crate::entail_oracle::globally_consistent`]) is — though sound and
//! conformance-tested — empirically intractable swept per-world across the whole
//! bundle (the same inherent OWL-Direct cost that kept the retired external
//! consistency oracle off-gate, independent of implementation). Native's own consistency verdict
//! remains gated on-gate by `reason-verify` (`ReasoningResult::is_consistent`);
//! the tableau consistency oracle stays a unit-tested capability, not an on-gate
//! 89-world sweep.
//!
//! # World alignment (the load-bearing modelling choice)
//!
//! gmeow's native structured chase is **world-scoped**:
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
//! * the **native** subsumptions come from a single [`crate::reason::reason_all`]
//!   over the bundle as-is — the production, world-partitioned chase `reason-verify`
//!   runs on-gate — with each axiom's own world dropped to `CROSSCHECK_WORLD` and
//!   the result unioned; and
//! * the **oracle** subsumptions come from running [`entail_oracle`] once per
//!   world, each world projected into its own default graph via
//!   `RdfDataset::project_named_graph` (the only graph `purrdf::entail` reads),
//!   with all worlds' pairs unioned.
//!
//! Reasoning per world (rather than collapsing every world into one) is both the
//! honest choice — it never invents a cross-world subsumption neither engine
//! would derive in isolation — and the tractable one: gmeow's native DL post-pass
//! is world-partitioned, so a single-world collapse of the whole bundle is
//! quadratic and blows up, whereas the per-world union matches production cost.
//! Any residual divergence between the two unions is therefore a genuine
//! reasoner/profile difference (native chase vs oracle OWL-RL), not a
//! graph-scoping artifact.

use purrdf::RdfDataset;

use crate::entail_oracle;
use crate::reason::InferredAxiom;
use crate::reason::ledger::{
    DivergenceLedger, LedgerVerdict, build_ledger, compare_subsumption, dl_gap_rows, enforce,
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
/// signal (tableau depth, off this subsumption gate), excluded from the compared
/// subsumption pairs on both sides.
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
/// `rdfs:Resource` — the RDFS universal; `X ⊑ rdfs:Resource` carries no info.
const RDFS_RESOURCE: &str = "http://www.w3.org/2000/01/rdf-schema#Resource";

/// Strip a decoded object display form (`<iri>`) back to a bare IRI.
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
    /// The classified divergence ledger (subsumption + gap rows).
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
/// The `inferred` closure is [`crate::reason::reason_all`]'s payload — the SAME
/// world-partitioned native chase `reason-verify` runs on-gate — so the comparison
/// holds gmeow's native subsumption verdict against the oracle and the whole bundle
/// is reasoned exactly ONCE.
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

/// Run the native ↔ entail-oracle subsumption divergence cross-check.
///
/// `native` is the caller's already-computed reasoning closure (the `reason-verify`
/// shipped or `--fresh` [`crate::result::ReasoningResult`]); `bundle` is the whole
/// committed dataset (all graphs) the oracle reasons over per world's projection
/// (see the module world-alignment note), so both sides see identical asserted
/// axioms. The result folds the subsumption comparison into a [`DivergenceLedger`],
/// carries the strict [`enforce`] verdict (`native ⊇ oracle`), and reports
/// calibration counts.
///
/// # Errors
///
/// Returns `Err` if the world enumeration cannot be built. An oracle
/// materialization error is a HARD FAIL inside [`entail_oracle`] (it panics) — the
/// cross-check never silently downgrades an unclosable graph.
pub fn run_entail_crosscheck(
    native: &crate::result::ReasoningResult,
    bundle: &RdfDataset,
) -> gmeow_errors::Result<CrosscheckOutcome> {
    // The native subsumptions are read from the caller's already-computed reasoning
    // closure — the SAME `reason-verify` shipped/fresh [`crate::result::ReasoningResult`]
    // — so the cross-check adds only the independent oracle sweep and never a second
    // chase. (No `reason_all` runs here: reasoning happens once, in the caller.)
    let native_subs = native_subsumptions(native.inferred());

    let worlds = {
        let store = crate::store::WorldStore::new();
        store.load_dataset(bundle)?;
        store.worlds()
    };
    let source_worlds = worlds.len();

    // Subsumption: native inferred rdfs:subClassOf (union over every world) vs the
    // oracle OWL-RL closure per world, both collapsed to the canonical world. This
    // is the on-gate anti-regression oracle: an independent, conformance-tested
    // engine confirming native derives EVERY standard OWL-RL subsumption (`native ⊇
    // oracle`), replacing — and promoting on-gate — the retired off-gate external
    // subsumption oracle.
    let oracle_subs = oracle_subsumptions(bundle, &worlds);
    let subsumption_rows = compare_subsumption(&native_subs, &oracle_subs);

    // No consistency comparison runs on this fast gate. An INDEPENDENT consistency
    // oracle requires the OWL-Direct tableau ([`entail_oracle::globally_consistent`]):
    // purrdf's OWL-RL `materialize` is a positive-only closure that cannot witness an
    // inconsistency, and the tableau — sound and conformance-tested but NP-hard —
    // is empirically intractable swept per-world across the whole bundle (the same
    // inherent OWL-Direct cost that kept the retired external consistency oracle
    // off-gate, independent of implementation). Native's own consistency verdict (`reason_all`'s
    // `is_consistent`) remains gated on-gate by `reason-verify`; the independent
    // tableau oracle stays a unit-tested capability, not a 89-world on-gate sweep.
    let gap_rows = dl_gap_rows(&[]);

    let ledger = build_ledger(subsumption_rows, Vec::new(), gap_rows, Vec::new());
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
        // :A ⊑ :B ⊑ :C — both engines must derive the transitive :A ⊑ :C, so the
        // subsumption ledger is pure Agree and the verdict passes.
        let ds = world_dataset(&[
            ("A", RDF_TYPE, OWL_CLASS),
            ("B", RDF_TYPE, OWL_CLASS),
            ("C", RDF_TYPE, OWL_CLASS),
            ("A", SUBCLASS_OF, "B"),
            ("B", SUBCLASS_OF, "C"),
        ]);
        let native = crate::reason::reason_all(ds.as_ref()).expect("native reasons");
        let outcome = run_entail_crosscheck(&native, ds.as_ref()).expect("cross-check runs");

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

        // The on-gate cross-check compares subsumptions only — no consistency row.
        assert!(
            !outcome
                .ledger
                .rows
                .iter()
                .any(|r| r.category == "consistency"),
            "the fast on-gate cross-check emits no consistency rows: {:#?}",
            outcome.ledger.rows
        );
    }

    #[test]
    fn disjointness_empty_class_is_not_flagged_by_the_owlrl_subsumption_oracle() {
        // :X ⊑ :Y, :X ⊑ :Z, :Y disjointWith :Z makes :X provably empty ONLY under the
        // OWL-Direct tableau. This on-gate cross-check is an OWL-RL SUBSUMPTION oracle
        // (a sound POSITIVE forward closure with no clash detection), so it neither
        // derives :X ⊑ owl:Nothing nor runs a consistency comparison: the asserted
        // :X⊑:Y and :X⊑:Z survive as ordinary Agree subsumptions and the verdict
        // passes. (The tableau-depth class-unsatisfiability check lives in — and is
        // exercised by —
        // `entail_oracle::consistency_detects_disjointness_class_unsatisfiability`.)
        let ds = world_dataset(&[
            ("X", RDF_TYPE, OWL_CLASS),
            ("Y", RDF_TYPE, OWL_CLASS),
            ("Z", RDF_TYPE, OWL_CLASS),
            ("X", SUBCLASS_OF, "Y"),
            ("X", SUBCLASS_OF, "Z"),
            ("Y", OWL_DISJOINT_WITH, "Z"),
        ]);
        let native = crate::reason::reason_all(ds.as_ref()).expect("native reasons");
        let outcome = run_entail_crosscheck(&native, ds.as_ref()).expect("cross-check runs");

        assert_eq!(
            outcome.ledger.oracle_only, 0,
            "no oracle-only rows: {:#?}",
            outcome.ledger.rows
        );
        assert_eq!(outcome.ledger.dl_gap, 0, "no native DL coverage gap");
        assert!(
            outcome.verdict.passed,
            "native ⊇ oracle passes: {:?}",
            outcome.verdict
        );
        let agrees: Vec<(&str, &str)> = outcome
            .ledger
            .rows
            .iter()
            .filter(|r| r.category == "subsumption" && r.kind == DivergenceKind::Agree)
            .map(|r| (r.subject.as_str(), r.object.as_str()))
            .collect();
        for (s, o) in [("X", "Y"), ("X", "Z")] {
            assert!(
                agrees.contains(&(iri(s).as_str(), iri(o).as_str())),
                "{s} ⊑ {o} is an Agree row: {agrees:?}"
            );
        }
        // No class is spuriously reported unsatisfiable and no consistency row is
        // emitted at this depth.
        assert!(
            !outcome
                .ledger
                .rows
                .iter()
                .any(|r| r.category == "consistency" || r.object == "owl:Nothing"),
            "no consistency / owl:Nothing rows on the subsumption gate: {:#?}",
            outcome.ledger.rows
        );
    }
}
