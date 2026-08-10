// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, oxigraph-free correspondence soundness pass.
//!
//! This is the wasm-clean home for the seven correspondence-stack semantic checks the
//! retired alignment-direction and FnO back-end lints enforced over the committed
//! alignment surface. Each check is ported VERBATIM — message wording,
//! severity, check/code tokens, instance/subject/predicate/object slots, and the
//! deterministic severity→check→instance sort are preserved exactly so the migration is
//! byte-for-byte equivalent (the parity harness in `crates/pipeline/tests` is the gate).
//!
//! The five **alignment** checks:
//!
//! * [`check_inverse_direction`] — self-contradiction (a property mapped to a term AND
//!   its declared inverse) + the domain/range orientation fallback.
//! * [`check_domain_range`] — a mapping whose GMEOW domain/range is incompatible with
//!   the target term's.
//! * [`check_property_character`] — strong-equivalent mappings with mismatched OWL
//!   property character (object↔datatype kind conflict; functional/transitive/… skew).
//! * [`check_equivalence_collapse`] — **Constitution Principle 5**: no equivalence
//!   closure may connect two declared-disjoint terms.
//! * [`lint_dc_refinement`] — DC refinement consistency + no hand-authored `dc:`.
//!
//! The two **FnO back-end soundness** checks:
//!
//! * [`fno_type_mismatches`] — an `fno:Parameter`/`fno:Output` whose `fno:predicate` is a
//!   GMEOW property with a declared `rdfs:range` must declare an `fno:type` equal to it.
//! * [`fno_reference_integrity`] — every FnO function an EDOAL cell invokes via
//!   `edoal:transformation` must be a defined `fno:Function`.
//!
//! ## Oxigraph-free input sourcing
//!
//! The historical lints queried an `oxigraph::store::Store` with `quads_for_pattern`.
//! Every read here goes through the wasm-clean [`DslView`] over an already-parsed
//! [`purrdf::RdfDataset`] instead (the file-reading + Turtle-parsing edge lives in the
//! caller — the pipeline `correspondence_soundness` stage module — exactly as the four
//! dialect lowerings are driven). The pure pass receives:
//!
//! * the merged ontology view,
//! * the per-prefix target-axiom views (snapshot ⊕ fixture ⊕ optional network fetch),
//! * the SSSOM [`Mapping`] rows (parsed from the committed `generated/mappings/*.sssom.tsv`,
//!   the same source the old check read — for exact parity),
//! * the merged FnO catalog view + the committed EDOAL views.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use gmeow_errors::Diag;

use crate::ingest::dataset::{DslTerm, DslView};
use crate::ingest::prefixes::{ns_to_prefix, registry_iri, sssom_id};
use crate::projections::edoal::template_target_kind;
use crate::projections::get_leg::ProjectionCell;

// ── Predicate / class constants (ported VERBATIM from the retired Python linter) ───

/// Predicate CURIEs whose alignment asserts (near-)equivalence for properties.
pub const STRONG_PROPERTY_PREDICATES: &[&str] = &["owl:equivalentProperty", "skos:exactMatch"];

/// Class-level strong equivalence (the collapse gate's edge set).
pub const STRONG_CLASS_PREDICATES: &[&str] = &["owl:equivalentClass", "skos:exactMatch"];

/// Intentionally directional/hierarchical predicates — exempt from direction checks.
pub const HIERARCHICAL_PREDICATES: &[&str] =
    &["skos:broadMatch", "skos:narrowMatch", "rdfs:subPropertyOf"];

/// Mapping predicates that assert (near-)equivalence and participate in the collapse
/// closure.
pub const COLLAPSE_PREDICATES: &[&str] = &[
    "owl:equivalentClass",
    "owl:equivalentProperty",
    "skos:exactMatch",
];

/// Strength rank used to pick the canonical term in a self-contradicting pair.
pub const PREDICATE_RANK: &[(&str, i32)] = &[
    ("owl:equivalentProperty", 3),
    ("skos:exactMatch", 3),
    ("skos:closeMatch", 1),
];

/// OWL property-character types read from `rdf:type` assertions.
pub const CHARACTER_TYPES: &[&str] = &[
    "http://www.w3.org/2002/07/owl#FunctionalProperty",
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
    "http://www.w3.org/2002/07/owl#TransitiveProperty",
    "http://www.w3.org/2002/07/owl#SymmetricProperty",
];

/// OWL property-typing terms. A target using none of these does not speak the OWL
/// characteristic vocabulary, so a character comparison would be noise.
pub const OWL_PROPERTY_TYPES: &[&str] = &[
    "http://www.w3.org/2002/07/owl#ObjectProperty",
    "http://www.w3.org/2002/07/owl#DatatypeProperty",
    "http://www.w3.org/2002/07/owl#FunctionalProperty",
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
    "http://www.w3.org/2002/07/owl#TransitiveProperty",
    "http://www.w3.org/2002/07/owl#SymmetricProperty",
    "http://www.w3.org/2002/07/owl#AsymmetricProperty",
];

/// dcterms refinements → broader dcterms element (per DCMI specification).
pub const DCTERMS_REFINEMENTS: &[(&str, &str)] = &[
    ("dcterms:abstract", "dcterms:description"),
    ("dcterms:tableOfContents", "dcterms:description"),
    ("dcterms:created", "dcterms:date"),
    ("dcterms:modified", "dcterms:date"),
    ("dcterms:issued", "dcterms:date"),
    ("dcterms:valid", "dcterms:date"),
    ("dcterms:available", "dcterms:date"),
    ("dcterms:dateAccepted", "dcterms:date"),
    ("dcterms:dateCopyrighted", "dcterms:date"),
    ("dcterms:dateSubmitted", "dcterms:date"),
    ("dcterms:references", "dcterms:relation"),
    ("dcterms:isReferencedBy", "dcterms:relation"),
    ("dcterms:requires", "dcterms:relation"),
    ("dcterms:isRequiredBy", "dcterms:relation"),
    ("dcterms:replaces", "dcterms:relation"),
    ("dcterms:isReplacedBy", "dcterms:relation"),
    ("dcterms:hasPart", "dcterms:relation"),
    ("dcterms:isPartOf", "dcterms:relation"),
    ("dcterms:hasVersion", "dcterms:relation"),
    ("dcterms:isVersionOf", "dcterms:relation"),
    ("dcterms:conformsTo", "dcterms:relation"),
    ("dcterms:license", "dcterms:rights"),
    ("dcterms:rightsHolder", "dcterms:rights"),
    ("dcterms:accessRights", "dcterms:rights"),
    ("dcterms:spatial", "dcterms:coverage"),
    ("dcterms:temporal", "dcterms:coverage"),
    ("dcterms:extent", "dcterms:format"),
    ("dcterms:medium", "dcterms:format"),
    ("dcterms:bibliographicCitation", "dcterms:identifier"),
];

/// Grandfathered hand-authored `dc:` alignments.
pub const GRANDFATHERED_DC: &[&str] = &["dc:rights"];

// ── Namespace constants ─────────────────────────────────────────────────────────

const GMEOW_PREFIX: &str = "gmeow:";

/// The grounding namespace, checked alongside `gmeow:`.
///
/// When a domain term is superseded by a grounding term, its alignment cells are
/// RE-KEYED onto the grounding spine rather than dropped. Scoping the direction check to
/// `gmeow:` subjects would silently stop checking those cells at exactly the moment they
/// moved — the alignment would still ship, and nothing would verify its direction again.
/// The `is_property` guard below still applies, so this admits only cells whose subject is
/// a declared property.
const LOGIC_PREFIX: &str = "logic:";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";

