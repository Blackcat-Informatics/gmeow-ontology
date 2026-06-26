// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native MAXIMAL(G) transform kernel: GMEOW A-Box -> base + E(G) + P(G).
//!
//! Python supplies serialized repo-or-bundle inputs and remains the CLI/file
//! surface. This module owns the graph dataflow: deterministic skolemization,
//! suppression-aware strong-equivalence saturation, projection CONSTRUCT
//! execution, provenance merge, and GTS byte emission.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use oxigraph::io::{RdfFormat, RdfParser, RdfSerializer};
use oxigraph::model::{
    GraphName, GraphNameRef, Literal, NamedNode, NamedOrBlankNode, Quad, Term, Triple,
};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use sha2::{Digest, Sha256};

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
const SKOLEM_BASE: &str = "https://blackcatinformatics.ca/gmeow/.well-known/genid/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUB_CLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

const GM_DISPLAYABLE: &str = "https://blackcatinformatics.ca/gmeow/displayable";
const GM_COARSEN_TO: &str = "https://blackcatinformatics.ca/gmeow/coarsenTo";
const GM_COARSEN_GUARDED: &str = "https://blackcatinformatics.ca/gmeow/coarsenGuarded";
const GM_APPELLATION: &str = "https://blackcatinformatics.ca/gmeow/Appellation";
const GM_STATEMENT_METADATA: &str = "https://blackcatinformatics.ca/gmeow/StatementMetadata";
const GM_Q_SUBJECT: &str = "https://blackcatinformatics.ca/gmeow/qSubject";
const GM_Q_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/qPredicate";
const GM_Q_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/qObject";
const GM_Q_OBJECT_LITERAL: &str = "https://blackcatinformatics.ca/gmeow/qObjectLiteral";
const GM_MAPPED_FROM: &str = "https://blackcatinformatics.ca/gmeow/mappedFrom";
const GM_CONFIDENCE: &str = "https://blackcatinformatics.ca/gmeow/confidence";
const SCHEMA_SAME_AS: &str = "https://schema.org/sameAs";
const SAME_AS_MIRROR_RULE: &str = "https://blackcatinformatics.ca/gmeow/rules/sameAsMirror";

const STRONG_CLASS_PREDICATES: &[&str] = &["owl:equivalentClass", "skos:exactMatch"];
const STRONG_PROPERTY_PREDICATES: &[&str] = &["owl:equivalentProperty", "skos:exactMatch"];

