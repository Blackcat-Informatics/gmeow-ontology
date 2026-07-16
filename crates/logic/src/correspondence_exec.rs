// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native correspondence-law execution.
//!
//! This module is the single behavioural authority for correspondence recovery.  Both the
//! pipeline's mapping-cell laws and the compiler's five correspondence gates call the same
//! graph executor: `get` and `put` are SPARQL `CONSTRUCT` programs, their intermediate carrier
//! remains typed RDF, and a section discharges only when `put(get(source))` is exactly the
//! original source atom set.
//!
//! A first-class [`RecoveryCaseIr`] supplies the complete query-class source pattern and its
//! ordered source-to-view transform as canonical `logic:Formula`.  The supported execution
//! fragment is `forall(vars, source -> view)`, where both sides are positive conjunctions of
//! binary RDF atoms.  The executor deterministically instantiates the source, lowers the
//! implication to get/put `CONSTRUCT`s, runs both, and returns a countermodel on information
//! loss.  This makes the evidence neutral: the same mechanism proves a genuine recovery and
//! refutes a lossy correspondence.
//!
//! Atomic pure-path renames retain a synthesized one-triple recovery case for the large mapping
//! surface.  Composite paths do not: endpoints alone cannot recover their hidden intermediate
//! nodes, so a `Seq`/`Alt` correspondence must author a complete recovery case instead of
//! passing because `put` was mechanically minted as `get.invert()`.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, DischargeCondition, DischargeVerdict, Formula, LawClaimIr,
    LegPath, LogicProgram, MorphismClass, PreservationKind, RecoveryCaseIr, Term,
};
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::projections::correspondence_gates::CorrespondenceVerdicts;
use purrdf::sparql::{
    GraphPattern, NamedNodePattern, NativeSparqlEngine, Query, SparqlParser, TermPattern,
    TriplePattern as SparqlTriplePattern,
};
use purrdf::{
    RdfQuad, RdfTerm, SerializeGraph, SparqlEngine, SparqlRequest, SparqlResult, canonicalize,
    parse_dataset, serialize_dataset,
};

const VIEW_PREDICATE: &str = "https://blackcatinformatics.ca/logic/recovery#view";
const ATOMIC_SEED_SUBJECT: &str =
    "https://blackcatinformatics.ca/logic/recovery-seed/atomic/subject";
const ATOMIC_SEED_OBJECT: &str = "https://blackcatinformatics.ca/logic/recovery-seed/atomic/object";
const RECOVERY_SEED_BASE: &str = "https://blackcatinformatics.ca/logic/recovery-seed/var/";

/// Common prefix of every generated recovery-execution IRI ([`VIEW_PREDICATE`],
/// [`ATOMIC_SEED_SUBJECT`], [`ATOMIC_SEED_OBJECT`], [`RECOVERY_SEED_BASE`]-derived bindings).
/// An authored recovery-case formula or atomic leg predicate that collides with this namespace
/// could make a generated seed/binding IRI equal an authored constant, collapsing two distinct
/// terms into one in the seed graph and letting a lossy correspondence FALSELY discharge.  The
/// guard below rejects that at lowering time, before any seed graph is built, so the generated
/// IRIs stay disjoint from authored constants by construction.
const RECOVERY_RESERVED_NS: &str = "https://blackcatinformatics.ca/logic/recovery";

/// Whether an authored IRI collides with the reserved recovery-execution namespace.
fn is_reserved_recovery_iri(iri: &str) -> bool {
    iri.starts_with(RECOVERY_RESERVED_NS)
}

/// A comparable RDF atom: subject, predicate, object as canonical term keys.
pub type Atom = (String, String, String);

/// A deterministic source graph used to discharge a correspondence law.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedGraph {
    /// Stable case or branch label used by countermodels.
    pub label: String,
    /// Source atoms.  Seed synthesis currently emits IRI terms in every position.
    pub atoms: Vec<Atom>,
}

impl SeedGraph {
    fn to_ntriples(&self) -> String {
        let mut out = String::new();
        for (subject, predicate, object) in &self.atoms {
            out.push_str(&format!("<{subject}> <{predicate}> <{object}> .\n"));
        }
        out
    }
}

/// A concrete refutation of one correspondence law.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Countermodel {
    /// Seed on which execution failed.
    pub seed_label: String,
    /// Deterministic human-readable summary.
    pub reason: String,
    /// Recovered atoms absent from the source.
    pub spurious: Vec<Atom>,
    /// Source atoms absent from the recovered graph.
    pub missing: Vec<Atom>,
}

/// The three-valued result of executing a law over a seed corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DischargeOutcome {
    /// Discharged, violated, or unknown when no case was available.
    pub verdict: DischargeVerdict,
    /// The first deterministic countermodel when violated.
    pub countermodel: Option<Countermodel>,
}

/// The executable lowering of one canonical recovery formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryExecution {
    /// Deterministically instantiated complete source graph.
    pub seed: SeedGraph,
    /// Source-to-view `CONSTRUCT`.
    pub get_query: String,
    /// View-to-source candidate inverse `CONSTRUCT`.
    pub put_query: String,
}

fn exec_error(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: detail.into(),
    })
}

/// Canonical comparison key for one RDF term.  Quoted triples recurse, so distinct RDF-star
/// terms never collapse to a shared placeholder during law comparison.
pub fn term_key(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => iri.clone(),
        RdfTerm::BlankNode(id) => format!("_:{id}"),
        RdfTerm::Literal(lit) => {
            let datatype = lit
                .datatype
                .as_deref()
                .map(|iri| format!("^^<{iri}>"))
                .unwrap_or_default();
            let language = lit
                .language
                .as_deref()
                .map(|tag| format!("@{tag}"))
                .unwrap_or_default();
            format!("\"{}\"{language}{datatype}", lit.lexical_form)
        }
        RdfTerm::Triple(triple) => format!(
            "<< {} {} {} >>",
            term_key(&triple.subject),
            triple.predicate,
            term_key(&triple.object)
        ),
    }
}

