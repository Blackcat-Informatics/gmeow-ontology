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

use serde::{Deserialize, Serialize};

use purrdf::slice::{
    ArtifactRecord, ArtifactRole, ManifestView, OwnershipAnalyzer, OwnershipReport, SliceCatalog,
    SliceError, SliceRecord, SliceTier,
};

use crate::i18n::{self, Translations, UiCatalog};
use crate::store::{Node, Object, Store};

// ── Namespace constants ───────────────────────────────────────────────────────

/// The GMEOW vocabulary namespace; IRIs under it get the `gmeow:` CURIE prefix.
use gmeow_ns::GMEOW_NS;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
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
const GMEOW_DOCUMENTATION_CONCERN: &str =
    "https://blackcatinformatics.ca/gmeow/DocumentationConcern";

const GMEOW_SSSOM_FILE: &str = "https://blackcatinformatics.ca/gmeow/sssomFile";
const GMEOW_SET_ID: &str = "https://blackcatinformatics.ca/gmeow/setId";
const GMEOW_LICENSE: &str = "https://blackcatinformatics.ca/gmeow/license";
const GMEOW_SET_COMMENT: &str = "https://blackcatinformatics.ca/gmeow/setComment";

// ── Per-term usage-advice predicates (rendered as the "Usage Advice" section) ────
const SKOS_SCOPE_NOTE: &str = "http://www.w3.org/2004/02/skos/core#scopeNote";
const SKOS_EXAMPLE: &str = "http://www.w3.org/2004/02/skos/core#example";
const GMEOW_USE_WHEN: &str = "https://blackcatinformatics.ca/gmeow/useWhen";
const GMEOW_AVOID_WHEN: &str = "https://blackcatinformatics.ca/gmeow/avoidWhen";
const GMEOW_HOW_TO_USE: &str = "https://blackcatinformatics.ca/gmeow/howToUse";
const GMEOW_USE_FOR_CONSUMER: &str = "https://blackcatinformatics.ca/gmeow/useForConsumer";
const GMEOW_AVOID_FOR_CONSUMER: &str = "https://blackcatinformatics.ca/gmeow/avoidForConsumer";

const GMEOW_DOCS_CONCERN: &str = "https://blackcatinformatics.ca/gmeow/docsConcern";

// ── Guides-slice predicates / classes (recipes + learning paths) ───────────────

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

// ── Logic stereotypes + relational surfaces ─────────────────────────────────────

/// The lowered-logic (OntoUML/UFO discipline) namespace; co-asserted `rdf:type`
/// values under it are surfaced as the term's logic stereotypes.
use gmeow_ns::LOGIC_NS;
const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";
/// `logic:PathShape` — a named, parametric predicate-path traversal specification
/// (design/LOGIC-PATHS.md). Its authored INSTANCES are first-class, reusable
/// by-name terms, so a GMEOW-namespaced subject typed with it is a documented term
/// (an [`DocTermCategory::Individual`]) whose projection-loss row joins its page.
const LOGIC_PATH_SHAPE: &str = "https://blackcatinformatics.ca/logic/PathShape";
/// `logic:preservationKind` — the preservation-polarity vocabulary object a
/// worked authored-example loss row declares (e.g. `logic:SoundUnderApproximation`,
/// `logic:ValidationOnly`).
const LOGIC_PRESERVATION_KIND: &str = "https://blackcatinformatics.ca/logic/preservationKind";
/// `logic:complexityClass` — the plain-literal complexity-class string a worked
/// authored-example loss row declares alongside its `logic:preservationKind`.
const LOGIC_COMPLEXITY_CLASS: &str = "https://blackcatinformatics.ca/logic/complexityClass";

/// The language-grounding vocabulary namespace (`slices/grounding/lang`). Its
/// slice owns first-class documented vocabulary in this namespace, exactly like
/// `logic:` / `math:`.
use gmeow_ns::LANG_NS;

// ── Math grounding: worked ℚ⁷ SI-dimension instances ─────────────────────────
/// The math-grounding vocabulary namespace (`slices/grounding/math`).
use gmeow_ns::MATH_NS;
/// `math:hasDimension` — the predicate a worked instance's subject (a
/// `Quantity`/`Integral`/`Measure`/measurable-function/…) declares its
/// dimension object through. The SUBJECT-discovery predicate for
/// [`extract_worked_instances`].
const MATH_HAS_DIMENSION: &str = "https://blackcatinformatics.ca/math/hasDimension";
/// `math:baseDimensionExponent` — a `math:DerivedDimension`'s edges to its
/// `math:DimensionExponent` individuals (one per non-trivially-exercised SI
/// base dimension). Absent (empty) for a dimensionless dimension object (e.g.
/// `math:dimensionless`) — an honest zero-exponent case, not a hard fail.
const MATH_BASE_DIMENSION_EXPONENT: &str =
    "https://blackcatinformatics.ca/math/baseDimensionExponent";
/// `math:exponentOfDimension` — a `math:DimensionExponent`'s edge to the SI
/// base-dimension IRI it exercises (e.g. `math:massDimension`).
const MATH_EXPONENT_OF_DIMENSION: &str = "https://blackcatinformatics.ca/math/exponentOfDimension";
/// `math:exponentNumerator` — an `xsd:integer` literal (may be negative).
const MATH_EXPONENT_NUMERATOR: &str = "https://blackcatinformatics.ca/math/exponentNumerator";
/// `math:exponentDenominator` — an `xsd:integer` literal.
const MATH_EXPONENT_DENOMINATOR: &str = "https://blackcatinformatics.ca/math/exponentDenominator";
/// `gmeow:unit` — a worked instance's QUDT unit realization, if it is a
/// `math:Quantity` (e.g. `<http://qudt.org/vocab/unit/J>`).
const GMEOW_UNIT: &str = "https://blackcatinformatics.ca/gmeow/unit";
/// `math:quantityValue` — a worked instance's `xsd:double` literal value, if it
/// is a `math:Quantity`.
const MATH_QUANTITY_VALUE: &str = "https://blackcatinformatics.ca/math/quantityValue";

// ── Constraint catalog (gmeow:ValidationRule individuals) ───────────────────────
/// The class every catalog subject is typed as in
/// `generated/catalog/constraint-catalog.nq`.
const GMEOW_VALIDATION_RULE: &str = "https://blackcatinformatics.ca/gmeow/ValidationRule";
const GMEOW_RULE_CODE: &str = "https://blackcatinformatics.ca/gmeow/ruleCode";
const GMEOW_RULE_CATEGORY: &str = "https://blackcatinformatics.ca/gmeow/ruleCategory";
const GMEOW_RULE_SEVERITY: &str = "https://blackcatinformatics.ca/gmeow/ruleSeverity";
const GMEOW_RULE_HELP_URI: &str = "https://blackcatinformatics.ca/gmeow/ruleHelpUri";
const GMEOW_APPLIES_TO_TERM: &str = "https://blackcatinformatics.ca/gmeow/appliesToTerm";
// The advice-catalog projection: the recommendation tier.
const GMEOW_ADVICE_ENTRY: &str = "https://blackcatinformatics.ca/gmeow/AdviceEntry";
const GMEOW_ADVICE_AVOID_WHEN: &str = "https://blackcatinformatics.ca/gmeow/adviceAvoidWhen";
const GMEOW_ADVICE_USE_WHEN: &str = "https://blackcatinformatics.ca/gmeow/adviceUseWhen";
const GMEOW_ADVICE_HOW_TO_USE: &str = "https://blackcatinformatics.ca/gmeow/adviceHowToUse";
const GMEOW_DOCUMENTED_BY_RULE: &str = "https://blackcatinformatics.ca/gmeow/documentedByRule";
/// `logic:instantiatesFramework` — the per-term reasoning-discipline selector;
/// its objects (closed `logic:LogicalFramework` individuals) surface as the
/// term's frameworks.
const LOGIC_INSTANTIATES_FRAMEWORK: &str =
    "https://blackcatinformatics.ca/logic/instantiatesFramework";

const SKOS_RELATED: &str = "http://www.w3.org/2004/02/skos/core#related";
const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";
const GMEOW_PAIRS_WITH: &str = "https://blackcatinformatics.ca/gmeow/pairsWith";
const GMEOW_GRAPH_BOX_ROLE: &str = "https://blackcatinformatics.ca/gmeow/graphBoxRole";

// ── Per-term lifecycle surface ───────────────────────────────────────────────────
// Deprecation is read through `gmeow_ns::DEPRECATED` (both the canonical
// `logic:deprecated` and its `owl:deprecated` OWL view).
const GMEOW_TERM_STABILITY: &str = "https://blackcatinformatics.ca/gmeow/termStability";
const GMEOW_ADDED_IN_VERSION: &str = "https://blackcatinformatics.ca/gmeow/addedInVersion";
const GMEOW_HAS_CHANGELOG_ENTRY: &str = "https://blackcatinformatics.ca/gmeow/hasChangelogEntry";
const GMEOW_ENTRY_VERSION: &str = "https://blackcatinformatics.ca/gmeow/entryVersion";
const GMEOW_ENTRY_NOTE: &str = "https://blackcatinformatics.ca/gmeow/entryNote";
const GMEOW_DEFINITION_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/definitionDigest";
const STABILITY_STABLE_CURIE: &str = "gmeow:stabilityStable";
const STABILITY_EXPERIMENTAL_CURIE: &str = "gmeow:stabilityExperimental";
const STABILITY_DEPRECATED_CURIE: &str = "gmeow:stabilityDeprecated";
/// The name of the everything-aggregation profile (root + every extension);
/// every documented term belongs to it (`full.ttl`).
const FULL_PROFILE_NAME: &str = "full";
const GMEOW_WORK: &str = "https://blackcatinformatics.ca/gmeow/Work";
const DCTERMS_IDENTIFIER: &str = "http://purl.org/dc/terms/identifier";

// ── SHACL constraint surface ─────────────────────────────────────────────────────

const SH_TARGET_CLASS: &str = "http://www.w3.org/ns/shacl#targetClass";
const SH_TARGET_SUBJECTS_OF: &str = "http://www.w3.org/ns/shacl#targetSubjectsOf";
const SH_TARGET_OBJECTS_OF: &str = "http://www.w3.org/ns/shacl#targetObjectsOf";
const SH_MESSAGE: &str = "http://www.w3.org/ns/shacl#message";

// ── Competency-question surface ──────────────────────────────────────────────────

const GMEOW_COMPETENCY_QUESTION: &str = "https://blackcatinformatics.ca/gmeow/CompetencyQuestion";
const GMEOW_CQ_RATIONALE: &str = "https://blackcatinformatics.ca/gmeow/cqRationale";
const GMEOW_CQ_QUERY_FILE: &str = "https://blackcatinformatics.ca/gmeow/cqQueryFile";
/// `gmeow:cqQuery` — the inline SPARQL literal, used instead of
/// [`GMEOW_CQ_QUERY_FILE`] by a CQ that embeds its query rather than pointing at
/// a committed `.rq` file. No authored competency question carries both.
const GMEOW_CQ_QUERY: &str = "https://blackcatinformatics.ca/gmeow/cqQuery";
/// `gmeow:cqExactRows` — an `xsd:boolean` pinning whether `cqExpectRow` is the
/// CLOSED result set (`true`) or a floor/subset (`false`/omitted → contains-check).
const GMEOW_CQ_EXACT_ROWS: &str = "https://blackcatinformatics.ca/gmeow/cqExactRows";
/// `gmeow:cqExpectRowCount` — an `xsd:integer` pinning an expected row COUNT
/// (e.g. `0` for a must-be-empty QC query), used instead of an enumerated
/// `cqExpectRow` list when only the cardinality — not the row content — matters.
const GMEOW_CQ_EXPECT_ROW_COUNT: &str = "https://blackcatinformatics.ca/gmeow/cqExpectRowCount";
const GMEOW_CQ_EXPECT_ROW: &str = "https://blackcatinformatics.ca/gmeow/cqExpectRow";
const GMEOW_ROW_CELL: &str = "https://blackcatinformatics.ca/gmeow/rowCell";
const GMEOW_CELL_VALUE_IRI: &str = "https://blackcatinformatics.ca/gmeow/cellValueIri";
/// `gmeow:cellVar` — the SPARQL projection variable name a cell binds (e.g.
/// `"neighbor"`).
const GMEOW_CELL_VAR: &str = "https://blackcatinformatics.ca/gmeow/cellVar";
/// `gmeow:cellValueLiteral` — a cell's expected literal lexical form, used
/// instead of [`GMEOW_CELL_VALUE_IRI`] when the bound variable is a literal
/// (e.g. a label or classification string) rather than an IRI.
const GMEOW_CELL_VALUE_LITERAL: &str = "https://blackcatinformatics.ca/gmeow/cellValueLiteral";

// ── Conformance-fixture Do/Don't binding surface ─────────────────────────────
// The fixtures themselves (`tests/conformance-fixtures/*.ttl` /
// `tests/counter-examples/*.ttl`) are pure ABox payloads carrying no
// `sh:message` or shape reference; the expected outcome / violation code /
// rationale live in a SEPARATE per-slice `tests/example-conformance.ttl`
// binding file, joined by slice-relative path (`gmeow:exampleFile`).

const GMEOW_EXAMPLE_CONFORMANCE: &str = "https://blackcatinformatics.ca/gmeow/ExampleConformance";
const GMEOW_EXAMPLE_FILE: &str = "https://blackcatinformatics.ca/gmeow/exampleFile";
const GMEOW_EXPECTED_OUTCOME: &str = "https://blackcatinformatics.ca/gmeow/expectedOutcome";
const GMEOW_EXPECTED_VIOLATION_CODE: &str =
    "https://blackcatinformatics.ca/gmeow/expectedViolationCode";
const GMEOW_CONFORMANCE_RATIONALE: &str =
    "https://blackcatinformatics.ca/gmeow/conformanceRationale";

// ── Build-pipeline DAG surface (slices/core/pipeline/module.ttl) ────────────
// The dogfooded build graph authored as data: `gmeow:PipelineStage` individuals
// gathered on the one `gmeow:Pipeline` (`gmeow:pipeline-build`) through
// `gmeow:hasStage`, wired by bare `gmeow:dataflowConsumes` edges and refined by
// reified `gmeow:BuildDataFlow` edges that name the flowing named graphs.

const GMEOW_PIPELINE: &str = "https://blackcatinformatics.ca/gmeow/Pipeline";
const GMEOW_PIPELINE_STAGE: &str = "https://blackcatinformatics.ca/gmeow/PipelineStage";
const GMEOW_BUILD_DATA_FLOW: &str = "https://blackcatinformatics.ca/gmeow/BuildDataFlow";
const GMEOW_STAGE_IMPL: &str = "https://blackcatinformatics.ca/gmeow/stageImpl";
const GMEOW_HAS_CAPABILITY: &str = "https://blackcatinformatics.ca/gmeow/hasCapability";
const GMEOW_REQUIRES_RESOURCE: &str = "https://blackcatinformatics.ca/gmeow/requiresResource";
const GMEOW_DATAFLOW_CONSUMES: &str = "https://blackcatinformatics.ca/gmeow/dataflowConsumes";
const GMEOW_BUILD_FLOW_FROM: &str = "https://blackcatinformatics.ca/gmeow/buildFlowFrom";
const GMEOW_BUILD_FLOW_TO: &str = "https://blackcatinformatics.ca/gmeow/buildFlowTo";
const GMEOW_FLOW_ENTITY: &str = "https://blackcatinformatics.ca/gmeow/flowEntity";
/// `gmeow:attachesGraph` — a named-graph IRI a `gmeow:PipelineStage` attaches to the
/// carrier as its delta (the stage's declared, run-verified contribution).
const GMEOW_ATTACHES_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/attachesGraph";
/// `gmeow:attachesBlobRep` — a blob-representation lane label a stage attaches.
const GMEOW_ATTACHES_BLOB_REP: &str = "https://blackcatinformatics.ca/gmeow/attachesBlobRep";
/// `logic:planGoal` — the `gmeow:Goal` the `gmeow:Pipeline` (a `logic:Plan`) is
/// arranged to reach (the shippable bundle).
const LOGIC_PLAN_GOAL: &str = "https://blackcatinformatics.ca/logic/planGoal";
/// `logic:planSuccessMode` — the plan's declared success mode (e.g.
/// `logic:StrongPlanSuccess`).
const LOGIC_PLAN_SUCCESS_MODE: &str = "https://blackcatinformatics.ca/logic/planSuccessMode";

// ── Grounding seams (`gmeow:Seam` individuals) ──────────────────────────────
// The closed set of sanctioned cross-grounding reference channels (Principle
// 19), authored as canonical governance DATA in a grounding slice's
// `manifest.ttl` (today, `logic:`'s) rather than hand-maintained prose.
// Discovered generically: [`extract_seams`] scans the `manifest_graph` of
// EVERY slice typed `gmeow:GroundingSlice`, so a future seam authored in
// `lang:`/`math:` is picked up without a code change.

/// The marker class typing a grounding slice (`logic:`/`lang:`/`math:`) in its
/// own `manifest.ttl` — the machine-checked signal [`extract_seams`] uses to
/// find every manifest that may carry `gmeow:Seam` individuals.
const GMEOW_GROUNDING_SLICE: &str = "https://blackcatinformatics.ca/gmeow/GroundingSlice";
/// `gmeow:Seam` — a sanctioned cross-grounding reference channel.
const GMEOW_SEAM: &str = "https://blackcatinformatics.ca/gmeow/Seam";
/// `gmeow:seamDirection` — a seam's directed (from, to) legs (blank nodes).
const GMEOW_SEAM_DIRECTION: &str = "https://blackcatinformatics.ca/gmeow/seamDirection";
/// `gmeow:seamFromSlice` — a seam-direction leg's referencing grounding slice.
const GMEOW_SEAM_FROM_SLICE: &str = "https://blackcatinformatics.ca/gmeow/seamFromSlice";
/// `gmeow:seamToSlice` — a seam-direction leg's referenced grounding slice.
const GMEOW_SEAM_TO_SLICE: &str = "https://blackcatinformatics.ca/gmeow/seamToSlice";
/// `gmeow:seamCarryingTerm` — an exact term IRI a seam sanctions crossing it.
const GMEOW_SEAM_CARRYING_TERM: &str = "https://blackcatinformatics.ca/gmeow/seamCarryingTerm";
/// `gmeow:seamOwningDoc` — the design-doc filename that owns a seam's theory.
const GMEOW_SEAM_OWNING_DOC: &str = "https://blackcatinformatics.ca/gmeow/seamOwningDoc";

/// An error building the documentation model.
#[derive(Debug)]
pub enum DocsError {
    /// A slice-catalog discovery / parse error.
    Slice(SliceError),
    /// The committed constraint-catalog fanout artifact
    /// (`generated/catalog/constraint-catalog.nq`) is missing, unreadable,
    /// unparsable, or carries a malformed `gmeow:ValidationRule` individual.
    ConstraintCatalog(String),
    /// The cross-slice `dsl/mappings/mapping-sets.ttl` publication-header file
    /// is present but unreadable or unparsable. Absent is fine (slices carry
    /// their own sets); a present-but-broken file is a hard failure, never a
    /// silent empty — a dropped `MappingSet` would leave relocated linkage
    /// resolving its set IRI to the raw filename.
    MappingSets(String),
    /// A slice-owned mapping artifact (`mappings/*.ttl`, `ArtifactRole::Mapping`)
    /// is present but will not parse as Turtle. Carries the owning slice IRI, the
    /// slice-relative source path, and the parser diagnostic. A mapping file that
    /// cannot be read contributes ZERO `gmeow:MappingSet` headers and ZERO
    /// alignment cells; swallowing the parse error would silently subtract its
    /// whole contribution from the published linkage index (the term-equivalence
    /// count that evidences a relocation preserved the grounding corpus), so the
    /// defect is raised here rather than absorbed into a smaller number.
    MappingParse {
        /// The owning slice IRI.
        slice_iri: String,
        /// The offending slice-relative mapping source path.
        source_path: String,
        /// The underlying Turtle parser diagnostic, preserved verbatim.
        detail: String,
    },
    /// The committed term content manifest
    /// (`generated/catalog/term-content-manifest.nq`) is missing, unreadable,
    /// unparsable, carries a term with no `gmeow:definitionDigest`, or omits a
    /// documented term (a coverage gap). A regenerated tree always carries a
    /// complete, well-formed manifest, so any of these is a broken invariant, never
    /// an optional input.
    TermManifest(String),
    /// A competency question declares `gmeow:cqQueryFile` (a repo-root-relative
    /// `.rq` path) but the file could not be read at that path. `cqQueryFile`
    /// existing is the ontology's own claim that a resolvable query file exists
    /// (mirroring the executing competency harness's own
    /// `crates/slicetest/src/paths.rs::query_file` resolution), so a dangling
    /// reference is a data bug, never an honest absence to swallow as `None`.
    CompetencyQuery(String),
    /// A `text/markdown` source under a slice carries invalid UTF-8, so it cannot be
    /// read into the strict-UTF-8 [`DocMarkdownDocument`] model. Carries the owning
    /// slice IRI and the offending slice-relative source path. There is no
    /// `from_utf8_lossy` fallback — a malformed source is a data bug surfaced loudly
    /// with its path, never silently mojibake'd.
    MarkdownUtf8 {
        /// The owning slice IRI.
        slice_iri: String,
        /// The offending slice-relative markdown source path.
        source_path: String,
    },
    /// Two `text/markdown` sources in ONE slice normalize to the SAME logical path,
    /// so one would silently shadow the other. Carries the owning slice IRI and the
    /// colliding normalized path.
    MarkdownPathCollision {
        /// The owning slice IRI.
        slice_iri: String,
        /// The normalized logical path two distinct sources both claim.
        source_path: String,
    },
    /// Two [`DocMarkdownDocument`]s map to the SAME generated page path, so one page
    /// would overwrite the other. Carries the colliding page path and the two
    /// `slice-iri :: source-path` document identities.
    MarkdownPageCollision {
        /// The generated page path two documents both claim.
        page: String,
        /// The first document's `slice-iri :: source-path` identity.
        first: String,
        /// The second document's `slice-iri :: source-path` identity.
        second: String,
    },
}

impl std::fmt::Display for DocsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocsError::Slice(e) => write!(f, "slice catalog error: {e}"),
            DocsError::ConstraintCatalog(msg) => write!(f, "constraint catalog error: {msg}"),
            DocsError::MappingSets(msg) => write!(f, "central mapping-sets error: {msg}"),
            DocsError::MappingParse {
                slice_iri,
                source_path,
                detail,
            } => write!(
                f,
                "mapping artifact `{source_path}` in slice {slice_iri} will not parse as Turtle \
                 (its mapping sets and alignment cells would silently vanish from the linkage \
                 index): {detail}"
            ),
            DocsError::TermManifest(msg) => write!(f, "term content manifest error: {msg}"),
            DocsError::CompetencyQuery(msg) => write!(f, "competency query file error: {msg}"),
            DocsError::MarkdownUtf8 {
                slice_iri,
                source_path,
            } => write!(
                f,
                "markdown source `{source_path}` in slice {slice_iri} is not valid UTF-8"
            ),
            DocsError::MarkdownPathCollision {
                slice_iri,
                source_path,
            } => write!(
                f,
                "two markdown sources in slice {slice_iri} normalize to the same logical path `{source_path}`"
            ),
            DocsError::MarkdownPageCollision {
                page,
                first,
                second,
            } => write!(
                f,
                "two documents map to the same generated page path `{page}`: {first} and {second}"
            ),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// A first-class, strictly-decoded Markdown source document owned by a slice.
///
/// Every `text/markdown` artifact recursively discovered in the slice becomes one
/// of these — selected by MEDIA TYPE, never by [`ArtifactRole`], so a
/// `design/*.md` file (classified `ArtifactRole::Other`) is a first-class document
/// exactly like the top-level `docs.md`. The source text is decoded with STRICT
/// `std::str::from_utf8` (never `from_utf8_lossy`), so a malformed source is a
/// hard failure carrying its path, not silent mojibake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocMarkdownDocument {
    /// The owning slice IRI (`a gmeow:Slice`).
    pub slice_iri: String,
    /// The owning slice's filesystem-safe slug (the `{slug}` in its
    /// `slices/{slug}/…` page space) — carried so the document self-describes its
    /// generated URL space without re-deriving it from the IRI.
    pub slice_slug: String,
    /// The normalized logical source path, relative within the slice: forward-slash
    /// separators, no leading `./` and no leading `/` (e.g. `design/ARCHITECTURE.md`).
    pub source_path: String,
    /// The document title: the first ATX (`#`) H1 of the source when present, else a
    /// humanized fallback from the filename stem (`design/ARCHITECTURE.md` →
    /// `Architecture`).
    pub title: String,
    /// The STRICT-UTF-8 decoded source text (via `std::str::from_utf8`).
    pub source_text: String,
    /// The raw digest already carried by the artifact record (`raw_digest`) — the
    /// content address of the source bytes.
    pub raw_digest: String,
}

