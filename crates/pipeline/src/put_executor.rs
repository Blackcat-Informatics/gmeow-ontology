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

use gmeow_errors::{Diag, ResultExt};
use gmeow_logic_compile::ir::LegPath;
use gmeow_logic_compile::projections::paths::lower_leg_path;
use gmeow_logic_compile::projections::reified_claim::{
    ClaimAnnotation, ClaimObject, IriStyle, ReifiedClaim, reified_claim_head,
};
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfQuad, RdfTerm, SparqlEngine, SparqlRequest, SparqlResult};

use crate::error::Put;
use crate::up_projection_corpus::{
    ADOPTED_PREDICATES, GM_CONFIDENCE, GM_MAPPED_FROM, GM_STATEMENT_METADATA, Graph,
    NORMALIZED_PREDICATES, RDF_TYPE, STATEMENT_METADATA_TERMS, XSD_DECIMAL, canon_qname, dump_nt,
    in_projection_ns, object_properties,
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

/// The corpus-independent put-leg program: the gate-verified rule sets plus the value-rule
/// residue count. Built ONCE from the SSSOM/projection/ontology inputs (which do not vary per
/// source file) via [`PutLegProgram::derive`], then applied to each source graph. Hoisting this
/// out of the per-file [`execute_put_legs`] loop makes the gate machinery (one
/// correspondence + five gates per candidate term) runs once per corpus, not once per file.
pub struct PutLegProgram {
    lawful: LawfulRules,
    value_rule_dropped: usize,
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
        Ok(Self {
            lawful,
            value_rule_dropped,
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
    if source.is_empty() {
        return Err(Diag::of_kind(Put {
            message: "execute_put_legs: source graph is empty".to_owned(),
        }));
    }

    let lawful = &program.lawful;
    let value_rule_dropped = program.value_rule_dropped;

    let engine = NativeSparqlEngine::new();
    // A single deduped, ordered fact+claim quad set. `dump_nt` freeze-sorts the final
    // N-Triples output by (s,p,o,g), so the emitted graph is deterministic regardless
    // of in-loop dedup structure or `Vec` push order — a plain `HashSet<RdfQuad>` dedups
    // directly with no per-quad string-tuple allocation.
    let mut facts: HashSet<RdfQuad> = HashSet::new();
    let mut fact_quads: Vec<RdfQuad> = Vec::new();
    let mut claim_quads: Vec<RdfQuad> = Vec::new();
    let mut claim_cells: BTreeSet<String> = BTreeSet::new();

    // Rename rules (predicate/class + gmeow-passthrough) and inverse rules produce FACTS.
    for query in fact_queries(lawful) {
        for quad in run_construct(&engine, &source.dataset, &query)? {
            if facts.insert(quad.clone()) {
                fact_quads.push(quad);
            }
        }
    }

    // Claim rules produce reified CLAIM cells; count distinct `?cell a gm:StatementMetadata`.
    // Each `engine.query()` builds an independent dataset, so two claim queries can mint the
    // SAME template blank label (e.g. both `_:b0`); merging them would collapse two distinct
    // cells into one corrupt node. Claim CONSTRUCTs filter `isIRI(?s)` and never bind a blank
    // object, so EVERY blank in a claim result is a minted template blank — safe to rescope to
    // a per-query-unique namespace before dedup/merge. (Fact outputs are NOT rescoped: they can
    // carry SOURCE blanks whose identity must persist across queries; fact templates mint none.)
    let mut seen_claim: HashSet<RdfQuad> = HashSet::new();
    for (idx, query) in claim_queries(lawful).into_iter().enumerate() {
        for quad in run_construct(&engine, &source.dataset, &query)? {
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
    let graph_nt = dump_nt(&all_quads)?;

    let residue = build_residue(
        value_rule_dropped,
        lawful.ambiguous_dropped,
        lawful.gate_excluded,
    );

    Ok(LiftedReport {
        graph_nt,
        lifted: facts.len(),
        claimed: claim_cells.len(),
        gap_terms,
        residue,
    })
}

/// Build the FACT put-leg `CONSTRUCT` queries: predicate/class renames, inverse
/// renames, and gmeow-namespace passthrough — everything that lands as a plain fact.
fn fact_queries(lawful: &LawfulRules) -> Vec<String> {
    let mut queries = Vec::new();
    for (ext, gmeow) in &lawful.rules {
        // CLASS rename: any subject typed <ext> is re-typed <gmeow>.
        queries.push(format!(
            "CONSTRUCT {{ ?s <{RDF_TYPE}> <{gmeow}> }} \
             WHERE {{ ?s <{RDF_TYPE}> <{ext}> }}"
        ));
        // PREDICATE rename, IRI/blank object -> fact. Route the single step through
        // the canonical F2 lowering so the executor genuinely uses lower_leg_path.
        let path = step_path(ext);
        queries.push(format!(
            "CONSTRUCT {{ ?s <{gmeow}> ?o }} \
             WHERE {{ ?s {path} ?o . FILTER(isIRI(?o) || isBlank(?o)) }}"
        ));
        // PREDICATE rename, LITERAL object: a literal on an object-property becomes a
        // claim (see claim_queries); otherwise a plain fact.
        if !lawful.object_properties.contains(gmeow) {
            queries.push(format!(
                "CONSTRUCT {{ ?s <{gmeow}> ?o }} \
                 WHERE {{ ?s {path} ?o . FILTER(isLiteral(?o)) }}"
            ));
        }
    }
    for (ext, gmeow) in &lawful.inverse_rules {
        // INVERSE rename: source binds `?s <ext> ?o`; emit `?o <gmeow> ?s`. The
        // inversion is in the CONSTRUCT template, so the plain forward step suffices
        // in the WHERE; the canonical `^<ext>` lowering is asserted in the unit tests.
        queries.push(format!(
            "CONSTRUCT {{ ?o <{gmeow}> ?s }} \
             WHERE {{ ?s <{ext}> ?o . FILTER(isIRI(?o) || isBlank(?o)) }}"
        ));
    }
    // gmeow-namespace passthrough: source triples whose predicate — or whose
    // rdf:type object — is already in the GMEOW namespace pass through unchanged.
    queries.push(format!(
        "CONSTRUCT {{ ?s ?p ?o }} \
         WHERE {{ ?s ?p ?o . FILTER(isIRI(?p) && STRSTARTS(STR(?p), \"{GM}\") \
                 && ?p != <{RDF_TYPE}>) }}",
        GM = crate::up_projection_corpus::GM
    ));
    queries.push(format!(
        "CONSTRUCT {{ ?s <{RDF_TYPE}> ?o }} \
         WHERE {{ ?s <{RDF_TYPE}> ?o . \
                 FILTER(isIRI(?o) && STRSTARTS(STR(?o), \"{GM}\")) }}",
        GM = crate::up_projection_corpus::GM
    ));
    queries
}

/// Build the reified-CLAIM put-leg `CONSTRUCT` queries: generalizing / close-match
/// targets, plus predicate renames whose literal object lands on an object-property.
fn claim_queries(lawful: &LawfulRules) -> Vec<String> {
    let mut queries = Vec::new();
    // Dedicated claim rules (generalizing struct + sssom closeMatch).
    for (ext, (gmeow, conf)) in &lawful.claim_rules {
        // Predicate-position claim: `?s <ext> ?o` (IRI-object then literal-object).
        queries.push(claim_query(ext, gmeow, conf, ClaimSlot::PredicateIri));
        queries.push(claim_query(ext, gmeow, conf, ClaimSlot::PredicateLiteral));
        // Class-position claim: `?s rdf:type <ext>` where <ext> is a claim-rule class,
        // mirroring `lift_edge`'s rdf:type-object claim branch. qPredicate = rdf:type,
        // qObject = the gmeow class IRI (a fixed IRI, not `?o`).
        queries.push(claim_query(ext, gmeow, conf, ClaimSlot::TypeObject));
    }
    // Rename rules whose literal object lands on a GMEOW object-property: the literal
    // is disclosed as a claim (empty confidence), not asserted as a fact.
    for (ext, gmeow) in &lawful.rules {
        if lawful.object_properties.contains(gmeow) {
            // Only the literal-object variant (IRI/blank objects are facts above).
            queries.push(claim_query(ext, gmeow, "", ClaimSlot::PredicateLiteral));
        }
    }
    queries
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
/// [`reified_claim_head`] builder in `gmeow-logic-compile` — the SINGLE definition both this
/// native executor (the [`AssertionPolarity::ReifyClaim`] reference semantics) and the committed
/// `.put.rq` emitter render through, so the two surfaces cannot drift. This function only chooses
/// the object slot, assembles the annotation list, and wraps the head in the executor's
/// `isIRI`/`isLiteral`-filtered WHERE. The blank labels are the fixed `cell`/`mapann`/`confann`;
/// [`rescope_blanks`] re-scopes them per query so distinct cells never collide on merge.
fn claim_query(ext: &str, gmeow: &str, conf: &str, slot: ClaimSlot) -> String {
    let (predicate, object, where_clause) = match slot {
        ClaimSlot::PredicateIri => (
            gmeow.to_owned(),
            ClaimObject::Iri("?o".to_owned()),
            format!("?s <{ext}> ?o . FILTER(isIRI(?s) && isIRI(?o))"),
        ),
        ClaimSlot::PredicateLiteral => (
            gmeow.to_owned(),
            ClaimObject::Literal("?o".to_owned()),
            format!("?s <{ext}> ?o . FILTER(isIRI(?s) && isLiteral(?o))"),
        ),
        ClaimSlot::TypeObject => (
            RDF_TYPE.to_owned(),
            ClaimObject::Iri(format!("<{gmeow}>")),
            format!("?s <{RDF_TYPE}> <{ext}> . FILTER(isIRI(?s))"),
        ),
    };
    let mut annotations = vec![ClaimAnnotation {
        label: "mapann".to_owned(),
        property: GM_MAPPED_FROM.to_owned(),
        value: format!("<{ext}>"),
    }];
    if !conf.is_empty() {
        annotations.push(ClaimAnnotation {
            label: "confann".to_owned(),
            property: GM_CONFIDENCE.to_owned(),
            value: format!("\"{conf}\"^^<{XSD_DECIMAL}>"),
        });
    }
    let claim = ReifiedClaim {
        cell_label: "cell".to_owned(),
        subject: "?s".to_owned(),
        predicate,
        object,
        annotations,
        // The native executor discloses import-provenance through the audit ledger, not an
        // in-graph wasGeneratedBy edge on every cell.
        generated_by: None,
    };
    let head = reified_claim_head(&claim, IriStyle::Full).join(" ");
    format!("CONSTRUCT {{ {head} }} WHERE {{ {where_clause} }}")
}

/// Lower a single forward predicate step through the canonical F2 property-path
/// surface, yielding the SPARQL path text (`<ext>`) used in a put-leg WHERE clause.
fn step_path(ext: &str) -> String {
    lower_leg_path(&LegPath::Step(ext.to_owned())).to_string()
}

/// Run one `CONSTRUCT` over the source dataset and return its default-graph quads.
/// Any engine failure or a non-graph result is a HARD error.
pub(crate) fn run_construct(
    engine: &NativeSparqlEngine,
    dataset: &std::sync::Arc<purrdf::RdfDataset>,
    query: &str,
) -> gmeow_errors::Result<Vec<RdfQuad>> {
    let result = engine
        .query(
            dataset,
            SparqlRequest {
                query,
                base_iri: None,
                substitutions: &[],
            },
        )
        .with_ctx(|| format!("put-leg CONSTRUCT evaluation failed\nquery: {query}"))?;
    let SparqlResult::Graph(ds) = result else {
        return Err(Diag::of_kind(Put {
            message: format!("put-leg CONSTRUCT did not return a graph\nquery: {query}"),
        }));
    };
    Ok(purrdf::native_quads::flat_rdf_quads_from_dataset(&ds)
        .into_iter()
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

#[cfg(test)]
mod tests {
    use super::*;
    // Reification consumer-side vocab used only by the assertions below (the executor's
    // production path renders these through the shared reified-claim builder).
    use crate::up_projection_corpus::{GM_Q_OBJECT, GM_Q_PREDICATE};

    fn run_one(
        engine: &NativeSparqlEngine,
        dataset: &std::sync::Arc<purrdf::RdfDataset>,
        q: &str,
    ) -> Vec<RdfQuad> {
        run_construct(engine, dataset, q).expect("construct ok")
    }

    #[test]
    fn canonical_leg_path_lowering_is_exercised() {
        // Single forward + inverse steps: the surface the put legs route through.
        assert_eq!(
            lower_leg_path(&LegPath::Step("http://ex/p".into())).to_string(),
            "<http://ex/p>"
        );
        assert_eq!(
            lower_leg_path(&LegPath::Inverse(Box::new(LegPath::Step(
                "http://ex/p".into()
            ))))
            .to_string(),
            "^<http://ex/p>"
        );
        // Seq / Alt confirm the paths lib handles composite bodies.
        let seq = LegPath::Seq(vec![
            LegPath::Step("http://ex/a".into()),
            LegPath::Step("http://ex/b".into()),
        ]);
        assert_eq!(
            lower_leg_path(&seq).to_string(),
            "<http://ex/a>/<http://ex/b>"
        );
        let alt = LegPath::Alt(vec![
            LegPath::Step("http://ex/a".into()),
            LegPath::Step("http://ex/b".into()),
        ]);
        assert_eq!(
            lower_leg_path(&alt).to_string(),
            "<http://ex/a>|<http://ex/b>"
        );
    }

    #[test]
    fn fact_construct_renames_a_predicate() {
        // Prove a rename CONSTRUCT executes and renames, driving the WHERE property
        // path through the canonical F2 lowering (step_path).
        let source_nt = "<http://a/1> <http://ex/knows> <http://a/2> .\n";
        let source = Graph::parse(source_nt.as_bytes(), "application/n-triples").expect("parse");
        let path = step_path("http://ex/knows");
        assert_eq!(path, "<http://ex/knows>");
        let query = format!(
            "CONSTRUCT {{ ?s <https://blackcatinformatics.ca/gmeow/knows> ?o }} \
             WHERE {{ ?s {path} ?o . FILTER(isIRI(?o) || isBlank(?o)) }}"
        );
        let engine = NativeSparqlEngine::new();
        let out = run_one(&engine, &source.dataset, &query);
        assert_eq!(out.len(), 1, "one renamed triple");
        assert_eq!(
            out[0].predicate,
            "https://blackcatinformatics.ca/gmeow/knows"
        );
        assert!(matches!(&out[0].subject, RdfTerm::Iri(n) if n == "http://a/1"));
        assert!(matches!(&out[0].object, RdfTerm::Iri(n) if n == "http://a/2"));
    }

    #[test]
    fn end_to_end_rename_lifts_via_sssom() {
        // A clean (exactMatch) SSSOM row aligning gmeow:knows and ex:knows, with a
        // custom curie map so `ex:` resolves. sssom_clean_pairs keys on the projection
        // namespace; foaf is a projection prefix, so align via foaf:knows.
        let sssom = concat!(
            "#curie_map:\n",
            "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
            "#  foaf: http://xmlns.com/foaf/0.1/\n",
            "#  skos: http://www.w3.org/2004/02/skos/core#\n",
            "subject_id\tpredicate_id\tobject_id\n",
            "gmeow:knows\tskos:exactMatch\tfoaf:knows\n",
        );
        let source_nt = "<http://a/1> <http://xmlns.com/foaf/0.1/knows> <http://a/2> .\n";
        let report = execute_put_legs(source_nt, &[sssom.to_owned()], &[], "", &BTreeSet::new())
            .expect("execute put legs");
        assert!(
            report
                .graph_nt
                .contains("<https://blackcatinformatics.ca/gmeow/knows>"),
            "renamed predicate present: {}",
            report.graph_nt
        );
        assert!(report.lifted >= 1, "at least one fact lifted");
    }

    #[test]
    fn close_match_yields_a_reified_claim() {
        // A closeMatch row (with confidence) becomes a lossy claim, not a fact.
        let sssom = concat!(
            "#curie_map:\n",
            "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
            "#  foaf: http://xmlns.com/foaf/0.1/\n",
            "#  skos: http://www.w3.org/2004/02/skos/core#\n",
            "subject_id\tpredicate_id\tobject_id\tconfidence\n",
            "gmeow:acquaintanceOf\tskos:closeMatch\tfoaf:knows\t0.8\n",
        );
        let source_nt = "<http://a/1> <http://xmlns.com/foaf/0.1/knows> <http://a/2> .\n";
        let report = execute_put_legs(source_nt, &[sssom.to_owned()], &[], "", &BTreeSet::new())
            .expect("execute put legs");
        assert!(report.claimed >= 1, "at least one claim cell: {report:?}");
        assert!(
            report
                .graph_nt
                .contains("<https://blackcatinformatics.ca/gmeow/StatementMetadata>"),
            "statement-metadata cell present: {}",
            report.graph_nt
        );
        assert!(
            report.graph_nt.contains(
                "<https://blackcatinformatics.ca/gmeow/qPredicate> \
                     <https://blackcatinformatics.ca/gmeow/acquaintanceOf>"
            ) || report
                .graph_nt
                .contains("<https://blackcatinformatics.ca/gmeow/acquaintanceOf>"),
            "qPredicate points at the gmeow term: {}",
            report.graph_nt
        );
        assert!(
            report
                .graph_nt
                .contains("<https://blackcatinformatics.ca/gmeow/mappedFrom>"),
            "mappedFrom annotation present: {}",
            report.graph_nt
        );
    }

    #[test]
    fn two_distinct_claim_rules_stay_on_separate_cells() {
        // Two distinct closeMatch claim rules, each matched by one source triple. Before
        // the per-query blank rescoping, the two claim CONSTRUCTs minted the same `_:cell`
        // label in independent datasets and merged into ONE corrupt node carrying both
        // qPredicates. After the fix they stay on two separate cells.
        let sssom = concat!(
            "#curie_map:\n",
            "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
            "#  foaf: http://xmlns.com/foaf/0.1/\n",
            "#  skos: http://www.w3.org/2004/02/skos/core#\n",
            "subject_id\tpredicate_id\tobject_id\tconfidence\n",
            "gmeow:a\tskos:closeMatch\tfoaf:knows\t0.8\n",
            "gmeow:b\tskos:closeMatch\tfoaf:member\t0.7\n",
        );
        let source_nt = concat!(
            "<http://a/1> <http://xmlns.com/foaf/0.1/knows> <http://a/2> .\n",
            "<http://a/1> <http://xmlns.com/foaf/0.1/member> <http://a/3> .\n",
        );
        let report = execute_put_legs(source_nt, &[sssom.to_owned()], &[], "", &BTreeSet::new())
            .expect("execute put legs");
        assert_eq!(report.claimed, 2, "two distinct claim cells: {report:?}");

        // Parse the lifted graph and map each cell blank -> the set of qPredicate IRIs on it.
        let graph = Graph::parse(report.graph_nt.as_bytes(), "application/n-triples")
            .expect("parse lifted graph");
        let mut cell_preds: std::collections::BTreeMap<String, BTreeSet<String>> =
            std::collections::BTreeMap::new();
        for q in &graph.quads {
            if q.predicate == GM_Q_PREDICATE
                && let (RdfTerm::BlankNode(cell), RdfTerm::Iri(p)) = (&q.subject, &q.object)
            {
                cell_preds
                    .entry(cell.clone())
                    .or_default()
                    .insert(p.clone());
            }
        }
        let a = "https://blackcatinformatics.ca/gmeow/a";
        let b = "https://blackcatinformatics.ca/gmeow/b";
        // No single cell carries BOTH qPredicates.
        for (cell, preds) in &cell_preds {
            assert!(
                !(preds.contains(a) && preds.contains(b)),
                "cell {cell} corruptly carries both qPredicates: {preds:?}"
            );
        }
        // Each qPredicate lands on exactly one distinct cell, and they differ.
        let cell_of = |pred: &str| -> Option<String> {
            cell_preds
                .iter()
                .find(|(_, preds)| preds.contains(pred))
                .map(|(cell, _)| cell.clone())
        };
        let cell_a = cell_of(a).expect("cell for gmeow:a");
        let cell_b = cell_of(b).expect("cell for gmeow:b");
        assert_ne!(cell_a, cell_b, "the two claims must be on separate cells");
    }

    #[test]
    fn class_level_close_match_yields_a_type_object_claim() {
        // A source `<x> rdf:type <ext_class>` where ext_class is a closeMatch claim rule.
        // lift_edge treats this as a claim: qPredicate = rdf:type, qObject = the gmeow class.
        let sssom = concat!(
            "#curie_map:\n",
            "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
            "#  foaf: http://xmlns.com/foaf/0.1/\n",
            "#  skos: http://www.w3.org/2004/02/skos/core#\n",
            "subject_id\tpredicate_id\tobject_id\tconfidence\n",
            "gmeow:Acquaintance\tskos:closeMatch\tfoaf:Agent\t0.75\n",
        );
        let source_nt = concat!(
            "<http://a/1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ",
            "<http://xmlns.com/foaf/0.1/Agent> .\n",
        );
        let report = execute_put_legs(source_nt, &[sssom.to_owned()], &[], "", &BTreeSet::new())
            .expect("execute put legs");
        assert!(
            report.claimed >= 1,
            "at least one type-object claim: {report:?}"
        );

        let graph = Graph::parse(report.graph_nt.as_bytes(), "application/n-triples")
            .expect("parse lifted graph");
        // A cell whose qPredicate is rdf:type AND qObject is the gmeow class.
        let type_object_cell = graph.quads.iter().any(|q| {
            q.predicate == GM_Q_PREDICATE
                && matches!(&q.object, RdfTerm::Iri(p) if p == RDF_TYPE)
                && matches!(&q.subject, RdfTerm::BlankNode(cell) if graph.quads.iter().any(|r| {
                    matches!(&r.subject, RdfTerm::BlankNode(c) if c == cell)
                        && r.predicate == GM_Q_OBJECT
                        && matches!(&r.object, RdfTerm::Iri(o)
                            if o == "https://blackcatinformatics.ca/gmeow/Acquaintance")
                }))
        });
        assert!(
            type_object_cell,
            "a claim cell with qPredicate rdf:type and qObject gmeow:Acquaintance: {}",
            report.graph_nt
        );
    }

    #[test]
    fn empty_source_is_an_error() {
        let err = execute_put_legs("", &[], &[], "", &BTreeSet::new())
            .expect_err("empty source rejected");
        assert!(err.to_string().contains("source graph is empty"), "{err}");
    }

    /// One `gmeow:ProjectionMapping` EDOAL-path cell: a single-atom, no-mint pattern whose atom
    /// carries `predicate <apred>` and (`subjectVar` == anchor ⇒ direct / `objectVar` == anchor
    /// ⇒ inverse), with a binding `toPredicate <target>`. Two such cells for the same target —
    /// a direct one and an inverse one — give `edoalpath_pairs` a `direct`/`inverse` pair, and
    /// the single-atom no-mint binding also registers the target as a `simple-1to1` structural
    /// class (bucket `clean`), so the term reaches `VerifiableRoundTrip` in the shared classifier.
    fn edoal_cell(cell: &str, target: &str, apred: &str, inverse: bool) -> String {
        let (subj, obj) = if inverse { ("o", "s") } else { ("s", "o") };
        format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix foaf: <http://xmlns.com/foaf/0.1/> .\n\
             gmeow:{cell} a gmeow:ProjectionMapping ;\n\
               gmeow:hasMappingPattern [\n\
                 gmeow:anchor \"s\" ; gmeow:value \"o\" ;\n\
                 gmeow:atom ( [ gmeow:subjectVar \"{subj}\" ; gmeow:predicate <{apred}> ; \
                                gmeow:objectVar \"{obj}\" ] ) ;\n\
                 gmeow:edoalPath true ] ;\n\
               gmeow:hasBinding [ gmeow:profile \"foaf\" ; gmeow:toPredicate <{target}> ; \
                                  gmeow:relation \"=\" ] .\n"
        )
    }

    #[test]
    fn gate_red_excluded_term_is_never_lifted_by_the_executor() {
        // The parity guard. `foaf:bad` has a direct EDOAL path on
        // <gmeow:forward> and an inverse EDOAL path on a DIFFERENT predicate <gmeow:notInverse>:
        // the reverse path does NOT invert the forward path, so the correspondence round-trip
        // gate RED-excludes it (audit tier = red_excluded). `foaf:good` has a matching
        // direct+inverse pair on <gmeow:good>, so it is proved-lawful and MUST be lifted.
        //
        // The executor's lifted external-term set must therefore be exactly the audit's
        // non-red-excluded (proved+claimed) set: `foaf:good` lifts, `foaf:bad` never does. If a
        // future change reintroduces an ungated derivation, `foaf:bad` would leak back in and
        // this test FAILS.
        let good = "http://xmlns.com/foaf/0.1/good";
        let bad = "http://xmlns.com/foaf/0.1/bad";
        let projection_ttls = vec![
            edoal_cell(
                "mapGoodFwd",
                good,
                "https://blackcatinformatics.ca/gmeow/good",
                false,
            ),
            edoal_cell(
                "mapGoodInv",
                good,
                "https://blackcatinformatics.ca/gmeow/good",
                true,
            ),
            edoal_cell(
                "mapBadFwd",
                bad,
                "https://blackcatinformatics.ca/gmeow/forward",
                false,
            ),
            edoal_cell(
                "mapBadInv",
                bad,
                "https://blackcatinformatics.ca/gmeow/notInverse",
                true,
            ),
        ];

        // Cross-check the shared producer directly: bad is gate-excluded, good survives.
        let program = gate_verified_lift_program(&[], &projection_ttls, &BTreeSet::new())
            .expect("lift program builds");
        assert!(
            program.rules.contains_key(good),
            "the proved (matching-inverse) term must survive the gate: {program:?}"
        );
        assert!(
            !program.rules.contains_key(bad),
            "the red-excluded (non-inverting) term must NOT survive the gate: {program:?}"
        );
        assert!(
            program.gate_excluded >= 1,
            "the non-inverting term is surfaced as gate-excluded residue: {program:?}"
        );

        // And end-to-end through the executor: a source using BOTH terms lifts good, never bad.
        let source_nt = concat!(
            "<http://a/1> <http://xmlns.com/foaf/0.1/good> <http://a/2> .\n",
            "<http://a/1> <http://xmlns.com/foaf/0.1/bad> <http://a/3> .\n",
            "<http://a/2> <http://xmlns.com/foaf/0.1/bad> <http://a/4> .\n",
        );
        let report = execute_put_legs(source_nt, &[], &projection_ttls, "", &BTreeSet::new())
            .expect("execute put legs");
        assert!(
            report
                .graph_nt
                .contains("<https://blackcatinformatics.ca/gmeow/good>"),
            "the proved term is lifted: {}",
            report.graph_nt
        );
        // No lifted triple may mention EITHER the gate-excluded external term or its gmeow
        // targets — the executor must not have run a rule for `foaf:bad` at all.
        for leaked in [
            bad,
            "https://blackcatinformatics.ca/gmeow/forward",
            "https://blackcatinformatics.ca/gmeow/notInverse",
        ] {
            assert!(
                !report.graph_nt.contains(leaked),
                "the gate-excluded term leaked a lifted triple mentioning <{leaked}>: {}",
                report.graph_nt
            );
        }
        // `foaf:bad` has no gate-surviving rule, so it is an honest gap term, not a silent drop.
        // It occurs TWICE in the source; the reported count must be the real occurrence count
        // (2), never a fabricated constant (1).
        assert_eq!(
            report.gap_terms.get("foaf:bad").copied(),
            Some(2),
            "the gate-excluded term's gap count must be the TRUE occurrence count, not a \
             fabricated constant: {:?}",
            report.gap_terms
        );
    }
}