const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
/// The remaining OWL 2 object-property subtypes: a term typed with ANY of these (or
/// `OWL_SYMMETRIC_PROPERTY` above), without a co-asserted `owl:ObjectProperty`, is still
/// an object property by OWL 2 semantics — [`owl_kind_edoal`] treats them the same as an
/// explicit `owl:ObjectProperty`.
const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const OWL_INVERSE_FUNCTIONAL_PROPERTY: &str =
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty";
const OWL_REFLEXIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ReflexiveProperty";
const OWL_ASYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";
const OWL_IRREFLEXIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";
/// The object-property subtype markers, for the `owl_kind_edoal` membership test.
const OWL_OBJECT_PROPERTY_SUBTYPES: &[&str] = &[
    OWL_SYMMETRIC_PROPERTY,
    OWL_TRANSITIVE_PROPERTY,
    OWL_INVERSE_FUNCTIONAL_PROPERTY,
    OWL_REFLEXIVE_PROPERTY,
    OWL_ASYMMETRIC_PROPERTY,
    OWL_IRREFLEXIVE_PROPERTY,
];
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_PROPERTY_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#propertyDisjointWith";
const OWL_ALL_DISJOINT_CLASSES: &str = "http://www.w3.org/2002/07/owl#AllDisjointClasses";
const OWL_ALL_DISJOINT_PROPERTIES: &str = "http://www.w3.org/2002/07/owl#AllDisjointProperties";
const OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";

const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";

const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";

const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

// ── logic: property-characteristic carrier ────────────────────────────────────
// GMEOW (authoring-namespace) properties no longer carry `rdf:type
// owl:FunctionalProperty` in source: functionality lives on the canonical
// `logic:PropertyCharacteristicAssertion` carrier (`logic:characterizes ?P` joined with
// `logic:characteristicSort logic:functionalProperty`). The GMEOW-side character read
// below joins this carrier so a functional GMEOW property is still recognized; the
// TARGET/external side continues to read `rdf:type owl:FunctionalProperty` directly, as
// those vocabularies legitimately declare the OWL characteristic.
const LOGIC_CHARACTERIZES: &str = "https://blackcatinformatics.ca/logic/characterizes";
const LOGIC_CHARACTERISTIC_SORT: &str = "https://blackcatinformatics.ca/logic/characteristicSort";
const LOGIC_FUNCTIONAL_PROPERTY: &str = "https://blackcatinformatics.ca/logic/functionalProperty";
const LOGIC_INVERSE_FUNCTIONAL_PROPERTY: &str =
    "https://blackcatinformatics.ca/logic/inverseFunctionalProperty";

const SCHEMA_INVERSE_OF: &str = "https://schema.org/inverseOf";
const SCHEMA_DOMAIN_INCLUDES: &str = "https://schema.org/domainIncludes";
const SCHEMA_RANGE_INCLUDES: &str = "https://schema.org/rangeIncludes";

const FNO_PARAMETER: &str = "https://w3id.org/function/ontology#Parameter";
const FNO_OUTPUT: &str = "https://w3id.org/function/ontology#Output";
const FNO_FUNCTION: &str = "https://w3id.org/function/ontology#Function";
const FNO_PREDICATE: &str = "https://w3id.org/function/ontology#predicate";
const FNO_TYPE: &str = "https://w3id.org/function/ontology#type";

const ALIGN_CELL: &str = "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#Cell";
const ALIGN_ENTITY1: &str = "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#entity1";
const ALIGN_ENTITY2: &str = "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#entity2";
const ALIGN_RELATION: &str = "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#relation";
const EDOAL_TRANSFORMATION: &str = "http://ns.inria.org/edoal/1.0/#transformation";
const EDOAL_URI: &str = "http://ns.inria.org/edoal/1.0/#uri";
const EDOAL_RELATION_T: &str = "http://ns.inria.org/edoal/1.0/#Relation";
const EDOAL_PROPERTY_T: &str = "http://ns.inria.org/edoal/1.0/#Property";
const EDOAL_CLASS_T: &str = "http://ns.inria.org/edoal/1.0/#Class";

/// Known alignment-target prefixes (the keys of the historical `ALIGNMENT_TARGETS`).
const ALIGNMENT_TARGETS: &[&str] = &[
    "gufo",
    "umbel",
    "dolce",
    "bfo",
    "foaf",
    "rel",
    "doap",
    "prov",
    "dqv",
    "org",
    "time",
    "schema",
    "dcterms",
    "mo",
    "mbz",
    "discogs",
    "afo",
    "afv",
    "jams",
    "pon",
    "chord",
    "gedcom",
    "vcard",
    "geo",
    "wgs84",
    "tgn",
    "gvp",
    "frbr",
    "fabio",
    "lrmoo",
    "bibo",
    "bibframe",
    "sioc",
    "skos",
    "nmo",
    "wot",
    "odrl",
    "cc",
    "premis",
    "rstmt",
    "spdx",
    "spdxlic",
    "codemeta",
    "forgefed",
    "ma",
    "gsso",
    "homosaurus",
    "fhir",
    "bio",
    "gedcomx",
    "geonames",
    "wikidata",
    "lexvo",
    "glottolog",
    "ontolex",
    "lime",
    "qudt",
    "gtfs",
    "fibo-fnd-acc-cur",
    "fibo-iso4217",
    "fibo-fnd-acc-ae",
    "fibo-fbc-fi-fi",
    "fibo-fbc-pas-fpas",
    "fibo-fnd-pas-ps",
    "brick",
    "bot",
    "ifc",
    "crmsci",
    "lvont",
    "moat",
    "tags",
    "qb",
    "mf",
    "faldo",
    "so",
    "crmarc",
    "crmdig",
    "exif",
    "iiif",
    "obscore",
    "ivoa",
    "bbc",
    "iptc",
    "loinc",
    "snomed",
];

/// Alignment-target prefixes that are aliases for canonical registry prefixes.
const TARGET_PREFIX_ALIASES: &[(&str, &str)] = &[
    ("dolce", "dul"),
    ("gedcomx", "gx"),
    ("geonames", "gn"),
    ("wikidata", "wd"),
];

// ── Diagnostic carrier ────────────────────────────────────────────────────────

/// One correspondence-soundness problem. The fields mirror the canonical projection
/// diagnostic shape EXACTLY (the parity harness byte-compares the two), so the finding
/// leg packs both into the same
/// `{severity, code, message, check, instance}` shape and carries the SSSOM row CURIEs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectionDiagnostic {
    /// Severity token: `"ERROR"`, `"WARNING"`, or `"INFO"`.
    pub severity: String,
    /// The drift family (e.g. `inverse-direction`, `equivalence-collapse`, `fno-type`).
    pub check: String,
    /// A stable per-check code (same value as `check`).
    pub code: String,
    /// The human-readable problem, verbatim from the retired lints.
    pub message: String,
    /// The most-specific RDF node the problem concerns, or `None`.
    pub instance: Option<String>,
    /// For alignment-direction findings, the SSSOM row CURIEs. `None` for FnO findings.
    pub subject_id: Option<String>,
    pub predicate_id: Option<String>,
    pub object_id: Option<String>,
}

impl ProjectionDiagnostic {
    fn error(check: &str, message: String, instance: Option<String>) -> Self {
        Self {
            severity: "ERROR".to_owned(),
            check: check.to_owned(),
            code: check.to_owned(),
            message,
            instance,
            subject_id: None,
            predicate_id: None,
            object_id: None,
        }
    }

    /// Severity-first ordering: ERROR < WARNING < INFO < everything else, then check,
    /// then instance.
    pub fn cmp_severity_check_instance(&self, other: &Self) -> std::cmp::Ordering {
        let order = |s: &str| match s {
            "ERROR" => 0,
            "WARNING" => 1,
            "INFO" => 2,
            _ => 3,
        };
        order(&self.severity)
            .cmp(&order(&other.severity))
            .then_with(|| self.check.cmp(&other.check))
            .then_with(|| self.instance.cmp(&other.instance))
    }
}

// ── SSSOM mapping row ─────────────────────────────────────────────────────────

/// One SSSOM mapping row — the subset the soundness pass consumes.
#[derive(Debug, Clone)]
pub struct Mapping {
    pub subject_id: String,
    pub predicate_id: String,
    pub object_id: String,
    pub confidence: String,
    pub mapping_justification: String,
}

