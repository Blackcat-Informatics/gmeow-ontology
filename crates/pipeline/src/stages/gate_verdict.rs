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
//! production surface `crates/conformance/tests/diagnostics_gate_morphism.rs` proves
//! equal to the single Rust `gate()` policy.

use std::collections::BTreeMap;

use gmeow_logic::reason::reason_program;
use gmeow_logic_compile::frontend::parse_logic_dataset;
use gmeow_logic_compile::ir::{LogicProgram, LogicRule};
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{
    NativeRdfFormat, RdfDatasetBuilder, RdfQuad, RdfTerm, SparqlEngine, SparqlRequest,
    SparqlResult, TermValue, dataset_from_bytes,
};

use gmeow_ns::GMEOW_NS;
/// The derived predicate and its Fatal value — the head of `logic:ruleGateFatalVerdict`.
const FINDING_GATE_VERDICT: &str = "https://blackcatinformatics.ca/gmeow/findingGateVerdict";
const GATE_FATAL: &str = "https://blackcatinformatics.ca/gmeow/gateFatal";
const CATEGORY_BLOCKING: &str = "https://blackcatinformatics.ca/gmeow/categoryBlocking";
/// The single named-graph world the encoded grades + wiring live in for the chase. The
/// chase reads facts out of named-graph worlds; a plain default-graph fact is invisible
/// to it by design, so the whole EDB (categoryBlocking AND the finding grades) must be
/// world-scoped into the SAME world.
const WORLD: &str =
    "https://blackcatinformatics.ca/gmeow/graph/diagnostics-gate-verdict-derivation";

/// The authored gate-verdict derivation, extracted ONCE from the source graph: the
/// isolated `logic:ruleGateFatalVerdict` rule plus the `gmeow:categoryBlocking`
/// category→Blocking-disposition wiring the rule joins against. Reasoning any projected
/// finding graph against this reproduces the ontology's derived verdict for the shipped
/// bundle.
pub struct GateProgram {
    rule: LogicRule,
    category_blocking: BTreeMap<String, String>,
}

