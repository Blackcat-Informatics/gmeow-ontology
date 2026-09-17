// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The reasoner-derived `math:` dimensional-homogeneity gate.
//!
//! Compiles the two builtin-bound-consequent `logic:Constraint`s authored in
//! `slices/grounding/math/module.ttl` (`math:DimensionalHomogeneityConstraint`,
//! `math:IntegralDimensionCompositionConstraint`) into VIOLATION-EMITTING forward
//! `EvalRule`s ([`crate::relational_core::lower_constraint_violation_rules`]) and drives
//! them through the native forward semi-naive chase over the SAME dataset `verify()` is
//! checking, so a `math:DimensionalInhomogeneity` marker is REASONER-derived from the
//! authored laws, never a Rust side-channel decision.
//!
//! # Why a standalone chase, not `reason_program`
//!
//! The production reasoning entrypoints ([`crate::reason::reason_all`],
//! [`crate::reason::reason_program`]) require a NAMED-GRAPH world
//! ([`crate::reason::build_edb_facts`] silently drops default-graph quads — a
//! deliberate multi-world-DL scoping decision elsewhere in the reasoner, untouched
//! here) and, for `reason_program`, drop literal-object quads from the typed EDB fact
//! stream entirely (the DL/EL calculi never need a literal). The dimension-gate builtins
//! need BOTH: the data `verify()` checks is typically default-graph Turtle (as its own
//! fixtures are), and the exact-rational exponent cells
//! (`math:exponentNumerator`/`math:exponentDenominator`) are literal facts a builtin
//! reads on demand. This module therefore promotes every quad (default OR named graph)
//! into ONE canonical scratch world — exactly as the SPARQL verify queries already
//! flatten every quad into one graph — and drives `physical::materialize_native`
//! directly over a literal-preserving [`crate::store::WorldStore`]
//! ([`crate::store::WorldStore::load_dataset`], the SAME full-fidelity load
//! [`crate::reason::run_nary_head_chase`] already uses for its n-ary head chase), rather
//! than through the literal-dropping [`crate::reason::build_edb_facts`] typed-fact-set
//! path.
//!
//! # Module and [`dimension_gate_markers`] visibility
//!
//! Both are `pub`, not `pub(crate)`: the production verify path consumes them through
//! [`crate::verify::verify_with_reasoning_result`], while `gmeow-slicetest` executes
//! the declarative math cells through the same entry point during the explicit cached
//! slice-spec producer. No test calls this function to construct a corpus.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue};

use crate::physical::{NativeOutcome, compile_cached, materialize_native};
use crate::rule_ir::EvalRule;
use crate::store::WorldStore;

/// Wrap a math-dimension-gate condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn math_gate_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// The `math:` slice module, embedded at compile time (the same convention
/// `crates/logic/build.rs` uses for the verify query set): production `verify()` never
/// reads `slices/` off disk at runtime.
pub(crate) const MATH_MODULE_TTL: &str =
    include_str!("../../../../slices/grounding/math/module.ttl");
/// The `math:` slice module's canonical source IRI (provenance only).
pub(crate) const MATH_MODULE_SOURCE_IRI: &str = "https://blackcatinformatics.ca/gmeow/slices/math";
/// `rdf:type` — the marker triple's predicate.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdfs:seeAlso` — the HiLog-reflection → object-level-property bridge predicate.
const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";
/// The canonical scratch named graph every caller-supplied quad (default OR named
/// graph) is promoted into before the forward chase runs — a single world, so the
/// world-indexed engine (which only reasons over named-graph worlds) sees the whole
/// dataset regardless of how its source graphs were named. Never leaks into a caller's
/// data: it exists only inside this module's own transient [`WorldStore`].
const MATH_GATE_WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/math-dimension-gate";
/// The plan-cache contract-hash namespace for the compiled violation rules — distinct
/// from [`crate::reason::native_contract_hash`]'s DL/EL contract, so the two never
/// collide in the shared process-wide plan cache.
const MATH_GATE_CONTRACT: &str =
    "https://blackcatinformatics.ca/gmeow/reason/math-dimension-gate/v1";

