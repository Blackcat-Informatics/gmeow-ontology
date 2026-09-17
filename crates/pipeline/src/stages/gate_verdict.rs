// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reasoner-derived `gmeow:findingGateVerdict` materialization for the shipped
//! diagnostics graph.
//!
//! `gmeow_errors::render::to_gmeow_rdf` deliberately emits ONLY the three grade-axis
//! coordinates of each finding (`gmeow:findingSeverity`, `gmeow:findingCategory`,
//! `gmeow:findingStandpoint`) and NEVER the derived verdict — the verdict is defined
//! by the ontology as an ENTAILMENT of the authored `logic:ruleGateFatalVerdict`
//! up-set rule, not a hand-asserted property. This module closes the loop for the
//! SHIPPED bundle: it runs that AUTHORED rule (via the native chase `reason_program`,
//! never the Rust `gate()` morphism) over the projected finding grades and returns the
//! derived `gmeow:findingGateVerdict gmeow:gateFatal` N-Quads so the diagnostics
//! renderer can canonicalize them into both the byte artifact and the carrier graph.
//! Without it, an up-set finding (Error / blocking category / Binding) rides the
//! diagnostics graph missing its verdict and `gmeow:GateFatalUpsetShape` fires under
//! the authored-source `make validate` / stage-validate SHACL pass.
//!
//! The rule and the `gmeow:categoryBlocking` map are READ from the authored source
//! graph (the validate stage's base-graph bytes), never re-typed here — exactly the
//! production surface `conformance::corpus_tests::diagnostics_gate_morphism` proves
//! equal to the single Rust `gate()` policy.

