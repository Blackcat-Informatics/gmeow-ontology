// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Execute source-selected quantitative Formula roots through the shared native engine.
//! This evidence checks numerical claims; it never certifies optic laws or rewrites.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_logic_compile::frontend::{CompiledTheory, FormulaDisposition};
use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceComposition, FiniteNumericLiteral, Formula, LOGIC_NAMESPACE,
    LogicProgram, SemanticProfileId, UnitInterval,
};
use gmeow_logic_compile::relational_core::{RcRule, RcTerm, formula_analysis};
use purrdf::{RdfLiteral, TermValue};

use crate::materialize::{Materialization, MaterializationLimits, materialize_prepared_store};
use crate::store::WorldStore;

#[cfg(test)]
mod tests;

const UNSPECIFIED: &str = "https://blackcatinformatics.ca/gmeow/unspecifiedStandpoint";
const AXES: [&str; 4] = ["confidence", "evidenceStrength", "weight", "probability"];
const OUTPUTS: [&str; 4] = [
    "composedConfidence",
    "composedEvidenceStrength",
    "composedWeight",
    "composedProbability",
];
const MEMBERS: [&str; 3] = ["compositionFirst", "compositionSecond", "compositionResult"];

fn iri(local: &str) -> String {
    format!("{LOGIC_NAMESPACE}{local}")
}
fn error(detail: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::exec_error(format!("correspondence axes: {detail}"))
}

/// Explicit result of one selected coordinate; absent inputs are never zero or one.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum AxisResult {
    /// No rule for this axis was selected, and no composite value claimed.
    NotSelected,
    /// The selected rule's premises did not derive a value; no value was claimed.
    NotEvaluated,
    /// Native output, numerically checked against an authored value when present.
    Computed {
        value: FiniteNumericLiteral,
        claimed: bool,
    },
}

/// One immutable source-bound run. Restoring its report does not recreate authority.
#[derive(Debug)]
pub struct SourceAxes {
    source: Arc<CompiledTheory>,
    results: BTreeMap<String, BTreeMap<String, AxisResult>>,
    runs: Vec<AxisRun>,
}

/// Exact selected source roots, context and native derivations retained together.
#[derive(Debug)]
pub struct AxisRun {
    pub rules: Vec<String>,
    pub standpoint: String,
    pub materialization: Materialization,
}

impl SourceAxes {
    pub fn source(&self) -> &Arc<CompiledTheory> {
        &self.source
    }
    pub fn results(&self) -> &BTreeMap<String, BTreeMap<String, AxisResult>> {
        &self.results
    }
    pub fn runs(&self) -> &[AxisRun] {
        &self.runs
    }
    /// Terminal observations only, distinct from source and native execution evidence.
    pub fn report(&self) -> gmeow_errors::Result<Vec<u8>> {
        #[derive(serde::Serialize)]
        struct RunReport<'a> {
            rules: &'a [String],
            standpoint: &'a str,
            derivations: &'a [crate::seam::DerivedQuad],
            preservation: &'a crate::result::PreservationClaim,
            frontier: &'a crate::query_ir::CompletionFrontier,
        }
        #[derive(serde::Serialize)]
        struct Report<'a> {
            results: &'a BTreeMap<String, BTreeMap<String, AxisResult>>,
            runs: Vec<RunReport<'a>>,
        }
        let runs = self
            .runs
            .iter()
            .map(|run| RunReport {
                rules: &run.rules,
                standpoint: &run.standpoint,
                derivations: &run.materialization.quads,
                preservation: &run.materialization.preservation,
                frontier: &run.materialization.frontier,
            })
            .collect();
        serde_json::to_vec_pretty(&Report {
            results: &self.results,
            runs,
        })
        .map_err(error)
    }
}

fn coordinate(cell: &Correspondence, axis: usize) -> Option<&RdfLiteral> {
    match axis {
        0 => cell.confidence.as_ref().map(UnitInterval::literal),
        1 => cell.evidence_strength.as_ref().map(UnitInterval::literal),
        2 => cell.weight.as_ref().map(FiniteNumericLiteral::literal),
        3 => cell.probability.as_ref().map(UnitInterval::literal),
        _ => unreachable!("fixed axis inventory"),
    }
}