/// The exact-rational dimension-cell predicates the `=:=` / `⊕` dimension builtins walk
/// on demand (via [`crate::physical::builtin_eval::load_dimension_cells`]) to project a
/// dimension IRI's ℚ⁷ exponent vector out of the store. Kept in the EDB projection below
/// because the compiled rule bodies never name them (a builtin reads them directly), so
/// deriving the read-set from the rules alone would miss them.
const DIMENSION_CELL_PREDICATES: [&str; 4] = [
    "https://blackcatinformatics.ca/math/baseDimensionExponent",
    "https://blackcatinformatics.ca/math/exponentOfDimension",
    "https://blackcatinformatics.ca/math/exponentNumerator",
    "https://blackcatinformatics.ca/math/exponentDenominator",
];

/// The two dimension classes the ℚ⁷ cell-builtin classifies a dimension node by (a
/// `math:DerivedDimension` has walkable exponent cells; a `math:Dimensionless` is the ℚ⁷
/// zero vector) — the ONLY `rdf:type` objects the gate reads. An `rdf:type` triple whose
/// object is neither is inert to the gate and is dropped by the projection.
const DIMENSION_TYPE_OBJECTS: [&str; 2] = [
    "https://blackcatinformatics.ca/math/DerivedDimension",
    "https://blackcatinformatics.ca/math/Dimensionless",
];

/// Repository-free callers share one immutable embedded law identity. Producer
/// verification passes its own prepared rules explicitly, while unit tests hydrate
/// the exact producer-selected native law record without compiling module sources.
fn compiled_rules() -> &'static [EvalRule] {
    &crate::verify::prepared_gates::shared().math_rules
}

/// Lower the already-compiled native math module, sharing the caller's source and IR.
pub(crate) fn prepare_rules(
    source: &RdfDataset,
    program: &gmeow_logic_compile::ir::LogicProgram,
) -> gmeow_errors::Result<Vec<EvalRule>> {
    let see_also = reflection_see_also_map(source);
    crate::relational_core::lower_constraint_violation_rules(program, &see_also)
}

/// Read every `rdfs:seeAlso` triple of the source graph into a substitution map:
/// HiLog-reflection relation IRI → the real object-level property the asserted data
/// carries. The dimension-gate laws predicate over the reflection relations
/// (`math:homogeneousOperandRel`, `math:hasDimensionRel`, `math:integrandRel`,
/// `math:withRespectToRel`) inside their `logic:Formula` ASTs, but real data (the
/// fixtures, the shipped examples) asserts the object-level property
/// (`math:homogeneousOperand`, …) as the actual triple predicate — this bridges the two
/// so the lowered antecedent body matches real triples, never a second, never-asserted
/// reflection-relation-keyed data source. Scans every `rdfs:seeAlso` triple (not just the
/// four known ones), so a future reflected relation is picked up the same way — never a
/// hardcoded four-entry table.
fn reflection_see_also_map(source: &RdfDataset) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for quad in source.owned_quads() {
        if quad.predicate != RDFS_SEE_ALSO {
            continue;
        }
        if let (RdfTerm::Iri(subject), RdfTerm::Iri(object)) = (&quad.subject, &quad.object) {
            map.entry(subject.clone()).or_insert_with(|| object.clone());
        }
    }
    map
}

/// The object-level predicates the compiled violation rules' antecedent bodies actually
/// match, unioned with the [`DIMENSION_CELL_PREDICATES`] the ℚ⁷ builtins read directly.
///
/// Deriving the antecedent predicates from the compiled `rules` (not a hardcoded list)
/// keeps the EDB projection below correct for any future dimension law that predicates
/// over a new property — the projection then automatically keeps that property's triples.
fn gate_read_predicates(rules: &[EvalRule]) -> BTreeSet<String> {
    let mut preds: BTreeSet<String> = DIMENSION_CELL_PREDICATES
        .iter()
        .map(|p| (*p).to_owned())
        .collect();
    for rule in rules {
        for atom in &rule.body {
            preds.insert(atom.predicate.clone());
        }
    }
    preds
}