use gmeow_logic::reason::{
    DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, prepare_reasoning_input,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_logic::reason::reason_program;
use gmeow_logic_compile::frontend::{
    CompiledTheory, OwnerDisposition, OwnerFamily, PreparedLogicSource, default_source_statements,
};
use gmeow_logic_compile::ir::LogicProgram;
use purrdf::sparql::{NativeSparqlEngine, PreparedQuery, QueryOptions};
use purrdf::{
    NativeRdfFormat, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, SparqlResult, TermRef,
    TermValue, dataset_from_bytes,
};

use gmeow_ns::GMEOW_NS;
/// The derived predicate and its Fatal value — the head of `logic:ruleGateFatalVerdict`.
const FINDING_GATE_VERDICT: &str = "https://blackcatinformatics.ca/gmeow/findingGateVerdict";
const GATE_FATAL: &str = "https://blackcatinformatics.ca/gmeow/gateFatal";
const CATEGORY_BLOCKING: &str = "https://blackcatinformatics.ca/gmeow/categoryBlocking";
/// The selected diagnostic theory containing both encoded grades and wiring.
/// Keeping categoryBlocking and finding grades in this one declared world isolates
/// the gate derivation from unrelated object-level theories.
const WORLD: &str =
    "https://blackcatinformatics.ca/gmeow/graph/diagnostics-gate-verdict-derivation";

/// The authored gate-verdict derivation, extracted ONCE from the source graph: the
/// isolated `logic:ruleGateFatalVerdict` rule plus the `gmeow:categoryBlocking`
/// category→Blocking-disposition wiring the rule joins against. Reasoning any projected
/// finding graph against this reproduces the ontology's derived verdict for the shipped
/// bundle.
pub struct GateProgram {
    program: LogicProgram,
    category_blocking: BTreeMap<String, String>,
    engine: NativeSparqlEngine,
    grade_query: Arc<PreparedQuery>,
}

impl GateProgram {
    /// Parse the authored `logic:ruleGateFatalVerdict` rule and the
    /// `gmeow:categoryBlocking` map out of the authored source graph N-Quads (the
    /// validate stage's `BASE_GRAPH_PATH` bytes, which carry the logic + diagnostics
    /// slices in the default graph).
    ///
    /// Returns `None` when the source graph does not carry the authored rule — a source
    /// without it derives nothing, so the projection stays byte-unchanged.
    ///
    /// # Errors
    /// Malformed source, ambiguous rules or missing wiring for a declared rule fail closed.
    pub fn from_source(source_nquads: &[u8]) -> gmeow_errors::Result<Option<Self>> {
        let dataset = dataset_from_bytes(source_nquads, NativeRdfFormat::NQuads)
            .map_err(|error| gate_error(format!("parse gate rule source: {error}")))?;
        Self::from_dataset(&dataset)
    }

    /// Read the authored rule and category wiring from the existing native source carrier.
    ///
    /// # Errors
    /// Invalid source or incomplete declared gate semantics fail closed.
    pub fn from_dataset(dataset: &Arc<RdfDataset>) -> gmeow_errors::Result<Option<Self>> {
        let theory = PreparedLogicSource::new(dataset)?.into_compiled(None)?;
        Self::from_compiled_theory(&theory)
    }

    /// Reuse the exact source program and native category wiring retained by the
    /// producer without reparsing, canonicalizing or lowering the source.
    ///
    /// # Errors
    /// Invalid or ambiguous selected gate semantics fail closed.
    pub fn from_compiled_theory(theory: &CompiledTheory) -> gmeow_errors::Result<Option<Self>> {
        Self::from_compiled_theory_with_wiring(theory, theory.source().dataset())
    }

    /// Reuse the admitted rule compilation with separately admitted category wiring.
    ///
    /// # Errors
    /// Rejected source owners and missing, ambiguous or malformed wiring fail closed.
    pub fn from_compiled_theory_with_wiring(
        theory: &CompiledTheory,
        wiring: &RdfDataset,
    ) -> gmeow_errors::Result<Option<Self>> {
        for owner in theory.owner_lowerings() {
            if owner.family == OwnerFamily::Rule
                && owner.source.graph.is_none()
                && matches!(
                    theory.source().dataset().resolve(owner.source.term),
                    TermRef::Iri("https://blackcatinformatics.ca/logic/ruleGateFatalVerdict")
                )
            {
                let valid = match owner.disposition {
                    OwnerDisposition::Emitted { index } => {
                        theory.program().rules[index].head.predicate == FINDING_GATE_VERDICT
                    }
                    OwnerDisposition::Rejected | OwnerDisposition::OutsideDefaultGraph => false,
                };
                if !valid {
                    return Err(gate_error(
                        "declared ruleGateFatalVerdict did not emit its required gate rule",
                    ));
                }
            }
        }
        Self::from_program(theory.program(), wiring)
    }

    /// Reuse the compiler's admitted program instead of lowering the source again.
    /// The category wiring still comes from the exact authored source carrier.
    ///
    /// # Errors
    /// Ambiguous or incomplete gate semantics and query preparation failures are errors.
    pub fn from_program(
        program: &LogicProgram,
        dataset: &RdfDataset,
    ) -> gmeow_errors::Result<Option<Self>> {
        let mut rules = program
            .rules
            .iter()
            .filter(|r| r.head.predicate == FINDING_GATE_VERDICT);
        let Some(rule) = rules.next() else {
            return Ok(None);
        };
        if rules.next().is_some() {
            return Err(gate_error("multiple authored findingGateVerdict rules"));
        }

        let mut category_blocking = BTreeMap::new();
        if let Some(predicate) = dataset.term_id_by_iri(CATEGORY_BLOCKING) {
            for quad in default_source_statements(dataset, None, Some(predicate), None) {
                let (TermRef::Iri(cat), TermRef::Iri(blocking)) =
                    (dataset.resolve(quad.s), dataset.resolve(quad.o))
                else {
                    return Err(gate_error("gate category wiring must use bound IRIs"));
                };
                if category_blocking
                    .insert(cat.to_owned(), blocking.to_owned())
                    .is_some_and(|old| old != blocking)
                {
                    return Err(gate_error(
                        "gate category has conflicting blocking dispositions",
                    ));
                }
            }
        }
        // A rule with no wiring to join against can never derive a verdict — that is a
        // hollow declared operation, so it must fail instead of skipping the verdicts.
        if category_blocking.is_empty() {
            return Err(gate_error(
                "authored gate rule has no categoryBlocking wiring",
            ));
        }
        let engine = NativeSparqlEngine::new();
        let grade_query = engine
            .prepare_query(
                &format!(
                    "SELECT ?f ?sev ?cat ?sp WHERE {{ GRAPH ?g {{ \
               ?f <{GMEOW_NS}findingSeverity> ?sev ; \
                  <{GMEOW_NS}findingCategory> ?cat ; \
                  <{GMEOW_NS}findingStandpoint> ?sp . }} }}"
                ),
                None,
            )
            .map_err(|error| gate_error(format!("prepare finding grade query: {error}")))?;
        Ok(Some(GateProgram {
            program: LogicProgram::new(Vec::new(), vec![rule.clone()], Vec::new(), None),
            category_blocking,
            engine,
            grade_query,
        }))
    }

    /// Run the authored gate rule over the grade tuples the projected diagnostics
    /// `finding_nq` (N-Quads) carries and return the derived
    /// `<finding> <findingGateVerdict> <gateFatal> <graph_iri> .` N-Quad lines (empty
    /// string when the reasoner derives none). `graph_iri` is the graph the findings
    /// live in, so the derived triples land in the same named graph.
    ///
    /// Hard-fails (`Err`) on a malformed `finding_nq` or a chase failure — never a
    /// silent fallback.
    pub fn derived_verdict_nquads(
        &self,
        finding_nq: &str,
        graph_iri: &str,
    ) -> gmeow_errors::Result<String> {
        let dataset = dataset_from_bytes(finding_nq.as_bytes(), NativeRdfFormat::NQuads)
            .map_err(|error| gate_error(format!("parse diagnostics N-Quads: {error}")))?;
        let verdicts = self.derived_verdict_dataset(&dataset, graph_iri)?;
        purrdf::canonical_flat_nquads(&verdicts)
            .map_err(|error| gate_error(format!("render derived gate verdicts: {error}")))
    }

    /// Derive verdicts over the existing carrier without rendering or reparsing it.
    ///
    /// # Errors
    /// Invalid finding coordinates, chase failures and invalid output terms fail closed.
    pub fn derived_verdict_dataset(
        &self,
        findings: &Arc<RdfDataset>,
        graph_iri: &str,
    ) -> gmeow_errors::Result<Arc<RdfDataset>> {
        let mut output = RdfDatasetBuilder::new();
        for subject in self.derived_fatal(findings)? {
            output.push_owned_quad(
                &RdfQuad::new(
                    RdfTerm::iri(subject),
                    FINDING_GATE_VERDICT,
                    RdfTerm::iri(GATE_FATAL),
                )
                .in_graph(RdfTerm::iri(graph_iri)),
            );
        }
        output
            .freeze()
            .map_err(|error| gate_error(format!("freeze gate verdicts: {error}")))
    }

    /// The exact authored category mapping used by the retained gate program.
    pub(crate) fn category_blocking(&self) -> &BTreeMap<String, String> {
        &self.category_blocking
    }

    /// Derive the fatal finding identities without materializing a projection dataset.
    pub(crate) fn derived_fatal(
        &self,
        findings: &Arc<RdfDataset>,
    ) -> gmeow_errors::Result<BTreeSet<String>> {
        let tuples = grade_tuples(self, findings)?;
        if tuples.is_empty() {
            return Ok(BTreeSet::new());
        }
        let mut builder = RdfDatasetBuilder::new();
        for (cat, b) in &self.category_blocking {
            push_triple(&mut builder, cat, CATEGORY_BLOCKING, b);
        }
        for (f, sev, cat, sp) in &tuples {
            push_triple(&mut builder, f, &format!("{GMEOW_NS}findingSeverity"), sev);
            push_triple(&mut builder, f, &format!("{GMEOW_NS}findingCategory"), cat);
            push_triple(&mut builder, f, &format!("{GMEOW_NS}findingStandpoint"), sp);
        }
        let edb = builder.freeze().map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Scoreboard {
                message: format!("freeze gate-verdict EDB: {e}"),
            })
        })?;
        let reasoning_input = prepare_reasoning_input(&edb)?;
        let domains = SelectedDomains::new([SelectedLogicalWorld::new(
            LogicalGraph::Named(purrdf::TermValue::iri(WORLD)),
            DomainProfile::NonemptyObjectDomainV1,
            "gmeow.pipeline.gate-verdict.v1".to_owned(),
            *reasoning_input.ingress_contract(),
        )?])?;
        let result = reason_program(&self.program, reasoning_input, &domains).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Scoreboard {
                message: format!("reason gate-verdict rule: {e}"),
            })
        })?;

        // Match the typed verdict resource without rendering the inferred object.
        Ok(result
            .inferred()
            .iter()
            .filter(|a| {
                !a.is_edb
                    && a.predicate == FINDING_GATE_VERDICT
                    && a.object.as_iri() == Some(GATE_FATAL)
            })
            .map(|a| a.subject.clone())
            .collect())
    }
}

