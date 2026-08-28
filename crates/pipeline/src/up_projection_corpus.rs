// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! SSSOM/EDOAL/corpus parse + bucket substrate and the invertibility audit enumeration.
//!
//! This module holds the Rust-native parse substrate (the frozen [`Graph`],
//! prefix/qname handling, and the linear-scan RDF readers) together with the
//! SSSOM/structural/EDOAL bucketing that classifies every external target term,
//! and the gate-derived invertibility audit ([`run_audit_nt`]) that enumerates the
//! coverage of that bucketing over a corpus. It has no dependency on the
//! heuristic lift engine; the lift engine imports from here, never the reverse.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::{Arc, OnceLock};

use gmeow_errors::ResultExt;
use purrdf::{RdfDataset, RdfQuad, RdfTerm};

pub(crate) const GM: &str = "https://blackcatinformatics.ca/gmeow/";
pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub(crate) const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
pub(crate) const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
pub(crate) const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
pub(crate) const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
pub(crate) const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
pub(crate) const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

pub(crate) const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
pub(crate) const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
pub(crate) const SKOS_CLOSE_MATCH: &str = "http://www.w3.org/2004/02/skos/core#closeMatch";
pub(crate) const SKOS_PREF_LABEL: &str = "http://www.w3.org/2004/02/skos/core#prefLabel";
pub(crate) const SKOS_ALT_LABEL: &str = "http://www.w3.org/2004/02/skos/core#altLabel";

pub(crate) const GM_PROJECTION_MAPPING: &str =
    "https://blackcatinformatics.ca/gmeow/ProjectionMapping";
pub(crate) const GM_HAS_MAPPING_PATTERN: &str =
    "https://blackcatinformatics.ca/gmeow/hasMappingPattern";
pub(crate) const GM_MINT: &str = "https://blackcatinformatics.ca/gmeow/mint";
pub(crate) const GM_PATH: &str = "https://blackcatinformatics.ca/gmeow/path";
pub(crate) const GM_FILTER: &str = "https://blackcatinformatics.ca/gmeow/filter";
pub(crate) const GM_EDOAL_SOURCE: &str = "https://blackcatinformatics.ca/gmeow/edoalSource";
pub(crate) const GM_EDOAL_PATH: &str = "https://blackcatinformatics.ca/gmeow/edoalPath";
pub(crate) const GM_HAS_BINDING: &str = "https://blackcatinformatics.ca/gmeow/hasBinding";
pub(crate) const GM_TEMPLATE_ATOMS: &str = "https://blackcatinformatics.ca/gmeow/templateAtoms";
pub(crate) const GM_TO_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/toPredicate";
pub(crate) const GM_TO_CLASS: &str = "https://blackcatinformatics.ca/gmeow/toClass";
pub(crate) const GM_RELATION: &str = "https://blackcatinformatics.ca/gmeow/relation";
pub(crate) const GM_MNEMOMORPHIC: &str = "https://blackcatinformatics.ca/gmeow/mnemomorphic";
pub(crate) const GM_CONFIDENCE: &str = "https://blackcatinformatics.ca/gmeow/confidence";
pub(crate) const GM_ATOM: &str = "https://blackcatinformatics.ca/gmeow/atom";
pub(crate) const GM_ANCHOR: &str = "https://blackcatinformatics.ca/gmeow/anchor";
pub(crate) const GM_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/predicate";
pub(crate) const GM_SUBJECT_VAR: &str = "https://blackcatinformatics.ca/gmeow/subjectVar";
pub(crate) const GM_OBJECT_VAR: &str = "https://blackcatinformatics.ca/gmeow/objectVar";
pub(crate) const GM_OBJECT_VALUE: &str = "https://blackcatinformatics.ca/gmeow/objectValue";
pub(crate) const GM_BIND_VAR: &str = "https://blackcatinformatics.ca/gmeow/bindVar";
pub(crate) const GM_BIND_EXPR: &str = "https://blackcatinformatics.ca/gmeow/bindExpr";
pub(crate) const GM_T_PRED: &str = "https://blackcatinformatics.ca/gmeow/tPred";
pub(crate) const GM_T_SUBJ: &str = "https://blackcatinformatics.ca/gmeow/tSubj";
pub(crate) const GM_T_OBJ: &str = "https://blackcatinformatics.ca/gmeow/tObj";