fn quad_atom(quad: &RdfQuad) -> Atom {
    (
        term_key(&quad.subject),
        quad.predicate.clone(),
        term_key(&quad.object),
    )
}

fn run_construct(
    engine: &NativeSparqlEngine,
    source_nt: &str,
    query: &str,
) -> gmeow_errors::Result<Vec<RdfQuad>> {
    let dataset = parse_dataset(source_nt.as_bytes(), "application/n-triples", None)
        .map_err(|error| exec_error(format!("parse correspondence source graph: {error}")))?;
    let result = engine
        .query(
            &dataset,
            SparqlRequest {
                query,
                base_iri: None,
                substitutions: &[],
            },
        )
        .map_err(|error| {
            exec_error(format!(
                "correspondence CONSTRUCT evaluation failed: {error}\nquery: {query}"
            ))
        })?;
    let SparqlResult::Graph(dataset) = result else {
        return Err(exec_error(format!(
            "correspondence CONSTRUCT did not return a graph\nquery: {query}"
        )));
    };
    Ok(purrdf::native_quads::flat_rdf_quads_from_dataset(&dataset)
        .into_iter()
        .filter(|quad| quad.graph_name.is_none())
        .collect())
}

fn quads_to_ntriples(quads: &[RdfQuad]) -> gmeow_errors::Result<String> {
    let dataset = purrdf::native_quads::flat_dataset_from_quads(quads)
        .map_err(|error| exec_error(format!("freeze correspondence carrier: {error}")))?;
    let bytes = serialize_dataset(&dataset, "application/n-triples", SerializeGraph::Dataset)
        .map_err(|error| exec_error(format!("serialize correspondence carrier: {error}")))?;
    String::from_utf8(bytes)
        .map_err(|error| exec_error(format!("correspondence carrier is not UTF-8: {error}")))
}

/// Canonical graph atom set for the uncommon raw-blank-label mismatch path.  The hot path
/// compares atom sets directly; RDFC-1.0 runs only when those differ, so ordinary IRI/literal
/// correspondence discharge pays no canonicalization cost while isomorphic fresh blank nodes
/// still compare by RDF identity rather than allocator labels.
fn canonical_atom_set(quads: &[RdfQuad]) -> gmeow_errors::Result<BTreeSet<Atom>> {
    let dataset = purrdf::native_quads::flat_dataset_from_quads(quads)
        .map_err(|error| exec_error(format!("freeze correspondence comparison graph: {error}")))?;
    let canonical = canonicalize(&dataset);
    let reparsed = parse_dataset(canonical.nquads.as_bytes(), "application/n-quads", None)
        .map_err(|error| exec_error(format!("parse canonical correspondence graph: {error}")))?;
    Ok(purrdf::native_quads::flat_rdf_quads_from_dataset(&reparsed)
        .iter()
        .filter(|quad| quad.graph_name.is_none())
        .map(quad_atom)
        .collect())
}

fn violated(seed: &SeedGraph, reason: String) -> DischargeOutcome {
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationViolated,
        countermodel: Some(Countermodel {
            seed_label: seed.label.clone(),
            reason,
            spurious: Vec::new(),
            missing: Vec::new(),
        }),
    }
}

/// Execute `put ∘ get = id_source` over a deterministic seed corpus.
pub fn discharge_section_law(
    get_query: &str,
    put_query: &str,
    seeds: &[SeedGraph],
) -> DischargeOutcome {
    if seeds.is_empty() {
        return DischargeOutcome {
            verdict: DischargeVerdict::ObligationUnknown,
            countermodel: None,
        };
    }
    let engine = NativeSparqlEngine::new();
    let mut ordered: Vec<&SeedGraph> = seeds.iter().collect();
    ordered.sort_by(|left, right| left.label.cmp(&right.label));

    for seed in ordered {
        let source: BTreeSet<Atom> = seed.atoms.iter().cloned().collect();
        let forward = match run_construct(&engine, &seed.to_ntriples(), get_query) {
            Ok(quads) => quads,
            Err(error) => return violated(seed, format!("get leg is not executable: {error}")),
        };
        let forward_nt = match quads_to_ntriples(&forward) {
            Ok(graph) => graph,
            Err(error) => {
                return violated(seed, format!("forward image is not serializable: {error}"));
            }
        };
        let recovered: BTreeSet<Atom> = match run_construct(&engine, &forward_nt, put_query) {
            Ok(quads) => quads.iter().map(quad_atom).collect(),
            Err(error) => return violated(seed, format!("put leg is not executable: {error}")),
        };
        if recovered != source {
            let spurious: Vec<Atom> = recovered.difference(&source).cloned().collect();
            let missing: Vec<Atom> = source.difference(&recovered).cloned().collect();
            return DischargeOutcome {
                verdict: DischargeVerdict::ObligationViolated,
                countermodel: Some(Countermodel {
                    seed_label: seed.label.clone(),
                    reason: format!(
                        "put∘get did not recover the source on seed `{}`: {} spurious, {} missing",
                        seed.label,
                        spurious.len(),
                        missing.len()
                    ),
                    spurious,
                    missing,
                }),
            };
        }
    }
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationDischarged,
        countermodel: None,
    }
}

