// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native correspondence-law execution.
//!
//! This module is the single behavioural authority for correspondence recovery.  Both the
//! pipeline's mapping-cell laws and the compiler's five correspondence gates call the same
//! native executors. Formula and mapping-query legs use prepared `CONSTRUCT`
//! programs; atomic property legs use the stateful native focus/complement
//! primitive in [`atomic_lens`]. Comparisons cover the complete RDF carrier.
//! Canonical recovery formulas and resolved paths lower directly to native query
//! algebra, admitted by PurRDF before execution. Only the explicit text-query entry
//! points parse SPARQL; generated recovery legs have no serialization/parsing boundary.
//!
//! A first-class [`RecoveryCaseIr`](gmeow_logic_compile::ir::RecoveryCaseIr) supplies the complete query-class source pattern and its
//! ordered source-to-view transform as canonical `logic:Formula`.  The supported execution
//! fragment is `forall(vars, source -> view)`, where both sides are positive conjunctions of
//! binary RDF atoms.  The executor deterministically instantiates the source, lowers the
//! implication to get/put `CONSTRUCT`s, runs both, and returns a countermodel on information
//! loss.  Recovery evidence is not an independent semantic source: the executor also runs the
//! correspondence's resolved `get` and `put`
//! [`LegPath`](gmeow_logic_compile::ir::LegPath) bodies on that same complete seed,
//! requires their endpoint relations to agree under inversion, and requires every variable-bound
//! endpoint selected by the executable `get` relation to survive in the formula's view.  This
//! makes the evidence neutral: the same mechanism proves a genuine recovery and refutes either a
//! lossy formula or an unrelated executable leg body.
//!
//! Atomic pure-path renames retain a synthesized one-triple recovery case for the large mapping
//! surface.  Composite paths do not: endpoints alone cannot recover their hidden intermediate
//! nodes, so a `Seq`/`Alt` correspondence must author a complete recovery case instead of
//! passing because `put` was mechanically minted as `get.invert()`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub mod atomic_lens;
pub mod axes;
pub mod physical_plan;
pub mod plan_projection;
pub mod presentation;
mod stable_digest;

use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, DischargeCondition, DischargeVerdict, Formula, LawClaimIr,
    LegPath, MorphismClass, RecoveryCaseIr, Term,
};
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::projections::correspondence_gates::{
    CorrespondenceVerdicts, ExecutedCorrespondenceLaws,
};
use gmeow_logic_compile::projections::paths::lower_leg_path;
use purrdf::ir::import::DatasetImporter;
use purrdf::sparql::{
    GraphPattern, NamedNode, NamedNodePattern, NativeSparqlEngine, PreparedQuery, QuadPattern,
    Query, QueryOptions, SparqlParser, TermPattern, TriplePattern as SparqlTriplePattern, Variable,
};
use purrdf::{
    RdfDataset, RdfLiteral, RdfQuad, RdfTerm, RdfTriple, SparqlResult, TermValue, canonical_relabel,
};

const VIEW_PREDICATE: &str = "https://blackcatinformatics.ca/logic/recovery#view";
const ATOMIC_SEED_SUBJECT: &str =
    "https://blackcatinformatics.ca/logic/recovery-seed/atomic/subject";
const ATOMIC_SEED_OBJECT: &str = "https://blackcatinformatics.ca/logic/recovery-seed/atomic/object";
const RECOVERY_SEED_BASE: &str = "https://blackcatinformatics.ca/logic/recovery-seed/var/";

/// Exact namespaces containing every generated recovery-execution IRI ([`VIEW_PREDICATE`],
/// [`ATOMIC_SEED_SUBJECT`], [`ATOMIC_SEED_OBJECT`], [`RECOVERY_SEED_BASE`]-derived bindings).
/// An authored recovery-case formula or atomic leg predicate that collides with either namespace
/// could make a generated seed/binding IRI equal an authored constant, collapsing two distinct
/// terms into one in the seed graph and letting a lossy correspondence FALSELY discharge.  The
/// guard below rejects that at lowering time, before any seed graph is built, so the generated
/// IRIs stay disjoint from authored constants by construction.
const RECOVERY_VIEW_NS: &str = "https://blackcatinformatics.ca/logic/recovery#";
const RECOVERY_SEED_NS: &str = "https://blackcatinformatics.ca/logic/recovery-seed/";

/// Whether an authored IRI collides with either reserved recovery-execution namespace.
fn is_reserved_recovery_iri(iri: &str) -> bool {
    iri.starts_with(RECOVERY_VIEW_NS) || iri.starts_with(RECOVERY_SEED_NS)
}

/// A comparable RDF assertion: subject, predicate, object and graph as canonical term keys.
pub type Atom = (String, String, String, Option<String>);

/// A deterministic source graph used to discharge a correspondence law.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedGraph {
    /// Stable case or branch label used by countermodels.
    pub label: String,
    /// Typed source assertions, including literal identity, quoted terms and graph names.
    pub quads: Vec<RdfQuad>,
}

impl SeedGraph {
    /// Construct an explicitly all-IRI default-graph seed.
    pub fn from_iri_atoms(label: impl Into<String>, atoms: Vec<(String, String, String)>) -> Self {
        Self {
            label: label.into(),
            quads: atoms
                .into_iter()
                .map(|(subject, predicate, object)| {
                    RdfQuad::new(RdfTerm::iri(subject), predicate, RdfTerm::iri(object))
                })
                .collect(),
        }
    }

    fn dataset(&self) -> gmeow_errors::Result<Arc<RdfDataset>> {
        purrdf::native_quads::flat_dataset_from_quads(&self.quads)
            .map_err(|error| exec_error(format!("freeze correspondence seed: {error}")))
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
    /// Named graph declarations introduced by recovery, including empty graphs.
    pub spurious_graphs: Vec<String>,
    /// Named graph declarations discarded by recovery, including empty graphs.
    pub missing_graphs: Vec<String>,
}

/// The three-valued result of executing a law over a seed corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DischargeOutcome {
    /// Discharged, violated, or unknown when no case was available.
    pub verdict: DischargeVerdict,
    /// The first deterministic countermodel when violated.
    pub countermodel: Option<Countermodel>,
    /// Why carrier comparison could not decide a law; this is not a countermodel.
    pub comparison_refusal: Option<String>,
}