// The `gmeow:StatementMetadata` reified-claim vocabulary is defined ONCE in
// `gmeow-logic-compile` (alongside the shared reified-claim template builder that both the
// native put executor and the committed `.put.rq` emitter render through) and re-exported here,
// so the reification consumer (`transform.rs`), the executor (`put_executor.rs`), and the
// non-gated `STATEMENT_METADATA_TERMS` passthrough all key off one definition — never a
// per-crate copy that could drift.
pub(crate) use gmeow_logic_compile::projections::reified_claim::{
    GM_MAPPED_FROM, GM_Q_OBJECT, GM_Q_OBJECT_LITERAL, GM_Q_PREDICATE, GM_Q_SUBJECT,
    GM_STATEMENT_METADATA,
};

pub(crate) const ADOPTED_PREDICATES: &[&str] = &[SKOS_EXACT_MATCH, SKOS_CLOSE_MATCH];
pub(crate) const STATEMENT_METADATA_TERMS: &[&str] = &[
    GM_STATEMENT_METADATA,
    GM_Q_SUBJECT,
    GM_Q_PREDICATE,
    GM_Q_OBJECT,
    GM_Q_OBJECT_LITERAL,
];
pub(crate) const NORMALIZED_PREDICATES: &[(&str, &str)] =
    &[(SKOS_PREF_LABEL, RDFS_LABEL), (SKOS_ALT_LABEL, RDFS_LABEL)];
