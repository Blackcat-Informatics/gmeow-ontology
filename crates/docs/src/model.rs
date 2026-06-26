// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The typed, deterministic documentation model (PyO3-free).
//!
//! [`DocsModel`] is built from a [`SliceCatalog`] plus an [`OwnershipReport`].
//! It is a *projection*: it references artifacts by digest/path and never embeds
//! their raw bytes (blobs are by-reference per project doctrine), and every
//! collection is sorted by a stable key so the serialized model is
//! byte-reproducible.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oxigraph::model::{GraphNameRef, NamedOrBlankNode, Term};
use oxigraph::store::Store;
use serde::Serialize;

use gmeow_slice::{
    ArtifactRecord, ArtifactRole, ManifestView, OwnershipAnalyzer, OwnershipReport, SliceCatalog,
    SliceError, SliceRecord, SliceTier,
};

use crate::i18n::{self, Translations, UiCatalog};

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

// ── GMEOW-vocabulary predicates / classes used by the linkage / concern surfaces ─

const GMEOW_MAPPING_SET: &str = "https://blackcatinformatics.ca/gmeow/MappingSet";
const GMEOW_TERM_EQUIVALENCE: &str = "https://blackcatinformatics.ca/gmeow/TermEquivalence";
const GMEOW_DOCUMENTATION_CONCERN: &str =
    "https://blackcatinformatics.ca/gmeow/DocumentationConcern";

const GMEOW_SSSOM_FILE: &str = "https://blackcatinformatics.ca/gmeow/sssomFile";
const GMEOW_SET_ID: &str = "https://blackcatinformatics.ca/gmeow/setId";
const GMEOW_LICENSE: &str = "https://blackcatinformatics.ca/gmeow/license";
const GMEOW_SET_COMMENT: &str = "https://blackcatinformatics.ca/gmeow/setComment";

const GMEOW_ALIGN_SUBJECT: &str = "https://blackcatinformatics.ca/gmeow/alignSubject";
const GMEOW_ALIGN_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/alignPredicate";
const GMEOW_ALIGN_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/alignObject";
const GMEOW_JUSTIFICATION: &str = "https://blackcatinformatics.ca/gmeow/justification";
const GMEOW_CONFIDENCE: &str = "https://blackcatinformatics.ca/gmeow/confidence";

// ── Per-term usage-advice predicates (rendered as the "Usage Advice" section) ────
const SKOS_SCOPE_NOTE: &str = "http://www.w3.org/2004/02/skos/core#scopeNote";
const SKOS_EXAMPLE: &str = "http://www.w3.org/2004/02/skos/core#example";
const GMEOW_USE_WHEN: &str = "https://blackcatinformatics.ca/gmeow/useWhen";
const GMEOW_AVOID_WHEN: &str = "https://blackcatinformatics.ca/gmeow/avoidWhen";
const GMEOW_HOW_TO_USE: &str = "https://blackcatinformatics.ca/gmeow/howToUse";
const GMEOW_USE_FOR_CONSUMER: &str = "https://blackcatinformatics.ca/gmeow/useForConsumer";
const GMEOW_AVOID_FOR_CONSUMER: &str = "https://blackcatinformatics.ca/gmeow/avoidForConsumer";

const GMEOW_DOCS_CONCERN: &str = "https://blackcatinformatics.ca/gmeow/docsConcern";

// ── Guides-slice predicates / classes (recipes + learning paths, #853 T3b) ─────

const GMEOW_RECIPE: &str = "https://blackcatinformatics.ca/gmeow/Recipe";
const GMEOW_LEARNING_PATH: &str = "https://blackcatinformatics.ca/gmeow/LearningPath";
const GMEOW_GUIDE_SLUG: &str = "https://blackcatinformatics.ca/gmeow/guideSlug";
const GMEOW_GUIDE_TITLE: &str = "https://blackcatinformatics.ca/gmeow/guideTitle";
const GMEOW_GUIDE_GOAL: &str = "https://blackcatinformatics.ca/gmeow/guideGoal";
const GMEOW_LEARNING_AUDIENCE: &str = "https://blackcatinformatics.ca/gmeow/learningAudience";
const GMEOW_USES_EXAMPLE_PATH: &str = "https://blackcatinformatics.ca/gmeow/usesExamplePath";
const GMEOW_USES_TERM: &str = "https://blackcatinformatics.ca/gmeow/usesTerm";
const GMEOW_INCLUDES_RECIPE: &str = "https://blackcatinformatics.ca/gmeow/includesRecipe";
const GMEOW_ADOPTION_TARGET: &str = "https://blackcatinformatics.ca/gmeow/adoptionTarget";
const GMEOW_FOLLOWS_GUIDE_PATH: &str = "https://blackcatinformatics.ca/gmeow/followsGuidePath";

// ── Logic stereotypes + relational surfaces (#1020) ─────────────────────────────

/// The lowered-logic (OntoUML/UFO discipline) namespace; co-asserted `rdf:type`
/// values under it are surfaced as the term's logic stereotypes.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";

const SKOS_RELATED: &str = "http://www.w3.org/2004/02/skos/core#related";
const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";
const GMEOW_PAIRS_WITH: &str = "https://blackcatinformatics.ca/gmeow/pairsWith";
const GMEOW_GRAPH_BOX_ROLE: &str = "https://blackcatinformatics.ca/gmeow/graphBoxRole";

// ── Per-term lifecycle surface (#1026) ──────────────────────────────────────────
const OWL_DEPRECATED: &str = "http://www.w3.org/2002/07/owl#deprecated";
const GMEOW_TERM_STABILITY: &str = "https://blackcatinformatics.ca/gmeow/termStability";
const GMEOW_ADDED_IN_VERSION: &str = "https://blackcatinformatics.ca/gmeow/addedInVersion";
const GMEOW_HAS_CHANGELOG_ENTRY: &str = "https://blackcatinformatics.ca/gmeow/hasChangelogEntry";
const GMEOW_ENTRY_VERSION: &str = "https://blackcatinformatics.ca/gmeow/entryVersion";
const GMEOW_ENTRY_NOTE: &str = "https://blackcatinformatics.ca/gmeow/entryNote";
const STABILITY_STABLE_CURIE: &str = "gmeow:stabilityStable";
const STABILITY_EXPERIMENTAL_CURIE: &str = "gmeow:stabilityExperimental";
const STABILITY_DEPRECATED_CURIE: &str = "gmeow:stabilityDeprecated";
/// The name of the everything-aggregation profile (root + every extension);
/// every documented term belongs to it (#330 `full.ttl`).
const FULL_PROFILE_NAME: &str = "full";
const GMEOW_WORK: &str = "https://blackcatinformatics.ca/gmeow/Work";
const DCTERMS_IDENTIFIER: &str = "http://purl.org/dc/terms/identifier";

// ── SHACL constraint surface (#1020) ────────────────────────────────────────────

const SH_TARGET_CLASS: &str = "http://www.w3.org/ns/shacl#targetClass";
const SH_TARGET_SUBJECTS_OF: &str = "http://www.w3.org/ns/shacl#targetSubjectsOf";
const SH_TARGET_OBJECTS_OF: &str = "http://www.w3.org/ns/shacl#targetObjectsOf";
const SH_MESSAGE: &str = "http://www.w3.org/ns/shacl#message";

// ── Competency-question surface (#1020) ─────────────────────────────────────────

const GMEOW_COMPETENCY_QUESTION: &str = "https://blackcatinformatics.ca/gmeow/CompetencyQuestion";
const GMEOW_CQ_RATIONALE: &str = "https://blackcatinformatics.ca/gmeow/cqRationale";
const GMEOW_CQ_QUERY_FILE: &str = "https://blackcatinformatics.ca/gmeow/cqQueryFile";
const GMEOW_CQ_EXPECT_ROW: &str = "https://blackcatinformatics.ca/gmeow/cqExpectRow";
const GMEOW_ROW_CELL: &str = "https://blackcatinformatics.ca/gmeow/rowCell";
const GMEOW_CELL_VALUE_IRI: &str = "https://blackcatinformatics.ca/gmeow/cellValueIri";

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Default)]
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
    #[default]
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
    /// `gmeow:sliceProfile` values — named profiles this slice declares
    /// membership in (sorted). Drives per-term profile chips (#1026).
    pub profiles: Vec<String>,
    /// `gmeow:sliceDependsOn` slice IRIs (sorted). The relation whose closure
    /// over a profile's declared members yields the profile's full membership
    /// (#330); reused to compute per-term profile membership (#1026).
    pub depends_on: Vec<String>,
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
            profiles,
            depends_on,
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
        let mut profiles = profiles.clone();
        profiles.sort();
        profiles.dedup();
        let mut depends_on = depends_on.clone();
        depends_on.sort();
        depends_on.dedup();

        Self {
            iri: slice_iri.clone(),
            label: label.clone(),
            title: title.clone(),
            tier: tier.clone(),
            identifier: identifier.clone(),
            creators,
            consumers,
            profiles,
            depends_on,
            artifacts,
        }
    }
}

