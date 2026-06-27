// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Executable typed-formalization-governance checks over the reasoned graph.
//!
//! Makes `design/LOGIC-FOUNDATION.md` §Typed formalization governance executable:
//!
//! * **Non-entailment obligations** — every `logic:NonEntailmentObligation` names a
//!   forbidden predicate the foundation must never derive, checked by two arms:
//!   - **Arm A — syntactic reachability** ([`foundation::head_predicate_iris`]): a
//!     forbidden predicate that is no rule head is unreachable and the obligation is
//!     `logic:ObligationDischarged`; one that IS a rule head is
//!     `logic:ObligationViolated` and a hard error.
//!   - **Arm B — finite closure**: an obligation declaring
//!     `logic:DischargeFiniteClosure` is checked against the *derived* (non-EDB) edge
//!     set of the materialized closure — if its forbidden predicate was *derived*, the
//!     obligation is violated. This deliberately inspects only DERIVED edges, so an
//!     asserted, properly-attributed fact (e.g. a `gmeow:deceptiveIntentClaim` an
//!     assessor stated) is never mistaken for an entailed one. (A predicate with a
//!     legitimate derivation — e.g. the symmetric `gmeow:counterpartOf` — therefore
//!     does NOT use this arm; its transitive-specific finite-closure check is the
//!     `queries/verify/non-entailment-counterpart.rq` negative test in [`crate::verify`].)
//!
//!   An obligation declaring a discharge condition the engine does not wire (anything
//!   other than syntactic-reachability or finite-closure) is a hard error, never a
//!   silent `unknown` — so an unwired condition can never be mistaken for a pass.
//! * **Per-category coverage** — the `logic:FormalizationCandidate` population, bucketed
//!   by `logic:candidateCategory` and cross-tabulated by `logic:candidateLifecycle`,
//!   reported per category (never one global %). A candidate with no closed-set
//!   category is a hard error (fail-fast), never a silently dropped row.
//!
//! Both run native over the reasoned `oxigraph::store::Store` the verify pass already
//! builds — Rust authority, surfaced through the already-wired `make verify` gate.

use std::collections::{BTreeMap, BTreeSet};

use oxigraph::model::Term;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use gmeow_diagnostics::{Finding, Severity};

use crate::foundation;

/// The `logic:` namespace prefix (every governance term is `logic:`-namespaced).
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The discharge conditions the engine actually wires. Any other declared condition
/// is a hard error rather than a silent `logic:ObligationUnknown` — an unwired
/// discharge path must never be mistaken for a pass (LOGIC-FOUNDATION.md, §Typed
/// formalization governance).
const WIRED_DISCHARGE: &[&str] = &["DischargeSyntacticReachability", "DischargeFiniteClosure"];

/// A `logic:NonEntailmentObligation` lifted from the reasoned graph.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Obligation {
    /// The obligation individual's IRI.
    iri: String,
    /// The forbidden predicate IRI (the value of `logic:obligationForbiddenPredicate`).
    forbidden_predicate: String,
    /// The declared discharge conditions, as `logic:`-local names (e.g.
    /// `DischargeSyntacticReachability`), sorted.
    discharge_conditions: BTreeSet<String>,
}

/// The local name of a `logic:`-namespaced IRI (`logic:DischargeX` → `DischargeX`),
/// or the whole string if it is not in the `logic:` namespace.
fn logic_local(iri: &str) -> &str {
    iri.strip_prefix(LOGIC_NS).unwrap_or(iri)
}