/// The executable lowering of one canonical recovery formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryExecution {
    /// Deterministically instantiated complete source graph.
    pub seed: SeedGraph,
    /// Source-to-view native `CONSTRUCT` algebra.
    pub get: Query,
    /// View-to-source candidate inverse native `CONSTRUCT` algebra.
    pub put: Query,
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
            let direction = lit
                .direction
                .map(|direction| format!("--{}", direction.as_str()))
                .unwrap_or_default();
            format!("{:?}{language}{direction}{datatype}", lit.lexical_form)
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
        quad.graph_name.as_ref().map(term_key),
    )
}

fn run_construct(
    engine: &NativeSparqlEngine,
    dataset: &Arc<RdfDataset>,
    query: &PreparedQuery,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let result = engine
        .query_prepared(dataset, query, &[], QueryOptions::EMPTY)
        .map_err(|error| {
            exec_error(format!(
                "correspondence CONSTRUCT evaluation failed: {error}"
            ))
        })?;
    let SparqlResult::Graph(dataset) = result else {
        return Err(exec_error(
            "correspondence CONSTRUCT did not return a graph",
        ));
    };
    Ok(dataset)
}

fn atom_set(dataset: &RdfDataset) -> BTreeSet<Atom> {
    purrdf::native_quads::flat_rdf_quads(dataset)
        .map(|quad| quad_atom(&quad))
        .collect()
}

/// Complete native carrier comparison, including declaration-only named graphs.
#[derive(PartialEq, Eq)]
struct CarrierAtoms {
    assertions: BTreeSet<Atom>,
    graphs: BTreeSet<String>,
}

impl CarrierAtoms {
    fn read(dataset: &RdfDataset) -> Self {
        Self {
            assertions: atom_set(dataset),
            graphs: dataset
                .owned_named_graphs()
                .map(|graph| term_key(&graph))
                .collect(),
        }
    }

    fn canonical(dataset: &RdfDataset, marker: &str) -> gmeow_errors::Result<Self> {
        let canonical = if dataset.named_graphs().next().is_none() {
            canonical_relabel(dataset)
        } else {
            // Empty graph declarations are not N-Quads statements. Make their
            // incidence visible to the native canonicalizer, including when a
            // graph name also occurs in an otherwise symmetric assertion graph.
            let mut builder = purrdf::RdfDatasetBuilder::new();
            let graphs = {
                let mut importer = DatasetImporter::new(&mut builder, dataset);
                importer.append();
                dataset
                    .named_graphs()
                    .map(|graph| importer.term(graph))
                    .collect::<Vec<_>>()
            };
            let marker_id = builder.intern_iri(marker);
            for graph in graphs {
                builder.push_quad(graph, marker_id, marker_id, Some(marker_id));
            }
            let augmented = builder.freeze().map_err(|error| {
                exec_error(format!("index carrier graph declarations: {error}"))
            })?;
            canonical_relabel(&augmented)
        }
        .map_err(|error| exec_error(format!("canonicalize correspondence carrier: {error}")))?;
        let mut atoms = Self::read(&canonical);
        atoms
            .assertions
            .retain(|atom| atom.3.as_deref() != Some(marker));
        atoms.graphs.remove(marker);
        Ok(atoms)
    }
}

/// Select a shared comparison-only IRI absent from both native term inventories.
/// At most one more candidate than the combined term count is needed.
fn comparison_marker(actual: &RdfDataset, expected: &RdfDataset) -> gmeow_errors::Result<String> {
    (0..=actual.term_count().saturating_add(expected.term_count()))
        .map(|index| format!("urn:gmeow:correspondence-carrier:{index}"))
        .find(|iri| actual.term_id_by_iri(iri).is_none() && expected.term_id_by_iri(iri).is_none())
        .ok_or_else(|| {
            exec_error("no fresh carrier-comparison identity within the finite inventory bound")
        })
}

fn violated(seed: &SeedGraph, reason: String) -> DischargeOutcome {
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationViolated,
        comparison_refusal: None,
        countermodel: Some(Countermodel {
            seed_label: seed.label.clone(),
            reason,
            spurious: Vec::new(),
            missing: Vec::new(),
            spurious_graphs: Vec::new(),
            missing_graphs: Vec::new(),
        }),
    }
}

fn discharged() -> DischargeOutcome {
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationDischarged,
        comparison_refusal: None,
        countermodel: None,
    }
}

fn compare_graphs(
    seed: &SeedGraph,
    law: &str,
    actual: &RdfDataset,
    expected: &RdfDataset,
) -> DischargeOutcome {
    if CarrierAtoms::read(actual) == CarrierAtoms::read(expected) {
        return discharged();
    }
    let canonical = comparison_marker(actual, expected).and_then(|marker| {
        CarrierAtoms::canonical(actual, &marker).and_then(|actual| {
            CarrierAtoms::canonical(expected, &marker).map(|expected| (actual, expected))
        })
    });
    let (actual, expected) = match canonical {
        Ok(sets) => sets,
        Err(error) => {
            return DischargeOutcome {
                verdict: DischargeVerdict::ObligationUnknown,
                countermodel: None,
                comparison_refusal: Some(format!("{law} comparison refused: {error}")),
            };
        }
    };
    if actual == expected {
        return discharged();
    }
    let spurious: Vec<Atom> = actual
        .assertions
        .difference(&expected.assertions)
        .cloned()
        .collect();
    let missing: Vec<Atom> = expected
        .assertions
        .difference(&actual.assertions)
        .cloned()
        .collect();
    let spurious_graphs: Vec<String> = actual
        .graphs
        .difference(&expected.graphs)
        .cloned()
        .collect();
    let missing_graphs: Vec<String> = expected
        .graphs
        .difference(&actual.graphs)
        .cloned()
        .collect();
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationViolated,
        comparison_refusal: None,
        countermodel: Some(Countermodel {
            seed_label: seed.label.clone(),
            reason: format!(
                "{law} failed on seed `{}`: {} spurious, {} missing assertions; {} spurious, {} missing graph declarations",
                seed.label,
                spurious.len(),
                missing.len(),
                spurious_graphs.len(),
                missing_graphs.len()
            ),
            spurious,
            missing,
            spurious_graphs,
            missing_graphs,
        }),
    }
}