/// The maturity status of a vocabulary term (#1026). Serializes as a lowercase
/// string (`stable` / `experimental` / `deprecated`). Resolved from an explicit
/// `gmeow:termStability` annotation, else `owl:deprecated`, else the owner
/// slice's tier (core → stable, extension → experimental).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DocTermStability {
    /// Mature, committed; safe to rely on. The core-tier default.
    #[default]
    Stable,
    /// Provisional; may change or be withdrawn. The extension-tier default.
    Experimental,
    /// Retained for continuity but should no longer be used.
    Deprecated,
}

impl DocTermStability {
    /// The lowercase badge label shown on a term page.
    pub fn label(&self) -> &'static str {
        match self {
            DocTermStability::Stable => "stable",
            DocTermStability::Experimental => "experimental",
            DocTermStability::Deprecated => "deprecated",
        }
    }
}

/// One reified per-release changelog entry for a term (#1026). Ordered by
/// `(version, note)` for deterministic rendering.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Default)]
pub struct DocChangelogEntry {
    /// `gmeow:entryVersion` — the release this entry pertains to.
    pub version: String,
    /// `gmeow:entryNote` — optional prose describing the change (English carrier).
    pub note: Option<String>,
}

/// A documented vocabulary term parsed from a slice's `module.ttl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
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
    /// `skos:scopeNote` — usage-advice prose (English carrier, sorted).
    pub scope_notes: Vec<String>,
    /// `skos:example` — worked-usage prose (English carrier, sorted).
    pub examples: Vec<String>,
    /// `gmeow:useWhen` — when to reach for this term (English carrier, sorted).
    pub use_when: Vec<String>,
    /// `gmeow:avoidWhen` — when NOT to use this term (English carrier, sorted).
    pub avoid_when: Vec<String>,
    /// `gmeow:howToUse` — idiomatic-use guidance (English carrier, sorted).
    pub how_to_use: Vec<String>,
    /// `gmeow:useForConsumer` — consumer profiles this term serves (CURIEs, sorted).
    pub use_for_consumer: Vec<String>,
    /// `gmeow:avoidForConsumer` — consumer profiles to steer away (CURIEs, sorted).
    pub avoid_for_consumer: Vec<String>,
    /// Logic stereotypes co-asserted as `rdf:type` values in the `logic:`
    /// namespace (`logic:Kind`, `logic:SubKind`, `logic:Relator`, …), rendered
    /// as `logic:`-prefixed CURIEs, sorted/deduped. The lowered OntoUML/UFO
    /// discipline of the term (see `slices/core/logic`).
    pub logic_stereotypes: Vec<String>,
    /// Related-term IRIs: the union of `skos:related`, `gmeow:pairsWith`, and
    /// `rdfs:seeAlso` objects, resolved BIDIRECTIONALLY in `from_catalog`
    /// (sorted/deduped).
    pub related_terms: Vec<String>,
    /// `gmeow:graphBoxRole` — the four-boxes role CURIE (`gmeow:boxTBox`,
    /// `gmeow:boxABox`, …); the lowest-sorted when multiply asserted.
    pub box_role: Option<String>,
    /// Reverse `logic:formalizes` back-references: the IRIs of logic axioms /
    /// subjects that declare `logic:formalizes <this term>` (sorted/deduped).
    /// Empty until the central logic slice carries such back-refs.
    pub formalized_by: Vec<String>,
    /// The term's maturity badge (#1026), always resolved: explicit
    /// `gmeow:termStability` > `owl:deprecated` > owner-slice tier default.
    pub stability: DocTermStability,
    /// `gmeow:addedInVersion` — the release a term first appeared in (the
    /// lowest-sorted literal when multiply asserted); `None` until seeded (#1026).
    pub added_in_version: Option<String>,
    /// `gmeow:hasChangelogEntry` — reified per-release change records, sorted by
    /// `(version, note)` (#1026). Empty when the term carries no changelog.
    pub changelog: Vec<DocChangelogEntry>,
    /// The named profiles whose membership closure includes this term's owner
    /// slice, plus the always-present `full` aggregate (sorted/deduped, #1026).
    /// Computed in `from_catalog` from the slices' `sliceProfile` /
    /// `sliceDependsOn` declarations.
    pub profiles: Vec<String>,
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

/// A mapping set (`gmeow:MappingSet`) owned by a slice's `mappings/` artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocMappingSet {
    /// The mapping-set IRI (`a gmeow:MappingSet`).
    pub iri: String,
    /// The compact CURIE.
    pub curie: String,
    /// `gmeow:setId`.
    pub set_id: Option<String>,
    /// `gmeow:sssomFile` — the compiled SSSOM filename.
    pub sssom_file: Option<String>,
    /// `gmeow:license`.
    pub license: Option<String>,
    /// `gmeow:setComment`.
    pub comment: Option<String>,
    /// The slice IRI that owns the mapping artifact.
    pub owner_slice: String,
    /// The number of `DocLinkage` equivalences in this set.
    pub equivalence_count: usize,
}

/// A single term equivalence (`gmeow:TermEquivalence`) — a cross-walk from a
/// GMEOW term to an external IRI via a SKOS-style alignment predicate.
///
/// `confidence` is an `f64`, so this type is `PartialEq` but not `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocLinkage {
    /// The mapping-set IRI this equivalence belongs to (by `gmeow:sssomFile`).
    pub mapping_set: Option<String>,
    /// `gmeow:alignSubject` — the GMEOW term IRI.
    pub subject: String,
    /// The subject as a CURIE.
    pub subject_curie: String,
    /// `gmeow:alignPredicate` — e.g. `skos:closeMatch`.
    pub predicate: String,
    /// `gmeow:alignObject` — the external IRI.
    pub object: String,
    /// `gmeow:justification`.
    pub justification: Option<String>,
    /// `gmeow:confidence` (a literal `xsd:decimal`/`xsd:double`), if present.
    pub confidence: Option<f64>,
    /// The slice IRI that owns the mapping artifact.
    pub owner_slice: String,
}

/// A worked example carried IN FULL (examples are small Turtle text, not blobs;
/// their source must be shown).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocExample {
    /// The slice IRI that owns the example.
    pub slice: String,
    /// The logical path within the slice directory.
    pub logical_path: String,
    /// A human title (an `rdfs:label` if any subject carries one, else derived
    /// from the filename).
    pub title: String,
    /// The Turtle source, carried in full.
    pub text: String,
    /// GMEOW CURIEs referenced anywhere in the example (sorted, deduped).
    pub terms_referenced: Vec<String>,
}

/// A SHACL node shape, reverse-mapped to the term it constrains. Parsed from a
/// slice's `shapes.ttl` (`ArtifactRole::Shapes`) and the root `shapes/*.ttl`
/// files. DISTINCT from the integrity-constraint index (SPARQL verify queries):
/// this surface is SHACL structural validation per target term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct DocShape {
    /// The `sh:NodeShape` IRI (or a `_:`-prefixed blank-node id for anonymous shapes).
    pub shape_iri: String,
    /// The constrained term IRI, resolved from `sh:targetClass` /
    /// `sh:targetSubjectsOf` / `sh:targetObjectsOf`.
    pub target_term: String,
    /// The `sh:message` strings reachable within the shape (including nested
    /// property shapes and `sh:or` lists), sorted/deduped.
    pub messages: Vec<String>,
    /// The slice IRI that owns the shapes artifact, or `"root"` for `shapes/*.ttl`.
    pub owner_slice: String,
}

