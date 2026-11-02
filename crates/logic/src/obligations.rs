// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Executable typed-formalization governance over retained native worlds.
//!
//! Makes `design/LOGIC-FOUNDATION.md` §Typed formalization governance executable:
//!
//! * **Non-entailment obligations** — every `logic:NonEntailmentObligation` names a
//!   forbidden predicate under the selected canonical program and native calculus.
//!   Syntactic reachability reuses compiled flow metadata, retaining authored heads
//!   even when their bodies have not fired and conservatively propagating native
//!   schema effects. Possible writers prevent this proof but never invent a violation.
//!   A selected certified complete finite closure is an alternative absence proof.
//!   An actual committed derived counterexample violates the obligation regardless
//!   of the selected negative-proof arm. Asserted edges never count as derivations.
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
//! Non-entailment consumes exact program/input/engine/world evidence, with a small
//! typed governance view per world. Query coverage and prose drift consume the
//! verifier's presentation graph. Neither recompiles the selected program.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::dataset_view::{DatasetView, GraphMatch};
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfDataset, SparqlEngine, SparqlRequest, SparqlResult, TermId, TermRef, TermValue};
use purrdf_lex::json::record::ToJson;
use sha2::{Digest, Sha256};

use gmeow_errors::{
    Diag, Finding, FindingCategory, Grade, Label, Location, Severity, Standpoint, register_code,
};

#[cfg(test)]
use crate::foundation;

mod reachability;
pub use reachability::{ProgramReachability, ReachabilityEvidence};
mod assessment;
pub(crate) use assessment::governance_statement;
pub use assessment::{
    ObligationAssessment, ObligationDisposition, ObligationOutcome,
    assess_non_entailment_obligations,
};

/// Wrap a governance-obligation condition message as a typed diagnostic on the
/// shared substrate, preserving the authored text verbatim.
fn obligation_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Obligation { detail })
}

#[track_caller]
fn obligation_source_error(
    detail: String,
    source: impl std::error::Error + Send + Sync + 'static,
) -> Diag {
    Diag::new(
        crate::error::Obligation::register(),
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        detail,
    )
    .with_source(source)
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
const DISCHARGE_SYNTACTIC: &str =
    "https://blackcatinformatics.ca/logic/DischargeSyntacticReachability";
const DISCHARGE_FINITE: &str = "https://blackcatinformatics.ca/logic/DischargeFiniteClosure";
const WIRED_DISCHARGE: &[&str] = &[DISCHARGE_SYNTACTIC, DISCHARGE_FINITE];

/// A `logic:NonEntailmentObligation` lifted from the reasoned graph.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Obligation {
    /// The obligation individual's IRI.
    iri: String,
    /// The forbidden predicate IRI (the value of `logic:obligationForbiddenPredicate`).
    forbidden_predicate: String,
    /// Complete declared discharge-condition IRIs, sorted without namespace loss.
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
        .map_err(|error| {
            obligation_source_error("governance query evaluation failed".to_owned(), error)
        })?;
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

/// Discover an admitted governance class through both structural typing surfaces.
/// Native term identities deduplicate dual typing without flattening blank scopes.
fn governance_subjects(store: &RdfDataset, class: &str) -> BTreeSet<TermId> {
    let Some(class) = store.term_id_by_value(&TermValue::Iri(format!("{LOGIC_NS}{class}"))) else {
        return BTreeSet::new();
    };
    [
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        "https://blackcatinformatics.ca/logic/instanceOf",
    ]
    .into_iter()
    .filter_map(|predicate| store.term_id_by_value(&TermValue::Iri(predicate.to_owned())))
    .flat_map(|predicate| {
        store.quads_for_pattern(None, Some(predicate), Some(class), GraphMatch::Default)
    })
    .map(|quad| quad.s)
    .collect()
}

/// Borrow the indexed values of one governance field without a SPARQL round trip.
fn governance_values<'a>(
    store: &'a RdfDataset,
    subject: TermId,
    field: &str,
) -> impl Iterator<Item = TermId> + use<'a> {
    store
        .term_id_by_value(&TermValue::Iri(format!("{LOGIC_NS}{field}")))
        .into_iter()
        .flat_map(move |predicate| {
            store
                .quads_for_pattern(Some(subject), Some(predicate), None, GraphMatch::Default)
                .map(|quad| quad.o)
        })
}

