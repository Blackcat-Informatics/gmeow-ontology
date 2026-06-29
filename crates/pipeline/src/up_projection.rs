// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native up-projection kernel: consumer RDF -> GMEOW.
//!
//! This module is the Rust authority for the historical `gmeow_tools.up_projection`
//! family. Python remains an interface layer: it supplies serialized RDF and the
//! same repo-or-bundle mapping/cell inputs the public CLI already used.
//!
//! The whole module runs on the oxigraph-free native kernel (EPIC #906): RDF is
//! parsed into a frozen [`Arc<RdfDataset>`](RdfDataset) via the canonical native
//! codec, reads are linear scans over the flat default-graph quad stream, the
//! accumulator is a deduped `Vec<RdfQuad>`, the reverse CONSTRUCT queries run
//! through the [`NativeSparqlEngine`], and output is serialized with the native
//! N-Triples writer.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::{Arc, OnceLock};

use gmeow_rdf::{
    RdfDataset, RdfLiteral, RdfQuad, RdfTerm, SparqlEngine, SparqlRequest, SparqlResult,
};
use gmeow_sparql_eval::NativeSparqlEngine;

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_SUB_CLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
const SKOS_CLOSE_MATCH: &str = "http://www.w3.org/2004/02/skos/core#closeMatch";
const SKOS_PREF_LABEL: &str = "http://www.w3.org/2004/02/skos/core#prefLabel";
const SKOS_ALT_LABEL: &str = "http://www.w3.org/2004/02/skos/core#altLabel";
const WD: &str = "http://www.wikidata.org/entity/";

const GM_PROJECTION_MAPPING: &str = "https://blackcatinformatics.ca/gmeow/ProjectionMapping";
const GM_HAS_MAPPING_PATTERN: &str = "https://blackcatinformatics.ca/gmeow/hasMappingPattern";
const GM_MINT: &str = "https://blackcatinformatics.ca/gmeow/mint";
const GM_PATH: &str = "https://blackcatinformatics.ca/gmeow/path";
const GM_FILTER: &str = "https://blackcatinformatics.ca/gmeow/filter";
const GM_EDOAL_SOURCE: &str = "https://blackcatinformatics.ca/gmeow/edoalSource";
const GM_EDOAL_PATH: &str = "https://blackcatinformatics.ca/gmeow/edoalPath";
const GM_HAS_BINDING: &str = "https://blackcatinformatics.ca/gmeow/hasBinding";
const GM_TEMPLATE_ATOMS: &str = "https://blackcatinformatics.ca/gmeow/templateAtoms";
const GM_TO_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/toPredicate";
const GM_TO_CLASS: &str = "https://blackcatinformatics.ca/gmeow/toClass";
const GM_RELATION: &str = "https://blackcatinformatics.ca/gmeow/relation";
const GM_CONFIDENCE: &str = "https://blackcatinformatics.ca/gmeow/confidence";
const GM_ATOM: &str = "https://blackcatinformatics.ca/gmeow/atom";
const GM_ANCHOR: &str = "https://blackcatinformatics.ca/gmeow/anchor";
const GM_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/predicate";
const GM_SUBJECT_VAR: &str = "https://blackcatinformatics.ca/gmeow/subjectVar";
const GM_OBJECT_VAR: &str = "https://blackcatinformatics.ca/gmeow/objectVar";
const GM_OBJECT_VALUE: &str = "https://blackcatinformatics.ca/gmeow/objectValue";
const GM_OPTIONAL_GROUP: &str = "https://blackcatinformatics.ca/gmeow/optionalGroup";
const GM_BIND_VAR: &str = "https://blackcatinformatics.ca/gmeow/bindVar";
const GM_BIND_EXPR: &str = "https://blackcatinformatics.ca/gmeow/bindExpr";
const GM_T_PRED: &str = "https://blackcatinformatics.ca/gmeow/tPred";
const GM_T_SUBJ: &str = "https://blackcatinformatics.ca/gmeow/tSubj";
const GM_T_OBJ: &str = "https://blackcatinformatics.ca/gmeow/tObj";

const GM_STATEMENT_METADATA: &str = "https://blackcatinformatics.ca/gmeow/StatementMetadata";
const GM_Q_SUBJECT: &str = "https://blackcatinformatics.ca/gmeow/qSubject";
const GM_Q_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/qPredicate";
const GM_Q_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/qObject";
const GM_Q_OBJECT_LITERAL: &str = "https://blackcatinformatics.ca/gmeow/qObjectLiteral";
const GM_ANNOTATION: &str = "https://blackcatinformatics.ca/gmeow/annotation";
const GM_ANN_PROPERTY: &str = "https://blackcatinformatics.ca/gmeow/annProperty";
const GM_ANN_VALUE: &str = "https://blackcatinformatics.ca/gmeow/annValue";
const GM_MAPPED_FROM: &str = "https://blackcatinformatics.ca/gmeow/mappedFrom";
const GM_AUTHORITY_LINK: &str = "https://blackcatinformatics.ca/gmeow/authorityLink";
const GM_TAG: &str = "https://blackcatinformatics.ca/gmeow/Tag";
const GM_HAS_TAG: &str = "https://blackcatinformatics.ca/gmeow/hasTag";