/// A competency question (`gmeow:CompetencyQuestion`) reverse-mapped to the terms
/// it exercises, so each term page can surface a "Tested by" block. Parsed from
/// each slice's `tests/competency.ttl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct DocCompetency {
    /// The competency-question IRI.
    pub iri: String,
    /// `gmeow:cqRationale` — why the ontology must answer this.
    pub rationale: Option<String>,
    /// `gmeow:cqQueryFile` — the slice-relative SPARQL query path.
    pub query_file: Option<String>,
    /// The term IRIs this CQ exercises, reached via
    /// `gmeow:cqExpectRow → gmeow:rowCell → gmeow:cellValueIri` (sorted/deduped).
    pub exercises: Vec<String>,
    /// The slice IRI that owns the competency artifact.
    pub owner_slice: String,
}

/// A documentation concern (`gmeow:DocumentationConcern`) and the terms that
/// declare it via `gmeow:docsConcern`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocConcern {
    /// The concern IRI.
    pub iri: String,
    /// The compact CURIE.
    pub curie: String,
    /// `rdfs:label`.
    pub label: Option<String>,
    /// `skos:definition` (falling back to `rdfs:comment`).
    pub definition: Option<String>,
    /// CURIEs of terms annotated with this concern (sorted, deduped).
    pub terms: Vec<String>,
    /// Slice IRIs whose terms declare this concern (sorted, deduped).
    pub slices: Vec<String>,
}

/// An external (non-GMEOW) term referenced by the ontology — via a linkage
/// object or a term domain/range/parent edge — grouped for an overview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocExternalTerm {
    /// The external IRI.
    pub iri: String,
    /// The namespace prefix (the IRI up to and including the last `/` or `#`).
    pub namespace: String,
    /// GMEOW CURIEs that reference this external IRI (sorted, deduped).
    pub referenced_by: Vec<String>,
    /// The predicates the reference travels over (`alignObject`, `subClassOf`,
    /// `domain`, `range`), sorted/deduped.
    pub via_predicate: Vec<String>,
}

/// A task-oriented adoption recipe (`gmeow:Recipe`), parsed from the guides
/// slice. A curated guide that sequences canonical examples + terms for one
/// recurring modelling task (no domain axiom — pure pedagogy).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocRecipe {
    /// `gmeow:guideSlug` — the stable, filesystem-safe identifier.
    pub slug: String,
    /// `gmeow:guideTitle` (falling back to `rdfs:label`).
    pub title: String,
    /// `gmeow:guideGoal` — the prose modelling outcome.
    pub goal: String,
    /// `gmeow:usesExamplePath` — slice-relative example file paths (sorted).
    pub example_paths: Vec<String>,
    /// `gmeow:usesTerm` — referenced GMEOW term CURIEs (sorted, deduped).
    pub term_curies: Vec<String>,
    /// `gmeow:followsGuidePath` — documentation-relative follow-on pages (sorted).
    pub follow_pages: Vec<String>,
}

/// A curated adoption journey (`gmeow:LearningPath`), parsed from the guides
/// slice. Sequences recipes, examples, and terms for a named audience.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocLearningPath {
    /// `gmeow:guideSlug` — the stable, filesystem-safe identifier.
    pub slug: String,
    /// `gmeow:guideTitle` (falling back to `rdfs:label`).
    pub title: String,
    /// `gmeow:learningAudience` — the intended-audience description.
    pub audience: String,
    /// `gmeow:guideGoal` — the prose modelling outcome.
    pub goal: String,
    /// `gmeow:includesRecipe` — the slugs of the recipes this path folds in
    /// (resolved from the recipe individuals; sorted, deduped).
    pub recipe_slugs: Vec<String>,
    /// `gmeow:usesExamplePath` — slice-relative example file paths (sorted).
    pub example_paths: Vec<String>,
    /// `gmeow:usesTerm` — referenced GMEOW term CURIEs (sorted, deduped).
    pub term_curies: Vec<String>,
    /// `gmeow:adoptionTarget` — external-vocabulary prefix strings (sorted).
    pub adoption_targets: Vec<String>,
}

/// The complete typed documentation model — one source of truth for every
/// renderer. All collections are sorted by a stable key.
///
/// Holds [`DocLinkage`] (with an `f64` confidence), so this type is `PartialEq`
/// but not `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
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
    /// All mapping sets (sorted by IRI).
    pub mapping_sets: Vec<DocMappingSet>,
    /// All term equivalences / linkages (sorted by subject/predicate/object).
    pub linkages: Vec<DocLinkage>,
    /// All worked examples (sorted by slice/logical-path).
    pub examples: Vec<DocExample>,
    /// All SHACL node shapes reverse-mapped to the terms they constrain
    /// (sorted by target term then shape IRI).
    pub shapes: Vec<DocShape>,
    /// All competency questions reverse-mapped to the terms they exercise
    /// (sorted by IRI).
    pub competencies: Vec<DocCompetency>,
    /// All documentation concerns (sorted by IRI).
    pub concerns: Vec<DocConcern>,
    /// All external (non-GMEOW) terms referenced (sorted by IRI).
    pub external_terms: Vec<DocExternalTerm>,
    /// All adoption recipes parsed from the guides slice (sorted by slug).
    pub recipes: Vec<DocRecipe>,
    /// All curated learning paths parsed from the guides slice (sorted by slug).
    pub learning_paths: Vec<DocLearningPath>,
    /// The curated "four boxes" doctrine prose, read at build time from
    /// `<root>/docs/four-boxes.md` if present (`None` when absent).
    pub four_boxes: Option<String>,
    /// The ontology's concept DOI (`dcterms:identifier` on the `gmeow:Work`
    /// subject of `<root>/metadata/gmeow-self.ttl`), read in `discover()`. Drives
    /// the per-term citation block's "cite the ontology" line (#1026). `None`
    /// when the metadata file is absent.
    pub concept_doi: Option<String>,
    /// Available documentation languages: the English carrier (`"english"`)
    /// first, then the BCP-47 codes (`fr`, `zh`) of every slice translation
    /// catalog, sorted. Deterministic.
    #[serde(skip)]
    pub available_languages: Vec<String>,
    /// The per-(term, predicate, language) translation index built from every
    /// slice's `i18n/<lang>.po` catalog. The renderer resolves localized
    /// labels / definitions through it, falling back to the English values
    /// carried on each model element. Skipped from serialization so the JSON
    /// model shape (and its golden) is unchanged.
    #[serde(skip)]
    pub translations: Translations,
    /// The UI-chrome override catalog (per-language nav / heading / footer
    /// strings) loaded from optional `<root>/i18n/ontology-docs-templates.<lang>.po`
    /// files. Empty when none are present (English fallback). Skipped from
    /// serialization.
    #[serde(skip)]
    pub ui_catalog: UiCatalog,
}

impl DocsModel {
    /// The model schema version. Bump when the serialized shape changes.
    pub const VERSION: &'static str = "5";

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
        // `logic:formalizes` back-references, collected while modules are parsed.
        let mut formalizes_edges: Vec<(String, String)> = Vec::new();
        for record in catalog.records() {
            let owner = &record.manifest.slice_iri;
            for artifact in &record.artifacts {
                if artifact.role != ArtifactRole::Module {
                    continue;
                }
                // A module that fails to parse is a hard fault — the same lenient
                // parser the slice catalog already validated it with is used here,
                // so this should never fail; surface it loudly with full context.
                let store = parse_turtle_lenient(&artifact.content).unwrap_or_else(|e| {
                    panic!("module.ttl for slice {owner} failed to parse: {e}")
                });
                terms.extend(extract_terms(&store, owner, record.manifest.tier.as_ref()));
                formalizes_edges.extend(extract_formalizes(&store));
            }
        }
        terms.sort_by(|a, b| a.iri.cmp(&b.iri));

        // Bidirectional related terms: if A lists B, ensure B lists A. The
        // forward edges were collected per-term in `extract_terms`; here we mirror
        // each one onto the (already-documented) target term.
        {
            let index: BTreeMap<String, usize> = terms
                .iter()
                .enumerate()
                .map(|(i, t)| (t.iri.clone(), i))
                .collect();
            let reverse_edges: Vec<(usize, String)> = terms
                .iter()
                .flat_map(|t| {
                    let from = t.iri.clone();
                    t.related_terms
                        .iter()
                        .filter_map(|to| index.get(to).map(|&i| (i, from.clone())))
                        .collect::<Vec<_>>()
                })
                .collect();
            for (target_idx, from_iri) in reverse_edges {
                terms[target_idx].related_terms.push(from_iri);
            }
            // `logic:formalizes` reverse pass: subject formalizes target term.
            for (subject, target) in &formalizes_edges {
                if let Some(&i) = index.get(target) {
                    terms[i].formalized_by.push(subject.clone());
                }
            }
            for t in &mut terms {
                t.related_terms.sort();
                t.related_terms.dedup();
                t.formalized_by.sort();
                t.formalized_by.dedup();
            }
        }