fn literal(value: &RdfLiteral) -> TermValue {
    TermValue::Literal {
        lexical_form: value.lexical_form.clone(),
        datatype: value.datatype_iri().to_owned(),
        language: value.language.clone(),
        direction: value.direction,
    }
}

fn insert(
    store: &WorldStore,
    world: &str,
    subject: &str,
    predicate: &str,
    object: TermValue,
) -> gmeow_errors::Result<()> {
    store.insert_quad_terms(
        world,
        TermValue::iri(subject),
        TermValue::iri(iri(predicate)),
        object,
    )
}

fn seed_cell(store: &WorldStore, world: &str, cell: &Correspondence) -> gmeow_errors::Result<()> {
    for (axis, predicate) in AXES.iter().enumerate() {
        if let Some(value) = coordinate(cell, axis) {
            insert(store, world, &cell.iri, predicate, literal(value))?;
        }
    }
    for (predicate, value) in [
        ("evidenceScale", &cell.axis_evidence.scale),
        (
            "crossChainProbabilityModel",
            &cell.axis_evidence.probability_model,
        ),
    ] {
        if let Some(value) = value {
            insert(store, world, &cell.iri, predicate, TermValue::iri(value))?;
        }
    }
    for evidence in &cell.axis_evidence.sources {
        insert(
            store,
            world,
            &cell.iri,
            "evidenceSource",
            TermValue::iri(evidence),
        )?;
    }
    Ok(())
}

fn seed_composition(
    store: &WorldStore,
    world: &str,
    c: &CorrespondenceComposition,
) -> gmeow_errors::Result<()> {
    for (predicate, value) in [
        ("compositionFirst", &c.first),
        ("compositionSecond", &c.second),
        ("compositionResult", &c.composite),
    ] {
        insert(store, world, &c.iri, predicate, TermValue::iri(value))?;
    }
    for rule in &c.axis_rules {
        insert(
            store,
            world,
            &c.iri,
            "compositionAxisRule",
            TermValue::iri(rule),
        )?;
    }
    for (predicate, value) in [
        ("confidenceIndependenceEvidence", &c.confidence_independence),
        (
            "probabilityIndependenceEvidence",
            &c.probability_independence,
        ),
    ] {
        if let Some(value) = value {
            insert(store, world, &c.iri, predicate, TermValue::iri(value))?;
        }
    }
    Ok(())
}

/// Resolve only the exact default-graph source roots recorded by the shared frontend.
/// Alpha-equivalent syntax elsewhere cannot substitute a selected named root.
fn selected_formulas<'a>(
    source: &'a CompiledTheory,
    names: &[String],
) -> gmeow_errors::Result<Vec<&'a Formula>> {
    let dataset = source.source().dataset();
    names
        .iter()
        .map(|name| {
            let term = dataset
                .term_id_by_iri(name)
                .ok_or_else(|| error(format!("missing rule <{name}>")))?;
            let mut matches = source
                .formula_lowerings()
                .iter()
                .filter(|lowering| lowering.source.term == term && lowering.source.graph.is_none());
            let Some(lowering) = matches.next() else {
                return Err(error(format!("unlowered rule <{name}>")));
            };
            if matches.next().is_some() {
                return Err(error(format!("ambiguous rule <{name}>")));
            }
            let FormulaDisposition::Formula(index) = lowering.disposition else {
                return Err(error(format!(
                    "rule <{name}> is not an admitted global Formula"
                )));
            };
            Ok(&source.program().formulas[index])
        })
        .collect()
}