/// Run a SELECT query and return each solution as a name→term map (variable names
/// without the leading `?`). Determinism: callers sort whatever they derive.
fn select(store: &Store, sparql: &str) -> Result<Vec<BTreeMap<String, Term>>, String> {
    let results = SparqlEvaluator::new()
        .parse_query(sparql)
        .map_err(|e| format!("governance query parse error: {e}"))?
        .on_store(store)
        .execute()
        .map_err(|e| format!("governance query evaluation error: {e}"))?;
    let solutions = match results {
        QueryResults::Solutions(solutions) => solutions,
        QueryResults::Boolean(_) | QueryResults::Graph(_) => {
            return Err("governance query must be a SPARQL SELECT".to_owned());
        }
    };
    let mut rows = Vec::new();
    for sol in solutions {
        let sol = sol.map_err(|e| format!("governance query solution error: {e}"))?;
        let mut row = BTreeMap::new();
        for (var, term) in sol.iter() {
            row.insert(var.as_str().to_owned(), term.clone());
        }
        rows.push(row);
    }
    Ok(rows)
}

/// The string value of a term: the IRI for a named node, the lexical value for a
/// literal, or the blank-node id. Used to read forbidden-predicate literals and IRIs
/// uniformly.
fn term_value(term: &Term) -> String {
    match term {
        Term::NamedNode(n) => n.as_str().to_owned(),
        Term::Literal(l) => l.value().to_owned(),
        Term::BlankNode(b) => b.as_str().to_owned(),
        Term::Triple(_) => String::new(),
    }
}

/// Lift every `logic:NonEntailmentObligation` from the store.
fn parse_obligations(store: &Store) -> Result<Vec<Obligation>, String> {
    let rows = select(
        store,
        "PREFIX logic: <https://blackcatinformatics.ca/logic/>
         SELECT ?o ?pred ?cond WHERE {
           ?o a logic:NonEntailmentObligation ;
              logic:obligationForbiddenPredicate ?pred .
           OPTIONAL { ?o logic:obligationDischargeCondition ?cond }
         }",
    )?;
    let mut by_iri: BTreeMap<String, Obligation> = BTreeMap::new();
    for row in rows {
        let Some(iri_term) = row.get("o") else {
            continue;
        };
        let iri = term_value(iri_term);
        let forbidden = row.get("pred").map(term_value).unwrap_or_default();
        let entry = by_iri.entry(iri.clone()).or_insert_with(|| Obligation {
            iri,
            forbidden_predicate: forbidden,
            discharge_conditions: BTreeSet::new(),
        });
        if let Some(cond) = row.get("cond") {
            entry
                .discharge_conditions
                .insert(logic_local(&term_value(cond)).to_owned());
        }
    }
    Ok(by_iri.into_values().collect())
}

/// Arm A — syntactic reachability. Returns a violation finding iff the obligation's
/// forbidden predicate is a foundation rule head (and is therefore derivable).
fn check_reachability(obligation: &Obligation, heads: &BTreeSet<String>) -> Option<Finding> {
    if heads.contains(&obligation.forbidden_predicate) {
        let mut finding = Finding::new(
            Severity::Error,
            "verify.non-entailment.violated",
            format!(
                "non-entailment obligation <{}> VIOLATED: forbidden predicate <{}> is a foundation \
                 rule head and is therefore derivable (logic:ObligationViolated)",
                obligation.iri, obligation.forbidden_predicate
            ),
        )
        .with_tool("verify");
        finding.tags = vec![
            "formalization-governance".to_owned(),
            "non-entailment".to_owned(),
        ];
        return Some(finding);
    }
    None
}

/// Arm B — finite closure. For an obligation declaring `logic:DischargeFiniteClosure`,
/// returns a violation finding iff its forbidden predicate appears among the DERIVED
/// (non-EDB) edges of the materialized closure. Asserted edges are excluded by
/// construction, so an attributed, hand-asserted fact never trips this — only an
/// entailed one does.
fn check_finite_closure(
    obligation: &Obligation,
    derived_predicates: &BTreeSet<String>,
) -> Option<Finding> {
    if !obligation
        .discharge_conditions
        .contains("DischargeFiniteClosure")
    {
        return None;
    }
    if derived_predicates.contains(&obligation.forbidden_predicate) {
        let mut finding = Finding::new(
            Severity::Error,
            "verify.non-entailment.derived",
            format!(
                "non-entailment obligation <{}> VIOLATED: forbidden predicate <{}> appears as a \
                 DERIVED edge in the materialized closure (logic:ObligationViolated)",
                obligation.iri, obligation.forbidden_predicate
            ),
        )
        .with_tool("verify");
        finding.tags = vec![
            "formalization-governance".to_owned(),
            "non-entailment".to_owned(),
        ];
        return Some(finding);
    }
    None
}