impl GateProgram {
    /// Parse the authored `logic:ruleGateFatalVerdict` rule and the
    /// `gmeow:categoryBlocking` map out of the authored source graph N-Quads (the
    /// validate stage's `BASE_GRAPH_PATH` bytes, which carry the logic + diagnostics
    /// slices in the default graph).
    ///
    /// Returns `None` when the source graph does not carry the authored rule — a source
    /// without it derives nothing, so the projection stays byte-unchanged. A malformed
    /// source graph (which the pipeline's own base-graph never is) also yields `None`;
    /// the caller then ships the projection unchanged rather than fabricate a verdict.
    pub fn from_source(source_nquads: &[u8]) -> Option<GateProgram> {
        let dataset = dataset_from_bytes(source_nquads, NativeRdfFormat::NQuads).ok()?;
        let (program, _diags) = parse_logic_dataset(dataset.as_ref(), None).ok()?;
        let rule = program
            .rules
            .into_iter()
            .find(|r| r.head.predicate == FINDING_GATE_VERDICT)?;

        let engine = NativeSparqlEngine::new();
        let result = engine
            .query(
                &dataset,
                SparqlRequest {
                    query: &format!("SELECT ?cat ?b WHERE {{ ?cat <{CATEGORY_BLOCKING}> ?b . }}"),
                    base_iri: None,
                    substitutions: &[],
                },
            )
            .ok()?;
        let (variables, rows) = match result {
            SparqlResult::Solutions {
                variables, rows, ..
            } => (variables, rows),
            _ => return None,
        };
        let cat_idx = variables.iter().position(|v| v == "cat")?;
        let b_idx = variables.iter().position(|v| v == "b")?;
        let mut category_blocking = BTreeMap::new();
        for sol in &rows {
            let cat = iri_of(sol.get(cat_idx).and_then(|t| t.as_ref())?)?;
            let b = iri_of(sol.get(b_idx).and_then(|t| t.as_ref())?)?;
            category_blocking.insert(cat, b);
        }
        // A rule with no wiring to join against can never derive a verdict — that is a
        // hollow source, not the shipped one. Treat it as absent so nothing is faked.
        if category_blocking.is_empty() {
            return None;
        }
        Some(GateProgram {
            rule,
            category_blocking,
        })
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
        let tuples = grade_tuples(finding_nq)?;
        if tuples.is_empty() {
            return Ok(String::new());
        }

        let program = LogicProgram::new(Vec::new(), vec![self.rule.clone()], Vec::new(), None);

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
        let result = reason_program(&program, edb.as_ref()).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Scoreboard {
                message: format!("reason gate-verdict rule: {e}"),
            })
        })?;

        // A derived row's IRI object renders in N-Triples display form (`<IRI>`); the
        // subject is the bare IRI. Emit one canonical N-Quad line per derived verdict,
        // sorted for determinism.
        let gate_fatal_display = format!("<{GATE_FATAL}>");
        let mut lines: Vec<String> = result
            .inferred()
            .iter()
            .filter(|a| a.predicate == FINDING_GATE_VERDICT && a.object == gate_fatal_display)
            .map(|a| {
                format!(
                    "<{}> <{FINDING_GATE_VERDICT}> <{GATE_FATAL}> <{graph_iri}> .",
                    a.subject
                )
            })
            .collect();
        lines.sort();
        lines.dedup();
        if lines.is_empty() {
            Ok(String::new())
        } else {
            let mut out = lines.join("\n");
            out.push('\n');
            Ok(out)
        }
    }
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
fn grade_tuples(nq: &str) -> gmeow_errors::Result<Vec<(String, String, String, String)>> {
    let sb = |message: String| gmeow_errors::Diag::of_kind(crate::error::Scoreboard { message });
    let dataset = dataset_from_bytes(nq.as_bytes(), NativeRdfFormat::NQuads)
        .map_err(|e| sb(format!("parse diagnostics N-Quads: {e}")))?;
    let engine = NativeSparqlEngine::new();
    let q = format!(
        "SELECT ?f ?sev ?cat ?sp WHERE {{ GRAPH ?g {{ \
           ?f <{GMEOW_NS}findingSeverity> ?sev ; \
              <{GMEOW_NS}findingCategory> ?cat ; \
              <{GMEOW_NS}findingStandpoint> ?sp . }} }}"
    );
    let result = engine
        .query(
            &dataset,
            SparqlRequest {
                query: &q,
                base_iri: None,
                substitutions: &[],
            },
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal authored source graph carrying the gate rule + the categoryBlocking
    /// wiring, in the DEFAULT graph exactly like the pipeline base-graph bytes. The rule
    /// mirrors the authored `logic:head`/`logic:body` reified-triple shape of
    /// `slices/grounding/logic/module.ttl`'s `logic:ruleGateFatalVerdict` (never a
    /// string body).
    const SOURCE_GRAPH: &str = concat!(
        // categoryBlocking wiring: one blocking category (DataShapeViolation) and one
        // coherent (PolicyWarning), enough to drive both a gating and a non-gating case.
        "<https://blackcatinformatics.ca/logic/FindingDataShapeViolation> ",
        "<https://blackcatinformatics.ca/gmeow/categoryBlocking> ",
        "<https://blackcatinformatics.ca/gmeow/blockingBlocking> .\n",
        "<https://blackcatinformatics.ca/logic/FindingPolicyWarning> ",
        "<https://blackcatinformatics.ca/gmeow/categoryBlocking> ",
        "<https://blackcatinformatics.ca/gmeow/blockingCoherent> .\n",
        // The authored up-set derivation rule: logic:Rule with a reified head and four
        // reified body atoms (severity, category, categoryBlocking join, standpoint).
        "<https://blackcatinformatics.ca/logic/ruleGateFatalVerdict> ",
        "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ",
        "<https://blackcatinformatics.ca/logic/Rule> .\n",
        // head: findingGateVerdict(?finding, gateFatal)
        "<https://blackcatinformatics.ca/logic/ruleGateFatalVerdict> ",
        "<https://blackcatinformatics.ca/logic/head> _:h .\n",
        "_:h <http://www.w3.org/1999/02/22-rdf-syntax-ns#subject> \"?finding\" .\n",
        "_:h <http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate> ",
        "<https://blackcatinformatics.ca/gmeow/findingGateVerdict> .\n",
        "_:h <http://www.w3.org/1999/02/22-rdf-syntax-ns#object> ",
        "<https://blackcatinformatics.ca/gmeow/gateFatal> .\n",
        // body atom 1: findingSeverity(?finding, severityError)
        "<https://blackcatinformatics.ca/logic/ruleGateFatalVerdict> ",
        "<https://blackcatinformatics.ca/logic/body> _:b1 .\n",
        "_:b1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#subject> \"?finding\" .\n",
        "_:b1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate> ",
        "<https://blackcatinformatics.ca/gmeow/findingSeverity> .\n",
        "_:b1 <http://www.w3.org/1999/02/22-rdf-syntax-ns#object> ",
        "<https://blackcatinformatics.ca/gmeow/severityError> .\n",
        // body atom 2: findingCategory(?finding, ?category)
        "<https://blackcatinformatics.ca/logic/ruleGateFatalVerdict> ",
        "<https://blackcatinformatics.ca/logic/body> _:b2 .\n",
        "_:b2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#subject> \"?finding\" .\n",
        "_:b2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate> ",
        "<https://blackcatinformatics.ca/gmeow/findingCategory> .\n",
        "_:b2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#object> \"?category\" .\n",
        // body atom 3: categoryBlocking(?category, blockingBlocking)
        "<https://blackcatinformatics.ca/logic/ruleGateFatalVerdict> ",
        "<https://blackcatinformatics.ca/logic/body> _:b3 .\n",
        "_:b3 <http://www.w3.org/1999/02/22-rdf-syntax-ns#subject> \"?category\" .\n",
        "_:b3 <http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate> ",
        "<https://blackcatinformatics.ca/gmeow/categoryBlocking> .\n",
        "_:b3 <http://www.w3.org/1999/02/22-rdf-syntax-ns#object> ",
        "<https://blackcatinformatics.ca/gmeow/blockingBlocking> .\n",
        // body atom 4: findingStandpoint(?finding, standpointBinding)
        "<https://blackcatinformatics.ca/logic/ruleGateFatalVerdict> ",
        "<https://blackcatinformatics.ca/logic/body> _:b4 .\n",
        "_:b4 <http://www.w3.org/1999/02/22-rdf-syntax-ns#subject> \"?finding\" .\n",
        "_:b4 <http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate> ",
        "<https://blackcatinformatics.ca/gmeow/findingStandpoint> .\n",
        "_:b4 <http://www.w3.org/1999/02/22-rdf-syntax-ns#object> ",
        "<https://blackcatinformatics.ca/gmeow/standpointBinding> .\n",
    );

    /// Two findings in the graph/diagnostics named graph: one up-set (Error /
    /// DataShapeViolation / Binding) that MUST derive gateFatal, and one non-up-set
    /// (Error / PolicyWarning / Binding — coherent category) that must derive nothing.
    fn finding_nq() -> String {
        let g = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
        let gm = GMEOW_NS;
        let logic = "https://blackcatinformatics.ca/logic/";
        let upset = "https://blackcatinformatics.ca/gmeow/examples/diagnostics/upset";
        let coherent = "https://blackcatinformatics.ca/gmeow/examples/diagnostics/coherent";
        format!(
            "<{upset}> <{gm}findingSeverity> <{gm}severityError> <{g}> .\n\
             <{upset}> <{gm}findingCategory> <{logic}FindingDataShapeViolation> <{g}> .\n\
             <{upset}> <{gm}findingStandpoint> <{gm}standpointBinding> <{g}> .\n\
             <{coherent}> <{gm}findingSeverity> <{gm}severityError> <{g}> .\n\
             <{coherent}> <{gm}findingCategory> <{logic}FindingPolicyWarning> <{g}> .\n\
             <{coherent}> <{gm}findingStandpoint> <{gm}standpointBinding> <{g}> .\n"
        )
    }

    #[test]
    fn derives_gatefatal_only_for_the_upset_finding() {
        let gate = GateProgram::from_source(SOURCE_GRAPH.as_bytes())
            .expect("the source graph carries the authored logic:ruleGateFatalVerdict + wiring");
        let graph = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
        let derived = gate
            .derived_verdict_nquads(&finding_nq(), graph)
            .expect("derivation must succeed over well-formed finding N-Quads");

        let lines: Vec<&str> = derived.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(
            lines.len(),
            1,
            "exactly one gateFatal must be derived (the up-set finding only): {derived:?}"
        );
        let line = lines[0];
        assert!(
            line.starts_with("<https://blackcatinformatics.ca/gmeow/examples/diagnostics/upset>"),
            "the derived verdict must be for the up-set finding: {line}"
        );
        assert!(
            line.contains("<https://blackcatinformatics.ca/gmeow/findingGateVerdict>")
                && line.contains("<https://blackcatinformatics.ca/gmeow/gateFatal>"),
            "the derived line must carry findingGateVerdict gateFatal: {line}"
        );
        assert!(
            line.ends_with("<https://blackcatinformatics.ca/gmeow/graph/diagnostics> ."),
            "the derived verdict must land in the findings' graph: {line}"
        );
        assert!(
            !line.contains("/coherent>"),
            "the coherent (non-up-set) finding must NOT be derived gateFatal: {derived}"
        );
    }

    #[test]
    fn absent_rule_yields_none() {
        // categoryBlocking wiring but NO gate rule → nothing to derive, byte-unchanged.
        let no_rule = concat!(
            "<https://blackcatinformatics.ca/logic/FindingDataShapeViolation> ",
            "<https://blackcatinformatics.ca/gmeow/categoryBlocking> ",
            "<https://blackcatinformatics.ca/gmeow/blockingBlocking> .\n",
        );
        assert!(
            GateProgram::from_source(no_rule.as_bytes()).is_none(),
            "a source without the authored gate rule must yield None"
        );
    }
}
