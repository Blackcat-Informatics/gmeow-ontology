// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, lawful up-projection executor: the `put`-leg cutover.
//!
//! This module lifts external-vocabulary source triples to GMEOW by running each
//! *gate-verified* alignment rule as a SPARQL `CONSTRUCT` — the "put leg" — through the
//! native [`NativeSparqlEngine`]. The set of external terms it lifts, their orientation
//! (direct vs inverse), and their gmeow target come SOLELY from the gate-derived audit's
//! lift program ([`gate_verified_lift_program`]): the single source of truth that both this
//! executor and the audit ledger consume. A term the audit RED-excludes (its reverse path
//! does not invert its forward path) or leaves unsupported is NEVER lifted — it becomes
//! honest residue, never a fact.
//!
//! Each surviving rule is one of:
//!
//! * a predicate/class **rename** (a lawful section) — [`LegPath::Step`];
//! * an **inverse** rename — [`LegPath::Inverse`];
//! * a lossy **reified claim** (`gm:StatementMetadata` cell) for a generalizing /
//!   close-match target.
//!
//! It DROPS the heuristic residue — value-transform rules, reverse minting,
//! context-descent, and concept-reference resolution — and records that residue
//! honestly in [`LiftedReport::residue`] rather than papering over it.
//!
//! Every rule body is expressed as a property path lowered through the canonical
//! F2 surface [`lower_leg_path`], so the executor genuinely exercises the
//! `logic:PathShape` → SPARQL property-path lowering rather than string-building the
//! predicate path itself. Any engine / query / lowering failure is a HARD error —
//! a rule is never silently skipped.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use gmeow_errors::{Diag, ResultExt};
use gmeow_logic_compile::ir::LegPath;
use gmeow_logic_compile::projections::paths::lower_leg_path;
use gmeow_logic_compile::projections::reified_claim::{
    ClaimAnnotation, ClaimObject, ReifiedClaim, reified_claim_template,
};
use purrdf::sparql::{
    Expression, Function, GraphPattern, Literal, NamedNode, NamedNodePattern, NativeSparqlEngine,
    PreparedQuery, QuadPattern, Query, QueryOptions, TermPattern, TriplePattern, Variable,
};
use purrdf::{RdfDataset, RdfQuad, RdfTerm, SparqlResult};

use crate::error::Put;
use crate::up_projection_corpus::{
    ADOPTED_PREDICATES, GM_CONFIDENCE, GM_MAPPED_FROM, GM_STATEMENT_METADATA, Graph,
    NORMALIZED_PREDICATES, RDF_TYPE, STATEMENT_METADATA_TERMS, XSD_DECIMAL, canon_qname,
    dump_dataset_nt, in_projection_ns, object_properties,
};
use crate::up_projection_gates::{
    LiftKind, LiftProgram, LiftRule, Orientation, gate_verified_lift_program,
};

/// The result of executing every lawful put leg over a source graph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiftedReport {
    /// The lifted GMEOW triples, serialized as N-Triples.
    pub graph_nt: String,
    /// Count of lawful FACT triples produced (rename + inverse + gmeow-passthrough).
    pub lifted: usize,
    /// Count of reified CLAIM cells produced (lossy lifts).
    pub claimed: usize,
    /// Projection-namespace source terms with NO lawful rule, mapped to their TRUE
    /// occurrence count in the source graph (canon qnames; never a fabricated constant).
    pub gap_terms: BTreeMap<String, usize>,
    /// Honest loss-ledger notes for the dropped heuristic categories (sorted, deduped).
    pub residue: Vec<String>,
}

/// A completed put operation whose RDF carrier stays native until an output is requested.
#[derive(Debug, Clone)]
pub struct LiftedDataset {
    /// Lifted facts and claim cells, including their RDF 1.2 statement metadata.
    pub dataset: Arc<RdfDataset>,
    /// Count of lawful fact triples.
    pub lifted: usize,
    /// Count of reified claim cells.
    pub claimed: usize,
    /// Unmapped source terms and their measured occurrence counts.
    pub gap_terms: BTreeMap<String, usize>,
    /// Sorted loss evidence from the selected lifting program.
    pub residue: Vec<String>,
}