const GENID: &str = "https://blackcatinformatics.ca/gmeow/.well-known/genid/up-";

const ADOPTED_PREDICATES: &[&str] = &[SKOS_EXACT_MATCH, SKOS_CLOSE_MATCH];
const STATEMENT_METADATA_TERMS: &[&str] = &[
    GM_STATEMENT_METADATA,
    GM_Q_SUBJECT,
    GM_Q_PREDICATE,
    GM_Q_OBJECT,
    GM_Q_OBJECT_LITERAL,
];
const NORMALIZED_PREDICATES: &[(&str, &str)] =
    &[(SKOS_PREF_LABEL, RDFS_LABEL), (SKOS_ALT_LABEL, RDFS_LABEL)];
const CONCEPT_REFERENCE_PREDICATES: &[&str] = &[
    "https://schema.org/keywords",
    "https://schema.org/programmingLanguage",
    "http://usefulinc.com/ns/doap#programming-language",
    "http://usefulinc.com/ns/doap#category",
];

const PROJECTION_PREFIXES: &[&str] = &[
    "schema",
    "foaf",
    "doap",
    "vcard",
    "vcardx",
    "org",
    "time",
    "sioc",
    "bibo",
    "gedcom",
    "rel",
    "cc",
    "odrl",
    "dcterms",
    "dc",
    "spdx",
    "prov",
    "geo",
    "geosparql",
    "sosa",
    "skos",
    "ical",
    "oa",
    "iiif",
    "exif",
    "wgs84",
    "mads",
    "codemeta",
];