        // ── Per-term profile membership (#1026) ─────────────────────────────
        // A term belongs to a named profile P iff P's declared-member-plus-
        // sliceDependsOn closure contains the term's owner slice; every term
        // also belongs to `full` (root + every extension). This MIRRORS the
        // pipeline `profiles` stage's closure (#330) from the same manifest
        // data, without touching that byte-identical stage.
        {
            // slice IRI → its sliceDependsOn list (for the closure walk).
            let depends: BTreeMap<&str, &[String]> = slices
                .iter()
                .map(|s| (s.iri.as_str(), s.depends_on.as_slice()))
                .collect();
            // profile name → declared member slice IRIs (from gmeow:sliceProfile).
            let mut declared: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for s in &slices {
                for name in &s.profiles {
                    declared
                        .entry(name.clone())
                        .or_default()
                        .push(s.iri.clone());
                }
            }
            // profile name → full membership closure over sliceDependsOn.
            let closures: BTreeMap<String, BTreeSet<String>> = declared
                .iter()
                .map(|(name, members)| {
                    let mut closed: BTreeSet<String> = BTreeSet::new();
                    let mut frontier: Vec<String> = members.clone();
                    while let Some(iri) = frontier.pop() {
                        if !closed.insert(iri.clone()) {
                            continue;
                        }
                        if let Some(deps) = depends.get(iri.as_str()) {
                            frontier.extend(deps.iter().cloned());
                        }
                    }
                    (name.clone(), closed)
                })
                .collect();
            for t in &mut terms {
                let mut profiles: Vec<String> = closures
                    .iter()
                    .filter(|(_, closed)| closed.contains(&t.owner_slice))
                    .map(|(name, _)| name.clone())
                    .collect();
                profiles.push(FULL_PROFILE_NAME.to_string());
                profiles.sort();
                profiles.dedup();
                t.profiles = profiles;
            }
        }

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

        // ── Mapping sets + linkages (parsed from each slice's Mapping artifacts) ─
        let mut mapping_sets: Vec<DocMappingSet> = Vec::new();
        let mut linkages: Vec<DocLinkage> = Vec::new();
        for record in catalog.records() {
            let owner = &record.manifest.slice_iri;
            for artifact in &record.artifacts {
                if artifact.role != ArtifactRole::Mapping {
                    continue;
                }
                let Ok(store) = parse_turtle_lenient(&artifact.content) else {
                    continue;
                };
                let (sets, links) = extract_mappings(&store, owner);
                mapping_sets.extend(sets);
                linkages.extend(links);
            }
        }
        // Resolve each linkage's mapping_set IRI from its sssom_file, then count.
        let set_by_file: BTreeMap<String, String> = mapping_sets
            .iter()
            .filter_map(|s| s.sssom_file.clone().map(|f| (f, s.iri.clone())))
            .collect();
        for link in &mut linkages {
            if let Some(file) = link.mapping_set.clone() {
                link.mapping_set = set_by_file.get(&file).cloned().or(Some(file));
            }
        }
        for set in &mut mapping_sets {
            set.equivalence_count = linkages
                .iter()
                .filter(|l| l.mapping_set.as_deref() == Some(set.iri.as_str()))
                .count();
        }
        mapping_sets.sort_by(|a, b| a.iri.cmp(&b.iri));
        mapping_sets.dedup_by(|a, b| a.iri == b.iri);
        linkages.sort_by(|a, b| {
            a.subject
                .cmp(&b.subject)
                .then_with(|| a.predicate.cmp(&b.predicate))
                .then_with(|| a.object.cmp(&b.object))
                .then_with(|| a.owner_slice.cmp(&b.owner_slice))
        });

        // ── Examples (carried in full from each slice's Example artifacts) ──────
        let mut examples: Vec<DocExample> = Vec::new();
        for record in catalog.records() {
            let owner = &record.manifest.slice_iri;
            for artifact in &record.artifacts {
                if artifact.role != ArtifactRole::Example {
                    continue;
                }
                examples.push(extract_example(artifact, owner));
            }
        }
        examples.sort_by(|a, b| {
            a.slice
                .cmp(&b.slice)
                .then_with(|| a.logical_path.cmp(&b.logical_path))
        });

        // ── SHACL shapes (reverse-mapped from each slice's shapes.ttl) ──────────
        let mut shapes: Vec<DocShape> = Vec::new();
        for record in catalog.records() {
            let owner = &record.manifest.slice_iri;
            for artifact in &record.artifacts {
                if artifact.role != ArtifactRole::Shapes {
                    continue;
                }
                let store = parse_turtle_lenient(&artifact.content).unwrap_or_else(|e| {
                    panic!("shapes.ttl for slice {owner} failed to parse: {e}")
                });
                shapes.extend(extract_shapes(&store, owner));
            }
        }
        sort_dedup_shapes(&mut shapes);

        // ── Competency questions (reverse-mapped from each slice's competency.ttl) ─
        let mut competencies: Vec<DocCompetency> = Vec::new();
        for record in catalog.records() {
            let owner = &record.manifest.slice_iri;
            for artifact in &record.artifacts {
                // The CQ data overlay lives under tests/competency.ttl, carried as
                // a TestDsl artifact (there is no dedicated role for the .ttl).
                if artifact.role != ArtifactRole::TestDsl
                    || !artifact.logical_path.ends_with("competency.ttl")
                {
                    continue;
                }
                let store = parse_turtle_lenient(&artifact.content).unwrap_or_else(|e| {
                    panic!("competency.ttl for slice {owner} failed to parse: {e}")
                });
                competencies.extend(extract_competency(&store, owner));
            }
        }
        competencies.sort_by(|a, b| a.iri.cmp(&b.iri));
        competencies.dedup_by(|a, b| a.iri == b.iri);

        // ── Concerns (collected from module graphs via gmeow:docsConcern) ──────
        let concerns = extract_concerns(catalog);

        // ── External terms (linkage objects + non-GMEOW term edges) ────────────
        let external_terms = extract_external_terms(&terms, &linkages);

        // ── Guides: recipes + learning paths (parsed from module graphs) ───────
        let (recipes, learning_paths) = extract_guides(catalog);

        // ── Translations (built from each slice's i18n/<lang>.po catalogs) ──────
        let translations = Translations::from_catalog(catalog);
        let available_languages = i18n::available_languages(&translations);

        Self {
            title: "GMEOW Ontology Documentation".to_string(),
            version: Self::VERSION.to_string(),
            slices,
            terms,
            dependency_edges,
            mapping_sets,
            linkages,
            examples,
            shapes,
            competencies,
            concerns,
            external_terms,
            recipes,
            learning_paths,
            four_boxes: None,
            concept_doi: None,
            available_languages,
            translations,
            ui_catalog: UiCatalog::default(),
        }
    }

    /// Discover the slice catalog under `root/slices`, run ownership analysis,
    /// and build the model. Also reads the curated `<root>/docs/four-boxes.md`
    /// prose at build time, if present.
    pub fn discover(root: &Path) -> Result<Self, DocsError> {
        let catalog = SliceCatalog::discover(&root.join("slices"))?;
        let ownership = OwnershipAnalyzer::new(&catalog).analyze()?;
        let mut model = Self::from_catalog(&catalog, &ownership);
        model.four_boxes = std::fs::read_to_string(root.join("docs/four-boxes.md")).ok();
        // Concept DOI for the per-term citation block (#1026): the
        // `dcterms:identifier` on the `gmeow:Work` subject of the self-description.
        model.concept_doi = read_concept_doi(root);
        // Root-level SHACL shapes (`<root>/shapes/*.ttl`) — aggregate node shapes
        // not owned by any single slice — merged into the slice-level shapes and
        // deduped by (target term, messages).
        merge_root_shapes(&mut model, &root.join("shapes"));
        // Optional UI-chrome overrides: `<root>/i18n/ontology-docs-templates.<lang>.po`.
        model.ui_catalog = UiCatalog::from_dir(&root.join("i18n"));
        Ok(model)
    }
}

