// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native up-projection kernel: consumer RDF -> GMEOW.
//!
//! This module is the Rust authority for the historical `gmeow_tools.up_projection`
//! family. Python remains an interface layer: it supplies serialized RDF and the
//! same repo-or-bundle mapping/cell inputs the public CLI already used.
//!
//! The whole module runs on the oxigraph-free native kernel: RDF is
//! parsed into a frozen `Arc<RdfDataset>` via the canonical native
//! codec, reads are linear scans over the flat default-graph quad stream, the
//! accumulator is a deduped `Vec<RdfQuad>`, the reverse CONSTRUCT queries run
//! through the [`NativeSparqlEngine`], and output is serialized with the native
//! N-Triples writer.
//!
//! The SSSOM/EDOAL/corpus parse + bucket substrate and the invertibility audit
//! live in [`crate::up_projection_corpus`]; this module holds only the heuristic
//! lift engine and imports the substrate one-way.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use gmeow_rdf::{RdfLiteral, RdfQuad, RdfTerm, SparqlEngine, SparqlRequest, SparqlResult};
use gmeow_sparql_eval::NativeSparqlEngine;

use crate::up_projection_corpus::{
    ancestor_closure, canon_qname, domain, dump_nt, edoalpath_pairs, in_projection_ns, is_gmeow_ns,
    multiatom_pairs, object_properties, objects, sssom_clean_pairs, sssom_closematch_pairs,
    structural_pairs, value_mapped_pairs, Graph, TargetSetMap, TermKey, ValueRuleMap,
    ADOPTED_PREDICATES, GM, GM_ANNOTATION, GM_ANN_PROPERTY, GM_ANN_VALUE, GM_AUTHORITY_LINK,
    GM_CONFIDENCE, GM_HAS_TAG, GM_MAPPED_FROM, GM_Q_OBJECT, GM_Q_OBJECT_LITERAL, GM_Q_PREDICATE,
    GM_Q_SUBJECT, GM_STATEMENT_METADATA, GM_TAG, NORMALIZED_PREDICATES, RDFS_LABEL, RDF_TYPE,
    SKOS_EXACT_MATCH, STATEMENT_METADATA_TERMS, WD, XSD_DECIMAL,
};

// Re-exports so the pre-existing `crate::up_projection::{...}` call sites in
// `py.rs` (which a later task repoints) keep resolving while the substrate lives
// in `up_projection_corpus`.
#[cfg(feature = "python")]
pub(crate) use crate::up_projection_corpus::{classify_sssom, combined_class};

const CONCEPT_REFERENCE_PREDICATES: &[&str] = &[
    "https://schema.org/keywords",
    "https://schema.org/programmingLanguage",
    "http://usefulinc.com/ns/doap#programming-language",
    "http://usefulinc.com/ns/doap#category",
];

const GENID: &str = "https://blackcatinformatics.ca/gmeow/.well-known/genid/up-";