/// Check the quantitative output envelope, not the mathematics of an arbitrary
/// user policy. Every branch must bind its composition and exact selection root.
fn selected_axes(names: &[String], formulas: &[&Formula]) -> gmeow_errors::Result<BTreeSet<usize>> {
    let analyses: Vec<_> = names
        .iter()
        .zip(formulas)
        .map(|(name, formula)| (name, formula_analysis::analyze(formula)))
        .collect();
    let mut axes = BTreeSet::new();
    for (name, analysis) in &analyses {
        if !analysis.residue.is_empty() || analysis.rules.is_empty() {
            return Err(error(format!(
                "rule <{name}> has incomplete lowering: {:?}",
                analysis.residue
            )));
        }
        for rule in &analysis.rules {
            let Some(axis) = OUTPUTS
                .iter()
                .position(|output| rule.head.predicate == iri(output))
            else {
                return Err(error(format!("rule <{name}> has a non-axis head")));
            };
            let RcTerm::Var(_) = &rule.head.subject else {
                return Err(error(format!(
                    "rule <{name}> must bind its composition subject"
                )));
            };
            if rule.head.negated
                || !rule.head_conjuncts.is_empty()
                || rule.has_existential_head()
                || rule.body.iter().any(|atom| atom.negated)
                || !rule.body.iter().any(|atom| {
                    atom.subject == rule.head.subject
                        && atom.predicate == iri("compositionAxisRule")
                        && atom.object == RcTerm::Iri((*name).clone())
                })
                || MEMBERS.iter().any(|predicate| {
                    !rule.body.iter().any(|atom| {
                        atom.subject == rule.head.subject && atom.predicate == iri(predicate)
                    })
                })
            {
                return Err(error(format!(
                    "rule <{name}> lacks a positive, selection-bound composition envelope"
                )));
            }
            for member in ["compositionFirst", "compositionSecond"] {
                let bindings: Vec<_> = rule
                    .body
                    .iter()
                    .filter(|atom| {
                        atom.subject == rule.head.subject && atom.predicate == iri(member)
                    })
                    .collect();
                if !bindings.iter().any(|binding| {
                    rule.body.iter().any(|atom| {
                        atom.subject == binding.object && atom.predicate == iri(AXES[axis])
                    })
                }) {
                    return Err(error(format!(
                        "rule <{name}> must read both member coordinates"
                    )));
                }
            }
            if axis == 0
                && rule.numeric.iter().any(|call| {
                    call.operator == gmeow_logic_compile::relational_core::NumericOperator::Multiply
                })
                && !rule.body.iter().any(|atom| {
                    atom.subject == rule.head.subject
                        && atom.predicate == iri("confidenceIndependenceEvidence")
                })
            {
                return Err(error(format!(
                    "rule <{name}> requires composition-owned confidence independence"
                )));
            }
            axes.insert(axis);
        }
    }
    // A composed-value premise is supplied by selected native rules, never by
    // an authored observation. Collect all heads before checking dependencies so
    // policy IRI order does not decide whether a producer is available.
    for (name, analysis) in &analyses {
        for rule in &analysis.rules {
            check_body_inputs(name, rule, &axes)?;
        }
    }
    Ok(axes)
}

/// Admit exactly the input supplied by `seed_composition` and `seed_cell`, plus
/// composition-local outputs of selected rules. This is a typed input fragment,
/// not a closed-world claim about every predicate in the original RDF source.
/// An unavailable source guard must fail admission even when no value is claimed.
fn check_body_inputs(
    name: &str,
    rule: &RcRule,
    selected: &BTreeSet<usize>,
) -> gmeow_errors::Result<()> {
    // Multiple positive bindings of a functional member relation are faithful
    // aliases: the native join makes them equal. Constants are also local when
    // an explicit member atom connects them to this exact composition.
    let members: BTreeSet<_> = rule
        .body
        .iter()
        .filter(|atom| {
            atom.subject == rule.head.subject
                && atom
                    .predicate
                    .strip_prefix(LOGIC_NAMESPACE)
                    .is_some_and(|local| MEMBERS.contains(&local))
        })
        .map(|atom| &atom.object)
        .collect();
    for atom in &rule.body {
        let local = atom.predicate.strip_prefix(LOGIC_NAMESPACE).unwrap_or("");
        let composition_input = MEMBERS.contains(&local)
            || matches!(
                local,
                "compositionAxisRule"
                    | "confidenceIndependenceEvidence"
                    | "probabilityIndependenceEvidence"
            );
        let member_input = AXES.contains(&local)
            || matches!(
                local,
                "evidenceScale" | "crossChainProbabilityModel" | "evidenceSource"
            );
        let local_subject = if composition_input {
            // Every selected-root fact is supplied; an additional constant or
            // variable root guard is not restricted to this branch's own root.
            atom.subject == rule.head.subject
        } else if member_input {
            members.contains(&atom.subject)
        } else if let Some(axis) = OUTPUTS.iter().position(|output| *output == local) {
            if !selected.contains(&axis) {
                return Err(error(format!(
                    "rule <{name}> reads <{}> without a selected producing rule",
                    atom.predicate
                )));
            }
            atom.subject == rule.head.subject
        } else {
            return Err(error(format!(
                "rule <{name}> requires unsupported source guard <{}>",
                atom.predicate
            )));
        };
        if !local_subject {
            return Err(error(format!(
                "rule <{name}> reads <{}> outside its composition-local input",
                atom.predicate
            )));
        }
    }
    Ok(())
}