/// Read the ontology's concept DOI from `<root>/metadata/gmeow-self.ttl`: the
/// `dcterms:identifier` literal on the `gmeow:Work` subject (#1026). Returns
/// `None` if the file is absent, unparsable, or carries no Work DOI — the
/// citation block degrades to the term-IRI permalink alone.
fn read_concept_doi(root: &Path) -> Option<String> {
    let bytes = std::fs::read(root.join("metadata/gmeow-self.ttl")).ok()?;
    let store = parse_turtle_lenient(&bytes).ok()?;
    let work = store
        .quads_for_pattern(
            None,
            Some(named(RDF_TYPE).as_ref()),
            Some(named(GMEOW_WORK).as_ref().into()),
            Some(GraphNameRef::DefaultGraph),
        )
        .flatten()
        .find_map(|q| match q.subject {
            NamedOrBlankNode::NamedNode(n) => Some(n),
            _ => None,
        })?;
    first_literal(&store, work.as_str(), DCTERMS_IDENTIFIER)
}

// ── Turtle parsing + term extraction ──────────────────────────────────────────

/// Parse Turtle bytes into an oxigraph store via the native codecs (the
/// gmeow-gts codecs are lenient on GMEOW's `@x-gmeow-*` language tags). The store
/// is kept for the term-extraction queries (SPARQL/pattern matching, scope-OUT of
/// #909); only the TEXT parse is routed natively.
pub(crate) fn parse_turtle_lenient(bytes: &[u8]) -> Result<Store, SliceError> {
    let dataset = gmeow_rdf::parse_dataset(bytes, "text/turtle", None)
        .map_err(|e| SliceError::Parse(format!("syntax error: {e}")))?;
    gmeow_rdf::oxigraph::store_from_dataset(
        &dataset,
        gmeow_rdf::oxigraph::GraphPolicy::PreserveNamedGraphs,
    )
    .map_err(|e| SliceError::Parse(format!("store build failed: {e}")))
}

/// Extract documented terms (GMEOW-namespaced typed subjects) from a module store.
fn extract_terms(store: &Store, owner_slice: &str, tier: Option<&SliceTier>) -> Vec<DocTerm> {
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

        // Per-term usage advice (English carrier from this slice's module.ttl).
        let scope_notes = literals(store, &iri, SKOS_SCOPE_NOTE);
        let examples = literals(store, &iri, SKOS_EXAMPLE);
        let use_when = literals(store, &iri, GMEOW_USE_WHEN);
        let avoid_when = literals(store, &iri, GMEOW_AVOID_WHEN);
        let how_to_use = literals(store, &iri, GMEOW_HOW_TO_USE);
        let use_for_consumer = curie_objects(store, &iri, GMEOW_USE_FOR_CONSUMER);
        let avoid_for_consumer = curie_objects(store, &iri, GMEOW_AVOID_FOR_CONSUMER);

        // Logic stereotypes: co-asserted `rdf:type` values under the logic NS.
        let logic_stereotypes = logic_stereotypes(store, &iri);

        // Related terms: union of skos:related + gmeow:pairsWith + rdfs:seeAlso
        // (IRIs; resolved bidirectionally in `from_catalog`).
        let mut related_terms = named_objects(store, &iri, SKOS_RELATED);
        related_terms.extend(named_objects(store, &iri, GMEOW_PAIRS_WITH));
        related_terms.extend(named_objects(store, &iri, RDFS_SEE_ALSO));
        related_terms.sort();
        related_terms.dedup();

        // Four-boxes role: the lowest-sorted gmeow:graphBoxRole CURIE.
        let box_role = curie_objects(store, &iri, GMEOW_GRAPH_BOX_ROLE)
            .into_iter()
            .next();

        // Per-term lifecycle (#1026): maturity badge (fully resolved with the
        // owner-slice tier in hand), added-in version, and reified changelog.
        let stability = resolve_stability(store, &iri, tier);
        let added_in_version = first_literal(store, &iri, GMEOW_ADDED_IN_VERSION);
        let changelog = extract_changelog(store, &iri);

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
            scope_notes,
            examples,
            use_when,
            avoid_when,
            how_to_use,
            use_for_consumer,
            avoid_for_consumer,
            logic_stereotypes,
            related_terms,
            box_role,
            formalized_by: Vec::new(),
            stability,
            added_in_version,
            changelog,
            // Profile membership needs the full slice set; computed in
            // `from_catalog`'s second pass.
            profiles: Vec::new(),
        });
    }
    terms
}

/// Resolve a term's stability badge (#1026): an explicit `gmeow:termStability`
/// annotation wins (the lowest-sorted CURIE when multiply asserted, a
/// deterministic and conservative tiebreak); else `owl:deprecated true` →
/// Deprecated; else the owner-slice tier default (extension → Experimental,
/// everything else → Stable).
fn resolve_stability(store: &Store, iri: &str, tier: Option<&SliceTier>) -> DocTermStability {
    if let Some(curie) = curie_objects(store, iri, GMEOW_TERM_STABILITY)
        .into_iter()
        .next()
    {
        match curie.as_str() {
            STABILITY_STABLE_CURIE => return DocTermStability::Stable,
            STABILITY_EXPERIMENTAL_CURIE => return DocTermStability::Experimental,
            STABILITY_DEPRECATED_CURIE => return DocTermStability::Deprecated,
            // An unrecognized value falls through to the derived default rather
            // than guessing — keeps the badge total without inventing a status.
            _ => {}
        }
    }
    if literals(store, iri, OWL_DEPRECATED)
        .iter()
        .any(|v| v == "true")
    {
        return DocTermStability::Deprecated;
    }
    match tier {
        Some(SliceTier::Extension) => DocTermStability::Experimental,
        _ => DocTermStability::Stable,
    }
}