/// Preserve the kind and scope of an authored governance node in report identities.
fn governance_node(store: &RdfDataset, node: TermId) -> gmeow_errors::Result<String> {
    match store.resolve(node) {
        TermRef::Iri(iri) => Ok(iri.to_owned()),
        TermRef::Blank { label, scope } => Ok(format!("_:{}", scope.qualify_label(label))),
        _ => Err(obligation_err(
            "a governance record or reference must be an IRI or blank node".to_owned(),
        )),
    }
}

/// Lift every declared obligation, rejecting malformed required fields rather
/// than losing the declaration in an inner join or choosing its first value.
fn parse_obligations(store: &Arc<RdfDataset>) -> gmeow_errors::Result<Vec<Obligation>> {
    let mut obligations = Vec::new();
    for subject in governance_subjects(store, "NonEntailmentObligation") {
        let iri = governance_node(store, subject)?;
        let mut values = governance_values(store, subject, "obligationForbiddenPredicate");
        let forbidden = values.next().ok_or_else(|| {
            obligation_err(format!(
                "non-entailment obligation {iri} has no logic:obligationForbiddenPredicate"
            ))
        })?;
        if values.next().is_some() {
            return Err(obligation_err(format!(
                "non-entailment obligation {iri} has multiple distinct \
                 logic:obligationForbiddenPredicate values"
            )));
        }
        let forbidden_predicate = match store.as_ref().resolve(forbidden) {
            TermRef::Iri(predicate) => predicate.to_owned(),
            TermRef::Literal {
                lexical,
                datatype,
                language: None,
                ..
            } if matches!(
                store.as_ref().resolve(datatype),
                TermRef::Iri("http://www.w3.org/2001/XMLSchema#anyURI")
            ) && !lexical.is_empty() =>
            {
                lexical.to_owned()
            }
            _ => {
                return Err(obligation_err(format!(
                    "non-entailment obligation {iri} requires a predicate IRI or xsd:anyURI value"
                )));
            }
        };
        let predicate = purrdf::iri::parse(&forbidden_predicate).map_err(|error| {
            obligation_source_error(
                format!("non-entailment obligation {iri} has an invalid forbidden predicate"),
                error,
            )
            .with_location(Location {
                logical: Some(iri.clone()),
                ..Location::default()
            })
        })?;
        if !predicate.has_scheme() {
            return Err(obligation_err(format!(
                "non-entailment obligation {iri} requires an absolute forbidden predicate IRI"
            )));
        }
        let mut discharge_conditions = BTreeSet::new();
        for condition in governance_values(store, subject, "obligationDischargeCondition") {
            let TermRef::Iri(condition) = store.as_ref().resolve(condition) else {
                return Err(obligation_err(format!(
                    "non-entailment obligation {iri} has a non-IRI discharge condition"
                )));
            };
            discharge_conditions.insert(condition.to_owned());
        }
        obligations.push(Obligation {
            iri,
            forbidden_predicate,
            discharge_conditions,
        });
    }
    obligations.sort_by(|left, right| left.iri.cmp(&right.iri));
    Ok(obligations)
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
    mut diag: Diag,
    obligation_iri: &str,
    candidates_by_obligation: &BTreeMap<String, BTreeSet<String>>,
) -> Diag {
    let Some(candidates) = candidates_by_obligation.get(obligation_iri) else {
        return diag;
    };
    if candidates.is_empty() {
        return diag;
    }
    let joined = candidates
        .iter()
        .map(|candidate| format!("<{candidate}>"))
        .collect::<Vec<_>>()
        .join(", ");
    diag.inner_mut().message.push_str(&format!(
        " — declared by formalization candidate(s) {joined} (over-typing surfaced through the typed candidate lifecycle)"
    ));
    for candidate in candidates {
        diag = diag
            .with_tag(format!("candidate:{candidate}"))
            .with_label(Label {
                location: Location {
                    logical: Some(candidate.clone()),
                    ..Location::default()
                },
                text: "declaring candidate".to_owned(),
            });
        if !candidate.starts_with("_:") {
            diag = diag.with_cited_iris([candidate]);
        }
    }
    diag
}