fn read_value(value: &TermValue, axis: usize) -> gmeow_errors::Result<FiniteNumericLiteral> {
    let TermValue::Literal {
        lexical_form,
        datatype,
        language,
        direction,
    } = value
    else {
        return Err(error("native axis output is not a numeric literal"));
    };
    let value = RdfLiteral {
        lexical_form: lexical_form.clone(),
        datatype: Some(datatype.clone()),
        language: language.clone(),
        direction: *direction,
    };
    if axis == 2 {
        FiniteNumericLiteral::new(value)
    } else {
        UnitInterval::new(value).map(UnitInterval::into_finite)
    }
}

fn compare_claim(
    cell: &Correspondence,
    axis: usize,
    value: &FiniteNumericLiteral,
) -> gmeow_errors::Result<bool> {
    let claim = match axis {
        0 => cell.confidence.as_ref().map(UnitInterval::value),
        1 => cell.evidence_strength.as_ref().map(UnitInterval::value),
        2 => cell.weight.as_ref().map(FiniteNumericLiteral::value),
        3 => cell.probability.as_ref().map(UnitInterval::value),
        _ => unreachable!("fixed axis inventory"),
    };
    let Some(claim) = claim else {
        return Ok(false);
    };
    if !purrdf::xsd::numeric_cmp(claim, value.value())
        .ok_or_else(|| error("numeric claim comparison is undefined"))?
        .is_eq()
    {
        return Err(error(format!(
            "<{}> {} differs from the declared composition rules",
            cell.iri, AXES[axis]
        )));
    }
    Ok(true)
}

/// Named scales and models are mandatory interpretation inputs even for a
/// user-authored rule. A numeric output cannot invent an absent interpretation.
fn check_interpretation(
    first: &Correspondence,
    second: &Correspondence,
    result: &Correspondence,
    axis: usize,
) -> gmeow_errors::Result<()> {
    let references = match axis {
        1 => [
            &first.axis_evidence.scale,
            &second.axis_evidence.scale,
            &result.axis_evidence.scale,
        ],
        3 => [
            &first.axis_evidence.probability_model,
            &second.axis_evidence.probability_model,
            &result.axis_evidence.probability_model,
        ],
        _ => return Ok(()),
    };
    if references[0].is_none() || references[0] != references[1] || references[0] != references[2] {
        return Err(error(format!(
            "{} requires an explicit shared interpretation on both members and the result",
            AXES[axis]
        )));
    }
    Ok(())
}