impl LiftedDataset {
    /// Materialize the existing text output at the explicit serialization boundary.
    pub fn into_text(self) -> gmeow_errors::Result<LiftedReport> {
        Ok(LiftedReport {
            graph_nt: dump_dataset_nt(&self.dataset)?,
            lifted: self.lifted,
            claimed: self.claimed,
            gap_terms: self.gap_terms,
            residue: self.residue,
        })
    }
}

/// The gate-verified rule sets the executor lifts — a projection of the shared
/// [`LiftProgram`] into the query-builder's shape, plus the NON-gated structural constants
/// (`ADOPTED_PREDICATES`, `NORMALIZED_PREDICATES`, `STATEMENT_METADATA_TERMS`) that are
/// identity/normalization passthroughs, not external renames, and are never gate-filtered.
struct LawfulRules {
    /// external term -> gmeow term (predicate/class **direct** rename → FACT).
    rules: BTreeMap<String, String>,
    /// external term -> gmeow term (**inverse** rename; IRI/blank objects only → FACT).
    inverse_rules: BTreeMap<String, String>,
    /// external term -> (gmeow term, confidence lexeme) (lossy reified CLAIM).
    claim_rules: BTreeMap<String, (String, String)>,
    /// GMEOW IRIs typed `owl:ObjectProperty` — a literal object on one is a claim.
    object_properties: BTreeSet<String>,
    /// Count of non-unique (ambiguous) targets dropped rather than emitted (from the program).
    ambiguous_dropped: usize,
    /// Count of terms the correspondence gate RED-excluded (non-inverting reverse paths),
    /// surfaced as residue rather than lifted (from the program).
    gate_excluded: usize,
}

/// Project the shared gate-verified [`LiftProgram`] into the executor's query-builder rule sets,
/// then seed the NON-gated structural constants. Orientation and gate-filtering are ENTIRELY the
/// program's responsibility (single source of truth); this function only re-shapes the surviving
/// rules and adds the identity/normalization passthroughs.
fn lawful_rules_from_program(
    program: &LiftProgram,
    ontology_nt: &str,
) -> gmeow_errors::Result<LawfulRules> {
    let mut rules: BTreeMap<String, String> = BTreeMap::new();
    let mut inverse_rules: BTreeMap<String, String> = BTreeMap::new();
    let mut claim_rules: BTreeMap<String, (String, String)> = BTreeMap::new();

    for (ext, rule) in &program.rules {
        let LiftRule {
            gmeow,
            orientation,
            kind,
        } = rule;
        match kind {
            LiftKind::Claim { confidence } => {
                claim_rules.insert(ext.clone(), (gmeow.clone(), confidence.clone()));
            }
            LiftKind::Fact => match orientation {
                Orientation::Direct => {
                    rules.insert(ext.clone(), gmeow.clone());
                }
                Orientation::Inverse => {
                    inverse_rules.insert(ext.clone(), gmeow.clone());
                }
            },
        }
    }

    // NON-gated structural constants: identity adoption + statement-metadata passthrough +
    // label normalization. These are not external renames (they carry no EDOAL round-trip to
    // verify), so they are seeded unconditionally — the invariant explicitly exempts them.
    for adopted in ADOPTED_PREDICATES {
        rules
            .entry((*adopted).to_owned())
            .or_insert_with(|| (*adopted).to_owned());
    }
    for term in STATEMENT_METADATA_TERMS {
        rules
            .entry((*term).to_owned())
            .or_insert_with(|| (*term).to_owned());
    }
    for (source, target) in NORMALIZED_PREDICATES {
        rules
            .entry((*source).to_owned())
            .or_insert_with(|| (*target).to_owned());
    }

    Ok(LawfulRules {
        rules,
        inverse_rules,
        claim_rules,
        object_properties: object_properties(ontology_nt)?,
        ambiguous_dropped: program.ambiguous_dropped,
        gate_excluded: program.gate_excluded,
    })
}

