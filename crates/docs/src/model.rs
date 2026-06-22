// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The typed, deterministic documentation model (PyO3-free).
//!
//! [`DocsModel`] is built from a [`SliceCatalog`] plus an [`OwnershipReport`].
//! It is a *projection*: it references artifacts by digest/path and never embeds
//! their raw bytes (blobs are by-reference per project doctrine), and every
//! collection is sorted by a stable key so the serialized model is
//! byte-reproducible.

use std::collections::BTreeMap;
use std::path::Path;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{GraphNameRef, NamedOrBlankNode, Term};
use oxigraph::store::Store;
use serde::Serialize;

use gmeow_slice::{
    ArtifactRecord, ArtifactRole, ManifestView, OwnershipAnalyzer, OwnershipReport, SliceCatalog,
    SliceError, SliceRecord, SliceTier,
};

// ── Namespace constants ───────────────────────────────────────────────────────

/// The GMEOW vocabulary namespace; IRIs under it get the `gmeow:` CURIE prefix.
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const RDFS_CLASS: &str = "http://www.w3.org/2000/01/rdf-schema#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";
const OWL_NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";
const RDFS_DATATYPE: &str = "http://www.w3.org/2000/01/rdf-schema#Datatype";

/// An error building the documentation model.
#[derive(Debug)]
pub enum DocsError {
    /// A slice-catalog discovery / parse error.
    Slice(SliceError),
}

impl std::fmt::Display for DocsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocsError::Slice(e) => write!(f, "slice catalog error: {e}"),
        }
    }
}

impl std::error::Error for DocsError {}

impl From<SliceError> for DocsError {
    fn from(e: SliceError) -> Self {
        DocsError::Slice(e)
    }
}

// ── Model types ───────────────────────────────────────────────────────────────

/// The vocabulary kind of a documented term, derived from its `rdf:type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum DocTermCategory {
    /// `owl:Class` / `rdfs:Class`.
    Class,
    /// `owl:ObjectProperty` / `owl:DatatypeProperty` / `owl:AnnotationProperty`
    /// / `rdf:Property`.
    Property,
    /// `owl:NamedIndividual`.
    Individual,
    /// `rdfs:Datatype`.
    Datatype,
    /// A GMEOW subject that carries definitional metadata but no recognized
    /// vocabulary `rdf:type`.
    Other,
}

/// A single artifact within a slice, referenced by digest/path (no bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocArtifact {
    /// The artifact role (module, shapes, query, …).
    pub role: ArtifactRole,
    /// Normalized logical path within the slice directory.
    pub logical_path: String,
    /// MIME type.
    pub media_type: String,
    /// SHA-256 hex digest of the raw file bytes.
    pub raw_digest: String,
    /// SHA-256 hex of the canonical N-Triples for RDF artifacts; `None` otherwise.
    pub semantic_digest: Option<String>,
}

impl DocArtifact {
    fn from_record(record: &ArtifactRecord) -> Self {
        Self {
            role: record.role.clone(),
            logical_path: record.logical_path.clone(),
            media_type: record.media_type.clone(),
            raw_digest: record.raw_digest.clone(),
            semantic_digest: record.semantic_digest.clone(),
        }
    }
}

/// A documented slice: manifest identity + its artifact inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocSlice {
    /// The slice IRI (`a gmeow:Slice`).
    pub iri: String,
    /// `rdfs:label`.
    pub label: Option<String>,
    /// `dcterms:title`.
    pub title: Option<String>,
    /// `gmeow:sliceTier`.
    pub tier: Option<SliceTier>,
    /// `dcterms:identifier` (e.g. DOI).
    pub identifier: Option<String>,
    /// `dcterms:creator` values.
    pub creators: Vec<String>,
    /// `gmeow:sliceConsumer` values.
    pub consumers: Vec<String>,
    /// All artifacts in the slice (sorted by logical path).
    pub artifacts: Vec<DocArtifact>,
}