/// Execute compatible compositions together without serialization or a temporary RDF
/// dataset. Preparation and clausification reuse their bounded native analyses.
/// Missing or conflicting claims, unsupported rules and incomplete execution fail closed.
pub fn execute(source: Arc<CompiledTheory>) -> gmeow_errors::Result<SourceAxes> {
    let mut cells = BTreeMap::new();
    for cell in &source.program().correspondences {
        if cells.insert(cell.iri.as_str(), cell).is_some() {
            return Err(error("duplicate correspondence identity"));
        }
    }
    let mut groups: BTreeMap<(Vec<String>, String), Vec<&CorrespondenceComposition>> =
        BTreeMap::new();
    let mut identities = BTreeSet::new();
    for c in &source.program().correspondence_compositions {
        if !identities.insert(c.iri.as_str()) {
            return Err(error("duplicate composition identity"));
        }
        let members = [&c.first, &c.second, &c.composite].map(|name| {
            cells
                .get(name.as_str())
                .copied()
                .ok_or_else(|| error(format!("missing member <{name}>")))
        });
        let [first, second, result] = members;
        let (first, second, result) = (first?, second?, result?);
        if first.according_to != second.according_to || first.according_to != result.according_to {
            return Err(error(format!(
                "<{}> requires explicit cross-context transport",
                c.iri
            )));
        }
        groups
            .entry((
                c.axis_rules.clone(),
                first
                    .according_to
                    .as_deref()
                    .unwrap_or(UNSPECIFIED)
                    .to_owned(),
            ))
            .or_default()
            .push(c);
    }
    let mut results = BTreeMap::new();
    let mut runs = Vec::new();
    // Sorted group keys keep a selection's standpoints contiguous. Retain only
    // that selection until its last world, then release it before the next one.
    // The separately shared native preparation cache remains bounded.
    let mut preparations = BTreeMap::new();
    for ((names, world), compositions) in groups {
        if !preparations.contains_key(&names) {
            preparations.clear();
            let formulas = selected_formulas(&source, &names)?;
            let axes = selected_axes(&names, &formulas)?;
            let program = LogicProgram::new(vec![], vec![], vec![], None)
                .with_formulas(formulas.into_iter().cloned().collect());
            let prepared = crate::program_analysis::prepare_program(&program)?;
            preparations.insert(names.clone(), (axes, prepared));
        }
        let (axes, prepared) = &preparations[&names];
        let mut values = BTreeMap::<(String, usize), FiniteNumericLiteral>::new();
        if !names.is_empty() {
            let store = WorldStore::new();
            let mut members = BTreeSet::new();
            let mut admitted = BTreeSet::new();
            for c in &compositions {
                seed_composition(&store, &world, c)?;
                admitted.insert(c.iri.as_str());
                members.extend([c.first.as_str(), c.second.as_str(), c.composite.as_str()]);
            }
            for name in members {
                seed_cell(&store, &world, cells[name])?;
            }
            let materialization = materialize_prepared_store(
                prepared,
                &store,
                MaterializationLimits::default(),
                SemanticProfileId::PositiveHorn,
            )
            .map_err(error)?;
            if !materialization
                .preservation
                .unsupported_constructs
                .is_empty()
                || materialization.frontier.completed != materialization.frontier.total
                || !materialization.non_quad_rows.is_empty()
                || materialization
                    .quads
                    .iter()
                    .any(|quad| quad.budget_status != crate::seam::BudgetStatus::Ok)
            {
                return Err(error("native axis execution is incomplete"));
            }
            for quad in &materialization.quads {
                let Some(axis) = OUTPUTS
                    .iter()
                    .position(|output| quad.predicate == iri(output))
                else {
                    continue;
                };
                let TermValue::Iri(subject) = &quad.subject else {
                    return Err(error("unnamed composition output"));
                };
                if quad.graph != world || !admitted.contains(subject.as_str()) {
                    return Err(error(
                        "native output escaped its selected composition context",
                    ));
                }
                let value = read_value(&quad.object, axis)?;
                let key = (subject.clone(), axis);
                if let Some(previous) = values.insert(key, value.clone())
                    && previous != value
                {
                    return Err(error(
                        "multiple distinct RDF values for one composition axis",
                    ));
                }
            }
            runs.push(AxisRun {
                rules: names,
                standpoint: world,
                materialization,
            });
        }
        for c in compositions {
            let mut row = BTreeMap::new();
            for (axis, axis_name) in AXES.iter().enumerate() {
                let claim = cells[c.composite.as_str()];
                let result = if let Some(value) = values.remove(&(c.iri.clone(), axis)) {
                    check_interpretation(
                        cells[c.first.as_str()],
                        cells[c.second.as_str()],
                        claim,
                        axis,
                    )?;
                    let claimed = compare_claim(claim, axis, &value)?;
                    AxisResult::Computed { value, claimed }
                } else if coordinate(claim, axis).is_some() {
                    return Err(error(format!(
                        "<{}> claims {} without a derived value and its required premises",
                        c.iri, AXES[axis]
                    )));
                } else if axes.contains(&axis) {
                    AxisResult::NotEvaluated
                } else {
                    AxisResult::NotSelected
                };
                row.insert((*axis_name).to_owned(), result);
            }
            results.insert(c.iri.clone(), row);
        }
    }
    Ok(SourceAxes {
        source,
        results,
        runs,
    })
}