/// The corpus-independent put-leg program: the gate-verified rules, prepared native operations
/// and value-rule residue count. Built ONCE from the SSSOM/projection/ontology inputs (which do not vary per
/// source file) via [`PutLegProgram::derive`], then applied to each source graph. Hoisting this
/// out of the per-file [`execute_put_legs`] loop makes the gate machinery (one
/// correspondence + five gates per candidate term) run once per corpus. Native query algebra
/// is admitted once; workers share immutable plans and own their execution state.
pub struct PutLegProgram {
    lawful: LawfulRules,
    value_rule_dropped: usize,
    facts: Vec<Arc<PreparedQuery>>,
    claims: Vec<Arc<PreparedQuery>>,
}

impl PutLegProgram {
    /// Derive the gate-verified put-leg program from the corpus-independent inputs. This is the
    /// single source of truth the executor lifts: [`gate_verified_lift_program`] decides which
    /// external terms survive the correspondence gates, their orientation, and their gmeow target.
    pub fn derive(
        sssom_texts: &[String],
        projection_ttls: &[String],
        ontology_nt: &str,
        discharged_section_cells: &BTreeSet<String>,
    ) -> gmeow_errors::Result<Self> {
        let program =
            gate_verified_lift_program(sssom_texts, projection_ttls, discharged_section_cells)?;
        let lawful = lawful_rules_from_program(&program, ontology_nt)?;
        let value_rule_dropped =
            crate::up_projection_corpus::value_mapped_pairs(projection_ttls)?.len();
        let engine = NativeSparqlEngine::new();
        let prepare = |query| {
            engine
                .prepare_algebra(query, QueryOptions::EMPTY)
                .with_ctx(|| "admit native put-leg CONSTRUCT".to_owned())
        };
        let facts = fact_queries(&lawful)?
            .into_iter()
            .map(prepare)
            .collect::<Result<_, _>>()?;
        let claims = claim_queries(&lawful)?
            .into_iter()
            .map(prepare)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            lawful,
            value_rule_dropped,
            facts,
            claims,
        })
    }
}

/// Execute every lawful put leg over `source_nt`, returning the lifted GMEOW graph and the honest
/// residue ledger. The gate-verified program is derived once here; when lifting many source files
/// with the SAME mappings, prefer [`PutLegProgram::derive`] once + [`execute_put_legs_with`] per
/// file so the gate machinery is not re-run per file. An empty source graph is a HARD error.
pub fn execute_put_legs(
    source_nt: &str,
    sssom_texts: &[String],
    projection_ttls: &[String],
    ontology_nt: &str,
    discharged_section_cells: &BTreeSet<String>,
) -> gmeow_errors::Result<LiftedReport> {
    let program = PutLegProgram::derive(
        sssom_texts,
        projection_ttls,
        ontology_nt,
        discharged_section_cells,
    )?;
    execute_put_legs_with(source_nt, &program)
}

/// Apply a pre-derived gate-verified [`PutLegProgram`] to one source graph. This is the per-file
/// hot path; the corpus-independent program is built once by the caller. An empty source graph is
/// a HARD error.
pub fn execute_put_legs_with(
    source_nt: &str,
    program: &PutLegProgram,
) -> gmeow_errors::Result<LiftedReport> {
    let source = Graph::parse(source_nt.as_bytes(), "application/n-triples")?;
    execute_put_legs_graph(&source, program)?.into_text()
}

/// Execute the default-graph put program against an existing native dataset.
/// Its named graphs remain in the source; this program selects only default-graph
/// assertions, matching the N-Triples adapter's explicitly selected input scope.
pub fn execute_put_legs_default_graph(
    source: &Arc<RdfDataset>,
    program: &PutLegProgram,
) -> gmeow_errors::Result<LiftedDataset> {
    execute_put_legs_graph(&Graph::from_dataset(Arc::clone(source)), program)
}