/// Execute `get ∘ put = id_view` on each seed's forward image.
pub fn discharge_put_get_law(
    get_query: &str,
    put_query: &str,
    seeds: &[SeedGraph],
) -> DischargeOutcome {
    if seeds.is_empty() {
        return DischargeOutcome {
            verdict: DischargeVerdict::ObligationUnknown,
            countermodel: None,
        };
    }
    let engine = NativeSparqlEngine::new();
    let mut ordered: Vec<&SeedGraph> = seeds.iter().collect();
    ordered.sort_by(|left, right| left.label.cmp(&right.label));

    for seed in ordered {
        let view_quads = match run_construct(&engine, &seed.to_ntriples(), get_query) {
            Ok(quads) => quads,
            Err(error) => return violated(seed, format!("get leg is not executable: {error}")),
        };
        let view: BTreeSet<Atom> = view_quads.iter().map(quad_atom).collect();
        let view_nt = match quads_to_ntriples(&view_quads) {
            Ok(graph) => graph,
            Err(error) => {
                return violated(seed, format!("forward image is not serializable: {error}"));
            }
        };
        let recovered_quads = match run_construct(&engine, &view_nt, put_query) {
            Ok(quads) => quads,
            Err(error) => return violated(seed, format!("put leg is not executable: {error}")),
        };
        let recovered_nt = match quads_to_ntriples(&recovered_quads) {
            Ok(graph) => graph,
            Err(error) => {
                return violated(
                    seed,
                    format!("recovered graph is not serializable: {error}"),
                );
            }
        };
        let reprojected_quads = match run_construct(&engine, &recovered_nt, get_query) {
            Ok(quads) => quads,
            Err(error) => {
                return violated(seed, format!("get leg is not executable: {error}"));
            }
        };
        let reprojected: BTreeSet<Atom> = reprojected_quads.iter().map(quad_atom).collect();
        if reprojected != view {
            let canonical_view = match canonical_atom_set(&view_quads) {
                Ok(atoms) => atoms,
                Err(error) => {
                    return violated(seed, format!("view graph is not canonicalizable: {error}"));
                }
            };
            let canonical_reprojected = match canonical_atom_set(&reprojected_quads) {
                Ok(atoms) => atoms,
                Err(error) => {
                    return violated(
                        seed,
                        format!("reprojected graph is not canonicalizable: {error}"),
                    );
                }
            };
            if canonical_reprojected == canonical_view {
                continue;
            }
            let spurious: Vec<Atom> = canonical_reprojected
                .difference(&canonical_view)
                .cloned()
                .collect();
            let missing: Vec<Atom> = canonical_view
                .difference(&canonical_reprojected)
                .cloned()
                .collect();
            return DischargeOutcome {
                verdict: DischargeVerdict::ObligationViolated,
                countermodel: Some(Countermodel {
                    seed_label: seed.label.clone(),
                    reason: format!(
                        "get∘put did not preserve the view on seed `{}`: {} spurious, {} missing",
                        seed.label,
                        spurious.len(),
                        missing.len()
                    ),
                    spurious,
                    missing,
                }),
            };
        }
    }
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationDischarged,
        countermodel: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PatternTerm {
    Iri(String),
    Var(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TriplePattern {
    subject: PatternTerm,
    predicate: String,
    object: PatternTerm,
}

fn pattern_term(term: &Term, position: &str) -> gmeow_errors::Result<PatternTerm> {
    match term {
        Term::Iri(iri) => Ok(PatternTerm::Iri(iri.clone())),
        Term::Var(variable) if valid_variable(variable) => Ok(PatternTerm::Var(variable.clone())),
        Term::Var(variable) => Err(exec_error(format!(
            "{position} variable `{variable}` is not a valid SPARQL variable name"
        ))),
        Term::Literal { .. } => Err(exec_error(format!(
            "{position} literals are outside the recovery-case RDF-atom fragment"
        ))),
        Term::SequenceMarker(_) => Err(exec_error(format!(
            "{position} sequence markers are outside the recovery-case RDF-atom fragment"
        ))),
    }
}

fn valid_variable(variable: &str) -> bool {
    let mut chars = variable.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn collect_patterns(formula: &Formula, side: &str) -> gmeow_errors::Result<Vec<TriplePattern>> {
    match formula {
        Formula::Atom { relation, args } => {
            let Term::Iri(predicate) = relation else {
                return Err(exec_error(format!("{side} atom relation must be an IRI")));
            };
            if args.len() != 2 {
                return Err(exec_error(format!(
                    "{side} atom <{predicate}> must have exactly two RDF arguments; found {}",
                    args.len()
                )));
            }
            Ok(vec![TriplePattern {
                subject: pattern_term(&args[0], &format!("{side} subject"))?,
                predicate: predicate.clone(),
                object: pattern_term(&args[1], &format!("{side} object"))?,
            }])
        }
        Formula::And(formulas) => {
            let mut patterns = Vec::new();
            for member in formulas {
                patterns.extend(collect_patterns(member, side)?);
            }
            Ok(patterns)
        }
        _ => Err(exec_error(format!(
            "{side} must be a positive binary atom or conjunction of such atoms"
        ))),
    }
}

/// Reject every authored IRI (predicate, or a subject/object `PatternTerm::Iri`) that falls
/// inside the reserved recovery-execution namespace.  Called on both the source and view
/// patterns before any seed/binding IRI is generated, so a collision is a hard authoring
/// error rather than a silent seed-graph collapse.
fn reject_reserved_recovery_iris(
    patterns: &[TriplePattern],
    side: &str,
) -> gmeow_errors::Result<()> {
    for pattern in patterns {
        if is_reserved_recovery_iri(&pattern.predicate) {
            return Err(exec_error(format!(
                "{side} atom predicate <{}> uses the reserved recovery-execution namespace \
                 `{RECOVERY_RESERVED_NS}`; author IRIs outside that namespace so generated seed \
                 bindings stay fresh",
                pattern.predicate
            )));
        }
        for (position, term) in [("subject", &pattern.subject), ("object", &pattern.object)] {
            if let PatternTerm::Iri(iri) = term
                && is_reserved_recovery_iri(iri)
            {
                return Err(exec_error(format!(
                    "{side} {position} <{iri}> uses the reserved recovery-execution namespace \
                     `{RECOVERY_RESERVED_NS}`; author IRIs outside that namespace so generated \
                     seed bindings stay fresh"
                )));
            }
        }
    }
    Ok(())
}

fn variables(patterns: &[TriplePattern]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for pattern in patterns {
        for term in [&pattern.subject, &pattern.object] {
            if let PatternTerm::Var(variable) = term {
                out.insert(variable.clone());
            }
        }
    }
    out
}

fn sparql_term(term: &PatternTerm) -> String {
    match term {
        PatternTerm::Iri(iri) => format!("<{iri}>"),
        PatternTerm::Var(variable) => format!("?{variable}"),
    }
}

fn render_patterns(patterns: &[TriplePattern]) -> String {
    patterns
        .iter()
        .map(|pattern| {
            format!(
                "{} <{}> {} .",
                sparql_term(&pattern.subject),
                pattern.predicate,
                sparql_term(&pattern.object)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn instantiate_term(term: &PatternTerm, bindings: &BTreeMap<String, String>) -> String {
    match term {
        PatternTerm::Iri(iri) => iri.clone(),
        PatternTerm::Var(variable) => bindings
            .get(variable)
            .expect("all source variables were bound before instantiation")
            .clone(),
    }
}

/// Lower one canonical `logic:RecoveryCase` formula to executable get/put queries and a
/// deterministic complete source seed.
pub fn lower_recovery_case(case: &RecoveryCaseIr) -> gmeow_errors::Result<RecoveryExecution> {
    let Formula::Forall { vars, body } = &case.transform else {
        return Err(exec_error(
            "recoveryTransform must be universally quantified",
        ));
    };
    let Formula::Implies(source, view) = body.as_ref() else {
        return Err(exec_error(
            "recoveryTransform body must be an ordered source-to-view implication",
        ));
    };
    let source_patterns = collect_patterns(source, "source")?;
    let view_patterns = collect_patterns(view, "view")?;
    reject_reserved_recovery_iris(&source_patterns, "source")?;
    reject_reserved_recovery_iris(&view_patterns, "view")?;
    if source_patterns.is_empty() || view_patterns.is_empty() {
        return Err(exec_error(
            "recoveryTransform source and view patterns must be non-empty",
        ));
    }

    let declared: BTreeSet<String> = vars.iter().cloned().collect();
    if declared.len() != vars.len() {
        return Err(exec_error(
            "recoveryTransform quantifier variables must be unique",
        ));
    }
    if let Some(invalid) = vars.iter().find(|variable| !valid_variable(variable)) {
        return Err(exec_error(format!(
            "quantified variable `{invalid}` is not a valid SPARQL variable name"
        )));
    }
    let source_variables = variables(&source_patterns);
    let view_variables = variables(&view_patterns);
    let used: BTreeSet<String> = source_variables.union(&view_variables).cloned().collect();
    if let Some(free) = used.difference(&declared).next() {
        return Err(exec_error(format!(
            "recoveryTransform variable `{free}` is free rather than universally quantified"
        )));
    }
    if let Some(unbound) = view_variables.difference(&source_variables).next() {
        return Err(exec_error(format!(
            "view variable `{unbound}` is not bound by the source pattern"
        )));
    }
    if let Some(unused) = declared.difference(&used).next() {
        return Err(exec_error(format!(
            "quantified variable `{unused}` is unused in the recovery transform"
        )));
    }

    let bindings: BTreeMap<String, String> = vars
        .iter()
        .enumerate()
        .map(|(index, variable)| (variable.clone(), format!("{RECOVERY_SEED_BASE}{index:04}")))
        .collect();
    let atoms: BTreeSet<Atom> = source_patterns
        .iter()
        .map(|pattern| {
            (
                instantiate_term(&pattern.subject, &bindings),
                pattern.predicate.clone(),
                instantiate_term(&pattern.object, &bindings),
            )
        })
        .collect();
    let seed = SeedGraph {
        label: case.iri.clone(),
        atoms: atoms.into_iter().collect(),
    };
    Ok(RecoveryExecution {
        seed,
        get_query: format!(
            "CONSTRUCT {{ {} }} WHERE {{ {} }}",
            render_patterns(&view_patterns),
            render_patterns(&source_patterns)
        ),
        put_query: format!(
            "CONSTRUCT {{ {} }} WHERE {{ {} }}",
            render_patterns(&source_patterns),
            render_patterns(&view_patterns)
        ),
    })
}

/// Execute one recovery case.  An out-of-fragment formula is an explicit violated
/// obligation with a countermodel reason, never a silently skipped case.
pub fn discharge_recovery_case(case: &RecoveryCaseIr) -> DischargeOutcome {
    let execution = match lower_recovery_case(case) {
        Ok(execution) => execution,
        Err(reason) => {
            return violated(
                &SeedGraph {
                    label: case.iri.clone(),
                    atoms: Vec::new(),
                },
                format!("recovery case is not executable: {reason}"),
            );
        }
    };
    discharge_section_law(
        &execution.get_query,
        &execution.put_query,
        std::slice::from_ref(&execution.seed),
    )
}

fn discharge_recovery_cases(correspondence: &Correspondence) -> DischargeOutcome {
    if correspondence.recovery_cases.is_empty() {
        return DischargeOutcome {
            verdict: DischargeVerdict::ObligationUnknown,
            countermodel: None,
        };
    }
    for case in &correspondence.recovery_cases {
        let outcome = discharge_recovery_case(case);
        if outcome.verdict != DischargeVerdict::ObligationDischarged {
            return outcome;
        }
    }
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationDischarged,
        countermodel: None,
    }
}

#[derive(Debug, Clone, Copy)]
struct AtomicPath<'a> {
    predicate: &'a str,
    inverse: bool,
}

fn atomic_path(path: &LegPath) -> Option<AtomicPath<'_>> {
    fn walk(path: &LegPath, inverse: bool) -> Option<AtomicPath<'_>> {
        match path {
            LegPath::Step(predicate) => Some(AtomicPath { predicate, inverse }),
            LegPath::Inverse(inner) => walk(inner, !inverse),
            LegPath::Seq(_) | LegPath::Alt(_) => None,
        }
    }
    walk(path, false)
}

fn atomic_pattern(path: AtomicPath<'_>, subject: &str, object: &str) -> String {
    if path.inverse {
        format!("{object} <{}> {subject} .", path.predicate)
    } else {
        format!("{subject} <{}> {object} .", path.predicate)
    }
}

/// Execute the synthesized complete one-triple recovery case for a pure atomic rename.
/// Composite paths return `ObligationUnknown`: their hidden intermediate graph is not
/// recoverable from endpoint equality and must be supplied by a first-class recovery case.
pub fn leg_pair_verdict(get: &LegPath, put: &LegPath) -> DischargeVerdict {
    let recovered = put.invert();
    let (Some(get_path), Some(recovered_path)) = (atomic_path(get), atomic_path(&recovered)) else {
        return DischargeVerdict::ObligationUnknown;
    };
    // The atomic get/put predicates are authored constants.  If either collides with the
    // reserved recovery-execution namespace (`RECOVERY_RESERVED_NS`), the synthesized atomic
    // seed (`ATOMIC_SEED_SUBJECT`/`ATOMIC_SEED_OBJECT`) or its `VIEW_PREDICATE` carrier could
    // collapse with an authored term and FALSELY discharge.  `ObligationUnknown` is not
    // acceptable here — the gates below treat Unknown as "honest and passes", which would let
    // the collision through silently.  `ObligationViolated` fails closed: it reds the Law,
    // Round-trip, and Mnemomorphism gates exactly like a genuine put∘get counterexample, and is
    // diagnosable because the gates' fixed-template refutation clause already names the failing
    // correspondence IRI.
    if is_reserved_recovery_iri(get_path.predicate)
        || is_reserved_recovery_iri(recovered_path.predicate)
    {
        return DischargeVerdict::ObligationViolated;
    }
    let source_atom = if get_path.inverse {
        (
            ATOMIC_SEED_OBJECT.to_owned(),
            get_path.predicate.to_owned(),
            ATOMIC_SEED_SUBJECT.to_owned(),
        )
    } else {
        (
            ATOMIC_SEED_SUBJECT.to_owned(),
            get_path.predicate.to_owned(),
            ATOMIC_SEED_OBJECT.to_owned(),
        )
    };
    let seed = SeedGraph {
        label: "synthesized-atomic-path".to_owned(),
        atoms: vec![source_atom],
    };
    let get_query = format!(
        "CONSTRUCT {{ ?s <{VIEW_PREDICATE}> ?o . }} WHERE {{ {} }}",
        atomic_pattern(get_path, "?s", "?o")
    );
    let put_query = format!(
        "CONSTRUCT {{ {} }} WHERE {{ ?s <{VIEW_PREDICATE}> ?o . }}",
        atomic_pattern(recovered_path, "?s", "?o")
    );
    discharge_section_law(&get_query, &put_query, &[seed]).verdict
}

/// Compute the executed recovery verdict for every correspondence.
///
/// First-class recovery cases are authoritative.  When none are authored, only a complete
/// atomic path rename can synthesize its one-triple case.  Missing/unresolvable or composite
/// legs remain Unknown rather than passing by structural inversion.
pub fn program_verdicts(program: &CorrespondenceProgram) -> CorrespondenceVerdicts {
    let mut verdicts = BTreeMap::new();
    for correspondence in &program.correspondences {
        let verdict = if correspondence.recovery_cases.is_empty() {
            let get = correspondence
                .get_leg
                .as_deref()
                .and_then(|iri| program.resolve_leg(iri));
            let put = correspondence
                .put_leg
                .as_deref()
                .and_then(|iri| program.resolve_leg(iri));
            match (get, put) {
                (Some(get), Some(put)) => leg_pair_verdict(get, put),
                _ => DischargeVerdict::ObligationUnknown,
            }
        } else {
            discharge_recovery_cases(correspondence).verdict
        };
        verdicts.insert(correspondence.iri.clone(), verdict);
    }
    verdicts
}

/// Assemble the same derived correspondence program as the compiler, then execute every
/// recovery obligation for the gate's total verdict map.
pub fn logic_program_verdicts(
    program: &LogicProgram,
) -> gmeow_errors::Result<CorrespondenceVerdicts> {
    if program.correspondences.is_empty() {
        return Ok(CorrespondenceVerdicts::new());
    }
    let assembled = CorrespondenceProgram::new(
        program.correspondences.clone(),
        Vec::new(),
        PreservationKind::SoundUnder,
    )
    .with_leg_programs(program.transaction_programs.clone());
    let (derived, _) = assembled.with_derived_puts()?;
    Ok(program_verdicts(&derived))
}

// Mapping-cell branch-covering seed derivation.  The `get` leg is a SPARQL `CONSTRUCT`; its
// `WHERE` clause is parsed by the SAME real algebra (`purrdf::sparql`) the executor above runs
// it through, then walked into disjunctive-normal-form branches.  A hand-rolled text splitter
// would be a second, weaker parser for a fragment the real one already covers exactly.

/// Deterministic base for fresh per-variable seed IRIs.  Kept byte-identical to the prior
/// scheme (`http://seed.example/v{n}`) so the branch-covering / determinism tests in
/// `crates/pipeline/src/correspondence_law.rs` are unaffected by this rewrite.
const BRANCH_SEED_BASE: &str = "http://seed.example/v";

/// Recursively enumerate the `WHERE` algebra into disjunctive branches: one `Vec` of triple
/// patterns per top-level `UNION` disjunct.  `Join`/`Lateral` distribute as a cartesian
/// product, so a pattern joined OUTSIDE a `UNION` — the shared-atom case the old text splitter
/// dropped — appears in every resulting branch.  `LeftJoin` (`OPTIONAL`) keeps only its
/// required (left) side: SPARQL OPTIONAL semantics do not require the right side to match, so
/// its content is not a positive obligation the seed corpus must recover — forcing it into
/// every seed would wrongly demand round-tripping data that is, by construction, optional.
/// `Filter`/`Extend` (`BIND`)/`Graph`/solution-modifier wrappers contribute no atoms of their
/// own and are unwrapped down to their inner pattern; `Minus` likewise keeps only its required
/// (left) side. Constructs with no positive triple-pattern content (`Path`, `Service`,
/// `Values`) yield no branches — deterministically dropped, never guessed at.
fn dnf_branches(pattern: &GraphPattern) -> Vec<Vec<SparqlTriplePattern>> {
    match pattern {
        GraphPattern::Bgp { patterns } => vec![patterns.clone()],
        GraphPattern::Join { left, right } | GraphPattern::Lateral { left, right } => {
            let left_branches = dnf_branches(left);
            let right_branches = dnf_branches(right);
            match (left_branches.is_empty(), right_branches.is_empty()) {
                (true, true) => Vec::new(),
                (true, false) => right_branches,
                (false, true) => left_branches,
                (false, false) => {
                    let mut out = Vec::with_capacity(left_branches.len() * right_branches.len());
                    for left_branch in &left_branches {
                        for right_branch in &right_branches {
                            let mut combined = left_branch.clone();
                            combined.extend(right_branch.iter().cloned());
                            out.push(combined);
                        }
                    }
                    out
                }
            }
        }
        GraphPattern::Union { left, right } => {
            let mut out = dnf_branches(left);
            out.extend(dnf_branches(right));
            out
        }
        GraphPattern::LeftJoin { left, .. } | GraphPattern::Minus { left, .. } => {
            dnf_branches(left)
        }
        GraphPattern::Filter { inner, .. }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::Graph { inner, .. }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Group { inner, .. } => dnf_branches(inner),
        GraphPattern::Path { .. } | GraphPattern::Service { .. } | GraphPattern::Values { .. } => {
            Vec::new()
        }
    }
}

/// Bind a fresh seed IRI to a first-seen variable/blank-node key (reused for repeat
/// occurrences within the same branch), advancing the shared, branch-spanning counter.
fn fresh_binding(
    key: &str,
    bindings: &mut BTreeMap<String, String>,
    counter: &mut usize,
) -> String {
    bindings
        .entry(key.to_owned())
        .or_insert_with(|| {
            let iri = format!("{BRANCH_SEED_BASE}{counter}");
            *counter += 1;
            iri
        })
        .clone()
}

fn resolve_named_node_pattern(
    term: &NamedNodePattern,
    bindings: &mut BTreeMap<String, String>,
    counter: &mut usize,
) -> String {
    match term {
        NamedNodePattern::NamedNode(iri) => iri.as_str().to_owned(),
        NamedNodePattern::Variable(variable) => fresh_binding(variable.as_str(), bindings, counter),
    }
}

/// Resolve one subject/object term to its concrete seed value.  IRIs and literals become
/// their own resolved lexical form (no manual prefix expansion — the parser already resolved
/// prefixed names to absolute IRIs); variables and blank nodes get a fresh deterministic seed
/// IRI.  An RDF-star quoted-triple term is outside the positive binary-atom seed fragment, so
/// it resolves to `None` and its containing triple pattern contributes no atom, rather than a
/// mis-parsed guess.
fn resolve_term_pattern(
    term: &TermPattern,
    bindings: &mut BTreeMap<String, String>,
    counter: &mut usize,
) -> Option<String> {
    match term {
        TermPattern::NamedNode(iri) => Some(iri.as_str().to_owned()),
        TermPattern::Variable(variable) => {
            Some(fresh_binding(variable.as_str(), bindings, counter))
        }
        TermPattern::Literal(literal) => Some(literal.value().to_owned()),
        TermPattern::BlankNode(blank) => Some(fresh_binding(
            &format!("_:{}", blank.as_str()),
            bindings,
            counter,
        )),
        TermPattern::Triple(_) => None,
    }
}

/// Instantiate one DNF branch into concrete seed atoms.  Variable/blank-node bindings are
/// scoped to this branch (a fresh map per branch); the seed-IRI counter is shared across
/// branches so every seed atom in the corpus carries a distinct fresh IRI.
fn instantiate_branch(branch: &[SparqlTriplePattern], counter: &mut usize) -> Vec<Atom> {
    let mut bindings = BTreeMap::new();
    let mut atoms = Vec::with_capacity(branch.len());
    for pattern in branch {
        let subject = resolve_term_pattern(&pattern.subject, &mut bindings, counter);
        let predicate = resolve_named_node_pattern(&pattern.predicate, &mut bindings, counter);
        let object = resolve_term_pattern(&pattern.object, &mut bindings, counter);
        if let (Some(subject), Some(object)) = (subject, object) {
            atoms.push((subject, predicate, object));
        }
    }
    atoms
}

/// Derive one deterministic seed per top-level `UNION` branch of `get_query`'s `WHERE` algebra
/// (a pattern joined outside a `UNION` is distributed into every branch) plus one combined
/// seed unioning all branches.  Deterministically returns an empty corpus — never a panic or a
/// guessed split — when `get_query` fails to parse or is not a `CONSTRUCT`.
pub fn derive_seeds(get_query: &str) -> Vec<SeedGraph> {
    let Ok(Query::Construct { pattern, .. }) = SparqlParser::new().parse_query(get_query) else {
        return Vec::new();
    };
    let branches = dnf_branches(&pattern);
    let mut counter = 0usize;
    let mut seeds = Vec::new();
    let mut combined = Vec::new();
    for (index, branch) in branches.iter().enumerate() {
        let atoms = instantiate_branch(branch, &mut counter);
        if atoms.is_empty() {
            continue;
        }
        combined.extend(atoms.iter().cloned());
        seeds.push(SeedGraph {
            label: format!("branch-{index}"),
            atoms,
        });
    }
    if !combined.is_empty() {
        seeds.push(SeedGraph {
            label: "combined".to_owned(),
            atoms: combined
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        });
    }
    seeds
}

fn claim_from(law: CorrespondenceLaw, outcome: &DischargeOutcome) -> LawClaimIr {
    LawClaimIr {
        law,
        verdict: outcome.verdict,
        condition: (outcome.verdict != DischargeVerdict::ObligationUnknown)
            .then_some(DischargeCondition::DischargeBoundedCorpus),
    }
}

/// Discharge every law permitted by a mapping cell's rung through the shared native graph
/// executor.
pub fn discharge_laws(get_query: &str, put_query: &str, rung: MorphismClass) -> Vec<LawClaimIr> {
    let seeds = derive_seeds(get_query);
    let mut claims = Vec::new();
    if rung.is_injective_rung() {
        claims.push(claim_from(
            CorrespondenceLaw::SectionLaw,
            &discharge_section_law(get_query, put_query, &seeds),
        ));
        claims.push(claim_from(
            CorrespondenceLaw::PutGet,
            &discharge_put_get_law(get_query, put_query, &seeds),
        ));
    }
    claims
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_logic_compile::ir::{CorrespondenceRelation, MorphismKind, TransactionProgramIr};

    fn step(predicate: &str) -> LegPath {
        LegPath::Step(predicate.to_owned())
    }

    fn atom(predicate: &str, subject: Term, object: Term) -> Formula {
        Formula::atom(
            Term::iri(predicate).expect("predicate IRI"),
            vec![subject, object],
        )
        .expect("binary atom")
    }

    fn recovery_case(view_keeps_detail: bool) -> RecoveryCaseIr {
        let subject = Term::var("subject").expect("subject variable");
        let detail = Term::var("detail").expect("detail variable");
        let source = Formula::And(vec![
            atom(
                "https://example.org/sourceKind",
                subject.clone(),
                Term::iri("https://example.org/Language").expect("class IRI"),
            ),
            atom(
                "https://example.org/sourceDetail",
                subject.clone(),
                detail.clone(),
            ),
        ]);
        let mut view = vec![atom(
            "https://example.org/viewKind",
            subject.clone(),
            Term::iri("https://example.org/SignSystem").expect("class IRI"),
        )];
        if view_keeps_detail {
            view.push(atom("https://example.org/viewDetail", subject, detail));
        }
        RecoveryCaseIr::new(
            "https://example.org/recovery/case",
            Formula::Forall {
                vars: vec!["subject".to_owned(), "detail".to_owned()],
                body: Box::new(Formula::Implies(
                    Box::new(source),
                    Box::new(Formula::And(view)),
                )),
            },
        )
        .expect("recovery case")
    }

    #[test]
    fn atomic_inverse_recovers_the_real_source_predicate() {
        let get = step("https://example.org/source");
        assert_eq!(
            leg_pair_verdict(&get, &get.invert()),
            DischargeVerdict::ObligationDischarged
        );
    }

    #[test]
    fn wrong_atomic_put_yields_a_real_missing_and_spurious_difference() {
        assert_eq!(
            leg_pair_verdict(
                &step("https://example.org/source"),
                &step("https://example.org/wrong")
            ),
            DischargeVerdict::ObligationViolated
        );
    }

    #[test]
    fn composite_path_is_unknown_without_a_complete_recovery_case() {
        let get = LegPath::Seq(vec![
            step("https://example.org/a"),
            step("https://example.org/b"),
        ]);
        assert_eq!(
            leg_pair_verdict(&get, &get.invert()),
            DischargeVerdict::ObligationUnknown
        );
    }

    #[test]
    fn recovery_formula_discharges_only_when_the_view_retains_every_source_variable() {
        let good = discharge_recovery_case(&recovery_case(true));
        assert_eq!(
            good.verdict,
            DischargeVerdict::ObligationDischarged,
            "{good:#?}"
        );

        let bad = discharge_recovery_case(&recovery_case(false));
        assert_eq!(
            bad.verdict,
            DischargeVerdict::ObligationViolated,
            "{bad:#?}"
        );
        let countermodel = bad.countermodel.expect("loss has a countermodel");
        assert_eq!(countermodel.missing.len(), 1, "{countermodel:#?}");
        assert!(countermodel.spurious.is_empty(), "{countermodel:#?}");
    }

    #[test]
    fn recovery_case_colliding_with_the_reserved_recovery_namespace_never_discharges() {
        // The view predicate is authored as the SAME IRI the executor generates internally
        // (`VIEW_PREDICATE`).  Without the reserved-namespace guard this collision would make
        // the mechanically synthesized view carrier indistinguishable from the authored view
        // atom in the seed graph, and the atom-set comparison in `discharge_section_law` could
        // FALSELY discharge a lossy correspondence.  The guard must reject it before any seed
        // is built.
        let subject = Term::var("subject").expect("subject variable");
        let object = Term::var("object").expect("object variable");
        let source = atom(
            "https://example.org/sourceKind",
            subject.clone(),
            object.clone(),
        );
        let view = atom(VIEW_PREDICATE, subject, object);
        let case = RecoveryCaseIr::new(
            "https://example.org/recovery/reserved-namespace-collision",
            Formula::Forall {
                vars: vec!["subject".to_owned(), "object".to_owned()],
                body: Box::new(Formula::Implies(Box::new(source), Box::new(view))),
            },
        )
        .expect("recovery case");

        let outcome = discharge_recovery_case(&case);
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationViolated,
            "a recovery case whose view predicate collides with the generated VIEW_PREDICATE \
             must never discharge: {outcome:#?}"
        );
    }

    #[test]
    fn program_prefers_authored_recovery_evidence_over_a_mechanical_put() {
        let correspondence = Correspondence::new(
            "https://example.org/correspondence",
            CorrespondenceRelation::Subsumes,
            MorphismClass::SectionRetraction,
            MorphismKind::InstitutionMorphism,
            true,
            None,
            Some("https://example.org/get".to_owned()),
            Some("https://example.org/put".to_owned()),
            Vec::new(),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("correspondence")
        .with_recovery_cases(vec![recovery_case(false)])
        .expect("case");
        let program = CorrespondenceProgram::new(
            vec![correspondence],
            Vec::new(),
            PreservationKind::SoundUnder,
        )
        .with_leg_programs(vec![
            TransactionProgramIr {
                iri: "https://example.org/get".to_owned(),
                body: step("https://example.org/source"),
            },
            TransactionProgramIr {
                iri: "https://example.org/put".to_owned(),
                body: step("https://example.org/source").invert(),
            },
        ]);
        assert_eq!(
            program_verdicts(&program)["https://example.org/correspondence"],
            DischargeVerdict::ObligationViolated,
            "the mechanically perfect path pair must not override a refuting source case"
        );
    }

    #[test]
    fn branch_seed_derivation_covers_plain_and_union_queries() {
        let plain = "CONSTRUCT { ?s <http://view/p> ?o } WHERE { ?s <http://source/p> ?o . }";
        let seeds = derive_seeds(plain);
        assert_eq!(seeds.len(), 2, "branch plus combined: {seeds:#?}");

        let union = "CONSTRUCT { ?s <http://view/p> ?o } WHERE { { ?s <http://source/a> ?o . } UNION { ?s <http://source/b> ?o . } }";
        let seeds = derive_seeds(union);
        assert_eq!(
            seeds
                .iter()
                .map(|seed| seed.label.as_str())
                .collect::<Vec<_>>(),
            vec!["branch-0", "branch-1", "combined"]
        );
    }

    #[test]
    fn derive_seeds_is_empty_for_a_query_that_is_not_a_construct() {
        // A hard-fail, not a silent mis-parse: an unparsable or non-CONSTRUCT query
        // deterministically yields no seeds rather than guessing at a split.
        assert_eq!(derive_seeds("this is not valid SPARQL {{{"), Vec::new());
        assert_eq!(
            derive_seeds("SELECT ?s WHERE { ?s <http://ex.example/p> ?o }"),
            Vec::new()
        );
    }

    // Gap G15: the prior hand-rolled splitter did `flat.split('.')` over the flattened WHERE
    // body, which mis-splits any full `<IRI>` whose authority/path contains a dot. A real
    // SPARQL predicate IRI is atomic to the parser regardless of embedded dots, so it must
    // survive into the seed as ONE triple pattern, not be chopped into garbage statements.
    #[test]
    fn full_dotted_iri_predicate_is_not_mis_split() {
        let get = "CONSTRUCT { ?s <http://view.example/p> ?o } \
                    WHERE { ?s <http://ex.example/p.q> ?o . }";
        let seeds = derive_seeds(get);
        let branch = seeds
            .iter()
            .find(|seed| seed.label == "branch-0")
            .expect("one branch for the single BGP");
        assert_eq!(
            branch.atoms.len(),
            1,
            "the dotted-IRI predicate triple must survive as exactly one atom: {branch:#?}"
        );
        assert_eq!(branch.atoms[0].1, "http://ex.example/p.q");
    }

    // Gap G15: a triple pattern joined OUTSIDE a `UNION` (`?s a ex:C .` here) must be
    // distributed into EVERY branch, not dropped. The prior `split_union_branches` only
    // extracted patterns found INSIDE `{...}` groups, silently losing this shared atom.
    #[test]
    fn triple_pattern_shared_outside_union_appears_in_every_branch() {
        let get = "PREFIX ex: <http://ex.example/> \
                    CONSTRUCT { ?s <http://view.example/p> ?o } \
                    WHERE { ?s a ex:C . { ?s ex:r1 ?o } UNION { ?s ex:r2 ?o } }";
        let seeds = derive_seeds(get);
        let branch_labels: Vec<&str> = seeds
            .iter()
            .filter(|seed| seed.label.starts_with("branch-"))
            .map(|seed| seed.label.as_str())
            .collect();
        assert_eq!(branch_labels, vec!["branch-0", "branch-1"], "{seeds:#?}");
        let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        for label in ["branch-0", "branch-1"] {
            let branch = seeds
                .iter()
                .find(|seed| seed.label == label)
                .unwrap_or_else(|| panic!("{label} present"));
            assert_eq!(branch.atoms.len(), 2, "{branch:#?}");
            assert!(
                branch
                    .atoms
                    .iter()
                    .any(|(_, predicate, object)| predicate == rdf_type
                        && object == "http://ex.example/C"),
                "the shared `?s a ex:C` atom must appear in {label}: {branch:#?}"
            );
        }
    }

    // A literal value containing a dot must not be split by any character-level pass; the
    // real parser hands us the literal's lexical form as one atomic token.
    #[test]
    fn dotted_literal_object_is_not_mis_split() {
        let get = "CONSTRUCT { ?s <http://view.example/p> ?o } \
                    WHERE { ?s <http://src.example/value> \"3.14\" . }";
        let seeds = derive_seeds(get);
        let branch = seeds
            .iter()
            .find(|seed| seed.label == "branch-0")
            .expect("one branch for the single BGP");
        assert_eq!(branch.atoms.len(), 1, "{branch:#?}");
        assert_eq!(branch.atoms[0].2, "3.14");
    }
}