/// Normalize an artifact's slice-relative logical path for the document model:
/// forward-slash separators, no leading `./`, no leading `/`. The purrdf slice
/// classifier already yields a relative path with no `..` and no leading `/`, so
/// this is a light idempotent fold, not a `..`-resolving canonicalizer.
pub(crate) fn normalize_logical_path(path: &str) -> String {
    let mut p = path.replace('\\', "/");
    while let Some(stripped) = p.strip_prefix("./") {
        p = stripped.to_string();
    }
    p.trim_start_matches('/').to_string()
}

/// Humanize a markdown filename stem into a title-cased fallback title:
/// `ARCHITECTURE` → `Architecture`, `getting-started` → `Getting Started`. Each
/// `-`/`_`/`.`-separated word is title-cased (first char upper, rest lower), so an
/// all-caps stem reads as a word rather than a shout.
fn humanize_stem(stem: &str) -> String {
    let words: Vec<String> = stem
        .split(['-', '_', '.', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect();
    if words.is_empty() {
        "Untitled".to_string()
    } else {
        words.join(" ")
    }
}

/// Resolve a markdown document title: the first ATX (`#`) H1 line of the source
/// (one leading `#` then whitespace; any trailing closing `#` run trimmed), else a
/// [`humanize_stem`] fallback over the source path's filename stem.
pub(crate) fn markdown_title(source: &str, source_path: &str) -> String {
    for line in source.lines() {
        let t = line.trim();
        // An ATX H1 is exactly one `#` followed by whitespace (a `##` heading is
        // not an H1). A setext H1 (`===` underline) is not matched — the ATX form
        // is the project's authored convention.
        if let Some(rest) = t.strip_prefix('#')
            && !rest.starts_with('#')
            && rest.starts_with(char::is_whitespace)
        {
            let title = rest.trim().trim_end_matches('#').trim();
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }
    let stem = source_path
        .rsplit('/')
        .next()
        .unwrap_or(source_path)
        .strip_suffix(".md")
        .unwrap_or(source_path);
    humanize_stem(stem)
}

impl DocMarkdownDocument {
    /// Collect every `text/markdown` artifact in `record` into a strictly-decoded,
    /// path-sorted document set. Selection is by `media_type == "text/markdown"`
    /// over EVERY recursively-discovered artifact (never by `ArtifactRole`).
    ///
    /// Hard-fails (a real `Err` naming the offending source path) on invalid UTF-8
    /// ([`DocsError::MarkdownUtf8`]) and on two sources normalizing to the same
    /// logical path ([`DocsError::MarkdownPathCollision`]). No lossy fallback, no
    /// silent skip.
    fn collect(
        record: &SliceRecord,
        slice_iri: &str,
        slice_slug: &str,
    ) -> Result<Vec<DocMarkdownDocument>, DocsError> {
        // A BTreeMap keyed by normalized path yields the path-sorted output order
        // deterministically and makes the collision check a simple insert probe.
        let mut by_path: BTreeMap<String, DocMarkdownDocument> = BTreeMap::new();
        for artifact in &record.artifacts {
            if artifact.media_type != "text/markdown" {
                continue;
            }
            let source_text = std::str::from_utf8(&artifact.content)
                .map_err(|_| DocsError::MarkdownUtf8 {
                    slice_iri: slice_iri.to_string(),
                    source_path: artifact.logical_path.clone(),
                })?
                .to_string();
            let source_path = normalize_logical_path(&artifact.logical_path);
            let title = markdown_title(&source_text, &source_path);
            let doc = DocMarkdownDocument {
                slice_iri: slice_iri.to_string(),
                slice_slug: slice_slug.to_string(),
                source_path: source_path.clone(),
                title,
                source_text,
                raw_digest: artifact.raw_digest.clone(),
            };
            if by_path.insert(source_path.clone(), doc).is_some() {
                return Err(DocsError::MarkdownPathCollision {
                    slice_iri: slice_iri.to_string(),
                    source_path,
                });
            }
        }
        Ok(by_path.into_values().collect())
    }
}

/// A documented slice: manifest identity + its artifact inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// membership in (sorted). Drives per-term profile chips.
    pub profiles: Vec<String>,
    /// `gmeow:sliceDependsOn` slice IRIs (sorted). The relation whose closure
    /// over a profile's declared members yields the profile's full membership;
    /// reused to compute per-term profile membership.
    pub depends_on: Vec<String>,
    /// All artifacts in the slice (sorted by logical path).
    pub artifacts: Vec<DocArtifact>,
    /// Every `text/markdown` source in the slice as a first-class, strictly-decoded
    /// [`DocMarkdownDocument`], selected by media type (never by `ArtifactRole`) and
    /// sorted deterministically by normalized logical path. The top-level `docs.md`
    /// appears here alongside `design/*.md` and any other markdown.
    #[serde(default)]
    pub documents: Vec<DocMarkdownDocument>,
    /// Deterministic `docs.md` fact: the slice's `docs.md` opens with a thesis
    /// sentence (a prose sentence, not a heading/table/list). Drives the
    /// slice-scoped `gmeow:dimThesisSentence` coverage dimension. Computed in
    /// [`DocSlice::from_record`] from the `docs.md` artifact so the coverage
    /// producer stays a pure function of the model.
    #[serde(default)]
    pub has_thesis_sentence: bool,
    /// Deterministic `docs.md` fact: every documented artifact in the slice's
    /// `docs.md` design-set table (a table with a "realized state" column) carries
    /// a realized-state marker (design-only / partial / built). Drives the
    /// slice-scoped `gmeow:dimRealizedState` coverage dimension. `false` when the
    /// slice ships no such table (a gated omission, not authorial vigilance).
    #[serde(default)]
    pub realized_state_complete: bool,
}

/// Deterministic detection of a `docs.md` opening thesis sentence: at least one
/// prose line — trimmed non-empty, beginning with an alphabetic character (so
/// headings `#`, tables `|`, block-quotes `>`, list markers `-`/`*`/`+`, and code
/// fences ` ``` ` are excluded) — that carries a sentence-ending period. A
/// present/absent structural fact over the narrative, never a tuned length.
fn detect_thesis_sentence(md: &str) -> bool {
    md.lines().any(|line| {
        let t = line.trim();
        t.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && t.contains('.')
    })
}

/// The interior cells of a markdown table row, preserving empty interior cells.
fn md_row_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return Vec::new();
    }
    let inner = trimmed
        .strip_prefix('|')
        .and_then(|s| s.strip_suffix('|'))
        .unwrap_or(trimmed);
    inner.split('|').map(|c| c.trim().to_string()).collect()
}

/// True if a markdown table row is the `|---|:--:|` separator (only dashes,
/// colons, spaces between the pipes).
fn is_table_separator(line: &str) -> bool {
    let cells = md_row_cells(line);
    !cells.is_empty()
        && cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' '))
}

/// Deterministic detection of a complete realized-state design-set table: the
/// `docs.md` carries a markdown table with a "realized state" header column, and
/// EVERY data row's cell in that column carries a realized-state marker
/// (design / partial / built). Returns `false` when no such table exists — a
/// silent omission is a gated miss, not authorial vigilance.
fn detect_realized_state_complete(md: &str) -> bool {
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let header = md_row_cells(lines[i]);
        let realized_col = header
            .iter()
            .position(|c| c.to_lowercase().contains("realized"));
        // A header row with a "realized" column, followed by a separator row.
        if let Some(col) = realized_col
            && i + 1 < lines.len()
            && is_table_separator(lines[i + 1])
        {
            let mut all_marked = true;
            let mut j = i + 2;
            while j < lines.len() && lines[j].trim().starts_with('|') {
                if is_table_separator(lines[j]) {
                    j += 1;
                    continue;
                }
                let cells = md_row_cells(lines[j]);
                let marked = cells.get(col).is_some_and(|cell| {
                    let c = cell.to_lowercase();
                    // The realized-state markers: `realized` / `built` (fully
                    // realized), `partial`, and `design-only` / `design` (not yet
                    // built). A row whose realized-state cell names none of these
                    // carries no honest marker and misses the dimension.
                    c.contains("realized")
                        || c.contains("built")
                        || c.contains("partial")
                        || c.contains("design")
                });
                if !marked {
                    all_marked = false;
                }
                j += 1;
            }
            return all_marked;
        }
        i += 1;
    }
    false
}

impl DocSlice {
    fn from_record(record: &SliceRecord) -> Result<Self, DocsError> {
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

        // Deterministic docs.md facts — read the slice's `docs.md` (an
        // ArtifactRole::Documentation artifact carrying its bytes) once so the
        // coverage producer stays a pure function of the model. Absent docs.md ⇒
        // both facts false (a gated miss, honest absence).
        let docs_md: Option<String> = record
            .artifacts
            .iter()
            .find(|a| {
                a.role == ArtifactRole::Documentation
                    && Path::new(&a.logical_path)
                        .file_name()
                        .is_some_and(|n| n == "docs.md")
            })
            .map(|a| String::from_utf8_lossy(&a.content).into_owned());
        let has_thesis_sentence = docs_md.as_deref().is_some_and(detect_thesis_sentence);
        let realized_state_complete = docs_md
            .as_deref()
            .is_some_and(detect_realized_state_complete);

        let mut creators = creators.clone();
        creators.sort();
        let mut consumers = consumers.clone();
        consumers.sort();
        // `extract_manifest_view` already sorts + dedups both vectors before
        // populating `ManifestView` (crates/slice/src/catalog.rs), so they
        // arrive deterministically ordered — no re-sort needed here.
        let profiles = profiles.clone();
        let depends_on = depends_on.clone();

        // Every `text/markdown` source as a first-class strictly-decoded document
        // (hard-fails on invalid UTF-8 or a normalized-path collision). The slice
        // slug is derived from the IRI directly — the same slug the renderer and RDF
        // projection use for this slice's `slices/{slug}/…` page space.
        let slice_slug = crate::render::slice_slug_of_iri(slice_iri);
        let documents = DocMarkdownDocument::collect(record, slice_iri, &slice_slug)?;

        Ok(Self {
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
            has_thesis_sentence,
            realized_state_complete,
            documents,
        })
    }

    /// A bare slice carrying only an IRI and a document set — for the
    /// [`crate::source_map`] unit tests that exercise the page map over a
    /// hand-built model without the full catalog machinery.
    #[cfg(test)]
    pub(crate) fn bare_for_test(iri: &str, documents: Vec<DocMarkdownDocument>) -> Self {
        Self {
            iri: iri.to_string(),
            label: None,
            title: None,
            tier: None,
            identifier: None,
            creators: Vec::new(),
            consumers: Vec::new(),
            profiles: Vec::new(),
            depends_on: Vec::new(),
            artifacts: Vec::new(),
            has_thesis_sentence: false,
            realized_state_complete: false,
            documents,
        }
    }
}

/// The maturity status of a vocabulary term. Serializes as a lowercase
/// string (`stable` / `experimental` / `deprecated`). Resolved from an explicit
/// `gmeow:termStability` annotation, else `owl:deprecated`, else the owner
/// slice's tier (core → stable, extension → experimental).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
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

/// One reified per-release changelog entry for a term. Ordered by
/// `(version, note)` for deterministic rendering.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct DocChangelogEntry {
    /// `gmeow:entryVersion` — the release this entry pertains to.
    pub version: String,
    /// `gmeow:entryNote` — optional prose describing the change (English carrier).
    pub note: Option<String>,
}

/// A documented vocabulary term parsed from a slice's `module.ttl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DocTerm {
    /// The full term IRI.
    pub iri: String,
    /// The INJECTIVE documentation-entry slug — the collision-free `{slug}` in this
    /// term's `documentation/term/{slug}` doc-entry IRI, page URL, and cross-page
    /// links. Resolved ONCE from the whole term set at model build (see
    /// [`crate::render::resolve_term_slugs`]) so no two distinct term IRIs share a
    /// doc-entry subject (which would conflate their coverage incidence). Empty only
    /// on a hand-built term that never went through model resolution;
    /// [`crate::render::term_slug`] then falls back to the base slug.
    #[serde(default)]
    pub slug: String,
    /// The compact CURIE (`gmeow:Foo` for GMEOW-namespaced terms, else the IRI).
    pub curie: String,
    /// `rdfs:label`.
    pub label: Option<String>,
    /// `skos:definition` (falling back to `rdfs:comment`).
    pub definition: Option<String>,
    /// The CANONICAL (authored English) `rdfs:label`, stashed by
    /// `render::localize_model` just before it overwrites [`Self::label`]
    /// with a translation. Documentation-COMPLETENESS is a property of the authored
    /// source, not the display language — viewing a term in French must not change
    /// its completeness score — so [`Self::coverage_label`] reads this in preference
    /// to the (possibly translated) display label. `None` on a canonical
    /// (English / unlocalized) model, where [`Self::label`] is already canonical.
    /// A pure in-memory render detail: NEVER serialized, so the persisted model and
    /// its golden snapshots are unchanged.
    #[serde(skip)]
    pub canonical_label: Option<String>,
    /// The CANONICAL (authored English) definition, the completeness-scoring twin of
    /// [`Self::canonical_label`] for [`Self::definition`]. See [`Self::coverage_definition`].
    #[serde(skip)]
    pub canonical_definition: Option<String>,
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
    /// `gmeow:adoptionTarget` — external-vocabulary prefix strings the term
    /// DECLARES a correspondence intent toward (sorted, deduped). A non-empty set
    /// is the term's own claim that it should carry an external crosswalk, and is
    /// one of the signals that makes the `dimAlignment` / `dimLinkageCoverage`
    /// coverage dimensions APPLICABLE to it (see [`crate::coverage`]). Empty on a
    /// superset-native term that maps to nothing external — an honest absence, not
    /// a coverage defect.
    #[serde(default)]
    pub adoption_targets: Vec<String>,
    /// Logic stereotypes co-asserted as `rdf:type` values in the `logic:`
    /// namespace (`logic:Kind`, `logic:SubKind`, `logic:Relator`, …), rendered
    /// as `logic:`-prefixed CURIEs, sorted/deduped. The lowered OntoUML/UFO
    /// discipline of the term (see `slices/grounding/logic`).
    pub logic_stereotypes: Vec<String>,
    /// `logic:instantiatesFramework` — the closed `logic:LogicalFramework`
    /// reasoning discipline(s) the term traffics in (`logic:HolonicFramework`,
    /// `logic:DeonticFramework`, …), rendered as `logic:`-prefixed CURIEs,
    /// sorted/deduped. Empty when the term traffics in no special discipline
    /// (honest absence — not every term inhabits a framework).
    pub frameworks: Vec<String>,
    /// Related-term IRIs: the union of `skos:related`, `gmeow:pairsWith`, and
    /// `rdfs:seeAlso` objects, resolved BIDIRECTIONALLY in `from_catalog`
    /// (sorted/deduped).
    pub related_terms: Vec<String>,
    /// `gmeow:graphBoxRole` — the four-boxes role CURIE (`gmeow:boxTBox`,
    /// `gmeow:boxABox`, …); the lowest-sorted when multiply asserted.
    pub box_role: Option<String>,
    /// `gmeow:graphBoxRole` — ALL asserted four-boxes role CURIEs (sorted/deduped).
    /// The full set, mirroring the folded snapshot's `Term::box_roles`, so the
    /// shared term card carries every box role, not just the first.
    pub box_roles: Vec<String>,
    /// Reverse `logic:formalizes` back-references: the IRIs of logic axioms /
    /// subjects that declare `logic:formalizes <this term>` (sorted/deduped).
    /// Empty until the central logic slice carries such back-refs.
    pub formalized_by: Vec<String>,
    /// The term's maturity badge, always resolved: explicit
    /// `gmeow:termStability` > `owl:deprecated` > owner-slice tier default.
    pub stability: DocTermStability,
    /// `gmeow:addedInVersion` — the release a term first appeared in (the
    /// lowest-sorted literal when multiply asserted); `None` until seeded.
    pub added_in_version: Option<String>,
    /// `gmeow:hasChangelogEntry` — reified per-release change records, sorted by
    /// `(version, note)`. Unions the authored changelog with the computed changelog
    /// read from the term content manifest. Empty when the term carries neither.
    pub changelog: Vec<DocChangelogEntry>,
    /// `gmeow:definitionDigest` — the RDFC-1.0-canonical blake3 content-address of
    /// the term's defining triples, read from the committed term content manifest
    /// (`generated/catalog/term-content-manifest.nq`). Always populated on a
    /// discovered model; empty on a bare `from_catalog` model until the manifest is
    /// applied.
    pub content_digest: String,
    /// The named profiles whose membership closure includes this term's owner
    /// slice, plus the always-present `full` aggregate (sorted/deduped).
    /// Computed in `from_catalog` from the slices' `sliceProfile` /
    /// `sliceDependsOn` declarations.
    pub profiles: Vec<String>,
}

impl DocTerm {
    /// The CANONICAL (authored English) label for documentation-completeness
    /// scoring — [`Self::canonical_label`] when a localized render stashed it,
    /// else [`Self::label`] (already canonical on an English / unlocalized model).
    ///
    /// Completeness is a property of the authored source, not the display language,
    /// so every coverage predicate reads through this rather than the possibly
    /// translated display label — keeping the completeness score (and the badge
    /// assets derived from it) byte-identical across languages.
    pub fn coverage_label(&self) -> Option<&str> {
        self.canonical_label.as_deref().or(self.label.as_deref())
    }

    /// The CANONICAL (authored English) definition for documentation-completeness
    /// scoring — the twin of [`Self::coverage_label`] for [`Self::definition`].
    pub fn coverage_definition(&self) -> Option<&str> {
        self.canonical_definition
            .as_deref()
            .or(self.definition.as_deref())
    }
}

/// A cross-slice dependency edge projected from the ownership report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// A single native alignment cell — a cross-walk from a GMEOW term to an external
/// IRI via a SKOS-style (`skos:*Match`) alignment predicate.
///
/// `confidence` is an `f64`, so this type is `PartialEq` but not `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocLinkage {
    /// The mapping-set IRI this equivalence belongs to (by `gmeow:sssomFile`).
    pub mapping_set: Option<String>,
    /// The GMEOW term IRI (the match subject).
    pub subject: String,
    /// The subject as a CURIE.
    pub subject_curie: String,
    /// The match predicate — e.g. `skos:closeMatch`.
    pub predicate: String,
    /// The match object — the external IRI.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Whether a [`DocFixture`] is a well-formed conformance instance or a
/// deliberately malformed counter-example.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DocFixtureKind {
    /// A `tests/conformance-fixtures/*.ttl` instance that MUST validate.
    Wellformed,
    /// A `tests/counter-examples/*.ttl` instance that MUST be rejected.
    CounterExample,
}

/// A conformance Do/Don't fixture — a well-formed instance
/// ([`DocFixtureKind::Wellformed`]) or a deliberately malformed counter-example
/// ([`DocFixtureKind::CounterExample`]), carried in full (small Turtle text,
/// not a blob). The fixture file itself is a pure ABox payload with no
/// `sh:message` or shape reference; the expected outcome, violation code, and
/// rationale — when the slice authors a binding — are joined in from that
/// slice's `tests/example-conformance.ttl` (`gmeow:ExampleConformance`) by
/// slice-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocFixture {
    /// The slice IRI that owns the fixture.
    pub slice: String,
    /// The logical path within the slice directory (e.g.
    /// `tests/counter-examples/plan-missing-successmode.ttl`).
    pub logical_path: String,
    /// A human title (an `rdfs:label` if any subject carries one, else derived
    /// from the filename — mirrors [`DocExample::title`]).
    pub title: String,
    /// The Turtle source, carried in full.
    pub text: String,
    /// Well-formed instance or counter-example.
    pub kind: DocFixtureKind,
    /// GMEOW CURIEs referenced anywhere in the fixture (sorted, deduped) —
    /// reuses [`DocExample`]'s term-reference extraction exactly.
    pub terms_referenced: Vec<String>,
    /// `gmeow:expectedOutcome`'s local name (`"conforms"` | `"violates"`), from
    /// this slice's `tests/example-conformance.ttl` binding. `None` when the
    /// fixture carries no authored binding — an honest absence (not every
    /// fixture is bound today).
    pub expected_outcome: Option<String>,
    /// `gmeow:expectedViolationCode` (e.g. `"shacl.MinCountConstraintComponent"`).
    /// `None` for a well-formed fixture or an unbound counter-example.
    pub violation_code: Option<String>,
    /// `gmeow:conformanceRationale` — the human-readable "why" the fixture
    /// conforms or violates. `None` when unbound.
    pub rationale: Option<String>,
    /// The constraint-catalog rule slug [`violation_code`](Self::violation_code)
    /// resolves to, when a genuine [`ConstraintRule::code`] match exists in
    /// [`DocsModel::constraint_rules`]. `None` when the fixture is unbound,
    /// well-formed, or (the common case today) the catalog carries no
    /// per-constraint-component rule matching the code — NEVER fabricated to
    /// avoid an absent link.
    pub catalog_slug: Option<String>,
}

/// A SHACL node shape, reverse-mapped to the term it constrains. Parsed from a
/// slice's `shapes.ttl` (`ArtifactRole::Shapes`) and the root `shapes/*.ttl`
/// files. DISTINCT from the integrity-constraint index (SPARQL verify queries):
/// this surface is SHACL structural validation per target term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
/// it exercises, so each term page can surface a "Tested by" block, and to a
/// full copy-paste-runnable SPARQL query + its expected result, so
/// `Page::CompetencyIndex` can render the whole question standalone. Parsed
/// from each slice's `tests/competency.ttl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DocCompetency {
    /// The competency-question IRI.
    pub iri: String,
    /// `gmeow:cqRationale` — why the ontology must answer this.
    pub rationale: Option<String>,
    /// `gmeow:cqQueryFile` — the REPO-ROOT-RELATIVE SPARQL query path, as
    /// authored (kept verbatim for citation/display even once resolved). Some
    /// slices author their own `.rq` under `slices/<group>/<name>/queries/…`;
    /// others point at the shared `queries/competency/…` (or `queries/qc/…`)
    /// tree at the repo root — both are legitimate repo-root-relative paths
    /// (mirrors `crates/slicetest/src/paths.rs::query_file`, the executing
    /// harness's own resolution contract). `None` when the CQ instead embeds
    /// its query inline via `gmeow:cqQuery`.
    pub query_file: Option<String>,
    /// The resolved SPARQL query body: `gmeow:cqQuery`'s inline literal when
    /// present, or the text read from [`query_file`](Self::query_file) via
    /// [`DocsModel::discover`]'s `apply_competency_query_text` pass (needs the
    /// repo root, so `extract_competency` cannot resolve it itself). `None`
    /// only when the CQ carries neither predicate (never happens for a
    /// well-formed `competency.ttl`, but is not asserted here — `dsl::load_spec`
    /// is the enforcement point for the harness; this is a docs *read*, not a
    /// re-validation of the DSL). A `query_file` that fails to resolve to an
    /// existing file is a hard fail (see `DocsError::CompetencyQuery`), never a
    /// silent `None`.
    pub query_text: Option<String>,
    /// `gmeow:cqExactRows` — `Some(true)` when `expected_rows` is the CLOSED
    /// result set, `Some(false)` when it is a floor/subset (contains-check),
    /// `None` when the CQ declares neither (most common for a subset check).
    pub exact_rows: Option<bool>,
    /// `gmeow:cqExpectRowCount` — an expected row COUNT (e.g. `0` for a
    /// must-be-empty QC query), used instead of an enumerated `expected_rows`
    /// list when only cardinality matters. `None` for a CQ that enumerates rows.
    pub expected_row_count: Option<i64>,
    /// The enumerated expected rows (`gmeow:cqExpectRow` → `gmeow:rowCell`),
    /// each a set of variable/value bindings, in [`GMEOW_CQ_EXPECT_ROW`]'s
    /// deterministic (row-IRI-sorted) order. Empty when the CQ instead pins
    /// [`expected_row_count`](Self::expected_row_count) or neither.
    pub expected_rows: Vec<DocExpectedRow>,
    /// The term IRIs this CQ exercises, reached via
    /// `gmeow:cqExpectRow → gmeow:rowCell → gmeow:cellValueIri` (sorted/deduped).
    pub exercises: Vec<String>,
    /// The slice IRI that owns the competency artifact.
    pub owner_slice: String,
}

/// One expected result row of a competency question (`gmeow:ExpectedRow`): the
/// set of per-variable cell bindings, sorted by `(var, value_iri, value_literal)`
/// for deterministic rendering (blank-node cell order is not itself meaningful).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct DocExpectedRow {
    /// The row's cell bindings, one per projected SPARQL variable.
    pub cells: Vec<DocExpectedCell>,
}

/// One expected cell binding (`gmeow:ExpectedCell`) within an expected row.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct DocExpectedCell {
    /// `gmeow:cellVar` — the SPARQL projection variable this cell binds.
    pub var: Option<String>,
    /// `gmeow:cellValueIri` — the expected IRI binding, when the variable binds
    /// a resource.
    pub value_iri: Option<String>,
    /// `gmeow:cellValueLiteral` — the expected literal lexical form, when the
    /// variable binds a literal.
    pub value_literal: Option<String>,
}