/// Extract a term's reified changelog entries (#1026): each
/// `?term gmeow:hasChangelogEntry ?entry` whose `?entry` carries a
/// `gmeow:entryVersion` (required) and optional `gmeow:entryNote`. Sorted by
/// `(version, note)`; oxigraph blank-node iteration order is not stable.
fn extract_changelog(store: &Store, iri: &str) -> Vec<DocChangelogEntry> {
    let Ok(subject) = oxigraph::model::NamedNode::new(iri) else {
        return Vec::new();
    };
    let mut entries: Vec<DocChangelogEntry> = Vec::new();
    for quad in store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(named(GMEOW_HAS_CHANGELOG_ENTRY).as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .flatten()
    {
        let entry_node = match quad.object {
            Term::NamedNode(n) => NamedOrBlankNode::NamedNode(n),
            Term::BlankNode(b) => NamedOrBlankNode::BlankNode(b),
            _ => continue,
        };
        let version = first_literal_of(store, entry_node.as_ref(), GMEOW_ENTRY_VERSION);
        let note = first_literal_of(store, entry_node.as_ref(), GMEOW_ENTRY_NOTE);
        if let Some(version) = version {
            entries.push(DocChangelogEntry { version, note });
        }
    }
    entries.sort();
    entries.dedup();
    entries
}

/// The logic stereotypes of a subject: its `rdf:type` values under the `logic:`
/// namespace, rendered as `logic:`-prefixed CURIEs (sorted/deduped).
fn logic_stereotypes(store: &Store, subject: &str) -> Vec<String> {
    let mut out: Vec<String> = named_objects(store, subject, RDF_TYPE)
        .into_iter()
        .filter(|t| t.starts_with(LOGIC_NS))
        .map(|t| format!("logic:{}", local_name(&t)))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// All `(subject, target)` pairs of `?s logic:formalizes ?target` in the store
/// (named subjects + named targets only).
fn extract_formalizes(store: &Store) -> Vec<(String, String)> {
    store
        .quads_for_pattern(
            None,
            Some(named(LOGIC_FORMALIZES).as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .flatten()
        .filter_map(|q| {
            let NamedOrBlankNode::NamedNode(subject) = &q.subject else {
                return None;
            };
            match q.object {
                Term::NamedNode(target) => {
                    Some((subject.as_str().to_string(), target.as_str().to_string()))
                }
                _ => None,
            }
        })
        .collect()
}

/// Extract SHACL node shapes reverse-mapped to the GMEOW terms they target.
/// Each `sh:targetClass` / `sh:targetSubjectsOf` / `sh:targetObjectsOf` edge to a
/// GMEOW IRI yields a [`DocShape`] carrying every `sh:message` reachable within
/// the shape (its nested property shapes and `sh:or` lists).
fn extract_shapes(store: &Store, owner_slice: &str) -> Vec<DocShape> {
    let mut out = Vec::new();
    for target_pred in [SH_TARGET_CLASS, SH_TARGET_SUBJECTS_OF, SH_TARGET_OBJECTS_OF] {
        for quad in store
            .quads_for_pattern(
                None,
                Some(named(target_pred).as_ref()),
                None,
                Some(GraphNameRef::DefaultGraph),
            )
            .flatten()
        {
            let Term::NamedNode(target) = &quad.object else {
                continue;
            };
            let target_term = target.as_str().to_string();
            // Term pages exist only for GMEOW terms — only those are documentable.
            if !target_term.starts_with(GMEOW_NS) {
                continue;
            }
            let messages = shape_messages(store, &quad.subject);
            let shape_iri = match &quad.subject {
                NamedOrBlankNode::NamedNode(n) => n.as_str().to_string(),
                NamedOrBlankNode::BlankNode(b) => format!("_:{}", b.as_str()),
            };
            out.push(DocShape {
                shape_iri,
                target_term,
                messages,
                owner_slice: owner_slice.to_string(),
            });
        }
    }
    out
}

/// Sort SHACL shapes by `(target term, messages, shape IRI)` and dedup
/// shapes that target the same term with the same messages (root + slice copies).
fn sort_dedup_shapes(shapes: &mut Vec<DocShape>) {
    shapes.sort_by(|a, b| {
        a.target_term
            .cmp(&b.target_term)
            .then_with(|| a.messages.cmp(&b.messages))
            .then_with(|| a.shape_iri.cmp(&b.shape_iri))
    });
    shapes.dedup_by(|a, b| a.target_term == b.target_term && a.messages == b.messages);
}

/// Read the root `<root>/shapes/*.ttl` files, extract their node shapes, and merge
/// them (deduped) into the model's shapes. A missing `shapes/` directory is a
/// no-op; a present-but-unparsable file is a hard fault.
fn merge_root_shapes(model: &mut DocsModel, shapes_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(shapes_dir) else {
        return;
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ttl"))
        .collect();
    paths.sort();
    for path in paths {
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("root shapes file {} unreadable: {e}", path.display()));
        let store = parse_turtle_lenient(&bytes)
            .unwrap_or_else(|e| panic!("root shapes file {} failed to parse: {e}", path.display()));
        model.shapes.extend(extract_shapes(&store, "root"));
    }
    sort_dedup_shapes(&mut model.shapes);
}

/// Every `sh:message` literal reachable from a shape subject by walking blank-node
/// objects (property shapes, `sh:or` / `sh:and` lists), sorted/deduped. Named-node
/// objects are NOT followed (they point out into the wider graph).
fn shape_messages(store: &Store, start: &NamedOrBlankNode) -> Vec<String> {
    use std::collections::{HashSet, VecDeque};
    let mut seen: HashSet<NamedOrBlankNode> = HashSet::new();
    let mut queue: VecDeque<NamedOrBlankNode> = VecDeque::from([start.clone()]);
    let mut msgs: Vec<String> = Vec::new();
    while let Some(node) = queue.pop_front() {
        if !seen.insert(node.clone()) {
            continue;
        }
        for quad in store
            .quads_for_pattern(
                Some(node.as_ref()),
                None,
                None,
                Some(GraphNameRef::DefaultGraph),
            )
            .flatten()
        {
            let is_message = quad.predicate.as_str() == SH_MESSAGE;
            match quad.object {
                Term::Literal(lit) if is_message => msgs.push(lit.value().to_string()),
                Term::BlankNode(b) => queue.push_back(NamedOrBlankNode::BlankNode(b)),
                _ => {}
            }
        }
    }
    msgs.sort();
    msgs.dedup();
    msgs
}

/// Extract competency questions reverse-mapped to the terms they exercise. The
/// terms are reached via `gmeow:cqExpectRow → gmeow:rowCell → gmeow:cellValueIri`.
fn extract_competency(store: &Store, owner_slice: &str) -> Vec<DocCompetency> {
    let mut out = Vec::new();
    for cq in subjects_of_type(store, GMEOW_COMPETENCY_QUESTION) {
        let rationale = first_literal(store, &cq, GMEOW_CQ_RATIONALE);
        let query_file = first_literal(store, &cq, GMEOW_CQ_QUERY_FILE);
        let mut exercises: Vec<String> = Vec::new();
        for row in named_objects(store, &cq, GMEOW_CQ_EXPECT_ROW) {
            for cell in blank_objects(store, &row, GMEOW_ROW_CELL) {
                for quad in store
                    .quads_for_pattern(
                        Some(cell.as_ref().into()),
                        Some(named(GMEOW_CELL_VALUE_IRI).as_ref()),
                        None,
                        Some(GraphNameRef::DefaultGraph),
                    )
                    .flatten()
                {
                    if let Term::NamedNode(v) = quad.object {
                        exercises.push(v.as_str().to_string());
                    }
                }
            }
        }
        exercises.sort();
        exercises.dedup();
        out.push(DocCompetency {
            iri: cq,
            rationale,
            query_file,
            exercises,
            owner_slice: owner_slice.to_string(),
        });
    }
    out
}

/// All BlankNode objects of `subject predicate ?o` in the default graph.
fn blank_objects(store: &Store, subject: &str, predicate: &str) -> Vec<oxigraph::model::BlankNode> {
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
            Term::BlankNode(b) => Some(b),
            _ => None,
        })
        .collect()
}

/// Extract mapping sets + term equivalences from a `mappings/*.ttl` store.
fn extract_mappings(store: &Store, owner_slice: &str) -> (Vec<DocMappingSet>, Vec<DocLinkage>) {
    let mut sets = Vec::new();
    for iri in subjects_of_type(store, GMEOW_MAPPING_SET) {
        sets.push(DocMappingSet {
            curie: to_curie(&iri),
            set_id: first_literal(store, &iri, GMEOW_SET_ID),
            sssom_file: first_literal(store, &iri, GMEOW_SSSOM_FILE),
            license: first_literal(store, &iri, GMEOW_LICENSE),
            comment: first_literal(store, &iri, GMEOW_SET_COMMENT),
            owner_slice: owner_slice.to_string(),
            equivalence_count: 0,
            iri,
        });
    }

    let mut links = Vec::new();
    for iri in subjects_of_type(store, GMEOW_TERM_EQUIVALENCE) {
        let Some(subject) = named_objects(store, &iri, GMEOW_ALIGN_SUBJECT)
            .into_iter()
            .next()
        else {
            continue;
        };
        let Some(predicate) = named_objects(store, &iri, GMEOW_ALIGN_PREDICATE)
            .into_iter()
            .next()
        else {
            continue;
        };
        let Some(object) = named_objects(store, &iri, GMEOW_ALIGN_OBJECT)
            .into_iter()
            .next()
        else {
            continue;
        };
        // The justification is usually a NamedNode (semapv:…); accept literal too.
        let justification = named_objects(store, &iri, GMEOW_JUSTIFICATION)
            .into_iter()
            .next()
            .map(|j| to_curie(&j))
            .or_else(|| first_literal(store, &iri, GMEOW_JUSTIFICATION));
        let confidence =
            first_literal(store, &iri, GMEOW_CONFIDENCE).and_then(|v| v.trim().parse::<f64>().ok());
        links.push(DocLinkage {
            mapping_set: first_literal(store, &iri, GMEOW_SSSOM_FILE),
            subject_curie: to_curie(&subject),
            subject,
            predicate: to_curie(&predicate),
            object,
            justification,
            confidence,
            owner_slice: owner_slice.to_string(),
        });
    }
    (sets, links)
}

/// Extract a single example, carrying its Turtle source in full.
fn extract_example(artifact: &ArtifactRecord, owner_slice: &str) -> DocExample {
    let text = String::from_utf8_lossy(&artifact.content).into_owned();
    let logical_path = artifact.logical_path.clone();

    // Title: first rdfs:label on any subject, else the filename stem.
    let title = parse_turtle_lenient(&artifact.content)
        .ok()
        .and_then(|store| {
            store
                .quads_for_pattern(
                    None,
                    Some(named(RDFS_LABEL).as_ref()),
                    None,
                    Some(GraphNameRef::DefaultGraph),
                )
                .flatten()
                .filter_map(|q| match q.object {
                    Term::Literal(lit) => Some(lit.value().to_string()),
                    _ => None,
                })
                .min()
        })
        .unwrap_or_else(|| filename_title(&logical_path));

    // Terms referenced: every gmeow: CURIE appearing as a NamedNode anywhere.
    let mut terms_referenced: Vec<String> = parse_turtle_lenient(&artifact.content)
        .ok()
        .map(|store| {
            let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            for quad in store.iter().flatten() {
                if let NamedOrBlankNode::NamedNode(s) = &quad.subject {
                    if s.as_str().starts_with(GMEOW_NS) {
                        set.insert(to_curie(s.as_str()));
                    }
                }
                if let Term::NamedNode(o) = &quad.object {
                    if o.as_str().starts_with(GMEOW_NS) {
                        set.insert(to_curie(o.as_str()));
                    }
                }
            }
            set.into_iter().collect()
        })
        .unwrap_or_default();
    terms_referenced.sort();
    terms_referenced.dedup();

    DocExample {
        slice: owner_slice.to_string(),
        logical_path,
        title,
        text,
        terms_referenced,
    }
}

/// A human title derived from a logical path's filename stem (kebab → Title).
fn filename_title(logical_path: &str) -> String {
    let stem = logical_path
        .rsplit('/')
        .next()
        .unwrap_or(logical_path)
        .trim_end_matches(".ttl")
        .trim_end_matches(".trig")
        .trim_end_matches(".nq");
    let mut out = String::with_capacity(stem.len());
    let mut new_word = true;
    for ch in stem.chars() {
        if ch == '-' || ch == '_' {
            out.push(' ');
            new_word = true;
        } else if new_word {
            out.extend(ch.to_uppercase());
            new_word = false;
        } else {
            out.push(ch);
        }
    }
    if out.is_empty() {
        stem.to_string()
    } else {
        out
    }
}

/// Collect documentation concerns from every module graph: the concern
/// individuals (`a gmeow:DocumentationConcern`) and the terms that declare each
/// via `gmeow:docsConcern`.
fn extract_concerns(catalog: &SliceCatalog) -> Vec<DocConcern> {
    // First pass: concern identity (label/definition), keyed by IRI.
    let mut iri_label: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut iri_def: BTreeMap<String, Option<String>> = BTreeMap::new();
    // Second pass aggregates: concern IRI → (terms curies, slice iris).
    let mut concern_terms: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    let mut concern_slices: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();

    for record in catalog.records() {
        let owner = &record.manifest.slice_iri;
        for artifact in &record.artifacts {
            if artifact.role != ArtifactRole::Module {
                continue;
            }
            let Ok(store) = parse_turtle_lenient(&artifact.content) else {
                continue;
            };
            for iri in subjects_of_type(&store, GMEOW_DOCUMENTATION_CONCERN) {
                iri_label
                    .entry(iri.clone())
                    .or_insert_with(|| first_literal(&store, &iri, RDFS_LABEL));
                iri_def.entry(iri.clone()).or_insert_with(|| {
                    first_literal(&store, &iri, SKOS_DEFINITION)
                        .or_else(|| first_literal(&store, &iri, RDFS_COMMENT))
                });
                concern_terms.entry(iri.clone()).or_default();
                concern_slices.entry(iri).or_default();
            }
            // Every `term gmeow:docsConcern concern` edge.
            for quad in store
                .quads_for_pattern(
                    None,
                    Some(named(GMEOW_DOCS_CONCERN).as_ref()),
                    None,
                    Some(GraphNameRef::DefaultGraph),
                )
                .flatten()
            {
                let (NamedOrBlankNode::NamedNode(subject), Term::NamedNode(concern)) =
                    (&quad.subject, &quad.object)
                else {
                    continue;
                };
                let concern = concern.as_str().to_string();
                if subject.as_str().starts_with(GMEOW_NS) {
                    concern_terms
                        .entry(concern.clone())
                        .or_default()
                        .insert(to_curie(subject.as_str()));
                }
                concern_slices
                    .entry(concern)
                    .or_default()
                    .insert(owner.clone());
            }
        }
    }

    let mut concerns: Vec<DocConcern> = iri_label
        .keys()
        .map(|iri| {
            let terms: Vec<String> = concern_terms
                .get(iri)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();
            let slices: Vec<String> = concern_slices
                .get(iri)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();
            DocConcern {
                curie: to_curie(iri),
                label: iri_label.get(iri).cloned().flatten(),
                definition: iri_def.get(iri).cloned().flatten(),
                terms,
                slices,
                iri: iri.clone(),
            }
        })
        .collect();
    concerns.sort_by(|a, b| a.iri.cmp(&b.iri));
    concerns
}

/// Derive the external-term overview: every non-GMEOW IRI referenced by a
/// linkage object or by a term's parents / domain / range, grouped by namespace.
fn extract_external_terms(terms: &[DocTerm], linkages: &[DocLinkage]) -> Vec<DocExternalTerm> {
    // external IRI → (referencing gmeow curies, predicates)
    let mut by_iri: BTreeMap<
        String,
        (
            std::collections::BTreeSet<String>,
            std::collections::BTreeSet<String>,
        ),
    > = BTreeMap::new();

    let mut record = |iri: &str, by: &str, via: &str| {
        if iri.starts_with(GMEOW_NS) || !is_external_iri(iri) {
            return;
        }
        let entry = by_iri.entry(iri.to_string()).or_default();
        entry.0.insert(by.to_string());
        entry.1.insert(via.to_string());
    };

    for link in linkages {
        record(&link.object, &link.subject_curie, "alignObject");
    }
    for term in terms {
        for parent in &term.parents {
            record(parent, &term.curie, "subClassOf");
        }
        for d in &term.domain {
            record(d, &term.curie, "domain");
        }
        for r in &term.range {
            record(r, &term.curie, "range");
        }
    }

    let mut out: Vec<DocExternalTerm> = by_iri
        .into_iter()
        .map(|(iri, (referenced_by, via_predicate))| DocExternalTerm {
            namespace: namespace_of(&iri),
            iri,
            referenced_by: referenced_by.into_iter().collect(),
            via_predicate: via_predicate.into_iter().collect(),
        })
        .collect();
    out.sort_by(|a, b| a.iri.cmp(&b.iri));
    out
}

/// Extract the curated recipes + learning paths from every module graph that
/// carries them (the guides slice). Recipes are parsed first so a learning
/// path's `gmeow:includesRecipe` IRI edges can be resolved to recipe slugs.
/// Both lists are returned sorted by slug.
fn extract_guides(catalog: &SliceCatalog) -> (Vec<DocRecipe>, Vec<DocLearningPath>) {
    // Recipe IRI → slug, so includesRecipe edges resolve to slugs.
    let mut recipe_slug_by_iri: BTreeMap<String, String> = BTreeMap::new();
    let mut recipes: Vec<(String, DocRecipe)> = Vec::new();

    // Stash learning-path raw parses (with includesRecipe IRIs) for a second
    // resolution pass once every recipe IRI→slug is known.
    struct RawPath {
        slug: String,
        title: String,
        audience: String,
        goal: String,
        recipe_iris: Vec<String>,
        example_paths: Vec<String>,
        term_curies: Vec<String>,
        adoption_targets: Vec<String>,
    }
    let mut raw_paths: Vec<RawPath> = Vec::new();

    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role != ArtifactRole::Module {
                continue;
            }
            let Ok(store) = parse_turtle_lenient(&artifact.content) else {
                continue;
            };

            for iri in subjects_of_type(&store, GMEOW_RECIPE) {
                let slug = first_literal(&store, &iri, GMEOW_GUIDE_SLUG)
                    .unwrap_or_else(|| local_name(&iri).to_string());
                let title = first_literal(&store, &iri, GMEOW_GUIDE_TITLE)
                    .or_else(|| first_literal(&store, &iri, RDFS_LABEL))
                    .unwrap_or_else(|| slug.clone());
                let goal = first_literal(&store, &iri, GMEOW_GUIDE_GOAL).unwrap_or_default();
                let example_paths = sorted_literals(&store, &iri, GMEOW_USES_EXAMPLE_PATH);
                let term_curies = sorted_curie_objects(&store, &iri, GMEOW_USES_TERM);
                let follow_pages = sorted_literals(&store, &iri, GMEOW_FOLLOWS_GUIDE_PATH);
                recipe_slug_by_iri.insert(iri.clone(), slug.clone());
                recipes.push((
                    slug.clone(),
                    DocRecipe {
                        slug,
                        title,
                        goal,
                        example_paths,
                        term_curies,
                        follow_pages,
                    },
                ));
            }

            for iri in subjects_of_type(&store, GMEOW_LEARNING_PATH) {
                let slug = first_literal(&store, &iri, GMEOW_GUIDE_SLUG)
                    .unwrap_or_else(|| local_name(&iri).to_string());
                let title = first_literal(&store, &iri, GMEOW_GUIDE_TITLE)
                    .or_else(|| first_literal(&store, &iri, RDFS_LABEL))
                    .unwrap_or_else(|| slug.clone());
                let audience =
                    first_literal(&store, &iri, GMEOW_LEARNING_AUDIENCE).unwrap_or_default();
                let goal = first_literal(&store, &iri, GMEOW_GUIDE_GOAL).unwrap_or_default();
                let mut recipe_iris = named_objects(&store, &iri, GMEOW_INCLUDES_RECIPE);
                recipe_iris.sort();
                recipe_iris.dedup();
                let example_paths = sorted_literals(&store, &iri, GMEOW_USES_EXAMPLE_PATH);
                let term_curies = sorted_curie_objects(&store, &iri, GMEOW_USES_TERM);
                let adoption_targets = sorted_literals(&store, &iri, GMEOW_ADOPTION_TARGET);
                raw_paths.push(RawPath {
                    slug,
                    title,
                    audience,
                    goal,
                    recipe_iris,
                    example_paths,
                    term_curies,
                    adoption_targets,
                });
            }
        }
    }

    let mut learning_paths: Vec<DocLearningPath> = raw_paths
        .into_iter()
        .map(|p| {
            let mut recipe_slugs: Vec<String> = p
                .recipe_iris
                .iter()
                .map(|iri| {
                    recipe_slug_by_iri
                        .get(iri)
                        .cloned()
                        .unwrap_or_else(|| local_name(iri).to_string())
                })
                .collect();
            recipe_slugs.sort();
            recipe_slugs.dedup();
            DocLearningPath {
                slug: p.slug,
                title: p.title,
                audience: p.audience,
                goal: p.goal,
                recipe_slugs,
                example_paths: p.example_paths,
                term_curies: p.term_curies,
                adoption_targets: p.adoption_targets,
            }
        })
        .collect();

    let mut recipes: Vec<DocRecipe> = recipes.into_iter().map(|(_, r)| r).collect();
    recipes.sort_by(|a, b| a.slug.cmp(&b.slug));
    recipes.dedup_by(|a, b| a.slug == b.slug);
    learning_paths.sort_by(|a, b| a.slug.cmp(&b.slug));
    learning_paths.dedup_by(|a, b| a.slug == b.slug);
    (recipes, learning_paths)
}