fn execute_put_legs_graph(
    source: &Graph,
    program: &PutLegProgram,
) -> gmeow_errors::Result<LiftedDataset> {
    if source.is_empty() {
        return Err(Diag::of_kind(Put {
            message: "execute_put_legs: source graph is empty".to_owned(),
        }));
    }

    let lawful = &program.lawful;
    let value_rule_dropped = program.value_rule_dropped;

    let engine = NativeSparqlEngine::new();
    // A single deduped, ordered fact+claim quad set. The native writer sorts the final
    // N-Triples output by (s,p,o,g), so the emitted graph is deterministic regardless
    // of in-loop dedup structure or `Vec` push order — a plain `HashSet<RdfQuad>` dedups
    // directly with no per-quad string-tuple allocation.
    let mut facts: HashSet<RdfQuad> = HashSet::new();
    let mut fact_quads: Vec<RdfQuad> = Vec::new();
    let mut claim_quads: Vec<RdfQuad> = Vec::new();
    let mut claim_cells: BTreeSet<String> = BTreeSet::new();

    // Rename rules (predicate/class + gmeow-passthrough) and inverse rules produce FACTS.
    for query in &program.facts {
        for quad in run_construct(&engine, &source.dataset, query)? {
            if facts.insert(quad.clone()) {
                fact_quads.push(quad);
            }
        }
    }

    // Claim rules produce reified CLAIM cells; count distinct `?cell a gm:StatementMetadata`.
    // Each prepared execution builds an independent dataset, so two claim queries can mint the
    // SAME template blank label (e.g. both `_:b0`); merging them would collapse two distinct
    // cells into one corrupt node. Claim CONSTRUCTs filter `isIRI(?s)` and never bind a blank
    // object, so EVERY blank in a claim result is a minted template blank — safe to rescope to
    // a per-query-unique namespace before dedup/merge. (Fact outputs are NOT rescoped: they can
    // carry SOURCE blanks whose identity must persist across queries; fact templates mint none.)
    let mut seen_claim: HashSet<RdfQuad> = HashSet::new();
    for (idx, query) in program.claims.iter().enumerate() {
        for quad in run_construct(&engine, &source.dataset, query)? {
            let quad = rescope_blanks(quad, idx);
            if !seen_claim.insert(quad.clone()) {
                continue;
            }
            if quad.predicate == RDF_TYPE
                && matches!(&quad.object, RdfTerm::Iri(n) if n == GM_STATEMENT_METADATA)
                && let RdfTerm::BlankNode(cell) = &quad.subject
            {
                claim_cells.insert(cell.clone());
            }
            claim_quads.push(quad);
        }
    }

    // Gap terms: projection-namespace source terms with no rule of any kind.
    let gap_terms = compute_gaps(&source.quads, lawful);

    let mut all_quads = fact_quads;
    all_quads.extend(claim_quads);
    let dataset = purrdf::flat_dataset_from_quads(&all_quads)
        .map_err(|message| Diag::of_kind(Put { message }))?;

    let residue = build_residue(
        value_rule_dropped,
        lawful.ambiguous_dropped,
        lawful.gate_excluded,
    );

    Ok(LiftedDataset {
        dataset,
        lifted: facts.len(),
        claimed: claim_cells.len(),
        gap_terms,
        residue,
    })
}

/// Build the FACT put-leg `CONSTRUCT` queries: predicate/class renames, inverse
/// renames, and gmeow-namespace passthrough — everything that lands as a plain fact.
fn fact_queries(lawful: &LawfulRules) -> gmeow_errors::Result<Vec<Query>> {
    let mut queries = Vec::new();
    for (ext, gmeow) in &lawful.rules {
        // CLASS rename: any subject typed <ext> is re-typed <gmeow>.
        queries.push(construct(
            vec![triple(var("s"), RDF_TYPE, iri_term(gmeow)?)?],
            GraphPattern::Bgp {
                patterns: vec![triple(var("s"), RDF_TYPE, iri_term(ext)?)?],
            },
        ));
        // PREDICATE rename, IRI/blank object -> fact. Route the single step through
        // the canonical F2 lowering so the executor genuinely uses lower_leg_path.
        let path = step_pattern(ext)?;
        queries.push(construct(
            vec![triple(var("s"), gmeow, var("o"))?],
            filtered(path.clone(), resource_object()),
        ));
        // PREDICATE rename, LITERAL object: a literal on an object-property becomes a
        // claim (see claim_queries); otherwise a plain fact.
        if !lawful.object_properties.contains(gmeow) {
            queries.push(construct(
                vec![triple(var("s"), gmeow, var("o"))?],
                filtered(path, call(Function::IsLiteral, "o")),
            ));
        }
    }
    for (ext, gmeow) in &lawful.inverse_rules {
        // INVERSE rename: source binds `?s <ext> ?o`; emit `?o <gmeow> ?s`. The
        // inversion is in the CONSTRUCT template, so the plain forward step suffices
        // in the WHERE; the canonical `^<ext>` lowering is asserted in the unit tests.
        queries.push(construct(
            vec![triple(var("o"), gmeow, var("s"))?],
            filtered(step_pattern(ext)?, resource_object()),
        ));
    }
    // gmeow-namespace passthrough: source triples whose predicate — or whose
    // rdf:type object — is already in the GMEOW namespace pass through unchanged.
    let passthrough = TriplePattern {
        subject: var("s"),
        predicate: NamedNodePattern::Variable(Variable::new("p")),
        object: var("o"),
    };
    queries.push(construct(
        vec![passthrough.clone()],
        filtered(
            GraphPattern::Bgp {
                patterns: vec![passthrough],
            },
            Expression::And(
                Box::new(gmeow_iri("p")),
                Box::new(Expression::Not(Box::new(Expression::Equal(
                    Box::new(Expression::Variable(Variable::new("p"))),
                    Box::new(Expression::NamedNode(iri(RDF_TYPE)?)),
                )))),
            ),
        ),
    ));
    let type_pattern = triple(var("s"), RDF_TYPE, var("o"))?;
    queries.push(construct(
        vec![type_pattern.clone()],
        filtered(
            GraphPattern::Bgp {
                patterns: vec![type_pattern],
            },
            gmeow_iri("o"),
        ),
    ));
    Ok(queries)
}