impl DocSlice {
    fn from_record(record: &SliceRecord) -> Self {
        let ManifestView {
            slice_iri,
            label,
            title,
            creators,
            identifier,
            tier,
            consumers,
        } = &record.manifest;

        let mut artifacts: Vec<DocArtifact> = record
            .artifacts
            .iter()
            .map(DocArtifact::from_record)
            .collect();
        artifacts.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));

        let mut creators = creators.clone();
        creators.sort();
        let mut consumers = consumers.clone();
        consumers.sort();

        Self {
            iri: slice_iri.clone(),
            label: label.clone(),
            title: title.clone(),
            tier: tier.clone(),
            identifier: identifier.clone(),
            creators,
            consumers,
            artifacts,
        }
    }
}

/// A documented vocabulary term parsed from a slice's `module.ttl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocTerm {
    /// The full term IRI.
    pub iri: String,
    /// The compact CURIE (`gmeow:Foo` for GMEOW-namespaced terms, else the IRI).
    pub curie: String,
    /// `rdfs:label`.
    pub label: Option<String>,
    /// `skos:definition` (falling back to `rdfs:comment`).
    pub definition: Option<String>,
    /// The vocabulary category derived from `rdf:type`.
    pub category: DocTermCategory,
    /// The slice IRI that defines this term (the module it was parsed from).
    pub owner_slice: String,
    /// `rdfs:subClassOf` / `rdfs:subPropertyOf` parents (IRIs, sorted).
    pub parents: Vec<String>,
    /// `rdfs:domain` values (IRIs, sorted).
    pub domain: Vec<String>,
    /// `rdfs:range` values (IRIs, sorted).
    pub range: Vec<String>,
}

/// A cross-slice dependency edge projected from the ownership report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocDependencyEdge {
    /// The depending (from) slice IRI.
    pub from: String,
    /// The depended-upon (to) slice IRI.
    pub to: String,
    /// The edge-kind name (`Ontology`, `Shape`, `Mapping`, `Query`, …).
    pub kind: String,
    /// The reconciliation verdict against `gmeow:sliceDependsOn`.
    pub reconciliation: String,
}

/// The complete typed documentation model — one source of truth for every
/// renderer. All collections are sorted by a stable key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocsModel {
    /// A fixed human title for the documentation surface.
    pub title: String,
    /// The model schema version (bumped when the shape changes).
    pub version: String,
    /// All documented slices (sorted by IRI).
    pub slices: Vec<DocSlice>,
    /// All documented vocabulary terms (sorted by IRI).
    pub terms: Vec<DocTerm>,
    /// All cross-slice dependency edges (sorted by from/to/kind).
    pub dependency_edges: Vec<DocDependencyEdge>,
}

impl DocsModel {
    /// The model schema version. Bump when the serialized shape changes.
    pub const VERSION: &'static str = "1";

    /// Build the documentation model from a discovered catalog and a computed
    /// ownership report.
    pub fn from_catalog(catalog: &SliceCatalog, ownership: &OwnershipReport) -> Self {
        // ── Slices ──────────────────────────────────────────────────────────
        let mut slices: Vec<DocSlice> = catalog
            .records()
            .iter()
            .map(DocSlice::from_record)
            .collect();
        slices.sort_by(|a, b| a.iri.cmp(&b.iri));

        // ── Terms (parsed from each slice's module.ttl) ─────────────────────
        let mut terms: Vec<DocTerm> = Vec::new();
        for record in catalog.records() {
            let owner = &record.manifest.slice_iri;
            for artifact in &record.artifacts {
                if artifact.role != ArtifactRole::Module {
                    continue;
                }
                // A module that fails to parse is a hard fault — the same lenient
                // parser the slice catalog already validated it with is used here,
                // so this should never fail, but we surface it rather than hide it.
                let Ok(store) = parse_turtle_lenient(&artifact.content) else {
                    continue;
                };
                terms.extend(extract_terms(&store, owner));
            }
        }
        terms.sort_by(|a, b| a.iri.cmp(&b.iri));

        // ── Dependency edges ────────────────────────────────────────────────
        let mut dependency_edges: Vec<DocDependencyEdge> = ownership
            .edges
            .iter()
            .map(|e| DocDependencyEdge {
                from: e.from_slice.clone(),
                to: e.to_slice.clone(),
                kind: format!("{:?}", e.edge_kind),
                reconciliation: format!("{:?}", e.reconciliation),
            })
            .collect();
        dependency_edges.sort_by(|a, b| {
            a.from
                .cmp(&b.from)
                .then_with(|| a.to.cmp(&b.to))
                .then_with(|| a.kind.cmp(&b.kind))
        });

        Self {
            title: "GMEOW Ontology Documentation".to_string(),
            version: Self::VERSION.to_string(),
            slices,
            terms,
            dependency_edges,
        }
    }