const PREFIXES: &[(&str, &str)] = &[
    ("gmeow", GM),
    ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
    ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
    ("owl", "http://www.w3.org/2002/07/owl#"),
    ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ("skos", "http://www.w3.org/2004/02/skos/core#"),
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransformReportNative {
    pub base_nt: String,
    pub base_plus_derived_nt: String,
    pub gts_bytes: Vec<u8>,
    pub asserted: usize,
    pub saturated: usize,
    pub projected: usize,
    pub suppressed_dropped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellInput {
    pub iri: String,
    pub subject: String,
    pub predicate_curie: String,
    pub object: String,
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedRowNative {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub reifier: String,
    pub annotations: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct Cell {
    iri: NamedNode,
    subject: NamedNode,
    predicate_curie: String,
    object: NamedNode,
    confidence: String,
}

type EdgeMap = BTreeMap<String, Vec<Cell>>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TripleKey(String, String, String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct AnnotationKey(String, String);

#[derive(Debug, Clone)]
struct DerivedTriple {
    subject: NamedOrBlankNode,
    predicate: NamedNode,
    object: Term,
    annotations: BTreeSet<AnnotationKey>,
}

#[derive(Debug, Clone, Default)]
struct SuppressionVocab {
    bearer_props: Vec<String>,
    appellation_domain_props: BTreeSet<String>,
    appellation_classes: BTreeSet<String>,
    coarsen_guarded: BTreeSet<String>,
}

/// Return the deterministic skolemized default graph as N-Triples.
pub fn skolemize_nt(raw_nt: &str) -> Result<String, String> {
    let store = skolemized_store(raw_nt)?;
    dump_nt(&store)
}

/// Compute only E(G), returning row-shaped data for the Python compatibility API.
pub fn saturate_nt(
    abox_nt: &str,
    ontology_nt: &str,
    cells: &[CellInput],
    denied: &[(String, String, String)],
) -> Result<Vec<DerivedRowNative>, String> {
    let abox = parse_store(abox_nt.as_bytes(), RdfFormat::NTriples)?;
    let onto = parse_store(ontology_nt.as_bytes(), RdfFormat::NTriples)?;
    let cells = convert_cells(cells)?;
    let denied = denied.iter().cloned().collect();
    let vocab = suppression_vocab(&onto)?;
    let derived = saturate_store(&abox, &onto, &cells, &denied, &vocab)?;
    Ok(derived_to_rows(&derived))
}

/// Compute MAXIMAL(G) over serialized inputs.
pub fn transform_nt(
    raw_nt: &str,
    ontology_nt: &str,
    cells: &[CellInput],
    denied: &[(String, String, String)],
    projection_queries: &[(String, String)],
) -> Result<TransformReportNative, String> {
    let mut abox = skolemized_store(raw_nt)?;
    let onto = parse_store(ontology_nt.as_bytes(), RdfFormat::NTriples)?;
    let cells = convert_cells(cells)?;
    let denied = denied.iter().cloned().collect();
    let vocab = suppression_vocab(&onto)?;
    let suppressed = suppressed_nodes(&abox, &vocab)?;
    if !suppressed.is_empty() {
        abox = published_store(&abox, &suppressed)?;
    }

    let saturated = saturate_store(&abox, &onto, &cells, &denied, &vocab)?;
    let saturated_count = saturated.len();
    let projected = projection_derived(&abox, &onto, projection_queries, &suppressed)?;
    let projected_count = projected.len();
    let derived = merge_derived(saturated, projected);

    let base_nt = dump_nt(&abox)?;
    let base_plus_derived = base_plus_derived_store(&abox, &derived)?;
    let base_plus_derived_nt = dump_nt(&base_plus_derived)?;
    let gts_bytes = gts_from_maximal(&base_plus_derived, &derived)?;

    Ok(TransformReportNative {
        asserted: store_len(&abox)?,
        saturated: saturated_count,
        projected: projected_count,
        suppressed_dropped: suppressed.len(),
        base_nt,
        base_plus_derived_nt,
        gts_bytes,
    })
}

fn convert_cells(inputs: &[CellInput]) -> Result<Vec<Cell>, String> {
    inputs
        .iter()
        .map(|cell| {
            Ok(Cell {
                iri: named(&cell.iri)?,
                subject: named(&cell.subject)?,
                predicate_curie: cell.predicate_curie.clone(),
                object: named(&cell.object)?,
                confidence: cell.confidence.clone(),
            })
        })
        .collect()
}

fn skolemized_store(raw_nt: &str) -> Result<Store, String> {
    let quads = parse_quads(raw_nt.as_bytes(), RdfFormat::NTriples)?;
    // Native full RDFC-1.0 (SHA-256) canonical labels — the oxrdf
    // `Dataset::canonicalize` is fully evicted (#910). Both implement the same
    // RDFC-1.0, so the `_:c14nN` labels (hence the persisted skolem IRIs) are stable.
    let canon = gmeow_rdf::canonicalize_quads(quads)
        .map_err(|e| format!("canonicalization failed: {e}"))?;
    let out = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
    for quad in &canon {
        let subject = skolem_subject(quad.subject.as_ref())?;
        let object = skolem_term(quad.object.as_ref())?;
        let graph_name = match quad.graph_name.as_ref() {
            GraphNameRef::DefaultGraph => GraphName::DefaultGraph,
            GraphNameRef::NamedNode(n) => GraphName::NamedNode(n.into_owned()),
            GraphNameRef::BlankNode(b) => GraphName::NamedNode(
                NamedNode::new(format!("{SKOLEM_BASE}{}", b.as_str()))
                    .map_err(|e| format!("invalid skolem graph IRI: {e}"))?,
            ),
        };
        out.insert(&Quad::new(
            subject,
            quad.predicate.clone(),
            object,
            graph_name,
        ))
        .map_err(|e| format!("skolemized store insert failed: {e}"))?;
    }
    Ok(out)
}

fn skolem_subject(
    subject: oxigraph::model::NamedOrBlankNodeRef<'_>,
) -> Result<NamedOrBlankNode, String> {
    Ok(match subject {
        oxigraph::model::NamedOrBlankNodeRef::NamedNode(n) => {
            NamedOrBlankNode::NamedNode(n.into_owned())
        }
        oxigraph::model::NamedOrBlankNodeRef::BlankNode(b) => NamedOrBlankNode::NamedNode(
            NamedNode::new(format!("{SKOLEM_BASE}{}", b.as_str()))
                .map_err(|e| format!("invalid skolem subject IRI: {e}"))?,
        ),
    })
}

fn skolem_term(term: oxigraph::model::TermRef<'_>) -> Result<Term, String> {
    Ok(match term {
        oxigraph::model::TermRef::NamedNode(n) => Term::NamedNode(n.into_owned()),
        oxigraph::model::TermRef::BlankNode(b) => Term::NamedNode(
            NamedNode::new(format!("{SKOLEM_BASE}{}", b.as_str()))
                .map_err(|e| format!("invalid skolem object IRI: {e}"))?,
        ),
        oxigraph::model::TermRef::Literal(l) => Term::Literal(l.into_owned()),
        oxigraph::model::TermRef::Triple(t) => Term::Triple(Box::new(Triple::new(
            skolem_subject(t.subject.as_ref())?,
            t.predicate.clone(),
            skolem_term(t.object.as_ref())?,
        ))),
    })
}

fn build_strong_edges(
    cells: &[Cell],
    onto: &Store,
    denied: &BTreeSet<(String, String, String)>,
) -> Result<(EdgeMap, EdgeMap), String> {
    let mut class_edges: EdgeMap = BTreeMap::new();
    let mut property_edges: EdgeMap = BTreeMap::new();
    for cell in cells {
        let subject = cell.subject.as_str();
        if !subject.starts_with(GM) {
            continue;
        }
        let denial = (
            curie(subject),
            cell.predicate_curie.clone(),
            curie(cell.object.as_str()),
        );
        if denied.contains(&denial) {
            continue;
        }
        let is_class = has_type(onto, subject, OWL_CLASS)?;
        let mut is_property = false;
        for kind in [
            OWL_OBJECT_PROPERTY,
            OWL_DATATYPE_PROPERTY,
            OWL_ANNOTATION_PROPERTY,
        ] {
            if has_type(onto, subject, kind)? {
                is_property = true;
                break;
            }
        }
        if is_class && STRONG_CLASS_PREDICATES.contains(&cell.predicate_curie.as_str()) {
            class_edges
                .entry(subject.to_owned())
                .or_default()
                .push(cell.clone());
        } else if is_property && STRONG_PROPERTY_PREDICATES.contains(&cell.predicate_curie.as_str())
        {
            property_edges
                .entry(subject.to_owned())
                .or_default()
                .push(cell.clone());
        }
    }
    Ok((class_edges, property_edges))
}

fn saturate_store(
    abox: &Store,
    onto: &Store,
    cells: &[Cell],
    denied: &BTreeSet<(String, String, String)>,
    vocab: &SuppressionVocab,
) -> Result<BTreeMap<TripleKey, DerivedTriple>, String> {
    let (class_edges, property_edges) = build_strong_edges(cells, onto, denied)?;
    let suppressed = suppressed_nodes(abox, vocab)?;
    let mut derived: BTreeMap<TripleKey, DerivedTriple> = BTreeMap::new();
    let rdf_type = named(RDF_TYPE)?;

    for q in abox.quads_for_pattern(
        None,
        Some(rdf_type.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    ) {
        let q = q.map_err(|e| format!("rdf:type scan failed: {e}"))?;
        if suppressed.contains(&subject_token(&q.subject)) {
            continue;
        }
        let Term::NamedNode(cls) = q.object else {
            continue;
        };
        if let Some(edge_cells) = class_edges.get(cls.as_str()) {
            for cell in edge_cells {
                emit_derived(
                    &mut derived,
                    abox,
                    q.subject.clone(),
                    rdf_type.clone(),
                    Term::NamedNode(cell.object.clone()),
                    cell_annotations(cell)?,
                )?;
            }
        }
    }

    for (prop, edge_cells) in &property_edges {
        let pred = named(prop)?;
        for q in abox.quads_for_pattern(
            None,
            Some(pred.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        ) {
            let q = q.map_err(|e| format!("property scan failed for {prop}: {e}"))?;
            if suppressed.contains(&subject_token(&q.subject))
                || suppressed.contains(&term_token(&q.object))
            {
                continue;
            }
            if vocab.coarsen_guarded.contains(prop) && has_any(abox, &q.subject, GM_COARSEN_TO)? {
                continue;
            }
            for cell in edge_cells {
                emit_derived(
                    &mut derived,
                    abox,
                    q.subject.clone(),
                    cell.object.clone(),
                    q.object.clone(),
                    cell_annotations(cell)?,
                )?;
            }
        }
    }

    for q in abox.quads_for_pattern(
        None,
        Some(named(OWL_SAME_AS)?.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    ) {
        let q = q.map_err(|e| format!("owl:sameAs scan failed: {e}"))?;
        if suppressed.contains(&subject_token(&q.subject))
            || suppressed.contains(&term_token(&q.object))
        {
            continue;
        }
        emit_derived(
            &mut derived,
            abox,
            q.subject,
            named(SCHEMA_SAME_AS)?,
            q.object,
            vec![AnnotationKey(
                GM_MAPPED_FROM.to_owned(),
                format!("<{SAME_AS_MIRROR_RULE}>"),
            )],
        )?;
    }

    Ok(derived)
}

fn emit_derived(
    derived: &mut BTreeMap<TripleKey, DerivedTriple>,
    abox: &Store,
    subject: NamedOrBlankNode,
    predicate: NamedNode,
    object: Term,
    annotations: Vec<AnnotationKey>,
) -> Result<(), String> {
    if contains_triple(abox, &subject, &predicate, &object)? {
        return Ok(());
    }
    let key = triple_key(&subject, &predicate, &object);
    let row = derived.entry(key).or_insert_with(|| DerivedTriple {
        subject,
        predicate,
        object,
        annotations: BTreeSet::new(),
    });
    row.annotations.extend(annotations);
    Ok(())
}

fn cell_annotations(cell: &Cell) -> Result<Vec<AnnotationKey>, String> {
    let mut rows = vec![AnnotationKey(
        GM_MAPPED_FROM.to_owned(),
        format!("<{}>", cell.iri.as_str()),
    )];
    if !cell.confidence.is_empty() {
        let lit = Term::Literal(Literal::new_typed_literal(
            &cell.confidence,
            named(XSD_DECIMAL)?,
        ));
        rows.push(AnnotationKey(GM_CONFIDENCE.to_owned(), term_token(&lit)));
    }
    Ok(rows)
}

fn projection_derived(
    abox: &Store,
    onto: &Store,
    projection_queries: &[(String, String)],
    suppressed: &BTreeSet<String>,
) -> Result<BTreeMap<TripleKey, DerivedTriple>, String> {
    let projection_input = projection_input_store(abox, onto)?;
    let onto_subjects = subjects(onto)?;
    let mut derived: BTreeMap<TripleKey, DerivedTriple> = BTreeMap::new();
    for (name, query) in projection_queries {
        let alignment = format!("{GM}projections/{name}");
        let results = SparqlEvaluator::new()
            .parse_query(query)
            .map_err(|e| format!("projection query parse failed for {name}: {e}"))?
            .on_store(&projection_input)
            .execute()
            .map_err(|e| format!("projection query evaluation failed for {name}: {e}"))?;
        let QueryResults::Graph(triples) = results else {
            return Err(format!("projection query {name} did not return a graph"));
        };
        for triple in triples {
            let triple = triple.map_err(|e| format!("projection triple failed for {name}: {e}"))?;
            if onto_subjects.contains(&subject_token(&triple.subject)) {
                continue;
            }
            if suppressed.contains(&subject_token(&triple.subject))
                || suppressed.contains(&term_token(&triple.object))
            {
                continue;
            }
            emit_derived(
                &mut derived,
                abox,
                triple.subject,
                triple.predicate,
                triple.object,
                vec![AnnotationKey(
                    GM_MAPPED_FROM.to_owned(),
                    format!("<{alignment}>"),
                )],
            )?;
        }
    }
    Ok(derived)
}

fn projection_input_store(abox: &Store, onto: &Store) -> Result<Store, String> {
    let store = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
    copy_store(onto, &store)?;
    copy_store(abox, &store)?;
    for triple in materialized_claims(abox)? {
        insert_triple(&store, triple.subject, triple.predicate, triple.object)?;
    }
    Ok(store)
}

fn materialized_claims(abox: &Store) -> Result<Vec<Triple>, String> {
    let mut out = Vec::new();
    for cell in subjects_for_type(abox, GM_STATEMENT_METADATA)? {
        let Some(s_term) = value(abox, &cell, GM_Q_SUBJECT)? else {
            continue;
        };
        let Some(p_term) = value(abox, &cell, GM_Q_PREDICATE)? else {
            continue;
        };
        let mut o_term = value(abox, &cell, GM_Q_OBJECT)?;
        if o_term.is_none() {
            o_term = value(abox, &cell, GM_Q_OBJECT_LITERAL)?;
        }
        let Some(o_term) = o_term else {
            continue;
        };
        let Some(subject) = term_as_subject(s_term) else {
            continue;
        };
        let Term::NamedNode(predicate) = p_term else {
            continue;
        };
        out.push(Triple::new(subject, predicate, o_term));
    }
    Ok(out)
}

fn merge_derived(
    saturated: BTreeMap<TripleKey, DerivedTriple>,
    projected: BTreeMap<TripleKey, DerivedTriple>,
) -> BTreeMap<TripleKey, DerivedTriple> {
    let mut merged = saturated;
    for (key, row) in projected {
        let target = merged.entry(key).or_insert_with(|| DerivedTriple {
            subject: row.subject,
            predicate: row.predicate,
            object: row.object,
            annotations: BTreeSet::new(),
        });
        target.annotations.extend(row.annotations);
    }
    merged
}

fn base_plus_derived_store(
    base: &Store,
    derived: &BTreeMap<TripleKey, DerivedTriple>,
) -> Result<Store, String> {
    let out = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
    copy_store(base, &out)?;
    for row in derived.values() {
        insert_triple(
            &out,
            row.subject.clone(),
            row.predicate.clone(),
            row.object.clone(),
        )?;
    }
    Ok(out)
}

fn gts_from_maximal(
    base_plus_derived: &Store,
    derived: &BTreeMap<TripleKey, DerivedTriple>,
) -> Result<Vec<u8>, String> {
    let mut builder = gmeow_rdf::gts_compose::SnapshotBuilder::new();
    let base_quads = quads(base_plus_derived)?;
    builder.add_quads(&base_quads, None, None);
    let statement_nt = statement_layer_nt(derived);
    if !statement_nt.trim().is_empty() {
        let statement_quads = gmeow_rdf::gts_compose::parse_quads_lenient(
            statement_nt.as_bytes(),
            gmeow_rdf::NativeRdfFormat::NTriples,
        )?;
        builder.add_rdf12(&statement_quads, None, None)?;
    }
    gmeow_rdf::gts_compose::emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        gmeow_rdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
    )
}

fn statement_layer_nt(derived: &BTreeMap<TripleKey, DerivedTriple>) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for row in derived.values() {
        let reifier = reifier_for(&row.subject, &row.predicate, &row.object);
        let _ = writeln!(
            &mut out,
            "<{reifier}> <{RDF_REIFIES}> <<( {} <{}> {} )>> .",
            subject_token(&row.subject),
            row.predicate.as_str(),
            term_token(&row.object)
        );
        for ann in &row.annotations {
            let _ = writeln!(&mut out, "<{reifier}> <{}> {} .", ann.0, ann.1);
        }
    }
    out
}

fn derived_to_rows(derived: &BTreeMap<TripleKey, DerivedTriple>) -> Vec<DerivedRowNative> {
    derived
        .values()
        .map(|row| DerivedRowNative {
            subject: subject_token(&row.subject),
            predicate: row.predicate.as_str().to_owned(),
            object: term_token(&row.object),
            reifier: reifier_for(&row.subject, &row.predicate, &row.object),
            annotations: row
                .annotations
                .iter()
                .map(|ann| (ann.0.clone(), ann.1.clone()))
                .collect(),
        })
        .collect()
}

fn published_store(abox: &Store, suppressed: &BTreeSet<String>) -> Result<Store, String> {
    let out = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
    for q in abox.quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph)) {
        let q = q.map_err(|e| format!("publication filtering failed: {e}"))?;
        if !suppressed.contains(&subject_token(&q.subject))
            && !suppressed.contains(&term_token(&q.object))
        {
            insert_triple(&out, q.subject, q.predicate, q.object)?;
        }
    }
    Ok(out)
}

fn suppression_vocab(onto: &Store) -> Result<SuppressionVocab, String> {
    let classes = subclass_closure(onto, GM_APPELLATION)?;
    let mut bearer: BTreeSet<String> = BTreeSet::new();
    for (prop, rng) in subject_objects(onto, RDFS_RANGE)? {
        if classes.contains(&term_token(&rng)) {
            if let Some(prop) = subject_iri(&prop) {
                bearer.insert(prop);
            }
        }
    }
    let mut domain_props: BTreeSet<String> = BTreeSet::new();
    for (prop, dom) in subject_objects(onto, RDFS_DOMAIN)? {
        if classes.contains(&term_token(&dom)) {
            if let Some(prop) = subject_iri(&prop) {
                domain_props.insert(prop);
            }
        }
    }
    let mut coarsen_guarded = BTreeSet::new();
    for (sub, obj) in subject_objects(onto, GM_COARSEN_GUARDED)? {
        if matches!(obj, Term::Literal(lit) if lit.value() == "true") {
            if let Some(prop) = subject_iri(&sub) {
                coarsen_guarded.insert(prop);
            }
        }
    }
    Ok(SuppressionVocab {
        bearer_props: bearer.into_iter().collect(),
        appellation_domain_props: domain_props,
        appellation_classes: classes,
        coarsen_guarded,
    })
}

fn suppressed_nodes(abox: &Store, vocab: &SuppressionVocab) -> Result<BTreeSet<String>, String> {
    let mut suppressed = BTreeSet::new();
    for (subject, object) in subject_objects(abox, GM_DISPLAYABLE)? {
        if matches!(object, Term::Literal(lit) if lit.value() == "false" || lit.value() == "0") {
            suppressed.insert(subject_token(&subject));
        }
    }
    if suppressed.is_empty() {
        return Ok(suppressed);
    }

    let mut appellations = BTreeSet::new();
    for cls in &vocab.appellation_classes {
        for subject in subjects_for_type_token(abox, cls)? {
            appellations.insert(subject);
        }
    }
    for prop in &vocab.appellation_domain_props {
        for (subject, _object) in subject_objects(abox, prop)? {
            appellations.insert(subject_token(&subject));
        }
    }

    let mut extra = BTreeSet::new();
    for prop in &vocab.bearer_props {
        for (bearer, appellation) in subject_objects(abox, prop)? {
            if suppressed.contains(&subject_token(&bearer))
                && appellations.contains(&term_token(&appellation))
            {
                extra.insert(term_token(&appellation));
            }
        }
    }
    suppressed.extend(extra);
    Ok(suppressed)
}

fn subclass_closure(store: &Store, root: &str) -> Result<BTreeSet<String>, String> {
    let mut closure = BTreeSet::new();
    closure.insert(format!("<{root}>"));
    let edges = subject_objects(store, RDFS_SUB_CLASS_OF)?;
    loop {
        let mut grew = false;
        for (sub, sup) in &edges {
            if closure.contains(&term_token(sup)) && !closure.contains(&subject_token(sub)) {
                closure.insert(subject_token(sub));
                grew = true;
            }
        }
        if !grew {
            return Ok(closure);
        }
    }
}

fn subjects_for_type(store: &Store, class: &str) -> Result<Vec<NamedOrBlankNode>, String> {
    let mut out = Vec::new();
    for q in store.quads_for_pattern(
        None,
        Some(named(RDF_TYPE)?.as_ref()),
        Some((&Term::NamedNode(named(class)?)).into()),
        Some(GraphNameRef::DefaultGraph),
    ) {
        let q = q.map_err(|e| format!("type subject scan failed: {e}"))?;
        out.push(q.subject);
    }
    Ok(out)
}

fn subjects_for_type_token(store: &Store, class_token: &str) -> Result<Vec<String>, String> {
    let class = iri_from_token(class_token)?;
    Ok(subjects_for_type(store, &class)?
        .iter()
        .map(subject_token)
        .collect())
}

fn subject_objects(
    store: &Store,
    predicate: &str,
) -> Result<Vec<(NamedOrBlankNode, Term)>, String> {
    let mut out = Vec::new();
    for q in store.quads_for_pattern(
        None,
        Some(named(predicate)?.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    ) {
        let q = q.map_err(|e| format!("subject-object scan failed for {predicate}: {e}"))?;
        out.push((q.subject, q.object));
    }
    Ok(out)
}

fn value(
    store: &Store,
    subject: &NamedOrBlankNode,
    predicate: &str,
) -> Result<Option<Term>, String> {
    let mut iter = store.quads_for_pattern(
        Some(subject.as_ref()),
        Some(named(predicate)?.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    );
    Ok(iter
        .next()
        .transpose()
        .map_err(|e| format!("value scan failed for {predicate}: {e}"))?
        .map(|q| q.object))
}

fn subjects(store: &Store) -> Result<BTreeSet<String>, String> {
    let mut out = BTreeSet::new();
    for q in store.quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph)) {
        let q = q.map_err(|e| format!("subject scan failed: {e}"))?;
        out.insert(subject_token(&q.subject));
    }
    Ok(out)
}

fn has_type(store: &Store, subject: &str, class: &str) -> Result<bool, String> {
    let subject = named(subject)?;
    let class = named(class)?;
    let mut iter = store.quads_for_pattern(
        Some(subject.as_ref().into()),
        Some(named(RDF_TYPE)?.as_ref()),
        Some((&Term::NamedNode(class)).into()),
        Some(GraphNameRef::DefaultGraph),
    );
    Ok(iter
        .next()
        .transpose()
        .map_err(|e| format!("type check failed: {e}"))?
        .is_some())
}

fn has_any(store: &Store, subject: &NamedOrBlankNode, predicate: &str) -> Result<bool, String> {
    let mut iter = store.quads_for_pattern(
        Some(subject.as_ref()),
        Some(named(predicate)?.as_ref()),
        None,
        Some(GraphNameRef::DefaultGraph),
    );
    Ok(iter
        .next()
        .transpose()
        .map_err(|e| format!("presence check failed for {predicate}: {e}"))?
        .is_some())
}

fn contains_triple(
    store: &Store,
    subject: &NamedOrBlankNode,
    predicate: &NamedNode,
    object: &Term,
) -> Result<bool, String> {
    let mut iter = store.quads_for_pattern(
        Some(subject.as_ref()),
        Some(predicate.as_ref()),
        Some(object.as_ref()),
        Some(GraphNameRef::DefaultGraph),
    );
    Ok(iter
        .next()
        .transpose()
        .map_err(|e| format!("triple presence check failed: {e}"))?
        .is_some())
}

fn copy_store(from: &Store, to: &Store) -> Result<(), String> {
    for q in from.quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph)) {
        let q = q.map_err(|e| format!("store copy failed: {e}"))?;
        to.insert(q.as_ref())
            .map_err(|e| format!("store copy insert failed: {e}"))?;
    }
    Ok(())
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
            GraphName::DefaultGraph,
        ))
        .map_err(|e| format!("store insert failed: {e}"))
}