/// Build the reified-CLAIM put-leg `CONSTRUCT` queries: generalizing / close-match
/// targets, plus predicate renames whose literal object lands on an object-property.
fn claim_queries(lawful: &LawfulRules) -> gmeow_errors::Result<Vec<Query>> {
    let mut queries = Vec::new();
    // Dedicated claim rules (generalizing struct + sssom closeMatch).
    for (ext, (gmeow, conf)) in &lawful.claim_rules {
        // Predicate-position claim: `?s <ext> ?o` (IRI-object then literal-object).
        queries.push(claim_query(ext, gmeow, conf, ClaimSlot::PredicateIri)?);
        queries.push(claim_query(ext, gmeow, conf, ClaimSlot::PredicateLiteral)?);
        // Class-position claim: `?s rdf:type <ext>` where <ext> is a claim-rule class,
        // mirroring `lift_edge`'s rdf:type-object claim branch. qPredicate = rdf:type,
        // qObject = the gmeow class IRI (a fixed IRI, not `?o`).
        queries.push(claim_query(ext, gmeow, conf, ClaimSlot::TypeObject)?);
    }
    // Rename rules whose literal object lands on a GMEOW object-property: the literal
    // is disclosed as a claim (empty confidence), not asserted as a fact.
    for (ext, gmeow) in &lawful.rules {
        if lawful.object_properties.contains(gmeow) {
            // Only the literal-object variant (IRI/blank objects are facts above).
            queries.push(claim_query(ext, gmeow, "", ClaimSlot::PredicateLiteral)?);
        }
    }
    Ok(queries)
}

/// Which reified-claim shape a `claim_query` emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaimSlot {
    /// `?s <ext> ?o`, IRI object → `qObject ?o`.
    PredicateIri,
    /// `?s <ext> ?o`, literal object → `qObjectLiteral ?o`.
    PredicateLiteral,
    /// `?s rdf:type <ext>` → `qPredicate rdf:type ; qObject <gmeow>` (fixed IRI).
    TypeObject,
}