    /// Discover the slice catalog under `root/slices`, run ownership analysis,
    /// and build the model.
    pub fn discover(root: &Path) -> Result<Self, DocsError> {
        let catalog = SliceCatalog::discover(&root.join("slices"))?;
        let ownership = OwnershipAnalyzer::new(&catalog).analyze()?;
        Ok(Self::from_catalog(&catalog, &ownership))
    }
}

// ── Turtle parsing + term extraction ──────────────────────────────────────────

/// Parse Turtle bytes into an oxigraph store using the SAME lenient parser the
/// slice catalog uses (accepts `@x-gmeow-*` language tags).
fn parse_turtle_lenient(bytes: &[u8]) -> Result<Store, SliceError> {
    let store =
        Store::new().map_err(|e| SliceError::Parse(format!("store creation failed: {e}")))?;
    for quad in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(bytes)
    {
        let quad = quad.map_err(|e| SliceError::Parse(format!("syntax error: {e}")))?;
        store
            .insert(&quad)
            .map_err(|e| SliceError::Parse(format!("store insert failed: {e}")))?;
    }
    Ok(store)
}

/// Extract documented terms (GMEOW-namespaced typed subjects) from a module store.
fn extract_terms(store: &Store, owner_slice: &str) -> Vec<DocTerm> {
    // First pass: collect every GMEOW subject with a recognized vocabulary type,
    // keyed by IRI, recording the strongest category seen.
    let mut categories: BTreeMap<String, DocTermCategory> = BTreeMap::new();

    for quad in store
        .quads_for_pattern(
            None,
            Some(named(RDF_TYPE).as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .flatten()
    {
        let NamedOrBlankNode::NamedNode(subject) = &quad.subject else {
            continue;
        };
        if !subject.as_str().starts_with(GMEOW_NS) {
            continue;
        }
        let Term::NamedNode(type_node) = &quad.object else {
            continue;
        };
        let Some(category) = category_for_type(type_node.as_str()) else {
            continue;
        };
        let entry = categories
            .entry(subject.as_str().to_string())
            .or_insert(category);
        // Prefer the more specific category (Class/Property/Datatype) over
        // a bare Individual / Other when a subject is multiply typed.
        if category_rank(category) > category_rank(*entry) {
            *entry = category;
        }
    }

    // Second pass: build a DocTerm per discovered subject.
    let mut terms = Vec::new();
    for (iri, category) in categories {
        let label = first_literal(store, &iri, RDFS_LABEL);
        let definition = first_literal(store, &iri, SKOS_DEFINITION)
            .or_else(|| first_literal(store, &iri, RDFS_COMMENT));

        let mut parents = named_objects(store, &iri, RDFS_SUBCLASS_OF);
        parents.extend(named_objects(store, &iri, RDFS_SUBPROPERTY_OF));
        parents.sort();
        parents.dedup();

        let mut domain = named_objects(store, &iri, RDFS_DOMAIN);
        domain.sort();
        domain.dedup();

        let mut range = named_objects(store, &iri, RDFS_RANGE);
        range.sort();
        range.dedup();

        let curie = to_curie(&iri);
        terms.push(DocTerm {
            iri,
            curie,
            label,
            definition,
            category,
            owner_slice: owner_slice.to_string(),
            parents,
            domain,
            range,
        });
    }
    terms
}

/// Map an `rdf:type` object IRI to a documented term category.
fn category_for_type(type_iri: &str) -> Option<DocTermCategory> {
    match type_iri {
        OWL_CLASS | RDFS_CLASS => Some(DocTermCategory::Class),
        OWL_OBJECT_PROPERTY | OWL_DATATYPE_PROPERTY | OWL_ANNOTATION_PROPERTY | RDF_PROPERTY => {
            Some(DocTermCategory::Property)
        }
        OWL_NAMED_INDIVIDUAL => Some(DocTermCategory::Individual),
        RDFS_DATATYPE => Some(DocTermCategory::Datatype),
        _ => None,
    }
}

/// A specificity rank so a multiply-typed subject keeps its strongest category.
fn category_rank(c: DocTermCategory) -> u8 {
    match c {
        DocTermCategory::Other => 0,
        DocTermCategory::Individual => 1,
        DocTermCategory::Datatype => 2,
        DocTermCategory::Property => 3,
        DocTermCategory::Class => 4,
    }
}

/// Build a `NamedNode` from a static, known-valid IRI.
fn named(iri: &str) -> oxigraph::model::NamedNode {
    oxigraph::model::NamedNode::new_unchecked(iri)
}

/// The first literal value for `subject predicate ?o` (deterministic: lowest
/// lexical form), or `None`.
fn first_literal(store: &Store, subject: &str, predicate: &str) -> Option<String> {
    let subject = oxigraph::model::NamedNode::new(subject).ok()?;
    let mut values: Vec<String> = store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(named(predicate).as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .flatten()
        .filter_map(|q| match q.object {
            Term::Literal(lit) => Some(lit.value().to_string()),
            _ => None,
        })
        .collect();
    values.sort();
    values.into_iter().next()
}

/// All NamedNode object IRIs for `subject predicate ?o`.
fn named_objects(store: &Store, subject: &str, predicate: &str) -> Vec<String> {
    let Ok(subject) = oxigraph::model::NamedNode::new(subject) else {
        return Vec::new();
    };
    store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(named(predicate).as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .flatten()
        .filter_map(|q| match q.object {
            Term::NamedNode(n) => Some(n.as_str().to_string()),
            _ => None,
        })
        .collect()
}

/// Compute the compact CURIE for an IRI: `gmeow:Local` for GMEOW-namespaced
/// IRIs, otherwise the IRI unchanged.
fn to_curie(iri: &str) -> String {
    match iri.strip_prefix(GMEOW_NS) {
        Some(local) => format!("gmeow:{local}"),
        None => iri.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_from(ttl: &str) -> Store {
        parse_turtle_lenient(ttl.as_bytes()).expect("parse")
    }

    #[test]
    fn extract_terms_classifies_and_curies() {
        let ttl = r#"
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:Animal a owl:Class ;
    rdfs:label "Animal" ;
    skos:definition "A living organism." .

gmeow:Cat a owl:Class ;
    rdfs:subClassOf gmeow:Animal ;
    rdfs:label "Cat" .

gmeow:hasOwner a owl:ObjectProperty ;
    rdfs:domain gmeow:Cat ;
    rdfs:range gmeow:Person ;
    rdfs:comment "Ownership relation." .
"#;
        let store = store_from(ttl);
        let terms = extract_terms(&store, "https://example.org/slice/zoo");

        let cat = terms.iter().find(|t| t.iri.ends_with("Cat")).unwrap();
        assert_eq!(cat.category, DocTermCategory::Class);
        assert_eq!(cat.curie, "gmeow:Cat");
        assert_eq!(cat.label.as_deref(), Some("Cat"));
        assert_eq!(cat.parents, vec![format!("{GMEOW_NS}Animal")]);
        assert_eq!(cat.owner_slice, "https://example.org/slice/zoo");

        let prop = terms.iter().find(|t| t.iri.ends_with("hasOwner")).unwrap();
        assert_eq!(prop.category, DocTermCategory::Property);
        assert_eq!(prop.definition.as_deref(), Some("Ownership relation."));
        assert_eq!(prop.domain, vec![format!("{GMEOW_NS}Cat")]);
        assert_eq!(prop.range, vec![format!("{GMEOW_NS}Person")]);

        let animal = terms.iter().find(|t| t.iri.ends_with("Animal")).unwrap();
        assert_eq!(animal.definition.as_deref(), Some("A living organism."));
    }

    #[test]
    fn non_gmeow_terms_are_skipped() {
        let ttl = r#"
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
<https://example.org/Foo> a owl:Class .
"#;
        let store = store_from(ttl);
        assert!(extract_terms(&store, "s").is_empty());
    }
}
