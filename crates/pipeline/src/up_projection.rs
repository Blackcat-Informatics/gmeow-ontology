// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native up-projection kernel: consumer RDF -> GMEOW.
//!
//! This module is the Rust authority for the historical `gmeow_tools.up_projection`
//! family. Python remains an interface layer: it supplies serialized RDF and the
//! same repo-or-bundle mapping/cell inputs the public CLI already used.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::OnceLock;

use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer};
use oxigraph::model::{
    BlankNode, GraphNameRef, Literal, NamedNode, NamedOrBlankNode, Quad, Term, Triple,
};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

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
    out: Store,
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
            out: Store::new().map_err(|e| format!("store creation failed: {e}"))?,
            lifted: 0,
            claimed: 0,
            gaps: BTreeMap::new(),
            ambiguous: BTreeMap::new(),
            claims: BTreeMap::new(),
            next_blank: 0,
        })
    }

    fn fact(&mut self, s: NamedOrBlankNode, p: NamedNode, o: Term) -> Result<(), String> {
        insert_triple(&self.out, s, p, o)?;
        self.lifted += 1;
        Ok(())
    }

    fn claim(
        &mut self,
        s: NamedNode,
        p: NamedNode,
        o: Term,
        source_term: NamedNode,
        conf: &str,
    ) -> Result<(), String> {
        let source_key = canon_qname(source_term.as_str());
        emit_claim(self, s, p, o, source_term, conf)?;
        self.claimed += 1;
        *self.claims.entry(source_key).or_insert(0) += 1;
        Ok(())
    }

    fn fresh_blank(&mut self, prefix: &str) -> Result<BlankNode, String> {
        self.next_blank += 1;
        BlankNode::new(format!("{prefix}{}", self.next_blank))
            .map_err(|e| format!("blank node mint failed: {e}"))
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
    let source = parse_store(source_nt.as_bytes(), RdfFormat::NTriples)?;
    if source
        .len()
        .map_err(|e| format!("store length failed: {e}"))?
        == 0
    {
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
        let store = parse_store(nt.as_bytes(), RdfFormat::NTriples)?;
        let mut per_term = BTreeMap::new();
        let mut per_vocab: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for iri in used_target_terms(&store)? {
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
    let source = parse_store(source_nt.as_bytes(), RdfFormat::NTriples)?;
    let mut acc = Acc::new()?;
    apply_reverse(&source, &mut acc)?;
    dump_nt(&acc.out)
}

fn up_project_store(source: &Store, lift: &LiftMap) -> Result<UpProjectionReport, String> {
    let mut acc = Acc::new()?;
    for triple in triples(source)? {
        lift_edge(
            &mut acc,
            triple.subject.clone(),
            triple.predicate.clone(),
            triple.object.clone(),
            lift,
        )?;
    }
    let minted = apply_reverse(source, &mut acc)?;
    let tag_terms = resolve_concept_references(source, &mut acc)?;
    finish_report(acc, 0, BTreeMap::new(), tag_terms, minted)
}

fn up_project_descend_store(
    source: &Store,
    lift: &LiftMap,
    sssom_texts: &[String],
    projection_ttls: &[String],
    ontology_nt: &str,
) -> Result<UpProjectionReport, String> {
    let ctx = build_context(sssom_texts, projection_ttls, ontology_nt)?;
    let mut node_types: BTreeMap<TermKey, BTreeSet<String>> = BTreeMap::new();
    let rdf_type = named(RDF_TYPE)?;
    for triple in triples(source)? {
        if triple.predicate != rdf_type {
            continue;
        }
        let Term::NamedNode(t) = triple.object else {
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
    for triple in triples(source)? {
        let key = triple.predicate.as_str().to_owned();
        if triple.predicate == rdf_type
            || lift.rules.contains_key(&key)
            || lift.inverse_rules.contains_key(&key)
        {
            lift_edge(
                &mut acc,
                triple.subject.clone(),
                triple.predicate.clone(),
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
                triple.predicate.clone(),
                triple.object.clone(),
                lift,
            )?;
            continue;
        };
        if cand.relation == "=" {
            acc.fact(
                triple.subject.clone(),
                named(&cand.gmeow)?,
                triple.object.clone(),
            )?;
        } else if let NamedOrBlankNode::NamedNode(s) = triple.subject.clone() {
            if matches!(triple.object, Term::NamedNode(_) | Term::Literal(_)) {
                acc.claim(
                    s,
                    named(&cand.gmeow)?,
                    triple.object.clone(),
                    triple.predicate.clone(),
                    &cand.confidence,
                )?;
            } else {
                lift_edge(
                    &mut acc,
                    triple.subject.clone(),
                    triple.predicate.clone(),
                    triple.object.clone(),
                    lift,
                )?;
                continue;
            }
        } else {
            lift_edge(
                &mut acc,
                triple.subject.clone(),
                triple.predicate.clone(),
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

fn lift_edge(
    acc: &mut Acc,
    s: NamedOrBlankNode,
    p: NamedNode,
    o: Term,
    lift: &LiftMap,
) -> Result<(), String> {
    let rdf_type = named(RDF_TYPE)?;
    if p == rdf_type {
        if let Term::NamedNode(class) = o {
            let key = class.as_str();
            if let Some(target) = lift.rules.get(key) {
                acc.fact(s, rdf_type, Term::NamedNode(named(target)?))?;
            } else if let Some((gmeow, conf)) = lift.claim_rules.get(key) {
                if let NamedOrBlankNode::NamedNode(subj) = s {
                    acc.claim(
                        subj,
                        named(RDF_TYPE)?,
                        Term::NamedNode(named(gmeow)?),
                        class,
                        conf,
                    )?;
                }
            } else if is_gmeow_ns(key) {
                acc.fact(s, named(RDF_TYPE)?, Term::NamedNode(class))?;
            } else {
                account(acc, lift, key);
            }
        }
        return Ok(());
    }

    let key = p.as_str();
    if let Term::Literal(lit) = &o {
        if let Some((gpred, gval)) = lift
            .value_rules
            .get(&(key.to_owned(), lit.value().to_owned()))
        {
            acc.fact(s, named(gpred)?, Term::NamedNode(named(gval)?))?;
            return Ok(());
        }
    }
    if let Some(target) = lift.rules.get(key) {
        if matches!(o, Term::Literal(_)) && lift.object_properties.contains(target) {
            if let NamedOrBlankNode::NamedNode(subj) = s {
                acc.claim(subj, named(target)?, o, p, "")?;
            }
            return Ok(());
        }
        acc.fact(s, named(target)?, o)?;
    } else if let Some(target) = lift.inverse_rules.get(key) {
        match o {
            Term::NamedNode(node) => {
                acc.fact(
                    NamedOrBlankNode::NamedNode(node),
                    named(target)?,
                    subject_to_term(s),
                )?;
            }
            Term::BlankNode(node) => {
                acc.fact(
                    NamedOrBlankNode::BlankNode(node),
                    named(target)?,
                    subject_to_term(s),
                )?;
            }
            Term::Literal(_) | Term::Triple(_) => {}
        }
    } else if let Some((gmeow, conf)) = lift.claim_rules.get(key) {
        if let NamedOrBlankNode::NamedNode(subj) = s {
            if matches!(o, Term::NamedNode(_) | Term::Literal(_)) {
                acc.claim(subj, named(gmeow)?, o, p, conf)?;
            }
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
    subj: NamedNode,
    qpred: NamedNode,
    qobj: Term,
    source_term: NamedNode,
    conf: &str,
) -> Result<(), String> {
    let cell = NamedOrBlankNode::BlankNode(acc.fresh_blank("up-claim-")?);
    insert_triple(
        &acc.out,
        cell.clone(),
        named(RDF_TYPE)?,
        Term::NamedNode(named(GM_STATEMENT_METADATA)?),
    )?;
    insert_triple(
        &acc.out,
        cell.clone(),
        named(GM_Q_SUBJECT)?,
        Term::NamedNode(subj),
    )?;
    insert_triple(
        &acc.out,
        cell.clone(),
        named(GM_Q_PREDICATE)?,
        Term::NamedNode(qpred),
    )?;
    let qobj_pred = if matches!(qobj, Term::Literal(_)) {
        GM_Q_OBJECT_LITERAL
    } else {
        GM_Q_OBJECT
    };
    insert_triple(&acc.out, cell.clone(), named(qobj_pred)?, qobj)?;
    emit_annotation(
        acc,
        cell.clone(),
        GM_MAPPED_FROM,
        Term::NamedNode(source_term),
    )?;
    if !conf.is_empty() {
        emit_annotation(
            acc,
            cell,
            GM_CONFIDENCE,
            Term::Literal(Literal::new_typed_literal(conf, named(XSD_DECIMAL)?)),
        )?;
    }
    Ok(())
}

fn emit_annotation(
    acc: &mut Acc,
    cell: NamedOrBlankNode,
    property: &str,
    value: Term,
) -> Result<(), String> {
    let ann = NamedOrBlankNode::BlankNode(acc.fresh_blank("up-ann-")?);
    insert_triple(
        &acc.out,
        cell,
        named(GM_ANNOTATION)?,
        Term::BlankNode(match &ann {
            NamedOrBlankNode::BlankNode(b) => b.clone(),
            NamedOrBlankNode::NamedNode(_) => unreachable!(),
        }),
    )?;
    insert_triple(
        &acc.out,
        ann.clone(),
        named(GM_ANN_PROPERTY)?,
        Term::NamedNode(named(property)?),
    )?;
    insert_triple(&acc.out, ann, named(GM_ANN_VALUE)?, value)?;
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
        let graph = parse_store(ttl.as_bytes(), RdfFormat::Turtle)?;
        for cell in subjects(&graph, RDF_TYPE, GM_PROJECTION_MAPPING)? {
            let Some(pattern) = value(&graph, &cell, GM_HAS_MAPPING_PATTERN)? else {
                continue;
            };
            if has_object(&graph, &pattern, GM_MINT)?
                || has_object(&graph, &pattern, GM_PATH)?
                || has_object(&graph, &pattern, GM_FILTER)?
            {
                continue;
            }
            let Some(src) = value_named(&graph, &pattern, GM_EDOAL_SOURCE)? else {
                continue;
            };
            for binding in objects(&graph, &cell, GM_HAS_BINDING)? {
                if template_atoms(&graph, &binding)?.len() > 1 {
                    continue;
                }
                let target = value_named(&graph, &binding, GM_TO_PREDICATE)?.or(value_named(
                    &graph,
                    &binding,
                    GM_TO_CLASS,
                )?);
                let Some(tgt) = target else {
                    continue;
                };
                if !in_projection_ns(tgt.as_str()) {
                    continue;
                }
                let rel = value_lexical(&graph, &binding, GM_RELATION)?.unwrap_or_default();
                if rel == "=" {
                    exact
                        .entry(tgt.as_str().to_owned())
                        .or_default()
                        .insert(src.as_str().to_owned());
                } else if rel == "<=" {
                    let cur = value_lexical(&graph, &binding, GM_CONFIDENCE)?
                        .and_then(|c| decimal_confidence(&c).map(|_| c))
                        .unwrap_or_default();
                    let bucket = generalizing.entry(tgt.as_str().to_owned()).or_default();
                    let replace = bucket.get(src.as_str()).is_none_or(|prev| {
                        !cur.is_empty()
                            && (prev.is_empty()
                                || decimal_confidence(&cur) > decimal_confidence(prev))
                    });
                    if replace {
                        bucket.insert(src.as_str().to_owned(), cur);
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
        let graph = parse_store(ttl.as_bytes(), RdfFormat::Turtle)?;
        for cell in subjects(&graph, RDF_TYPE, GM_PROJECTION_MAPPING)? {
            let Some(pattern) = value(&graph, &cell, GM_HAS_MAPPING_PATTERN)? else {
                continue;
            };
            let Some(anchor) = value(&graph, &pattern, GM_ANCHOR)? else {
                continue;
            };
            let atoms = rdf_list(&graph, value(&graph, &pattern, GM_ATOM)?.as_ref())?;
            if atoms.len() != 1 {
                continue;
            }
            let atom = &atoms[0];
            let gmeow_pred = value_named(&graph, atom, GM_PREDICATE)?;
            let gmeow_val = value_named(&graph, atom, GM_OBJECT_VALUE)?;
            if value(&graph, atom, GM_SUBJECT_VAR)? != Some(anchor.clone())
                || gmeow_pred
                    .as_ref()
                    .is_none_or(|p| !p.as_str().starts_with(GM))
                || gmeow_val.is_none()
            {
                continue;
            }
            let mints = objects(&graph, &pattern, GM_MINT)?;
            if mints.len() != 1 {
                continue;
            }
            let mint = &mints[0];
            let Some(bind_var) = value(&graph, mint, GM_BIND_VAR)? else {
                continue;
            };
            let Some(bind_expr) = value(&graph, mint, GM_BIND_EXPR)? else {
                continue;
            };
            let Term::Literal(bind_literal) = bind_expr else {
                continue;
            };
            for binding in objects(&graph, &cell, GM_HAS_BINDING)? {
                let tas = template_atoms(&graph, &binding)?;
                if tas.len() != 1 {
                    continue;
                }
                let ta = &tas[0];
                let tpred = value_named(&graph, ta, GM_T_PRED)?;
                if value(&graph, ta, GM_T_SUBJ)? == Some(anchor.clone())
                    && value(&graph, ta, GM_T_OBJ)? == Some(bind_var.clone())
                    && tpred.as_ref().is_some_and(|p| in_projection_ns(p.as_str()))
                {
                    candidates
                        .entry((
                            tpred.as_ref().expect("checked").as_str().to_owned(),
                            bind_literal.value().to_owned(),
                        ))
                        .or_default()
                        .insert((
                            gmeow_pred.as_ref().expect("checked").as_str().to_owned(),
                            gmeow_val.as_ref().expect("checked").as_str().to_owned(),
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

pub(crate) fn edoalpath_pairs(
    projection_ttls: &[String],
) -> Result<(TargetSetMap, TargetSetMap), String> {
    let mut direct: TargetSetMap = BTreeMap::new();
    let mut inverse: TargetSetMap = BTreeMap::new();
    for ttl in projection_ttls {
        let graph = parse_store(ttl.as_bytes(), RdfFormat::Turtle)?;
        for cell in subjects(&graph, RDF_TYPE, GM_PROJECTION_MAPPING)? {
            let Some(pattern) = value(&graph, &cell, GM_HAS_MAPPING_PATTERN)? else {
                continue;
            };
            if !has_object(&graph, &pattern, GM_EDOAL_PATH)?
                || has_object(&graph, &pattern, GM_MINT)?
            {
                continue;
            }
            let atoms = rdf_list(&graph, value(&graph, &pattern, GM_ATOM)?.as_ref())?;
            if atoms.len() != 1 {
                continue;
            }
            let Some(apred) = value_named(&graph, &atoms[0], GM_PREDICATE)? else {
                continue;
            };
            let Some(anchor) = value(&graph, &pattern, GM_ANCHOR)? else {
                continue;
            };
            let subjvar = value(&graph, &atoms[0], GM_SUBJECT_VAR)?;
            let objvar = value(&graph, &atoms[0], GM_OBJECT_VAR)?;
            let bucket = if subjvar.as_ref() == Some(&anchor) {
                &mut direct
            } else if objvar.as_ref() == Some(&anchor) {
                &mut inverse
            } else {
                continue;
            };
            for binding in objects(&graph, &cell, GM_HAS_BINDING)? {
                if let Some(tgt) = value_named(&graph, &binding, GM_TO_PREDICATE)? {
                    if in_projection_ns(tgt.as_str()) {
                        bucket
                            .entry(tgt.as_str().to_owned())
                            .or_default()
                            .insert(apred.as_str().to_owned());
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
        let graph = parse_store(ttl.as_bytes(), RdfFormat::Turtle)?;
        for cell in subjects(&graph, RDF_TYPE, GM_PROJECTION_MAPPING)? {
            let Some(pattern) = value(&graph, &cell, GM_HAS_MAPPING_PATTERN)? else {
                continue;
            };
            let mut obj_source = BTreeMap::new();
            for atom in pattern_atoms(
                &graph,
                value(&graph, &pattern, GM_ATOM)?.as_ref(),
                &mut HashSet::new(),
            )? {
                let objvar = value(&graph, &atom, GM_OBJECT_VAR)?;
                let pred = value_named(&graph, &atom, GM_PREDICATE)?;
                if let (Some(objvar), Some(pred)) = (objvar, pred) {
                    if pred.as_str().starts_with(GM) {
                        obj_source.insert(term_key(&objvar), pred.as_str().to_owned());
                    }
                }
            }
            for binding in objects(&graph, &cell, GM_HAS_BINDING)? {
                for tmpl in objects(&graph, &binding, GM_TEMPLATE_ATOMS)? {
                    for tatom in rdf_list(&graph, Some(&tmpl))? {
                        let tpred = value_named(&graph, &tatom, GM_T_PRED)?;
                        let tobj = value(&graph, &tatom, GM_T_OBJ)?;
                        let Some(tpred) = tpred else {
                            continue;
                        };
                        if !in_projection_ns(tpred.as_str()) {
                            continue;
                        }
                        if let Some(source) =
                            tobj.as_ref().and_then(|t| obj_source.get(&term_key(t)))
                        {
                            pairs
                                .entry(tpred.as_str().to_owned())
                                .or_default()
                                .insert(source.clone());
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
        let graph = parse_store(ttl.as_bytes(), RdfFormat::Turtle)?;
        for cell in subjects(&graph, RDF_TYPE, GM_PROJECTION_MAPPING)? {
            let Some(pattern) = value(&graph, &cell, GM_HAS_MAPPING_PATTERN)? else {
                continue;
            };
            let has_mint = has_object(&graph, &pattern, GM_MINT)?;
            let has_guard =
                has_object(&graph, &pattern, GM_PATH)? || has_object(&graph, &pattern, GM_FILTER)?;
            for binding in objects(&graph, &cell, GM_HAS_BINDING)? {
                let targets = emitted_targets(&graph, &binding)?;
                let atoms = template_atoms(&graph, &binding)?;
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

fn emitted_targets(graph: &Store, binding: &Term) -> Result<BTreeSet<String>, String> {
    let mut targets = BTreeSet::new();
    for pred in [
        GM_TO_CLASS,
        GM_TO_PREDICATE,
        "https://blackcatinformatics.ca/gmeow/edoalTarget",
    ] {
        for obj in objects(graph, binding, pred)? {
            if let Term::NamedNode(node) = obj {
                if in_projection_ns(node.as_str()) {
                    targets.insert(node.as_str().to_owned());
                }
            }
        }
    }
    for atom in template_atoms(graph, binding)? {
        for pred in [GM_T_PRED, "https://blackcatinformatics.ca/gmeow/tObjValue"] {
            for obj in objects(graph, &atom, pred)? {
                if let Term::NamedNode(node) = obj {
                    if in_projection_ns(node.as_str()) {
                        targets.insert(node.as_str().to_owned());
                    }
                }
            }
        }
    }
    Ok(targets)
}

fn build_context(
    sssom_texts: &[String],
    projection_ttls: &[String],
    ontology_nt: &str,
) -> Result<Context, String> {
    let graph = parse_store(ontology_nt.as_bytes(), RdfFormat::NTriples)?;
    let ancestors = ancestor_closure(&graph)?;
    let mut candidates: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
    let mut add =
        |target: String, gmeow: String, relation: &str, conf: String| -> Result<(), String> {
            candidates.entry(target).or_default().push(Candidate {
                context_type: domain(&graph, &gmeow)?,
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

fn ancestor_closure(graph: &Store) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut direct: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for q in graph.quads_for_pattern(None, Some(named(RDFS_SUB_CLASS_OF)?.as_ref()), None, None) {
        let q = q.map_err(|e| format!("subClassOf scan failed: {e}"))?;
        if let (NamedOrBlankNode::NamedNode(sub), Term::NamedNode(obj)) = (q.subject, q.object) {
            direct
                .entry(sub.as_str().to_owned())
                .or_default()
                .insert(obj.as_str().to_owned());
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
    Ok(closure)
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

fn domain(graph: &Store, iri: &str) -> Result<Option<String>, String> {
    let s = Term::NamedNode(named(iri)?);
    let domains = objects(graph, &s, RDFS_DOMAIN)?
        .into_iter()
        .filter_map(|term| match term {
            Term::NamedNode(node) => Some(node.as_str().to_owned()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    Ok((domains.len() == 1).then(|| domains.into_iter().next().expect("one domain")))
}

fn object_properties(ontology_nt: &str) -> Result<BTreeSet<String>, String> {
    let graph = parse_store(ontology_nt.as_bytes(), RdfFormat::NTriples)?;
    let mut props = BTreeSet::new();
    for q in graph.quads_for_pattern(
        None,
        Some(named(RDF_TYPE)?.as_ref()),
        Some((&Term::NamedNode(named(OWL_OBJECT_PROPERTY)?)).into()),
        None,
    ) {
        let q = q.map_err(|e| format!("ObjectProperty scan failed: {e}"))?;
        if let NamedOrBlankNode::NamedNode(s) = q.subject {
            props.insert(s.as_str().to_owned());
        }
    }
    Ok(props)
}

fn apply_reverse(source: &Store, acc: &mut Acc) -> Result<usize, String> {
    let mut count = 0;
    for query in reverse_queries() {
        let results = SparqlEvaluator::new()
            .parse_query(&query)
            .map_err(|e| format!("reverse query parse failed: {e}"))?
            .on_store(source)
            .execute()
            .map_err(|e| format!("reverse query evaluation failed: {e}"))?;
        let QueryResults::Graph(triples) = results else {
            return Err("reverse query did not return a graph".to_owned());
        };
        for triple in triples {
            let triple = triple.map_err(|e| format!("reverse query triple failed: {e}"))?;
            if !contains_triple(&acc.out, &triple)? {
                insert_triple(
                    &acc.out,
                    triple.subject.clone(),
                    triple.predicate.clone(),
                    triple.object.clone(),
                )?;
                count += 1;
            }
        }
    }
    Ok(count)
}

fn resolve_concept_references(
    source: &Store,
    acc: &mut Acc,
) -> Result<BTreeMap<String, usize>, String> {
    let mut anchored = BTreeSet::new();
    for q in acc.out.quads_for_pattern(
        None,
        Some(named(RDF_TYPE)?.as_ref()),
        Some((&Term::NamedNode(named(GM_TAG)?)).into()),
        None,
    ) {
        let q = q.map_err(|e| format!("tag scan failed: {e}"))?;
        let NamedOrBlankNode::NamedNode(tag) = q.subject else {
            continue;
        };
        if tag.as_str().starts_with(WD)
            || [SKOS_EXACT_MATCH, GM_AUTHORITY_LINK].iter().any(|pred| {
                objects(&acc.out, &Term::NamedNode(tag.clone()), pred)
                    .unwrap_or_default()
                    .into_iter()
                    .any(|o| matches!(o, Term::NamedNode(n) if n.as_str().starts_with(WD)))
            })
        {
            anchored.insert(tag);
        }
    }
    let mut by_label: BTreeMap<String, BTreeSet<NamedNode>> = BTreeMap::new();
    for tag in anchored {
        for label in objects(&acc.out, &Term::NamedNode(tag.clone()), RDFS_LABEL)? {
            if let Term::Literal(lit) = label {
                by_label
                    .entry(normalize_label(lit.value()))
                    .or_default()
                    .insert(tag.clone());
            }
        }
    }
    let index: BTreeMap<String, NamedNode> = by_label
        .into_iter()
        .filter_map(|(label, tags)| {
            (tags.len() == 1).then(|| (label, tags.into_iter().next().expect("one tag")))
        })
        .collect();
    if index.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut terms = BTreeMap::new();
    for triple in triples(source)? {
        if !CONCEPT_REFERENCE_PREDICATES.contains(&triple.predicate.as_str()) {
            continue;
        }
        let Term::Literal(lit) = triple.object else {
            continue;
        };
        let Some(tag) = index.get(&normalize_label(lit.value())) else {
            continue;
        };
        let out_triple = Triple::new(
            triple.subject.clone(),
            named(GM_HAS_TAG)?,
            Term::NamedNode(tag.clone()),
        );
        if !contains_triple(&acc.out, &out_triple)? {
            insert_triple(
                &acc.out,
                out_triple.subject,
                out_triple.predicate,
                out_triple.object,
            )?;
            *terms
                .entry(canon_qname(triple.predicate.as_str()))
                .or_insert(0) += 1;
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
        graph_nt: dump_nt(&acc.out)?,
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

fn used_target_terms(store: &Store) -> Result<BTreeSet<String>, String> {
    let mut terms = BTreeSet::new();
    for triple in triples(store)? {
        if in_projection_ns(triple.predicate.as_str()) {
            terms.insert(triple.predicate.as_str().to_owned());
        }
        if triple.predicate.as_str() == RDF_TYPE {
            if let Term::NamedNode(node) = triple.object {
                if in_projection_ns(node.as_str()) {
                    terms.insert(node.as_str().to_owned());
                }
            }
        }
    }
    Ok(terms)
}

fn parse_store(data: &[u8], format: RdfFormat) -> Result<Store, String> {
    let store = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
    for quad in RdfParser::from_format(format).lenient().for_slice(data) {
        store
            .insert(quad.map_err(|e| format!("RDF parse failed: {e}"))?.as_ref())
            .map_err(|e| format!("RDF store insert failed: {e}"))?;
    }
    Ok(store)
}

/// Parse a Turtle document and re-serialize it as N-Triples — the Rust-native TTL→NT
/// conversion the gate-derived audit uses so corpus reading never re-enters Python (rdflib).
pub(crate) fn ttl_to_nt(ttl: &str) -> Result<String, String> {
    let store = parse_store(ttl.as_bytes(), RdfFormat::Turtle)?;
    dump_nt(&store)
}

fn dump_nt(store: &Store) -> Result<String, String> {
    let mut buf = Vec::new();
    store
        .dump_graph_to_writer(
            GraphNameRef::DefaultGraph,
            RdfSerializer::from_format(RdfFormat::NTriples),
            &mut buf,
        )
        .map_err(|e| format!("N-Triples serialization failed: {e}"))?;
    String::from_utf8(buf).map_err(|e| format!("N-Triples output is not UTF-8: {e}"))
}

fn triples(store: &Store) -> Result<Vec<Triple>, String> {
    let mut out = Vec::new();
    for q in store.quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph)) {
        let q = q.map_err(|e| format!("store iteration failed: {e}"))?;
        out.push(Triple::new(q.subject, q.predicate, q.object));
    }
    Ok(out)
}

fn insert_triple(
    store: &Store,
    subject: NamedOrBlankNode,
    predicate: NamedNode,
    object: Term,
) -> Result<(), String> {
    store
        .insert(&Quad::new(
            subject,
            predicate,
            object,
            oxigraph::model::GraphName::DefaultGraph,
        ))
        .map_err(|e| format!("store insert failed: {e}"))
}

fn contains_triple(store: &Store, triple: &Triple) -> Result<bool, String> {
    Ok(store
        .quads_for_pattern(
            Some((&triple.subject).into()),
            Some(triple.predicate.as_ref()),
            Some((&triple.object).into()),
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
        .transpose()
        .map_err(|e| format!("store lookup failed: {e}"))?
        .is_some())
}

fn subjects(store: &Store, pred: &str, obj: &str) -> Result<Vec<Term>, String> {
    let mut out = Vec::new();
    for q in store.quads_for_pattern(
        None,
        Some(named(pred)?.as_ref()),
        Some((&Term::NamedNode(named(obj)?)).into()),
        Some(GraphNameRef::DefaultGraph),
    ) {
        let q = q.map_err(|e| format!("subject scan failed: {e}"))?;
        out.push(subject_to_term(q.subject));
    }
    Ok(out)
}

fn objects(store: &Store, subject: &Term, pred: &str) -> Result<Vec<Term>, String> {
    let Some(subject) = term_subject(subject) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for q in store.quads_for_pattern(
        Some((&subject).into()),
        Some(named(pred)?.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    ) {
        out.push(q.map_err(|e| format!("object scan failed: {e}"))?.object);
    }
    Ok(out)
}

fn value(store: &Store, subject: &Term, pred: &str) -> Result<Option<Term>, String> {
    Ok(objects(store, subject, pred)?.into_iter().next())
}

fn value_named(store: &Store, subject: &Term, pred: &str) -> Result<Option<NamedNode>, String> {
    Ok(value(store, subject, pred)?.and_then(|term| match term {
        Term::NamedNode(node) => Some(node),
        _ => None,
    }))
}

fn value_lexical(store: &Store, subject: &Term, pred: &str) -> Result<Option<String>, String> {
    Ok(value(store, subject, pred)?.map(|term| match term {
        Term::NamedNode(node) => node.as_str().to_owned(),
        Term::BlankNode(node) => node.as_str().to_owned(),
        Term::Literal(lit) => lit.value().to_owned(),
        Term::Triple(_) => String::new(),
    }))
}

fn has_object(store: &Store, subject: &Term, pred: &str) -> Result<bool, String> {
    Ok(!objects(store, subject, pred)?.is_empty())
}

fn template_atoms(store: &Store, binding: &Term) -> Result<Vec<Term>, String> {
    let mut out = Vec::new();
    for ta in objects(store, binding, GM_TEMPLATE_ATOMS)? {
        out.extend(rdf_list(store, Some(&ta))?);
    }
    Ok(out)
}

fn pattern_atoms(
    store: &Store,
    node: Option<&Term>,
    seen: &mut HashSet<String>,
) -> Result<Vec<Term>, String> {
    let mut out = Vec::new();
    for atom in rdf_list(store, node)? {
        let key = term_key(&atom);
        if !seen.insert(key) {
            continue;
        }
        if let Some(group) = value(store, &atom, GM_OPTIONAL_GROUP)? {
            out.extend(pattern_atoms(store, Some(&group), seen)?);
        } else {
            out.push(atom);
        }
    }
    Ok(out)
}

fn rdf_list(store: &Store, node: Option<&Term>) -> Result<Vec<Term>, String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let nil = Term::NamedNode(named(RDF_NIL)?);
    let mut node = node.cloned();
    while let Some(cur) = node {
        if cur == nil || !seen.insert(term_key(&cur)) {
            break;
        }
        if let Some(first) = value(store, &cur, RDF_FIRST)? {
            out.push(first);
        }
        node = value(store, &cur, RDF_REST)?;
        if let Some(rest) = &node {
            if rest == &nil {
                break;
            }
        }
    }
    Ok(out)
}

fn term_subject(term: &Term) -> Option<NamedOrBlankNode> {
    match term {
        Term::NamedNode(node) => Some(NamedOrBlankNode::NamedNode(node.clone())),
        Term::BlankNode(node) => Some(NamedOrBlankNode::BlankNode(node.clone())),
        _ => None,
    }
}

fn subject_to_term(subject: NamedOrBlankNode) -> Term {
    match subject {
        NamedOrBlankNode::NamedNode(node) => Term::NamedNode(node),
        NamedOrBlankNode::BlankNode(node) => Term::BlankNode(node),
    }
}

fn named(iri: &str) -> Result<NamedNode, String> {
    NamedNode::new(iri).map_err(|e| format!("invalid IRI {iri}: {e}"))
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

pub(crate) fn canon_qname(iri: &str) -> String {
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

pub(crate) fn prefix(term: &str) -> &str {
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
    fn from_subject(subject: &NamedOrBlankNode) -> Self {
        Self(match subject {
            NamedOrBlankNode::NamedNode(node) => format!("<{}>", node.as_str()),
            NamedOrBlankNode::BlankNode(node) => format!("_:{}", node.as_str()),
        })
    }
}

fn term_key(term: &Term) -> String {
    match term {
        Term::NamedNode(node) => format!("<{}>", node.as_str()),
        Term::BlankNode(node) => format!("_:{}", node.as_str()),
        Term::Literal(lit) => format!("\"{}\"", lit.value()),
        Term::Triple(_) => "<<triple>>".to_owned(),
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