/// A single reified-claim `CONSTRUCT`. The `slot` selects the predicate-position
/// (IRI or literal object) or the class-position (rdf:type-object) claim shape.
///
/// The `gmeow:StatementMetadata` reified-claim template itself is rendered by the shared
/// [`reified_claim_template`] builder in `gmeow-logic-compile` — the SINGLE definition both this
/// native executor (the [`AssertionPolarity::ReifyClaim`] reference semantics) and the committed
/// `.put.rq` emitter render through, so the two surfaces cannot drift. This function only chooses
/// the object slot, assembles the annotation list, and wraps the head in the executor's
/// `isIRI`/`isLiteral`-filtered WHERE. The blank labels are the fixed `cell`/`mapann`/`confann`;
/// [`rescope_blanks`] re-scopes them per query so distinct cells never collide on merge.
fn claim_query(ext: &str, gmeow: &str, conf: &str, slot: ClaimSlot) -> gmeow_errors::Result<Query> {
    let (predicate, object, pattern, filter) = match slot {
        ClaimSlot::PredicateIri => (
            gmeow.to_owned(),
            ClaimObject::Iri(var("o")),
            triple(var("s"), ext, var("o"))?,
            Expression::And(
                Box::new(call(Function::IsIri, "s")),
                Box::new(call(Function::IsIri, "o")),
            ),
        ),
        ClaimSlot::PredicateLiteral => (
            gmeow.to_owned(),
            ClaimObject::Literal(var("o")),
            triple(var("s"), ext, var("o"))?,
            Expression::And(
                Box::new(call(Function::IsIri, "s")),
                Box::new(call(Function::IsLiteral, "o")),
            ),
        ),
        ClaimSlot::TypeObject => (
            RDF_TYPE.to_owned(),
            ClaimObject::Iri(iri_term(gmeow)?),
            triple(var("s"), RDF_TYPE, iri_term(ext)?)?,
            call(Function::IsIri, "s"),
        ),
    };
    let mut annotations = vec![ClaimAnnotation {
        label: "mapann".to_owned(),
        property: GM_MAPPED_FROM.to_owned(),
        value: iri_term(ext)?,
    }];
    if !conf.is_empty() {
        annotations.push(ClaimAnnotation {
            label: "confann".to_owned(),
            property: GM_CONFIDENCE.to_owned(),
            value: TermPattern::Literal(Literal::new_typed(conf, iri(XSD_DECIMAL)?)),
        });
    }
    let claim = ReifiedClaim {
        cell_label: "cell".to_owned(),
        subject: var("s"),
        predicate,
        object,
        annotations,
        // The native executor discloses import-provenance through the audit ledger, not an
        // in-graph wasGeneratedBy edge on every cell.
        generated_by: None,
    };
    Ok(construct(
        reified_claim_template(&claim)?,
        filtered(
            GraphPattern::Bgp {
                patterns: vec![pattern],
            },
            filter,
        ),
    ))
}

/// Admit and lower a forward predicate step through the canonical F2 algebra.
fn step_pattern(ext: &str) -> gmeow_errors::Result<GraphPattern> {
    iri(ext)?;
    Ok(GraphPattern::Path {
        subject: var("s"),
        path: lower_leg_path(&LegPath::Step(ext.to_owned())),
        object: var("o"),
    })
}

fn iri(value: &str) -> gmeow_errors::Result<NamedNode> {
    NamedNode::new(value).with_ctx(|| "put-leg IRI admission".to_owned())
}

fn iri_term(value: &str) -> gmeow_errors::Result<TermPattern> {
    iri(value).map(TermPattern::NamedNode)
}

fn var(name: &str) -> TermPattern {
    TermPattern::Variable(Variable::new(name))
}

fn triple(
    subject: TermPattern,
    predicate: &str,
    object: TermPattern,
) -> gmeow_errors::Result<TriplePattern> {
    Ok(TriplePattern {
        subject,
        predicate: NamedNodePattern::NamedNode(iri(predicate)?),
        object,
    })
}

fn construct(template: Vec<TriplePattern>, pattern: GraphPattern) -> Query {
    Query::Construct {
        template: template
            .into_iter()
            .map(|triple| QuadPattern {
                triple,
                graph: None,
            })
            .collect(),
        pattern,
        dataset: Default::default(),
        base_iri: None,
        version: None,
    }
}

fn filtered(inner: GraphPattern, expr: Expression) -> GraphPattern {
    GraphPattern::Filter {
        expr,
        inner: Box::new(inner),
    }
}

fn call(function: Function, variable: &str) -> Expression {
    Expression::FunctionCall(
        function,
        vec![Expression::Variable(Variable::new(variable))],
    )
}

fn resource_object() -> Expression {
    Expression::Or(
        Box::new(call(Function::IsIri, "o")),
        Box::new(call(Function::IsBlank, "o")),
    )
}

