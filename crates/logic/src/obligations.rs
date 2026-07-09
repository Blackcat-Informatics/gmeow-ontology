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
//!
//!   Every violation finding is additionally attributed back to the
//!   `logic:FormalizationCandidate`(s) that declared the obligation via
//!   `logic:candidateNonEntailment` ([`candidates_by_obligation`]): the structural
//!   requirement that a `CategoryNonEntailmentObligation` candidate carry that edge is
//!   enforced separately by `queries/verify/non-entailment-carrier-required.rq`; this
//!   traversal only names the declaring candidate(s) in the finding, which is what
//!   realizes the over-typing review "through the typed candidate lifecycle."
//! * **Per-category coverage** — the `logic:FormalizationCandidate` population, bucketed
//!   by `logic:candidateCategory` and cross-tabulated by `logic:candidateLifecycle`,
//!   reported per category (never one global %). A candidate with no closed-set
//!   category is a hard error (fail-fast), never a silently dropped row.
//!
//! Both run native over the reasoned `Arc<RdfDataset>` the verify pass already
//! builds — Rust authority, surfaced through the already-wired `make verify` gate.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::sync::Arc;

use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfDataset, SparqlEngine, SparqlRequest, SparqlResult, TermValue};
use sha2::{Digest, Sha256};

use gmeow_errors::{Finding, Severity};

use crate::foundation;

/// Wrap a governance-obligation condition message as a typed diagnostic on the
/// shared substrate, preserving the authored text verbatim.
fn obligation_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Obligation { detail })
}

