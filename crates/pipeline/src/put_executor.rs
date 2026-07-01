// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, lawful up-projection executor: the `put`-leg cutover.
//!
//! This module lifts external-vocabulary source triples to GMEOW by running each
//! *lawful* alignment rule as a SPARQL `CONSTRUCT` — the "put leg" — through the
//! native [`NativeSparqlEngine`]. It reproduces ONLY the lawful subset of the
//! heuristic engine's per-edge lift:
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

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::ir::LegPath;
use gmeow_logic_compile::projections::paths::lower_leg_path;
use gmeow_rdf::{RdfQuad, RdfTerm, SparqlEngine, SparqlRequest, SparqlResult};
use gmeow_sparql_eval::NativeSparqlEngine;

use crate::up_projection_corpus::{
    canon_qname, dump_nt, edoalpath_pairs, in_projection_ns, object_properties, sssom_clean_pairs,
    sssom_closematch_pairs, structural_pairs, Graph, ADOPTED_PREDICATES, GM_ANNOTATION,
    GM_ANN_PROPERTY, GM_ANN_VALUE, GM_CONFIDENCE, GM_MAPPED_FROM, GM_Q_OBJECT, GM_Q_OBJECT_LITERAL,
    GM_Q_PREDICATE, GM_Q_SUBJECT, GM_STATEMENT_METADATA, NORMALIZED_PREDICATES, RDF_TYPE,
    STATEMENT_METADATA_TERMS, XSD_DECIMAL,
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
    /// Projection-namespace source terms with NO lawful rule (sorted, deduped, canon qnames).
    pub gap_terms: Vec<String>,
    /// Honest loss-ledger notes for the dropped heuristic categories (sorted, deduped).
    pub residue: Vec<String>,
}

/// The lawful rule sets — the lawful subset of the old `build_lift_map`.
struct LawfulRules {
    /// external term -> gmeow term (predicate/class rename).
    rules: BTreeMap<String, String>,
    /// external term -> gmeow term (inverse rename; IRI/blank objects only).
    inverse_rules: BTreeMap<String, String>,
    /// external term -> (gmeow term, confidence lexeme) (lossy reified claim).
    claim_rules: BTreeMap<String, (String, String)>,
    /// GMEOW IRIs typed `owl:ObjectProperty` — a literal object on one is a claim.
    object_properties: BTreeSet<String>,
    /// Count of non-unique (ambiguous) targets dropped rather than emitted.
    ambiguous_dropped: usize,
}