/// Promote the dimension-relevant quads of the reasoned closure — every default-graph or
/// named-graph assertion of `edb`, including native statement-layer assertions,
/// PLUS the DL-`derived` non-EDB edges the reasoner layered onto
/// the reasoned graph — into the single canonical [`MATH_GATE_WORLD`], preserving every
/// term (literals included).
///
/// Both sources are filtered by the same projection and re-graphed identically, so a
/// dimension triple is gated whether it was asserted or derived — matching the verify
/// queries that evaluate the same closure.
///
/// Two reasons the whole quad set is NOT promoted verbatim:
/// 1. The world-indexed engine only reasons over named-graph worlds, so a caller's plain
///    default-graph Turtle (the common case for `verify()`'s own fixtures) must be
///    re-graphed to participate at all.
/// 2. **Projection.** The gate's rules and ℚ⁷ cell-builtins read ONLY the predicates in
///    `keep_predicates` plus the `rdf:type` triples that classify a dimension node
///    ([`DIMENSION_TYPE_OBJECTS`]); every other triple is inert to the gate — its rules
///    never match it and its builtins never read it — so dropping it cannot change any
///    materialized marker. Over a ~100k-triple whole-bundle `verify()` this is the
///    difference between a bounded chase and a pathological one, with identical results.
///    The `derived` set is small (only the reasoner's non-EDB output), so unioning it in
///    adds negligibly to the projected chase.
///
/// # Errors
///
/// Returns `Err` if the promoted dataset fails the freeze-time structural contract.
fn promote_to_single_world(
    edb: &RdfDataset,
    derived: &[RdfQuad],
    keep_predicates: &BTreeSet<String>,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    // A dimension triple is kept iff its predicate is one the rules/builtins read, or it
    // is an `rdf:type` classifying a node as a dimension the ℚ⁷ builtin walks.
    let keep = |predicate: &str, object: &RdfTerm| -> bool {
        keep_predicates.contains(predicate)
            || (predicate == RDF_TYPE
                && matches!(object, RdfTerm::Iri(o) if DIMENSION_TYPE_OBJECTS.contains(&o.as_str())))
    };
    let mut builder = RdfDatasetBuilder::new();
    for quad in purrdf::native_quads::flat_rdf_quads(edb) {
        if !keep(&quad.predicate, &quad.object) {
            continue;
        }
        let promoted = RdfQuad::new(quad.subject, quad.predicate, quad.object)
            .in_graph(RdfTerm::iri(MATH_GATE_WORLD));
        builder.push_owned_quad(&promoted);
    }
    // The reasoner's DL-derived (non-EDB) edges — the same edges the verify queries see —
    // so a derived dimension triple is gated too, not only the asserted ones.
    for quad in derived {
        if !keep(&quad.predicate, &quad.object) {
            continue;
        }
        let promoted = RdfQuad::new(
            quad.subject.clone(),
            quad.predicate.clone(),
            quad.object.clone(),
        )
        .in_graph(RdfTerm::iri(MATH_GATE_WORLD));
        builder.push_owned_quad(&promoted);
    }
    builder.freeze().map_err(|e| {
        math_gate_err(format!(
            "promote the reasoned closure into the math dimension-gate scratch world: {e}"
        ))
    })
}

#[cfg(test)]
#[path = "math_gate_routing_tests.rs"]
mod routing_tests;