#[track_caller]
fn obligation_diag(
    world: &purrdf::iri::BaseIri,
    obligation: &Obligation,
    field: &str,
    code: &str,
    message: String,
) -> Diag {
    let mut diag = Diag::new(
        register_code(code),
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        message,
    )
    .with_location(Location {
        logical: Some(obligation.iri.clone()),
        ..Location::default()
    })
    .with_focus(purrdf_lex::json::write_compact(
        &(&obligation.forbidden_predicate, field).to_json(),
    ))
    .with_graph(world.clone())
    .with_tag("formalization-governance")
    .with_tag("non-entailment")
    .with_label(Label {
        location: Location {
            logical: Some(obligation.forbidden_predicate.clone()),
            ..Location::default()
        },
        text: "forbidden predicate".to_owned(),
    })
    .with_cited_iris([&obligation.forbidden_predicate]);
    if !obligation.iri.starts_with("_:") {
        diag = diag.with_cited_iris([&obligation.iri]);
    }
    diag
}

/// Build the `obligation IRI -> sorted set of declaring candidate IRIs` attribution map
/// by reading each admitted `logic:FormalizationCandidate`'s
/// `logic:candidateNonEntailment` edges through the native index. This is the read
/// side of the structural link that
/// `queries/verify/non-entailment-carrier-required.rq` already enforces (a
/// `CategoryNonEntailmentObligation` candidate MUST carry `candidateNonEntailment`);
/// this function does not re-check that constraint, it only harvests the edge for
/// attribution.
fn candidates_by_obligation(
    store: &Arc<RdfDataset>,
) -> gmeow_errors::Result<BTreeMap<String, BTreeSet<String>>> {
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for subject in governance_subjects(store, "FormalizationCandidate") {
        let candidate = governance_node(store, subject)?;
        for obligation in governance_values(store, subject, "candidateNonEntailment") {
            map.entry(governance_node(store, obligation)?)
                .or_default()
                .insert(candidate.clone());
        }
    }
    Ok(map)
}

/// Arm A — syntactic reachability. A matching head prevents syntactic discharge;
/// whether its body is satisfied requires evaluation and cannot be inferred here.
fn check_reachability(
    world: &purrdf::iri::BaseIri,
    obligation: &Obligation,
    may_write: bool,
    candidates: &BTreeMap<String, BTreeSet<String>>,
) -> Option<Diag> {
    if !obligation
        .discharge_conditions
        .contains(DISCHARGE_SYNTACTIC)
        || !may_write
    {
        return None;
    }
    Some(attribute_to_candidates(
        obligation_diag(
            world,
            obligation,
            DISCHARGE_SYNTACTIC,
            "verify.non-entailment.not-discharged",
            format!(
                "non-entailment obligation <{}> is NOT SYNTACTICALLY DISCHARGED: forbidden predicate <{}> matches a rule head; this is possibility, not a witnessed derivation (logic:ObligationUnknown)",
                obligation.iri, obligation.forbidden_predicate,
            ),
        ),
        &obligation.iri,
        candidates,
    ))
}

/// Positive evidence is independent of the selected negative-proof arm.
fn check_finite_closure(
    world: &purrdf::iri::BaseIri,
    obligation: &Obligation,
    derived_predicates: &BTreeSet<String>,
    candidates: &BTreeMap<String, BTreeSet<String>>,
) -> Option<Diag> {
    derived_predicates.contains(&obligation.forbidden_predicate).then(|| {
        attribute_to_candidates(obligation_diag(world, obligation, "derived",
            "verify.non-entailment.derived", format!(
                "non-entailment obligation <{}> VIOLATED: forbidden predicate <{}> appears as a DERIVED edge in the materialized closure (logic:ObligationViolated)",
                obligation.iri, obligation.forbidden_predicate,
            )), &obligation.iri, candidates)
    })
}