pub(crate) const PROJECTION_PREFIXES: &[&str] = &[
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

pub(crate) const PREFIXES: &[(&str, &str)] = &[
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

pub(crate) type TargetSetMap = BTreeMap<String, BTreeSet<String>>;
pub(crate) type TargetClaimMap = BTreeMap<String, BTreeMap<String, String>>;
pub(crate) type ValueRuleMap = BTreeMap<(String, String), (String, String)>;

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
            };
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

pub fn run_audit_nt(
    sssom_texts: &[String],
    projection_ttls: &[String],
    corpus_nts: &[(String, String)],
) -> gmeow_errors::Result<AuditReport> {
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

pub(crate) fn sssom_records(
    sssom_texts: &[String],
) -> gmeow_errors::Result<Vec<(purrdf::SssomMapping, PrefixMap)>> {
    let mut rows = Vec::new();
    for text in sssom_texts {
        let set = purrdf::sssom::parse_tsv(text)?;
        let prefixes = PrefixMap::from_sssom(&set.meta.curie_map);
        for row in set.mappings {
            rows.push((row, prefixes.clone()));
        }
    }
    Ok(rows)
}

pub(crate) fn sssom_clean_pairs(sssom_texts: &[String]) -> gmeow_errors::Result<TargetSetMap> {
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

pub(crate) fn sssom_closematch_pairs(
    sssom_texts: &[String],
) -> gmeow_errors::Result<TargetClaimMap> {
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

/// The per-target best SSSOM bucket, keyed by full target IRI — the corpus-independent
/// half of [`combined_class`]. Exposed for the gate-verified lift producer, which classifies
/// every candidate term through the SAME bucketing the audit uses (never a bespoke copy).
pub(crate) fn sssom_best_buckets_pub(
    sssom_texts: &[String],
) -> gmeow_errors::Result<BTreeMap<String, String>> {
    sssom_best_buckets(sssom_texts)
}

/// The per-target best structural class, keyed by full target IRI — the other half of
/// [`combined_class`]. Exposed for the gate-verified lift producer.
pub(crate) fn structural_best_classes_pub(
    projection_ttls: &[String],
) -> gmeow_errors::Result<BTreeMap<String, String>> {
    structural_best_classes(projection_ttls)
}

fn sssom_best_buckets(sssom_texts: &[String]) -> gmeow_errors::Result<BTreeMap<String, String>> {
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

pub(crate) fn structural_pairs(
    projection_ttls: &[String],
) -> gmeow_errors::Result<(TargetSetMap, TargetClaimMap)> {
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

pub(crate) fn value_mapped_pairs(projection_ttls: &[String]) -> gmeow_errors::Result<ValueRuleMap> {
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

pub(crate) fn edoalpath_pairs(
    projection_ttls: &[String],
) -> gmeow_errors::Result<(TargetSetMap, TargetSetMap)> {
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
                if let Some(tgt) = value_named(q, &binding, GM_TO_PREDICATE)
                    && in_projection_ns(&tgt)
                {
                    bucket.entry(tgt).or_default().insert(apred.clone());
                }
            }
        }
    }
    Ok((direct, inverse))
}

fn structural_best_classes(
    projection_ttls: &[String],
) -> gmeow_errors::Result<BTreeMap<String, String>> {
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
            if let RdfTerm::Iri(node) = obj
                && in_projection_ns(&node)
            {
                targets.insert(node);
            }
        }
    }
    for atom in template_atoms(quads, binding) {
        for pred in [GM_T_PRED, "https://blackcatinformatics.ca/gmeow/tObjValue"] {
            for obj in objects(quads, &atom, pred) {
                if let RdfTerm::Iri(node) = obj
                    && in_projection_ns(&node)
                {
                    targets.insert(node);
                }
            }
        }
    }
    targets
}

pub(crate) fn object_properties(ontology_nt: &str) -> gmeow_errors::Result<BTreeSet<String>> {
    let graph = Graph::parse(ontology_nt.as_bytes(), "application/n-triples")?;
    let mut props = BTreeSet::new();
    for q in &graph.quads {
        if q.predicate == RDF_TYPE
            && matches!(&q.object, RdfTerm::Iri(n) if n == OWL_OBJECT_PROPERTY || n == gmeow_ns::LOGIC_OBJECT_PROPERTY)
            && let RdfTerm::Iri(s) = &q.subject
        {
            props.insert(s.clone());
        }
    }
    Ok(props)
}

pub(crate) fn used_target_terms(quads: &[RdfQuad]) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    for triple in quads {
        if in_projection_ns(&triple.predicate) {
            terms.insert(triple.predicate.clone());
        }
        if triple.predicate == RDF_TYPE
            && let RdfTerm::Iri(node) = &triple.object
            && in_projection_ns(node)
        {
            terms.insert(node.clone());
        }
    }
    terms
}

/// Serialize the accumulator to N-Triples via the native writer.
pub(crate) fn dump_nt(quads: &[RdfQuad]) -> gmeow_errors::Result<String> {
    let dataset = purrdf::native_quads::flat_dataset_from_quads(quads).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::UpProjection {
            message: format!("re-freeze accumulated quads: {e}"),
        })
    })?;
    let bytes = purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::Dataset,
    )
    .with_ctx(|| "N-Triples serialization failed")?;
    String::from_utf8(bytes).with_ctx(|| "N-Triples output is not UTF-8")
}

/// Subjects of `(?, pred, <obj>)`, as `RdfTerm`.
pub(crate) fn subjects(quads: &[RdfQuad], pred: &str, obj: &str) -> Vec<RdfTerm> {
    quads
        .iter()
        .filter(|q| q.predicate == pred && matches!(&q.object, RdfTerm::Iri(n) if n == obj))
        .map(|q| q.subject.clone())
        .collect()
}

/// All objects of `(subject, pred, ?)`.
pub(crate) fn objects(quads: &[RdfQuad], subject: &RdfTerm, pred: &str) -> Vec<RdfTerm> {
    if !subject_addressable(subject) {
        return Vec::new();
    }
    quads
        .iter()
        .filter(|q| q.predicate == pred && term_key(&q.subject) == term_key(subject))
        .map(|q| q.object.clone())
        .collect()
}