/// A first-class rendering of one of the project's `lang:Grammar` individuals:
/// the GMN / GTS / Turtle surface-syntax productions authored in full, plain
/// W3C EBNF text under `slices/grounding/lang/grammars/*.ebnf` (one file per
/// notation), carried verbatim — never a second parser, just the notation
/// exhibit the grammar object itself carries the normative claims for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocGrammar {
    /// The filename stem (e.g. `"gmn"`, `"gts"`, `"turtle"`), used as the page
    /// slug.
    pub slug: String,
    /// A human title. Derived from the file's leading `#`-commented header
    /// description (its first sentence, once the wrapped comment lines are
    /// joined) rather than a mechanically humanized filename: the header prose
    /// (e.g. "The GMN-1 (GMEOW Model Notation) surface grammar in W3C EBNF, one
    /// production per line") is materially more informative than a naive
    /// `Gmn`/`Gts`/`Turtle` filename split would be, and joining a handful of
    /// `#` lines is no harder to implement correctly than that split — see
    /// [`extract_grammar`].
    pub title: String,
    /// The full W3C EBNF source, carried in full.
    pub source: String,
    /// The `SPDX-License-Identifier` header value (e.g. `"AGPL-3.0-only"` for
    /// the authored GMN/GTS grammars, `"W3C-20150513"` for the Turtle
    /// transcription).
    pub license: String,
}

/// One authored, worked projection-loss-ledger row: a `gmeow:InformationObject`
/// individual (in ANY slice's `examples/*.ttl`) that carries BOTH
/// `logic:preservationKind` and `logic:complexityClass` — the pedagogical,
/// concrete-artifact twin of the compiler-emitted static whole-program ledger
/// (`gmeow_logic_compile::projections::projection_ledger_rows`, rendered by
/// `render.rs::md_logic_loss_ledger`'s existing table). Discovered generically:
/// ANY example subject authoring both predicates becomes a row, not just the
/// individuals in `slices/grounding/logic/examples/projection-loss-ledger.ttl`
/// (today's only author).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocLossTarget {
    /// The subject IRI's local name (e.g. `"elProjectionReport"`) — a stable,
    /// code-like identifier mirroring the compiler ledger's `target` column
    /// (which shows a target code, not prose). The human-readable
    /// [`label`](Self::label) carries the prose description separately, so
    /// this field stays a terse, sortable/citable id rather than duplicating
    /// the label.
    pub target: String,
    /// `rdfs:label`, when the subject carries one (every authored row does
    /// today, but this is a documentation READ, not a re-validation of the
    /// authoring convention — so it stays `Option`).
    pub label: Option<String>,
    /// The local name of the `logic:preservationKind` object IRI (e.g.
    /// `"SoundUnderApproximation"`, `"ValidationOnly"`).
    pub preservation_kind: String,
    /// `logic:complexityClass`'s literal value.
    pub complexity_class: String,
    /// The slice IRI that owns the example artifact this row was parsed from.
    pub slice: String,
}

/// One SI base-dimension exponent (`math:DimensionExponent`) within a
/// `math:DerivedDimension`'s `math:baseDimensionExponent` set — one ℚ
/// coordinate of the ℚ⁷ dimension vector (mass, length, time, electric
/// current, temperature, amount of substance, luminous intensity), though only
/// the base dimensions actually exercised by a given derived dimension are
/// authored (a dimension with a zero exponent on an axis carries no
/// [`DocDimExponent`] for it — sparse by construction, matching the source
/// data).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DocDimExponent {
    /// The local name of the SI base-dimension IRI this exponent is over
    /// (e.g. `"massDimension"`, `"lengthDimension"`, `"timeDimension"`).
    pub base_dimension: String,
    /// `math:exponentNumerator` (may be negative, e.g. `-2`).
    pub numerator: i64,
    /// `math:exponentDenominator`.
    pub denominator: i64,
}

/// One worked math instance: a subject (in any slice's `examples/*.ttl`)
/// carrying `math:hasDimension`, with its dimension resolved down to the ℚ⁷
/// SI base-dimension exponent vector when the dimension object is a
/// `math:DerivedDimension` — or an honest empty exponent vector when it is a
/// dimensionless object (e.g. `math:dimensionless`) that carries no
/// `math:baseDimensionExponent` breakdown. Discovered generically, in the SAME
/// `examples/*.ttl` scan [`extract_loss_targets`] uses — any example subject
/// declaring `math:hasDimension` becomes a row, not just the individuals in
/// `slices/grounding/math/examples/measure-and-dimension.ttl` (today's only
/// author).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocWorkedInstance {
    /// The slice IRI that owns the example artifact this instance was parsed
    /// from.
    pub slice: String,
    /// The logical path within the slice directory (e.g.
    /// `examples/measure-and-dimension.ttl`).
    pub logical_path: String,
    /// The local name of the dimensioned subject (e.g. `"expectedEnergy"`).
    pub subject: String,
    /// The local names of the subject's `rdf:type` values (sorted, deduped) —
    /// e.g. `["Integral"]` for `ex:expectedEnergy` (a `math:Integral`).
    pub types: Vec<String>,
    /// `rdfs:label`, when the subject carries one.
    pub label: Option<String>,
    /// `rdfs:label` of the resolved dimension object (the `math:hasDimension`
    /// target), when it carries one. `None` for `math:dimensionless` in the
    /// current data (it authors no local label in this file) — an honest
    /// absence, not a fabricated string.
    pub dimension_label: Option<String>,
    /// The ℚ⁷ SI base-dimension exponent vector, sorted by
    /// [`base_dimension`](DocDimExponent::base_dimension). Empty when the
    /// dimension object carries no `math:baseDimensionExponent` (the
    /// dimensionless case) — an honest zero-exponent case, not a hard fail.
    pub dimension_exponents: Vec<DocDimExponent>,
    /// `gmeow:unit` — the QUDT unit object IRI, when the subject is a
    /// `math:Quantity` realizing one (e.g.
    /// `<http://qudt.org/vocab/unit/J>`).
    pub unit: Option<String>,
    /// `math:quantityValue`'s literal lexical form, when the subject is a
    /// `math:Quantity` carrying one (e.g. `"8.187e-14"`).
    pub quantity_value: Option<String>,
    /// A small, deterministic, copy-paste-runnable Turtle block reconstructed
    /// from the extracted fields (never a byte-slice of the source file — see
    /// [`render_worked_instance_turtle`] for why). Carries the subject's own
    /// triples and, when the dimension resolves to a `math:DerivedDimension`,
    /// that dimension's `math:baseDimensionExponent` breakdown as anonymous
    /// blank-node objects.
    pub turtle: String,
}

/// A documentation concern (`gmeow:DocumentationConcern`) and the terms that
/// declare it via `gmeow:docsConcern`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocExternalTerm {
    /// The external IRI.
    pub iri: String,
    /// The namespace prefix (the IRI up to and including the last `/` or `#`).
    pub namespace: String,
    /// GMEOW CURIEs that reference this external IRI (sorted, deduped).
    pub referenced_by: Vec<String>,
    /// The predicates the reference travels over (`matchObject`, `subClassOf`,
    /// `domain`, `range`), sorted/deduped.
    pub via_predicate: Vec<String>,
}

/// One directed leg of a `gmeow:Seam` (a `gmeow:SeamDirection` blank node): the
/// referencing (from) and referenced (to) grounding-slice IRIs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DocSeamDirection {
    /// `gmeow:seamFromSlice` — the referencing (source) grounding slice IRI.
    pub from: String,
    /// `gmeow:seamToSlice` — the referenced (target) grounding slice IRI.
    pub to: String,
}

/// A sanctioned cross-grounding reference channel (`gmeow:Seam`) — one edge of
/// the grounding-reference information-flow policy (Principle 19). Authored as
/// canonical governance data in a grounding slice's `manifest.ttl` (today,
/// `logic:`'s — see [`extract_seams`]), never hand-duplicated as a markdown
/// table: [`crate::render::Page::SeamRegistry`] projects this set to the
/// generated seam-registry page, and `gmeow-validate`'s authoring-integrity
/// gate asserts that projection never drifts from this data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocSeam {
    /// The seam IRI (`a gmeow:Seam`).
    pub iri: String,
    /// `rdfs:label`.
    pub label: Option<String>,
    /// `skos:definition` (falling back to `rdfs:comment`).
    pub definition: Option<String>,
    /// `gmeow:seamDirection` legs, sorted by `(from, to)` and deduped.
    pub directions: Vec<DocSeamDirection>,
    /// `gmeow:seamCarryingTerm` IRIs, sorted/deduped.
    pub carrying_terms: Vec<String>,
    /// `gmeow:seamOwningDoc` filenames, sorted/deduped.
    pub owning_docs: Vec<String>,
}

/// A task-oriented adoption recipe (`gmeow:Recipe`), parsed from the guides
/// slice. A curated guide that sequences canonical examples + terms for one
/// recurring modelling task (no domain axiom — pure pedagogy).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// One `gmeow:PipelineStage` individual of the dogfooded build DAG
/// (`slices/core/pipeline/module.ttl`): a typed unit of build work bound to a
/// Rust `Stage` implementation through [`stage_impl`](DocStage::stage_impl).
/// Sorted collections keep the serialized model byte-reproducible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DocStage {
    /// The full stage IRI (e.g. `.../stage-gts-sink`). Also a documented term,
    /// so the enriched stage section on its term page links back to the DAG.
    pub iri: String,
    /// `rdfs:label`.
    pub label: Option<String>,
    /// `skos:definition`.
    pub definition: Option<String>,
    /// `gmeow:stageImpl` — the registry key binding the stage to its Rust `Stage`
    /// implementation (`crates/pipeline/src/stages/<impl>.rs`).
    pub stage_impl: Option<String>,
    /// `gmeow:hasCapability` value CURIEs (e.g. `gmeow:sinkCapability`,
    /// `gmeow:sourceOrigin`), sorted/deduped. Empty for a plain transform leaf.
    pub capabilities: Vec<String>,
    /// `gmeow:requiresResource` value CURIEs (e.g. `gmeow:engineResource`),
    /// sorted/deduped. Empty when the stage holds no shared resource.
    pub resources: Vec<String>,
    /// `gmeow:graphBoxRole` — the lowest-sorted four-boxes role CURIE, if any.
    pub box_role: Option<String>,
    /// `gmeow:dataflowConsumes` — the producer stage IRIs this stage reads,
    /// sorted/deduped.
    pub consumes: Vec<String>,
    /// `gmeow:attachesGraph` — the named-graph IRIs this stage attaches to the carrier
    /// as its delta (its declared, run-verified contribution), sorted/deduped. The
    /// self-explaining surface: `gmeow docs-on <stage>` shows what the stage produced.
    pub attaches_graphs: Vec<String>,
    /// `gmeow:attachesBlobRep` — the blob-representation lane labels this stage attaches,
    /// sorted/deduped.
    pub attaches_blob_reps: Vec<String>,
}

/// One dataflow edge of the build DAG: the union of a bare
/// `gmeow:dataflowConsumes` dependency (consumer reads the producer's whole
/// product) with any reified `gmeow:BuildDataFlow` refinement that names the
/// specific flowing named graphs. [`flow_entities`](DocFlowEdge::flow_entities)
/// is populated ONLY from a reified edge — a missing label is honest
/// computed-absence (no reified edge authored), never a failure or placeholder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DocFlowEdge {
    /// The producer stage IRI (`gmeow:buildFlowFrom` / the `dataflowConsumes`
    /// object).
    pub from: String,
    /// The consumer stage IRI (`gmeow:buildFlowTo` / the `dataflowConsumes`
    /// subject).
    pub to: String,
    /// The named-graph IRIs that flow on this edge (`gmeow:flowEntity`),
    /// sorted/deduped. Empty unless a reified `gmeow:BuildDataFlow` authors them.
    pub flow_entities: Vec<String>,
}

/// The dogfooded build pipeline as a first-class documentation surface: the
/// `gmeow:PipelineStage` node set, the dataflow edge set (bare consumes unioned
/// with reified `gmeow:BuildDataFlow` flow-entity refinements), and the
/// `gmeow:Pipeline` plan's goal + success mode. A source-lane projection of
/// `slices/core/pipeline/module.ttl` (read as authored input, PIPELINE_SPINE
/// §3.1 — never a `generated/` disk read).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DocPipeline {
    /// Every `gmeow:PipelineStage`, sorted by IRI.
    pub stages: Vec<DocStage>,
    /// Every dataflow edge, sorted by `(from, to)`. Flow-entity labels are
    /// present only where a reified `gmeow:BuildDataFlow` authors them.
    pub edges: Vec<DocFlowEdge>,
    /// `logic:planGoal` of the `gmeow:Pipeline` (a CURIE), if authored.
    pub goal: Option<String>,
    /// `logic:planSuccessMode` of the `gmeow:Pipeline` (a CURIE), if authored.
    pub success_mode: Option<String>,
}

/// The complete typed documentation model — one source of truth for every
/// renderer. All collections are sorted by a stable key.
///
/// Holds [`DocLinkage`] (with an `f64` confidence), so this type is `PartialEq`
/// but not `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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
    /// All conformance Do/Don't fixtures — well-formed instances and
    /// deliberately malformed counter-examples, joined to their owning slice's
    /// `tests/example-conformance.ttl` binding when one exists (sorted by
    /// slice then logical path).
    pub fixtures: Vec<DocFixture>,
    /// All SHACL node shapes reverse-mapped to the terms they constrain
    /// (sorted by target term then shape IRI).
    pub shapes: Vec<DocShape>,
    /// All competency questions reverse-mapped to the terms they exercise
    /// (sorted by IRI).
    pub competencies: Vec<DocCompetency>,
    /// All notation grammars — first-class W3C EBNF renderings of the
    /// project's own serialization surface syntaxes (GMN, GTS, Turtle),
    /// discovered from every slice's `grammars/*.ebnf` artifacts (sorted by
    /// slug).
    pub grammars: Vec<DocGrammar>,
    /// All authored, worked projection-loss-ledger rows — every example
    /// subject (in any slice) carrying both `logic:preservationKind` and
    /// `logic:complexityClass` (sorted by slice then target). Distinct from
    /// the compiler-emitted static ledger already rendered from
    /// `gmeow_logic_compile::projections::projection_ledger_rows()`; this is
    /// the pedagogical, concrete-artifact companion table.
    pub loss_targets: Vec<DocLossTarget>,
    /// All worked math instances — every example subject (in any slice)
    /// carrying `math:hasDimension`, with its ℚ⁷ SI base-dimension exponent
    /// vector when resolvable (sorted by slice then subject). See
    /// [`DocWorkedInstance`].
    pub worked_instances: Vec<DocWorkedInstance>,
    /// All documentation concerns (sorted by IRI).
    pub concerns: Vec<DocConcern>,
    /// All external (non-GMEOW) terms referenced (sorted by IRI).
    pub external_terms: Vec<DocExternalTerm>,
    /// All sanctioned cross-grounding seams (`gmeow:Seam` individuals), read
    /// from every slice typed `gmeow:GroundingSlice` (sorted by IRI). See
    /// [`DocSeam`].
    pub seams: Vec<DocSeam>,
    /// All adoption recipes parsed from the guides slice (sorted by slug).
    pub recipes: Vec<DocRecipe>,
    /// All curated learning paths parsed from the guides slice (sorted by slug).
    pub learning_paths: Vec<DocLearningPath>,
    /// The constraint catalog — every `gmeow:ValidationRule` individual read from
    /// `generated/catalog/constraint-catalog.nq` in `discover()` (sorted by code).
    /// Empty in `from_catalog` and when the generated artifact is absent (a bare
    /// unit-test model). Drives the "What GMEOW enforces" page, whose per-rule
    /// anchor is the same slug the validator mints for a finding's `helpUri`.
    pub constraint_rules: Vec<ConstraintRule>,
    /// The advice catalog — every `gmeow:AdviceEntry` individual read from the same
    /// `constraint-catalog.nq` in `discover()` (sorted by term). The recommendation
    /// tier peer of `constraint_rules`: drives the distinct "Advice" section of the
    /// "What GMEOW enforces" page. Empty when no realized advice carrier exists.
    pub advice_entries: Vec<AdviceEntry>,
    /// The curated "four boxes" doctrine prose, read at build time from
    /// `<root>/docs/four-boxes.md` if present (`None` when absent).
    pub four_boxes: Option<String>,
    /// The ontology's concept DOI (`dcterms:identifier` on the `gmeow:Work`
    /// subject of `<root>/metadata/gmeow-self.ttl`), read in `discover()`. Drives
    /// the per-term citation block's "cite the ontology" line. `None`
    /// when the metadata file is absent.
    pub concept_doi: Option<String>,
    /// The dogfooded build pipeline DAG (`slices/core/pipeline/module.ttl`): the
    /// `gmeow:PipelineStage` node set, the dataflow edges, and the plan goal +
    /// success mode. A REGULAR serialized field (source lane — discovered from an
    /// authored module, so it belongs in the model JSON). `None` only for a bare
    /// hand-built unit-test model whose catalog carries no pipeline module; the
    /// full `discover`/`from_catalog` path always populates it. Drives
    /// [`Page::PipelineDag`](crate::render), the per-stage enriched term section,
    /// and the per-page provenance footer.
    pub pipeline: Option<DocPipeline>,
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
    /// The target render language for the body renderers: the English carrier
    /// (`""` / `"english"`) or a BCP-47 code (`"fr"`, `"zh"`). Set by
    /// `localize_model` on the cloned per-language model; the English carrier keeps
    /// `""` (which resolves to the English UI-chrome defaults). Skipped from
    /// serialization so the model JSON golden is unchanged (deserialize defaults to
    /// `""`).
    #[serde(skip)]
    pub lang: String,
    /// The native reasoner's per-ontology consistency verdict, attached AFTER
    /// source discovery by the production build (the carrier render path and the
    /// docs-graph stage both consume `stage-reason`). `None` in source-only
    /// contexts (unit tests, a bare `discover`): the per-term reasoning badge and
    /// the reasoning-status RDF projection render ONLY when a verdict is present,
    /// so an unevaluated model never fabricates a "satisfiable" claim. The
    /// production path attaches it (or hard-fails), never silently skips it.
    /// `#[serde(skip)]` so the source-model JSON golden is unaffected.
    #[serde(skip)]
    pub reasoning: Option<ReasoningVerdict>,
    /// The diagnostics→term join, attached AFTER source discovery by the production
    /// build from the already-materialized `stage-validate` + `stage-compile-logic`
    /// products (never a re-run of SHACL or the logic compiler — reason/validate-once).
    /// `None` in source-only contexts (unit tests, a bare `discover`): the per-term
    /// "Diagnostics you might hit" section and any `gmeow:doc*` diagnostics projection
    /// render ONLY when a digest is attached, so an unevaluated model never fabricates a
    /// "no diagnostics" claim. The production path attaches it (or hard-fails on a
    /// missing declared upstream product), never silently skips it. `#[serde(skip)]` so
    /// the source-model JSON golden is unaffected.
    #[serde(skip)]
    pub diagnostics: Option<DiagnosticsDigest>,
    /// The dynamic per-term projection-loss join, attached AFTER source discovery
    /// by the production build from the already-materialized `stage-mappings`
    /// product's live `GRAPH_PROJECTION_LEDGER` graph (never a re-run of the logic
    /// compiler — reason/compile-once). `None` in source-only contexts (unit
    /// tests, a bare `discover`): the per-term "how this term degrades under
    /// projection" section renders ONLY when a digest is attached, so an
    /// unevaluated model never fabricates a "carried exactly" claim. The
    /// production path attaches it (or hard-fails on a missing `stage-mappings`
    /// upstream product), never silently skips it. `#[serde(skip)]` so the
    /// source-model JSON golden is unaffected.
    #[serde(skip)]
    pub term_loss: Option<TermLossDigest>,
    /// The per-term JSON Schema / OpenAPI fragment join, attached AFTER source
    /// discovery by the production build from the already-materialized
    /// `stage-export-json-schema` product (the same `gmeow.schema.json` /
    /// `gmeow.openapi.json` bytes the carrier folds into the packed
    /// `schemas-archive`, read in-memory — never a `generated/` disk read). Each
    /// entry is the pretty-printed JSON Schema `$defs` (respectively OpenAPI
    /// `components/schemas`) fragment for a documented class whose emitter def key
    /// resolves it. `None` in source-only contexts (unit tests, a bare
    /// `discover`): the per-term "use this term without RDF" JSON-Schema panel and
    /// the OpenAPI tab render ONLY when the digest is attached, so an unevaluated
    /// model never fabricates a schema fragment. `#[serde(skip)]` so the
    /// source-model JSON golden is unaffected.
    #[serde(skip)]
    pub schema_fragments: Option<SchemaFragmentDigest>,
}

/// The native reasoner's consistency verdict, attached to a [`DocsModel`] by the
/// production build so the docs can surface a per-term reasoning-status badge.
///
/// Carries the global consistency flag and the set of class IRIs the native DL
/// reasoner proved unsatisfiable (each entailed `rdfs:subClassOf owl:Nothing` in
/// the inferred closure). A term's three-state status is derived honestly:
/// satisfiability is a CLASS notion, so a documented class is *evaluated* —
/// satisfiable unless its IRI is in [`unsatisfiable`](Self::unsatisfiable) — while
/// a property, individual, or datatype is *not-evaluated* (the reasoner decides no
/// satisfiability for it). The not-evaluated state never collapses into
/// satisfiable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReasoningVerdict {
    /// The global native-reasoner consistency flag (`ReasoningResult::is_consistent`).
    pub is_consistent: bool,
    /// The class IRIs proven unsatisfiable (entailed `rdfs:subClassOf owl:Nothing`).
    /// Empty for a healthy ontology; a non-empty set lights the affected class
    /// pages red.
    pub unsatisfiable: std::collections::BTreeSet<String>,
}

/// One `gmeow:ValidationRule` individual from the constraint catalog: the
/// human-readable record of a constraint the validator enforces. Read from
/// `generated/catalog/constraint-catalog.nq` (a committed fixed-point projection
/// of `gmeow.gts`) and rendered on the "What GMEOW enforces" page, where each
/// rule is anchored by [`slug`](ConstraintRule::slug) — the identical slug the
/// validator mints for a finding's `helpUri`, so a finding's help link resolves
/// straight to its rule entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintRule {
    /// The stable rule code (`gmeow:ruleCode`, e.g. `box-roles.invalid`).
    pub code: String,
    /// The in-page anchor slug: `gmeow_validate::rule_catalog::slugify(code)`,
    /// matching the `#fragment` of the rule's `helpUri`.
    pub slug: String,
    /// The `logic:FindingCategory` IRI this rule reports under (`gmeow:ruleCategory`).
    pub category: String,
    /// The severity token (`gmeow:ruleSeverity`): `binding` or `advisory`.
    pub severity: String,
    /// The absolute `helpUri` (`gmeow:ruleHelpUri`) — the deep link a finding
    /// carries; its fragment is `#{slug}`.
    pub help_uri: String,
    /// The human label (`rdfs:label`), if present.
    pub label: Option<String>,
    /// The prose definition (`skos:definition`), if present.
    pub definition: Option<String>,
    /// The GMEOW term IRIs this rule applies to (`gmeow:appliesToTerm`), sorted.
    pub applies_to_terms: Vec<String>,
    /// The `logic:` axiom IRI this rule formalizes (`logic:formalizes`), if present.
    pub formalizes: Option<String>,
}

/// One advice-catalog entry (`gmeow:AdviceEntry`): the standing RECOMMENDATION
/// harvested for one governed term — the advisory peer of [`ConstraintRule`].
/// Rendered in the distinct "Advice" section beneath the `advice.` family rule
/// (`documented_by_rule`), the single `#advice-` anchor every advisory finding code
/// resolves to. Each prose leg is the term's OWN verbatim prose, projected from the
/// realized carriers — never a fabricated recommendation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdviceEntry {
    /// The governed term IRI (`gmeow:appliesToTerm` / `logic:formalizes`).
    pub term: String,
    /// The readable in-page sub-anchor slug `advice-{slugify(local-name(term))}`
    /// (navigation only; the guaranteed static resolution target is `#advice-`).
    pub slug: String,
    /// The term's human label (`rdfs:label`), if present.
    pub label: Option<String>,
    /// The term's prose definition (`skos:definition`), if present.
    pub definition: Option<String>,
    /// The prohibition prose (`gmeow:adviceAvoidWhen`), sorted; may be empty.
    pub avoid_when: Vec<String>,
    /// The conditional-permission prose (`gmeow:adviceUseWhen`), sorted; may be empty.
    pub use_when: Vec<String>,
    /// The positive-directive prose (`gmeow:adviceHowToUse`), sorted; may be empty.
    pub how_to_use: Vec<String>,
    /// The advice family `gmeow:ValidationRule` IRI this entry hangs beneath
    /// (`gmeow:documentedByRule`), if present.
    pub documented_by_rule: Option<String>,
}