/// Each unsupported declared condition retains its own native-IRI anchor.
fn check_discharge_conditions(world: &purrdf::iri::BaseIri, obligation: &Obligation) -> Vec<Diag> {
    let mut diagnostics = Vec::new();
    if obligation.discharge_conditions.is_empty() {
        diagnostics.push(obligation_diag(world, obligation, "discharge-condition",
            "verify.non-entailment.no-discharge", format!(
                "non-entailment obligation <{}> declares no logic:obligationDischargeCondition; it has no executable discharge path", obligation.iri,
            )));
    }
    for condition in &obligation.discharge_conditions {
        if !WIRED_DISCHARGE.contains(&condition.as_str()) {
            diagnostics.push(obligation_diag(world, obligation, condition,
                "verify.non-entailment.unwired-discharge", format!(
                    "non-entailment obligation <{}> declares discharge condition <{condition}>, which the engine does not wire (only syntactic-reachability and finite-closure are executable); no executable discharge path", obligation.iri,
                )).with_cited_iris([condition]));
        }
    }
    diagnostics
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
         PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
         SELECT ?c ?cat ?life WHERE {
           ?c (rdf:type|logic:instanceOf) logic:FormalizationCandidate .
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

/// The bare lowercase-hex SHA-256 digest of `prose`, matching the recorded
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
    purrdf_hash::hex::encode(&digest)
}

/// Admit the complete SHA-256 source literal before exposing its hexadecimal value.
///
/// # Errors
/// Returns an error for any other datatype, literal facets or malformed hexadecimal value.
pub(crate) fn admit_candidate_source_hash(value: &TermValue) -> gmeow_errors::Result<&str> {
    let TermValue::Literal {
        lexical_form,
        datatype,
        language,
        direction,
    } = value
    else {
        return Err(obligation_err(
            "logic:candidateSourceHash requires a SHA-256 literal".to_owned(),
        ));
    };
    if gmeow_logic_compile::literal_datatypes::Sha256Digest::from_parts(
        lexical_form,
        datatype,
        language.as_deref(),
        *direction,
    )
    .is_none()
    {
        return Err(obligation_err("logic:candidateSourceHash requires gmeow:sha256Digest with 64 lowercase hexadecimal digits and no language or direction".to_owned()));
    }
    Ok(lexical_form)
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
/// language, recomputes the SHA-256 digest, and emits an error `Finding` on any mismatch.
/// It is the recompute the SHACL `sh:minCount 1` presence shape structurally cannot express.
///
/// The soft-advice peer — an advisory `logic:Constraint` whose `logic:message` mirrors a term's
/// `gmeow:avoidWhen` prose — is guarded by [`check_advice_message_prose_binding`], a direct
/// string binding rather than a stored digest (the message *is* the prose, so equality is the
/// check).
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
         PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>
         SELECT ?c ?term ?prop ?hash ?prose WHERE {
           ?c (rdf:type|logic:instanceOf) logic:FormalizationCandidate .
           ?c logic:candidateFormalizes ?term ;
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
        let declared_hash = admit_candidate_source_hash(hash)?.to_owned();
        if by_candidate
            .get(&term_value(c))
            .is_some_and(|previous| previous.declared_hash != declared_hash)
        {
            return Err(obligation_err(format!(
                "formalization candidate <{}> carries conflicting candidateSourceHash values",
                term_value(c)
            )));
        }
        let entry = by_candidate
            .entry(term_value(c))
            .or_insert_with(|| HarvestHash {
                term: term_value(term),
                declared_hash,
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

/// Enforce the soft-advice message↔prose binding over the reasoned store.
///
/// A realized advice carrier — an advisory `logic:Constraint` (avoidWhen) or a
/// `logic:AdviceGuidance` (useWhen) — surfaces guidance whose text must be the term's own prose,
/// not a hand-authored paraphrase (making "the advice prose machine-active" literal). A carrier
/// that declares `logic:adviceSourceField ?field` binds its `logic:message` to the
/// `logic:formalizes` term's prose for the annotation that field names (`?field
/// logic:proseFieldProperty ?prop`, the same closed `logic:ProseField` → property map the
/// candidate drift gate uses), read in the canonical source language. The message must equal
/// that prose exactly; any divergence is an error `Finding` — the soft-advice peer of
/// [`check_candidate_source_hash_drift`], a direct string binding because the message *is* the
/// prose (no stored digest to recompute). A node carrying no `adviceSourceField` never enters
/// the join, so ordinary constraints and other nodes are untouched. Zero or multiple distinct
/// source-language prose literals for the resolved field is itself a binding error (a dangling or
/// ambiguous advice link).
///
/// # Errors
///
/// Returns `Err` if the governance query fails to parse or evaluate.
pub fn check_advice_message_prose_binding(
    store: &Arc<RdfDataset>,
) -> gmeow_errors::Result<Vec<Finding>> {
    // Match any advice carrier by the logic:adviceSourceField back-link — both an advisory
    // logic:Constraint (avoidWhen) and a logic:AdviceGuidance (useWhen) carry it, and no other
    // node kind does, so the predicate alone selects exactly the advice carriers without a type
    // filter (logic:adviceSourceField is deliberately domain-free so it entails neither type).
    let rows = select(
        store,
        "PREFIX logic: <https://blackcatinformatics.ca/logic/>
         SELECT ?c ?term ?prop ?msg ?prose WHERE {
           ?c logic:formalizes ?term ;
              logic:adviceSourceField ?field ;
              logic:message ?msg .
           ?field logic:proseFieldProperty ?prop .
           ?term ?prop ?prose .
         }",
    )?;

    // Accumulate per carrier: its declared message and the distinct source-language prose
    // literals resolved for the named field (should be exactly one), so the cardinality and
    // language checks run once per carrier regardless of row fan-out.
    struct AdviceBinding {
        term: String,
        message: String,
        prose: BTreeSet<String>,
    }
    let mut by_carrier: BTreeMap<String, AdviceBinding> = BTreeMap::new();
    for row in rows {
        let (Some(c), Some(term), Some(msg)) = (row.get("c"), row.get("term"), row.get("msg"))
        else {
            continue;
        };
        let entry = by_carrier
            .entry(term_value(c))
            .or_insert_with(|| AdviceBinding {
                term: term_value(term),
                message: term_value(msg),
                prose: BTreeSet::new(),
            });
        // Only the canonical source-language literal is the bound prose; projected public-language
        // translations (@en/@zh/@fr) are never the binding target.
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
    for (carrier, binding) in &by_carrier {
        let prose = match binding.prose.len() {
            1 => binding.prose.iter().next().expect("len == 1"),
            0 => {
                findings.push(
                    Finding::new(
                        Severity::Error,
                        "verify.advice-message.no-source-prose",
                        format!(
                            "advice carrier <{carrier}> declares logic:adviceSourceField for \
                             <{}> but that term carries no @{SOURCE_LANG} prose for the named field; \
                             the advice message cannot be bound (dangling advice link)",
                            binding.term
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
                        "verify.advice-message.ambiguous-source-prose",
                        format!(
                            "advice carrier <{carrier}> binding to <{}> resolves to multiple \
                             distinct @{SOURCE_LANG} prose literals for its field; the advice source \
                             is ambiguous",
                            binding.term
                        ),
                    )
                    .with_tool("verify"),
                );
                continue;
            }
        };
        if &binding.message != prose {
            let mut finding = Finding::new(
                Severity::Error,
                "verify.advice-message.drift",
                format!(
                    "advice carrier <{carrier}> is stale: its logic:message diverges from \
                     the current @{SOURCE_LANG} prose of <{term}> — the advice must surface the \
                     term's own prose verbatim, so re-copy the edited prose into the message",
                    term = binding.term,
                ),
            )
            .with_tool("verify");
            finding.tags = vec![
                "formalization-governance".to_owned(),
                "advice-message-drift".to_owned(),
            ];
            findings.push(finding);
        }
    }
    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    Ok(findings)
}

#[path = "obligations.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
mod program_tests;