/// Decode a materialized row's subject to a bare IRI — every dimension-gate violation
/// marker's subject is a `math:DimensionalExpression` or `math:Integral` node (always an
/// IRI or Skolemized blank node in the reified sense, never a literal).
fn subject_iri(term: &TermValue) -> gmeow_errors::Result<String> {
    match term {
        TermValue::Iri(iri) => Ok(iri.clone()),
        other => Err(math_gate_err(format!(
            "math dimension-gate: a violation marker's subject must be an IRI, got {other:?}"
        ))),
    }
}

/// Run the reasoner-derived `math:` dimensional-homogeneity gate over the reasoned closure
/// (`edb` UNIONED with the DL-`derived` non-EDB edges the caller layered onto the reasoned
/// graph), returning every materialized `(subject, failure_class)` marker pair —
/// deduplicated, sorted — ready to be inserted as `subject rdf:type failure_class` quads.
///
/// Taking the derived edges (not just the raw EDB) is what keeps a *derived* dimension
/// triple — a `math:hasDimension` / `math:homogeneousOperand` / integral part reached by
/// subproperty or class inference — inside the hard-fail gate's domain, matching the
/// verify queries that evaluate the same closure. Literal exponent cells are never
/// DL-derived, so the derived set is all-IRI and loses nothing versus the asserted EDB.
///
/// Returns an empty vector when the embedded module authors no builtin-bound-consequent
/// constraint (never reached in production — the two `math:` constraints always compile)
/// and, ordinarily, whenever no violation exists in the reasoned closure.
///
/// # Errors
///
/// Returns `Err` if the promoted closure fails to freeze, if the compiled violation rules
/// are (unexpectedly) not stratifiable, or if the native forward chase declines the
/// program — every case a genuine internal-invariant failure, never a silent empty result
/// standing in for an error.
pub fn dimension_gate_markers(
    edb: &RdfDataset,
    derived: &[RdfQuad],
) -> gmeow_errors::Result<Vec<(String, String)>> {
    dimension_gate_markers_with_rules(edb, derived, compiled_rules())
}

pub(crate) fn dimension_gate_markers_with_rules(
    edb: &RdfDataset,
    derived: &[RdfQuad],
    rules: &[EvalRule],
) -> gmeow_errors::Result<Vec<(String, String)>> {
    if rules.is_empty() {
        return Ok(Vec::new());
    }

    let keep_predicates = gate_read_predicates(rules);
    let promoted = promote_to_single_world(edb, derived, &keep_predicates)?;
    let store = WorldStore::from_dataset(promoted.as_ref())?;

    let lookup = compile_cached(MATH_GATE_CONTRACT, rules.to_vec());
    let Some(executable) = lookup.executable else {
        return Err(math_gate_err(
            "math dimension-gate: the compiled violation rules are not stratifiable — an \
             internal invariant is violated (the two authored laws are single-stratum, \
             negation-free Horn-shaped rules)"
                .to_owned(),
        ));
    };

    let outcome = materialize_native(&store, executable.as_ref(), None)?;
    let budgeted = match outcome {
        NativeOutcome::Decided(budgeted) => budgeted,
        NativeOutcome::Unsupported(kind) => {
            return Err(math_gate_err(format!(
                "math dimension-gate: the native forward chase declined the compiled \
                 violation rules ({kind:?})"
            )));
        }
    };

    let mut markers: Vec<(String, String)> = Vec::new();
    for row in budgeted.rows {
        // Drop the echoed-EDB rows: a violation marker is always chase-DERIVED, and the
        // caller's data never asserts `a math:DimensionalInhomogeneity` directly.
        if row.rule_iri == crate::provenance::ASSERT_RULE_IRI {
            continue;
        }
        if row.predicate != RDF_TYPE {
            continue;
        }
        let TermValue::Iri(class) = &row.object else {
            continue;
        };
        markers.push((subject_iri(&row.subject)?, class.clone()));
    }
    markers.sort();
    markers.dedup();
    Ok(markers)
}

#[path = "math_gate.tests.rs"]
#[cfg(test)]
mod tests;