/// One diagnostic (`gmeow_errors::DiagNode`) projected for docs rendering: the
/// display-ready severity/category, the first observation's human message (a
/// `DiagNode` carries no dedicated `message` field — the message lives on its
/// first [`Observation`](gmeow_errors::Observation)), the primary attribution's
/// slice IRI when one is recorded, and a `help_uri` ONLY when the finding's
/// `code` genuinely resolves against the constraint catalog
/// (`DocsModel::constraint_rules`) — never a fabricated link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocDiagFinding {
    /// The stable diagnostic code (`DiagNode::code`, e.g. `shacl.MinCountConstraintComponent`
    /// or `logic-compile.UNKNOWN_PROFILE`).
    pub code: String,
    /// The display spelling of `DiagNode::grade.severity` (`Severity::as_str()`).
    pub severity: String,
    /// The display spelling of `DiagNode::grade.category` (`FindingCategory::as_str()`).
    pub category: String,
    /// The human message: the first observation's `message`, when the node carries
    /// one, else the code itself (a `DiagNode` always carries at least one
    /// observation in practice, but the fallback keeps this a total function).
    pub message: String,
    /// The primary (first) attribution's slice IRI, when the node carries one.
    pub slice_iri: Option<String>,
    /// The constraint-catalog rule's absolute help URI, resolved by exact `code`
    /// match against `DocsModel::constraint_rules`. `None` when no rule shares this
    /// exact code (an honest absence — never a fabricated deep link).
    pub help_uri: Option<String>,
}

/// The diagnostics→term join folded from the `stage-validate` + `stage-compile-logic`
/// products' `diagnostics:nodes` blobs — the carrier-lane digest attached to a
/// [`DocsModel`] AFTER source discovery (never a re-run of SHACL or the logic
/// compiler). Keys on the diagnostic's `source_ctx.location.logical` string (the
/// SHACL focus-node bare IRI / the logic-compile diagnostic `subject`), matched by
/// EXACT string equality against a known [`DocTerm::iri`] — a diagnostic whose
/// location doesn't name a known term simply has no `by_term` entry (honest, not a
/// bug). `by_slice` is keyed on every recorded [`DiagnosticAttribution::slice_iri`](
/// gmeow_errors::DiagnosticAttribution) instead — a coarser, always-available join.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DiagnosticsDigest {
    /// Findings keyed by the exact term IRI their location names (sorted keys; each
    /// finding list is in stable, deterministic node order).
    pub by_term: BTreeMap<String, Vec<DocDiagFinding>>,
    /// Findings keyed by every attributed slice IRI (sorted keys; each finding list
    /// is in stable, deterministic node order).
    pub by_slice: BTreeMap<String, Vec<DocDiagFinding>>,
    /// The total number of diagnostic nodes folded from both upstream products
    /// (before any term/slice join — the raw union count).
    pub total: usize,
}

/// One row of the per-term dynamic projection-loss join: a single
/// `logic:ProjectionTarget` from the live `GRAPH_PROJECTION_LEDGER` graph whose
/// `rdfs:label` carries the `property-path:<shape-iri>` prefix and resolved to a
/// documented term (see [`TermLossDigest`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TermLossRow {
    /// The FULL `rdfs:label` value carried by the `logic:ProjectionTarget`
    /// (e.g. `property-path:https://.../nearbyOrgs`), kept whole (not stripped
    /// of its prefix) so the row is traceable back to its exact ledger entry.
    pub target: String,
    /// The local name of the `logic:preservationKind` object IRI (e.g.
    /// `SoundUnderApproximation`, `ExactPreservation`).
    pub preservation_kind: String,
    /// The `logic:complexityClass` plain-string literal.
    pub complexity_class: String,
    /// The `gmeow:lossyDrop` plain-string literals, sorted and deduped.
    pub lossy_drops: Vec<String>,
}

/// The dynamic per-term projection-loss join, folded from the LIVE
/// `GRAPH_PROJECTION_LEDGER` named graph — `stage-mappings`'s committed
/// projection report, attached AFTER source discovery (never a re-run of the
/// logic compiler; reason/compile-once). DISTINCT from the STATIC whole-program
/// rows already rendered on `Page::LogicLossLedger`
/// (`gmeow_logic_compile::projections::projection_ledger_rows`, e.g. `owl-dl`,
/// `datalog`) and from the authored worked examples
/// ([`DocsModel::loss_targets`], A4): this digest carries ONLY the per-shape
/// `property-path:<shape-iri>` rows the ledger emits when a concrete
/// `logic:PathShape` is compiled, joined to a documented term via
/// [`DocShape::shape_iri`] → [`DocShape::target_term`] (falling back to an
/// exact match of the bare shape IRI against a [`DocTerm::iri`] when no
/// `DocShape` claims it). Whole-program rows never enter `by_term` — they
/// apply project-wide, not per-term, and stay on the static ledger page. A
/// property-path row that resolves to no documented term is honestly absent
/// from `by_term` (never forced) — see [`total_property_path_rows`](
/// Self::total_property_path_rows) for the raw pre-join count, so a real-repo
/// non-vacuity assertion can distinguish "the ledger genuinely has no
/// property-path content" from "content exists but nothing joined a term".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TermLossDigest {
    /// Rows keyed by the exact documented term IRI they joined to (sorted keys;
    /// each row list sorted by `target`).
    pub by_term: BTreeMap<String, Vec<TermLossRow>>,
    /// The total count of `property-path:`-prefixed `logic:ProjectionTarget`
    /// rows found in the live ledger, whether or not they joined to a
    /// documented term.
    pub total_property_path_rows: usize,
}

/// The per-term JSON Schema / OpenAPI fragment join for the term-page
/// "use this term without RDF" panel + OpenAPI tab.
///
/// Both maps are keyed by the exact documented term IRI; each value is the
/// pretty-printed, deterministic JSON text of that class's `$defs` (respectively
/// `components/schemas`) fragment, extracted from the generated
/// `gmeow.schema.json` / `gmeow.openapi.json`. Only documented classes whose
/// emitter def key (`purrdf::shapes::json_schema::Namespaces::def_key`: a bare
/// local name for a primary-namespace class, a CURIE otherwise) resolves an
/// entry appear — a term with no schema fragment is honestly absent, never a
/// fabricated stub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SchemaFragmentDigest {
    /// The JSON Schema `$defs` fragment text keyed by documented term IRI.
    pub schema_by_term: BTreeMap<String, String>,
    /// The OpenAPI `components/schemas` fragment text keyed by documented term IRI.
    pub openapi_by_term: BTreeMap<String, String>,
}

impl DocsModel {
    /// Attach the native reasoner's consistency verdict to this model (the
    /// production build's post-discovery step). Idempotent: overwrites any prior
    /// verdict.
    pub fn attach_reasoning(&mut self, verdict: ReasoningVerdict) {
        self.reasoning = Some(verdict);
    }

    /// Attach the diagnostics→term join digest to this model (the production
    /// build's post-discovery step, mirroring [`attach_reasoning`](Self::attach_reasoning)).
    /// Idempotent: overwrites any prior digest.
    pub fn attach_diagnostics(&mut self, digest: DiagnosticsDigest) {
        self.diagnostics = Some(digest);
    }

    /// Attach the dynamic per-term projection-loss digest to this model (the
    /// production build's post-discovery step, mirroring
    /// [`attach_diagnostics`](Self::attach_diagnostics)). Idempotent: overwrites
    /// any prior digest.
    pub fn attach_term_loss(&mut self, digest: TermLossDigest) {
        self.term_loss = Some(digest);
    }

    /// Attach the per-term JSON Schema / OpenAPI fragment digest to this model
    /// (the production build's post-discovery step, mirroring
    /// [`attach_term_loss`](Self::attach_term_loss)). Idempotent: overwrites any
    /// prior digest.
    pub fn attach_schema_fragments(&mut self, digest: SchemaFragmentDigest) {
        self.schema_fragments = Some(digest);
    }

    /// Resolve a UI-chrome string for `key` in this model's target [`lang`], using
    /// the per-language override catalog when present and falling back to the
    /// `'static` English default. Empty `lang` and `"english"` both resolve to the
    /// English default (see [`i18n::ui_string`]).
    ///
    /// [`lang`]: DocsModel::lang
    pub(crate) fn ui(&self, key: &str) -> &str {
        i18n::ui_string(key, &self.lang, &self.ui_catalog)
    }
}

impl DocsModel {
    /// The model schema version. Bump when the serialized shape changes.
    ///
    /// v8: adds [`fixtures`](DocsModel::fixtures) — conformance Do/Don't
    /// fixtures joined to their slice's `tests/example-conformance.ttl`
    /// binding.
    ///
    /// v9: [`DocCompetency`] grows `query_text` (resolved `.rq` body / inline
    /// `cqQuery`), `exact_rows`, `expected_row_count`, and structured
    /// `expected_rows` (`DocExpectedRow`/`DocExpectedCell`) — the full
    /// copy-paste-runnable competency-question surface for `Page::CompetencyIndex`.
    ///
    /// v10: adds [`grammars`](DocsModel::grammars) — first-class W3C EBNF
    /// notation exhibits discovered from every slice's `grammars/*.ebnf`.
    ///
    /// v11: adds [`loss_targets`](DocsModel::loss_targets) — authored, worked
    /// projection-loss-ledger rows discovered generically from every example
    /// subject (in any slice) carrying both `logic:preservationKind` and
    /// `logic:complexityClass`.
    ///
    /// v12: adds [`worked_instances`](DocsModel::worked_instances) — worked
    /// math ℚ⁷ SI-dimension instances discovered generically from every
    /// example subject (in any slice) carrying `math:hasDimension`, resolved
    /// down to a labeled base-dimension exponent table plus a copy-paste
    /// Turtle block.
    ///
    /// v14: lifts every `gmeow:PipelineStage` individual into a documented term,
    /// so each stage's term page renders the enriched build-pipeline section
    /// (`stageImpl` link, consumes / consumed-by tables, flowing graphs).
    ///
    /// v15: adds [`DocTerm::slug`] — the INJECTIVE `documentation/term/{slug}`
    /// doc-entry slug, resolved once from the whole term set so no two distinct
    /// term IRIs collide onto one doc-entry subject (which previously conflated
    /// their coverage incidence).
    ///
    /// v16: each [`DocStage`] grows `attaches_graphs` / `attaches_blob_reps`
    /// (`gmeow:attachesGraph` / `gmeow:attachesBlobRep`) — the stage's declared,
    /// run-verified carrier contribution, so a stage term page self-explains what it
    /// produced.
    ///
    /// v17: two additions land together. [`seams`](DocsModel::seams) carries the
    /// sanctioned cross-grounding `gmeow:Seam` registry, read generically from every
    /// slice manifest typed `gmeow:GroundingSlice` and projected to the generated
    /// seam-registry page (`Page::SeamRegistry`). Each [`DocSlice`] also grows
    /// `documents` — every `text/markdown` source in the slice as a first-class,
    /// strictly-decoded [`DocMarkdownDocument`] (selected by media type, never by
    /// `ArtifactRole`, so `design/*.md` is a first-class document). The
    /// [`crate::source_map::SourceToPageMap`] is the single link-rewrite authority
    /// over that set (a pure function of the model).
    pub const VERSION: &'static str = "17";

    /// An empty model with every collection cleared — for the [`crate::source_map`]
    /// unit tests, which populate only `slices` before exercising the page map.
    #[cfg(test)]
    pub(crate) fn empty_for_test() -> Self {
        Self {
            title: "GMEOW Ontology Documentation".to_string(),
            version: Self::VERSION.to_string(),
            slices: Vec::new(),
            terms: Vec::new(),
            dependency_edges: Vec::new(),
            mapping_sets: Vec::new(),
            linkages: Vec::new(),
            examples: Vec::new(),
            fixtures: Vec::new(),
            shapes: Vec::new(),
            seams: Vec::new(),
            competencies: Vec::new(),
            grammars: Vec::new(),
            loss_targets: Vec::new(),
            worked_instances: Vec::new(),
            concerns: Vec::new(),
            external_terms: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            advice_entries: Vec::new(),
            constraint_rules: Vec::new(),
            four_boxes: None,
            concept_doi: None,
            pipeline: None,
            available_languages: vec!["english".to_string()],
            translations: Translations::default(),
            ui_catalog: UiCatalog::default(),
            reasoning: None,
            diagnostics: None,
            term_loss: None,
            schema_fragments: None,
            lang: String::new(),
        }
    }