/// The `logic:` namespace prefix (every governance term is `logic:`-namespaced).
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The canonical source language every localizable source literal carries
/// (`@x-gmeow-english`). A `logic:candidateSourceHash` is computed over exactly this
/// lexical form; generators project public `@en`/`@zh`/`@fr`, which must never be the
/// hashed text — so the drift check reads only the source-language literal.
const SOURCE_LANG: &str = "x-gmeow-english";

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
fn select(
    store: &Arc<RdfDataset>,
    sparql: &str,
) -> gmeow_errors::Result<Vec<BTreeMap<String, TermValue>>> {
    let engine = NativeSparqlEngine::new();
    let result = engine
        .query(
            store,
            SparqlRequest {
                query: sparql,
                base_iri: None,
                substitutions: &[],
            },
        )
        .map_err(|e| obligation_err(format!("governance query evaluation error: {e}")))?;
    let (variables, result_rows) = match result {
        SparqlResult::Solutions {
            variables, rows, ..
        } => (variables, rows),
        SparqlResult::Boolean(_) | SparqlResult::Graph(_) => {
            return Err(obligation_err(
                "governance query must be a SPARQL SELECT".to_owned(),
            ));
        }
    };
    let mut rows = Vec::new();
    for sol in &result_rows {
        let mut row = BTreeMap::new();
        for (var, cell) in variables.iter().zip(sol.iter()) {
            // Only bound variables enter the row, mirroring the prior oxigraph
            // `QuerySolution::iter()` (it yields only bound (var, term) pairs).
            if let Some(term) = cell {
                row.insert(var.clone(), term.clone());
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

/// The string value of a term: the IRI for an IRI term, the lexical value for a
/// literal, or the (scope-qualified) blank-node label. Used to read
/// forbidden-predicate literals and IRIs uniformly.
fn term_value(term: &TermValue) -> String {
    match term {
        TermValue::Iri(iri) => iri.clone(),
        TermValue::Literal { lexical_form, .. } => lexical_form.clone(),
        TermValue::Blank { label, scope } => scope.qualify_label(label).into_owned(),
        TermValue::Triple { .. } => String::new(),
    }
}

/// Lift every `logic:NonEntailmentObligation` from the store.
fn parse_obligations(store: &Arc<RdfDataset>) -> gmeow_errors::Result<Vec<Obligation>> {
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

/// Append the declaring-candidate attribution to a violation finding's message and
/// tags, iff `candidates_by_obligation` names one or more `logic:FormalizationCandidate`
/// that declared this obligation via `logic:candidateNonEntailment`. This is the
/// traversal that makes the over-typing review "realized through the typed candidate
/// lifecycle" (LOGIC-FOUNDATION.md, §Typed formalization governance) literally true in
/// this code path: the structural link (a `CategoryNonEntailmentObligation` candidate
/// MUST carry `candidateNonEntailment`) is already enforced by
/// `queries/verify/non-entailment-carrier-required.rq`; this only attributes an already
/// -firing violation back to its declaring candidate(s), so a real candidate population
/// makes the finding text/tags depend on the edge without changing pass/fail.
fn attribute_to_candidates(
    mut finding: Finding,
    obligation_iri: &str,
    candidates_by_obligation: &BTreeMap<String, BTreeSet<String>>,
) -> Finding {
    let Some(candidates) = candidates_by_obligation.get(obligation_iri) else {
        return finding;
    };
    if candidates.is_empty() {
        return finding;
    }
    let joined = candidates
        .iter()
        .map(|c| format!("<{c}>"))
        .collect::<Vec<_>>()
        .join(", ");
    finding.message.push_str(&format!(
        " — declared by formalization candidate(s) {joined} (over-typing surfaced through \
         the typed candidate lifecycle)"
    ));
    finding
        .tags
        .extend(candidates.iter().map(|c| format!("candidate:{c}")));
    finding
}

/// Build the `obligation IRI -> sorted set of declaring candidate IRIs` attribution map
/// by querying every `logic:FormalizationCandidate ; logic:candidateNonEntailment
/// ?obligation` edge. This is the read side of the structural link that
/// `queries/verify/non-entailment-carrier-required.rq` already enforces (a
/// `CategoryNonEntailmentObligation` candidate MUST carry `candidateNonEntailment`);
/// this function does not re-check that constraint, it only harvests the edge for
/// attribution.
fn candidates_by_obligation(
    store: &Arc<RdfDataset>,
) -> gmeow_errors::Result<BTreeMap<String, BTreeSet<String>>> {
    let rows = select(
        store,
        "PREFIX logic: <https://blackcatinformatics.ca/logic/>
         SELECT ?candidate ?o WHERE {
           ?candidate a logic:FormalizationCandidate ;
                      logic:candidateNonEntailment ?o .
         }",
    )?;
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        let (Some(candidate_term), Some(obligation_term)) = (row.get("candidate"), row.get("o"))
        else {
            continue;
        };
        map.entry(term_value(obligation_term))
            .or_default()
            .insert(term_value(candidate_term));
    }
    Ok(map)
}

/// Arm A — syntactic reachability. Returns a violation finding iff the obligation's
/// forbidden predicate is a foundation rule head (and is therefore derivable).
fn check_reachability(
    obligation: &Obligation,
    heads: &BTreeSet<String>,
    candidates_by_obligation: &BTreeMap<String, BTreeSet<String>>,
) -> Option<Finding> {
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
        finding = attribute_to_candidates(finding, &obligation.iri, candidates_by_obligation);
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
    candidates_by_obligation: &BTreeMap<String, BTreeSet<String>>,
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
        finding = attribute_to_candidates(finding, &obligation.iri, candidates_by_obligation);
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
    store: &Arc<RdfDataset>,
    derived_predicates: &BTreeSet<String>,
) -> gmeow_errors::Result<Vec<Finding>> {
    let obligations = parse_obligations(store)?;
    let heads = foundation::head_predicate_iris();
    let candidates_by_obligation = candidates_by_obligation(store)?;
    let mut findings = Vec::new();
    for obligation in &obligations {
        findings.extend(check_discharge_conditions(obligation));
        if let Some(violation) = check_reachability(obligation, &heads, &candidates_by_obligation) {
            findings.push(violation);
        }
        if let Some(violation) =
            check_finite_closure(obligation, derived_predicates, &candidates_by_obligation)
        {
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
pub fn formalization_coverage(store: &Arc<RdfDataset>) -> gmeow_errors::Result<Vec<Finding>> {
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

/// The `sha256:`-prefixed lowercase-hex digest of `prose`, matching the recorded
/// `logic:candidateSourceHash` lexical form byte-for-byte (the digest is over the raw
/// UTF-8 lexical text — no language tag, no surrounding quotes, no trailing newline).
///
/// Public so the prose-lift corpus (`lang-form`) mints the SAME `logic:candidateSourceHash`
/// value for a lifted `lang:SurfaceForm` that this gate recomputes for the term it lifts:
/// the prose-hash discipline resolves through the lifted surface. The algorithm is fixed —
/// the byte-identical output is the whole point, so callers MUST hash the raw literal text
/// (no normalization) they read from the RDF.
pub fn candidate_source_hash(prose: &str) -> String {
    let digest = Sha256::digest(prose.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    format!("sha256:{hex}")
}

/// A harvested candidate's declared hash and the distinct source-language prose literals
/// resolved for the annotation field it names — accumulated across query rows so the
/// language filter and cardinality checks run once per candidate.
struct HarvestHash {
    /// The `logic:candidateFormalizes` term IRI.
    term: String,
    /// The recorded `logic:candidateSourceHash` lexical form.
    declared_hash: String,
    /// Distinct `@x-gmeow-english` lexical forms of the harvested field (should be one).
    prose: BTreeSet<String>,
}

/// Recompute-and-enforce `logic:candidateSourceHash` drift over the reasoned store.
///
/// Every `logic:FormalizationCandidate` claims, in its governance prose, that a later
/// edit to the prose it harvested "surfaces as drift". This makes that claim executable:
/// for a candidate carrying the full harvest back-link — `logic:candidateFormalizes ?term`
/// AND `logic:candidateSourceField ?field` — it resolves the exact annotation the ontology
/// itself names for that field (`?field logic:proseFieldProperty ?prop`, the closed
/// `logic:ProseField` → property map), reads `?term ?prop ?prose` in the canonical source
/// language, recomputes the `sha256:` digest, and emits an error `Finding` on any mismatch.
/// It is the recompute the SHACL `sh:minCount 1` presence shape structurally cannot express.
///
/// A candidate carrying neither back-link leg (e.g. a doc-section harvest with no single
/// source triple) has nothing to recompute against and never enters the inner join, so it
/// is skipped; a candidate carrying exactly one leg is a half-link already hard-failed by
/// the paired-harvest verify query, so it is out of scope here too. Zero or multiple
/// distinct source-language prose literals for a resolved field is itself a drift error
/// (a dangling or ambiguous harvest link).
///
/// # Errors
///
/// Returns `Err` if the governance query fails to parse or evaluate.
pub fn check_candidate_source_hash_drift(
    store: &Arc<RdfDataset>,
) -> gmeow_errors::Result<Vec<Finding>> {
    let rows = select(
        store,
        "PREFIX logic: <https://blackcatinformatics.ca/logic/>
         SELECT ?c ?term ?prop ?hash ?prose WHERE {
           ?c a logic:FormalizationCandidate ;
              logic:candidateFormalizes ?term ;
              logic:candidateSourceField ?field ;
              logic:candidateSourceHash ?hash .
           ?field logic:proseFieldProperty ?prop .
           ?term ?prop ?prose .
         }",
    )?;

    let mut by_candidate: BTreeMap<String, HarvestHash> = BTreeMap::new();
    for row in rows {
        let (Some(c), Some(term), Some(hash)) = (row.get("c"), row.get("term"), row.get("hash"))
        else {
            continue;
        };
        let entry = by_candidate
            .entry(term_value(c))
            .or_insert_with(|| HarvestHash {
                term: term_value(term),
                declared_hash: term_value(hash),
                prose: BTreeSet::new(),
            });
        // Only the canonical source-language literal is the hashed text; projected
        // public-language translations (@en/@zh/@fr) must never be hashed.
        if let Some(TermValue::Literal {
            lexical_form,
            language,
            ..
        }) = row.get("prose")
            && language.as_deref() == Some(SOURCE_LANG)
        {
            entry.prose.insert(lexical_form.clone());
        }
    }

    let mut findings = Vec::new();
    for (candidate, harvest) in &by_candidate {
        let prose = match harvest.prose.len() {
            1 => harvest.prose.iter().next().expect("len == 1"),
            0 => {
                findings.push(
                    Finding::new(
                        Severity::Error,
                        "verify.candidate-hash.no-source-prose",
                        format!(
                            "formalization candidate <{candidate}> harvests <{}> but that term \
                             carries no @{SOURCE_LANG} prose for its declared source field; the \
                             logic:candidateSourceHash cannot be recomputed (dangling harvest link)",
                            harvest.term
                        ),
                    )
                    .with_tool("verify"),
                );
                continue;
            }
            _ => {
                findings.push(
                    Finding::new(
                        Severity::Error,
                        "verify.candidate-hash.ambiguous-source-prose",
                        format!(
                            "formalization candidate <{candidate}> harvesting <{}> resolves to \
                             multiple distinct @{SOURCE_LANG} prose literals for its source field; \
                             the harvested source is ambiguous",
                            harvest.term
                        ),
                    )
                    .with_tool("verify"),
                );
                continue;
            }
        };
        let recomputed = candidate_source_hash(prose);
        if recomputed != harvest.declared_hash {
            let mut finding = Finding::new(
                Severity::Error,
                "verify.candidate-hash.drift",
                format!(
                    "formalization candidate <{candidate}> is stale: logic:candidateSourceHash \
                     records {declared} but the current @{SOURCE_LANG} prose of <{term}> hashes to \
                     {recomputed} — the harvested prose changed without re-review",
                    declared = harvest.declared_hash,
                    term = harvest.term,
                ),
            )
            .with_tool("verify");
            finding.tags = vec![
                "formalization-governance".to_owned(),
                "source-hash-drift".to_owned(),
            ];
            findings.push(finding);
        }
    }
    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
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
        let no_candidates = BTreeMap::new();
        assert!(check_reachability(&counterpart, &heads, &no_candidates).is_none());
        assert!(check_reachability(&deception, &heads, &no_candidates).is_none());
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
        let no_candidates = BTreeMap::new();
        let finding = check_reachability(&counterpart, &heads, &no_candidates).expect("must fire");
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
        let no_candidates = BTreeMap::new();
        assert!(check_finite_closure(&deception, &empty, &no_candidates).is_none());
        // Red: a (synthetic) derivation of the forbidden predicate trips the obligation.
        let mut derived = BTreeSet::new();
        derived.insert(pred.to_owned());
        let finding =
            check_finite_closure(&deception, &derived, &no_candidates).expect("must fire");
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
        let no_candidates = BTreeMap::new();
        assert!(check_finite_closure(&counterpart, &derived, &no_candidates).is_none());
    }

    #[test]
    fn candidate_over_typing_a_non_assertion_is_surfaced_and_attributed_to_the_candidate() {
        // The over-typing review is realized by the shipped non-entailment machinery, not
        // a separate flag: a FormalizationCandidate categorized
        // logic:CategoryNonEntailmentObligation records a deliberate non-assertion, and its
        // axiom is FORBIDDEN from letting the engine derive that predicate. If a
        // formalization over-types — entailing the deliberately withheld conclusion — the
        // executable check SURFACES it as logic:ObligationViolated (a hard error), never
        // silently asserting it; when the predicate is genuinely absent the obligation is
        // DISCHARGED (actively checked, never silently skipped).
        //
        // This test also proves `logic:candidateNonEntailment` is load-bearing, not
        // decorative: the check traverses candidate->obligation via that edge
        // (`candidates_by_obligation`) and APPENDS the declaring candidate's IRI to the
        // violation finding's message/tags. That traversal is what makes
        // LOGIC-FOUNDATION.md's claim — the over-typing review is "realized through the
        // typed candidate lifecycle" — literally true in this code path: removing the
        // edge from the store must make the candidate-attribution assertion below fail,
        // even though the obligation itself still fires (structural presence of the edge
        // is separately hard-enforced by
        // queries/verify/non-entailment-carrier-required.rq, not re-checked here).
        let logic = "https://blackcatinformatics.ca/logic/";
        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let any_uri = "http://www.w3.org/2001/XMLSchema#anyURI";
        let forbidden = "https://blackcatinformatics.ca/gmeow/overTypedClaim";
        let candidate_iri = "https://ex/cand";
        // A store carrying the obligation AND the candidate that declares it — the exact
        // shape of a candidate whose formalization touches a deliberate non-assertion.
        let ntriples = format!(
            "<https://ex/obl> <{rdf_type}> <{logic}NonEntailmentObligation> .\n\
             <https://ex/obl> <{logic}obligationForbiddenPredicate> \"{forbidden}\"^^<{any_uri}> .\n\
             <https://ex/obl> <{logic}obligationDischargeCondition> <{logic}DischargeFiniteClosure> .\n\
             <{candidate_iri}> <{rdf_type}> <{logic}FormalizationCandidate> .\n\
             <{candidate_iri}> <{logic}candidateCategory> <{logic}CategoryNonEntailmentObligation> .\n\
             <{candidate_iri}> <{logic}candidateNonEntailment> <https://ex/obl> .\n"
        );
        let store = store_from_ntriples(&ntriples);

        // Green: the forbidden predicate is not derived → the obligation is DISCHARGED. The
        // check runs and surfaces no error: checked-and-passed, not silently skipped.
        let discharged =
            check_non_entailment_obligations(&store, &BTreeSet::new()).expect("check runs");
        assert!(
            !discharged.iter().any(|f| f.severity == Severity::Error),
            "a discharged non-entailment obligation must produce no error finding: {discharged:?}"
        );

        // Red: the formalization over-types — the forbidden predicate appears as a DERIVED
        // edge. The check must SURFACE it as ObligationViolated, never silently assert it,
        // AND name the declaring candidate — this assertion fails if
        // `candidateNonEntailment` is removed from the store or not traversed.
        let mut derived = BTreeSet::new();
        derived.insert(forbidden.to_owned());
        let surfaced = check_non_entailment_obligations(&store, &derived).expect("check runs");
        let violation = surfaced
            .iter()
            .find(|f| f.code == "verify.non-entailment.derived" && f.severity == Severity::Error)
            .expect(
                "a candidate over-typing a deliberate non-assertion must be surfaced as \
                 logic:ObligationViolated",
            );
        assert!(
            violation.message.contains(candidate_iri)
                || violation
                    .tags
                    .iter()
                    .any(|t| t == &format!("candidate:{candidate_iri}")),
            "the violation must name the declaring candidate <{candidate_iri}> in its message \
             or tags, proving candidateNonEntailment is load-bearing: {violation:?}"
        );

        // Companion: an obligation with NO declaring candidate must NOT get a candidate
        // suffix — the attribution is conditional on the edge, not always-on.
        let orphan_ntriples = format!(
            "<https://ex/orphan-obl> <{rdf_type}> <{logic}NonEntailmentObligation> .\n\
             <https://ex/orphan-obl> <{logic}obligationForbiddenPredicate> \"{forbidden}\"^^<{any_uri}> .\n\
             <https://ex/orphan-obl> <{logic}obligationDischargeCondition> <{logic}DischargeFiniteClosure> .\n"
        );
        let orphan_store = store_from_ntriples(&orphan_ntriples);
        let orphan_surfaced =
            check_non_entailment_obligations(&orphan_store, &derived).expect("check runs");
        let orphan_violation = orphan_surfaced
            .iter()
            .find(|f| f.code == "verify.non-entailment.derived" && f.severity == Severity::Error)
            .expect("an undischarged obligation with no declaring candidate must still fire");
        assert!(
            !orphan_violation
                .message
                .contains("declared by formalization candidate")
                && !orphan_violation
                    .tags
                    .iter()
                    .any(|t| t.starts_with("candidate:")),
            "an obligation with no declaring candidate must not carry candidate attribution: \
             {orphan_violation:?}"
        );
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

    /// Build a minimal in-memory dataset from N-Triples for query tests.
    fn store_from_ntriples(ntriples: &str) -> Arc<RdfDataset> {
        purrdf::parse_dataset(ntriples.as_bytes(), "application/n-triples", None)
            .expect("load N-Triples")
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

    /// Build a store carrying one harvested candidate: it formalizes `term` via a source
    /// field whose `logic:proseFieldProperty` is `prop`, records `declared_hash`, and the
    /// term carries `prose` on `prop` in the given language. Mirrors the shape of a real
    /// foundational-partition candidate's harvest back-link.
    fn harvested_candidate_store(
        term: &str,
        prop: &str,
        prose: &str,
        lang: &str,
        declared_hash: &str,
    ) -> Arc<RdfDataset> {
        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let logic = "https://blackcatinformatics.ca/logic/";
        let field = "https://blackcatinformatics.ca/logic/ProseFieldDefinition";
        let ntriples = format!(
            "<https://ex/cand> <{rdf_type}> <{logic}FormalizationCandidate> .\n\
             <https://ex/cand> <{logic}candidateFormalizes> <{term}> .\n\
             <https://ex/cand> <{logic}candidateSourceField> <{field}> .\n\
             <https://ex/cand> <{logic}candidateSourceHash> \"{declared_hash}\" .\n\
             <{field}> <{logic}proseFieldProperty> <{prop}> .\n\
             <{term}> <{prop}> \"{prose}\"@{lang} .\n"
        );
        store_from_ntriples(&ntriples)
    }

    #[test]
    fn source_hash_matches_prose_no_drift() {
        // A candidate whose recorded hash IS the sha256 of the current source-language
        // prose produces no finding — the teeth stay silent when nothing drifted.
        let term = "https://blackcatinformatics.ca/logic/Endurant";
        let prop = "http://www.w3.org/2004/02/skos/core#definition";
        let prose = "A continuant wholly present at each moment of its existence.";
        let hash = candidate_source_hash(prose);
        let store = harvested_candidate_store(term, prop, prose, SOURCE_LANG, &hash);
        let findings = check_candidate_source_hash_drift(&store).expect("check runs");
        assert!(
            findings.is_empty(),
            "a matching source hash must produce no drift finding: {findings:?}"
        );
    }

    #[test]
    fn edited_prose_surfaces_as_drift() {
        // The recorded hash anchors the OLD prose; the term now carries EDITED prose, so
        // the recompute no longer matches — the drift check must fire a hard error, giving
        // the "a later prose edit surfaces as drift" governance claim real teeth.
        let term = "https://blackcatinformatics.ca/logic/Endurant";
        let prop = "http://www.w3.org/2004/02/skos/core#definition";
        let stale_hash = candidate_source_hash("The ORIGINAL, reviewed definition prose.");
        let edited_prose = "The definition prose after an un-reviewed edit.";
        let store = harvested_candidate_store(term, prop, edited_prose, SOURCE_LANG, &stale_hash);
        let findings = check_candidate_source_hash_drift(&store).expect("check runs");
        let drift = findings
            .iter()
            .find(|f| f.code == "verify.candidate-hash.drift")
            .expect("edited prose must surface as a drift finding");
        assert_eq!(drift.severity, Severity::Error);
        assert!(
            drift.message.contains(term),
            "the drift finding must name the formalized term: {drift:?}"
        );
    }

    #[test]
    fn projected_translation_is_not_hashed() {
        // Only the @x-gmeow-english source literal is the hashed text. A projected public
        // translation (@en) on the same field must be ignored: the term carries ONLY an
        // @en literal here, so the harvest resolves no source-language prose and the check
        // reports a dangling link rather than silently hashing the translation.
        let term = "https://blackcatinformatics.ca/logic/Endurant";
        let prop = "http://www.w3.org/2004/02/skos/core#definition";
        let prose = "An English projection that must never be hashed.";
        let hash = candidate_source_hash(prose);
        let store = harvested_candidate_store(term, prop, prose, "en", &hash);
        let findings = check_candidate_source_hash_drift(&store).expect("check runs");
        assert!(
            findings
                .iter()
                .any(|f| f.code == "verify.candidate-hash.no-source-prose"),
            "a term with only a projected @en literal must report no source-language prose, \
             never silently hash the translation: {findings:?}"
        );
    }

    #[test]
    fn candidate_without_harvest_link_is_skipped() {
        // The Event⊥Situation shape: a candidate carrying neither back-link leg has no
        // single source triple to recompute against and must be silently skipped, never a
        // false drift error.
        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let logic = "https://blackcatinformatics.ca/logic/";
        let ntriples = format!(
            "<https://ex/cand> <{rdf_type}> <{logic}FormalizationCandidate> .\n\
             <https://ex/cand> <{logic}candidateSourceHash> \"sha256:deadbeef\" .\n"
        );
        let store = store_from_ntriples(&ntriples);
        let findings = check_candidate_source_hash_drift(&store).expect("check runs");
        assert!(
            findings.is_empty(),
            "a candidate with no harvest back-link must be skipped, not flagged: {findings:?}"
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