/// Parse one SSSOM TSV file's text into [`Mapping`] rows (pure; the file-reading edge
/// reads the bytes). Comment lines starting with `#` are skipped; the first non-comment
/// line is the TSV header. Mirrors the retired `parse_sssom_tsv`.
pub fn parse_sssom_tsv(text: &str) -> gmeow_errors::Result<Vec<Mapping>> {
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    if lines.is_empty() {
        return Ok(Vec::new());
    }
    let header = lines.remove(0);
    let columns: Vec<&str> = header.split('\t').collect();
    let idx = |name: &str| columns.iter().position(|c| *c == name);

    let subject_idx = idx("subject_id").ok_or_else(|| {
        Diag::of_kind(crate::error::Correspondence {
            detail: "missing subject_id column".to_owned(),
        })
    })?;
    let predicate_idx = idx("predicate_id").ok_or_else(|| {
        Diag::of_kind(crate::error::Correspondence {
            detail: "missing predicate_id column".to_owned(),
        })
    })?;
    let object_idx = idx("object_id").ok_or_else(|| {
        Diag::of_kind(crate::error::Correspondence {
            detail: "missing object_id column".to_owned(),
        })
    })?;
    let justification_idx = idx("mapping_justification");
    let confidence_idx = idx("confidence");

    let mut rows: Vec<Mapping> = Vec::new();
    for line in &lines {
        if line.is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        let get = |i: usize| cells.get(i).unwrap_or(&"").to_string();
        rows.push(Mapping {
            subject_id: get(subject_idx),
            predicate_id: get(predicate_idx),
            object_id: get(object_idx),
            confidence: confidence_idx.map(get).unwrap_or_default(),
            mapping_justification: justification_idx.map(get).unwrap_or_default(),
        });
    }
    Ok(rows)
}

/// Keys judged by the inverse-direction check so domain-range does not double-report.
type JudgedSet = BTreeSet<(String, String, String)>;

// ── Public entry point ────────────────────────────────────────────────────────

/// The already-parsed inputs the pure soundness pass operates over (the file-reading +
/// Turtle-parsing edge in the caller hands these in).
pub struct SoundnessInputs<'a> {
    /// The merged ontology view (`ontology/gmeow.ttl` ⊕ every slice `Module`).
    pub ontology: &'a DslView<'a>,
    /// Per-prefix target-axiom views (snapshot ⊕ fixture ⊕ any successful network fetch).
    pub target_graphs: &'a BTreeMap<String, DslView<'a>>,
    /// Prefixes for which a network fetch was attempted and FAILED (prefix → error text).
    /// These suppress the per-row `info_unavailable` finding (the failure INFO covers them)
    /// and emit one domain-range INFO each, matching the retired lint.
    pub network_failed: &'a BTreeMap<String, String>,
    /// The SSSOM mapping rows (from the committed `generated/mappings/*.sssom.tsv`).
    pub mappings: &'a [Mapping],
    /// The merged FnO catalog view (`functions.fno.ttl` ⊕ `transforms.fno.ttl`).
    pub fno: &'a DslView<'a>,
    /// The committed EDOAL views, sorted by file name (mirrors the sorted glob).
    pub edoal: &'a [(String, DslView<'a>)],
    /// Every parsed `gmeow:ProjectionMapping` cell (the SAME shared get-leg model both the
    /// EDOAL and SPARQL-CONSTRUCT lowerings render from — [`crate::projections::get_leg::projections`]
    /// over the merged `dsl/mappings/` view). The sole authority
    /// [`check_edoal_entity_kind`]'s entity2 check correlates a committed EDOAL cell
    /// against, so the gate verifies internal template coherence — never the external
    /// target vocabulary (EDOAL is DERIVED FROM GMEOW's own templates, per Principle 17).
    pub cells: &'a [ProjectionCell],
}

/// Run the seven correspondence-soundness checks over the already-parsed inputs.
///
/// Ordering mirrors the retired `lint_projection`: the two FnO checks first
/// (`fno-type` → `fno-ref`), then the five alignment checks, with a final stable
/// severity→check→instance sort over the combined list.
pub fn run_soundness(inputs: &SoundnessInputs<'_>) -> Vec<ProjectionDiagnostic> {
    let mut out: Vec<ProjectionDiagnostic> = Vec::new();
    out.extend(fno_type_mismatches(inputs.ontology, inputs.fno));
    out.extend(fno_reference_integrity(inputs.fno, inputs.edoal));
    out.extend(check_edoal_entity_kind(
        inputs.ontology,
        inputs.cells,
        inputs.edoal,
    ));
    out.extend(lint_alignment_directions(inputs));
    out.sort_by(ProjectionDiagnostic::cmp_severity_check_instance);
    out
}