    /// Build the documentation model from a discovered catalog and a computed
    /// ownership report. `central_mapping_sets` carries the cross-slice SSSOM
    /// `gmeow:MappingSet` publication headers (one per external vocabulary — they
    /// aggregate cells authored across many slices, so they belong to no single
    /// slice); they resolve each slice-authored linkage's `sssom_file` to its set
    /// IRI and are documented alongside the slice-owned sets.
    pub fn from_catalog(
        catalog: &SliceCatalog,
        ownership: &OwnershipReport,
        central_mapping_sets: &[DocMappingSet],
    ) -> Result<Self, DocsError> {
        // ── Slices ──────────────────────────────────────────────────────────
        // `from_record` hard-fails on a markdown-document defect (invalid UTF-8 or a
        // normalized-path collision) — propagated here rather than silently skipped.
        let mut slices: Vec<DocSlice> = catalog
            .records()
            .iter()
            .map(DocSlice::from_record)
            .collect::<Result<Vec<_>, _>>()?;
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

        // ── PathShape example terms (design/LOGIC-PATHS.md) ─────────────────
        // A `logic:PathShape` is a first-class reusable by-name term, but its
        // authored INSTANCES live in worked-example artifacts, not module.ttl, so
        // the module-only scan above misses them. Lift them here — BEFORE the
        // related-terms / profile-membership passes below — so an example
        // PathShape term participates in those passes exactly like a module term
        // (and its `property-path:<iri>` projection-loss row joins its page). This
        // is a small independent parse of each Example artifact; the reuse loop
        // further down re-parses for the DocExample / loss / worked-instance scans.
        for record in catalog.records() {
            let owner = &record.manifest.slice_iri;
            for artifact in &record.artifacts {
                if artifact.role != ArtifactRole::Example {
                    continue;
                }
                let Ok(store) = parse_turtle_lenient(&artifact.content) else {
                    continue;
                };
                terms.extend(extract_path_shape_terms(
                    &store,
                    owner,
                    record.manifest.tier.as_ref(),
                ));
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

        // ── Per-term profile membership ──────────────────────────────────────
        // A term belongs to a named profile P iff P's declared-member-plus-
        // sliceDependsOn closure contains the term's owner slice; every term
        // also belongs to `full` (root + every extension). This MIRRORS the
        // pipeline `profiles` stage's closure from the same manifest
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

        // ── Injective doc-entry slugs ────────────────────────────────────────
        // Resolve a globally-unique `documentation/term/{slug}` slug for every
        // term, so no two distinct term IRIs collide onto one doc-entry subject
        // (which would conflate their projected coverage incidence and earned
        // maturity). A deterministic pure function of the IRI-sorted term set;
        // see `crate::render::resolve_term_slugs`.
        {
            let slugs = crate::render::resolve_term_slugs(&terms);
            for t in &mut terms {
                if let Some(slug) = slugs.get(&t.iri) {
                    t.slug = slug.clone();
                }
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
                // Fail closed. A `mappings/*.ttl` that will not parse carries an
                // unknown number of `gmeow:MappingSet` headers and alignment cells;
                // skipping it silently subtracts every one of them from the linkage
                // index with no diagnostic, so the published equivalence count drops
                // and nothing says why. The count IS the evidence that the grounding
                // corpus survived a relocation, so it may never be quietly wrong.
                let store = parse_turtle_lenient(&artifact.content).map_err(|e| {
                    DocsError::MappingParse {
                        slice_iri: owner.clone(),
                        source_path: artifact.logical_path.clone(),
                        detail: e.to_string(),
                    }
                })?;
                let (sets, links) = extract_mappings(&store, owner);
                mapping_sets.extend(sets);
                linkages.extend(links);
            }
        }
        // Cross-slice SSSOM publication headers (per external vocabulary): the
        // linkage cells live in their owning slices, but the set-level metadata
        // (setId/license/setComment) is shared, so it is authored centrally.
        mapping_sets.extend(central_mapping_sets.iter().cloned());
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
        // Each example artifact's Turtle is parsed exactly ONCE here and reused for
        // `DocExample` extraction, the generic authored projection-loss-ledger scan
        // (`DocLossTarget`), and the generic worked-math-instance scan
        // (`DocWorkedInstance`) — no artifact is re-parsed.
        let mut examples: Vec<DocExample> = Vec::new();
        let mut loss_targets: Vec<DocLossTarget> = Vec::new();
        let mut worked_instances: Vec<DocWorkedInstance> = Vec::new();
        for record in catalog.records() {
            let owner = &record.manifest.slice_iri;
            for artifact in &record.artifacts {
                if artifact.role != ArtifactRole::Example {
                    continue;
                }
                let parsed = parse_turtle_lenient(&artifact.content).ok();
                examples.push(extract_example_from(artifact, owner, parsed.as_ref()));
                if let Some(store) = &parsed {
                    loss_targets.extend(extract_loss_targets(store, owner));
                    worked_instances.extend(extract_worked_instances(store, artifact, owner));
                }
            }
        }
        examples.sort_by(|a, b| {
            a.slice
                .cmp(&b.slice)
                .then_with(|| a.logical_path.cmp(&b.logical_path))
        });
        loss_targets.sort_by(|a, b| a.slice.cmp(&b.slice).then_with(|| a.target.cmp(&b.target)));
        worked_instances.sort_by(|a, b| {
            a.slice
                .cmp(&b.slice)
                .then_with(|| a.subject.cmp(&b.subject))
        });

        // ── Conformance fixtures (Do/Don't pairs, joined to example-conformance.ttl) ─
        let mut fixtures: Vec<DocFixture> = Vec::new();
        for record in catalog.records() {
            let owner = &record.manifest.slice_iri;
            // This slice's `tests/example-conformance.ttl` bindings, keyed by the
            // slice-relative fixture path each `gmeow:exampleFile` pins. Empty when
            // the slice authors no bindings — fixtures then join to nothing (an
            // honest absence, not an error).
            let mut bindings: BTreeMap<String, FixtureBinding> = BTreeMap::new();
            for artifact in &record.artifacts {
                // The binding overlay lives under tests/example-conformance.ttl,
                // carried as a TestDsl artifact (same role as competency.ttl;
                // discriminated by filename suffix).
                if artifact.role != ArtifactRole::TestDsl
                    || !artifact.logical_path.ends_with("example-conformance.ttl")
                {
                    continue;
                }
                let store = parse_turtle_lenient(&artifact.content).unwrap_or_else(|e| {
                    panic!("example-conformance.ttl for slice {owner} failed to parse: {e}")
                });
                bindings.extend(extract_fixture_bindings(&store));
            }
            for artifact in &record.artifacts {
                let kind = match artifact.role {
                    ArtifactRole::TestDsl
                        if artifact
                            .logical_path
                            .starts_with("tests/conformance-fixtures/") =>
                    {
                        DocFixtureKind::Wellformed
                    }
                    ArtifactRole::CounterExample => DocFixtureKind::CounterExample,
                    _ => continue,
                };
                let binding = bindings.get(&artifact.logical_path);
                fixtures.push(extract_fixture(artifact, owner, kind, binding));
            }
        }
        fixtures.sort_by(|a, b| {
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

        // ── Notation grammars (W3C EBNF renderings under grammars/*.ebnf) ───────
        // `.ebnf` files under a slice's `grammars/` directory fall through
        // `purrdf-slice`'s artifact classifier to the open `ArtifactRole::Other`
        // variant (no dedicated role exists for them); matched here by the
        // slice-relative path it carries rather than a bare unit-variant match.
        let mut grammars: Vec<DocGrammar> = Vec::new();
        for record in catalog.records() {
            for artifact in &record.artifacts {
                let ArtifactRole::Other(path) = &artifact.role else {
                    continue;
                };
                if !(path.starts_with("grammars/") && path.ends_with(".ebnf")) {
                    continue;
                }
                grammars.push(extract_grammar(artifact));
            }
        }
        grammars.sort_by(|a, b| a.slug.cmp(&b.slug));

        // ── Concerns (collected from module graphs via gmeow:docsConcern) ──────
        let concerns = extract_concerns(catalog);

        // ── Build-pipeline DAG (the dogfooded build graph authored as data) ────
        let pipeline = extract_pipeline(catalog);

        // ── External terms (linkage objects + non-GMEOW term edges) ────────────
        let external_terms = extract_external_terms(&terms, &linkages);

        // ── Grounding seams (gmeow:Seam individuals, read from every slice's
        // manifest_graph typed gmeow:GroundingSlice) ────────────────────────────
        // Governance data, not module.ttl vocabulary: read directly off the
        // catalog's already-parsed, lossless per-slice manifest IR (no re-parse of
        // manifest.ttl bytes). Generic across every grounding slice — today only
        // `logic:`'s manifest carries the registry, but a future seam authored in
        // `lang:`/`math:` is picked up without a code change.
        let mut seams: Vec<DocSeam> = Vec::new();
        for record in catalog.records() {
            let manifest_store = Store::from_dataset(std::sync::Arc::clone(&record.manifest_graph));
            if is_grounding_slice(&manifest_store, &record.manifest.slice_iri) {
                seams.extend(extract_seams(&manifest_store));
            }
        }
        seams.sort_by(|a, b| a.iri.cmp(&b.iri));
        seams.dedup_by(|a, b| a.iri == b.iri);

        // ── Guides: recipes + learning paths (parsed from module graphs) ───────
        let (recipes, learning_paths) = extract_guides(catalog);

        // ── Translations (built from each slice's i18n/<lang>.po catalogs) ──────
        let translations = Translations::from_catalog(catalog);
        let available_languages = i18n::available_languages(&translations);

        let model = Self {
            title: "GMEOW Ontology Documentation".to_string(),
            version: Self::VERSION.to_string(),
            slices,
            terms,
            dependency_edges,
            mapping_sets,
            linkages,
            examples,
            fixtures,
            shapes,
            competencies,
            grammars,
            loss_targets,
            worked_instances,
            concerns,
            external_terms,
            seams,
            recipes,
            learning_paths,
            constraint_rules: Vec::new(),
            advice_entries: Vec::new(),
            four_boxes: None,
            concept_doi: None,
            pipeline,
            available_languages,
            translations,
            ui_catalog: UiCatalog::default(),
            reasoning: None,
            diagnostics: None,
            term_loss: None,
            schema_fragments: None,
            lang: String::new(),
        };

        // The single link-rewrite authority is a pure function of the assembled
        // model. Build it now to VALIDATE the last markdown-document invariant —
        // that no two documents map to one generated page path
        // ([`DocsError::MarkdownPageCollision`]) — before the model escapes the
        // constructor. Renderers and the RDF projection rebuild it from the model on
        // demand (it is a pure function), so it is not stored on the model.
        crate::source_map::SourceToPageMap::build(&model)?;
        Ok(model)
    }

    /// Discover the slice catalog under `root/slices`, run ownership analysis,
    /// and build the model. Also reads the curated `<root>/docs/four-boxes.md`
    /// prose at build time, if present.
    ///
    /// The per-term content manifest AND the constraint catalog are sourced from the
    /// committed `<root>/generated/catalog/*.nq` files
    /// ([`read_term_manifest`] / [`read_constraint_catalog`]) — the disk-sourced path
    /// for the standalone `make docs` fanout, which runs post-pipeline against the
    /// fanout-refreshed committed files. The in-pipeline `stage-docs-render` run uses
    /// [`discover_with_manifest_and_catalog`](Self::discover_with_manifest_and_catalog)
    /// instead, so the model reflects THIS run's freshly-computed products rather than
    /// lagging one regenerate behind (the stale-disk-fold class).
    pub fn discover(root: &Path) -> Result<Self, DocsError> {
        let manifest = read_term_manifest(root)?;
        Self::discover_with_manifest_map(root, manifest, CatalogSource::Disk)
    }

    /// Same as [`discover`](Self::discover) but sources BOTH the per-term content
    /// manifest AND the constraint catalog from THIS run's fresh pipeline stage
    /// products (`manifest_bytes` from `stage-term-manifest`, `catalog_bytes` from
    /// `stage-constraint-catalog`), instead of the committed (previous-run)
    /// `generated/catalog/*.nq` files on disk. This is the in-pipeline `stage-docs-render`
    /// entry point: when a term's definition digest changes this build the fresh
    /// manifest carries the newly-minted "Definition changed" changelog entry, and on
    /// a cold tree (no materialized `generated/`) the catalog read no longer
    /// hard-fails — both are the stale-disk-fold / cold-absence class this retires.
    /// Shares the whole discovery body with [`discover`](Self::discover) via
    /// [`discover_with_manifest_map`](Self::discover_with_manifest_map); only the
    /// manifest and catalog sources differ.
    pub fn discover_with_manifest_and_catalog(
        root: &Path,
        manifest_bytes: &[u8],
        catalog_bytes: &[u8],
    ) -> Result<Self, DocsError> {
        let manifest = parse_term_manifest(manifest_bytes, "stage-term-manifest product")?;
        Self::discover_with_manifest_map(root, manifest, CatalogSource::Live(catalog_bytes))
    }

    /// Same as [`discover`](Self::discover) but sources the constraint catalog from
    /// THIS run's freshly-rendered `stage-constraint-catalog` bytes instead of the
    /// committed `generated/catalog/constraint-catalog.nq` on disk. The in-pipeline
    /// DocMaturity axis (slice-quality) uses this so a cold tree does not hard-fail
    /// on the not-yet-materialized catalog, and every run scores against the SAME
    /// freshly-produced catalog (cold == warm — the catalog content does not feed the
    /// coverage fraction, so the guarantee is that the model always BUILDS, never
    /// collapsing every slice to the vacuous model-unavailable 1.0 a failed disk read
    /// would force). The per-term content manifest stays disk-sourced and tolerant
    /// ([`read_term_manifest`] returns an empty map when absent), because it is
    /// provenance-only and likewise does not feed the coverage fraction.
    pub fn discover_with_catalog(root: &Path, catalog_bytes: &[u8]) -> Result<Self, DocsError> {
        let manifest = read_term_manifest(root)?;
        Self::discover_with_manifest_map(root, manifest, CatalogSource::Live(catalog_bytes))
    }

    /// The shared discovery body: build the model from the slice catalog and layer
    /// on every repo-only enrichment, applying the already-obtained per-term content
    /// `manifest` (from disk in [`discover`](Self::discover), from the fresh stage
    /// product in [`discover_with_manifest_and_catalog`](Self::discover_with_manifest_and_catalog))
    /// and sourcing the constraint catalog per `catalog` (disk in [`discover`](Self::discover),
    /// live stage bytes in the in-pipeline entry points).
    fn discover_with_manifest_map(
        root: &Path,
        manifest: BTreeMap<String, TermProvenance>,
        catalog_source: CatalogSource<'_>,
    ) -> Result<Self, DocsError> {
        let catalog = SliceCatalog::discover(&root.join("slices"), gmeow_ns::gmeow_slice_vocab())?;
        let ownership = OwnershipAnalyzer::new(&catalog).analyze()?;
        let central_sets = read_central_mapping_sets(root)?;
        let mut model = Self::from_catalog(&catalog, &ownership, &central_sets)?;
        model.four_boxes = std::fs::read_to_string(root.join("docs/four-boxes.md")).ok();
        // Concept DOI for the per-term citation block: the `dcterms:identifier` on
        // the `gmeow:Work` subject of the self-description.
        model.concept_doi = read_concept_doi(root);
        // Root-level SHACL shapes (`<root>/shapes/*.ttl`) — aggregate node shapes
        // not owned by any single slice — merged into the slice-level shapes and
        // deduped by (target term, messages).
        merge_root_shapes(&mut model, &root.join("shapes"));
        // Optional UI-chrome overrides: `<root>/i18n/ontology-docs-templates.<lang>.po`.
        model.ui_catalog = UiCatalog::from_dir(&root.join("i18n"));
        // The constraint catalog (`gmeow:ValidationRule` individuals). Sourced per
        // `catalog`: the committed N-Quads fanout artifact on disk (post-pipeline /
        // CLI consumers), or THIS run's freshly-rendered `stage-constraint-catalog`
        // bytes (the in-pipeline consumers, which must not read a not-yet-materialized
        // `generated/` file). An unparsable/malformed catalog is a broken invariant
        // either way — hard-fail rather than render an empty-state page.
        let parsed_catalog = match catalog_source {
            CatalogSource::Disk => read_constraint_catalog(root)?,
            CatalogSource::Live(bytes) => {
                parse_constraint_catalog(bytes, "stage-constraint-catalog product")?
            }
        };
        model.constraint_rules = parsed_catalog.rules;
        model.advice_entries = parsed_catalog.advice;
        // Resolve each fixture's catalog_slug now that constraint_rules is
        // populated (from_catalog runs before the catalog is read, so every
        // fixture starts with catalog_slug: None).
        apply_fixture_catalog_slugs(&mut model);
        // Resolve each competency question's declared `cqQueryFile` to its
        // repo-root-relative `.rq` file text (`extract_competency` only fills
        // `query_text` from an inline `cqQuery`, since it never sees the repo
        // root). Hard-fails on a dangling `cqQueryFile` — see `DocsError::CompetencyQuery`.
        apply_competency_query_text(&mut model, root)?;
        // The per-term content-address manifest (already obtained by the caller:
        // from the committed N-Quads fanout artifact in `discover`, or from THIS
        // run's fresh stage-term-manifest product in `discover_with_manifest_and_catalog`). It
        // sets each documented term's content digest and first-seen version and
        // unions the computed changelog into the authored one. A term absent from
        // the manifest is a term added since the last commit — its content-address
        // self-heals on the next regenerate pass (the stage recomputes the manifest
        // THIS build; the committed docs catch up the next), so it is skipped rather
        // than a hard-fail (the two-phase fixed-point convergence, not a coverage bug).
        apply_term_manifest(&mut model, manifest);
        Ok(model)
    }

    /// Build a documentation model scoped to EXACTLY ONE external slice directory —
    /// the whole slice rooted at `slice_dir`, never a repo-wide `slices/` sweep.
    /// [`SliceCatalog::discover`] stops recursing at the first `manifest.ttl` it
    /// meets, so pointing it straight at one slice dir yields a catalog with
    /// exactly one [`SliceRecord`] scoped to `slice_dir`. slice-quality's
    /// DocMaturity axis reads this to measure a foreign slice's documentation
    /// coverage from that slice's OWN files (`module.ttl`, `docs.md`,
    /// `examples/*.ttl`, …), never the host repo's.
    ///
    /// This deliberately returns a bare [`from_catalog`](Self::from_catalog) model.
    /// The repo-only enrichments `discover` layers on afterward (constraint
    /// catalog, term content manifest, root SHACL shapes, four-boxes prose, concept
    /// DOI, `cqQueryFile` resolution) do NOT feed the per-term / per-slice coverage
    /// dimension computation in [`crate::coverage`], so a `from_catalog` model is
    /// full-fidelity for maturity scoring. `central_mapping_sets` is empty (`&[]`):
    /// a foreign slice carries its own mapping sets, and no cross-slice publication
    /// header is in scope here.
    ///
    /// The resulting model's [`term_loss`](Self::term_loss) is `None`, and that is a
    /// DELIBERATE, correct scope boundary — a not-applicable fact, NEVER an
    /// "unknown" or "failed" join. The dynamic per-term projection-loss ledger is a
    /// `stage-mappings` product; a foreign slice pulled in on its own was never
    /// compiled through the pipeline, so it has no dynamic projection-loss rows to
    /// attach. The STATIC [`loss_targets`](Self::loss_targets) discovered from the
    /// slice's own `examples/*.ttl` still populate, so `applicable_lossy` remains
    /// driven honestly by the slice's authored content.
    pub fn from_slice_dir(slice_dir: &Path) -> Result<Self, DocsError> {
        let catalog = SliceCatalog::discover(slice_dir, gmeow_ns::gmeow_slice_vocab())?;
        let ownership = OwnershipAnalyzer::new(&catalog).analyze()?;
        Self::from_catalog(&catalog, &ownership, &[])
    }
}

/// The prior-independent provenance a term carries in the committed manifest.
struct TermProvenance {
    /// `gmeow:definitionDigest` — the term's content-address (always present).
    digest: String,
    /// `gmeow:addedInVersion` — the release the term was first seen in.
    added_in_version: Option<String>,
    /// The computed changelog: one entry per release whose digest diverged.
    changelog: Vec<DocChangelogEntry>,
}

/// Read the term content manifest from
/// `<root>/generated/catalog/term-content-manifest.nq` — every term's
/// `gmeow:definitionDigest`, `gmeow:addedInVersion`, and reified
/// `gmeow:hasChangelogEntry` records, keyed by term IRI. The file is a committed
/// fixed-point projection of `gmeow.gts` (N-Quads: every triple in the manifest
/// fanout named graph), so the reader queries graph-agnostically.
///
/// This is a hard-fail read (mirrors [`read_constraint_catalog`]): a regenerated
/// tree always carries this artifact, so a missing file, an unparsable one, or a
/// term subject with no `gmeow:definitionDigest` is a broken invariant, never an
/// optional input.
fn read_term_manifest(root: &Path) -> Result<BTreeMap<String, TermProvenance>, DocsError> {
    let path = root.join("generated/catalog/term-content-manifest.nq");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        // Absent only during the one-shot bootstrap build that first mints the
        // manifest (the stage writes it THIS build; the committed docs pick it up
        // the next pass). An empty map skips every term's content-address for this
        // pass — the two-phase fixed-point convergence. Strict sync still
        // guarantees the committed manifest is present + current in a landed tree,
        // so a genuinely-missing committed manifest is caught there, not silently.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => {
            return Err(DocsError::TermManifest(format!(
                "cannot read {}: {e}",
                path.display()
            )));
        }
    };
    parse_term_manifest(&bytes, &path.display().to_string())
}

/// Parse term-content-manifest N-Quads (`source` names them for diagnostics) into
/// the per-term provenance map. Shared by the committed-file reader
/// ([`read_term_manifest`]) and the fresh-stage-product path
/// ([`DocsModel::discover_with_manifest_and_catalog`]).
fn parse_term_manifest(
    bytes: &[u8],
    source: &str,
) -> Result<BTreeMap<String, TermProvenance>, DocsError> {
    let store = Store::parse_nquads(bytes)
        .map_err(|e| DocsError::TermManifest(format!("cannot parse {source}: {e}")))?;
    let mut out: BTreeMap<String, TermProvenance> = BTreeMap::new();
    for term in store.subjects_with_predicate_any(GMEOW_DEFINITION_DIGEST) {
        // The digest is the identity; a term subject with none is malformed
        // generated data, not a tolerable optional field.
        let digest = store
            .first_literal_any(&term, GMEOW_DEFINITION_DIGEST)
            .ok_or_else(|| {
                DocsError::TermManifest(format!(
                    "term {term} in {source} carries no gmeow:definitionDigest"
                ))
            })?;
        let added_in_version = store.first_literal_any(&term, GMEOW_ADDED_IN_VERSION);
        let mut changelog: Vec<DocChangelogEntry> = Vec::new();
        for object in store.objects_any(&term, GMEOW_HAS_CHANGELOG_ENTRY) {
            let entry_node = match object {
                Object::Named(n) => Node::Named(n),
                Object::Blank(b) => Node::Blank(b),
                _ => continue,
            };
            let version = store.first_literal_of_any(&entry_node, GMEOW_ENTRY_VERSION);
            let note = store.first_literal_of_any(&entry_node, GMEOW_ENTRY_NOTE);
            if let Some(version) = version {
                changelog.push(DocChangelogEntry { version, note });
            }
        }
        changelog.sort();
        changelog.dedup();
        out.insert(
            term,
            TermProvenance {
                digest,
                added_in_version,
                changelog,
            },
        );
    }
    Ok(out)
}

/// Apply the term content manifest to a discovered model: set each documented
/// term's `content_digest` and (manifest-authoritative) `added_in_version`, and
/// UNION the manifest's computed changelog with the authored one (keyed by version;
/// the authored `entryNote` wins a collision; authored-only and manifest-only
/// versions are both kept).
///
/// A documented term with NO manifest entry is a term added since the last commit:
/// the stage recomputes the manifest to cover it THIS build, but the committed file
/// the model reads still lags by one build. Such a term keeps its authored
/// provenance (empty `content_digest`, so the content-address citation line is
/// simply omitted until the next regenerate pass promotes the fresh manifest) — the
/// two-phase fixed-point convergence, never a hard-fail that would brick a
/// term-adding regenerate.
///
/// The `manifest` map is obtained by the caller — from the committed on-disk file
/// ([`read_term_manifest`], used by [`DocsModel::discover`]) or from THIS run's
/// fresh `stage-term-manifest` product bytes ([`parse_term_manifest`], used by
/// [`DocsModel::discover_with_manifest_and_catalog`]) — so the pure application logic
/// below is identical regardless of the manifest source.
fn apply_term_manifest(model: &mut DocsModel, manifest: BTreeMap<String, TermProvenance>) {
    for term in &mut model.terms {
        let Some(provenance) = manifest.get(&term.iri) else {
            continue;
        };
        term.content_digest = provenance.digest.clone();
        term.added_in_version = provenance.added_in_version.clone();
        // Union by version: seed with the manifest entries, then let the authored
        // entries override (authored note wins) — authored-only and manifest-only
        // versions both survive.
        let mut by_version: BTreeMap<String, Option<String>> = BTreeMap::new();
        for entry in &provenance.changelog {
            by_version.insert(entry.version.clone(), entry.note.clone());
        }
        for entry in &term.changelog {
            by_version.insert(entry.version.clone(), entry.note.clone());
        }
        let mut merged: Vec<DocChangelogEntry> = by_version
            .into_iter()
            .map(|(version, note)| DocChangelogEntry { version, note })
            .collect();
        merged.sort();
        merged.dedup();
        term.changelog = merged;
    }
}

/// Resolve each fixture's [`DocFixture::catalog_slug`] from its
/// [`violation_code`](DocFixture::violation_code) against
/// `model.constraint_rules`, once the catalog is populated (`from_catalog` runs
/// before the committed catalog is read, so every fixture starts with
/// `catalog_slug: None`). Only sets a slug when a genuine
/// [`ConstraintRule::code`] match exists — the catalog's `shacl.*` codes are
/// currently generic (`shacl.nonconforming`), not per-constraint-component, so
/// a fixture's `shacl.<ConstraintComponent>` code has no match today; that is
/// an honest absence, never a fabricated link.
fn apply_fixture_catalog_slugs(model: &mut DocsModel) {
    let by_code: BTreeMap<String, String> = model
        .constraint_rules
        .iter()
        .map(|r| (r.code.clone(), r.slug.clone()))
        .collect();
    for fixture in &mut model.fixtures {
        fixture.catalog_slug = fixture
            .violation_code
            .as_ref()
            .and_then(|code| by_code.get(code))
            .cloned();
    }
}

/// The repo-root-relative directory roots a `gmeow:cqQueryFile` may resolve into:
/// the shared root-level SPARQL tree and a slice's own committed queries.
///
/// Both are content-addressed by `crate::fixture::cache_key` (which walks `queries`
/// and `slices` in full), which is what makes the documentation-model fixture cache
/// sound with respect to competency-query text. `crate::fixture` re-exports this as
/// its own boundary constant so the two can never drift.
pub const COMPETENCY_QUERY_ROOTS: &[&str] = &["queries/", "slices/"];

/// Whether `rel` is a legal `gmeow:cqQueryFile` value: repo-root-relative (never
/// absolute), free of any `..` component (which could escape the hashed roots
/// while still passing a naive prefix test), and under one of
/// [`COMPETENCY_QUERY_ROOTS`].
fn is_competency_query_path(rel: &str) -> bool {
    if rel.starts_with('/') || rel.starts_with('\\') {
        return false;
    }
    if rel.split(['/', '\\']).any(|seg| seg == "..") {
        return false;
    }
    COMPETENCY_QUERY_ROOTS.iter().any(|r| rel.starts_with(r))
}

/// Resolve each [`DocCompetency::query_file`] to its [`DocCompetency::query_text`]
/// by reading the repo-root-relative `.rq` path. `query_file` is
/// REPO-ROOT-RELATIVE regardless of whether it happens to start with
/// `slices/<group>/<name>/…` (a slice's own committed query) or
/// `queries/competency/…` / `queries/qc/…` (the shared root-level query tree) —
/// this is exactly `crates/slicetest/src/paths.rs::query_file`'s own resolution
/// contract (`repo_root().join(rel)`), the single source of truth the executing
/// competency harness already uses, so docs reuses it rather than guessing at a
/// slice-relative convention that does not exist in the data.
///
/// A CQ with `query_text` already set (an inline `gmeow:cqQuery`) is left alone.
/// A CQ with `query_file` set but no readable file at that path is a hard
/// fail — `cqQueryFile` existing is the ontology's own claim that the file
/// resolves; a dangling reference is a data bug, not an honest absence.
///
/// A `cqQueryFile` that resolves OUTSIDE [`COMPETENCY_QUERY_ROOTS`] is likewise a
/// hard fail. Two reasons, both structural: it is outside the resolution contract
/// `crates/slicetest/src/paths.rs::query_file` documents (so the executing
/// competency harness and the docs model would disagree about where the query
/// lives), and it is outside the content-addressed input set
/// `crate::fixture::cache_key` folds — a query whose TEXT changed under an
/// unhashed path would be served stale from `.cache/docs-fixture` forever. Making
/// the boundary an error keeps the cache sound BY CONSTRUCTION rather than by the
/// authoring convention that today's values all happen to satisfy.
fn apply_competency_query_text(model: &mut DocsModel, root: &Path) -> Result<(), DocsError> {
    for cq in &mut model.competencies {
        if cq.query_text.is_some() {
            continue;
        }
        let Some(rel) = &cq.query_file else { continue };
        if !is_competency_query_path(rel) {
            return Err(DocsError::CompetencyQuery(format!(
                "competency question <{}> declares gmeow:cqQueryFile {rel:?}, which is outside \
                 the resolution contract: the path must be repo-root-relative, free of `..`, and \
                 under one of {}",
                cq.iri,
                COMPETENCY_QUERY_ROOTS.join(" / "),
            )));
        }
        let path = root.join(rel);
        let text = std::fs::read_to_string(&path).map_err(|e| {
            DocsError::CompetencyQuery(format!(
                "competency question <{}> declares gmeow:cqQueryFile {rel:?} but the file could \
                 not be read at {}: {e}",
                cq.iri,
                path.display()
            ))
        })?;
        cq.query_text = Some(text);
    }
    Ok(())
}

/// Where [`DocsModel::discover_with_manifest_map`] sources the constraint-catalog
/// N-Quads: the committed `generated/catalog/constraint-catalog.nq` on disk
/// (post-pipeline / CLI consumers scanning a materialized tree), or THIS run's
/// freshly-rendered `stage-constraint-catalog` bytes carried in from the pipeline
/// (the in-pipeline consumers, which must never read a not-yet-materialized
/// `generated/` file — the cold-absence class this retires).
enum CatalogSource<'a> {
    /// Read the committed `generated/catalog/constraint-catalog.nq` off disk.
    Disk,
    /// Use THIS run's freshly-rendered catalog bytes (never a disk read).
    Live(&'a [u8]),
}

/// Read the constraint catalog from `<root>/generated/catalog/constraint-catalog.nq`
/// — every `gmeow:ValidationRule` individual, sorted by rule code. The file is a
/// committed fixed-point projection of `gmeow.gts` (N-Quads: every triple in the
/// catalog fanout named graph), so the reader queries graph-agnostically.
///
/// This is a hard-fail read: a regenerated tree always carries this artifact, so
/// a missing file, an unparsable one, or a `gmeow:ValidationRule` subject with no
/// `gmeow:ruleCode` is a broken invariant (a pipeline bug), never an optional
/// input — the caller must stop and report rather than silently render the
/// "What GMEOW enforces" page as empty.
fn read_constraint_catalog(root: &Path) -> Result<ParsedCatalog, DocsError> {
    let path = root.join("generated/catalog/constraint-catalog.nq");
    let bytes = std::fs::read(&path).map_err(|e| {
        DocsError::ConstraintCatalog(format!("cannot read {}: {e}", path.display()))
    })?;
    parse_constraint_catalog(&bytes, &path.display().to_string())
}

/// The two tiers a `constraint-catalog.nq` carries: the enforced-check
/// [`ConstraintRule`] compliance tier and the [`AdviceEntry`] recommendation tier.
/// Parsed together from one document so the disk and live sources cannot diverge.
struct ParsedCatalog {
    rules: Vec<ConstraintRule>,
    advice: Vec<AdviceEntry>,
}

/// Parse constraint-catalog N-Quads (`source` names them for diagnostics) into the
/// sorted [`ConstraintRule`] list. Shared by the committed-file reader
/// ([`read_constraint_catalog`]) and the in-pipeline fresh-stage-product path
/// ([`DocsModel::discover_with_catalog`] /
/// [`DocsModel::discover_with_manifest_and_catalog`]), so the disk and live sources
/// can never diverge in how a rule is decoded. An unparsable document or a
/// `gmeow:ValidationRule` subject with no `gmeow:ruleCode` is a broken invariant,
/// never an optional input.
fn parse_constraint_catalog(bytes: &[u8], source: &str) -> Result<ParsedCatalog, DocsError> {
    let store = Store::parse_nquads(bytes)
        .map_err(|e| DocsError::ConstraintCatalog(format!("cannot parse {source}: {e}")))?;
    let mut rules: Vec<ConstraintRule> = Vec::new();
    for iri in store.subjects_of_type_any(GMEOW_VALIDATION_RULE) {
        // The rule code is the identity; a subject with none is malformed
        // generated data, not a tolerable optional field.
        let code = store
            .first_literal_any(&iri, GMEOW_RULE_CODE)
            .ok_or_else(|| {
                DocsError::ConstraintCatalog(format!(
                    "gmeow:ValidationRule {iri} in {source} carries no gmeow:ruleCode"
                ))
            })?;
        let slug = gmeow_validate::rule_catalog::slugify(&code);
        let category = store
            .named_objects_any(&iri, GMEOW_RULE_CATEGORY)
            .into_iter()
            .min()
            .unwrap_or_default();
        let severity = store
            .first_literal_any(&iri, GMEOW_RULE_SEVERITY)
            .unwrap_or_default();
        let help_uri = store
            .first_literal_any(&iri, GMEOW_RULE_HELP_URI)
            .unwrap_or_default();
        let label = store.first_literal_any(&iri, RDFS_LABEL);
        let definition = store.first_literal_any(&iri, SKOS_DEFINITION);
        let applies_to_terms = store.named_objects_any(&iri, GMEOW_APPLIES_TO_TERM);
        let formalizes = store
            .named_objects_any(&iri, LOGIC_FORMALIZES)
            .into_iter()
            .min();
        rules.push(ConstraintRule {
            code,
            slug,
            category,
            severity,
            help_uri,
            label,
            definition,
            applies_to_terms,
            formalizes,
        });
    }
    rules.sort_by(|a, b| a.code.cmp(&b.code));

    // The advice tier: every gmeow:AdviceEntry, keyed by its governed term. The
    // recommendation peer of the ValidationRule loop above, parsed from the SAME
    // document so disk and live sources cannot diverge. A subject with no governed
    // term is malformed generated data, never a tolerable optional field.
    let mut advice: Vec<AdviceEntry> = Vec::new();
    for iri in store.subjects_of_type_any(GMEOW_ADVICE_ENTRY) {
        let term = store
            .named_objects_any(&iri, GMEOW_APPLIES_TO_TERM)
            .into_iter()
            .min()
            .or_else(|| {
                store
                    .named_objects_any(&iri, LOGIC_FORMALIZES)
                    .into_iter()
                    .min()
            })
            .ok_or_else(|| {
                DocsError::ConstraintCatalog(format!(
                    "gmeow:AdviceEntry {iri} in {source} carries no governed term \
                     (gmeow:appliesToTerm / logic:formalizes)"
                ))
            })?;
        let local = term.rsplit(['/', '#']).next().unwrap_or(&term);
        let slug = format!("advice-{}", gmeow_validate::rule_catalog::slugify(local));
        advice.push(AdviceEntry {
            term: term.clone(),
            slug,
            label: store.first_literal_any(&iri, RDFS_LABEL),
            definition: store.first_literal_any(&iri, SKOS_DEFINITION),
            avoid_when: store.literals_any(&iri, GMEOW_ADVICE_AVOID_WHEN),
            use_when: store.literals_any(&iri, GMEOW_ADVICE_USE_WHEN),
            how_to_use: store.literals_any(&iri, GMEOW_ADVICE_HOW_TO_USE),
            documented_by_rule: store
                .named_objects_any(&iri, GMEOW_DOCUMENTED_BY_RULE)
                .into_iter()
                .min(),
        });
    }
    advice.sort_by(|a, b| a.term.cmp(&b.term));

    Ok(ParsedCatalog { rules, advice })
}

/// Read the ontology's concept DOI from `<root>/metadata/gmeow-self.ttl`: the
/// `dcterms:identifier` literal on the `gmeow:Work` subject. Returns
/// `None` if the file is absent, unparsable, or carries no Work DOI — the
/// citation block degrades to the term-IRI permalink alone.
fn read_concept_doi(root: &Path) -> Option<String> {
    let bytes = std::fs::read(root.join("metadata/gmeow-self.ttl")).ok()?;
    let store = parse_turtle_lenient(&bytes).ok()?;
    // The self-description may carry more than one gmeow:Work (e.g. the root
    // ontology Work and a visual-identity Work), and store iteration order is
    // not stable, so scan every Work and return the first `dcterms:identifier`
    // found — only the concept-bearing Work carries the DOI.
    store
        .subjects_of_type(GMEOW_WORK)
        .into_iter()
        .find_map(|work| store.first_literal(&work, DCTERMS_IDENTIFIER))
}

/// Read the cross-slice SSSOM `gmeow:MappingSet` publication headers from
/// `<root>/dsl/mappings/mapping-sets.ttl`. Each set aggregates linkage cells
/// authored across many slices (one set per external vocabulary), so it has no
/// single owning slice — it is marked with the ontology-root owner. Only the
/// `gmeow:MappingSet` headers are read; the file carries no alignment cells.
///
/// An **absent** file ⇒ `Ok(Vec::new())` (slices carry their own sets). A file
/// that **exists but is unreadable or unparsable** is a hard failure, never a
/// silent empty: a dropped set would leave every relocated slice linkage
/// resolving its `MappingSet` IRI to the raw filename. This matches the
/// hard-fail policy of the sibling readers in this impl block
/// (`read_constraint_catalog`, module/shapes/competency).
fn read_central_mapping_sets(root: &Path) -> Result<Vec<DocMappingSet>, DocsError> {
    const CROSS_SLICE_OWNER: &str = "https://blackcatinformatics.ca/gmeow/";
    let path = root.join("dsl/mappings/mapping-sets.ttl");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(DocsError::MappingSets(format!(
                "cannot read {}: {e}",
                path.display()
            )));
        }
    };
    let store = parse_turtle_lenient(&bytes)
        .map_err(|e| DocsError::MappingSets(format!("cannot parse {}: {e}", path.display())))?;
    let (sets, _links) = extract_mappings(&store, CROSS_SLICE_OWNER);
    Ok(sets)
}