pub(crate) fn value(quads: &[RdfQuad], subject: &RdfTerm, pred: &str) -> Option<RdfTerm> {
    objects(quads, subject, pred).into_iter().next()
}

/// Parse a Turtle document and re-serialize it as N-Triples — the Rust-native TTL→NT
/// conversion the gate-derived audit uses so corpus reading never re-enters Python (rdflib).
pub(crate) fn ttl_to_nt(ttl: &str) -> gmeow_errors::Result<String> {
    let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .with_ctx(|| "TTL parse failed")?;
    let bytes = purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::Dataset,
    )
    .with_ctx(|| "N-Triples serialization failed")?;
    String::from_utf8(bytes).with_ctx(|| "N-Triples output is not UTF-8")
}

/// The first object that is an IRI, returned as its IRI string.
pub(crate) fn value_named(quads: &[RdfQuad], subject: &RdfTerm, pred: &str) -> Option<String> {
    value(quads, subject, pred).and_then(|term| match term {
        RdfTerm::Iri(node) => Some(node),
        _ => None,
    })
}

pub(crate) fn value_lexical(quads: &[RdfQuad], subject: &RdfTerm, pred: &str) -> Option<String> {
    value(quads, subject, pred).map(|term| match term {
        RdfTerm::Iri(node) => node,
        RdfTerm::BlankNode(node) => node,
        RdfTerm::Literal(lit) => lit.lexical_form,
        RdfTerm::Triple(_) => String::new(),
    })
}

pub(crate) fn has_object(quads: &[RdfQuad], subject: &RdfTerm, pred: &str) -> bool {
    !objects(quads, subject, pred).is_empty()
}

pub(crate) fn template_atoms(quads: &[RdfQuad], binding: &RdfTerm) -> Vec<RdfTerm> {
    let mut out = Vec::new();
    for ta in objects(quads, binding, GM_TEMPLATE_ATOMS) {
        out.extend(rdf_list(quads, Some(&ta)));
    }
    out
}

pub(crate) fn rdf_list(quads: &[RdfQuad], node: Option<&RdfTerm>) -> Vec<RdfTerm> {
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
        if let Some(rest) = &node
            && rest == &nil
        {
            break;
        }
    }
    out
}

/// `true` iff `term` can stand as a subject (an IRI or blank node).
pub(crate) fn subject_addressable(term: &RdfTerm) -> bool {
    matches!(term, RdfTerm::Iri(_) | RdfTerm::BlankNode(_))
}

pub(crate) fn in_projection_ns(iri: &str) -> bool {
    projection_namespaces().iter().any(|ns| iri.starts_with(ns))
}

pub(crate) fn projection_namespaces() -> &'static [&'static str] {
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

pub(crate) fn decimal_confidence(conf: &str) -> Option<f64> {
    if conf.is_empty() || conf.contains('e') || conf.contains('E') {
        return None;
    }
    let value: f64 = conf.parse().ok()?;
    value
        .is_finite()
        .then_some(value)
        .filter(|v| (0.0..=1.0).contains(v))
}

pub(crate) fn confidence_lexeme(value: f64) -> String {
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
pub(crate) struct Graph {
    pub(crate) dataset: Arc<RdfDataset>,
    pub(crate) quads: Vec<RdfQuad>,
}

impl Graph {
    pub(crate) fn parse(data: &[u8], media_type: &str) -> gmeow_errors::Result<Self> {
        let dataset =
            purrdf::parse_dataset(data, media_type, None).with_ctx(|| "RDF parse failed")?;
        let quads = purrdf::native_quads::flat_rdf_quads_from_dataset(&dataset)
            .into_iter()
            .filter(|q| q.graph_name.is_none())
            .collect();
        Ok(Self { dataset, quads })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PrefixMap {
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

pub(crate) fn term_key(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(node) => format!("<{node}>"),
        RdfTerm::BlankNode(node) => format!("_:{node}"),
        RdfTerm::Literal(lit) => format!("\"{}\"", lit.lexical_form),
        RdfTerm::Triple(_) => "<<triple>>".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