fn term_as_subject(term: Term) -> Option<NamedOrBlankNode> {
    match term {
        Term::NamedNode(n) => Some(NamedOrBlankNode::NamedNode(n)),
        Term::BlankNode(b) => Some(NamedOrBlankNode::BlankNode(b)),
        Term::Literal(_) | Term::Triple(_) => None,
    }
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

fn parse_quads(data: &[u8], format: RdfFormat) -> Result<Vec<Quad>, String> {
    let mut out = Vec::new();
    for quad in RdfParser::from_format(format).lenient().for_slice(data) {
        out.push(quad.map_err(|e| format!("RDF parse failed: {e}"))?);
    }
    Ok(out)
}

fn quads(store: &Store) -> Result<Vec<Quad>, String> {
    let mut out = Vec::new();
    for q in store.quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph)) {
        out.push(q.map_err(|e| format!("quad collection failed: {e}"))?);
    }
    Ok(out)
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

fn store_len(store: &Store) -> Result<usize, String> {
    store.len().map_err(|e| format!("store length failed: {e}"))
}

fn named(iri: &str) -> Result<NamedNode, String> {
    NamedNode::new(iri).map_err(|e| format!("invalid IRI {iri:?}: {e}"))
}

fn subject_token(subject: &NamedOrBlankNode) -> String {
    match subject {
        NamedOrBlankNode::NamedNode(n) => format!("<{}>", n.as_str()),
        NamedOrBlankNode::BlankNode(b) => format!("_:{}", b.as_str()),
    }
}