/// Cross-check the committed EDOAL cells' entity kinds for coherence. Emission DERIVES
/// each kind from GMEOW's own model — the source term's OWL character (entity1) or the
/// correspondence TEMPLATE's object-position (entity2) — so a finding here is a genuine
/// drift, never a guess:
/// * **A** — a direct-URI cell must align same-kind entities (`entity1` kind = `entity2`).
/// * **C** — `entity1` (the GMEOW source) must match its own OWL character in the ontology
///   (an authored `gmeow:edoalSourceKind` override that contradicts GMEOW is rejected).
/// * **B** — every committed cell's `entity2` must match the kind its OWN correspondence
///   TEMPLATE derives ([`template_target_kind`], the SAME derivation the EDOAL lowering
///   itself uses), correlated by profile + `to_predicate`. This is INTERNAL coherence —
///   the committed bytes must agree with the templates that (re)generation would emit —
///   never a comparison against the external target vocabulary (EDOAL is a lossy
///   projection DERIVED FROM GMEOW's `gmeow:ProjectionMapping` templates, per Principle
///   17; the target vocabulary is not an authority over it). Runs on EVERY cell, not just
///   `=`: a `<=` subsumption target must be as truthfully typed as an equivalence one.
fn check_edoal_entity_kind(
    onto: &DslView<'_>,
    cells: &[ProjectionCell],
    edoal: &[(String, DslView<'_>)],
) -> Vec<ProjectionDiagnostic> {
    let mut out: Vec<ProjectionDiagnostic> = Vec::new();
    for (name, view) in edoal {
        let profile = name.strip_suffix(".edoal.ttl").unwrap_or(name.as_str());
        for cell in subject_terms_of_type(view, ALIGN_CELL) {
            let (Some(n1), Some(n2)) = (
                view.objects_of_term(&cell, ALIGN_ENTITY1)
                    .into_iter()
                    .next(),
                view.objects_of_term(&cell, ALIGN_ENTITY2)
                    .into_iter()
                    .next(),
            ) else {
                continue;
            };
            let (Some((k1, uri1)), Some((k2, uri2))) = (
                edoal_node_kind_uri(view, &n1),
                edoal_node_kind_uri(view, &n2),
            ) else {
                continue;
            };

            // A — an EQUIVALENCE (`=`) direct-URI cell must align same-kind entities. A
            // lossy `<=` collapse may legitimately cross kinds (e.g. a GMEOW relation
            // projected onto a target literal or class), documented by its lossyDrop.
            if let (Some(u1), Some(u2)) = (&uri1, &uri2)
                && k1 != k2
                && cell_is_equivalence(view, &cell)
            {
                out.push(ProjectionDiagnostic::error(
                    "edoal-entity-kind",
                    format!(
                        "{name}: equivalence cell aligns edoal:{k1} ({u1}) with edoal:{k2} ({u2}) \
                         — entity1/entity2 kind mismatch"
                    ),
                    Some(u2.clone()),
                ));
            }

            // C — an equivalence cell's entity1 (GMEOW source) must match its ontology OWL
            // character; a lossy `<=` projection may carry a deliberately coarsened kind.
            if let Some(u1) = &uri1
                && cell_is_equivalence(view, &cell)
                && let Some(expected) = owl_kind_edoal(onto, u1)
                && expected != k1
            {
                out.push(ProjectionDiagnostic::error(
                    "edoal-entity-kind",
                    format!(
                        "{name}: equivalence cell entity1 emitted edoal:{k1} but GMEOW {u1} is an \
                         owl:{expected} term"
                    ),
                    Some(u1.clone()),
                ));
            }

            // B — every cell's entity2 must match the correspondence TEMPLATE's own
            // derivation (never the external target vocabulary). `None` means no template
            // in this profile targets `u2` via `to_predicate` — a direct 1:1 predicate
            // mapping has no template to check coherence against, so no claim is made.
            if let Some(u2) = &uri2
                && let Some(expected) =
                    expected_entity2_kind(cells, onto, profile, u2, uri1.as_deref())
                && expected != k2
            {
                out.push(ProjectionDiagnostic::error(
                    "edoal-entity-kind",
                    format!(
                        "{name}: entity2 emitted edoal:{k2} ({u2}) but the correspondence \
                         template derives edoal:{expected} for this target — kind mismatch"
                    ),
                    Some(u2.clone()),
                ));
            }
        }
    }
    out
}

/// The `edoal:Relation`/`edoal:Property` capitalized token the correspondence TEMPLATE
/// derives for `to_predicate` (== the committed cell's `entity2` `edoal:uri`, `u2`) in
/// `profile`, by re-running [`template_target_kind`] — the SAME derivation the EDOAL
/// lowering itself used to emit the committed bytes — over the cell that produced this
/// committed row. `None` when no matching binding targets `u2` via a correspondence
/// template (a direct 1:1 predicate target with no `gmeow:templateAtoms` naming it carries
/// no template-derived expectation).
///
/// `source_uri` is the committed cell's `entity1` `edoal:uri` (the GMEOW source term) when
/// it is a direct term reference. It disambiguates a POLYMORPHIC target predicate — one
/// `to_predicate` legitimately reused by two mappings with different source kinds (e.g.
/// `bf:title` carrying a flat literal from `gmeow:title` in one cell and a structured
/// `bf:Title` node from `gmeow:hasTitle` in another): the expectation must come from the
/// cell whose OWN source matches this committed row, never from a sibling cell that merely
/// shares the target predicate. A compose/restriction `entity1` (`source_uri` `None`, e.g.
/// a path source) cannot be matched by source, so it falls back to `to_predicate` alone.
fn expected_entity2_kind(
    cells: &[ProjectionCell],
    onto: &DslView<'_>,
    profile: &str,
    to_predicate: &str,
    source_uri: Option<&str>,
) -> Option<&'static str> {
    let lower = cells.iter().find_map(|cell| {
        // Correlate to the committed row's OWN source: when entity1 is a direct term, only
        // the mapping whose `edoalSource` matches it may supply this row's expectation.
        if let Some(src) = source_uri
            && cell.pattern.edoal_source.as_deref() != Some(src)
        {
            return None;
        }
        cell.bindings
            .iter()
            .filter(|b| b.profile == profile && b.to_predicate.as_deref() == Some(to_predicate))
            .find_map(|b| template_target_kind(onto, b, &cell.pattern))
    })?;
    Some(match lower {
        "relation" => "Relation",
        "property" => "Property",
        "class" => "Class",
        other => unreachable!("template_target_kind returned unknown token {other:?}"),
    })
}

/// The EDOAL kind (`Relation`/`Property`/`Class`) an entity blank node is typed as, plus
/// its `edoal:uri` when it is a direct term reference (`None` for a compose/restriction).
fn edoal_node_kind_uri(
    view: &DslView<'_>,
    node: &DslTerm,
) -> Option<(&'static str, Option<String>)> {
    let kind = view
        .objects_of_term(node, RDF_TYPE)
        .into_iter()
        .find_map(|t| match t.as_iri()? {
            EDOAL_RELATION_T => Some("Relation"),
            EDOAL_PROPERTY_T => Some("Property"),
            EDOAL_CLASS_T => Some("Class"),
            _ => None,
        })?;
    Some((kind, view.object_iri_of_term(node, EDOAL_URI)))
}

/// The EDOAL kind implied by a GMEOW term's OWL character in the ontology, or `None`.
/// A term typed with an OWL 2 object-property subtype (Symmetric/Transitive/
/// InverseFunctional/Reflexive/Asymmetric/Irreflexive) — even without a co-asserted
/// `owl:ObjectProperty` — is still an object property by OWL 2 semantics, so it also
/// derives `Relation` here.
fn owl_kind_edoal(onto: &DslView<'_>, iri: &str) -> Option<&'static str> {
    if has_type(onto, iri, OWL_OBJECT_PROPERTY)
        || OWL_OBJECT_PROPERTY_SUBTYPES
            .iter()
            .any(|t| has_type(onto, iri, t))
    {
        Some("Relation")
    } else if has_type(onto, iri, OWL_DATATYPE_PROPERTY) {
        Some("Property")
    } else if has_type(onto, iri, OWL_CLASS) {
        Some("Class")
    } else {
        None
    }
}

/// Whether an `align:Cell` carries the equivalence relation token `=`.
fn cell_is_equivalence(view: &DslView<'_>, cell: &DslTerm) -> bool {
    view.objects_of_term(cell, ALIGN_RELATION)
        .iter()
        .any(|t| t.as_literal() == Some("="))
}

/// Run only the five alignment-direction checks (the historical
/// `lint_alignment_directions` surface), returning a severity-sorted list.
pub fn lint_alignment_directions(inputs: &SoundnessInputs<'_>) -> Vec<ProjectionDiagnostic> {
    let onto = inputs.ontology;
    let mappings = inputs.mappings;
    let target_graphs = inputs.target_graphs;

    // Group property mappings (subject a GMEOW property, object a known alignment
    // target) by the GMEOW property they align.
    let mut gmeow_props: BTreeMap<String, Vec<Mapping>> = BTreeMap::new();
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for m in mappings {
        if !m.subject_id.starts_with(GMEOW_PREFIX) && !m.subject_id.starts_with(LOGIC_PREFIX) {
            continue;
        }
        let Some(prefix) = prefix_of(&m.object_id) else {
            continue;
        };
        let Some(subj_iri) = expand_curie(&m.subject_id) else {
            continue;
        };
        if !is_property(onto, &subj_iri) {
            continue;
        }
        gmeow_props.entry(subj_iri).or_default().push(m.clone());
        referenced.insert(prefix);
    }

    let mut findings: Vec<ProjectionDiagnostic> = Vec::new();

    // Network-failure INFOs (one per attempted-and-failed prefix). The fetch itself
    // happens in the caller; this preserves the exact diagnostic the retired lint
    // emitted on a network failure.
    for (prefix, err) in inputs.network_failed {
        findings.push(ProjectionDiagnostic {
            severity: "INFO".to_owned(),
            check: "domain-range".to_owned(),
            code: "domain-range".to_owned(),
            message: format!("network fetch failed for target '{prefix}': {err}; skipped"),
            instance: None,
            subject_id: None,
            predicate_id: None,
            object_id: None,
        });
    }

    // Per-row INFO for every referenced prefix with no axioms available (and not already
    // covered by a network-failure INFO).
    for prop_mappings in gmeow_props.values() {
        for m in prop_mappings {
            let prefix = prefix_of(&m.object_id).expect("filtered to alignment targets");
            if !target_graphs.contains_key(&prefix) && !inputs.network_failed.contains_key(&prefix)
            {
                findings.push(info_unavailable(m, &prefix));
            }
        }
    }

    let bridge = build_class_bridge(mappings, onto, target_graphs);

    let (inverse_findings, judged) =
        check_inverse_direction(&gmeow_props, onto, target_graphs, &bridge);
    findings.extend(inverse_findings);

    let domain_findings = check_domain_range(&gmeow_props, onto, target_graphs, &bridge, &judged);
    findings.extend(domain_findings);

    let character_findings = check_property_character(&gmeow_props, onto, target_graphs);
    findings.extend(character_findings);

    let collapse_findings = check_equivalence_collapse(mappings, onto, target_graphs);
    findings.extend(collapse_findings);

    findings.extend(lint_dc_refinement(mappings));

    findings.sort_by(ProjectionDiagnostic::cmp_severity_check_instance);
    findings
}

// ── DC refinement / dumb-down lint ────────────────────────────────────────────

/// Lint DC alignments for refinement consistency and dumb-down hygiene.
pub fn lint_dc_refinement(mappings: &[Mapping]) -> Vec<ProjectionDiagnostic> {
    let mut aligned_targets: BTreeSet<String> = BTreeSet::new();
    for m in mappings {
        aligned_targets.insert(m.object_id.clone());
    }

    let mut findings: Vec<ProjectionDiagnostic> = Vec::new();

    for (refinement, broader) in DCTERMS_REFINEMENTS {
        let refinement_aligned = aligned_targets.contains(*refinement);
        let broader_aligned = aligned_targets.contains(*broader);
        if refinement_aligned && !broader_aligned {
            let message = format!(
                "{refinement} is aligned but its broader element {broader} is not \
                 — did you mean add an alignment for {broader} or document why it is absent?"
            );
            findings.push(ProjectionDiagnostic {
                severity: "WARNING".to_owned(),
                check: "dc-refinement".to_owned(),
                code: "dc-refinement".to_owned(),
                message,
                instance: expand_curie(broader),
                subject_id: None,
                predicate_id: None,
                object_id: Some(refinement.to_string()),
            });
        }
    }

    for m in mappings {
        if m.object_id.starts_with("dc:") && !GRANDFATHERED_DC.contains(&m.object_id.as_str()) {
            let message = format!(
                "{} is hand-authored; dc: alignments should be derived from \
                 dcterms: via dumb-down — did you mean remove the dc: alignment \
                 and rely on the dcterms:→dc: subproperty derivation?",
                m.object_id
            );
            findings.push(ProjectionDiagnostic {
                severity: "WARNING".to_owned(),
                check: "dc-hand-authored".to_owned(),
                code: "dc-hand-authored".to_owned(),
                message,
                instance: expand_curie(&m.object_id),
                subject_id: Some(m.subject_id.clone()),
                predicate_id: Some(m.predicate_id.clone()),
                object_id: Some(m.object_id.clone()),
            });
        }
    }

    findings.sort_by(ProjectionDiagnostic::cmp_severity_check_instance);
    findings
}

// ── Inverse-direction check ───────────────────────────────────────────────────

fn check_inverse_direction(
    gmeow_props: &BTreeMap<String, Vec<Mapping>>,
    onto: &DslView<'_>,
    target_graphs: &BTreeMap<String, DslView<'_>>,
    bridge: &BTreeMap<String, BTreeSet<String>>,
) -> (Vec<ProjectionDiagnostic>, JudgedSet) {
    let mut findings: Vec<ProjectionDiagnostic> = Vec::new();
    let mut judged: BTreeSet<(String, String, String)> = BTreeSet::new();

    for (prop, prop_mappings) in gmeow_props {
        if has_type(onto, prop, OWL_SYMMETRIC_PROPERTY) {
            continue;
        }

        let mut by_iri: BTreeMap<String, Mapping> = BTreeMap::new();
        for m in prop_mappings {
            if HIERARCHICAL_PREDICATES.contains(&m.predicate_id.as_str()) {
                continue;
            }
            if let Some(obj_iri) = expand_curie(&m.object_id) {
                by_iri.insert(obj_iri, m.clone());
            }
        }

        let g_dom = objects_iri(onto, prop, RDFS_DOMAIN);
        let g_rng = objects_iri(onto, prop, RDFS_RANGE);

        let mut seen_pairs: BTreeSet<[String; 2]> = BTreeSet::new();
        for (target_iri, m) in &by_iri {
            let Some(prefix) = prefix_of(&m.object_id) else {
                continue;
            };
            let Some(graph) = target_graphs.get(&prefix) else {
                continue;
            };
            let inverses = target_inverses(graph, target_iri);

            for inv in &inverses {
                if inv == target_iri {
                    continue;
                }
                let Some(m_inv) = by_iri.get(inv) else {
                    continue;
                };
                let mut pair = [target_iri.clone(), inv.clone()];
                pair.sort();
                if seen_pairs.contains(&pair) {
                    continue;
                }
                seen_pairs.insert(pair);

                let (canonical, offender) = rank_pair(m, m_inv);
                let key = (
                    offender.subject_id.clone(),
                    offender.predicate_id.clone(),
                    offender.object_id.clone(),
                );
                judged.insert(key);

                let severity =
                    if STRONG_PROPERTY_PREDICATES.contains(&canonical.predicate_id.as_str()) {
                        "ERROR"
                    } else {
                        "WARNING"
                    };
                let message = format!(
                    "mapped to {}, but the property is also mapped to its declared inverse {} \
                     (via {}) — one direction is wrong",
                    offender.object_id, canonical.object_id, canonical.predicate_id
                );
                let suggestion = canonical.object_id.clone();
                findings.push(ProjectionDiagnostic {
                    severity: severity.to_owned(),
                    check: "inverse-direction".to_owned(),
                    code: "inverse-direction".to_owned(),
                    message: format!("{message} — did you mean {suggestion}?"),
                    instance: expand_curie(&offender.object_id),
                    subject_id: Some(offender.subject_id.clone()),
                    predicate_id: Some(offender.predicate_id.clone()),
                    object_id: Some(offender.object_id.clone()),
                });
            }

            let key = (
                m.subject_id.clone(),
                m.predicate_id.clone(),
                m.object_id.clone(),
            );
            if judged.contains(&key) {
                continue;
            }
            let t_dom = target_domain(graph, target_iri);
            let t_rng = target_range(graph, target_iri);
            if t_dom.is_empty() || t_rng.is_empty() || g_dom.is_empty() || g_rng.is_empty() {
                continue;
            }
            let direct_fit = overlaps(&g_dom, &t_dom, bridge) && overlaps(&g_rng, &t_rng, bridge);
            if direct_fit {
                continue;
            }
            for inv in &inverses {
                if inv == target_iri {
                    continue;
                }
                let inv_dom = target_domain(graph, inv);
                let inv_rng = target_range(graph, inv);
                if inv_dom.is_empty() || inv_rng.is_empty() {
                    continue;
                }
                if overlaps(&g_dom, &inv_dom, bridge) && overlaps(&g_rng, &inv_rng, bridge) {
                    judged.insert(key);
                    let inv_curie = shorten_iri(inv);
                    findings.push(ProjectionDiagnostic {
                        severity: severity_for(&m.predicate_id).to_owned(),
                        check: "inverse-direction".to_owned(),
                        code: "inverse-direction".to_owned(),
                        message: format!(
                            "{}'s domain/range is inverted relative to {}; its inverse {} \
                             matches the direction — did you mean {}?",
                            m.object_id, m.subject_id, inv_curie, inv_curie
                        ),
                        instance: expand_curie(&m.object_id),
                        subject_id: Some(m.subject_id.clone()),
                        predicate_id: Some(m.predicate_id.clone()),
                        object_id: Some(m.object_id.clone()),
                    });
                    break;
                }
            }
        }
    }

    (findings, judged)
}

// ── Domain-range check ────────────────────────────────────────────────────────

fn check_domain_range(
    gmeow_props: &BTreeMap<String, Vec<Mapping>>,
    onto: &DslView<'_>,
    target_graphs: &BTreeMap<String, DslView<'_>>,
    bridge: &BTreeMap<String, BTreeSet<String>>,
    judged: &JudgedSet,
) -> Vec<ProjectionDiagnostic> {
    let mut findings: Vec<ProjectionDiagnostic> = Vec::new();
    for (prop, prop_mappings) in gmeow_props {
        let g_dom = objects_iri(onto, prop, RDFS_DOMAIN);
        let g_rng = objects_iri(onto, prop, RDFS_RANGE);
        for m in prop_mappings {
            if HIERARCHICAL_PREDICATES.contains(&m.predicate_id.as_str()) {
                continue;
            }
            let key = (
                m.subject_id.clone(),
                m.predicate_id.clone(),
                m.object_id.clone(),
            );
            if judged.contains(&key) {
                continue;
            }
            let Some(prefix) = prefix_of(&m.object_id) else {
                continue;
            };
            let Some(graph) = target_graphs.get(&prefix) else {
                continue;
            };
            let Some(target_iri) = expand_curie(&m.object_id) else {
                continue;
            };
            let t_dom = target_domain(graph, &target_iri);
            let t_rng = target_range(graph, &target_iri);
            if t_dom.is_empty() || t_rng.is_empty() {
                findings.push(info_not_checkable(
                    m,
                    "target term declares no domain/range to check against",
                ));
                continue;
            }
            if g_dom.is_empty() || g_rng.is_empty() {
                findings.push(info_not_checkable(
                    m,
                    "GMEOW term declares no domain/range to check against",
                ));
                continue;
            }
            if overlaps(&g_dom, &t_dom, bridge) && overlaps(&g_rng, &t_rng, bridge) {
                continue;
            }
            let swapped = overlaps(&g_dom, &t_rng, bridge) && overlaps(&g_rng, &t_dom, bridge);
            if !swapped {
                findings.push(ProjectionDiagnostic {
                    severity: "INFO".to_owned(),
                    check: "domain-range".to_owned(),
                    code: "domain-range".to_owned(),
                    message: "domain/range overlap could not be established \
                              (no class bridge to the target's domain/range)"
                        .to_owned(),
                    instance: Some(target_iri),
                    subject_id: Some(m.subject_id.clone()),
                    predicate_id: Some(m.predicate_id.clone()),
                    object_id: Some(m.object_id.clone()),
                });
                continue;
            }
            findings.push(ProjectionDiagnostic {
                severity: severity_for(&m.predicate_id).to_owned(),
                check: "domain-range".to_owned(),
                code: "domain-range".to_owned(),
                message: "domain/range are inverted relative to the target term".to_owned(),
                instance: Some(target_iri),
                subject_id: Some(m.subject_id.clone()),
                predicate_id: Some(m.predicate_id.clone()),
                object_id: Some(m.object_id.clone()),
            });
        }
    }
    findings
}

// ── Property-character check ──────────────────────────────────────────────────

fn check_property_character(
    gmeow_props: &BTreeMap<String, Vec<Mapping>>,
    onto: &DslView<'_>,
    target_graphs: &BTreeMap<String, DslView<'_>>,
) -> Vec<ProjectionDiagnostic> {
    let mut findings: Vec<ProjectionDiagnostic> = Vec::new();
    let owl_prop_types: BTreeSet<&str> = OWL_PROPERTY_TYPES.iter().copied().collect();

    for (prop, prop_mappings) in gmeow_props {
        let g_is_object = has_type(onto, prop, OWL_OBJECT_PROPERTY);
        let g_is_data = has_type(onto, prop, OWL_DATATYPE_PROPERTY);
        // The functional / inverse-functional characteristics of a GMEOW property now live
        // on the canonical `logic:PropertyCharacteristicAssertion` carrier, not on an
        // `rdf:type owl:FunctionalProperty` triple; the remaining characteristics
        // (Transitive/Symmetric) are still authored as OWL types. Read owl-typed
        // characteristics via `rdf:type` AND the functional pair from the carrier.
        let carrier_chars = gmeow_carrier_characteristics(onto, prop);
        let mut g_chars: Vec<String> = Vec::new();
        for char_iri in CHARACTER_TYPES {
            if has_type(onto, prop, char_iri) || carrier_chars.contains(*char_iri) {
                g_chars.push((*char_iri).to_owned());
            }
        }

        for m in prop_mappings {
            if !STRONG_PROPERTY_PREDICATES.contains(&m.predicate_id.as_str()) {
                continue;
            }
            let Some(prefix) = prefix_of(&m.object_id) else {
                continue;
            };
            let Some(graph) = target_graphs.get(&prefix) else {
                continue;
            };
            let Some(term) = expand_curie(&m.object_id) else {
                continue;
            };
            let t_types: BTreeSet<String> =
                objects_iri(graph, &term, RDF_TYPE).into_iter().collect();
            if t_types.is_empty() {
                continue;
            }

            if g_is_object && t_types.contains(OWL_DATATYPE_PROPERTY) {
                findings.push(character_finding(
                    m,
                    "ERROR",
                    "GMEOW object property vs target datatype property",
                    &term,
                ));
            } else if g_is_data && t_types.contains(OWL_OBJECT_PROPERTY) {
                findings.push(character_finding(
                    m,
                    "ERROR",
                    "GMEOW datatype property vs target object property",
                    &term,
                ));
            }

            let speaks_owl = t_types.iter().any(|t| owl_prop_types.contains(t.as_str()));
            if !speaks_owl {
                continue;
            }
            for char_iri in &g_chars {
                if !t_types.contains(char_iri) {
                    let shortened = shorten_iri(char_iri);
                    let label = shortened.split(':').next_back().unwrap_or(char_iri);
                    findings.push(character_finding(
                        m,
                        "WARNING",
                        &format!("GMEOW declares {label} but the target does not"),
                        &term,
                    ));
                }
            }

            for char_iri in CHARACTER_TYPES {
                if t_types.contains(*char_iri) && !g_chars.iter().any(|g| g == char_iri) {
                    let shortened = shorten_iri(char_iri);
                    let label = shortened.split(':').next_back().unwrap_or(char_iri);
                    findings.push(character_finding(
                        m,
                        "WARNING",
                        &format!("target declares {label} but GMEOW does not"),
                        &term,
                    ));
                }
            }
        }
    }
    findings
}

/// The OWL property-character IRIs a GMEOW property bears via its canonical
/// `logic:PropertyCharacteristicAssertion` carrier(s). Each carrier record joins
/// `logic:characterizes <prop>` with a `logic:characteristicSort` marker; the functional
/// markers map back to their OWL projections so a functional GMEOW property compares
/// equal to a target that declares `owl:FunctionalProperty`. Only the functional
/// characteristics migrated to the carrier; Transitive/Symmetric stay OWL-typed and are
/// read via `rdf:type` at the call site.
fn gmeow_carrier_characteristics(onto: &DslView<'_>, prop: &str) -> BTreeSet<&'static str> {
    let mut out: BTreeSet<&'static str> = BTreeSet::new();
    for record in onto.subjects_with_object_iri(LOGIC_CHARACTERIZES, prop) {
        for sort in onto.object_iris(&record, LOGIC_CHARACTERISTIC_SORT) {
            match sort.as_str() {
                LOGIC_FUNCTIONAL_PROPERTY => {
                    out.insert(OWL_FUNCTIONAL_PROPERTY);
                }
                LOGIC_INVERSE_FUNCTIONAL_PROPERTY => {
                    out.insert(OWL_INVERSE_FUNCTIONAL_PROPERTY);
                }
                _ => {}
            }
        }
    }
    out
}

fn character_finding(
    m: &Mapping,
    severity: &str,
    message: &str,
    term: &str,
) -> ProjectionDiagnostic {
    ProjectionDiagnostic {
        severity: severity.to_owned(),
        check: "property-character".to_owned(),
        code: "property-character".to_owned(),
        message: message.to_owned(),
        instance: Some(term.to_owned()),
        subject_id: Some(m.subject_id.clone()),
        predicate_id: Some(m.predicate_id.clone()),
        object_id: Some(m.object_id.clone()),
    }
}

// ── Equivalence-collapse check (Principle 5) ──────────────────────────────────

/// Principle 5: no equivalence chain may connect disjoint terms.
fn check_equivalence_collapse(
    mappings: &[Mapping],
    onto: &DslView<'_>,
    target_graphs: &BTreeMap<String, DslView<'_>>,
) -> Vec<ProjectionDiagnostic> {
    let adjacency = equivalence_adjacency(mappings, onto, target_graphs);
    let component = equivalence_components(&adjacency);
    let mut findings: Vec<ProjectionDiagnostic> = Vec::new();

    for (a, b, _axiom) in disjoint_pairs(onto) {
        if component.get(&a) != component.get(&b) {
            continue;
        }
        let Some(path) = equivalence_path(&adjacency, &a, &b) else {
            continue;
        };
        let chain = path
            .iter()
            .map(|n| shorten_iri(n))
            .collect::<Vec<_>>()
            .join(" = ");
        findings.push(ProjectionDiagnostic {
            severity: "ERROR".to_owned(),
            check: "equivalence-collapse".to_owned(),
            code: "equivalence-collapse".to_owned(),
            message: format!(
                "declared disjoint, but the equivalence closure connects them \
                 (Principle 5): {chain}"
            ),
            instance: Some(a.clone()),
            subject_id: None,
            predicate_id: None,
            object_id: None,
        });
    }
    findings
}

fn equivalence_adjacency(
    mappings: &[Mapping],
    onto: &DslView<'_>,
    target_graphs: &BTreeMap<String, DslView<'_>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    let mut link = |a: String, b: String| {
        if a != b {
            adjacency.entry(a.clone()).or_default().insert(b.clone());
            adjacency.entry(b).or_default().insert(a);
        }
    };

    for m in mappings {
        if !COLLAPSE_PREDICATES.contains(&m.predicate_id.as_str()) {
            continue;
        }
        let (Some(subj), Some(obj)) = (expand_curie(&m.subject_id), expand_curie(&m.object_id))
        else {
            continue;
        };
        link(subj, obj);
    }

    let mut graphs: Vec<&DslView<'_>> = vec![onto];
    graphs.extend(target_graphs.values());
    for graph in graphs {
        for pred in [
            OWL_EQUIVALENT_CLASS,
            OWL_EQUIVALENT_PROPERTY,
            SKOS_EXACT_MATCH,
        ] {
            for (a, b) in subject_objects_iri(graph, pred) {
                link(a, b);
            }
        }
    }
    adjacency
}

fn equivalence_components(
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, usize> {
    let mut component: BTreeMap<String, usize> = BTreeMap::new();
    let mut next_id: usize = 0;
    for start in adjacency.keys() {
        if component.contains_key(start) {
            continue;
        }
        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(start.clone());
        component.insert(start.clone(), next_id);
        while let Some(node) = queue.pop_front() {
            for nxt in adjacency.get(&node).into_iter().flatten() {
                if component.insert(nxt.clone(), next_id).is_none() {
                    queue.push_back(nxt.clone());
                }
            }
        }
        next_id += 1;
    }
    component
}

fn equivalence_path(
    adjacency: &BTreeMap<String, BTreeSet<String>>,
    start: &str,
    goal: &str,
) -> Option<Vec<String>> {
    if !adjacency.contains_key(start) {
        return None;
    }
    let mut previous: BTreeMap<String, String> = BTreeMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    queue.push_back(start.to_owned());
    seen.insert(start.to_owned());

    while let Some(node) = queue.pop_front() {
        if node == goal {
            let mut path = vec![goal.to_owned()];
            while path.last() != Some(&start.to_owned()) {
                let last = path.last().expect("path is non-empty");
                let prev = previous.get(last).expect("path has predecessor");
                path.push(prev.clone());
            }
            path.reverse();
            return Some(path);
        }
        for nxt in adjacency.get(&node).into_iter().flatten() {
            if seen.insert(nxt.clone()) {
                previous.insert(nxt.clone(), node.clone());
                queue.push_back(nxt.clone());
            }
        }
    }
    None
}

fn disjoint_pairs(onto: &DslView<'_>) -> Vec<(String, String, String)> {
    let mut pairs: BTreeSet<(String, String, String)> = BTreeSet::new();

    let mut add = |a: &str, b: &str, axiom: &str| {
        if a != b {
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            pairs.insert((lo.to_owned(), hi.to_owned(), axiom.to_owned()));
        }
    };

    for (s, o) in subject_objects_iri(onto, OWL_DISJOINT_WITH) {
        add(&s, &o, "owl:disjointWith");
    }
    for (s, o) in subject_objects_iri(onto, OWL_PROPERTY_DISJOINT_WITH) {
        add(&s, &o, "owl:propertyDisjointWith");
    }

    for (axiom_class, axiom_curie) in [
        (OWL_ALL_DISJOINT_CLASSES, "owl:disjointWith"),
        (OWL_ALL_DISJOINT_PROPERTIES, "owl:propertyDisjointWith"),
    ] {
        for node in onto.subjects_of_type(axiom_class) {
            for head in onto.objects_of(&node, OWL_MEMBERS) {
                let members = rdf_list_named_members(onto, &head);
                for i in 0..members.len() {
                    for j in i + 1..members.len() {
                        add(&members[i], &members[j], axiom_curie);
                    }
                }
            }
        }
    }

    pairs.into_iter().collect()
}

// ── Class-equivalence bridge ──────────────────────────────────────────────────

fn build_class_bridge(
    mappings: &[Mapping],
    onto: &DslView<'_>,
    target_graphs: &BTreeMap<String, DslView<'_>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    let mut link = |a: String, b: String| {
        if a != b {
            adjacency.entry(a).or_default().insert(b);
        }
    };

    for m in mappings {
        let (Some(subj), Some(obj)) = (expand_curie(&m.subject_id), expand_curie(&m.object_id))
        else {
            continue;
        };
        if STRONG_CLASS_PREDICATES.contains(&m.predicate_id.as_str()) {
            link(subj.clone(), obj.clone());
            link(obj, subj);
        } else if m.predicate_id == "rdfs:subClassOf" {
            link(subj, obj);
        }
    }

    // `onto` is the merged AUTHORED ontology view (ontology/gmeow.ttl ⊕ every
    // slice module.ttl — see the file-reading edge in
    // crates/pipeline/src/stages/correspondence_soundness.rs), so it must scan
    // both the canonical `logic:subClassOf` edge and its `rdfs:` projection
    // (gmeow_ns::SUB_CLASS_OF doctrine; crates/ns/src/lib.rs:106-166); the vendored
    // `target_graphs` are external vocabularies that only ever speak `rdfs:`, so the
    // extra canonical-predicate scan there is a harmless no-op.
    let mut graphs: Vec<&DslView<'_>> = vec![onto];
    graphs.extend(target_graphs.values());
    for graph in graphs {
        for predicate in gmeow_ns::SUB_CLASS_OF {
            for (sub, sup) in subject_objects_iri(graph, predicate) {
                link(sub, sup);
            }
        }
        for (a, b) in subject_objects_iri(graph, OWL_EQUIVALENT_CLASS) {
            link(a.clone(), b.clone());
            link(b, a);
        }
    }

    let mut closure: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for start in adjacency.keys() {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut stack = vec![start.clone()];
        while let Some(node) = stack.pop() {
            for nxt in adjacency.get(&node).into_iter().flatten() {
                if seen.insert(nxt.clone()) {
                    stack.push(nxt.clone());
                }
            }
        }
        closure.insert(start.clone(), seen);
    }
    closure
}

fn resolve_class(iri: &str, bridge: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    out.insert(iri.to_owned());
    if let Some(set) = bridge.get(iri) {
        out.extend(set.iter().cloned());
    }
    out
}

fn overlaps(
    gmeow_classes: &[String],
    target_classes: &[String],
    bridge: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    if gmeow_classes.is_empty() || target_classes.is_empty() {
        return false;
    }
    let target_set: BTreeSet<String> = target_classes.iter().cloned().collect();
    let mut expanded: BTreeSet<String> = BTreeSet::new();
    for cls in gmeow_classes {
        expanded.extend(resolve_class(cls, bridge));
    }
    let mut expanded_targets: BTreeSet<String> = target_set;
    for cls in target_classes {
        expanded_targets.extend(resolve_class(cls, bridge));
    }
    expanded.iter().any(|c| expanded_targets.contains(c))
}

// ── Target-axiom accessors ────────────────────────────────────────────────────

fn target_domain(graph: &DslView<'_>, term: &str) -> Vec<String> {
    let mut out = objects_iri(graph, term, RDFS_DOMAIN);
    out.extend(objects_iri(graph, term, SCHEMA_DOMAIN_INCLUDES));
    out
}

fn target_range(graph: &DslView<'_>, term: &str) -> Vec<String> {
    let mut out = objects_iri(graph, term, RDFS_RANGE);
    out.extend(objects_iri(graph, term, SCHEMA_RANGE_INCLUDES));
    out
}

fn target_inverses(graph: &DslView<'_>, term: &str) -> Vec<String> {
    let mut out = objects_iri(graph, term, OWL_INVERSE_OF);
    out.extend(objects_iri(graph, term, SCHEMA_INVERSE_OF));
    out.extend(graph.subjects_with_object_iri(OWL_INVERSE_OF, term));
    out.extend(graph.subjects_with_object_iri(SCHEMA_INVERSE_OF, term));
    out.sort();
    out.dedup();
    out
}

// ── FnO back-end soundness: fno:type ↔ rdfs:range ─────────────────────────────

/// FnO param/output `fno:type`s that disagree with their predicate's `rdfs:range`.
pub fn fno_type_mismatches(onto: &DslView<'_>, fno: &DslView<'_>) -> Vec<ProjectionDiagnostic> {
    let mut params: BTreeSet<String> = BTreeSet::new();
    params.extend(fno.subjects_of_type(FNO_PARAMETER));
    params.extend(fno.subjects_of_type(FNO_OUTPUT));

    let mut problems: Vec<ProjectionDiagnostic> = Vec::new();
    for param in &params {
        let Some(predicate) = fno.object_iri(param, FNO_PREDICATE) else {
            continue;
        };
        let Some(ftype) = fno.object_iri(param, FNO_TYPE) else {
            continue;
        };
        let mut ranges: Vec<String> = onto.object_iris(&predicate, RDFS_RANGE);
        if ranges.is_empty() {
            continue;
        }
        ranges.sort();
        ranges.dedup();
        if !ranges.contains(&ftype) {
            problems.push(ProjectionDiagnostic::error(
                "fno-type",
                format!(
                    "{param}: predicate {predicate} has range {} but fno:type is {ftype}",
                    py_list_repr(&ranges)
                ),
                Some(param.clone()),
            ));
        }
    }
    problems
}

// ── FnO back-end soundness: EDOAL → FnO reference integrity ───────────────────

/// EDOAL `edoal:transformation` references to undefined FnO functions.
pub fn fno_reference_integrity(
    fno: &DslView<'_>,
    edoal: &[(String, DslView<'_>)],
) -> Vec<ProjectionDiagnostic> {
    let defined: BTreeSet<String> = fno.subjects_of_type(FNO_FUNCTION).into_iter().collect();
    let mut problems: Vec<ProjectionDiagnostic> = Vec::new();

    for (name, view) in edoal {
        for cell in subject_terms_of_type(view, ALIGN_CELL) {
            for trans in view.objects_of_term(&cell, EDOAL_TRANSFORMATION) {
                for refr in view.objects_of_term(&trans, RDFS_SEE_ALSO) {
                    let DslTerm::Iri(iri) = &refr else {
                        continue;
                    };
                    let local = iri.rsplit(['/', '#']).next().unwrap_or(iri);
                    if local.starts_with("fn") && !defined.contains(iri) {
                        problems.push(ProjectionDiagnostic::error(
                            "fno-ref",
                            format!("{name}: undefined FnO function {iri}"),
                            Some(iri.to_owned()),
                        ));
                    }
                }
            }
        }
    }
    problems
}

// ── DslView query adapters (mirror the historical oxigraph store reads) ────────

/// Every IRI object of `<subject> <pred> ?o`.
fn objects_iri(view: &DslView<'_>, subject: &str, pred: &str) -> Vec<String> {
    view.object_iris(subject, pred)
}

/// Every `(subject, object)` pair of `?s <pred> ?o` where both are named nodes.
fn subject_objects_iri(view: &DslView<'_>, pred: &str) -> Vec<(String, String)> {
    view.quads_with_predicate(pred)
        .into_iter()
        .filter_map(|(s, o)| match (s, o) {
            (DslTerm::Iri(s), DslTerm::Iri(o)) => Some((s, o)),
            _ => None,
        })
        .collect()
}

/// Whether `term` has `term a type_iri`.
fn has_type(view: &DslView<'_>, term: &str, type_iri: &str) -> bool {
    view.objects_of(term, RDF_TYPE)
        .into_iter()
        .any(|t| t.as_iri() == Some(type_iri))
}

/// Whether the IRI is declared as an OWL ObjectProperty or DatatypeProperty.
fn is_property(view: &DslView<'_>, iri: &str) -> bool {
    has_type(view, iri, OWL_OBJECT_PROPERTY) || has_type(view, iri, OWL_DATATYPE_PROPERTY)
}

/// Named-node members of an RDF list headed by `head` (only IRI members kept).
fn rdf_list_named_members(view: &DslView<'_>, head: &DslTerm) -> Vec<String> {
    // The list head term must NOT be `rdf:nil`; `DslView::rdf_list` already terminates
    // on it. Filter to IRI members (mirrors the historical named-node-only collection).
    if let DslTerm::Iri(iri) = head
        && iri == RDF_NIL
    {
        return Vec::new();
    }
    view.rdf_list(Some(head))
        .into_iter()
        .filter_map(|t| match t {
            DslTerm::Iri(iri) => Some(iri),
            _ => None,
        })
        .collect()
}

/// Every (named OR blank) subject of `?s a <type_iri>`, as a [`DslTerm`].
fn subject_terms_of_type(view: &DslView<'_>, type_iri: &str) -> Vec<DslTerm> {
    // The retired check carried blank-node Cell subjects; mirror that by scanning the
    // `rdf:type` predicate for both IRI and blank subjects (`subjects_of_type` keeps only
    // IRIs). Use `quads_with_predicate` to recover blank subjects too.
    view.quads_with_predicate(RDF_TYPE)
        .into_iter()
        .filter_map(|(s, o)| match (&s, &o) {
            (DslTerm::Iri(_), DslTerm::Iri(t)) if t == type_iri => Some(s),
            (DslTerm::Blank { .. }, DslTerm::Iri(t)) if t == type_iri => Some(s),
            _ => None,
        })
        .collect()
}

// ── CURIE / prefix helpers ────────────────────────────────────────────────────

/// Return the canonical CURIE prefix of `curie` if it names a known alignment target.
pub fn prefix_of(curie: &str) -> Option<String> {
    let (prefix, _) = curie.split_once(':')?;
    let canonical = TARGET_PREFIX_ALIASES
        .iter()
        .find(|(alias, _)| alias == &prefix)
        .map(|(_, canonical)| *canonical)
        .unwrap_or(prefix);
    if ALIGNMENT_TARGETS.contains(&canonical) {
        Some(canonical.to_owned())
    } else {
        None
    }
}

/// Expand a CURIE to an absolute IRI using the curated prefix registry.
pub fn expand_curie(curie: &str) -> Option<String> {
    let (prefix, local) = curie.split_once(':')?;
    let canonical = TARGET_PREFIX_ALIASES
        .iter()
        .find(|(alias, _)| alias == &prefix)
        .map(|(_, canonical)| *canonical)
        .unwrap_or(prefix);
    let ns = registry_iri(canonical)?;
    Some(format!("{ns}{local}"))
}

/// Render an absolute IRI as a CURIE using the curated prefix registry (longest-ns
/// match, registry order as tie-break). Reuses the shared `sssom_id` shortener — the
/// `ns_to_prefix` table is descending-namespace-length sorted (stable), so its tie-break
/// is the registry insertion order, matching the historical `shorten_iri`.
fn shorten_iri(iri: &str) -> String {
    sssom_id(iri, ns_to_prefix())
}

// ── Severity / ranking helpers ────────────────────────────────────────────────

fn severity_for(predicate_id: &str) -> &'static str {
    if STRONG_PROPERTY_PREDICATES.contains(&predicate_id) {
        "ERROR"
    } else {
        "WARNING"
    }
}

fn score_mapping(m: &Mapping) -> (i32, f64) {
    let rank = PREDICATE_RANK
        .iter()
        .find(|(p, _)| p == &m.predicate_id)
        .map(|(_, r)| *r)
        .unwrap_or(0);
    let conf = m.confidence.parse::<f64>().unwrap_or(0.0);
    (rank, conf)
}

fn rank_pair<'a>(a: &'a Mapping, b: &'a Mapping) -> (&'a Mapping, &'a Mapping) {
    if score_mapping(a) >= score_mapping(b) {
        (a, b)
    } else {
        (b, a)
    }
}

// ── Diagnostic builders ───────────────────────────────────────────────────────

fn info_unavailable(m: &Mapping, prefix: &str) -> ProjectionDiagnostic {
    ProjectionDiagnostic {
        severity: "INFO".to_owned(),
        check: "domain-range".to_owned(),
        code: "domain-range".to_owned(),
        message: format!(
            "skipped — no axioms available for target '{prefix}' \
             (vendor a snapshot or run with --network)"
        ),
        instance: expand_curie(&m.object_id),
        subject_id: Some(m.subject_id.clone()),
        predicate_id: Some(m.predicate_id.clone()),
        object_id: Some(m.object_id.clone()),
    }
}

fn info_not_checkable(m: &Mapping, reason: &str) -> ProjectionDiagnostic {
    ProjectionDiagnostic {
        severity: "INFO".to_owned(),
        check: "domain-range".to_owned(),
        code: "domain-range".to_owned(),
        message: format!("direction not checked — {reason}"),
        instance: expand_curie(&m.object_id),
        subject_id: Some(m.subject_id.clone()),
        predicate_id: Some(m.predicate_id.clone()),
        object_id: Some(m.object_id.clone()),
    }
}

/// Format a sorted IRI list as Python's `sorted(...)` list repr (`['a', 'b']`).
fn py_list_repr(items: &[String]) -> String {
    let inner = items
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

#[cfg(test)]
mod tests;