// ── Turtle parsing + term extraction ──────────────────────────────────────────

/// Parse Turtle bytes into the native [`Store`] query wrapper via the native
/// codecs (the gmeow-gts codecs are lenient on GMEOW's `@x-gmeow-*` language
/// tags). The store is kept for the term-extraction pattern queries; no oxigraph
/// round-trip.
pub(crate) fn parse_turtle_lenient(bytes: &[u8]) -> Result<Store, SliceError> {
    Store::parse_turtle(bytes)
}

/// Extract documented terms (GMEOW-namespaced typed subjects) from a module store.
/// The grounding vocabulary namespace a slice OWNS, if it is one of the three
/// grounding slices (`slices/grounding/{math,logic,lang}`, whose slice IRIs end
/// `/slices/{math,logic,lang}`). A grounding slice declares its vocabulary in
/// its OWN namespace (`math:` / `logic:` / `lang:`), not `gmeow:`, so its
/// own-namespace TBox classes / properties / named vocabulary individuals become
/// documented terms in addition to any `gmeow:` ones. A non-grounding slice
/// returns `None` — its documented-term set is unchanged (`gmeow:` only).
fn grounding_namespace(owner_slice: &str) -> Option<&'static str> {
    if owner_slice.ends_with("/slices/math") {
        Some(MATH_NS)
    } else if owner_slice.ends_with("/slices/logic") {
        Some(LOGIC_NS)
    } else if owner_slice.ends_with("/slices/lang") {
        Some(LANG_NS)
    } else {
        None
    }
}

/// Whether a module subject is a documentable-term subject for a slice: any
/// `gmeow:` subject, plus — for a grounding slice — any subject in that slice's
/// OWN grounding namespace. The worked-instance / stereotype ABox nodes a
/// grounding module also carries (subjects typed only by a domain class, e.g.
/// `logic:Formula` / `math:Axiom` / `lang:Denotation`) are NOT admitted here as
/// terms: [`category_for_type`] returns `None` for those types, so they surface
/// only as worked-instances / examples on the vocabulary terms' pages, never as
/// standalone term pages.
fn is_documented_subject(subject: &str, grounding_ns: Option<&str>) -> bool {
    subject.starts_with(GMEOW_NS) || grounding_ns.is_some_and(|ns| subject.starts_with(ns))
}

fn extract_terms(store: &Store, owner_slice: &str, tier: Option<&SliceTier>) -> Vec<DocTerm> {
    // First pass: collect every documentable subject with a recognized vocabulary
    // type, keyed by IRI, recording the strongest category seen. A grounding slice
    // also admits its own-namespace vocabulary (`math:` / `logic:` / `lang:`).
    let grounding_ns = grounding_namespace(owner_slice);
    let mut categories: BTreeMap<String, DocTermCategory> = BTreeMap::new();

    for (subject, object) in store.pattern_subjects_objects(RDF_TYPE) {
        let Some(subject) = subject.as_named() else {
            continue;
        };
        if !is_documented_subject(subject, grounding_ns) {
            continue;
        }
        let Object::Named(type_node) = &object else {
            continue;
        };
        let Some(category) = category_for_type(type_node.as_str()) else {
            continue;
        };
        let entry = categories.entry(subject.to_string()).or_insert(category);
        // Prefer the more specific category (Class/Property/Datatype) over
        // a bare Individual / Other when a subject is multiply typed.
        if category_rank(category) > category_rank(*entry) {
            *entry = category;
        }
    }

    build_doc_terms(store, categories, owner_slice, tier)
}

/// Lift every GMEOW-namespaced `a logic:PathShape` subject in an EXAMPLE store as a
/// documented [`DocTermCategory::Individual`] term. A `logic:PathShape` is, by
/// canonical design (`design/LOGIC-PATHS.md`), "a first-class, reusable, by-name
/// term", but the only authored PathShape INSTANCES live in worked-example
/// artifacts (e.g. `slices/grounding/logic/examples/predicate-paths.ttl`), never a
/// `module.ttl` — so the module-only [`extract_terms`] scan never sees them and
/// their `property-path:<iri>` projection-loss rows joined no term page
/// (`TermLossDigest.by_term` was vacuous). This focused scan admits ONLY the
/// `logic:PathShape` type: an example's demonstrative ABox (its
/// `owl:NamedIndividual` / class instances) is NOT lifted — that stays example
/// payload, not documented vocabulary. The full-IRI subject is what the ledger
/// row's `property-path:<iri>` label strips to, so the resulting `DocTerm.iri` is
/// byte-identical to the join key.
fn extract_path_shape_terms(
    store: &Store,
    owner_slice: &str,
    tier: Option<&SliceTier>,
) -> Vec<DocTerm> {
    let mut categories: BTreeMap<String, DocTermCategory> = BTreeMap::new();
    for (subject, object) in store.pattern_subjects_objects(RDF_TYPE) {
        let Some(subject) = subject.as_named() else {
            continue;
        };
        if !subject.starts_with(GMEOW_NS) {
            continue;
        }
        let Object::Named(type_node) = &object else {
            continue;
        };
        if type_node.as_str() != LOGIC_PATH_SHAPE {
            continue;
        }
        categories
            .entry(subject.to_string())
            .or_insert(DocTermCategory::Individual);
    }
    build_doc_terms(store, categories, owner_slice, tier)
}

/// Build one [`DocTerm`] per `(iri, category)` in `categories`, reading its label /
/// definition / relations / lifecycle off `store`. Shared by the module-wide
/// [`extract_terms`] scan and the example-only [`extract_path_shape_terms`] scan.
fn build_doc_terms(
    store: &Store,
    categories: BTreeMap<String, DocTermCategory>,
    owner_slice: &str,
    tier: Option<&SliceTier>,
) -> Vec<DocTerm> {
    let mut terms = Vec::new();
    for (iri, category) in categories {
        let label = first_literal(store, &iri, RDFS_LABEL);
        let definition = first_literal(store, &iri, SKOS_DEFINITION)
            .or_else(|| first_literal(store, &iri, RDFS_COMMENT));

        // BOTH spellings of the subsumption edge: a term re-authored onto the
        // canonical `logic:subClassOf` has no `rdfs:` edge at all, and reading only
        // the projection renders it with an empty parent list and an empty
        // hierarchy diagram. One definition, in `gmeow_ns`.
        let mut parents: Vec<String> = gmeow_ns::subsumption_predicates()
            .into_iter()
            .flat_map(|predicate| named_objects(store, &iri, predicate))
            .collect();
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
        // External-correspondence intent the term declares for itself (sorted,
        // deduped by `sorted_literals`). Drives alignment/linkage APPLICABILITY.
        let adoption_targets = sorted_literals(store, &iri, GMEOW_ADOPTION_TARGET);

        // Logic stereotypes: co-asserted `rdf:type` values under the logic NS.
        let logic_stereotypes = logic_stereotypes(store, &iri);

        // Logical frameworks: the closed logic:LogicalFramework discipline(s) the
        // term declares via logic:instantiatesFramework, rendered as `logic:`-prefixed
        // CURIEs (mirroring logic_stereotypes — `to_curie` only abbreviates gmeow:),
        // sorted/deduped.
        let mut frameworks: Vec<String> = named_objects(store, &iri, LOGIC_INSTANTIATES_FRAMEWORK)
            .iter()
            .filter_map(|o| {
                o.strip_prefix(LOGIC_NS)
                    .map(|local| format!("logic:{local}"))
            })
            .collect();
        frameworks.sort();
        frameworks.dedup();

        // Related terms: union of skos:related + gmeow:pairsWith + rdfs:seeAlso
        // (IRIs; resolved bidirectionally in `from_catalog`).
        let mut related_terms = named_objects(store, &iri, SKOS_RELATED);
        related_terms.extend(named_objects(store, &iri, GMEOW_PAIRS_WITH));
        related_terms.extend(named_objects(store, &iri, RDFS_SEE_ALSO));
        related_terms.sort();
        related_terms.dedup();

        // Four-boxes roles: every gmeow:graphBoxRole CURIE (sorted/deduped by
        // `curie_objects`), plus the lowest-sorted one for the legacy singular.
        let mut box_roles = curie_objects(store, &iri, GMEOW_GRAPH_BOX_ROLE);
        box_roles.sort();
        box_roles.dedup();
        let box_role = box_roles.first().cloned();

        // Per-term lifecycle: maturity badge (fully resolved with the
        // owner-slice tier in hand), added-in version, and reified changelog.
        let stability = resolve_stability(store, &iri, tier);
        let added_in_version = first_literal(store, &iri, GMEOW_ADDED_IN_VERSION);
        let changelog = extract_changelog(store, &iri);

        let curie = to_curie(&iri);
        terms.push(DocTerm {
            iri,
            // Resolved after the whole term set is assembled (build() calls
            // render::resolve_term_slugs); empty here is a placeholder.
            slug: String::new(),
            curie,
            label,
            definition,
            // The parsed model IS the canonical English carrier; the completeness
            // fallbacks read `label`/`definition` directly. `localize_model` stashes
            // these when it later builds a translated copy.
            canonical_label: None,
            canonical_definition: None,
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
            adoption_targets,
            logic_stereotypes,
            frameworks,
            related_terms,
            box_role,
            box_roles,
            formalized_by: Vec::new(),
            stability,
            added_in_version,
            changelog,
            // The content-address is read from the committed term content manifest
            // in `discover` (a disk-read leaf), not from the module graph.
            content_digest: String::new(),
            // Profile membership needs the full slice set; computed in
            // `from_catalog`'s second pass.
            profiles: Vec::new(),
        });
    }
    terms
}

/// Resolve a term's stability badge: an explicit `gmeow:termStability`
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
    // The store spells deprecation in the canonical `logic:deprecated`; its
    // generated OWL view uses `owl:deprecated`. Read both spellings.
    if gmeow_ns::DEPRECATED
        .iter()
        .flat_map(|&pred| literals(store, iri, pred))
        .any(|v| v == "true")
    {
        return DocTermStability::Deprecated;
    }
    match tier {
        Some(SliceTier::Extension) => DocTermStability::Experimental,
        _ => DocTermStability::Stable,
    }
}