/// One explicitly supplied law case, retaining the complete immutable RDF 1.2
/// carrier, including declaration-only named graphs and statement metadata.
#[derive(Debug, Clone)]
pub struct LawInput {
    /// Stable identity of the source or independently edited view case.
    pub identity: String,
    /// Native input; a law worker borrows this carrier without flattening it.
    pub dataset: Arc<RdfDataset>,
}

enum LawCase<'a> {
    Native(&'a LawInput),
    Seed(&'a SeedGraph),
}

impl LawCase<'_> {
    fn identity(&self) -> &str {
        match self {
            Self::Native(input) => &input.identity,
            Self::Seed(seed) => &seed.label,
        }
    }

    fn dataset(&self) -> gmeow_errors::Result<Arc<RdfDataset>> {
        match self {
            Self::Native(input) => Ok(Arc::clone(&input.dataset)),
            Self::Seed(seed) => seed.dataset(),
        }
    }
}

/// Prepared native execution for the source-replacing CONSTRUCT fragment.
///
/// Both legs are compiled once and reused across separate source and view domains.
/// This fragment does not implement a stateful lens update: its put replaces the
/// source from the view. Its bounded verdicts establish neither GetPut/PutPut with
/// prior state nor an unrestricted optimization certificate.
pub struct PreparedLawExecution {
    engine: NativeSparqlEngine,
    get: Arc<PreparedQuery>,
    put: Arc<PreparedQuery>,
}

fn unknown() -> DischargeOutcome {
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationUnknown,
        comparison_refusal: None,
        countermodel: None,
    }
}

impl PreparedLawExecution {
    /// Prepare both required CONSTRUCT legs through the native engine.
    ///
    /// # Errors
    /// Refuses malformed queries and query forms that do not produce a carrier.
    pub fn new(get_query: &str, put_query: &str) -> gmeow_errors::Result<Self> {
        let engine = NativeSparqlEngine::new();
        let get = engine
            .prepare_query(get_query, None)
            .map_err(|error| exec_error(format!("prepare correspondence get: {error}")))?;
        let put = engine
            .prepare_query(put_query, None)
            .map_err(|error| exec_error(format!("prepare correspondence put: {error}")))?;
        Self::from_prepared(engine, get, put)
    }

    /// Admit compiler-produced legs directly, without a SPARQL text boundary.
    /// Uses the native engine's algebra admission and the same carrier-form check
    /// as the explicit text adapter. Inputs and verdicts remain operation-scoped.
    ///
    /// # Errors
    /// Refuses query forms that cannot produce a carrier and native admission errors.
    pub fn from_algebra(get: Query, put: Query) -> gmeow_errors::Result<Self> {
        let engine = NativeSparqlEngine::new();
        let prepare = |query| {
            engine
                .prepare_algebra(query, QueryOptions::EMPTY)
                .map_err(|error| exec_error(format!("admit correspondence algebra: {error}")))
        };
        let get = prepare(get)?;
        let put = prepare(put)?;
        Self::from_prepared(engine, get, put)
    }

    fn from_prepared(
        engine: NativeSparqlEngine,
        get: Arc<PreparedQuery>,
        put: Arc<PreparedQuery>,
    ) -> gmeow_errors::Result<Self> {
        if !matches!(get.query, Query::Construct { .. })
            || !matches!(put.query, Query::Construct { .. })
        {
            return Err(exec_error("correspondence laws require two CONSTRUCT legs"));
        }
        Ok(Self { engine, get, put })
    }

    /// Check complete source recovery on exactly these native source inputs.
    pub fn section(&self, sources: &[LawInput]) -> DischargeOutcome {
        self.roundtrip(sources.iter().map(LawCase::Native), true)
    }

    /// Check update faithfulness on exactly these independently supplied views.
    pub fn put_get(&self, views: &[LawInput]) -> DischargeOutcome {
        self.roundtrip(views.iter().map(LawCase::Native), false)
    }

    fn roundtrip<'a>(
        &self,
        inputs: impl IntoIterator<Item = LawCase<'a>>,
        source_domain: bool,
    ) -> DischargeOutcome {
        let mut ordered: Vec<LawCase<'_>> = inputs.into_iter().collect();
        ordered.sort_by(|left, right| left.identity().cmp(right.identity()));
        if ordered.is_empty() {
            return unknown();
        }
        let (first, second, law) = if source_domain {
            (&self.get, &self.put, "put∘get = id_source")
        } else {
            (&self.put, &self.get, "get∘put = id_independent_view")
        };
        // A refusal on one case must not conceal a concrete refutation on a later case.
        let mut aggregate = discharged();
        for input in ordered {
            let seed = SeedGraph {
                label: input.identity().to_owned(),
                quads: Vec::new(),
            };
            let outcome = match input.dataset().and_then(|dataset| {
                let intermediate = run_construct(&self.engine, &dataset, first)?;
                let recovered = run_construct(&self.engine, &intermediate, second)?;
                Ok((dataset, recovered))
            }) {
                Ok((dataset, recovered)) => compare_graphs(&seed, law, &recovered, &dataset),
                Err(error) => violated(&seed, format!("correspondence execution failed: {error}")),
            };
            match outcome.verdict {
                DischargeVerdict::ObligationViolated => return outcome,
                DischargeVerdict::ObligationUnknown
                    if aggregate.verdict == DischargeVerdict::ObligationDischarged =>
                {
                    aggregate = outcome;
                }
                _ => {}
            }
        }
        aggregate
    }
}

fn discharge_domain(
    prepared: &gmeow_errors::Result<PreparedLawExecution>,
    seeds: &[SeedGraph],
    source_domain: bool,
) -> DischargeOutcome {
    match prepared {
        Ok(legs) => legs.roundtrip(seeds.iter().map(LawCase::Seed), source_domain),
        Err(error) => seeds
            .iter()
            .min_by(|left, right| left.label.cmp(&right.label))
            .map_or_else(unknown, |seed| {
                violated(
                    seed,
                    format!("correspondence leg is not executable: {error}"),
                )
            }),
    }
}