/// The hard-error check for an obligation declaring a discharge condition the engine
/// does not wire. Empty conditions are also an error (an obligation must declare how
/// it is discharged).
fn check_discharge_conditions(obligation: &Obligation) -> Vec<Finding> {
    let mut findings = Vec::new();
    if obligation.discharge_conditions.is_empty() {
        findings.push(
            Finding::new(
                Severity::Error,
                "verify.non-entailment.no-discharge",
                format!(
                    "non-entailment obligation <{}> declares no logic:obligationDischargeCondition; \
                     it has no executable discharge path",
                    obligation.iri
                ),
            )
            .with_tool("verify"),
        );
    }
    for cond in &obligation.discharge_conditions {
        if !WIRED_DISCHARGE.contains(&cond.as_str()) {
            findings.push(
                Finding::new(
                    Severity::Error,
                    "verify.non-entailment.unwired-discharge",
                    format!(
                        "non-entailment obligation <{}> declares discharge condition logic:{cond}, \
                         which the engine does not wire (only syntactic-reachability and \
                         finite-closure are executable); no executable discharge path",
                        obligation.iri
                    ),
                )
                .with_tool("verify"),
            );
        }
    }
    findings
}

/// Run the executable non-entailment obligation checks over the reasoned store:
/// Arm A (syntactic reachability over the foundation rule strata), Arm B (finite
/// closure over the `derived_predicates` — the predicate IRIs of the materialized
/// closure's DERIVED, non-EDB edges), and the unwired-discharge hard error. Findings
/// are sorted by code+message for determinism.
///
/// # Errors
///
/// Returns `Err` if a governance query fails to parse or evaluate.
pub fn check_non_entailment_obligations(
    store: &Store,
    derived_predicates: &BTreeSet<String>,
) -> Result<Vec<Finding>, String> {
    let obligations = parse_obligations(store)?;
    let heads = foundation::head_predicate_iris();
    let mut findings = Vec::new();
    for obligation in &obligations {
        findings.extend(check_discharge_conditions(obligation));
        if let Some(violation) = check_reachability(obligation, &heads) {
            findings.push(violation);
        }
        if let Some(violation) = check_finite_closure(obligation, derived_predicates) {
            findings.push(violation);
        }
    }
    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    Ok(findings)
}

/// The eleven closed `logic:FormalizationCategory` local names, in lifecycle-narrative
/// order — used as the deterministic coverage-report row order and as the validity set
/// for the uncategorized hard-fail.
const CATEGORIES: &[&str] = &[
    "CategoryEquivalenceDefinition",
    "CategoryNecessaryCondition",
    "CategorySufficientCondition",
    "CategoryIntegrityConstraint",
    "CategoryDerivationRule",
    "CategoryDefeasibleDefault",
    "CategoryTypicality",
    "CategoryRecommendation",
    "CategoryNonEntailmentObligation",
    "CategoryDeliberateOverlap",
    "CategoryDocumentationOnly",
];

/// The four closed `logic:CandidateLifecycleState` local names, in lifecycle order.
const LIFECYCLE_STATES: &[&str] = &[
    "CandidateProposed",
    "CandidateUnderReview",
    "CandidateAccepted",
    "CandidateRejected",
];