/// Extract a term's reified changelog entries: each
/// `?term gmeow:hasChangelogEntry ?entry` whose `?entry` carries a
/// `gmeow:entryVersion` (required) and optional `gmeow:entryNote`. Sorted by
/// `(version, note)`; oxigraph blank-node iteration order is not stable.
fn extract_changelog(store: &Store, iri: &str) -> Vec<DocChangelogEntry> {
    let mut entries: Vec<DocChangelogEntry> = Vec::new();
    for object in store.objects(iri, GMEOW_HAS_CHANGELOG_ENTRY) {
        let entry_node = match object {
            Object::Named(n) => Node::Named(n),
            Object::Blank(b) => Node::Blank(b),
            _ => continue,
        };
        let version = store.first_literal_of(&entry_node, GMEOW_ENTRY_VERSION);
        let note = store.first_literal_of(&entry_node, GMEOW_ENTRY_NOTE);
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
        .pattern_subjects_objects(LOGIC_FORMALIZES)
        .into_iter()
        .filter_map(|(subject, object)| match (subject, object) {
            (Node::Named(s), Object::Named(target)) => Some((s, target)),
            _ => None,
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
        for (subject, object) in store.pattern_subjects_objects(target_pred) {
            let Object::Named(target_term) = object else {
                continue;
            };
            // Term pages exist only for GMEOW terms — only those are documentable.
            if !target_term.starts_with(GMEOW_NS) {
                continue;
            }
            let messages = shape_messages(store, &subject);
            let shape_iri = match &subject {
                Node::Named(n) => n.clone(),
                Node::Blank(label) => format!("_:{label}"),
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
fn shape_messages(store: &Store, start: &Node) -> Vec<String> {
    use std::collections::{HashSet, VecDeque};
    let mut seen: HashSet<Node> = HashSet::new();
    let mut queue: VecDeque<Node> = VecDeque::from([start.clone()]);
    let mut msgs: Vec<String> = Vec::new();
    while let Some(node) = queue.pop_front() {
        if !seen.insert(node.clone()) {
            continue;
        }
        for (predicate, object) in store.predicate_objects_of(&node) {
            let is_message = predicate == SH_MESSAGE;
            match object {
                Object::Literal(value) if is_message => msgs.push(value),
                Object::Blank(label) => queue.push_back(Node::Blank(label)),
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
///
/// `query_text` is only filled in here for an inline `gmeow:cqQuery`; a
/// `gmeow:cqQueryFile` reference needs the repo root to resolve (this function
/// only sees one slice's parsed store), so that half is completed afterwards by
/// `apply_competency_query_text` in [`DocsModel::discover`].
fn extract_competency(store: &Store, owner_slice: &str) -> Vec<DocCompetency> {
    let mut out = Vec::new();
    for cq in subjects_of_type(store, GMEOW_COMPETENCY_QUESTION) {
        let rationale = first_literal(store, &cq, GMEOW_CQ_RATIONALE);
        let query_file = first_literal(store, &cq, GMEOW_CQ_QUERY_FILE);
        // No authored CQ carries both `cqQuery` and `cqQueryFile` (verified
        // across all `slices/*/*/tests/competency.ttl`), so filling `query_text`
        // from the inline literal here never collides with the file-based
        // resolution `apply_competency_query_text` performs afterwards.
        let query_text = first_literal(store, &cq, GMEOW_CQ_QUERY);
        let exact_rows = first_literal(store, &cq, GMEOW_CQ_EXACT_ROWS).map(|v| v == "true");
        let expected_row_count = first_literal(store, &cq, GMEOW_CQ_EXPECT_ROW_COUNT).map(|v| {
            v.parse::<i64>().unwrap_or_else(|e| {
                panic!(
                    "competency question <{cq}> gmeow:cqExpectRowCount {v:?} is not a valid \
                     xsd:integer: {e}"
                )
            })
        });
        let mut exercises: Vec<String> = Vec::new();
        // `named_objects` returns rows sorted/deduped by row-subject IRI, which is
        // already deterministic — no further row-level sort is needed.
        let mut expected_rows: Vec<DocExpectedRow> = Vec::new();
        for row in named_objects(store, &cq, GMEOW_CQ_EXPECT_ROW) {
            let mut cells: Vec<DocExpectedCell> = Vec::new();
            for cell in blank_objects(store, &row, GMEOW_ROW_CELL) {
                let cell_node = Node::Blank(cell);
                let var = store.first_literal_of(&cell_node, GMEOW_CELL_VAR);
                let value_iri = first_named_of_node(store, &cell_node, GMEOW_CELL_VALUE_IRI);
                let value_literal = store.first_literal_of(&cell_node, GMEOW_CELL_VALUE_LITERAL);
                if let Some(v) = &value_iri {
                    exercises.push(v.clone());
                }
                cells.push(DocExpectedCell {
                    var,
                    value_iri,
                    value_literal,
                });
            }
            // Cell blank-node discovery order is not itself meaningful; sort by
            // content for deterministic column order in the rendered table.
            cells.sort();
            expected_rows.push(DocExpectedRow { cells });
        }
        exercises.sort();
        exercises.dedup();
        out.push(DocCompetency {
            iri: cq,
            rationale,
            query_file,
            query_text,
            exact_rows,
            expected_row_count,
            expected_rows,
            exercises,
            owner_slice: owner_slice.to_string(),
        });
    }
    out
}

/// The lowest-sorted named-node object of `<node> <pred> ?o`, or `None` — the
/// blank-node-subject twin of a named-object read, used for per-cell
/// `gmeow:cellValueIri` lookups where the cell is itself a blank node.
fn first_named_of_node(store: &Store, node: &Node, pred: &str) -> Option<String> {
    store
        .objects_of_node(node, pred)
        .into_iter()
        .filter_map(|o| match o {
            Object::Named(v) => Some(v),
            _ => None,
        })
        .min()
}

/// All blank-node object labels of `subject predicate ?o` in the default graph.
fn blank_objects(store: &Store, subject: &str, predicate: &str) -> Vec<String> {
    store.blank_objects(subject, predicate)
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

    // Native alignment cells (the legacy `gmeow:TermEquivalence` + `alignSubject/Predicate/
    // Object` cell form was deleted): read through the canonical `equivalence_cells` reader,
    // which resolves the reified `S P O {| … |}` statements + their reifier annotations.
    // Fail closed: a malformed alignment cell is a hard error. Swallowing the reader error
    // here would silently drop *every* mapping linkage from the docs output for this slice.
    let mut links = Vec::new();
    for cell in gmeow_logic_compile::projections::sssom::equivalence_cells(
        &gmeow_logic_compile::ingest::DslView::new(store.dataset()),
    )
    .expect("alignment cell extraction must not fail while building docs mapping linkages")
    {
        links.push(DocLinkage {
            mapping_set: Some(cell.sssom_file.clone()),
            subject_curie: to_curie(&cell.subject),
            predicate: to_curie(&cell.predicate),
            object: cell.obj.clone(),
            justification: cell.justification.as_deref().map(to_curie),
            confidence: cell.confidence,
            owner_slice: owner_slice.to_string(),
            subject: cell.subject,
        });
    }
    (sets, links)
}

/// True when `slice_iri` is typed `gmeow:GroundingSlice` in its own already-
/// parsed manifest `store` — the machine-checked signal (replacing the
/// `slices/grounding/*` directory-path convention) that a manifest may carry
/// `gmeow:Seam` individuals.
fn is_grounding_slice(store: &Store, slice_iri: &str) -> bool {
    subjects_of_type(store, GMEOW_GROUNDING_SLICE)
        .iter()
        .any(|iri| iri == slice_iri)
}

/// Extract every `gmeow:Seam` individual from an already-parsed manifest
/// `store`. Generic over ANY grounding slice's manifest (gated by
/// [`is_grounding_slice`] at the call site in [`DocsModel::from_catalog`]) — a
/// future seam authored in `lang:`/`math:` is picked up without a code change.
fn extract_seams(store: &Store) -> Vec<DocSeam> {
    let mut seams = Vec::new();
    for iri in subjects_of_type(store, GMEOW_SEAM) {
        let label = first_literal(store, &iri, RDFS_LABEL);
        let definition = first_literal(store, &iri, SKOS_DEFINITION)
            .or_else(|| first_literal(store, &iri, RDFS_COMMENT));

        let mut directions: Vec<DocSeamDirection> =
            blank_objects(store, &iri, GMEOW_SEAM_DIRECTION)
                .into_iter()
                .filter_map(|blank| {
                    let node = Node::Blank(blank);
                    let from = first_named_of_node(store, &node, GMEOW_SEAM_FROM_SLICE)?;
                    let to = first_named_of_node(store, &node, GMEOW_SEAM_TO_SLICE)?;
                    Some(DocSeamDirection { from, to })
                })
                .collect();
        directions.sort();
        directions.dedup();

        let mut carrying_terms = named_objects(store, &iri, GMEOW_SEAM_CARRYING_TERM);
        carrying_terms.sort();
        carrying_terms.dedup();

        let owning_docs = literals(store, &iri, GMEOW_SEAM_OWNING_DOC);

        seams.push(DocSeam {
            iri,
            label,
            definition,
            directions,
            carrying_terms,
            owning_docs,
        });
    }
    seams
}

/// Extract a single example, carrying its Turtle source in full. Parses the
/// artifact itself; use [`extract_example_from`] when a store is already
/// parsed (the `examples/*.ttl` discovery loop reuses one parse for both
/// [`DocExample`] and [`DocLossTarget`] extraction).
fn extract_example(artifact: &ArtifactRecord, owner_slice: &str) -> DocExample {
    let parsed = parse_turtle_lenient(&artifact.content).ok();
    extract_example_from(artifact, owner_slice, parsed.as_ref())
}

/// Extract a single example from an already-parsed `store` (or `None` when
/// the artifact failed to parse), carrying its Turtle source in full.
fn extract_example_from(
    artifact: &ArtifactRecord,
    owner_slice: &str,
    parsed: Option<&Store>,
) -> DocExample {
    let text = String::from_utf8_lossy(&artifact.content).into_owned();
    let logical_path = artifact.logical_path.clone();

    // Title: lexically-lowest rdfs:label literal on any subject, else the stem.
    let title = parsed
        .and_then(|store| {
            let mut labels: Vec<String> = Vec::new();
            store.for_each_quad(|_s, p, o| {
                if p == RDFS_LABEL
                    && let Object::Literal(value) = o
                {
                    labels.push(value.clone());
                }
            });
            labels.into_iter().min()
        })
        .unwrap_or_else(|| filename_title(&logical_path));

    // Terms referenced: every GMEOW-family CURIE (`gmeow:` + the three grounding
    // vocabularies `logic:` / `math:` / `lang:`) appearing as a NamedNode
    // anywhere. Collecting the grounding namespaces — not just `gmeow:` — is what
    // lets a grounding slice's conformance fixtures and worked examples light up
    // `dimFixturePair` / `dimWorkedInstance` / `dimTestReach` for the grounding
    // vocabulary terms keyed to those `math:` / `logic:` / `lang:` CURIEs.
    let mut terms_referenced: Vec<String> = parsed
        .map(|store| {
            let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            store.for_each_quad(|s, _p, o| {
                if let Some(curie) = s.as_named().and_then(family_curie) {
                    set.insert(curie);
                }
                if let Object::Named(iri) = o
                    && let Some(curie) = family_curie(iri)
                {
                    set.insert(curie);
                }
            });
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

/// Extract every authored projection-loss-ledger row from an already-parsed
/// example `store`: ANY subject carrying BOTH `logic:preservationKind` and
/// `logic:complexityClass` is a worked preservation example.
/// Generic across every `examples/*.ttl` artifact in every slice — NOT
/// special-cased to `projection-loss-ledger.ttl` — so any future authored
/// example declaring a loss row the same way is picked up automatically. A
/// subject typed `gmeow:InformationObject` that carries only ONE of the two
/// predicates (like this file's `ex:mortalityRuleSet` / `ex:derivation-
/// socratesMortal`, which are unrelated pedagogical individuals) is correctly
/// skipped — it is not a loss-ledger row.
fn extract_loss_targets(store: &Store, owner_slice: &str) -> Vec<DocLossTarget> {
    let mut out = Vec::new();
    for subject in subjects_with_predicate(store, LOGIC_PRESERVATION_KIND) {
        // Deterministic even if a subject somehow declared more than one
        // `logic:preservationKind` object: the lexically-lowest IRI wins.
        let Some(kind_iri) = named_objects(store, &subject, LOGIC_PRESERVATION_KIND)
            .into_iter()
            .min()
        else {
            continue;
        };
        let Some(complexity_class) = first_literal(store, &subject, LOGIC_COMPLEXITY_CLASS) else {
            // Carries `preservationKind` but no `complexityClass` — not a
            // loss-ledger row by this surface's definition (both required).
            continue;
        };
        let label = first_literal(store, &subject, RDFS_LABEL);
        out.push(DocLossTarget {
            target: local_name(&subject).to_string(),
            label,
            preservation_kind: local_name(&kind_iri).to_string(),
            complexity_class,
            slice: owner_slice.to_string(),
        });
    }
    out
}

/// Extract every worked math instance from an already-parsed example `store`:
/// ANY subject carrying `math:hasDimension`. Generic across every
/// `examples/*.ttl` artifact in every slice — NOT special-cased to
/// `measure-and-dimension.ttl` — so any future authored example declaring a
/// dimensioned quantity the same way is picked up automatically. A dimension
/// object with no `math:baseDimensionExponent` breakdown (e.g.
/// `math:dimensionless`) yields an honest empty [`DocDimExponent`] vector, not
/// a hard fail — a dimensionless quantity is a real, well-formed zero-exponent
/// case, not a data error.
fn extract_worked_instances(
    store: &Store,
    artifact: &ArtifactRecord,
    owner_slice: &str,
) -> Vec<DocWorkedInstance> {
    let mut out = Vec::new();
    for subject in subjects_with_predicate(store, MATH_HAS_DIMENSION) {
        // Deterministic even if a subject somehow declared more than one
        // `math:hasDimension` object: the lexically-lowest IRI wins.
        let Some(dimension_iri) = named_objects(store, &subject, MATH_HAS_DIMENSION)
            .into_iter()
            .min()
        else {
            continue;
        };

        let mut type_iris = named_objects(store, &subject, RDF_TYPE);
        type_iris.sort();
        type_iris.dedup();
        let types: Vec<String> = type_iris
            .iter()
            .map(|t| local_name(t).to_string())
            .collect();

        let label = first_literal(store, &subject, RDFS_LABEL);
        let dimension_label = first_literal(store, &dimension_iri, RDFS_LABEL);

        // The ℚ⁷ SI base-dimension exponent vector: empty when the dimension
        // object carries no `math:baseDimensionExponent` breakdown (the
        // dimensionless case) — an honest zero-exponent case, not a hard fail.
        // Exponent individuals are always named in the authored data (never
        // blank nodes), so the default-graph named-object walk is exhaustive.
        let mut dimension_exponents: Vec<DocDimExponent> = Vec::new();
        for exponent in named_objects(store, &dimension_iri, MATH_BASE_DIMENSION_EXPONENT) {
            let Some(base_iri) = named_objects(store, &exponent, MATH_EXPONENT_OF_DIMENSION)
                .into_iter()
                .min()
            else {
                continue;
            };
            let Some(numerator) = first_literal(store, &exponent, MATH_EXPONENT_NUMERATOR)
                .and_then(|v| v.parse::<i64>().ok())
            else {
                continue;
            };
            let Some(denominator) = first_literal(store, &exponent, MATH_EXPONENT_DENOMINATOR)
                .and_then(|v| v.parse::<i64>().ok())
            else {
                continue;
            };
            dimension_exponents.push(DocDimExponent {
                base_dimension: local_name(&base_iri).to_string(),
                numerator,
                denominator,
            });
        }
        dimension_exponents.sort();
        dimension_exponents.dedup();

        let unit = named_objects(store, &subject, GMEOW_UNIT).into_iter().min();
        let quantity_value = first_literal(store, &subject, MATH_QUANTITY_VALUE);

        let turtle = render_worked_instance_turtle(
            &subject,
            &type_iris,
            label.as_deref(),
            &dimension_iri,
            dimension_label.as_deref(),
            &dimension_exponents,
            unit.as_deref(),
            quantity_value.as_deref(),
        );

        out.push(DocWorkedInstance {
            slice: owner_slice.to_string(),
            logical_path: artifact.logical_path.clone(),
            subject: local_name(&subject).to_string(),
            types,
            label,
            dimension_label,
            dimension_exponents,
            unit,
            quantity_value,
            turtle,
        });
    }
    out
}

/// Render an IRI for the reconstructed worked-instance Turtle block: a CURIE
/// for a handful of well-known vocabulary namespaces (`rdf:`, `rdfs:`,
/// `math:`, `gmeow:`), a full `<...>` IRI otherwise. The example's own subject
/// / dimension IRIs (an `ex:`-style namespace whose prefix binding is
/// per-file, not carried by the extracted model) and any external IRI (a QUDT
/// unit) fall through to the `<...>` form — always syntactically valid
/// Turtle, regardless of which example file the instance was parsed from.
fn turtle_iri(iri: &str) -> String {
    const PREFIXES: &[(&str, &str)] = &[
        ("http://www.w3.org/1999/02/22-rdf-syntax-ns#", "rdf"),
        ("http://www.w3.org/2000/01/rdf-schema#", "rdfs"),
        (MATH_NS, "math"),
        (GMEOW_NS, "gmeow"),
    ];
    for (ns, prefix) in PREFIXES {
        if let Some(local) = iri.strip_prefix(ns)
            // A Turtle PN_LOCAL cannot contain an unescaped `/` or `#` — an
            // example's own subject namespace (e.g.
            // `https://blackcatinformatics.ca/gmeow/examples/math/`) is
            // itself nested UNDER `GMEOW_NS`, so a naive prefix strip would
            // mint an invalid CURIE like `gmeow:examples/math/energyDensityFn`.
            // Falling through to the full `<...>` form for any such nested
            // namespace keeps every rendered CURIE syntactically valid.
            && !local.is_empty()
            && !local.contains(['/', '#'])
        {
            return format!("{prefix}:{local}");
        }
    }
    format!("<{iri}>")
}

/// Render `value` as a Turtle short-form string literal (backslash/quote/
/// control-character escaped).
fn turtle_string_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Render a subject's predicate list (each entry already a complete
/// `predicate object` clause, no trailing punctuation) as
/// `    predicate object ;\n` … `    predicate object .\n` — the project's own
/// 4-space-indent, trailing-`.` Turtle authoring convention.
fn push_turtle_predicate_lines(out: &mut String, lines: &[String]) {
    for (i, line) in lines.iter().enumerate() {
        let terminator = if i + 1 == lines.len() { '.' } else { ';' };
        out.push_str("    ");
        out.push_str(line);
        out.push(' ');
        out.push(terminator);
        out.push('\n');
    }
}

/// Reconstruct a small, deterministic, copy-paste-runnable Turtle block for
/// one worked instance FROM ITS EXTRACTED FIELDS — never a byte-slice of the
/// source artifact. A byte-slice risks incompleteness (the source file may
/// interleave unrelated subjects between the instance and its dimension) and
/// non-determinism (comment/whitespace drift is not part of the model); hand
/// -rendering from the same typed fields the model already carries is no
/// harder to get right and stays exactly as informative as the model itself.
/// Renders the subject's own triples, then — only when
/// `dimension_exponents` is non-empty — a second stanza for the resolved
/// `math:DerivedDimension`'s `math:baseDimensionExponent` breakdown (each
/// exponent as an anonymous `[ … ]` blank-node object, matching the project's
/// own authoring convention in `measure-and-dimension.ttl`). The dimensionless
/// case (empty `dimension_exponents`) renders only the subject stanza — honest,
/// not a fabricated breakdown.
#[allow(clippy::too_many_arguments)]
fn render_worked_instance_turtle(
    subject_iri: &str,
    type_iris: &[String],
    label: Option<&str>,
    dimension_iri: &str,
    dimension_label: Option<&str>,
    dimension_exponents: &[DocDimExponent],
    unit: Option<&str>,
    quantity_value: Option<&str>,
) -> String {
    let mut out = String::new();

    // ── The dimensioned subject's own triples ──────────────────────────────
    out.push_str(&turtle_iri(subject_iri));
    out.push('\n');
    let mut lines: Vec<String> = Vec::new();
    for type_iri in type_iris {
        lines.push(format!("a {}", turtle_iri(type_iri)));
    }
    if let Some(label) = label {
        lines.push(format!("rdfs:label {}", turtle_string_literal(label)));
    }
    lines.push(format!("math:hasDimension {}", turtle_iri(dimension_iri)));
    if let Some(unit) = unit {
        lines.push(format!("gmeow:unit {}", turtle_iri(unit)));
    }
    if let Some(value) = quantity_value {
        lines.push(format!(
            "math:quantityValue {}^^xsd:double",
            turtle_string_literal(value)
        ));
    }
    push_turtle_predicate_lines(&mut out, &lines);

    // ── The resolved dimension's own base-dimension-exponent breakdown ─────
    if !dimension_exponents.is_empty() {
        out.push('\n');
        out.push_str(&turtle_iri(dimension_iri));
        out.push('\n');
        let mut dim_lines: Vec<String> = vec!["a math:DerivedDimension".to_string()];
        if let Some(label) = dimension_label {
            dim_lines.push(format!("rdfs:label {}", turtle_string_literal(label)));
        }
        let exponent_blanks: Vec<String> = dimension_exponents
            .iter()
            .map(|e| {
                format!(
                    "[ math:exponentOfDimension math:{} ; math:exponentNumerator {} ; \
                     math:exponentDenominator {} ]",
                    e.base_dimension, e.numerator, e.denominator
                )
            })
            .collect();
        dim_lines.push(format!(
            "math:baseDimensionExponent {}",
            exponent_blanks.join(" ,\n        ")
        ));
        push_turtle_predicate_lines(&mut out, &dim_lines);
    }

    out
}

/// One `gmeow:ExampleConformance` binding read from a slice's
/// `tests/example-conformance.ttl`: the expected outcome / violation code /
/// rationale it asserts for the fixture path its `gmeow:exampleFile` pins.
struct FixtureBinding {
    /// `gmeow:expectedOutcome`'s local name (`"conforms"` | `"violates"`).
    expected_outcome: Option<String>,
    /// `gmeow:expectedViolationCode`.
    violation_code: Option<String>,
    /// `gmeow:conformanceRationale`.
    rationale: Option<String>,
}

/// Extract the per-fixture-path conformance bindings from a slice's
/// `tests/example-conformance.ttl` store, keyed by the slice-relative
/// `gmeow:exampleFile` path each `gmeow:ExampleConformance` cell pins.
fn extract_fixture_bindings(store: &Store) -> BTreeMap<String, FixtureBinding> {
    let mut out = BTreeMap::new();
    for cell in subjects_of_type(store, GMEOW_EXAMPLE_CONFORMANCE) {
        let Some(file) = first_literal(store, &cell, GMEOW_EXAMPLE_FILE) else {
            continue;
        };
        // The lowest-sorted object IRI's local name — deterministic even if a
        // cell were ever multiply asserted.
        let expected_outcome = named_objects(store, &cell, GMEOW_EXPECTED_OUTCOME)
            .into_iter()
            .min()
            .map(|iri| local_name(&iri).to_string());
        let violation_code = first_literal(store, &cell, GMEOW_EXPECTED_VIOLATION_CODE);
        let rationale = first_literal(store, &cell, GMEOW_CONFORMANCE_RATIONALE);
        out.insert(
            file,
            FixtureBinding {
                expected_outcome,
                violation_code,
                rationale,
            },
        );
    }
    out
}

/// Extract a single conformance fixture, reusing [`extract_example`]'s title /
/// text / term-reference extraction verbatim (the fixture files are structured
/// identically to worked examples) and joining `binding` — this slice's
/// `tests/example-conformance.ttl` entry for this fixture's path, if any.
/// `catalog_slug` starts `None`; [`apply_fixture_catalog_slugs`] resolves it
/// once `constraint_rules` is populated in `discover()`.
fn extract_fixture(
    artifact: &ArtifactRecord,
    owner_slice: &str,
    kind: DocFixtureKind,
    binding: Option<&FixtureBinding>,
) -> DocFixture {
    let example = extract_example(artifact, owner_slice);
    let (expected_outcome, violation_code, rationale) = match binding {
        Some(b) => (
            b.expected_outcome.clone(),
            b.violation_code.clone(),
            b.rationale.clone(),
        ),
        None => (None, None, None),
    };
    DocFixture {
        slice: example.slice,
        logical_path: example.logical_path,
        title: example.title,
        text: example.text,
        kind,
        terms_referenced: example.terms_referenced,
        expected_outcome,
        violation_code,
        rationale,
        catalog_slug: None,
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

/// Extract a single [`DocGrammar`] from a `grammars/*.ebnf` artifact: the slug
/// is the filename stem, the title is the header comment's first sentence
/// (see [`DocGrammar::title`]), and the license is the file's
/// `SPDX-License-Identifier` header line. Every grammar file in this
/// repository is required to carry both a descriptive header and an SPDX
/// license line — their absence is a broken authoring invariant, not an
/// optional input, so a malformed header is a hard fail rather than a silent
/// placeholder.
fn extract_grammar(artifact: &ArtifactRecord) -> DocGrammar {
    let source = String::from_utf8_lossy(&artifact.content).into_owned();
    let filename = artifact
        .logical_path
        .rsplit('/')
        .next()
        .unwrap_or(&artifact.logical_path);
    // `strip_suffix` removes the extension exactly once (never mid-name characters,
    // unlike `trim_end_matches`), falling back to the bare filename when absent.
    let slug = filename
        .strip_suffix(".ebnf")
        .unwrap_or(filename)
        .to_string();
    let title = grammar_title(&source).unwrap_or_else(|| {
        panic!(
            "{}: missing a `#`-commented header description to derive a title from",
            artifact.logical_path
        )
    });
    let license = grammar_license(&source).unwrap_or_else(|| {
        panic!(
            "{}: missing an `SPDX-License-Identifier:` header line",
            artifact.logical_path
        )
    });
    DocGrammar {
        slug,
        title,
        source,
        license,
    }
}

/// Join a grammar file's leading `#`-commented header lines (skipping the
/// blank comment separators) into one string, then take the first sentence
/// (up to the first `". "`, or the whole joined string when it contains no
/// internal sentence break) as the title. Stops at the first line that is not
/// a `#` comment (the header always precedes the first production line).
/// `None` when the file carries no descriptive header comment at all.
fn grammar_title(source: &str) -> Option<String> {
    let mut body = String::new();
    for raw_line in source.lines() {
        let Some(rest) = raw_line.trim_start().strip_prefix('#') else {
            break;
        };
        let content = rest.trim();
        if content.is_empty() || content.starts_with("SPDX-") {
            continue;
        }
        if !body.is_empty() {
            body.push(' ');
        }
        body.push_str(content);
    }
    if body.is_empty() {
        return None;
    }
    let sentence = body.split(". ").next().unwrap_or(&body);
    Some(sentence.trim_end_matches('.').to_string())
}

/// Parse a grammar file's `SPDX-License-Identifier:` header comment line.
fn grammar_license(source: &str) -> Option<String> {
    source.lines().find_map(|raw_line| {
        let content = raw_line.trim_start().trim_start_matches('#').trim();
        content
            .strip_prefix("SPDX-License-Identifier:")
            .map(|v| v.trim().to_string())
    })
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
            for (subject, object) in store.pattern_subjects_objects(GMEOW_DOCS_CONCERN) {
                let (Node::Named(subject), Object::Named(concern)) = (subject, object) else {
                    continue;
                };
                if subject.starts_with(GMEOW_NS) {
                    concern_terms
                        .entry(concern.clone())
                        .or_default()
                        .insert(to_curie(&subject));
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

/// Extract the dogfooded build-pipeline DAG from every module graph that carries
/// `gmeow:PipelineStage` individuals (the pipeline slice today). Scans module
/// stores exactly as [`extract_concerns`] does — a source-lane read of authored
/// `module.ttl`, never a `generated/` artifact. Returns `None` when no module
/// authors a stage (a bare unit-test catalog); the whole-repo `discover` path
/// always finds `slices/core/pipeline/module.ttl` and populates it.
///
/// Edges are the union of bare `gmeow:dataflowConsumes` dependencies (added with
/// no flow entities) and reified `gmeow:BuildDataFlow` refinements (whose
/// `gmeow:flowEntity` named-graph IRIs decorate the matching edge). A
/// `BuildDataFlow` whose `(from, to)` names no bare consumes edge still yields an
/// edge — it is a genuine authored dependency.
fn extract_pipeline(catalog: &SliceCatalog) -> Option<DocPipeline> {
    let mut stages: Vec<DocStage> = Vec::new();
    let mut seen_stage: BTreeSet<String> = BTreeSet::new();
    // (producer, consumer) → flowing named-graph IRIs (BTreeSet keeps them sorted
    // and the outer BTreeMap keeps the edge order deterministic).
    let mut edges: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    let mut goal: Option<String> = None;
    let mut success_mode: Option<String> = None;

    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role != ArtifactRole::Module {
                continue;
            }
            let Ok(store) = parse_turtle_lenient(&artifact.content) else {
                continue;
            };

            // Stages + their bare dataflowConsumes edges.
            for iri in subjects_of_type(&store, GMEOW_PIPELINE_STAGE) {
                if !seen_stage.insert(iri.clone()) {
                    continue;
                }
                let mut consumes = named_objects(&store, &iri, GMEOW_DATAFLOW_CONSUMES);
                consumes.sort();
                consumes.dedup();
                let mut attaches_graphs = named_objects(&store, &iri, GMEOW_ATTACHES_GRAPH);
                attaches_graphs.sort();
                attaches_graphs.dedup();
                let mut attaches_blob_reps = literals(&store, &iri, GMEOW_ATTACHES_BLOB_REP);
                attaches_blob_reps.sort();
                attaches_blob_reps.dedup();
                let capabilities = curie_objects(&store, &iri, GMEOW_HAS_CAPABILITY);
                let resources = curie_objects(&store, &iri, GMEOW_REQUIRES_RESOURCE);
                // The lowest-sorted box-role CURIE (mirrors the per-term surface).
                let box_role = curie_objects(&store, &iri, GMEOW_GRAPH_BOX_ROLE)
                    .into_iter()
                    .next();
                for producer in &consumes {
                    edges.entry((producer.clone(), iri.clone())).or_default();
                }
                stages.push(DocStage {
                    label: first_literal(&store, &iri, RDFS_LABEL),
                    definition: first_literal(&store, &iri, SKOS_DEFINITION),
                    stage_impl: first_literal(&store, &iri, GMEOW_STAGE_IMPL),
                    capabilities,
                    resources,
                    box_role,
                    consumes,
                    attaches_graphs,
                    attaches_blob_reps,
                    iri,
                });
            }

            // Reified BuildDataFlow edges: decorate the matching edge with the
            // flowing named-graph IRIs (honest computed-absence where none exist).
            for edge_iri in subjects_of_type(&store, GMEOW_BUILD_DATA_FLOW) {
                let from = named_objects(&store, &edge_iri, GMEOW_BUILD_FLOW_FROM)
                    .into_iter()
                    .next();
                let to = named_objects(&store, &edge_iri, GMEOW_BUILD_FLOW_TO)
                    .into_iter()
                    .next();
                let (Some(from), Some(to)) = (from, to) else {
                    continue;
                };
                let entry = edges.entry((from, to)).or_default();
                for graph in named_objects(&store, &edge_iri, GMEOW_FLOW_ENTITY) {
                    entry.insert(graph);
                }
            }

            // The Pipeline plan's goal + success mode.
            for pipeline_iri in subjects_of_type(&store, GMEOW_PIPELINE) {
                if goal.is_none() {
                    goal = named_objects(&store, &pipeline_iri, LOGIC_PLAN_GOAL)
                        .into_iter()
                        .next()
                        .map(|iri| to_curie(&iri));
                }
                if success_mode.is_none() {
                    success_mode = named_objects(&store, &pipeline_iri, LOGIC_PLAN_SUCCESS_MODE)
                        .into_iter()
                        .next()
                        .map(|iri| to_curie(&iri));
                }
            }
        }
    }

    if stages.is_empty() {
        return None;
    }
    stages.sort_by(|a, b| a.iri.cmp(&b.iri));
    let edges: Vec<DocFlowEdge> = edges
        .into_iter()
        .map(|((from, to), flow)| DocFlowEdge {
            from,
            to,
            flow_entities: flow.into_iter().collect(),
        })
        .collect();
    Some(DocPipeline {
        stages,
        edges,
        goal,
        success_mode,
    })
}

/// Derive the external-term overview: every GENUINELY-external IRI referenced by
/// a linkage object or by a term's parents / domain / range, grouped by
/// namespace. An IRI that is itself a documented term (a `gmeow:` term, or a
/// grounding-vocabulary `math:` / `logic:` / `lang:` term this projection now
/// documents) is NOT external — it resolves to its own page — so it is excluded.
fn extract_external_terms(terms: &[DocTerm], linkages: &[DocLinkage]) -> Vec<DocExternalTerm> {
    let documented: std::collections::HashSet<&str> =
        terms.iter().map(|t| t.iri.as_str()).collect();
    // external IRI → (referencing gmeow curies, predicates)
    let mut by_iri: BTreeMap<
        String,
        (
            std::collections::BTreeSet<String>,
            std::collections::BTreeSet<String>,
        ),
    > = BTreeMap::new();

    let mut record = |iri: &str, by: &str, via: &str| {
        if iri.starts_with(GMEOW_NS) || !is_external_iri(iri) || documented.contains(iri) {
            return;
        }
        let entry = by_iri.entry(iri.to_string()).or_default();
        entry.0.insert(by.to_string());
        entry.1.insert(via.to_string());
    };

    for link in linkages {
        record(&link.object, &link.subject_curie, "matchObject");
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
    store.sorted_literals(subject, predicate)
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
    store.subjects_of_type(type_iri)
}

/// Every distinct named subject carrying `?s <predicate> ?o` in the default
/// graph (sorted, deduped).
fn subjects_with_predicate(store: &Store, predicate: &str) -> Vec<String> {
    store.subjects_with_predicate(predicate)
}

/// Map an `rdf:type` object IRI to a documented term category.
///
/// The authored/bundle store types a term in the canonical `logic:` spelling
/// (`logic:Class`, not `owl:Class`, after the `logic:`→`owl:` surface flip);
/// `gmeow_ns::to_owl_view` lowers a typing marker to its OWL view so the arms
/// keyed on the `owl:` constants match both spellings. A non-marker IRI (e.g.
/// `logic:PathShape`, `gmeow:PipelineStage`) passes through unchanged.
fn category_for_type(type_iri: &str) -> Option<DocTermCategory> {
    match gmeow_ns::to_owl_view(type_iri) {
        OWL_CLASS | RDFS_CLASS => Some(DocTermCategory::Class),
        OWL_OBJECT_PROPERTY | OWL_DATATYPE_PROPERTY | OWL_ANNOTATION_PROPERTY | RDF_PROPERTY => {
            Some(DocTermCategory::Property)
        }
        OWL_NAMED_INDIVIDUAL => Some(DocTermCategory::Individual),
        RDFS_DATATYPE => Some(DocTermCategory::Datatype),
        // A `logic:PathShape` INSTANCE is an OWL individual (an instance of the
        // `logic:PathShape` class), not a TBox class/property/datatype — so
        // `Individual` is its definitionally-honest category. Its low
        // `category_rank` (Individual = 1) is deliberate: a domain property that
        // is ALSO grounded as a PathShape keeps its stronger `Property` category.
        LOGIC_PATH_SHAPE => Some(DocTermCategory::Individual),
        // A `gmeow:PipelineStage` INSTANCE (each `gmeow:stage-*` node of the
        // dogfooded build DAG, authored in `slices/core/pipeline/module.ttl`) is
        // an OWL individual, so `Individual` is its honest category — and, unlike
        // `logic:PathShape`, its instances live in a `module.ttl`, so the
        // module-wide `extract_terms` scan lifts them here directly (no separate
        // example scan). Making each stage a documented term is what gives it a
        // term page, which is the surface `render::append_stage_section` enriches
        // with the stage's Rust `stageImpl` binding and consumes / consumed-by /
        // flowing-graph dataflow tables read back from `DocPipeline`.
        GMEOW_PIPELINE_STAGE => Some(DocTermCategory::Individual),
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

/// The first literal value for `subject predicate ?o` (deterministic: lowest
/// lexical form), or `None`.
fn first_literal(store: &Store, subject: &str, predicate: &str) -> Option<String> {
    store.first_literal(subject, predicate)
}

/// All literal values for `subject predicate ?o`, sorted and deduped
/// (deterministic; carries the English-carrier text from `module.ttl`).
fn literals(store: &Store, subject: &str, predicate: &str) -> Vec<String> {
    store.literals(subject, predicate)
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
    store.named_objects(subject, predicate)
}

/// The GMEOW-family namespaces that abbreviate to a CURIE prefix: `gmeow:` plus
/// the three grounding vocabularies (`logic:` / `math:` / `lang:`) whose slices
/// own first-class documented terms. The namespaces are pairwise
/// non-overlapping (no one is a prefix of another), so ordering is immaterial.
const CURIE_NAMESPACES: &[(&str, &str)] = &[
    (GMEOW_NS, "gmeow"),
    (LOGIC_NS, "logic"),
    (MATH_NS, "math"),
    (LANG_NS, "lang"),
];

/// The compact CURIE for an IRI under a GMEOW-family namespace
/// ([`CURIE_NAMESPACES`]), or `None` for any other IRI. A family-nested example
/// namespace (e.g. `https://blackcatinformatics.ca/gmeow/examples/logic/nearbyOrgs`)
/// has a local part carrying `/` — an invalid Turtle PN_LOCAL — so it yields
/// `None` rather than a broken `gmeow:examples/logic/nearbyOrgs` CURIE (the same
/// invariant [`turtle_iri`] enforces). Shared by [`to_curie`] (which falls back
/// to the full IRI) and the corpora term-reference scan (which keeps ONLY
/// family CURIEs).
fn family_curie(iri: &str) -> Option<String> {
    CURIE_NAMESPACES.iter().find_map(|(ns, prefix)| {
        iri.strip_prefix(ns)
            .filter(|local| !local.is_empty() && !local.contains(['/', '#']))
            .map(|local| format!("{prefix}:{local}"))
    })
}

/// Compute the compact CURIE for an IRI: a `gmeow:` / `logic:` / `math:` /
/// `lang:` CURIE for a GMEOW-family-namespaced IRI, otherwise the IRI unchanged.
fn to_curie(iri: &str) -> String {
    family_curie(iri).unwrap_or_else(|| iri.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_from(ttl: &str) -> Store {
        parse_turtle_lenient(ttl.as_bytes()).expect("parse")
    }

    /// The `cqQueryFile` resolution boundary: repo-root-relative, `..`-free, and
    /// under one of the content-addressed roots. Anything else is a hard fail —
    /// see `apply_competency_query_text`.
    #[test]
    fn competency_query_paths_are_confined_to_the_hashed_roots() {
        assert!(is_competency_query_path("queries/competency/agents.rq"));
        assert!(is_competency_query_path(
            "slices/core/kernel/queries/competency/k.rq"
        ));
        // Outside the hashed roots: unhashed text would be served stale forever.
        assert!(!is_competency_query_path("dsl/competency/agents.rq"));
        assert!(!is_competency_query_path("generated/queries/agents.rq"));
        // Absolute, and `..` escapes that would pass a naive prefix test.
        assert!(!is_competency_query_path("/etc/passwd"));
        assert!(!is_competency_query_path("queries/../dsl/agents.rq"));
        assert!(!is_competency_query_path("slices/..\\dsl\\agents.rq"));
    }

    /// A `cqQueryFile` outside the boundary is an ERROR, not a silent skip and not
    /// a tolerated read — the model build fails and names the offending path.
    #[test]
    fn competency_query_outside_the_hashed_roots_hard_fails() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let root = tmp.path().join("gmeow-cq-root-test");
        std::fs::create_dir_all(root.join("dsl")).expect("mkdir");
        // The file EXISTS and is readable — only its location is illegal, so this
        // proves the boundary itself rejects, not a dangling-path fallback.
        std::fs::write(root.join("dsl/escape.rq"), b"SELECT * {}").expect("write");

        let mut model = DocsModel {
            competencies: vec![DocCompetency {
                iri: "https://blackcatinformatics.ca/gmeow/cq/escape".to_owned(),
                query_file: Some("dsl/escape.rq".to_owned()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let err = apply_competency_query_text(&mut model, &root)
            .expect_err("a cqQueryFile outside the hashed roots must hard-fail");
        let msg = err.to_string();
        assert!(
            msg.contains("dsl/escape.rq") && msg.contains("outside the resolution contract"),
            "the error must name the offending path and the contract, got: {msg}"
        );

        // The legal location, same bytes, resolves.
        std::fs::create_dir_all(root.join("queries/competency")).expect("mkdir");
        std::fs::write(root.join("queries/competency/ok.rq"), b"SELECT * {}").expect("write");
        model.competencies[0].query_file = Some("queries/competency/ok.rq".to_owned());
        apply_competency_query_text(&mut model, &root).expect("a query under queries/ resolves");
        assert_eq!(
            model.competencies[0].query_text.as_deref(),
            Some("SELECT * {}")
        );
    }

    #[test]
    fn thesis_sentence_detection_is_structural() {
        assert!(detect_thesis_sentence(
            "# Heading\n\nThis slice grounds the documentation standard in RDF."
        ));
        // Only headings / tables / lists — no prose sentence.
        assert!(!detect_thesis_sentence(
            "# Heading\n\n| a | b |\n| - | - |\n"
        ));
        assert!(!detect_thesis_sentence("- a bullet\n- another"));
        assert!(!detect_thesis_sentence(""));
    }

    #[test]
    fn realized_state_table_detection_requires_every_row_marked() {
        // A table with a "Realized state" column where every row carries a marker
        // (realized / design-only / partial / built) is complete.
        let complete = "\
| Document | Genre | Realized state | Contents |
| --- | --- | --- | --- |
| a.md | charter | realized | x |
| b.md | charter | **design-only** — nothing yet | y |
| c.md | charter | partial | z |
";
        assert!(detect_realized_state_complete(complete));

        // One row with an empty realized-state cell → incomplete.
        let holey = "\
| Document | Genre | Realized state | Contents |
| --- | --- | --- | --- |
| a.md | charter | realized | x |
| b.md | charter |  | y |
";
        assert!(!detect_realized_state_complete(holey));

        // No realized-state table at all → not complete (a gated miss).
        assert!(!detect_realized_state_complete(
            "| Rule | Shape |\n| - | - |\n| r | s |\n"
        ));
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

    /// A term whose taxonomy is authored in the CANONICAL `logic:` spelling —
    /// with no `rdfs:` edge anywhere — still renders its parents.
    ///
    /// This is the blinding regression: reading only `rdfs:subClassOf` /
    /// `rdfs:subPropertyOf` gave a re-authored term an EMPTY parent list, which
    /// silently emptied both its parent section and its `term_neighbourhood_svg`
    /// hierarchy diagram (which projects exactly `DocTerm::parents`).
    #[test]
    fn canonical_logic_subsumption_edges_are_parents() {
        let ttl = r#"
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:Animal a owl:Class ;
    rdfs:label "Animal" .

gmeow:Cat a owl:Class ;
    logic:subClassOf gmeow:Animal ;
    rdfs:label "Cat" .

gmeow:touches a owl:ObjectProperty ;
    rdfs:label "touches" .

gmeow:grooms a owl:ObjectProperty ;
    logic:subPropertyOf gmeow:touches ;
    rdfs:label "grooms" .
"#;
        let store = store_from(ttl);
        let terms = extract_terms(&store, "https://example.org/slice/zoo", None);

        let cat = terms.iter().find(|t| t.iri.ends_with("Cat")).unwrap();
        assert_eq!(
            cat.parents,
            vec![format!("{GMEOW_NS}Animal")],
            "a `logic:subClassOf` edge is a parent"
        );

        let grooms = terms.iter().find(|t| t.iri.ends_with("grooms")).unwrap();
        assert_eq!(
            grooms.parents,
            vec![format!("{GMEOW_NS}touches")],
            "a `logic:subPropertyOf` edge is a parent"
        );

        // The user-visible surface: the hierarchy diagram is non-empty.
        assert!(
            crate::svg::term_neighbourhood_svg(cat).contains("Animal"),
            "the neighbourhood diagram must draw the canonical parent edge"
        );
    }

    /// `read_central_mapping_sets` distinguishes an absent file (fine — slices
    /// carry their own sets) from a present-but-unparsable one (hard fail, never
    /// a silent empty that would drop every relocated linkage's `MappingSet`).
    #[test]
    fn central_mapping_sets_absent_ok_but_malformed_hard_fails() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let root = tmp.path().join("gmeow-mapsets-test");
        let dir = root.join("dsl").join("mappings");
        std::fs::create_dir_all(&dir).expect("mkdir");

        // Absent file ⇒ Ok(empty).
        assert!(
            read_central_mapping_sets(&root)
                .expect("absent mapping-sets.ttl must be Ok")
                .is_empty(),
            "an absent central mapping-sets.ttl yields no sets, not an error"
        );

        // Present but unparsable ⇒ hard fail (no silent empty).
        std::fs::write(
            dir.join("mapping-sets.ttl"),
            "this is @@@ definitely not { valid ] turtle <<< ;;;",
        )
        .expect("write malformed");
        let err = read_central_mapping_sets(&root)
            .expect_err("a malformed central mapping-sets.ttl must hard-fail, not return empty");
        assert!(matches!(err, DocsError::MappingSets(_)));
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

    /// Stability derivation precedence: explicit `gmeow:termStability`
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
    /// `(version, note)`; `addedInVersion` is the lowest literal.
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

    /// [`extract_loss_targets`] is generic: it finds every subject carrying
    /// BOTH `logic:preservationKind` and `logic:complexityClass` — not just a
    /// hardcoded filename's individuals — and correctly skips a subject that
    /// carries only one of the two predicates (an unrelated pedagogical
    /// individual, mirroring `ex:mortalityRuleSet` in the real
    /// `projection-loss-ledger.ttl`).
    #[test]
    fn extract_loss_targets_finds_rows_and_skips_partial_subjects() {
        let ttl = r#"
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/demo/> .

ex:notARow a gmeow:InformationObject ;
    rdfs:label "carries only preservationKind, not a loss row"@x-gmeow-english ;
    logic:preservationKind logic:ValidationOnly .

ex:elProjectionReport a gmeow:InformationObject ;
    rdfs:label "OWL-EL projection of the demo rule set"@x-gmeow-english ;
    logic:preservationKind logic:SoundUnderApproximation ;
    logic:complexityClass "EL -> PTIME" .
"#;
        let store = store_from(ttl);
        let rows = extract_loss_targets(&store, "https://example.org/slice/demo");
        assert_eq!(rows.len(), 1, "only the fully-attributed subject is a row");
        let row = &rows[0];
        assert_eq!(row.target, "elProjectionReport");
        assert_eq!(
            row.label.as_deref(),
            Some("OWL-EL projection of the demo rule set")
        );
        assert_eq!(row.preservation_kind, "SoundUnderApproximation");
        assert_eq!(row.complexity_class, "EL -> PTIME");
        assert_eq!(row.slice, "https://example.org/slice/demo");
    }

    #[test]
    fn extract_seams_reads_labels_directions_terms_and_docs() {
        let ttl = r#"
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix lang:  <https://blackcatinformatics.ca/lang/> .

<https://blackcatinformatics.ca/gmeow/slices/logic> a gmeow:Slice, gmeow:GroundingSlice .

<https://blackcatinformatics.ca/gmeow/seam/denotation>
    a gmeow:Seam ;
    rdfs:label "Denotation seam"@x-gmeow-english ;
    skos:definition "The lang -> logic seam."@x-gmeow-english ;
    gmeow:seamDirection [
        gmeow:seamFromSlice <https://blackcatinformatics.ca/gmeow/slices/lang> ;
        gmeow:seamToSlice <https://blackcatinformatics.ca/gmeow/slices/logic>
    ] ;
    gmeow:seamCarryingTerm lang:denotationTarget , lang:denotationKind ;
    gmeow:seamOwningDoc "LANG-MEANING.md" .
"#;
        let store = store_from(ttl);
        assert!(is_grounding_slice(
            &store,
            "https://blackcatinformatics.ca/gmeow/slices/logic"
        ));
        assert!(!is_grounding_slice(
            &store,
            "https://blackcatinformatics.ca/gmeow/slices/lang"
        ));

        let seams = extract_seams(&store);
        assert_eq!(seams.len(), 1);
        let seam = &seams[0];
        assert_eq!(
            seam.iri,
            "https://blackcatinformatics.ca/gmeow/seam/denotation"
        );
        assert_eq!(seam.label.as_deref(), Some("Denotation seam"));
        assert_eq!(seam.definition.as_deref(), Some("The lang -> logic seam."));
        assert_eq!(
            seam.directions,
            vec![DocSeamDirection {
                from: "https://blackcatinformatics.ca/gmeow/slices/lang".to_string(),
                to: "https://blackcatinformatics.ca/gmeow/slices/logic".to_string(),
            }]
        );
        assert_eq!(
            seam.carrying_terms,
            vec![
                "https://blackcatinformatics.ca/lang/denotationKind".to_string(),
                "https://blackcatinformatics.ca/lang/denotationTarget".to_string(),
            ]
        );
        assert_eq!(seam.owning_docs, vec!["LANG-MEANING.md".to_string()]);
    }

    /// Build a bare in-memory [`ArtifactRecord`] for a Turtle example, mirroring
    /// the shape [`extract_worked_instances`] reads (`logical_path` is the only
    /// field it consults; the rest are filler).
    fn example_artifact(logical_path: &str, ttl: &str) -> ArtifactRecord {
        ArtifactRecord {
            role: ArtifactRole::Example,
            logical_path: logical_path.to_string(),
            media_type: "text/turtle".to_string(),
            raw_digest: String::new(),
            semantic_digest: None,
            content: ttl.as_bytes().to_vec(),
        }
    }

    /// A bare in-memory `text/markdown` [`ArtifactRecord`] carrying `body`. Selected
    /// by media type (like a real `design/*.md`), so [`DocMarkdownDocument::collect`]
    /// treats it as a first-class document.
    fn markdown_artifact(logical_path: &str, body: &str) -> ArtifactRecord {
        ArtifactRecord {
            role: ArtifactRole::Other(logical_path.to_string()),
            logical_path: logical_path.to_string(),
            media_type: "text/markdown".to_string(),
            raw_digest: format!("digest-{logical_path}"),
            semantic_digest: None,
            content: body.as_bytes().to_vec(),
        }
    }

    /// A hand-built [`SliceRecord`] carrying only a set of artifacts — the minimum
    /// [`DocMarkdownDocument::collect`] reads (it consults `record.artifacts` only).
    /// The manifest graph is an empty frozen dataset; the manifest view is filler.
    fn record_with_artifacts(slice_iri: &str, artifacts: Vec<ArtifactRecord>) -> SliceRecord {
        SliceRecord {
            manifest: ManifestView {
                slice_iri: slice_iri.to_string(),
                label: None,
                title: None,
                creators: Vec::new(),
                identifier: None,
                tier: None,
                consumers: Vec::new(),
                profiles: Vec::new(),
                depends_on: Vec::new(),
            },
            manifest_graph: purrdf::RdfDatasetBuilder::new()
                .freeze()
                .expect("empty dataset freezes"),
            artifacts,
            slice_dir: std::path::PathBuf::from("/nonexistent/synthetic-slice"),
        }
    }

    /// Item 1 (model ordering): `collect` selects every `text/markdown` artifact,
    /// decodes it strictly, sorts by normalized logical path, and derives each
    /// title from its first ATX H1 — regardless of artifact input order or role.
    #[test]
    fn collect_orders_documents_and_derives_titles() {
        // Deliberately out of sorted order on input; `design/*.md` carries the open
        // `ArtifactRole::Other` role, exercising media-type (not role) selection.
        let record = record_with_artifacts(
            "https://blackcatinformatics.ca/gmeow/slices/zoo",
            vec![
                markdown_artifact("docs.md", "# Zoo Guide\n\nProse.\n"),
                markdown_artifact("design/ARCHITECTURE.md", "# Architecture\n\n## Overview\n"),
                // A non-markdown artifact is ignored.
                example_artifact("examples/x.ttl", "ex:a a ex:B ."),
            ],
        );
        let docs = DocMarkdownDocument::collect(
            &record,
            "https://blackcatinformatics.ca/gmeow/slices/zoo",
            "zoo",
        )
        .expect("collect succeeds");
        assert_eq!(docs.len(), 2, "only the two markdown sources");
        assert_eq!(docs[0].source_path, "design/ARCHITECTURE.md");
        assert_eq!(docs[0].title, "Architecture");
        assert_eq!(docs[1].source_path, "docs.md");
        assert_eq!(docs[1].title, "Zoo Guide");
        assert_eq!(docs[0].raw_digest, "digest-design/ARCHITECTURE.md");
    }

    /// Item 10a (hard-fail): an invalid-UTF-8 markdown artifact makes `collect`
    /// return `Err(MarkdownUtf8)` naming the offending source path — no lossy
    /// fallback.
    #[test]
    fn collect_hard_fails_on_invalid_utf8_naming_path() {
        let mut bad = markdown_artifact("design/BAD.md", "");
        bad.content = b"# X\n\xff\xfe\n".to_vec();
        let record =
            record_with_artifacts("https://blackcatinformatics.ca/gmeow/slices/zoo", vec![bad]);
        let err = DocMarkdownDocument::collect(
            &record,
            "https://blackcatinformatics.ca/gmeow/slices/zoo",
            "zoo",
        )
        .expect_err("invalid UTF-8 must hard-fail");
        match &err {
            DocsError::MarkdownUtf8 { source_path, .. } => {
                assert_eq!(source_path, "design/BAD.md");
            }
            other => panic!("expected MarkdownUtf8, got {other:?}"),
        }
        assert!(err.to_string().contains("design/BAD.md"));
    }

    /// Item 10b (hard-fail): two markdown artifacts whose logical paths NORMALIZE to
    /// the same logical path (`./design/A.md` vs `design/A.md`) make `collect` return
    /// `Err(MarkdownPathCollision)` naming the colliding path — one source can never
    /// silently shadow the other.
    #[test]
    fn collect_hard_fails_on_normalized_path_collision() {
        let record = record_with_artifacts(
            "https://blackcatinformatics.ca/gmeow/slices/zoo",
            vec![
                markdown_artifact("design/A.md", "# First\n"),
                // Distinct input path, identical after `./`-stripping normalization.
                markdown_artifact("./design/A.md", "# Second\n"),
            ],
        );
        let err = DocMarkdownDocument::collect(
            &record,
            "https://blackcatinformatics.ca/gmeow/slices/zoo",
            "zoo",
        )
        .expect_err("normalized-path collision must hard-fail");
        match &err {
            DocsError::MarkdownPathCollision { source_path, .. } => {
                assert_eq!(source_path, "design/A.md");
            }
            other => panic!("expected MarkdownPathCollision, got {other:?}"),
        }
        assert!(err.to_string().contains("design/A.md"));
    }

    /// [`extract_worked_instances`] is generic: it finds every subject carrying
    /// `math:hasDimension` — not just the individuals in the real
    /// `measure-and-dimension.ttl` — resolves a `math:DerivedDimension`'s ℚ⁷
    /// SI base-dimension exponent vector (sorted, negative numerators handled),
    /// and honestly emits an EMPTY exponent vector (not a hard fail) for a
    /// dimensionless subject whose dimension object carries no
    /// `math:baseDimensionExponent` breakdown.
    #[test]
    fn extract_worked_instances_resolves_exponents_and_honest_dimensionless() {
        let ttl = r#"
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix math:  <https://blackcatinformatics.ca/math/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/demo/> .

ex:restEnergy a math:Quantity ;
    rdfs:label "rest energy"@x-gmeow-english ;
    math:hasDimension ex:energyDimension ;
    gmeow:unit <http://qudt.org/vocab/unit/J> ;
    gmeow:hasReferenceFrame gmeow:referenceFrameSI ;
    math:quantityValue "8.187e-14"^^xsd:double .

ex:energyDimension a math:DerivedDimension ;
    rdfs:label "energy dimension (M*L^2*T^-2)"@x-gmeow-english ;
    math:baseDimensionExponent ex:eMass1 , ex:eTimeMinus2 .

ex:eMass1 a math:DimensionExponent ;
    math:exponentOfDimension math:massDimension ;
    math:exponentNumerator 1 ; math:exponentDenominator 1 .
ex:eTimeMinus2 a math:DimensionExponent ;
    math:exponentOfDimension math:timeDimension ;
    math:exponentNumerator -2 ; math:exponentDenominator 1 .

ex:uniformProbability a math:ProbabilityMeasure ;
    rdfs:label "uniform probability measure"@x-gmeow-english ;
    math:hasDimension math:dimensionless .
"#;
        let store = store_from(ttl);
        let artifact = example_artifact("examples/demo.ttl", ttl);
        let mut rows =
            extract_worked_instances(&store, &artifact, "https://example.org/slice/demo");
        rows.sort_by(|a, b| a.subject.cmp(&b.subject));
        assert_eq!(rows.len(), 2, "both dimensioned subjects are picked up");

        let rest_energy = &rows[0];
        assert_eq!(rest_energy.subject, "restEnergy");
        assert_eq!(rest_energy.logical_path, "examples/demo.ttl");
        assert_eq!(rest_energy.slice, "https://example.org/slice/demo");
        assert_eq!(rest_energy.types, vec!["Quantity".to_string()]);
        assert_eq!(rest_energy.label.as_deref(), Some("rest energy"));
        assert_eq!(
            rest_energy.dimension_label.as_deref(),
            Some("energy dimension (M*L^2*T^-2)")
        );
        assert_eq!(
            rest_energy.unit.as_deref(),
            Some("http://qudt.org/vocab/unit/J")
        );
        assert_eq!(rest_energy.quantity_value.as_deref(), Some("8.187e-14"));
        assert_eq!(
            rest_energy.dimension_exponents,
            vec![
                DocDimExponent {
                    base_dimension: "massDimension".to_string(),
                    numerator: 1,
                    denominator: 1,
                },
                DocDimExponent {
                    base_dimension: "timeDimension".to_string(),
                    numerator: -2,
                    denominator: 1,
                },
            ],
            "sorted by base_dimension local name; negative numerator preserved"
        );
        assert!(rest_energy.turtle.contains("math:exponentNumerator -2"));
        assert!(
            rest_energy
                .turtle
                .contains("gmeow:unit <http://qudt.org/vocab/unit/J>")
        );

        let uniform = &rows[1];
        assert_eq!(uniform.subject, "uniformProbability");
        assert_eq!(
            uniform.dimension_exponents,
            Vec::new(),
            "dimensionless subject honestly renders an EMPTY exponent vector, not a hard fail"
        );
        assert_eq!(
            uniform.dimension_label, None,
            "math:dimensionless carries no local label in this file — an honest absence"
        );
        assert!(uniform.unit.is_none());
        assert!(uniform.quantity_value.is_none());
        assert!(
            !uniform.turtle.contains("baseDimensionExponent"),
            "no fabricated exponent breakdown for the dimensionless case"
        );
    }

    /// Cold-tree bootstrap + determinism: [`DocsModel::discover_with_catalog`] builds
    /// the whole model from LIVE constraint-catalog bytes with NO
    /// `generated/catalog/constraint-catalog.nq` on disk — the state a fresh clone /
    /// cold `make check` is in, where the pure-disk [`DocsModel::discover`] HARD-FAILS.
    /// Once the SAME bytes are written to disk, the disk path yields byte-identical
    /// constraint rules AND identical per-slice DocMaturity coverage facts. This pins
    /// the guarantee the in-pipeline DocMaturity axis relies on: cold (live bytes) ==
    /// warm (disk bytes), so the `graph/quality-assessment` in `gmeow.gts` cannot
    /// differ between a cold and a warm sync run.
    #[test]
    fn discover_with_catalog_bootstraps_cold_tree_and_matches_disk_path() {
        // A temp repo root carrying exactly one real slice (copied from the committed
        // single-slice fixture) and — deliberately — NO generated/ tree.
        let tmp = tempfile::tempdir().expect("create temp dir");
        let root = tmp.path().join("gmeow-catalog-bootstrap");
        let slice_dir = root.join("slices").join("fixture").join("single");
        std::fs::create_dir_all(&slice_dir).expect("mkdir slice");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("single-slice");
        for file in ["manifest.ttl", "module.ttl"] {
            std::fs::copy(fixture.join(file), slice_dir.join(file))
                .unwrap_or_else(|e| panic!("copy fixture {file}: {e}"));
        }

        // Minimal but valid constraint-catalog N-Quads: one gmeow:ValidationRule with a
        // gmeow:ruleCode (the identity the reader keys on).
        let graph =
            "https://blackcatinformatics.ca/gmeow/graph/fanout/catalog/constraint-catalog.nq";
        let rule = "https://blackcatinformatics.ca/gmeow/rule/box-roles-invalid";
        let catalog_bytes = format!(
            "<{rule}> <{RDF_TYPE}> <{GMEOW_VALIDATION_RULE}> <{graph}> .\n\
             <{rule}> <{GMEOW_RULE_CODE}> \"box-roles.invalid\" <{graph}> .\n"
        )
        .into_bytes();

        // Cold tree: the pure-disk discover HARD-FAILS on the absent catalog...
        let cold = DocsModel::discover(&root);
        assert!(
            matches!(cold, Err(DocsError::ConstraintCatalog(_))),
            "discover() must hard-fail on a cold tree with no generated catalog, got {cold:?}"
        );

        // ...but the live-bytes path BUILDS the whole model with NO disk file present.
        let live = DocsModel::discover_with_catalog(&root, &catalog_bytes)
            .expect("discover_with_catalog must build with no generated/ on disk");
        assert_eq!(
            live.constraint_rules.len(),
            1,
            "the one live rule is decoded"
        );
        assert_eq!(live.constraint_rules[0].code, "box-roles.invalid");

        // Warm tree: writing the SAME bytes to disk lets the disk path build; it must
        // agree with the live path byte-for-byte on the constraint rules AND on the
        // per-slice DocMaturity coverage facts (documents / covers / coverage_fraction),
        // which are exactly what the DocMaturity axis consumes.
        std::fs::create_dir_all(root.join("generated").join("catalog")).expect("mkdir generated");
        std::fs::write(
            root.join("generated")
                .join("catalog")
                .join("constraint-catalog.nq"),
            &catalog_bytes,
        )
        .expect("write catalog");
        let warm =
            DocsModel::discover(&root).expect("discover() must build once the catalog is on disk");
        assert_eq!(
            live.constraint_rules, warm.constraint_rules,
            "live-bytes and disk-bytes constraint rules must be identical"
        );

        // DocSliceFacts is not PartialEq, so compare the load-bearing projection the
        // DocMaturity axis reads: (documents, covers, coverage_fraction bit pattern).
        let project = |m: &DocsModel| -> Vec<(String, std::collections::BTreeSet<String>, u64)> {
            crate::rdf::documentation_graph(m)
                .slices
                .into_iter()
                .map(|s| (s.documents, s.covers, s.coverage_fraction.to_bits()))
                .collect()
        };
        let live_facts = project(&live);
        let warm_facts = project(&warm);
        assert_eq!(
            live_facts, warm_facts,
            "DocMaturity per-slice coverage facts must be identical cold(live) vs warm(disk)"
        );
        assert_eq!(
            live_facts.len(),
            1,
            "the one fixture slice yields exactly one coverage fact"
        );
        let fraction = f64::from_bits(live_facts[0].2);
        assert!(
            (0.0..=1.0).contains(&fraction),
            "the fixture slice earns a bounded, non-vacuous coverage fraction, got {fraction}"
        );
    }
}