/// All literal objects of `subject predicate ?o`, sorted + deduped.
fn sorted_literals(store: &Store, subject: &str, predicate: &str) -> Vec<String> {
    let Ok(subject) = oxigraph::model::NamedNode::new(subject) else {
        return Vec::new();
    };
    let mut out: Vec<String> = store
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
    out.sort();
    out.dedup();
    out
}

/// All NamedNode objects of `subject predicate ?o` rendered as CURIEs, sorted +
/// deduped (used for `gmeow:usesTerm` term references).
fn sorted_curie_objects(store: &Store, subject: &str, predicate: &str) -> Vec<String> {
    let mut out: Vec<String> = named_objects(store, subject, predicate)
        .iter()
        .map(|iri| to_curie(iri))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The local name of an IRI: the tail after the last `/` or `#`.
fn local_name(iri: &str) -> &str {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    &iri[cut..]
}

/// Whether an IRI is an http(s) external reference (excludes bnodes / non-IRIs).
fn is_external_iri(iri: &str) -> bool {
    iri.starts_with("http://") || iri.starts_with("https://")
}

/// The namespace of an IRI: everything up to and including the last `/` or `#`.
fn namespace_of(iri: &str) -> String {
    match iri.rfind(['/', '#']) {
        Some(i) => iri[..=i].to_string(),
        None => iri.to_string(),
    }
}

/// All NamedNode subjects of `?s a <type>` in the default graph (sorted, deduped).
fn subjects_of_type(store: &Store, type_iri: &str) -> Vec<String> {
    let mut out: Vec<String> = store
        .quads_for_pattern(
            None,
            Some(named(RDF_TYPE).as_ref()),
            Some(named(type_iri).as_ref().into()),
            Some(GraphNameRef::DefaultGraph),
        )
        .flatten()
        .filter_map(|q| match q.subject {
            NamedOrBlankNode::NamedNode(n) => Some(n.as_str().to_string()),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
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

/// The first literal value for `subject predicate ?o` where `subject` is a
/// named-or-blank node (deterministic: lowest lexical form), or `None`. Used to
/// read reified changelog-entry blank nodes (#1026).
fn first_literal_of(
    store: &Store,
    subject: oxigraph::model::NamedOrBlankNodeRef,
    predicate: &str,
) -> Option<String> {
    let mut values: Vec<String> = store
        .quads_for_pattern(
            Some(subject),
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

/// All literal values for `subject predicate ?o`, sorted and deduped
/// (deterministic; carries the English-carrier text from `module.ttl`).
fn literals(store: &Store, subject: &str, predicate: &str) -> Vec<String> {
    let Ok(subject) = oxigraph::model::NamedNode::new(subject) else {
        return Vec::new();
    };
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
    values.dedup();
    values
}

/// Named object IRIs for `subject predicate ?o` as CURIEs, sorted and deduped.
fn curie_objects(store: &Store, subject: &str, predicate: &str) -> Vec<String> {
    let mut values: Vec<String> = named_objects(store, subject, predicate)
        .iter()
        .map(|iri| to_curie(iri))
        .collect();
    values.sort();
    values.dedup();
    values
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
        let terms = extract_terms(&store, "https://example.org/slice/zoo", None);

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
        assert!(extract_terms(&store, "s", None).is_empty());
    }

    /// Stability derivation precedence (#1026): explicit `gmeow:termStability`
    /// wins; else `owl:deprecated`; else the owner-slice tier default.
    #[test]
    fn stability_resolves_by_precedence() {
        let ttl = r#"
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:CoreDefault a owl:Class .
gmeow:ExtDefault  a owl:Class .
gmeow:Deprecated  a owl:Class ; owl:deprecated true .
gmeow:Explicit    a owl:Class ;
    owl:deprecated true ;
    gmeow:termStability gmeow:stabilityExperimental .
"#;
        let store = store_from(ttl);
        let core = extract_terms(&store, "s", Some(&SliceTier::Core));
        let by = |ts: &[DocTerm], suffix: &str| {
            ts.iter()
                .find(|t| t.iri.ends_with(suffix))
                .unwrap()
                .stability
        };
        // Core tier → Stable default.
        assert_eq!(by(&core, "CoreDefault"), DocTermStability::Stable);
        // owl:deprecated overrides the tier default.
        assert_eq!(by(&core, "Deprecated"), DocTermStability::Deprecated);
        // Explicit annotation beats owl:deprecated.
        assert_eq!(by(&core, "Explicit"), DocTermStability::Experimental);

        // Same terms under an extension tier → ExtDefault becomes Experimental.
        let ext = extract_terms(&store, "s", Some(&SliceTier::Extension));
        assert_eq!(by(&ext, "ExtDefault"), DocTermStability::Experimental);
    }

    /// Reified changelog entries are parsed from blank nodes and sorted by
    /// `(version, note)` (#1026); `addedInVersion` is the lowest literal.
    #[test]
    fn changelog_entries_parse_and_sort() {
        let ttl = r#"
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:Thing a owl:Class ;
    gmeow:addedInVersion "1.0.2" ;
    gmeow:hasChangelogEntry [
        a gmeow:ChangelogEntry ;
        gmeow:entryVersion "1.1.0" ;
        gmeow:entryNote "Widened range." ] ;
    gmeow:hasChangelogEntry [
        a gmeow:ChangelogEntry ;
        gmeow:entryVersion "1.0.2" ] .
"#;
        let store = store_from(ttl);
        let terms = extract_terms(&store, "s", Some(&SliceTier::Core));
        let thing = terms.iter().find(|t| t.iri.ends_with("Thing")).unwrap();
        assert_eq!(thing.added_in_version.as_deref(), Some("1.0.2"));
        assert_eq!(
            thing.changelog,
            vec![
                DocChangelogEntry {
                    version: "1.0.2".to_string(),
                    note: None,
                },
                DocChangelogEntry {
                    version: "1.1.0".to_string(),
                    note: Some("Widened range.".to_string()),
                },
            ]
        );
    }
}