fn subject_iri(subject: &NamedOrBlankNode) -> Option<String> {
    match subject {
        NamedOrBlankNode::NamedNode(n) => Some(n.as_str().to_owned()),
        NamedOrBlankNode::BlankNode(_) => None,
    }
}

fn term_token(term: &Term) -> String {
    term.to_string()
}

fn triple_key(subject: &NamedOrBlankNode, predicate: &NamedNode, object: &Term) -> TripleKey {
    TripleKey(
        subject_token(subject),
        format!("<{}>", predicate.as_str()),
        term_token(object),
    )
}

fn reifier_for(subject: &NamedOrBlankNode, predicate: &NamedNode, object: &Term) -> String {
    let mut hasher = Sha256::new();
    hasher.update(
        format!(
            "{}|<{}>|{}",
            subject_token(subject),
            predicate.as_str(),
            term_token(object)
        )
        .as_bytes(),
    );
    let digest = hasher.finalize();
    format!(
        "{GM}derivations/{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7]
    )
}

fn curie(iri: &str) -> String {
    static SORTED_PREFIXES: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    let prefixes = SORTED_PREFIXES.get_or_init(|| {
        let mut prefixes = PREFIXES.to_vec();
        prefixes.sort_by_key(|(_, ns)| std::cmp::Reverse(ns.len()));
        prefixes
    });
    for (prefix, ns) in prefixes {
        if let Some(local) = iri.strip_prefix(ns) {
            return format!("{prefix}:{local}");
        }
    }
    iri.to_owned()
}

fn iri_from_token(token: &str) -> Result<String, String> {
    token
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .map(str::to_owned)
        .ok_or_else(|| format!("expected IRI token, got {token:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reifier_hash_matches_python_contract() {
        let s = NamedOrBlankNode::NamedNode(named("https://example.org/s").unwrap());
        let p = named("https://example.org/p").unwrap();
        let o = Term::NamedNode(named("https://example.org/o").unwrap());
        assert_eq!(
            reifier_for(&s, &p, &o),
            "https://blackcatinformatics.ca/gmeow/derivations/bc5c0b0074e06845"
        );
    }

    #[test]
    fn curie_prefers_longest_namespace() {
        assert_eq!(
            curie("http://id.loc.gov/ontologies/bibframe/Work"),
            "bf:Work"
        );
    }
}