/// Execute `put ∘ get = id_source` over a deterministic seed corpus.
/// This is bounded evidence on these seeds, not universal rewrite authority.
pub fn discharge_section_law(
    get_query: &str,
    put_query: &str,
    seeds: &[SeedGraph],
) -> DischargeOutcome {
    discharge_domain(
        &PreparedLawExecution::new(get_query, put_query),
        seeds,
        true,
    )
}

/// Execute `get ∘ put = id_view` on independently supplied view seeds.
/// The seeds belong to the view domain, not the source or its forward image.
/// This is bounded evidence for the source-replacing CONSTRUCT fragment, not
/// a claim about stateful put, arbitrary views, GetPut or PutPut.
pub fn discharge_put_get_law(
    get_query: &str,
    put_query: &str,
    views: &[SeedGraph],
) -> DischargeOutcome {
    discharge_domain(
        &PreparedLawExecution::new(get_query, put_query),
        views,
        false,
    )
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
        Term::Literal(_) => Err(exec_error(format!(
            "{position} literals are outside the recovery-case RDF-atom fragment"
        ))),
        Term::SequenceMarker(_) => Err(exec_error(format!(
            "{position} sequence markers are outside the recovery-case RDF-atom fragment"
        ))),
        Term::App { .. } => Err(exec_error(format!(
            "{position} compound function terms are outside the recovery-case RDF-atom fragment"
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
                "{side} atom predicate <{}> uses a reserved recovery-execution namespace \
                 (`{RECOVERY_VIEW_NS}` or `{RECOVERY_SEED_NS}`); author IRIs outside those \
                 namespaces so generated seed bindings stay fresh",
                pattern.predicate
            )));
        }
        for (position, term) in [("subject", &pattern.subject), ("object", &pattern.object)] {
            if let PatternTerm::Iri(iri) = term
                && is_reserved_recovery_iri(iri)
            {
                return Err(exec_error(format!(
                    "{side} {position} <{iri}> uses a reserved recovery-execution namespace \
                     (`{RECOVERY_VIEW_NS}` or `{RECOVERY_SEED_NS}`); author IRIs outside those \
                     namespaces so generated seed bindings stay fresh"
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

/// Validate a compiler-produced IRI before handing algebra to the native engine.
fn algebra_iri(iri: &str) -> gmeow_errors::Result<NamedNode> {
    NamedNode::new(iri)
        .map_err(|error| exec_error(format!("invalid recovery IRI <{iri}>: {error}")))
}

fn algebra_variable(variable: &str) -> gmeow_errors::Result<Variable> {
    if !valid_variable(variable) {
        return Err(exec_error(format!("invalid recovery variable {variable}")));
    }
    Ok(Variable::new(variable))
}

fn algebra_term(term: &PatternTerm) -> gmeow_errors::Result<TermPattern> {
    match term {
        PatternTerm::Iri(iri) => algebra_iri(iri).map(TermPattern::NamedNode),
        PatternTerm::Var(variable) => algebra_variable(variable).map(TermPattern::Variable),
    }
}

/// Lower the already-admitted binary formula patterns directly to native algebra.
fn algebra_patterns(patterns: &[TriplePattern]) -> gmeow_errors::Result<Vec<SparqlTriplePattern>> {
    patterns
        .iter()
        .map(|pattern| {
            Ok(SparqlTriplePattern {
                subject: algebra_term(&pattern.subject)?,
                predicate: NamedNodePattern::NamedNode(algebra_iri(&pattern.predicate)?),
                object: algebra_term(&pattern.object)?,
            })
        })
        .collect()
}

fn construct_algebra(
    template: Vec<SparqlTriplePattern>,
    patterns: Vec<SparqlTriplePattern>,
) -> Query {
    Query::Construct {
        template: template
            .into_iter()
            .map(|triple| QuadPattern {
                triple,
                graph: None,
            })
            .collect(),
        pattern: GraphPattern::Bgp { patterns },
        dataset: Default::default(),
        base_iri: None,
        version: None,
    }
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

/// Lower one canonical `logic:RecoveryCase` formula to native get/put algebra and a
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
    let atoms: Vec<(String, String, String)> = source_patterns
        .iter()
        .map(|pattern| {
            (
                instantiate_term(&pattern.subject, &bindings),
                pattern.predicate.clone(),
                instantiate_term(&pattern.object, &bindings),
            )
        })
        .collect();
    let seed = SeedGraph::from_iri_atoms(case.iri.clone(), atoms);
    let source = algebra_patterns(&source_patterns)?;
    let view = algebra_patterns(&view_patterns)?;
    Ok(RecoveryExecution {
        seed,
        get: construct_algebra(view.clone(), source.clone()),
        put: construct_algebra(source, view),
    })
}

type EndpointRelation = BTreeSet<(String, String)>;

/// Admit the same resolved path the text parser previously guarded, then use
/// the shared compiler lowering. An empty composite is not an executable path.
fn leg_relation_algebra(path: &LegPath) -> gmeow_errors::Result<Query> {
    let mut pending = vec![path];
    while let Some(part) = pending.pop() {
        match part {
            LegPath::Step(iri) => {
                algebra_iri(iri)?;
            }
            LegPath::Inverse(inner) => pending.push(inner),
            LegPath::Seq(parts) | LegPath::Alt(parts) => {
                if parts.is_empty() {
                    return Err(exec_error(
                        "resolved recovery path has an empty sequence or alternative",
                    ));
                }
                pending.extend(parts);
            }
        }
    }
    let subject = algebra_variable("s")?;
    let object = algebra_variable("o")?;
    Ok(Query::Select {
        pattern: GraphPattern::Project {
            inner: Box::new(GraphPattern::Path {
                subject: TermPattern::Variable(subject.clone()),
                path: lower_leg_path(path),
                object: TermPattern::Variable(object.clone()),
            }),
            variables: vec![subject, object],
        },
        dataset: Default::default(),
        base_iri: None,
        version: None,
    })
}

/// Convert a selected endpoint into the deterministic key used for relation comparison.
fn endpoint_key(term: &TermValue) -> gmeow_errors::Result<String> {
    match term {
        TermValue::Iri(iri) => Ok(iri.clone()),
        _ => crate::provenance::term_n3(term),
    }
}

/// Execute one prepared resolved leg body against the complete recovery seed.
fn execute_leg_relation(
    engine: &NativeSparqlEngine,
    dataset: &Arc<RdfDataset>,
    query: &PreparedQuery,
) -> gmeow_errors::Result<EndpointRelation> {
    let result = engine
        .query_prepared(dataset, query, &[], QueryOptions::EMPTY)
        .map_err(|error| {
            exec_error(format!(
                "correspondence leg SELECT evaluation failed: {error}"
            ))
        })?;
    let SparqlResult::Solutions {
        variables, rows, ..
    } = result
    else {
        return Err(exec_error(
            "correspondence leg SELECT did not return solutions",
        ));
    };
    let subject_index = variables
        .iter()
        .position(|variable| variable == "s")
        .ok_or_else(|| exec_error("correspondence leg SELECT omitted ?s".to_owned()))?;
    let object_index = variables
        .iter()
        .position(|variable| variable == "o")
        .ok_or_else(|| exec_error("correspondence leg SELECT omitted ?o".to_owned()))?;

    let mut relation = EndpointRelation::new();
    for row in rows {
        let subject = row
            .get(subject_index)
            .and_then(Option::as_ref)
            .ok_or_else(|| exec_error("correspondence leg SELECT left ?s unbound".to_owned()))?;
        let object = row
            .get(object_index)
            .and_then(Option::as_ref)
            .ok_or_else(|| exec_error("correspondence leg SELECT left ?o unbound".to_owned()))?;
        relation.insert((endpoint_key(subject)?, endpoint_key(object)?));
    }
    Ok(relation)
}

/// Build a deterministic countermodel for two endpoint relations that should agree.
fn relation_mismatch(
    seed: &SeedGraph,
    reason: String,
    actual: &EndpointRelation,
    expected: &EndpointRelation,
) -> DischargeOutcome {
    let as_atoms = |relation: &EndpointRelation| {
        relation
            .iter()
            .map(|(subject, object)| {
                (
                    subject.clone(),
                    VIEW_PREDICATE.to_owned(),
                    object.clone(),
                    None,
                )
            })
            .collect::<BTreeSet<_>>()
    };
    let actual = as_atoms(actual);
    let expected = as_atoms(expected);
    DischargeOutcome {
        verdict: DischargeVerdict::ObligationViolated,
        comparison_refusal: None,
        countermodel: Some(Countermodel {
            seed_label: seed.label.clone(),
            reason,
            spurious: actual.difference(&expected).cloned().collect(),
            missing: expected.difference(&actual).cloned().collect(),
            spurious_graphs: Vec::new(),
            missing_graphs: Vec::new(),
        }),
    }
}

/// The two resolved relations shared by every recovery case of one correspondence.
/// The resolved paths use the shared compiler's native algebra lowering. This
/// operation-scoped holder never retains case datasets or an earlier case's result.
struct PreparedRecoveryLegs {
    engine: NativeSparqlEngine,
    get: Arc<PreparedQuery>,
    put: Arc<PreparedQuery>,
}

impl PreparedRecoveryLegs {
    fn new(get: &LegPath, put: &LegPath) -> gmeow_errors::Result<Self> {
        let engine = NativeSparqlEngine::new();
        let prepare = |path| {
            engine
                .prepare_algebra(leg_relation_algebra(path)?, QueryOptions::EMPTY)
                .map_err(|error| exec_error(format!("prepare resolved recovery leg: {error}")))
        };
        let get = prepare(get)?;
        let put = prepare(put)?;
        Ok(Self { engine, get, put })
    }

    /// Run a complete formula recovery and both already-prepared resolved legs.
    fn discharge(&self, case: &RecoveryCaseIr) -> DischargeOutcome {
        let execution = match lower_recovery_case(case) {
            Ok(execution) => execution,
            Err(reason) => {
                return violated(
                    &SeedGraph {
                        label: case.iri.clone(),
                        quads: Vec::new(),
                    },
                    format!("recovery case is not executable: {reason}"),
                );
            }
        };
        let engine = &self.engine;
        let graphs = execution.seed.dataset().and_then(|source| {
            let get = engine
                .prepare_algebra(execution.get, QueryOptions::EMPTY)
                .map_err(|error| exec_error(format!("prepare recovery get: {error}")))?;
            let put = engine
                .prepare_algebra(execution.put, QueryOptions::EMPTY)
                .map_err(|error| exec_error(format!("prepare recovery put: {error}")))?;
            let view = run_construct(engine, &source, &get)?;
            let recovered = run_construct(engine, &view, &put)?;
            Ok((source, view, recovered))
        });
        let (source, formula_view, recovered) = match graphs {
            Ok(graphs) => graphs,
            Err(error) => {
                return violated(
                    &execution.seed,
                    format!("recovery case execution failed: {error}"),
                );
            }
        };
        let formula_outcome =
            compare_graphs(&execution.seed, "put∘get = id_source", &recovered, &source);
        if formula_outcome.verdict != DischargeVerdict::ObligationDischarged {
            return formula_outcome;
        }

        let get_relation = match execute_leg_relation(engine, &source, &self.get) {
            Ok(relation) => relation,
            Err(error) => {
                return violated(
                    &execution.seed,
                    format!("resolved get leg body is not executable: {error}"),
                );
            }
        };
        if get_relation.is_empty() {
            return violated(
                &execution.seed,
                "resolved get leg body produced no relation on the recovery seed".to_owned(),
            );
        }

        let put_relation = match execute_leg_relation(engine, &source, &self.put) {
            Ok(relation) => relation,
            Err(error) => {
                return violated(
                    &execution.seed,
                    format!("resolved put leg body is not executable: {error}"),
                );
            }
        };
        if put_relation.is_empty() {
            return violated(
                &execution.seed,
                "resolved put leg body produced no relation on the recovery seed".to_owned(),
            );
        }
        let recovered_get: EndpointRelation = put_relation
            .into_iter()
            .map(|(subject, object)| (object, subject))
            .collect();
        if recovered_get != get_relation {
            return relation_mismatch(
                &execution.seed,
                "resolved get and put leg bodies disagree under inversion on the recovery seed"
                    .to_owned(),
                &get_relation,
                &recovered_get,
            );
        }

        let formula_view_terms: BTreeSet<String> =
            purrdf::native_quads::flat_rdf_quads(&formula_view)
                .flat_map(|quad| [term_key(&quad.subject), term_key(&quad.object)])
                .collect();
        let unwitnessed_bindings: BTreeSet<String> = get_relation
            .iter()
            .flat_map(|(subject, object)| [subject, object])
            .filter(|term| term.starts_with(RECOVERY_SEED_BASE))
            .filter(|term| !formula_view_terms.contains(*term))
            .cloned()
            .collect();
        if !unwitnessed_bindings.is_empty() {
            return violated(
                &execution.seed,
                format!(
                    "resolved get leg binds recovery variables absent from the formula view: {}",
                    unwitnessed_bindings
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }

        DischargeOutcome {
            verdict: DischargeVerdict::ObligationDischarged,
            comparison_refusal: None,
            countermodel: None,
        }
    }
}

/// Execute one recovery case and cross-check it against the correspondence's actual resolved
/// leg bodies.  An out-of-fragment formula, an unexecutable or empty leg relation, or any
/// formula/body disagreement is an explicit violated obligation with a countermodel reason,
/// never a silently skipped case.
pub fn discharge_recovery_case(
    case: &RecoveryCaseIr,
    get: &LegPath,
    put: &LegPath,
) -> DischargeOutcome {
    match PreparedRecoveryLegs::new(get, put) {
        Ok(legs) => legs.discharge(case),
        Err(error) => violated(
            &SeedGraph {
                label: case.iri.clone(),
                quads: Vec::new(),
            },
            format!("resolved recovery legs are not executable: {error}"),
        ),
    }
}

/// Require every attached recovery case to discharge against the same resolved leg bodies.
fn discharge_recovery_cases(
    correspondence: &Correspondence,
    get: &LegPath,
    put: &LegPath,
) -> DischargeOutcome {
    let Some(first) = correspondence.recovery_cases.first() else {
        return unknown();
    };
    let legs = match PreparedRecoveryLegs::new(get, put) {
        Ok(legs) => legs,
        Err(error) => {
            return violated(
                &SeedGraph {
                    label: first.iri.clone(),
                    quads: Vec::new(),
                },
                format!("resolved recovery legs are not executable: {error}"),
            );
        }
    };
    let mut aggregate = discharged();
    for case in &correspondence.recovery_cases {
        let outcome = legs.discharge(case);
        match outcome.verdict {
            DischargeVerdict::ObligationViolated => return outcome,
            DischargeVerdict::ObligationUnknown
                if aggregate.verdict == DischargeVerdict::ObligationDischarged =>
            {
                aggregate = outcome;
            }
            _ => {}
        }
    }
    aggregate
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

/// Execute the synthesized complete one-triple recovery case for a pure atomic rename.
/// Composite paths return `ObligationUnknown`: their hidden intermediate graph is not
/// recoverable from endpoint equality and must be supplied by a first-class recovery case.
pub fn leg_pair_verdict(get: &LegPath, put: &LegPath) -> DischargeVerdict {
    let recovered = put.invert();
    let (Some(get_path), Some(recovered_path)) = (atomic_path(get), atomic_path(&recovered)) else {
        return DischargeVerdict::ObligationUnknown;
    };
    // The atomic get/put predicates are authored constants.  If either collides with the
    // reserved recovery-execution namespaces (`RECOVERY_VIEW_NS` / `RECOVERY_SEED_NS`), the
    // synthesized atomic seed (`ATOMIC_SEED_SUBJECT`/`ATOMIC_SEED_OBJECT`) or its
    // `VIEW_PREDICATE` carrier could collapse with an authored term and FALSELY discharge.
    // `ObligationUnknown` is not acceptable here — the gates below treat Unknown as "honest and
    // passes", which would let the collision through silently.  `ObligationViolated` fails
    // closed: it reds the Law, Round-trip, and Mnemomorphism gates exactly like a genuine put∘get
    // counterexample, and is diagnosable because the gates' fixed-template refutation clause
    // already names the failing correspondence IRI.
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
    let seed = SeedGraph::from_iri_atoms("synthesized-atomic-path", vec![source_atom]);
    let executed = (|| -> gmeow_errors::Result<DischargeOutcome> {
        let get = atomic_lens::AtomicPropertyLens::new(
            get_path.predicate,
            VIEW_PREDICATE,
            get_path.inverse,
            purrdf::ViewLimits::default(),
        )?;
        let put = atomic_lens::AtomicPropertyLens::new(
            recovered_path.predicate,
            VIEW_PREDICATE,
            recovered_path.inverse,
            purrdf::ViewLimits::default(),
        )?;
        let source = seed.dataset()?;
        let projected = get.acquire(Arc::clone(&source))?.get();
        // Run the independently authored candidate put against an explicitly
        // empty initial state. Reusing the forward leg's complement or inverse would
        // conceal a wrong candidate predicate or direction.
        let empty = SeedGraph::from_iri_atoms("atomic-initial-state", Vec::new()).dataset()?;
        let recovered = put
            .acquire(empty)?
            .put_shared_scopes(Arc::clone(projected.dataset()))?;
        let actual = recovered
            .carrier()
            .materialize()
            .map_err(|error| exec_error(format!("compare atomic recovery: {error}")))?;
        Ok(compare_graphs(
            &seed,
            "put∘get = id_source",
            &actual,
            &source,
        ))
    })();
    executed
        .unwrap_or_else(|error| {
            violated(&seed, format!("atomic recovery execution failed: {error}"))
        })
        .verdict
}

/// Compute the executed recovery verdict for every correspondence.
///
/// First-class recovery cases are authoritative evidence, but never an independent semantic
/// source: their formula execution is cross-checked against both resolved leg bodies.  When no
/// cases are authored, only a complete atomic path rename can synthesize its one-triple case.
/// Missing/unresolvable or composite legs remain Unknown without evidence; missing legs are a
/// violated obligation when recovery evidence claims that executable bodies exist.
pub fn program_verdicts(program: &CorrespondenceProgram) -> CorrespondenceVerdicts {
    let mut verdicts = BTreeMap::new();
    for correspondence in &program.correspondences {
        let get = correspondence
            .get_leg
            .as_deref()
            .and_then(|iri| program.resolve_leg(iri));
        let put = correspondence
            .put_leg
            .as_deref()
            .and_then(|iri| program.resolve_leg(iri));
        let verdict = match (correspondence.recovery_cases.is_empty(), get, put) {
            (true, Some(get), Some(put)) => leg_pair_verdict(get, put),
            (true, _, _) => DischargeVerdict::ObligationUnknown,
            (false, Some(get), Some(put)) => {
                discharge_recovery_cases(correspondence, get, put).verdict
            }
            (false, _, _) => DischargeVerdict::ObligationViolated,
        };
        verdicts.insert(
            correspondence.iri.clone(),
            ExecutedCorrespondenceLaws::section_only(verdict),
        );
    }
    verdicts
}

// Mapping-cell branch-covering seed derivation.  The `get` leg is a SPARQL `CONSTRUCT`; its
// `WHERE` clause is parsed by the SAME real algebra (`purrdf::sparql`) the executor above runs
// it through, then walked into disjunctive-normal-form branches.  A hand-rolled text splitter
// would be a second, weaker parser for a fragment the real one already covers exactly.

/// Deterministic base for fresh per-variable seed IRIs.  Kept byte-identical to the prior
/// scheme (`http://seed.example/v{n}`) so the branch-covering / determinism tests in
/// `crates/pipeline/src/correspondence_law.rs` are unaffected by this rewrite.
const BRANCH_SEED_BASE: &str = "http://seed.example/v";

#[derive(Clone)]
struct SeedPattern {
    triple: SparqlTriplePattern,
    graph: Option<NamedNodePattern>,
}

/// Admission envelope for the synthetic recovery corpus, independent of runtime
/// query budgets. Check products BEFORE allocating their distributed branches.
const MAX_SEED_BRANCHES: usize = 4096;
const MAX_SEED_PATTERNS: usize = 65_536;
const MAX_SEED_ALGEBRA_DEPTH: usize = 128;

fn admit_seed_dimensions(branches: usize, patterns: usize) -> gmeow_errors::Result<()> {
    if branches > MAX_SEED_BRANCHES || patterns > MAX_SEED_PATTERNS {
        return Err(exec_error(format!(
            "correspondence recovery corpus exceeds admission limits: \
             {branches} branches / {MAX_SEED_BRANCHES}, \
             {patterns} patterns / {MAX_SEED_PATTERNS}; no partial domain is admitted"
        )));
    }
    Ok(())
}

fn seed_pattern_count(branches: &[Vec<SeedPattern>]) -> usize {
    branches.iter().map(Vec::len).sum()
}

/// Recursively enumerate the `WHERE` algebra into disjunctive branches: one `Vec` of triple
/// patterns per top-level `UNION` disjunct.  `Join`/`Lateral` distribute as a cartesian
/// product, so a pattern joined OUTSIDE a `UNION` — the shared-atom case the old text splitter
/// dropped — appears in every resulting branch.  `LeftJoin` (`OPTIONAL`) keeps only its
/// required (left) side: SPARQL OPTIONAL semantics do not require the right side to match, so
/// its content is not a positive obligation the seed corpus must recover — forcing it into
/// every seed would wrongly demand round-tripping data that is, by construction, optional.
/// `Filter`/`Extend` (`BIND`)/`Unfold` (`UNFOLD`)/`Graph`/solution-modifier wrappers contribute
/// no atoms of their own and are unwrapped down to their inner pattern; `Minus` likewise keeps
/// only its required (left) side. Constructs with no positive triple-pattern content (`Path`,
/// `Service`,
/// `Values`, and a configured `PropertyFunction` call — a computed relation, not asserted
/// triples the seed corpus could recover) yield no branches — deterministically dropped,
/// never guessed at.
fn dnf_branches(
    pattern: &GraphPattern,
    depth: usize,
) -> gmeow_errors::Result<Vec<Vec<SeedPattern>>> {
    if depth > MAX_SEED_ALGEBRA_DEPTH {
        return Err(exec_error(format!(
            "correspondence recovery algebra exceeds depth {MAX_SEED_ALGEBRA_DEPTH}"
        )));
    }
    let branches = match pattern {
        GraphPattern::Bgp { patterns } => {
            admit_seed_dimensions(1, patterns.len())?;
            vec![
                patterns
                    .iter()
                    .map(|triple| SeedPattern {
                        triple: triple.clone(),
                        graph: None,
                    })
                    .collect(),
            ]
        }
        GraphPattern::Join { left, right } | GraphPattern::Lateral { left, right } => {
            let left_branches = dnf_branches(left, depth + 1)?;
            let right_branches = dnf_branches(right, depth + 1)?;
            match (left_branches.is_empty(), right_branches.is_empty()) {
                (true, true) => Vec::new(),
                (true, false) => right_branches,
                (false, true) => left_branches,
                (false, false) => {
                    // Both inputs were admitted, so these products fit usize on
                    // supported targets. Bound the expanded corpus before cloning.
                    let branch_count = left_branches.len() * right_branches.len();
                    let pattern_count = seed_pattern_count(&left_branches) * right_branches.len()
                        + seed_pattern_count(&right_branches) * left_branches.len();
                    admit_seed_dimensions(branch_count, pattern_count)?;
                    let mut out = Vec::with_capacity(branch_count);
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
            let mut out = dnf_branches(left, depth + 1)?;
            let right = dnf_branches(right, depth + 1)?;
            admit_seed_dimensions(
                out.len() + right.len(),
                seed_pattern_count(&out) + seed_pattern_count(&right),
            )?;
            out.extend(right);
            out
        }
        GraphPattern::LeftJoin { left, .. } | GraphPattern::Minus { left, .. } => {
            dnf_branches(left, depth + 1)?
        }
        GraphPattern::Graph { name, inner } => {
            let mut branches = dnf_branches(inner, depth + 1)?;
            for branch in &mut branches {
                for pattern in branch {
                    pattern.graph.get_or_insert_with(|| name.clone());
                }
            }
            branches
        }
        GraphPattern::Filter { inner, .. }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::Unfold { inner, .. }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Group { inner, .. } => dnf_branches(inner, depth + 1)?,
        GraphPattern::Path { .. }
        | GraphPattern::Service { .. }
        | GraphPattern::Values { .. }
        | GraphPattern::PropertyFunction(_) => Vec::new(),
    };
    Ok(branches)
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

/// Instantiate an algebra term without losing literal or nested quoted-triple identity.
fn resolve_term_pattern(
    term: &TermPattern,
    bindings: &mut BTreeMap<String, String>,
    counter: &mut usize,
) -> RdfTerm {
    match term {
        TermPattern::NamedNode(iri) => RdfTerm::iri(iri.as_str()),
        TermPattern::Variable(variable) => {
            RdfTerm::iri(fresh_binding(variable.as_str(), bindings, counter))
        }
        TermPattern::Literal(literal) => RdfTerm::literal(RdfLiteral {
            lexical_form: literal.value().to_owned(),
            datatype: Some(literal.datatype().as_str().to_owned()),
            language: literal.language().map(str::to_owned),
            direction: literal.direction().map(|direction| match direction {
                purrdf::sparql::BaseDirection::Ltr => purrdf::RdfTextDirection::Ltr,
                purrdf::sparql::BaseDirection::Rtl => purrdf::RdfTextDirection::Rtl,
            }),
        }),
        TermPattern::BlankNode(blank) => RdfTerm::iri(fresh_binding(
            &format!("_:{}", blank.as_str()),
            bindings,
            counter,
        )),
        TermPattern::Triple(triple) => RdfTerm::triple(RdfTriple::new(
            resolve_term_pattern(&triple.subject, bindings, counter),
            resolve_named_node_pattern(&triple.predicate, bindings, counter),
            resolve_term_pattern(&triple.object, bindings, counter),
        )),
    }
}

/// Instantiate a branch with branch-local bindings and a corpus-wide fresh IRI counter.
fn instantiate_branch(branch: &[SeedPattern], counter: &mut usize) -> Vec<RdfQuad> {
    let mut bindings = BTreeMap::new();
    branch
        .iter()
        .map(|seed_pattern| {
            let pattern = &seed_pattern.triple;
            let mut quad = RdfQuad::new(
                resolve_term_pattern(&pattern.subject, &mut bindings, counter),
                resolve_named_node_pattern(&pattern.predicate, &mut bindings, counter),
                resolve_term_pattern(&pattern.object, &mut bindings, counter),
            );
            quad.graph_name = seed_pattern.graph.as_ref().map(|graph| {
                RdfTerm::iri(resolve_named_node_pattern(graph, &mut bindings, counter))
            });
            quad
        })
        .collect()
}

/// Derive one deterministic seed per top-level `UNION` branch of `get_query`'s `WHERE` algebra
/// (a pattern joined outside a `UNION` is distributed into every branch) plus one combined
/// seed unioning all branches.
///
/// # Errors
/// Refuses malformed/non-CONSTRUCT queries and excessive branch, pattern or depth
/// expansion. A refusal never supplies an empty or truncated domain as evidence.
pub fn derive_seeds(get_query: &str) -> gmeow_errors::Result<Vec<SeedGraph>> {
    let query = SparqlParser::new()
        .parse_query(get_query)
        .map_err(|error| exec_error(format!("parse correspondence recovery query: {error}")))?;
    derive_query_seeds(&query)
}

/// Reuse the exact prepared algebra for analysis; parsing is confined to the
/// public text-input adapter above.
fn derive_query_seeds(query: &Query) -> gmeow_errors::Result<Vec<SeedGraph>> {
    let Query::Construct { pattern, .. } = query else {
        return Err(exec_error(
            "correspondence recovery cases require a CONSTRUCT query",
        ));
    };
    let branches = dnf_branches(pattern, 0)?;
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
            quads: atoms,
        });
    }
    if !combined.is_empty() {
        seeds.push(SeedGraph {
            label: "combined".to_owned(),
            quads: combined,
        });
    }
    Ok(seeds)
}

fn claim_from(law: CorrespondenceLaw, outcome: &DischargeOutcome) -> LawClaimIr {
    LawClaimIr {
        law,
        verdict: outcome.verdict,
        condition: (outcome.verdict != DischargeVerdict::ObligationUnknown)
            .then_some(DischargeCondition::DischargeBoundedCorpus),
    }
}

/// Check source recovery and independent edited views in the source-replacing
/// CONSTRUCT fragment for a mapping cell that requests round-trip checking.
/// These bounded checks do not discharge stateful GetPut or PutPut obligations.
///
/// # Errors
/// Refuses inadmissible legs or synthetic domains before publishing any claims.
pub fn discharge_laws(
    get_query: &str,
    put_query: &str,
    rung: MorphismClass,
) -> gmeow_errors::Result<Vec<LawClaimIr>> {
    if !rung.is_injective_rung() {
        return Ok(Vec::new());
    }
    PreparedLawExecution::new(get_query, put_query)?.discharge_laws(rung)
}

/// Discharge compiler-produced mapping legs through native algebra admission.
/// No text is rendered or parsed. Domains and evidence remain bounded exactly as
/// for explicitly supplied queries; this is not a stateful-lens certificate.
pub fn discharge_algebra_laws(
    get: Query,
    put: Query,
    rung: MorphismClass,
) -> gmeow_errors::Result<Vec<LawClaimIr>> {
    if !rung.is_injective_rung() {
        return Ok(Vec::new());
    }
    PreparedLawExecution::from_algebra(get, put)?.discharge_laws(rung)
}

impl PreparedLawExecution {
    /// Execute each permitted law on its own independently synthesized domain.
    pub fn discharge_laws(&self, rung: MorphismClass) -> gmeow_errors::Result<Vec<LawClaimIr>> {
        if !rung.is_injective_rung() {
            return Ok(Vec::new());
        }
        let sources = derive_query_seeds(&self.get.query)?;
        let views = derive_query_seeds(&self.put.query)?;
        let section = self.roundtrip(sources.iter().map(LawCase::Seed), true);
        let put_get = self.roundtrip(views.iter().map(LawCase::Seed), false);
        Ok(vec![
            claim_from(CorrespondenceLaw::SectionLaw, &section),
            claim_from(CorrespondenceLaw::PutGet, &put_get),
        ])
    }
}

#[path = "correspondence_exec.tests.rs"]
#[cfg(test)]
mod tests;