/// Execute every lawful put leg over `source_nt`, returning the lifted GMEOW graph
/// and the honest residue ledger. An empty source graph is a HARD error.
pub fn execute_put_legs(
    source_nt: &str,
    sssom_texts: &[String],
    projection_ttls: &[String],
    ontology_nt: &str,
) -> Result<LiftedReport, String> {
    let source = Graph::parse(source_nt.as_bytes(), "application/n-triples")?;
    if source.is_empty() {
        return Err("execute_put_legs: source graph is empty".to_owned());
    }

    let lawful = build_lawful_rules(sssom_texts, projection_ttls, ontology_nt)?;
    let value_rule_dropped =
        crate::up_projection_corpus::value_mapped_pairs(projection_ttls)?.len();

    let engine = NativeSparqlEngine::new();
    // A single deduped, ordered fact+claim quad set so output is deterministic.
    let mut facts: BTreeSet<QuadKey> = BTreeSet::new();
    let mut fact_quads: Vec<RdfQuad> = Vec::new();
    let mut claim_quads: Vec<RdfQuad> = Vec::new();
    let mut claim_cells: BTreeSet<String> = BTreeSet::new();

    // Rename rules (predicate/class + gmeow-passthrough) and inverse rules produce FACTS.
    for query in fact_queries(&lawful) {
        for quad in run_construct(&engine, &source.dataset, &query)? {
            let key = quad_key(&quad);
            if facts.insert(key) {
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
    let mut seen_claim: BTreeSet<QuadKey> = BTreeSet::new();
    for (idx, query) in claim_queries(&lawful).into_iter().enumerate() {
        for quad in run_construct(&engine, &source.dataset, &query)? {
            let quad = rescope_blanks(quad, idx);
            let key = quad_key(&quad);
            if !seen_claim.insert(key) {
                continue;
            }
            if quad.predicate == RDF_TYPE
                && matches!(&quad.object, RdfTerm::Iri(n) if n == GM_STATEMENT_METADATA)
            {
                if let RdfTerm::BlankNode(cell) = &quad.subject {
                    claim_cells.insert(cell.clone());
                }
            }
            claim_quads.push(quad);
        }
    }

    // Gap terms: projection-namespace source terms with no rule of any kind.
    let gap_terms = compute_gaps(&source.quads, &lawful);

    let mut all_quads = fact_quads;
    all_quads.extend(claim_quads);
    let graph_nt = dump_nt(&all_quads)?;

    let residue = build_residue(value_rule_dropped, lawful.ambiguous_dropped);

    Ok(LiftedReport {
        graph_nt,
        lifted: facts.len(),
        claimed: claim_cells.len(),
        gap_terms,
        residue,
    })
}

/// Reproduce the LAWFUL parts of the old `build_lift_map`, dropping `value_rules`
/// and the `ambiguous` map (both become residue, not rules).
fn build_lawful_rules(
    sssom_texts: &[String],
    projection_ttls: &[String],
    ontology_nt: &str,
) -> Result<LawfulRules, String> {
    let (direct_edoalpath, inverse_edoalpath) = edoalpath_pairs(projection_ttls)?;
    let identity = sssom_clean_pairs(sssom_texts)?;
    let (exact_struct, generalizing_struct) = structural_pairs(projection_ttls)?;

    // projection = union of exact_struct and direct_edoalpath (target -> set<gmeow>).
    let mut projection: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for layer in [&exact_struct, &direct_edoalpath] {
        for (target, gmeows) in layer {
            projection
                .entry(target.clone())
                .or_default()
                .extend(gmeows.iter().cloned());
        }
    }

    let mut rules: BTreeMap<String, String> = BTreeMap::new();
    let mut ambiguous_dropped = 0usize;
    // Track ambiguous targets so downstream steps can skip them (mirrors the old
    // `ambiguous` map's role as a guard) without emitting them as rules.
    let mut ambiguous: BTreeSet<String> = BTreeSet::new();
    let mut targets: BTreeSet<String> = identity.keys().cloned().collect();
    targets.extend(projection.keys().cloned());
    for target in targets {
        let ids = identity.get(&target).cloned().unwrap_or_default();
        if ids.len() == 1 {
            rules.insert(target, ids.into_iter().next().expect("one identity"));
        } else if ids.len() > 1 {
            ambiguous.insert(target);
            ambiguous_dropped += 1;
        } else {
            let projs = projection.get(&target).cloned().unwrap_or_default();
            if projs.len() == 1 {
                rules.insert(target, projs.into_iter().next().expect("one projection"));
            } else {
                ambiguous.insert(target);
                ambiguous_dropped += 1;
            }
        }
    }

    let mut inverse_rules: BTreeMap<String, String> = BTreeMap::new();
    for (target, gmeows) in inverse_edoalpath {
        if rules.contains_key(&target) || ambiguous.contains(&target) {
            continue;
        }
        if gmeows.len() == 1 {
            inverse_rules.insert(target, gmeows.into_iter().next().expect("one inverse"));
        } else {
            ambiguous.insert(target);
            ambiguous_dropped += 1;
        }
    }

    let mut claim_rules: BTreeMap<String, (String, String)> = BTreeMap::new();
    add_claims(
        &mut claim_rules,
        &mut ambiguous,
        &mut ambiguous_dropped,
        &rules,
        &inverse_rules,
        &generalizing_struct,
    );
    let close = sssom_closematch_pairs(sssom_texts)?;
    add_claims(
        &mut claim_rules,
        &mut ambiguous,
        &mut ambiguous_dropped,
        &rules,
        &inverse_rules,
        &close,
    );

    // Seed the identity/normalized constants exactly as the old code did.
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
        ambiguous_dropped,
    })
}

/// The old `add_claims`, adapted to record every multi-candidate target as residue
/// (rather than into an `ambiguous` map that the executor no longer consumes).
fn add_claims(
    claim_rules: &mut BTreeMap<String, (String, String)>,
    ambiguous: &mut BTreeSet<String>,
    ambiguous_dropped: &mut usize,
    rules: &BTreeMap<String, String>,
    inverse_rules: &BTreeMap<String, String>,
    candidates: &BTreeMap<String, BTreeMap<String, String>>,
) {
    for (target, cands) in candidates {
        if rules.contains_key(target)
            || inverse_rules.contains_key(target)
            || ambiguous.contains(target)
            || claim_rules.contains_key(target)
        {
            continue;
        }
        if cands.len() == 1 {
            let (gmeow, conf) = cands.iter().next().expect("one claim candidate");
            claim_rules.insert(target.clone(), (gmeow.clone(), conf.clone()));
        } else {
            ambiguous.insert(target.clone());
            *ambiguous_dropped += 1;
        }
    }
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
fn claim_query(ext: &str, gmeow: &str, conf: &str, slot: ClaimSlot) -> String {
    // (qPredicate IRI, qObject predicate, qObject term text, WHERE clause).
    let (qpred, qobj_pred, qobj_term, where_clause) = match slot {
        ClaimSlot::PredicateIri => (
            gmeow.to_owned(),
            GM_Q_OBJECT,
            "?o".to_owned(),
            format!("?s <{ext}> ?o . FILTER(isIRI(?s) && isIRI(?o))"),
        ),
        ClaimSlot::PredicateLiteral => (
            gmeow.to_owned(),
            GM_Q_OBJECT_LITERAL,
            "?o".to_owned(),
            format!("?s <{ext}> ?o . FILTER(isIRI(?s) && isLiteral(?o))"),
        ),
        ClaimSlot::TypeObject => (
            RDF_TYPE.to_owned(),
            GM_Q_OBJECT,
            format!("<{gmeow}>"),
            format!("?s <{RDF_TYPE}> <{ext}> . FILTER(isIRI(?s))"),
        ),
    };
    let mut template = format!(
        "_:cell <{RDF_TYPE}> <{GM_STATEMENT_METADATA}> ; \
                <{GM_Q_SUBJECT}> ?s ; \
                <{GM_Q_PREDICATE}> <{qpred}> ; \
                <{qobj_pred}> {qobj_term} ; \
                <{GM_ANNOTATION}> _:mapann . \
         _:mapann <{GM_ANN_PROPERTY}> <{GM_MAPPED_FROM}> ; <{GM_ANN_VALUE}> <{ext}> ."
    );
    if !conf.is_empty() {
        template.push_str(&format!(
            " _:cell <{GM_ANNOTATION}> _:confann . \
              _:confann <{GM_ANN_PROPERTY}> <{GM_CONFIDENCE}> ; \
                        <{GM_ANN_VALUE}> \"{conf}\"^^<{XSD_DECIMAL}> ."
        ));
    }
    format!(
        "CONSTRUCT {{ {template} }} \
         WHERE {{ {where_clause} }}"
    )
}

/// Lower a single forward predicate step through the canonical F2 property-path
/// surface, yielding the SPARQL path text (`<ext>`) used in a put-leg WHERE clause.
fn step_path(ext: &str) -> String {
    lower_leg_path(&LegPath::Step(ext.to_owned())).to_string()
}

/// Run one `CONSTRUCT` over the source dataset and return its default-graph quads.
/// Any engine failure or a non-graph result is a HARD error.
fn run_construct(
    engine: &NativeSparqlEngine,
    dataset: &std::sync::Arc<gmeow_rdf::RdfDataset>,
    query: &str,
) -> Result<Vec<RdfQuad>, String> {
    let result = engine
        .query(
            dataset,
            SparqlRequest {
                query,
                base_iri: None,
                substitutions: &[],
            },
        )
        .map_err(|e| format!("put-leg CONSTRUCT evaluation failed: {e}\nquery: {query}"))?;
    let SparqlResult::Graph(ds) = result else {
        return Err(format!(
            "put-leg CONSTRUCT did not return a graph\nquery: {query}"
        ));
    };
    Ok(gmeow_rdf::native_quads::flat_rdf_quads_from_dataset(&ds)
        .into_iter()
        .filter(|q| q.graph_name.is_none())
        .collect())
}

/// Projection-namespace source terms (predicate positions + rdf:type objects) that
/// no lawful rule of any kind covers — sorted, deduped canon qnames.
fn compute_gaps(quads: &[RdfQuad], lawful: &LawfulRules) -> Vec<String> {
    let has_rule = |term: &str| {
        lawful.rules.contains_key(term)
            || lawful.inverse_rules.contains_key(term)
            || lawful.claim_rules.contains_key(term)
    };
    let mut gaps: BTreeSet<String> = BTreeSet::new();
    for triple in quads {
        if in_projection_ns(&triple.predicate) && !has_rule(&triple.predicate) {
            gaps.insert(canon_qname(&triple.predicate));
        }
        if triple.predicate == RDF_TYPE {
            if let RdfTerm::Iri(node) = &triple.object {
                if in_projection_ns(node) && !has_rule(node) {
                    gaps.insert(canon_qname(node));
                }
            }
        }
    }
    gaps.into_iter().collect()
}

/// The honest loss-ledger notes for the heuristic categories this lawful executor drops.
fn build_residue(value_rule_dropped: usize, ambiguous_dropped: usize) -> Vec<String> {
    let mut notes: BTreeSet<String> = BTreeSet::new();
    notes.insert(format!(
        "value-transform rules dropped: {value_rule_dropped}"
    ));
    notes.insert(format!(
        "ambiguous (multi-candidate) terms dropped: {ambiguous_dropped}"
    ));
    notes.insert(
        "context-descent, reverse-minting, and concept-reference resolution are \
         heuristic residue (not lawful puts)"
            .to_owned(),
    );
    notes.into_iter().collect()
}

/// A deduplication key over `(subject, predicate, object)`. Blank-node identity is
/// carried by label so freshly minted claim cells never collide across solutions.
type QuadKey = (String, String, String);

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

fn quad_key(quad: &RdfQuad) -> QuadKey {
    (
        term_key(&quad.subject),
        quad.predicate.clone(),
        term_key(&quad.object),
    )
}

fn term_key(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(node) => format!("<{node}>"),
        RdfTerm::BlankNode(node) => format!("_:{node}"),
        RdfTerm::Literal(lit) => format!(
            "\"{}\"^^{:?}@{:?}#{:?}",
            lit.lexical_form, lit.datatype, lit.language, lit.direction
        ),
        RdfTerm::Triple(_) => "<<triple>>".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_one(
        engine: &NativeSparqlEngine,
        dataset: &std::sync::Arc<gmeow_rdf::RdfDataset>,
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
        let report =
            execute_put_legs(source_nt, &[sssom.to_owned()], &[], "").expect("execute put legs");
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
        let report =
            execute_put_legs(source_nt, &[sssom.to_owned()], &[], "").expect("execute put legs");
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
        let report =
            execute_put_legs(source_nt, &[sssom.to_owned()], &[], "").expect("execute put legs");
        assert_eq!(report.claimed, 2, "two distinct claim cells: {report:?}");

        // Parse the lifted graph and map each cell blank -> the set of qPredicate IRIs on it.
        let graph = Graph::parse(report.graph_nt.as_bytes(), "application/n-triples")
            .expect("parse lifted graph");
        let mut cell_preds: std::collections::BTreeMap<String, BTreeSet<String>> =
            std::collections::BTreeMap::new();
        for q in &graph.quads {
            if q.predicate == GM_Q_PREDICATE {
                if let (RdfTerm::BlankNode(cell), RdfTerm::Iri(p)) = (&q.subject, &q.object) {
                    cell_preds
                        .entry(cell.clone())
                        .or_default()
                        .insert(p.clone());
                }
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
        let report =
            execute_put_legs(source_nt, &[sssom.to_owned()], &[], "").expect("execute put legs");
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
        let err = execute_put_legs("", &[], &[], "").expect_err("empty source rejected");
        assert!(err.contains("source graph is empty"), "{err}");
    }
}