fn gmeow_iri(variable: &str) -> Expression {
    Expression::And(
        Box::new(call(Function::IsIri, variable)),
        Box::new(Expression::FunctionCall(
            Function::StrStarts,
            vec![
                call(Function::Str, variable),
                Expression::Literal(Literal::new_simple(crate::up_projection_corpus::GM)),
            ],
        )),
    )
}

/// Run one `CONSTRUCT` over the source dataset and return its default-graph quads.
/// Any engine failure or a non-graph result is a HARD error.
fn run_construct(
    engine: &NativeSparqlEngine,
    dataset: &Arc<RdfDataset>,
    query: &PreparedQuery,
) -> gmeow_errors::Result<Vec<RdfQuad>> {
    let result = engine
        .query_prepared(dataset, query, &[], QueryOptions::EMPTY)
        .with_ctx(|| {
            format!(
                "put-leg CONSTRUCT evaluation failed\nquery: {:?}",
                query.query
            )
        })?;
    let SparqlResult::Graph(ds) = result else {
        return Err(Diag::of_kind(Put {
            message: format!(
                "put-leg CONSTRUCT did not return a graph\nquery: {:?}",
                query.query
            ),
        }));
    };
    Ok(purrdf::native_quads::flat_rdf_quads(&ds)
        .filter(|q| q.graph_name.is_none())
        .collect())
}

/// Projection-namespace source terms (predicate positions + rdf:type objects) that
/// no lawful rule of any kind covers, mapped to their TRUE occurrence count — one
/// increment per matching source triple position, never deduped away. This is the
/// real per-term frequency downstream prioritization relies on; it must never be
/// flattened to a fabricated constant.
fn compute_gaps(quads: &[RdfQuad], lawful: &LawfulRules) -> BTreeMap<String, usize> {
    let has_rule = |term: &str| {
        lawful.rules.contains_key(term)
            || lawful.inverse_rules.contains_key(term)
            || lawful.claim_rules.contains_key(term)
    };
    let mut gaps: BTreeMap<String, usize> = BTreeMap::new();
    for triple in quads {
        if in_projection_ns(&triple.predicate) && !has_rule(&triple.predicate) {
            *gaps.entry(canon_qname(&triple.predicate)).or_insert(0) += 1;
        }
        if triple.predicate == RDF_TYPE
            && let RdfTerm::Iri(node) = &triple.object
            && in_projection_ns(node)
            && !has_rule(node)
        {
            *gaps.entry(canon_qname(node)).or_insert(0) += 1;
        }
    }
    gaps
}

/// The honest loss-ledger notes for the heuristic categories this lawful executor drops,
/// plus the correspondence-gate exclusions (non-inverting reverse paths the gate refuses).
fn build_residue(
    value_rule_dropped: usize,
    ambiguous_dropped: usize,
    gate_excluded: usize,
) -> Vec<String> {
    let mut notes: BTreeSet<String> = BTreeSet::new();
    notes.insert(format!(
        "value-transform rules dropped: {value_rule_dropped}"
    ));
    notes.insert(format!(
        "ambiguous (multi-candidate) terms dropped: {ambiguous_dropped}"
    ));
    notes.insert(format!(
        "gate-excluded (non-inverting reverse path) terms dropped: {gate_excluded}"
    ));
    notes.insert(
        "context-descent, reverse-minting, and concept-reference resolution are \
         heuristic residue (not lawful puts)"
            .to_owned(),
    );
    notes.into_iter().collect()
}

/// Rewrite every blank-node label in `quad` to the per-query-unique namespace
/// `_:q{idx}__{label}`, in both subject and object positions. Applied ONLY to claim
/// results, where every blank is a minted template blank (the `isIRI(?s)` filter and
/// the fixed-IRI object slots guarantee no source blank ever appears), so distinct
/// cells minted under identical labels by different queries never collide on merge.
fn rescope_blanks(quad: RdfQuad, idx: usize) -> RdfQuad {
    let rescope = |term: RdfTerm| match term {
        RdfTerm::BlankNode(label) => RdfTerm::BlankNode(format!("q{idx}__{label}")),
        other => other,
    };
    RdfQuad::new(rescope(quad.subject), quad.predicate, rescope(quad.object))
}

#[path = "put_executor.tests.rs"]
#[cfg(test)]
mod tests;