const PREFIXES: &[(&str, &str)] = &[
    ("gmeow", GM),
    ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
    ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
    ("owl", "http://www.w3.org/2002/07/owl#"),
    ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ("skos", SKOS),
    ("schema", "https://schema.org/"),
    ("foaf", "http://xmlns.com/foaf/0.1/"),
    ("doap", "http://usefulinc.com/ns/doap#"),
    ("vcard", "http://www.w3.org/2006/vcard/ns#"),
    ("vcardx", "http://www.w3.org/2006/vcard/ns#"),
    ("org", "http://www.w3.org/ns/org#"),
    ("time", "http://www.w3.org/2006/time#"),
    ("sioc", "http://rdfs.org/sioc/ns#"),
    ("bibo", "http://purl.org/ontology/bibo/"),
    ("bf", "http://id.loc.gov/ontologies/bibframe/"),
    ("bibframe", "http://id.loc.gov/ontologies/bibframe/"),
    ("gedcom", "http://www.w3.org/2000/10/swap/pim/gedcom#"),
    ("rel", "http://purl.org/vocab/relationship/"),
    ("cc", "http://creativecommons.org/ns#"),
    ("odrl", "http://www.w3.org/ns/odrl/2/"),
    ("dcterms", "http://purl.org/dc/terms/"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("spdx", "http://spdx.org/rdf/terms#"),
    ("prov", "http://www.w3.org/ns/prov#"),
    ("geo", "http://www.opengis.net/ont/geosparql#"),
    ("geosparql", "http://www.opengis.net/ont/geosparql#"),
    ("sosa", "http://www.w3.org/ns/sosa/"),
    ("ical", "http://www.w3.org/2002/12/cal/ical#"),
    ("oa", "http://www.w3.org/ns/oa#"),
    ("iiif", "http://iiif.io/api/presentation/3#"),
    ("exif", "http://www.w3.org/2003/12/exif/ns#"),
    ("wgs84", "http://www.w3.org/2003/01/geo/wgs84_pos#"),
    ("mads", "http://www.loc.gov/mads/rdf/v1#"),
    ("codemeta", "https://codemeta.github.io/terms/"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SssomClass {
    pub bucket: String,
    pub gmeow: String,
    pub target: String,
}

type TargetSetMap = BTreeMap<String, BTreeSet<String>>;
type TargetClaimMap = BTreeMap<String, BTreeMap<String, String>>;
type ValueRuleMap = BTreeMap<(String, String), (String, String)>;
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileBaseline {
    pub name: String,
    pub per_term: BTreeMap<String, String>,
    pub per_vocab: BTreeMap<String, BTreeMap<String, usize>>,
}

impl FileBaseline {
    pub fn liftable(&self) -> usize {
        self.per_term
            .values()
            .filter(|bucket| matches!(bucket.as_str(), "clean" | "liftable-with-claim"))
            .count()
    }

    pub fn total(&self) -> usize {
        self.per_term.len()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditReport {
    pub files: Vec<FileBaseline>,
    pub gaps: Vec<String>,
    pub sssom_total: usize,
    pub struct_total: usize,
}

impl AuditReport {
    pub fn liftable(&self) -> usize {
        self.files.iter().map(FileBaseline::liftable).sum()
    }

    pub fn total(&self) -> usize {
        self.files.iter().map(FileBaseline::total).sum()
    }
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

pub fn classify_sssom(subj: &str, pred: &str, obj: &str) -> SssomClass {
    let is_gmeow_ref = |term: &str| term.starts_with("gmeow:") || term.starts_with(GM);
    let (gmeow, target, gmeow_is_subject) = if is_gmeow_ref(subj) && !is_gmeow_ref(obj) {
        (subj, obj, true)
    } else if is_gmeow_ref(obj) && !is_gmeow_ref(subj) {
        (obj, subj, false)
    } else {
        return SssomClass {
            bucket: "both-or-neither-gmeow".to_owned(),
            gmeow: subj.to_owned(),
            target: obj.to_owned(),
        };
    };
    let rel = pred.rsplit([':', '#', '/']).next().unwrap_or(pred);
    let bucket = match rel {
        "exactMatch" | "equivalentClass" | "equivalentProperty" | "sameAs" => "clean-reversible",
        "closeMatch" => "liftable-with-claim",
        "broadMatch" | "narrowMatch" => {
            let gmeow_is_broader = gmeow_is_subject == (rel == "broadMatch");
            if gmeow_is_broader {
                "liftable-generalizing"
            } else {
                "down-only-narrowing"
            }
        }
        "relatedMatch" | "subClassOf" => "down-only-related",
        _ => {
            return SssomClass {
                bucket: format!("other:{rel}"),
                gmeow: gmeow.to_owned(),
                target: target.to_owned(),
            }
        }
    };
    SssomClass {
        bucket: bucket.to_owned(),
        gmeow: gmeow.to_owned(),
        target: target.to_owned(),
    }
}

pub fn combined_class(
    term: &str,
    sssom: &BTreeMap<String, String>,
    structural: &BTreeMap<String, String>,
) -> String {
    let s = sssom.get(term).map(String::as_str);
    let t = structural.get(term).map(String::as_str);
    if s == Some("clean-reversible") || t == Some("simple-1to1") {
        "clean".to_owned()
    } else if s.is_some_and(sssom_liftable) || t.is_some_and(struct_liftable) {
        "liftable-with-claim".to_owned()
    } else if t == Some("structural-mint") {
        "hard-mint".to_owned()
    } else if matches!(s, Some("down-only-related" | "down-only-narrowing")) {
        "down-only".to_owned()
    } else {
        "GAP".to_owned()
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

pub fn run_audit_nt(
    sssom_texts: &[String],
    projection_ttls: &[String],
    corpus_nts: &[(String, String)],
) -> Result<AuditReport, String> {
    let sssom = sssom_best_buckets(sssom_texts)?;
    let structural = structural_best_classes(projection_ttls)?;
    let mut files = Vec::new();
    let mut gaps = BTreeSet::new();
    for (name, nt) in corpus_nts {
        let store = Graph::parse(nt.as_bytes(), "application/n-triples")?;
        let mut per_term = BTreeMap::new();
        let mut per_vocab: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for iri in used_target_terms(&store.quads) {
            let class = combined_class(&iri, &sssom, &structural);
            let term = canon_qname(&iri);
            *per_vocab
                .entry(prefix(&term).to_owned())
                .or_default()
                .entry(class.clone())
                .or_insert(0) += 1;
            if class == "GAP" {
                gaps.insert(term.clone());
            }
            per_term.insert(term, class);
        }
        files.push(FileBaseline {
            name: name.clone(),
            per_term,
            per_vocab,
        });
    }
    Ok(AuditReport {
        files,
        gaps: gaps.into_iter().collect(),
        sssom_total: sssom.len(),
        struct_total: structural.len(),
    })
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

fn sssom_records(
    sssom_texts: &[String],
) -> Result<Vec<(gmeow_rdf::SssomMapping, PrefixMap)>, String> {
    let mut rows = Vec::new();
    for text in sssom_texts {
        let set = gmeow_rdf::sssom::parse_tsv(text).map_err(|e| e.to_string())?;
        let prefixes = PrefixMap::from_sssom(&set.meta.curie_map);
        for row in set.mappings {
            rows.push((row, prefixes.clone()));
        }
    }
    Ok(rows)
}

fn sssom_clean_pairs(sssom_texts: &[String]) -> Result<TargetSetMap, String> {
    let mut pairs: TargetSetMap = BTreeMap::new();
    for (row, prefixes) in sssom_records(sssom_texts)? {
        let class = classify_sssom(&row.subject_id, &row.predicate_id, &row.object_id);
        if class.bucket != "clean-reversible" {
            continue;
        }
        let target = prefixes.to_iri(&class.target);
        if in_projection_ns(&target) {
            pairs
                .entry(target)
                .or_default()
                .insert(prefixes.to_iri(&class.gmeow));
        }
    }
    Ok(pairs)
}

fn sssom_closematch_pairs(sssom_texts: &[String]) -> Result<TargetClaimMap, String> {
    let mut pairs: TargetClaimMap = BTreeMap::new();
    for (row, prefixes) in sssom_records(sssom_texts)? {
        let class = classify_sssom(&row.subject_id, &row.predicate_id, &row.object_id);
        if class.bucket != "liftable-with-claim" {
            continue;
        }
        let Some(conf_raw) = row.confidence.map(confidence_lexeme) else {
            continue;
        };
        if decimal_confidence(&conf_raw).is_none() {
            continue;
        }
        let target = prefixes.to_iri(&class.target);
        if !in_projection_ns(&target) {
            continue;
        }
        let gmeow = prefixes.to_iri(&class.gmeow);
        let bucket = pairs.entry(target).or_default();
        let replace = bucket
            .get(&gmeow)
            .is_none_or(|prev| decimal_confidence(&conf_raw) > decimal_confidence(prev));
        if replace {
            bucket.insert(gmeow, conf_raw);
        }
    }
    Ok(pairs)
}

fn sssom_best_buckets(sssom_texts: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut best: BTreeMap<String, String> = BTreeMap::new();
    for (row, prefixes) in sssom_records(sssom_texts)? {
        let class = classify_sssom(&row.subject_id, &row.predicate_id, &row.object_id);
        if class.bucket == "both-or-neither-gmeow" {
            continue;
        }
        let target = prefixes.to_iri(&class.target);
        if !in_projection_ns(&target) {
            continue;
        }
        let replace = best
            .get(&target)
            .is_none_or(|cur| sssom_rank(&class.bucket) > sssom_rank(cur));
        if replace {
            best.insert(target, class.bucket);
        }
    }
    Ok(best)
}

fn structural_pairs(projection_ttls: &[String]) -> Result<(TargetSetMap, TargetClaimMap), String> {
    let mut exact: TargetSetMap = BTreeMap::new();
    let mut generalizing: TargetClaimMap = BTreeMap::new();
    for ttl in projection_ttls {
        let graph = Graph::parse(ttl.as_bytes(), "text/turtle")?;
        let q = &graph.quads;
        for cell in subjects(q, RDF_TYPE, GM_PROJECTION_MAPPING) {
            let Some(pattern) = value(q, &cell, GM_HAS_MAPPING_PATTERN) else {
                continue;
            };
            if has_object(q, &pattern, GM_MINT)
                || has_object(q, &pattern, GM_PATH)
                || has_object(q, &pattern, GM_FILTER)
            {
                continue;
            }
            let Some(src) = value_named(q, &pattern, GM_EDOAL_SOURCE) else {
                continue;
            };
            for binding in objects(q, &cell, GM_HAS_BINDING) {
                if template_atoms(q, &binding).len() > 1 {
                    continue;
                }
                let target = value_named(q, &binding, GM_TO_PREDICATE).or(value_named(
                    q,
                    &binding,
                    GM_TO_CLASS,
                ));
                let Some(tgt) = target else {
                    continue;
                };
                if !in_projection_ns(&tgt) {
                    continue;
                }
                let rel = value_lexical(q, &binding, GM_RELATION).unwrap_or_default();
                if rel == "=" {
                    exact.entry(tgt).or_default().insert(src.clone());
                } else if rel == "<=" {
                    let cur = value_lexical(q, &binding, GM_CONFIDENCE)
                        .and_then(|c| decimal_confidence(&c).map(|_| c))
                        .unwrap_or_default();
                    let bucket = generalizing.entry(tgt).or_default();
                    let replace = bucket.get(&src).is_none_or(|prev| {
                        !cur.is_empty()
                            && (prev.is_empty()
                                || decimal_confidence(&cur) > decimal_confidence(prev))
                    });
                    if replace {
                        bucket.insert(src.clone(), cur);
                    }
                }
            }
        }
    }
    Ok((exact, generalizing))
}

fn value_mapped_pairs(projection_ttls: &[String]) -> Result<ValueRuleMap, String> {
    let mut candidates: BTreeMap<(String, String), BTreeSet<(String, String)>> = BTreeMap::new();
    for ttl in projection_ttls {
        let graph = Graph::parse(ttl.as_bytes(), "text/turtle")?;
        let q = &graph.quads;
        for cell in subjects(q, RDF_TYPE, GM_PROJECTION_MAPPING) {
            let Some(pattern) = value(q, &cell, GM_HAS_MAPPING_PATTERN) else {
                continue;
            };
            let Some(anchor) = value(q, &pattern, GM_ANCHOR) else {
                continue;
            };
            let atoms = rdf_list(q, value(q, &pattern, GM_ATOM).as_ref());
            if atoms.len() != 1 {
                continue;
            }
            let atom = &atoms[0];
            let gmeow_pred = value_named(q, atom, GM_PREDICATE);
            let gmeow_val = value_named(q, atom, GM_OBJECT_VALUE);
            if value(q, atom, GM_SUBJECT_VAR) != Some(anchor.clone())
                || gmeow_pred.as_ref().is_none_or(|p| !p.starts_with(GM))
                || gmeow_val.is_none()
            {
                continue;
            }
            let mints = objects(q, &pattern, GM_MINT);
            if mints.len() != 1 {
                continue;
            }
            let mint = &mints[0];
            let Some(bind_var) = value(q, mint, GM_BIND_VAR) else {
                continue;
            };
            let Some(bind_expr) = value(q, mint, GM_BIND_EXPR) else {
                continue;
            };
            let RdfTerm::Literal(bind_literal) = bind_expr else {
                continue;
            };
            for binding in objects(q, &cell, GM_HAS_BINDING) {
                let tas = template_atoms(q, &binding);
                if tas.len() != 1 {
                    continue;
                }
                let ta = &tas[0];
                let tpred = value_named(q, ta, GM_T_PRED);
                if value(q, ta, GM_T_SUBJ) == Some(anchor.clone())
                    && value(q, ta, GM_T_OBJ) == Some(bind_var.clone())
                    && tpred.as_ref().is_some_and(|p| in_projection_ns(p))
                {
                    candidates
                        .entry((tpred.expect("checked"), bind_literal.lexical_form.clone()))
                        .or_default()
                        .insert((
                            gmeow_pred.clone().expect("checked"),
                            gmeow_val.clone().expect("checked"),
                        ));
                }
            }
        }
    }
    Ok(candidates
        .into_iter()
        .filter_map(|(key, values)| {
            (values.len() == 1).then(|| (key, values.into_iter().next().expect("one value rule")))
        })
        .collect())
}

fn edoalpath_pairs(projection_ttls: &[String]) -> Result<(TargetSetMap, TargetSetMap), String> {
    let mut direct: TargetSetMap = BTreeMap::new();
    let mut inverse: TargetSetMap = BTreeMap::new();
    for ttl in projection_ttls {
        let graph = Graph::parse(ttl.as_bytes(), "text/turtle")?;
        let q = &graph.quads;
        for cell in subjects(q, RDF_TYPE, GM_PROJECTION_MAPPING) {
            let Some(pattern) = value(q, &cell, GM_HAS_MAPPING_PATTERN) else {
                continue;
            };
            if !has_object(q, &pattern, GM_EDOAL_PATH) || has_object(q, &pattern, GM_MINT) {
                continue;
            }
            let atoms = rdf_list(q, value(q, &pattern, GM_ATOM).as_ref());
            if atoms.len() != 1 {
                continue;
            }
            let Some(apred) = value_named(q, &atoms[0], GM_PREDICATE) else {
                continue;
            };
            let Some(anchor) = value(q, &pattern, GM_ANCHOR) else {
                continue;
            };
            let subjvar = value(q, &atoms[0], GM_SUBJECT_VAR);
            let objvar = value(q, &atoms[0], GM_OBJECT_VAR);
            let bucket = if subjvar.as_ref() == Some(&anchor) {
                &mut direct
            } else if objvar.as_ref() == Some(&anchor) {
                &mut inverse
            } else {
                continue;
            };
            for binding in objects(q, &cell, GM_HAS_BINDING) {
                if let Some(tgt) = value_named(q, &binding, GM_TO_PREDICATE) {
                    if in_projection_ns(&tgt) {
                        bucket.entry(tgt).or_default().insert(apred.clone());
                    }
                }
            }
        }
    }
    Ok((direct, inverse))
}

fn multiatom_pairs(
    projection_ttls: &[String],
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut pairs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for ttl in projection_ttls {
        let graph = Graph::parse(ttl.as_bytes(), "text/turtle")?;
        let q = &graph.quads;
        for cell in subjects(q, RDF_TYPE, GM_PROJECTION_MAPPING) {
            let Some(pattern) = value(q, &cell, GM_HAS_MAPPING_PATTERN) else {
                continue;
            };
            let mut obj_source = BTreeMap::new();
            for atom in pattern_atoms(q, value(q, &pattern, GM_ATOM).as_ref(), &mut HashSet::new())
            {
                let objvar = value(q, &atom, GM_OBJECT_VAR);
                let pred = value_named(q, &atom, GM_PREDICATE);
                if let (Some(objvar), Some(pred)) = (objvar, pred) {
                    if pred.starts_with(GM) {
                        obj_source.insert(term_key(&objvar), pred);
                    }
                }
            }
            for binding in objects(q, &cell, GM_HAS_BINDING) {
                for tmpl in objects(q, &binding, GM_TEMPLATE_ATOMS) {
                    for tatom in rdf_list(q, Some(&tmpl)) {
                        let tpred = value_named(q, &tatom, GM_T_PRED);
                        let tobj = value(q, &tatom, GM_T_OBJ);
                        let Some(tpred) = tpred else {
                            continue;
                        };
                        if !in_projection_ns(&tpred) {
                            continue;
                        }
                        if let Some(source) =
                            tobj.as_ref().and_then(|t| obj_source.get(&term_key(t)))
                        {
                            pairs.entry(tpred).or_default().insert(source.clone());
                        }
                    }
                }
            }
        }
    }
    Ok(pairs)
}

fn structural_best_classes(projection_ttls: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut best: BTreeMap<String, String> = BTreeMap::new();
    for ttl in projection_ttls {
        let graph = Graph::parse(ttl.as_bytes(), "text/turtle")?;
        let q = &graph.quads;
        for cell in subjects(q, RDF_TYPE, GM_PROJECTION_MAPPING) {
            let Some(pattern) = value(q, &cell, GM_HAS_MAPPING_PATTERN) else {
                continue;
            };
            let has_mint = has_object(q, &pattern, GM_MINT);
            let has_guard = has_object(q, &pattern, GM_PATH) || has_object(q, &pattern, GM_FILTER);
            for binding in objects(q, &cell, GM_HAS_BINDING) {
                let targets = emitted_targets(q, &binding);
                let atoms = template_atoms(q, &binding);
                let cls = if has_mint {
                    "structural-mint"
                } else if has_guard {
                    "structural-guarded"
                } else if atoms.len() > 1 {
                    "structural-multileg"
                } else {
                    "simple-1to1"
                };
                for target in targets {
                    let replace = best
                        .get(&target)
                        .is_none_or(|cur| struct_rank(cls) > struct_rank(cur));
                    if replace {
                        best.insert(target, cls.to_owned());
                    }
                }
            }
        }
    }
    Ok(best)
}

fn emitted_targets(quads: &[RdfQuad], binding: &RdfTerm) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    for pred in [
        GM_TO_CLASS,
        GM_TO_PREDICATE,
        "https://blackcatinformatics.ca/gmeow/edoalTarget",
    ] {
        for obj in objects(quads, binding, pred) {
            if let RdfTerm::Iri(node) = obj {
                if in_projection_ns(&node) {
                    targets.insert(node);
                }
            }
        }
    }
    for atom in template_atoms(quads, binding) {
        for pred in [GM_T_PRED, "https://blackcatinformatics.ca/gmeow/tObjValue"] {
            for obj in objects(quads, &atom, pred) {
                if let RdfTerm::Iri(node) = obj {
                    if in_projection_ns(&node) {
                        targets.insert(node);
                    }
                }
            }
        }
    }
    targets
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

fn ancestor_closure(quads: &[RdfQuad]) -> BTreeMap<String, BTreeSet<String>> {
    let mut direct: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for q in quads.iter().filter(|q| q.predicate == RDFS_SUB_CLASS_OF) {
        if let (RdfTerm::Iri(sub), RdfTerm::Iri(obj)) = (&q.subject, &q.object) {
            direct.entry(sub.clone()).or_default().insert(obj.clone());
        }
    }
    let classes: BTreeSet<String> = direct
        .keys()
        .cloned()
        .chain(direct.values().flat_map(|v| v.iter().cloned()))
        .collect();
    let mut closure = BTreeMap::new();
    for cls in classes {
        let mut seen = BTreeSet::new();
        walk_ancestors(&cls, &direct, &mut seen);
        closure.insert(cls, seen);
    }
    closure
}

fn walk_ancestors(
    cls: &str,
    direct: &BTreeMap<String, BTreeSet<String>>,
    seen: &mut BTreeSet<String>,
) {
    if !seen.insert(cls.to_owned()) {
        return;
    }
    if let Some(parents) = direct.get(cls) {
        for parent in parents {
            walk_ancestors(parent, direct, seen);
        }
    }
}

fn domain(quads: &[RdfQuad], iri: &str) -> Option<String> {
    let s = RdfTerm::iri(iri);
    let domains = objects(quads, &s, RDFS_DOMAIN)
        .into_iter()
        .filter_map(|term| match term {
            RdfTerm::Iri(node) => Some(node),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    (domains.len() == 1).then(|| domains.into_iter().next().expect("one domain"))
}

fn object_properties(ontology_nt: &str) -> Result<BTreeSet<String>, String> {
    let graph = Graph::parse(ontology_nt.as_bytes(), "application/n-triples")?;
    let mut props = BTreeSet::new();
    for q in &graph.quads {
        if q.predicate == RDF_TYPE
            && matches!(&q.object, RdfTerm::Iri(n) if n == OWL_OBJECT_PROPERTY)
        {
            if let RdfTerm::Iri(s) = &q.subject {
                props.insert(s.clone());
            }
        }
    }
    Ok(props)
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

fn used_target_terms(quads: &[RdfQuad]) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    for triple in quads {
        if in_projection_ns(&triple.predicate) {
            terms.insert(triple.predicate.clone());
        }
        if triple.predicate == RDF_TYPE {
            if let RdfTerm::Iri(node) = &triple.object {
                if in_projection_ns(node) {
                    terms.insert(node.clone());
                }
            }
        }
    }
    terms
}

/// Serialize the accumulator to N-Triples via the native writer.
fn dump_nt(quads: &[RdfQuad]) -> Result<String, String> {
    let dataset = gmeow_rdf::native_quads::flat_dataset_from_quads(quads)
        .map_err(|e| format!("re-freeze accumulated quads: {e}"))?;
    let bytes = gmeow_rdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        gmeow_rdf::SerializeGraph::Dataset,
    )
    .map_err(|e| format!("N-Triples serialization failed: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("N-Triples output is not UTF-8: {e}"))
}

/// Subjects of `(?, pred, <obj>)`, as `RdfTerm`.
fn subjects(quads: &[RdfQuad], pred: &str, obj: &str) -> Vec<RdfTerm> {
    quads
        .iter()
        .filter(|q| q.predicate == pred && matches!(&q.object, RdfTerm::Iri(n) if n == obj))
        .map(|q| q.subject.clone())
        .collect()
}

/// All objects of `(subject, pred, ?)`.
fn objects(quads: &[RdfQuad], subject: &RdfTerm, pred: &str) -> Vec<RdfTerm> {
    if !subject_addressable(subject) {
        return Vec::new();
    }
    quads
        .iter()
        .filter(|q| q.predicate == pred && term_key(&q.subject) == term_key(subject))
        .map(|q| q.object.clone())
        .collect()
}

fn value(quads: &[RdfQuad], subject: &RdfTerm, pred: &str) -> Option<RdfTerm> {
    objects(quads, subject, pred).into_iter().next()
}

/// The first object that is an IRI, returned as its IRI string.
fn value_named(quads: &[RdfQuad], subject: &RdfTerm, pred: &str) -> Option<String> {
    value(quads, subject, pred).and_then(|term| match term {
        RdfTerm::Iri(node) => Some(node),
        _ => None,
    })
}

fn value_lexical(quads: &[RdfQuad], subject: &RdfTerm, pred: &str) -> Option<String> {
    value(quads, subject, pred).map(|term| match term {
        RdfTerm::Iri(node) => node,
        RdfTerm::BlankNode(node) => node,
        RdfTerm::Literal(lit) => lit.lexical_form,
        RdfTerm::Triple(_) => String::new(),
    })
}

fn has_object(quads: &[RdfQuad], subject: &RdfTerm, pred: &str) -> bool {
    !objects(quads, subject, pred).is_empty()
}

fn template_atoms(quads: &[RdfQuad], binding: &RdfTerm) -> Vec<RdfTerm> {
    let mut out = Vec::new();
    for ta in objects(quads, binding, GM_TEMPLATE_ATOMS) {
        out.extend(rdf_list(quads, Some(&ta)));
    }
    out
}

fn pattern_atoms(
    quads: &[RdfQuad],
    node: Option<&RdfTerm>,
    seen: &mut HashSet<String>,
) -> Vec<RdfTerm> {
    let mut out = Vec::new();
    for atom in rdf_list(quads, node) {
        let key = term_key(&atom);
        if !seen.insert(key) {
            continue;
        }
        if let Some(group) = value(quads, &atom, GM_OPTIONAL_GROUP) {
            out.extend(pattern_atoms(quads, Some(&group), seen));
        } else {
            out.push(atom);
        }
    }
    out
}

fn rdf_list(quads: &[RdfQuad], node: Option<&RdfTerm>) -> Vec<RdfTerm> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let nil = RdfTerm::Iri(RDF_NIL.to_owned());
    let mut node = node.cloned();
    while let Some(cur) = node {
        if cur == nil || !seen.insert(term_key(&cur)) {
            break;
        }
        if let Some(first) = value(quads, &cur, RDF_FIRST) {
            out.push(first);
        }
        node = value(quads, &cur, RDF_REST);
        if let Some(rest) = &node {
            if rest == &nil {
                break;
            }
        }
    }
    out
}

/// `true` iff `term` can stand as a subject (an IRI or blank node).
fn subject_addressable(term: &RdfTerm) -> bool {
    matches!(term, RdfTerm::Iri(_) | RdfTerm::BlankNode(_))
}

fn is_gmeow_ns(iri: &str) -> bool {
    iri.starts_with(GM)
}

fn in_projection_ns(iri: &str) -> bool {
    projection_namespaces().iter().any(|ns| iri.starts_with(ns))
}

fn projection_namespaces() -> &'static [&'static str] {
    static NAMESPACES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMESPACES.get_or_init(|| {
        let prefixes: HashSet<&str> = PROJECTION_PREFIXES.iter().copied().collect();
        PREFIXES
            .iter()
            .filter_map(|(pfx, ns)| prefixes.contains(pfx).then_some(*ns))
            .collect()
    })
}

fn canon_qname(iri: &str) -> String {
    static SORTED_PREFIXES: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    let prefixes = SORTED_PREFIXES.get_or_init(|| {
        let mut prefixes = PREFIXES.to_vec();
        prefixes.sort_by_key(|(_pfx, ns)| std::cmp::Reverse(ns.len()));
        prefixes
    });
    for (pfx, ns) in prefixes {
        if let Some(local) = iri.strip_prefix(ns) {
            return format!("{pfx}:{local}");
        }
    }
    iri.to_owned()
}

fn prefix(term: &str) -> &str {
    term.split_once(':').map_or("", |(pfx, _)| pfx)
}

fn normalize_label(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn decimal_confidence(conf: &str) -> Option<f64> {
    if conf.is_empty() || conf.contains('e') || conf.contains('E') {
        return None;
    }
    let value: f64 = conf.parse().ok()?;
    value
        .is_finite()
        .then_some(value)
        .filter(|v| (0.0..=1.0).contains(v))
}

fn confidence_lexeme(value: f64) -> String {
    let mut s = value.to_string();
    if s.ends_with(".0") {
        s.truncate(s.len() - 2);
    }
    s
}

fn sssom_rank(bucket: &str) -> usize {
    match bucket {
        "clean-reversible" => 3,
        "liftable-generalizing" => 2,
        "liftable-with-claim" => 1,
        _ => 0,
    }
}

fn struct_rank(bucket: &str) -> usize {
    match bucket {
        "simple-1to1" => 3,
        "structural-guarded" | "structural-multileg" => 2,
        "structural-mint" => 1,
        _ => 0,
    }
}

fn sssom_liftable(bucket: &str) -> bool {
    matches!(
        bucket,
        "clean-reversible" | "liftable-generalizing" | "liftable-with-claim"
    )
}

fn struct_liftable(bucket: &str) -> bool {
    matches!(
        bucket,
        "simple-1to1" | "structural-guarded" | "structural-multileg"
    )
}

/// A parsed RDF graph: the frozen dataset (for SPARQL) paired with its flat
/// default-graph quad stream (collected once for the many linear-scan reads).
struct Graph {
    dataset: Arc<RdfDataset>,
    quads: Vec<RdfQuad>,
}

impl Graph {
    fn parse(data: &[u8], media_type: &str) -> Result<Self, String> {
        let dataset = gmeow_rdf::parse_dataset(data, media_type, None)
            .map_err(|e| format!("RDF parse failed: {e}"))?;
        let quads = gmeow_rdf::native_quads::flat_rdf_quads_from_dataset(&dataset)
            .into_iter()
            .filter(|q| q.graph_name.is_none())
            .collect();
        Ok(Self { dataset, quads })
    }

    fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }
}

#[derive(Debug, Clone)]
struct PrefixMap {
    prefixes: BTreeMap<String, String>,
}

impl PrefixMap {
    fn from_sssom(curie_map: &BTreeMap<String, String>) -> Self {
        let mut prefixes: BTreeMap<String, String> = PREFIXES
            .iter()
            .map(|(pfx, ns)| ((*pfx).to_owned(), (*ns).to_owned()))
            .collect();
        for (pfx, ns) in curie_map {
            prefixes.insert(pfx.trim_end_matches(':').to_owned(), ns.clone());
        }
        Self { prefixes }
    }

    fn to_iri(&self, curie: &str) -> String {
        if curie.starts_with("http://") || curie.starts_with("https://") {
            return curie.to_owned();
        }
        let Some((pfx, local)) = curie.split_once(':') else {
            return curie.to_owned();
        };
        self.prefixes
            .get(pfx)
            .map_or_else(|| curie.to_owned(), |ns| format!("{ns}{local}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct TermKey(String);

impl TermKey {
    fn from_subject(subject: &RdfTerm) -> Self {
        Self(term_key(subject))
    }
}

fn term_key(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(node) => format!("<{node}>"),
        RdfTerm::BlankNode(node) => format!("_:{node}"),
        RdfTerm::Literal(lit) => format!("\"{}\"", lit.lexical_form),
        RdfTerm::Triple(_) => "<<triple>>".to_owned(),
    }
}

/// A full term-identity key for accumulator deduplication: unlike [`term_key`]
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
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn sssom_relation_buckets_match_python_contract() {
        assert_eq!(
            classify_sssom("gmeow:Person", "skos:exactMatch", "foaf:Person").bucket,
            "clean-reversible"
        );
        assert_eq!(
            classify_sssom("foaf:Agent", "skos:exactMatch", "gmeow:Agent").bucket,
            "clean-reversible"
        );
        assert_eq!(
            classify_sssom("gmeow:noteContent", "skos:closeMatch", "schema:text").bucket,
            "liftable-with-claim"
        );
        assert_eq!(
            classify_sssom("gmeow:Appellation", "skos:broadMatch", "schema:name").bucket,
            "liftable-generalizing"
        );
        assert_eq!(
            classify_sssom("gmeow:X", "skos:narrowMatch", "schema:Y").bucket,
            "down-only-narrowing"
        );
        assert_eq!(
            classify_sssom(
                &format!("{GM}Person"),
                SKOS_EXACT_MATCH,
                "http://xmlns.com/foaf/0.1/Person"
            )
            .bucket,
            "clean-reversible"
        );
        assert_eq!(
            classify_sssom(
                "https://schema.org/text",
                SKOS_CLOSE_MATCH,
                &format!("{GM}noteContent")
            )
            .bucket,
            "liftable-with-claim"
        );
    }

    #[test]
    fn combined_class_prefers_best_layer() {
        assert_eq!(
            combined_class(
                "x",
                &BTreeMap::from([("x".into(), "clean-reversible".into())]),
                &BTreeMap::new()
            ),
            "clean"
        );
        assert_eq!(
            combined_class(
                "x",
                &BTreeMap::new(),
                &BTreeMap::from([("x".into(), "structural-mint".into())])
            ),
            "hard-mint"
        );
    }

    #[test]
    fn decimal_confidence_rejects_exponents_and_out_of_range_values() {
        assert_eq!(decimal_confidence("0.9"), Some(0.9));
        for bad in ["1e-1", "NaN", "Infinity", "-0.1", "1.5", "abc", ""] {
            assert!(decimal_confidence(bad).is_none(), "{bad}");
        }
    }

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