fn gate_error(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Scoreboard {
        message: message.into(),
    })
}

/// The IRI string of a bound SPARQL term, or `None` if it is not an IRI.
fn iri_of(term: &TermValue) -> Option<String> {
    match term {
        TermValue::Iri(i) => Some(i.clone()),
        _ => None,
    }
}

/// Push one IRI triple into the chase world graph.
fn push_triple(builder: &mut RdfDatasetBuilder, s: &str, p: &str, o: &str) {
    let quad = RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(WORLD));
    builder.push_owned_quad(&quad);
}

/// Extract every `(finding, severity, category, standpoint)` grade tuple SPARQL sees in
/// the projected diagnostics graph — the exact three coordinates the up-set rule reads.
/// Findings ride a named graph (`graph/diagnostics`), so the pattern is world-scoped.
fn grade_tuples(
    gate: &GateProgram,
    dataset: &Arc<RdfDataset>,
) -> gmeow_errors::Result<Vec<(String, String, String, String)>> {
    let sb = |message: String| gmeow_errors::Diag::of_kind(crate::error::Scoreboard { message });
    let result = gate
        .engine
        .query_prepared(dataset, &gate.grade_query, &[], QueryOptions::EMPTY)
        .map_err(|e| sb(format!("grade-tuple query: {e}")))?;
    let (variables, rows) = match result {
        SparqlResult::Solutions {
            variables, rows, ..
        } => (variables, rows),
        _ => return Err(sb("grade-tuple query must be a SELECT".to_owned())),
    };
    let col = |n: &str| {
        variables
            .iter()
            .position(|v| v == n)
            .ok_or_else(|| sb(format!("grade-tuple query missing column {n}")))
    };
    let (fi, si, ci, pi) = (col("f")?, col("sev")?, col("cat")?, col("sp")?);
    let bound_iri =
        |sol: &[Option<TermValue>], idx: usize, name: &str| -> gmeow_errors::Result<String> {
            sol.get(idx)
                .and_then(|t| t.as_ref())
                .and_then(iri_of)
                .ok_or_else(|| sb(format!("grade term ?{name} must be a bound IRI")))
        };
    let mut out = Vec::with_capacity(rows.len());
    for sol in &rows {
        out.push((
            bound_iri(sol, fi, "f")?,
            bound_iri(sol, si, "sev")?,
            bound_iri(sol, ci, "cat")?,
            bound_iri(sol, pi, "sp")?,
        ));
    }
    Ok(out)
}

#[path = "gate_verdict.tests.rs"]
#[cfg(test)]
mod tests;