/// Produce the per-category formalization-candidate coverage report.
///
/// Returns a hard-error finding for every candidate whose `logic:candidateCategory` is
/// absent or outside the closed eleven-member set (fail-fast, never silently dropped),
/// plus one deterministic `note` finding whose detail is the per-category × per-lifecycle
/// breakdown. The counts themselves are report-only (a coverage count is not a pass/fail);
/// the uncategorized check is the hard enforcer.
///
/// # Errors
///
/// Returns `Err` if a governance query fails to parse or evaluate.
pub fn formalization_coverage(store: &Store) -> Result<Vec<Finding>, String> {
    let valid_categories: BTreeSet<&str> = CATEGORIES.iter().copied().collect();
    let rows = select(
        store,
        "PREFIX logic: <https://blackcatinformatics.ca/logic/>
         SELECT ?c ?cat ?life WHERE {
           ?c a logic:FormalizationCandidate .
           OPTIONAL { ?c logic:candidateCategory ?cat }
           OPTIONAL { ?c logic:candidateLifecycle ?life }
         }",
    )?;

    // category local-name -> (lifecycle local-name -> count)
    let mut buckets: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut findings = Vec::new();
    // Collapse multiple rows per candidate (a candidate has one category/lifecycle, but
    // the OPTIONAL join fans out one row per (cat, life) combination when the data is
    // well-formed, and additionally produces duplicated rows when malformed).
    // candidateCategory is single-valued by spec; a second DISTINCT category on the same
    // candidate is malformed data and is a hard error, not a silent first-wins collapse.
    let mut per_candidate: BTreeMap<String, (Option<String>, Option<String>)> = BTreeMap::new();
    // Tracks candidates whose candidateCategory violated the single-value constraint so
    // we emit exactly one deterministic Finding per offender (BTreeSet → sorted order).
    let mut multi_category_violations: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        let Some(c) = row.get("c") else { continue };
        let candidate = term_value(c);
        let cat = row
            .get("cat")
            .map(|t| logic_local(&term_value(t)).to_owned());
        let life = row
            .get("life")
            .map(|t| logic_local(&term_value(t)).to_owned());
        let entry = per_candidate
            .entry(candidate.clone())
            .or_insert((None, None));
        match (&entry.0, &cat) {
            (Some(existing), Some(incoming)) if existing != incoming => {
                multi_category_violations.insert(candidate);
            }
            (None, _) => {
                entry.0 = cat;
            }
            _ => {}
        }
        if entry.1.is_none() {
            entry.1 = life;
        }
    }
    for candidate in &multi_category_violations {
        findings.push(
            Finding::new(
                Severity::Error,
                "verify.formalization.multi-category",
                format!(
                    "formalization candidate <{candidate}> has multiple distinct \
                     logic:candidateCategory values; candidateCategory is single-valued \
                     by spec — each candidate must carry exactly one category",
                ),
            )
            .with_tool("verify"),
        );
    }

    for (candidate, (cat, life)) in &per_candidate {
        match cat {
            Some(cat) if valid_categories.contains(cat.as_str()) => {
                let life = life
                    .clone()
                    .unwrap_or_else(|| "unstated-lifecycle".to_owned());
                *buckets
                    .entry(cat.clone())
                    .or_default()
                    .entry(life)
                    .or_insert(0) += 1;
            }
            other => {
                let shown = other.clone().unwrap_or_else(|| "(none)".to_owned());
                findings.push(
                    Finding::new(
                        Severity::Error,
                        "verify.formalization.uncategorized",
                        format!(
                            "formalization candidate <{candidate}> has no closed-set \
                             logic:candidateCategory (found: {shown}); every candidate must carry \
                             one of the eleven categories",
                        ),
                    )
                    .with_tool("verify"),
                );
            }
        }
    }
    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));

    // Deterministic per-category report: every category row, even zero-count ones, in
    // the canonical category order, with the lifecycle breakdown in canonical order.
    let mut detail = Vec::new();
    let mut total = 0usize;
    for cat in CATEGORIES {
        let life_counts = buckets.get(*cat);
        let cat_total: usize = life_counts.map_or(0, |m| m.values().sum());
        total += cat_total;
        let mut parts = Vec::new();
        for state in LIFECYCLE_STATES {
            let n = life_counts
                .and_then(|m| m.get(*state))
                .copied()
                .unwrap_or(0);
            parts.push(format!("{state}={n}"));
        }
        detail.push(format!("{cat}: total={cat_total} [{}]", parts.join(" ")));
    }
    let mut note = Finding::new(
        Severity::Note,
        "verify.formalization.coverage",
        format!(
            "formalization-candidate coverage: {total} candidate(s) across {} categories \
             (per-category counts, not one global %)",
            CATEGORIES.len()
        ),
    )
    .with_tool("verify");
    note.detail = Some(detail.join("; "));
    note.tags = vec!["formalization-governance".to_owned(), "coverage".to_owned()];
    findings.push(note);
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obligation(iri: &str, pred: &str, conds: &[&str]) -> Obligation {
        Obligation {
            iri: iri.to_owned(),
            forbidden_predicate: pred.to_owned(),
            discharge_conditions: conds.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn standing_obligations_discharge_on_the_real_strata() {
        // The two standing obligations forbid assertion-only gmeow: predicates, which
        // are not foundation rule heads → no violation.
        let heads = foundation::head_predicate_iris();
        let counterpart = obligation(
            "ex:counterpart",
            "https://blackcatinformatics.ca/gmeow/counterpartOf",
            &["DischargeSyntacticReachability"],
        );
        let deception = obligation(
            "ex:deception",
            "https://blackcatinformatics.ca/gmeow/deceptiveIntentClaim",
            &["DischargeSyntacticReachability", "DischargeFiniteClosure"],
        );
        assert!(check_reachability(&counterpart, &heads).is_none());
        assert!(check_reachability(&deception, &heads).is_none());
        assert!(check_discharge_conditions(&counterpart).is_empty());
        assert!(check_discharge_conditions(&deception).is_empty());
    }

    #[test]
    fn arm_a_red_path_synthetic_transitive_counterpart() {
        // If a (synthetic) rule made counterpartOf a derived head, the obligation is
        // VIOLATED and a hard error — proving the check fails red, not just declares.
        let mut heads = foundation::head_predicate_iris();
        heads.insert("https://blackcatinformatics.ca/gmeow/counterpartOf".to_owned());
        let counterpart = obligation(
            "ex:counterpart",
            "https://blackcatinformatics.ca/gmeow/counterpartOf",
            &["DischargeSyntacticReachability"],
        );
        let finding = check_reachability(&counterpart, &heads).expect("must fire");
        assert_eq!(finding.severity, Severity::Error);
        assert!(finding.code.contains("non-entailment.violated"));
    }

    #[test]
    fn arm_b_finite_closure_green_then_red() {
        let pred = "https://blackcatinformatics.ca/gmeow/deceptiveIntentClaim";
        let deception = obligation(
            "ex:deception",
            pred,
            &["DischargeSyntacticReachability", "DischargeFiniteClosure"],
        );
        // Green: the forbidden predicate is NOT among the derived edges (the foundation
        // never derives it; an asserted, attributed intent claim is EDB, not derived).
        let empty = BTreeSet::new();
        assert!(check_finite_closure(&deception, &empty).is_none());
        // Red: a (synthetic) derivation of the forbidden predicate trips the obligation.
        let mut derived = BTreeSet::new();
        derived.insert(pred.to_owned());
        let finding = check_finite_closure(&deception, &derived).expect("must fire");
        assert_eq!(finding.severity, Severity::Error);
        assert!(finding.code.contains("non-entailment.derived"));
    }

    #[test]
    fn arm_b_skipped_without_finite_closure_condition() {
        // counterpart declares only syntactic-reachability, so the derived-edge arm
        // does NOT apply to it — its legitimate symmetric derivation must not trip it.
        let pred = "https://blackcatinformatics.ca/gmeow/counterpartOf";
        let counterpart = obligation("ex:counterpart", pred, &["DischargeSyntacticReachability"]);
        let mut derived = BTreeSet::new();
        derived.insert(pred.to_owned());
        assert!(check_finite_closure(&counterpart, &derived).is_none());
    }

    #[test]
    fn unwired_discharge_condition_is_a_hard_error() {
        // Declaring a discharge condition the engine does not wire is an error, never a
        // silent unknown.
        let obl = obligation(
            "ex:bounded",
            "https://blackcatinformatics.ca/gmeow/somePredicate",
            &["DischargeBoundedCorpus"],
        );
        let findings = check_discharge_conditions(&obl);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].code.contains("unwired-discharge"));
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn missing_discharge_condition_is_a_hard_error() {
        let obl = obligation(
            "ex:bare",
            "https://blackcatinformatics.ca/gmeow/somePredicate",
            &[],
        );
        let findings = check_discharge_conditions(&obl);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].code.contains("no-discharge"));
    }

    /// Build a minimal in-memory Store with N-Triples and return it for query tests.
    fn store_from_ntriples(ntriples: &str) -> Store {
        use oxigraph::io::RdfFormat;
        let store = Store::new().expect("in-memory store");
        store
            .load_from_slice(RdfFormat::NTriples, ntriples)
            .expect("load N-Triples");
        store
    }

    #[test]
    fn duplicate_category_on_candidate_is_a_hard_error() {
        // A candidate that carries two DISTINCT candidateCategory values is malformed;
        // candidateCategory is single-valued by spec. The function must emit exactly one
        // error Finding with the multi-category code and the report must not be ok().
        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let logic = "https://blackcatinformatics.ca/logic/";
        let ntriples = format!(
            "<https://ex/cand1> <{logic}candidateCategory> <{logic}CategoryDerivationRule> .\n\
             <https://ex/cand1> <{logic}candidateCategory> <{logic}CategoryIntegrityConstraint> .\n\
             <https://ex/cand1> <{rdf_type}> <{logic}FormalizationCandidate> .\n",
        );
        let store = store_from_ntriples(&ntriples);
        let findings = formalization_coverage(&store).expect("query must not error");
        let errors: Vec<_> = findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .collect();
        assert_eq!(
            errors.len(),
            1,
            "expected exactly one error finding; got {errors:?}"
        );
        assert!(
            errors[0].code.contains("multi-category"),
            "error code must contain 'multi-category'; got {:?}",
            errors[0].code
        );
        assert!(
            errors[0].message.contains("https://ex/cand1"),
            "error message must name the offending candidate; got {:?}",
            errors[0].message
        );
        // The presence of any error-severity Finding means the run is not ok.
        let has_error = findings.iter().any(|f| f.severity == Severity::Error);
        assert!(
            has_error,
            "findings must contain at least one error for malformed multi-category candidate"
        );
    }

    #[test]
    fn single_category_candidate_produces_correct_coverage() {
        // A well-formed candidate with a single valid category must not produce any
        // error finding; the coverage note must count it under the right category.
        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let logic = "https://blackcatinformatics.ca/logic/";
        let ntriples = format!(
            "<https://ex/cand2> <{rdf_type}> <{logic}FormalizationCandidate> .\n\
             <https://ex/cand2> <{logic}candidateCategory> <{logic}CategoryDerivationRule> .\n\
             <https://ex/cand2> <{logic}candidateLifecycle> <{logic}CandidateAccepted> .\n",
        );
        let store = store_from_ntriples(&ntriples);
        let findings = formalization_coverage(&store).expect("query must not error");
        let errors: Vec<_> = findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "no errors expected for well-formed candidate; got {errors:?}"
        );
        let note = findings
            .iter()
            .find(|f| f.code == "verify.formalization.coverage")
            .expect("coverage note must be present");
        let detail = note.detail.as_deref().unwrap_or("");
        assert!(
            detail.contains("CategoryDerivationRule: total=1"),
            "coverage detail must count the single candidate under CategoryDerivationRule; got {detail:?}",
        );
    }
}