type ContextResolution = (String, Option<String>, String, String);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiftMap {
    pub rules: BTreeMap<String, String>,
    pub ambiguous: TargetSetMap,
    pub inverse_rules: BTreeMap<String, String>,
    pub claim_rules: BTreeMap<String, (String, String)>,
    pub object_properties: BTreeSet<String>,
    pub value_rules: ValueRuleMap,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpProjectionReport {
    pub graph_nt: String,
    pub lifted: usize,
    pub claimed: usize,
    pub gap_terms: BTreeMap<String, usize>,
    pub ambiguous_terms: BTreeMap<String, usize>,
    pub claim_terms: BTreeMap<String, usize>,
    pub context_resolved: usize,
    pub context_terms: BTreeMap<String, usize>,
    pub tag_resolved: usize,
    pub tag_resolved_terms: BTreeMap<String, usize>,
    pub minted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    gmeow: String,
    context_type: Option<String>,
    relation: String,
    confidence: String,
}

#[derive(Debug, Clone, Default)]
struct Context {
    candidates: BTreeMap<String, Vec<Candidate>>,
    ancestors: BTreeMap<String, BTreeSet<String>>,
}

struct Acc {
    /// The accumulated default-graph triples, deduped on their `(s, p, o)` string key
    /// (mirroring oxigraph `Store::insert`, which silently dropped identical quads).
    quads: Vec<RdfQuad>,
    seen: HashSet<(String, String, String)>,
    lifted: usize,
    claimed: usize,
    gaps: BTreeMap<String, usize>,
    ambiguous: BTreeMap<String, usize>,
    claims: BTreeMap<String, usize>,
    next_blank: usize,
}

impl Acc {
    fn new() -> Result<Self, String> {
        Ok(Self {
            quads: Vec::new(),
            seen: HashSet::new(),
            lifted: 0,
            claimed: 0,
            gaps: BTreeMap::new(),
            ambiguous: BTreeMap::new(),
            claims: BTreeMap::new(),
            next_blank: 0,
        })
    }

    /// Insert `(s, p, o)` into the accumulator, deduping on the string key (the native
    /// twin of oxigraph `Store::insert`, which silently dropped exact duplicates).
    /// Returns `true` if the triple was new.
    fn insert_triple(&mut self, s: RdfTerm, p: &str, o: RdfTerm) -> bool {
        let key = (full_term_key(&s), p.to_owned(), full_term_key(&o));
        if !self.seen.insert(key) {
            return false;
        }
        self.quads.push(RdfQuad::new(s, p.to_owned(), o));
        true
    }

    /// `true` iff `(s, p, o)` is already accumulated.
    fn contains_triple(&self, s: &RdfTerm, p: &str, o: &RdfTerm) -> bool {
        self.seen
            .contains(&(full_term_key(s), p.to_owned(), full_term_key(o)))
    }

    fn fact(&mut self, s: RdfTerm, p: &str, o: RdfTerm) -> Result<(), String> {
        self.insert_triple(s, p, o);
        self.lifted += 1;
        Ok(())
    }

    fn claim(
        &mut self,
        s: RdfTerm,
        p: &str,
        o: RdfTerm,
        source_term: &str,
        conf: &str,
    ) -> Result<(), String> {
        let source_key = canon_qname(source_term);
        emit_claim(self, s, p, o, source_term, conf)?;
        self.claimed += 1;
        *self.claims.entry(source_key).or_insert(0) += 1;
        Ok(())
    }

    fn fresh_blank(&mut self, prefix: &str) -> RdfTerm {
        self.next_blank += 1;
        RdfTerm::BlankNode(format!("{prefix}{}", self.next_blank))
    }
}

pub fn build_lift_map(
    sssom_texts: &[String],
    projection_ttls: &[String],
    ontology_nt: &str,
) -> Result<LiftMap, String> {
    let direct_edoalpath;
    let inverse_edoalpath;
    {
        let paths = edoalpath_pairs(projection_ttls)?;
        direct_edoalpath = paths.0;
        inverse_edoalpath = paths.1;
    }
    let identity = sssom_clean_pairs(sssom_texts)?;
    let (exact_struct, generalizing_struct) = structural_pairs(projection_ttls)?;
    let mut projection: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for layer in [&exact_struct, &direct_edoalpath] {
        for (target, gmeows) in layer {
            projection
                .entry(target.clone())
                .or_default()
                .extend(gmeows.iter().cloned());
        }
    }

    let mut rules = BTreeMap::new();
    let mut ambiguous = BTreeMap::new();
    let mut targets: BTreeSet<String> = identity.keys().cloned().collect();
    targets.extend(projection.keys().cloned());
    for target in targets {
        let ids = identity.get(&target).cloned().unwrap_or_default();
        if ids.len() == 1 {
            rules.insert(target, ids.into_iter().next().expect("one identity"));
        } else if ids.len() > 1 {
            ambiguous.insert(target, ids);
        } else {
            let projs = projection.get(&target).cloned().unwrap_or_default();
            if projs.len() == 1 {
                rules.insert(target, projs.into_iter().next().expect("one projection"));
            } else {
                ambiguous.insert(target, projs);
            }
        }
    }

    let mut inverse_rules = BTreeMap::new();
    for (target, gmeows) in inverse_edoalpath {
        if rules.contains_key(&target) || ambiguous.contains_key(&target) {
            continue;
        }
        if gmeows.len() == 1 {
            inverse_rules.insert(target, gmeows.into_iter().next().expect("one inverse"));
        } else {
            ambiguous.insert(target, gmeows);
        }
    }

    let mut claim_rules = BTreeMap::new();
    add_claims(
        &mut claim_rules,
        &mut ambiguous,
        &rules,
        &inverse_rules,
        &generalizing_struct,
    );
    let close = sssom_closematch_pairs(sssom_texts)?;
    add_claims(
        &mut claim_rules,
        &mut ambiguous,
        &rules,
        &inverse_rules,
        &close,
    );

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

    Ok(LiftMap {
        rules,
        ambiguous,
        inverse_rules,
        claim_rules,
        object_properties: object_properties(ontology_nt)?,
        value_rules: value_mapped_pairs(projection_ttls)?,
    })
}

pub fn up_project_nt(
    source_nt: &str,
    sssom_texts: &[String],
    projection_ttls: &[String],
    ontology_nt: &str,
    descend: bool,
) -> Result<UpProjectionReport, String> {
    let source = Graph::parse(source_nt.as_bytes(), "application/n-triples")?;
    if source.is_empty() {
        return Err(if descend {
            "up_project_descend: source graph is empty"
        } else {
            "up_project: source graph is empty"
        }
        .to_owned());
    }
    let lift = build_lift_map(sssom_texts, projection_ttls, ontology_nt)?;
    if descend {
        up_project_descend_store(&source, &lift, sssom_texts, projection_ttls, ontology_nt)
    } else {
        up_project_store(&source, &lift)
    }
}

pub fn resolve_context_candidate(
    predicate: &str,
    subject_types: &BTreeSet<String>,
    sssom_texts: &[String],
    projection_ttls: &[String],
    ontology_nt: &str,
) -> Result<Option<ContextResolution>, String> {
    let ctx = build_context(sssom_texts, projection_ttls, ontology_nt)?;
    Ok(
        resolve_candidate(predicate, subject_types, &ctx).map(|cand| {
            (
                cand.gmeow,
                cand.context_type,
                cand.relation,
                cand.confidence,
            )
        }),
    )
}

pub fn reverse_nt(source_nt: &str) -> Result<String, String> {
    let source = Graph::parse(source_nt.as_bytes(), "application/n-triples")?;
    let mut acc = Acc::new()?;
    apply_reverse(&source, &mut acc)?;
    dump_nt(&acc.quads)
}

fn up_project_store(source: &Graph, lift: &LiftMap) -> Result<UpProjectionReport, String> {
    let mut acc = Acc::new()?;
    for triple in &source.quads {
        lift_edge(
            &mut acc,
            triple.subject.clone(),
            &triple.predicate,
            triple.object.clone(),
            lift,
        )?;
    }
    let minted = apply_reverse(source, &mut acc)?;
    let tag_terms = resolve_concept_references(source, &mut acc)?;
    finish_report(acc, 0, BTreeMap::new(), tag_terms, minted)
}

fn up_project_descend_store(
    source: &Graph,
    lift: &LiftMap,
    sssom_texts: &[String],
    projection_ttls: &[String],
    ontology_nt: &str,
) -> Result<UpProjectionReport, String> {
    let ctx = build_context(sssom_texts, projection_ttls, ontology_nt)?;
    let mut node_types: BTreeMap<TermKey, BTreeSet<String>> = BTreeMap::new();
    for triple in &source.quads {
        if triple.predicate != RDF_TYPE {
            continue;
        }
        let RdfTerm::Iri(t) = &triple.object else {
            continue;
        };
        let key = t.as_str();
        if let Some(rule) = lift.rules.get(key) {
            node_types
                .entry(TermKey::from_subject(&triple.subject))
                .or_default()
                .insert(rule.clone());
        } else if let Some((gmeow, _conf)) = lift.claim_rules.get(key) {
            node_types
                .entry(TermKey::from_subject(&triple.subject))
                .or_default()
                .insert(gmeow.clone());
        } else if is_gmeow_ns(key) {
            node_types
                .entry(TermKey::from_subject(&triple.subject))
                .or_default()
                .insert(key.to_owned());
        }
    }

    let mut acc = Acc::new()?;
    let mut context_resolved = 0;
    let mut context_terms = BTreeMap::new();
    for triple in &source.quads {
        let key = triple.predicate.clone();
        if triple.predicate == RDF_TYPE
            || lift.rules.contains_key(&key)
            || lift.inverse_rules.contains_key(&key)
        {
            lift_edge(
                &mut acc,
                triple.subject.clone(),
                &triple.predicate,
                triple.object.clone(),
                lift,
            )?;
            continue;
        }
        let subject_types = node_types
            .get(&TermKey::from_subject(&triple.subject))
            .cloned()
            .unwrap_or_default();
        let Some(cand) = resolve_candidate(&key, &subject_types, &ctx) else {
            lift_edge(
                &mut acc,
                triple.subject.clone(),
                &triple.predicate,
                triple.object.clone(),
                lift,
            )?;
            continue;
        };
        if cand.relation == "=" {
            acc.fact(triple.subject.clone(), &cand.gmeow, triple.object.clone())?;
        } else if matches!(triple.subject, RdfTerm::Iri(_)) {
            if matches!(triple.object, RdfTerm::Iri(_) | RdfTerm::Literal(_)) {
                acc.claim(
                    triple.subject.clone(),
                    &cand.gmeow,
                    triple.object.clone(),
                    &triple.predicate,
                    &cand.confidence,
                )?;
            } else {
                lift_edge(
                    &mut acc,
                    triple.subject.clone(),
                    &triple.predicate,
                    triple.object.clone(),
                    lift,
                )?;
                continue;
            }
        } else {
            lift_edge(
                &mut acc,
                triple.subject.clone(),
                &triple.predicate,
                triple.object.clone(),
                lift,
            )?;
            continue;
        }
        context_resolved += 1;
        *context_terms.entry(canon_qname(&key)).or_insert(0) += 1;
    }
    let minted = apply_reverse(source, &mut acc)?;
    let tag_terms = resolve_concept_references(source, &mut acc)?;
    finish_report(acc, context_resolved, context_terms, tag_terms, minted)
}

fn lift_edge(acc: &mut Acc, s: RdfTerm, p: &str, o: RdfTerm, lift: &LiftMap) -> Result<(), String> {
    if p == RDF_TYPE {
        if let RdfTerm::Iri(class) = &o {
            let key = class.as_str();
            if let Some(target) = lift.rules.get(key) {
                acc.fact(s, RDF_TYPE, RdfTerm::iri(target))?;
            } else if let Some((gmeow, conf)) = lift.claim_rules.get(key) {
                if matches!(s, RdfTerm::Iri(_)) {
                    let class_iri = class.clone();
                    acc.claim(s, RDF_TYPE, RdfTerm::iri(gmeow), &class_iri, conf)?;
                }
            } else if is_gmeow_ns(key) {
                acc.fact(s, RDF_TYPE, o)?;
            } else {
                account(acc, lift, key);
            }
        }
        return Ok(());
    }

    let key = p;
    if let RdfTerm::Literal(lit) = &o {
        if let Some((gpred, gval)) = lift
            .value_rules
            .get(&(key.to_owned(), lit.lexical_form.clone()))
        {
            acc.fact(s, gpred, RdfTerm::iri(gval))?;
            return Ok(());
        }
    }
    if let Some(target) = lift.rules.get(key) {
        if matches!(o, RdfTerm::Literal(_)) && lift.object_properties.contains(target) {
            if matches!(s, RdfTerm::Iri(_)) {
                acc.claim(s, target, o, p, "")?;
            }
            return Ok(());
        }
        acc.fact(s, target, o)?;
    } else if let Some(target) = lift.inverse_rules.get(key) {
        match o {
            RdfTerm::Iri(_) | RdfTerm::BlankNode(_) => {
                acc.fact(o, target, s)?;
            }
            RdfTerm::Literal(_) | RdfTerm::Triple(_) => {}
        }
    } else if let Some((gmeow, conf)) = lift.claim_rules.get(key) {
        if matches!(s, RdfTerm::Iri(_)) && matches!(o, RdfTerm::Iri(_) | RdfTerm::Literal(_)) {
            acc.claim(s, gmeow, o, p, conf)?;
        }
    } else if is_gmeow_ns(key) {
        acc.fact(s, p, o)?;
    } else {
        account(acc, lift, key);
    }
    Ok(())
}

fn emit_claim(
    acc: &mut Acc,
    subj: RdfTerm,
    qpred: &str,
    qobj: RdfTerm,
    source_term: &str,
    conf: &str,
) -> Result<(), String> {
    let cell = acc.fresh_blank("up-claim-");
    acc.insert_triple(cell.clone(), RDF_TYPE, RdfTerm::iri(GM_STATEMENT_METADATA));
    acc.insert_triple(cell.clone(), GM_Q_SUBJECT, subj);
    acc.insert_triple(cell.clone(), GM_Q_PREDICATE, RdfTerm::iri(qpred));
    let qobj_pred = if matches!(qobj, RdfTerm::Literal(_)) {
        GM_Q_OBJECT_LITERAL
    } else {
        GM_Q_OBJECT
    };
    acc.insert_triple(cell.clone(), qobj_pred, qobj);
    emit_annotation(acc, cell.clone(), GM_MAPPED_FROM, RdfTerm::iri(source_term))?;
    if !conf.is_empty() {
        emit_annotation(
            acc,
            cell,
            GM_CONFIDENCE,
            RdfTerm::Literal(RdfLiteral::typed(conf, XSD_DECIMAL)),
        )?;
    }
    Ok(())
}

fn emit_annotation(
    acc: &mut Acc,
    cell: RdfTerm,
    property: &str,
    value: RdfTerm,
) -> Result<(), String> {
    let ann = acc.fresh_blank("up-ann-");
    acc.insert_triple(cell, GM_ANNOTATION, ann.clone());
    acc.insert_triple(ann.clone(), GM_ANN_PROPERTY, RdfTerm::iri(property));
    acc.insert_triple(ann, GM_ANN_VALUE, value);
    Ok(())
}

fn account(acc: &mut Acc, lift: &LiftMap, key: &str) {
    if lift.ambiguous.contains_key(key) {
        *acc.ambiguous.entry(canon_qname(key)).or_insert(0) += 1;
    } else if in_projection_ns(key) {
        *acc.gaps.entry(canon_qname(key)).or_insert(0) += 1;
    }
}

fn add_claims(
    claim_rules: &mut BTreeMap<String, (String, String)>,
    ambiguous: &mut BTreeMap<String, BTreeSet<String>>,
    rules: &BTreeMap<String, String>,
    inverse_rules: &BTreeMap<String, String>,
    candidates: &BTreeMap<String, BTreeMap<String, String>>,
) {
    for (target, cands) in candidates {
        if rules.contains_key(target)
            || inverse_rules.contains_key(target)
            || ambiguous.contains_key(target)
            || claim_rules.contains_key(target)
        {
            continue;
        }
        if cands.len() == 1 {
            let (gmeow, conf) = cands.iter().next().expect("one claim candidate");
            claim_rules.insert(target.clone(), (gmeow.clone(), conf.clone()));
        } else {
            ambiguous.insert(target.clone(), cands.keys().cloned().collect());
        }
    }
}

fn build_context(
    sssom_texts: &[String],
    projection_ttls: &[String],
    ontology_nt: &str,
) -> Result<Context, String> {
    let graph = Graph::parse(ontology_nt.as_bytes(), "application/n-triples")?;
    let ontology_quads = &graph.quads;
    let ancestors = ancestor_closure(ontology_quads);
    let mut candidates: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
    let mut add =
        |target: String, gmeow: String, relation: &str, conf: String| -> Result<(), String> {
            candidates.entry(target).or_default().push(Candidate {
                context_type: domain(ontology_quads, &gmeow),
                gmeow,
                relation: relation.to_owned(),
                confidence: conf,
            });
            Ok(())
        };
    for (target, gmeows) in sssom_clean_pairs(sssom_texts)? {
        for gmeow in gmeows {
            add(target.clone(), gmeow, "=", String::new())?;
        }
    }
    for (target, gmeow_confs) in sssom_closematch_pairs(sssom_texts)? {
        for (gmeow, conf) in gmeow_confs {
            add(target.clone(), gmeow, "<=", conf)?;
        }
    }
    let (exact, generalizing) = structural_pairs(projection_ttls)?;
    for (target, gmeows) in exact {
        for gmeow in gmeows {
            add(target.clone(), gmeow, "=", String::new())?;
        }
    }
    for (target, gmeow_confs) in generalizing {
        for (gmeow, conf) in gmeow_confs {
            add(target.clone(), gmeow, "<=", conf)?;
        }
    }
    let (direct, _inverse) = edoalpath_pairs(projection_ttls)?;
    for (target, gmeows) in direct {
        for gmeow in gmeows {
            add(target.clone(), gmeow, "=", String::new())?;
        }
    }
    for (target, sources) in multiatom_pairs(projection_ttls)? {
        for gmeow in sources {
            add(target.clone(), gmeow, "=", String::new())?;
        }
    }
    for cands in candidates.values_mut() {
        let mut seen = BTreeSet::new();
        cands.retain(|cand| {
            seen.insert((
                cand.gmeow.clone(),
                cand.context_type.clone(),
                cand.relation.clone(),
                cand.confidence.clone(),
            ))
        });
    }
    Ok(Context {
        candidates,
        ancestors,
    })
}

fn resolve_candidate(
    predicate: &str,
    subject_types: &BTreeSet<String>,
    ctx: &Context,
) -> Option<Candidate> {
    let cands = ctx.candidates.get(predicate)?;
    if subject_types.is_empty() {
        return None;
    }
    let mut supers = BTreeSet::new();
    for t in subject_types {
        if let Some(anc) = ctx.ancestors.get(t) {
            supers.extend(anc.iter().cloned());
        } else {
            supers.insert(t.clone());
        }
    }
    let typed: Vec<&Candidate> = cands
        .iter()
        .filter(|c| {
            c.context_type
                .as_ref()
                .is_some_and(|ct| supers.contains(ct))
        })
        .collect();
    let facts: Vec<&Candidate> = typed
        .iter()
        .copied()
        .filter(|c| c.relation == "=")
        .collect();
    let tier = if facts.is_empty() { typed } else { facts };
    if tier.is_empty() {
        return None;
    }
    let minima: Vec<&Candidate> = tier
        .iter()
        .copied()
        .filter(|c| {
            tier.iter().all(|d| {
                narrower_or_equal(c.context_type.as_deref(), d.context_type.as_deref(), ctx)
            })
        })
        .collect();
    let chosen: BTreeSet<&String> = minima.iter().map(|c| &c.gmeow).collect();
    (chosen.len() == 1).then(|| minima[0].clone())
}

fn narrower_or_equal(a: Option<&str>, b: Option<&str>, ctx: &Context) -> bool {
    let Some(a) = a else {
        return false;
    };
    a == b.unwrap_or_default()
        || b.is_some_and(|b| ctx.ancestors.get(a).is_some_and(|anc| anc.contains(b)))
}

fn apply_reverse(source: &Graph, acc: &mut Acc) -> Result<usize, String> {
    let mut count = 0;
    let engine = NativeSparqlEngine::new();
    for query in reverse_queries() {
        let result = engine
            .query(
                &source.dataset,
                SparqlRequest {
                    query: &query,
                    base_iri: None,
                    substitutions: &[],
                },
            )
            .map_err(|e| format!("reverse query evaluation failed: {e}"))?;
        let SparqlResult::Graph(ds) = result else {
            return Err("reverse query did not return a graph".to_owned());
        };
        for quad in gmeow_rdf::native_quads::flat_rdf_quads_from_dataset(&ds) {
            if quad.graph_name.is_some() {
                continue;
            }
            if !acc.contains_triple(&quad.subject, &quad.predicate, &quad.object) {
                acc.insert_triple(quad.subject, &quad.predicate, quad.object);
                count += 1;
            }
        }
    }
    Ok(count)
}

fn resolve_concept_references(
    source: &Graph,
    acc: &mut Acc,
) -> Result<BTreeMap<String, usize>, String> {
    // Snapshot the accumulator's current quads for the read pass; the final loop
    // mutates `acc`, so reads run against this frozen view (matching the old code,
    // where `index` is built from the store state before the insert loop).
    let acc_quads = acc.quads.clone();
    let mut anchored: BTreeSet<String> = BTreeSet::new();
    for q in &acc_quads {
        if q.predicate != RDF_TYPE || !matches!(&q.object, RdfTerm::Iri(n) if n == GM_TAG) {
            continue;
        }
        let RdfTerm::Iri(tag) = &q.subject else {
            continue;
        };
        let tag_term = RdfTerm::iri(tag.clone());
        if tag.starts_with(WD)
            || [SKOS_EXACT_MATCH, GM_AUTHORITY_LINK].iter().any(|pred| {
                objects(&acc_quads, &tag_term, pred)
                    .into_iter()
                    .any(|o| matches!(o, RdfTerm::Iri(n) if n.starts_with(WD)))
            })
        {
            anchored.insert(tag.clone());
        }
    }
    let mut by_label: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for tag in anchored {
        for label in objects(&acc_quads, &RdfTerm::iri(tag.clone()), RDFS_LABEL) {
            if let RdfTerm::Literal(lit) = label {
                by_label
                    .entry(normalize_label(&lit.lexical_form))
                    .or_default()
                    .insert(tag.clone());
            }
        }
    }
    let index: BTreeMap<String, String> = by_label
        .into_iter()
        .filter_map(|(label, tags)| {
            (tags.len() == 1).then(|| (label, tags.into_iter().next().expect("one tag")))
        })
        .collect();
    if index.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut terms = BTreeMap::new();
    for triple in &source.quads {
        if !CONCEPT_REFERENCE_PREDICATES.contains(&triple.predicate.as_str()) {
            continue;
        }
        let RdfTerm::Literal(lit) = &triple.object else {
            continue;
        };
        let Some(tag) = index.get(&normalize_label(&lit.lexical_form)) else {
            continue;
        };
        let subject = triple.subject.clone();
        let object = RdfTerm::iri(tag.clone());
        if !acc.contains_triple(&subject, GM_HAS_TAG, &object) {
            acc.insert_triple(subject, GM_HAS_TAG, object);
            *terms.entry(canon_qname(&triple.predicate)).or_insert(0) += 1;
        }
    }
    Ok(terms)
}

fn finish_report(
    acc: Acc,
    context_resolved: usize,
    context_terms: BTreeMap<String, usize>,
    tag_terms: BTreeMap<String, usize>,
    minted: usize,
) -> Result<UpProjectionReport, String> {
    Ok(UpProjectionReport {
        graph_nt: dump_nt(&acc.quads)?,
        lifted: acc.lifted,
        claimed: acc.claimed,
        gap_terms: acc.gaps,
        ambiguous_terms: acc.ambiguous,
        claim_terms: acc.claims,
        context_resolved,
        context_terms,
        tag_resolved: tag_terms.values().sum(),
        tag_resolved_terms: tag_terms,
        minted,
    })
}

/// A full term-identity key for accumulator deduplication: unlike `term_key`
/// (which keys only on a literal's lexical form for list traversal), this folds in
/// the literal datatype, language tag, and direction so two literals that differ
/// only in those slots are NOT collapsed — matching oxigraph `Store` term identity.
fn full_term_key(term: &RdfTerm) -> String {
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

fn normalize_label(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn reverse_queries() -> Vec<String> {
    let prefixes = format!(
        r#"
PREFIX gmeow: <{GM}>
PREFIX rdf: <{RDF_TYPE_NS}>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
PREFIX foaf: <http://xmlns.com/foaf/0.1/>
PREFIX vcard: <http://www.w3.org/2006/vcard/ns#>
PREFIX schema: <https://schema.org/>
PREFIX gedcom: <http://www.w3.org/2000/10/swap/pim/gedcom#>
PREFIX doap: <http://usefulinc.com/ns/doap#>
PREFIX sioc: <http://rdfs.org/sioc/ns#>
PREFIX time: <http://www.w3.org/2006/time#>
PREFIX dcterms: <http://purl.org/dc/terms/>
PREFIX bf: <http://id.loc.gov/ontologies/bibframe/>
"#,
        RDF_TYPE_NS = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    );
    let mut queries = Vec::new();
    for (source_pred, part_type) in [
        ("foaf:givenName", "namePartGiven"),
        ("schema:givenName", "namePartGiven"),
        ("vcard:given-name", "namePartGiven"),
        ("foaf:familyName", "namePartSurname"),
        ("schema:familyName", "namePartSurname"),
        ("vcard:family-name", "namePartSurname"),
    ] {
        queries.push(format!(
            r#"{prefixes}
CONSTRUCT {{
  ?p gmeow:hasName ?app .
  ?app rdf:type gmeow:PersonName .
  ?app gmeow:hasNamePart ?part .
  ?part gmeow:namePartType gmeow:{part_type} .
  ?part gmeow:partText ?v .
}}
WHERE {{
  ?p {source_pred} ?v .
  FILTER(isLiteral(?v))
  BIND(IRI(CONCAT("{GENID}name-", MD5(STR(?p)))) AS ?app)
  BIND(IRI(CONCAT("{GENID}part-{part_type}-", MD5(CONCAT(STR(?p), "|", STR(?v))))) AS ?part)
}}"#
        ));
    }
    for source_pred in ["foaf:name", "schema:name", "vcard:fn"] {
        queries.push(format!(
            r#"{prefixes}
CONSTRUCT {{
  ?p gmeow:hasName ?app .
  ?app rdf:type gmeow:PersonName .
  ?app gmeow:fullName ?v .
}}
WHERE {{
  ?p {source_pred} ?v .
  FILTER(isLiteral(?v))
  BIND(IRI(CONCAT("{GENID}name-", MD5(STR(?p)))) AS ?app)
}}"#
        ));
    }
    for source_pred in ["foaf:nick", "vcard:nickname"] {
        queries.push(format!(
            r#"{prefixes}
CONSTRUCT {{
  ?p gmeow:hasName ?nn .
  ?nn rdf:type gmeow:PersonName .
  ?nn gmeow:namePurpose gmeow:namePurposeNickname .
  ?nn gmeow:fullName ?v .
}}
WHERE {{
  ?p {source_pred} ?v .
  FILTER(isLiteral(?v))
  BIND(IRI(CONCAT("{GENID}nick-", MD5(CONCAT(STR(?p), "|", STR(?v))))) AS ?nn)
}}"#
        ));
    }
    for (source_pred, cp_type) in [
        ("vcard:hasEmail", "EmailAddress"),
        ("foaf:mbox", "EmailAddress"),
        ("vcard:hasTelephone", "TelephoneNumber"),
        ("foaf:phone", "TelephoneNumber"),
        ("vcard:hasInstantMessage", "InstantMessageAddress"),
    ] {
        queries.push(format!(
            r#"{prefixes}
CONSTRUCT {{
  ?p gmeow:hasContactPoint ?cp .
  ?cp rdf:type gmeow:{cp_type} .
}}
WHERE {{
  ?p {source_pred} ?cp .
  FILTER(isIRI(?cp))
}}"#
        ));
    }
    queries.extend([
        format!(
            r#"{prefixes}
CONSTRUCT {{
  ?cr rdf:type gmeow:CoupleRelationship .
  ?cr gmeow:hasPartner ?h .
  ?cr gmeow:hasPartner ?w .
  ?cr gmeow:withinFamily ?fam .
}}
WHERE {{
  ?fam gedcom:husband ?h . ?fam gedcom:wife ?w .
  BIND(IRI(CONCAT("{GENID}couple-", MD5(STR(?fam)))) AS ?cr)
}}"#
        ),
        format!(
            r#"{prefixes}
CONSTRUCT {{
  ?pcr rdf:type gmeow:ParentChildRelationship .
  ?pcr gmeow:relationshipChild ?c .
  ?pcr gmeow:withinFamily ?fam .
}}
WHERE {{
  {{ ?c gedcom:childIn ?fam }} UNION {{ ?fam gedcom:child ?c }}
  BIND(IRI(CONCAT("{GENID}pcr-", MD5(CONCAT(STR(?fam), "|", STR(?c))))) AS ?pcr)
}}"#
        ),
        format!(
            r#"{prefixes}
CONSTRUCT {{ ?parent gmeow:hasChild ?c . }}
WHERE {{
  {{ ?c gedcom:childIn ?fam }} UNION {{ ?fam gedcom:child ?c }}
  {{ ?fam gedcom:husband ?parent }} UNION {{ ?fam gedcom:wife ?parent }}
}}"#
        ),
    ]);
    for (source_pred, slug, role) in [
        ("doap:maintainer", "maint", "roleSoftwareMaintainer"),
        ("doap:developer", "dev", "roleSoftwareDeveloper"),
    ] {
        queries.push(format!(
            r#"{prefixes}
CONSTRUCT {{
  ?contrib rdf:type gmeow:Contribution .
  ?contrib gmeow:contributionTarget ?proj .
  ?contrib gmeow:contributor ?agent .
  ?contrib gmeow:contributionRole gmeow:{role} .
}}
WHERE {{
  ?proj {source_pred} ?agent .
  FILTER(isIRI(?agent))
  BIND(IRI(CONCAT("{GENID}contrib-{slug}-", MD5(CONCAT(STR(?proj), "|", STR(?agent))))) AS ?contrib)
}}"#
        ));
    }
    for (src_type, gmeow_type) in [
        ("sioc:Post", "gmeow:EmailMessage"),
        ("sioc:Thread", "gmeow:Thread"),
        ("sioc:Container", "gmeow:Thread"),
        ("bf:Work", "gmeow:Work"),
    ] {
        queries.push(format!(
            "{prefixes}\nCONSTRUCT {{ ?s rdf:type {gmeow_type} . }} WHERE {{ ?s rdf:type {src_type} . }}"
        ));
    }
    for (src_pred, gmeow_pred) in [
        ("sioc:has_container", "gmeow:partOfThread"),
        ("sioc:reply_of", "gmeow:inReplyTo"),
        ("sioc:has_creator", "gmeow:from"),
        ("sioc:topic", "gmeow:isAbout"),
        ("sioc:link", "gmeow:sourceLocation"),
        ("time:hasTime", "gmeow:eventTime"),
        ("doap:repository", "gmeow:hasRepository"),
        ("doap:browse", "gmeow:webUrl"),
        ("dcterms:rights", "gmeow:copyrightNotice"),
        ("bf:title", "gmeow:hasTitle"),
    ] {
        queries.push(format!(
            "{prefixes}\nCONSTRUCT {{ ?s {gmeow_pred} ?o . }} WHERE {{ ?s {src_pred} ?o . }}"
        ));
    }
    for (src_pred, gmeow_pred) in [
        ("sioc:container_of", "gmeow:partOfThread"),
        ("sioc:has_reply", "gmeow:inReplyTo"),
        ("foaf:depiction", "gmeow:depicts"),
        ("foaf:publications", "gmeow:hasAuthor"),
    ] {
        queries.push(format!(
            "{prefixes}\nCONSTRUCT {{ ?o {gmeow_pred} ?s . }} WHERE {{ ?s {src_pred} ?o . }}"
        ));
    }
    for (src_pred, gmeow_type) in [
        ("doap:repository", "gmeow:Repository"),
        ("foaf:depiction", "gmeow:MediaObject"),
    ] {
        queries.push(format!(
            "{prefixes}\nCONSTRUCT {{ ?o rdf:type {gmeow_type} . }} WHERE {{ ?s {src_pred} ?o . }}"
        ));
    }
    queries.push(format!(
        r#"{prefixes}
CONSTRUCT {{
  ?m rdf:type gmeow:Membership .
  ?m gmeow:membershipMember ?person .
  ?m gmeow:hasRole ?role .
  ?role rdfs:label ?title .
}}
WHERE {{
  {{ {{ ?person schema:jobTitle ?title }} UNION {{ ?person foaf:title ?title }} }}
  FILTER(isLiteral(?title))
  BIND(IRI(CONCAT("{GENID}membership-", MD5(CONCAT(STR(?person), "|", STR(?title))))) AS ?m)
  BIND(IRI(CONCAT("{GENID}role-", MD5(CONCAT(STR(?person), "|", STR(?title))))) AS ?role)
}}"#
    ));
    queries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::up_projection_corpus::RDFS_DOMAIN;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn native_inverse_path_preserves_blank_node_subject_after_swap() {
        let source = r#"<https://example.org/organizations/meridian-institute> <https://schema.org/alumni> _:alum .
"#;
        let report = up_project_nt(
            source,
            &repo_sssom_texts(),
            &repo_projection_ttls(),
            "",
            false,
        )
        .expect("native up-projection succeeds");
        assert!(report.graph_nt.contains(
            "_:alum <https://blackcatinformatics.ca/gmeow/alumniOf> <https://example.org/organizations/meridian-institute>"
        ));
    }

    #[test]
    fn native_context_resolves_multiatom_blank_node_identifier_legs() {
        let source = r#"
_:id <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://schema.org/PropertyValue> .
_:id <https://schema.org/value> "0000-0002-1825-0097" .
_:id <https://schema.org/propertyID> "orcid" .
_:id <https://schema.org/url> <https://orcid.org/0000-0002-1825-0097> .
"#;
        let report = up_project_nt(
            source,
            &repo_sssom_texts(),
            &repo_projection_ttls(),
            &minimal_context_ontology_nt(),
            true,
        )
        .expect("native descent succeeds");
        assert_eq!(report.context_resolved, 3);
        assert!(!report.ambiguous_terms.contains_key("schema:url"));
        assert!(report.graph_nt.contains(
            "_:id <https://blackcatinformatics.ca/gmeow/identifierUrl> <https://orcid.org/0000-0002-1825-0097>"
        ));
        assert!(report.graph_nt.contains(
            "_:id <https://blackcatinformatics.ca/gmeow/identifierValue> \"0000-0002-1825-0097\""
        ));
        assert!(report
            .graph_nt
            .contains("_:id <https://blackcatinformatics.ca/gmeow/identifierScheme> \"orcid\""));
    }

    #[test]
    fn native_context_uses_existing_gmeow_types() {
        let source = r#"
_:id <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/Identifier> .
_:id <https://schema.org/value> "0000-0002-1825-0097" .
"#;
        let report = up_project_nt(
            source,
            &repo_sssom_texts(),
            &repo_projection_ttls(),
            &minimal_context_ontology_nt(),
            true,
        )
        .expect("native descent succeeds");
        assert_eq!(report.context_resolved, 1);
        assert!(report.graph_nt.contains(
            "_:id <https://blackcatinformatics.ca/gmeow/identifierValue> \"0000-0002-1825-0097\""
        ));
    }

    #[test]
    fn descend_mode_applies_reverse_minting() {
        let source = r#"<https://example.org/person/alex> <https://schema.org/jobTitle> "Engineer" .
"#;
        let report = up_project_nt(
            source,
            &repo_sssom_texts(),
            &repo_projection_ttls(),
            "",
            true,
        )
        .expect("native descent succeeds");
        assert!(report.minted > 0);
        assert!(report
            .graph_nt
            .contains("<https://blackcatinformatics.ca/gmeow/membershipMember>"));
        assert!(report
            .graph_nt
            .contains("<https://blackcatinformatics.ca/gmeow/hasRole>"));
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn repo_sssom_texts() -> Vec<String> {
        read_matching_files(&repo_root().join("generated/mappings"), ".sssom.tsv")
    }

    fn repo_projection_ttls() -> Vec<String> {
        let root = repo_root();
        let mut files = read_matching_files(&root.join("dsl/mappings/projections"), ".ttl");
        let slices = root.join("slices");
        let mut slice_mapping_files = Vec::new();
        for group in sorted_dirs(&slices) {
            for slice in sorted_dirs(&group) {
                slice_mapping_files.extend(read_matching_files(&slice.join("mappings"), ".ttl"));
            }
        }
        files.extend(slice_mapping_files);
        files
    }

    fn read_matching_files(dir: &Path, suffix: &str) -> Vec<String> {
        if !dir.exists() {
            return Vec::new();
        }
        let mut paths = fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with(suffix))
            })
            .collect::<Vec<_>>();
        paths.sort();
        paths
            .into_iter()
            .map(|path| {
                fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
            })
            .collect()
    }

    fn sorted_dirs(dir: &Path) -> Vec<PathBuf> {
        let mut paths = fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }

    fn minimal_context_ontology_nt() -> String {
        [
            format!("<{GM}identifierUrl> <{RDFS_DOMAIN}> <{GM}Identifier> ."),
            format!("<{GM}identifierValue> <{RDFS_DOMAIN}> <{GM}Identifier> ."),
            format!("<{GM}identifierScheme> <{RDFS_DOMAIN}> <{GM}Identifier> ."),
        ]
        .join("\n")
    }
}
