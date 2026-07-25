// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Peerage-aware projection of an [`OwnershipReport`]'s undeclared-dependency
//! diagnostics: an undeclared cross-slice edge between two mutually declared
//! `gmeow:sliceCoFoundationalWith` grounding peers (`lang:`/`math:`/`logic:`) is
//! not automatically an ownership violation — Principle 19's peerage grant
//! deliberately lets the three grounding slices reference each other — but that
//! grant is not a blank cheque either: docs/GROUNDING.md's "seam registry" is the
//! CLOSED set of six sanctioned cross-grounding reference channels, and every
//! peered crossing must land on one of them.
//!
//! [`classify`] joins each undeclared *semantic* [`OwnershipDiagnostic`] to its
//! computed [`DependencyEdge`] (evidence + reconciliation) and to the seam
//! registry + peerage relation read straight off the grounding manifests, and
//! [`peerage_aware_ownership_findings`] projects the result into the same
//! `Finding` surface [`crate::slice_ownership::ownership_findings`] uses:
//!
//! * **Covered** — both grounding peers, mutually declared, and every
//!   referenced term on the edge is carried by a seam whose direction matches
//!   the crossing exactly. Suppressed: no finding (this is the whole point —
//!   a real `lang:` → `math:Quantity` reference on a registered seam must not
//!   also HARD-FAIL as an undeclared dependency).
//! * **PeeredUnregisteredSeam** — both grounding peers, mutually declared, but
//!   at least one referenced term rides the peerage grant with no seam
//!   covering it. `Error` (a NEW code: `slice-ownership.peered-unregistered-seam`)
//!   — the peerage grant is not a general license to reference anything.
//! * **Uncovered** — not both grounding peers (or not mutually declared): the
//!   ordinary `slice-ownership.undeclared-dependency` observation applies,
//!   unchanged, at its current severity.
//!
//! The join from a diagnostic to its edge is TOTAL: `OwnershipAnalyzer` only
//! emits `OwnershipDiagnostic::UndeclaredDependency` for a semantic edge it also
//! placed in `OwnershipReport::edges` with `ReconciliationStatus::Undeclared`
//! (RFC §10), so a diagnostic with no matching edge is an internal-invariant
//! violation of that contract — a HARD FAIL (`Err`), never a silently skipped
//! diagnostic (no-optionality: a missing join is a defect, not a degraded read).
//!
//! # Seam-reader reuse
//!
//! [`SeamRecord`] and [`seam_records_of`] are the SAME reader
//! [`crate::authoring_integrity`]'s R7 seam-registry-drift gate uses (lifted
//! here so both consumers share one seam reader instead of each parsing
//! `gmeow:Seam` individuals independently) — extended with the directed
//! `(from, to)` legs (`gmeow:seamDirection`/`seamFromSlice`/`seamToSlice`) and the
//! raw carrying-term IRIs the drift gate's CURIE-reduced text comparison never
//! needed, but this engine's exact-IRI join does.
//!
//! # R5: tier-forbidden edges
//!
//! [`peerage_aware_ownership_findings`] ALSO folds in [`forbidden_tier_findings`]:
//! every computed dependency edge that violates the tier model (a core slice
//! depending on an extension, or an extension depending on another extension,
//! Principle 16 / RFC §10) is surfaced as a `slice-ownership.forbidden-dependency`
//! `Error`, independent of the peerage/seam classification above and of the
//! edge's declaration status. This is a distinct concern from grounding peerage
//! (a core→core grounding-peer crossing is never tier-forbidden — the three
//! grounding slices are all `tierCore`), folded into the same function only
//! because both existing `make validate` gate sites already call it.
//!
//! # Genuine cross-slice TERM usage only
//!
//! `purrdf`'s [`OwnershipReport::edges`] mines EVERY IRI in an artifact (subject,
//! predicate, object, datatype, graph — RFC §10) that happens to be a
//! validated-owned vocabulary term of some other slice, so an edge's raw
//! evidence over-counts two shapes that are not genuine term USAGE at all:
//!
//! * **Class 1 — slice-IRI-as-data.** A slice referencing another slice's own
//!   IRI as DATA (e.g. `slice-quality-rubric`'s ABox quality records naming
//!   `gmeow:ceilingSlice`/`gmeow:floorSlice <…/slices/norms>` — the assessment
//!   TARGET, not one of `norms`' vocabulary terms). [`slice_iris`] collects the
//!   closed set of every catalogued slice's own IRI; a crossing whose
//!   `referenced_term` IS one of those IRIs is never term usage.
//! * **Class 2 — grounding-slice meta-formalization.** The grounding slices
//!   (`logic:`/`math:`, typed `gmeow:GroundingSlice`) FORMALIZE terms across the
//!   whole ontology (e.g. `logic:characterizes gmeow:assertionFacet` /
//!   `logic:formalizes gmeow:assertionFacet`) — that is their role, analogous to
//!   the meta-level `graph/correspondence-laws` graph, never a build dependency.
//!   [`ReferencePredicateIndex`] re-parses EVERY catalogued slice's own
//!   module/shapes artifact and, for a crossing's referenced term, checks
//!   whether EVERY triple naming it as an object uses one of the
//!   [`GROUNDING_META_PREDICATES`] — if the term is ALSO referenced via a
//!   non-meta predicate (a real object-level use), it is never excluded.
//! * **Class 3 — `gmeow:usesTerm` documentation indexing.** `gmeow:usesTerm`
//!   (owned by `guides:`, domain `gmeow:Recipe`/`gmeow:LearningPath`, range
//!   the wide-open `rdfs:Resource`) is a documentation-index predicate: "a
//!   guide may point at any documented term across any slice" (its own
//!   `skos:definition`), never a build dependency — exactly like Class 2's
//!   grounding-formalization predicates, just authored from any slice rather
//!   than only the grounding three. [`ReferencePredicateIndex`] folds
//!   `gmeow:usesTerm` into the same "is this term named EXCLUSIVELY via a
//!   documentation/meta predicate" test, so a term reachable from a slice's
//!   module ONLY via `gmeow:usesTerm` is excluded exactly like a pure Class 2
//!   crossing — and, symmetrically, a term ALSO referenced via a non-meta
//!   predicate is never excluded.
//! * **Class 4 — competency-query integration tests.** A `queries/competency/
//!   *.rq` SPARQL file (`purrdf`'s `ArtifactRole::CompetencyQuery`) tests
//!   whether the COMPOSED (post-fold) ontology can answer a competency
//!   question — inherently cross-slice by design, an integration test over the
//!   merged bundle, conceptually identical to the already-non-semantic
//!   `EdgeKind::Test`. A crossing whose evidence entry's `from_artifact.role`
//!   IS `ArtifactRole::CompetencyQuery` is never genuine; a term ALSO
//!   referenced by a genuine (non-competency-query) artifact still counts,
//!   since the exclusion is per-evidence-entry, not per-edge.
//! * **Class 5 — internal `skos:relatedMatch` correspondences.** A slice's own
//!   `mappings/*.ttl` asserting `<own-term> skos:relatedMatch <other-gmeow-
//!   term>` is a soft, by-reference CORRESPONDENCE cell — "never a redeclared
//!   range axiom" — a meta-level "see also" link between two GMEOW terms,
//!   never a build dependency. [`ReferencePredicateIndex`] also indexes
//!   `ArtifactRole::Mapping` artifacts and, for those, treats
//!   `skos:relatedMatch` as the sole pure-meta predicate: a term reachable
//!   from a slice's mapping artifact ONLY via `skos:relatedMatch` is excluded,
//!   and — symmetrically — a term ALSO referenced structurally (any other
//!   predicate) in that same mapping artifact is never excluded. External
//!   alignments (`skos:broadMatch`/`closeMatch` to a non-`gmeow` vocabulary)
//!   are untouched: their objects are never a catalogued GMEOW slice's own
//!   term in the first place, so they never reach this filter at all.
//!
//! [`classify`] and [`forbidden_tier_findings`] both apply
//! [`is_genuine_crossing_term`] to every piece of an edge's evidence; an edge
//! left with zero genuine crossing terms is suppressed entirely (no
//! `undeclared-dependency`/`forbidden-dependency`/`peered-unregistered-seam`
//! finding) — this is a filter on WHICH terms count as a crossing, never a
//! license to skip an edge that also carries genuine evidence.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use gmeow_errors::{Diag, Finding, Result, Severity};
use purrdf::slice::catalog::{SliceCatalog, SliceRecord, SliceTier};
use purrdf::slice::rdf_query::{Dataset, Object, Subject};
use purrdf::slice::{
    ArtifactRole, DependencyEdge, EdgeKind, NamedNode, OwnershipDiagnostic, OwnershipReport,
    ReconciliationStatus, SliceIri, is_forbidden_edge,
};

// ── Namespace constants ──────────────────────────────────────────────────────
//
// Grounding-peerage / seam-registry vocabulary is `gmeow:`-fixed governance
// data (Principle 19), never parameterized by the caller's `SliceVocab` — the
// same posture `crate::authoring_integrity`'s R7 gate already takes.

/// `gmeow:GroundingSlice` — a slice typed as one of the three co-foundational
/// grounding layers (`lang:`/`math:`/`logic:`).
const GMEOW_GROUNDING_SLICE: &str = "https://blackcatinformatics.ca/gmeow/GroundingSlice";
/// `gmeow:Seam` — a sanctioned cross-grounding reference channel individual.
const GMEOW_SEAM: &str = "https://blackcatinformatics.ca/gmeow/Seam";
/// `gmeow:seamDirection` — a seam's directed `(from, to)` leg (a blank node).
const GMEOW_SEAM_DIRECTION: &str = "https://blackcatinformatics.ca/gmeow/seamDirection";
/// `gmeow:seamFromSlice` — a seam-direction leg's referencing grounding slice.
const GMEOW_SEAM_FROM_SLICE: &str = "https://blackcatinformatics.ca/gmeow/seamFromSlice";
/// `gmeow:seamToSlice` — a seam-direction leg's referenced grounding slice.
const GMEOW_SEAM_TO_SLICE: &str = "https://blackcatinformatics.ca/gmeow/seamToSlice";
/// `gmeow:seamCarryingTerm` — a term IRI a seam sanctions crossing on.
const GMEOW_SEAM_CARRYING_TERM: &str = "https://blackcatinformatics.ca/gmeow/seamCarryingTerm";
/// `gmeow:seamOwningDoc` — the design-doc filename a seam is documented in.
const GMEOW_SEAM_OWNING_DOC: &str = "https://blackcatinformatics.ca/gmeow/seamOwningDoc";
/// `gmeow:sliceCoFoundationalWith` — the symmetric grounding-peerage relation.
const GMEOW_CO_FOUNDATIONAL_WITH: &str =
    "https://blackcatinformatics.ca/gmeow/sliceCoFoundationalWith";
/// `rdfs:label`.
const RDFS_LABEL_TERM: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// Reduce a term IRI to its `family:Local` CURIE for the four grounding
/// namespaces — the same family map `gmeow_docs::render::to_curie` uses. Used
/// ONLY for the human-readable [`SeamRecord::carrying_terms`] projection (the
/// R7 page-drift text comparison); [`classify`]'s exact-IRI join always
/// compares the raw [`SeamRecord::carrying_term_iris`] instead.
pub(crate) fn seam_term_curie(iri: &str) -> String {
    const FAMILIES: &[(&str, &str)] = &[
        ("https://blackcatinformatics.ca/gmeow/", "gmeow"),
        ("https://blackcatinformatics.ca/logic/", "logic"),
        ("https://blackcatinformatics.ca/math/", "math"),
        ("https://blackcatinformatics.ca/lang/", "lang"),
    ];
    for (ns, prefix) in FAMILIES {
        if let Some(local) = iri.strip_prefix(ns) {
            return format!("{prefix}:{local}");
        }
    }
    iri.to_string()
}

/// One `gmeow:Seam` individual's canonical data, read directly off a grounding
/// slice's `manifest.ttl` — the single reader both the R7 seam-registry drift
/// gate (`crate::authoring_integrity`) and this peerage-coverage engine share.
pub(crate) struct SeamRecord {
    /// The seam's own IRI (`a gmeow:Seam` subject).
    pub(crate) iri: String,
    /// `rdfs:label` (lexically-lowest, deterministic), falling back to the
    /// seam's CURIE when unlabeled.
    pub(crate) name: String,
    /// `gmeow:seamCarryingTerm` objects, reduced to `family:Local` CURIEs (for
    /// the R7 page-drift text comparison ONLY).
    pub(crate) carrying_terms: BTreeSet<String>,
    /// `gmeow:seamCarryingTerm` objects, as raw term IRIs — the exact-IRI set
    /// [`classify`] matches a crossing's referenced term against.
    pub(crate) carrying_term_iris: BTreeSet<String>,
    /// `gmeow:seamDirection` legs, each a `(from_slice_iri, to_slice_iri)` pair,
    /// sorted and deduped.
    pub(crate) directions: Vec<(String, String)>,
    /// `gmeow:seamOwningDoc` literal values.
    pub(crate) owning_docs: BTreeSet<String>,
}

/// The IRI object of `<subject> <pred> ?o`, where `subject` is a blank node (a
/// seam-direction leg). A full default-graph quad scan — NOT
/// `Dataset::objects_of_subject` — because `purrdf_slice::rdf_query`'s
/// `Subject::Blank` → `term_id_by_value` path always looks the label up under
/// `BlankScope::DEFAULT` (`TermValue::blank`'s only scope), so it can never
/// resolve a blank node from a REAL parsed document (Turtle parsing always
/// assigns a non-default per-document scope): `objects_of_subject` silently
/// returns `Ok(vec![])` for every real blank-node subject, never an error.
/// `Subject`'s `PartialEq` compares the already scope-qualified label string
/// [`object_of`]/[`subject_of`] rendered, so equality-matching a quad's
/// resolved `Subject` against this direction leg's blank `Subject` is exact
/// and scope-safe without going through the broken by-value lookup at all.
///
/// `None` on no matching IRI object (a malformed direction leg is skipped,
/// mirroring `gmeow_docs::model::extract_seams`'s `filter_map` posture; a
/// seam's OTHER, well-formed directions still classify correctly).
fn first_named_object_of_subject(ds: &Dataset, subject: &Subject, pred: &str) -> Option<String> {
    let mut found = None;
    ds.for_each_quad(|s, p, o, _graph| {
        if found.is_none()
            && &s == subject
            && p == pred
            && let Object::Named(iri) = o
        {
            found = Some(iri);
        }
    });
    found
}

fn parse_err(path: &Path, e: &str) -> Diag {
    Diag::of_kind(crate::error::Parse {
        detail: format!("{}: {e}", path.display()),
    })
}

/// Every `gmeow:Seam` individual declared in a manifest typed
/// `gmeow:GroundingSlice` — generic over every grounding slice (mirrors
/// `gmeow_docs::model::extract_seams`'s discovery gate; today only `logic:`'s
/// manifest carries the registry, but a future seam authored in `lang:`/`math:`
/// is picked up without a code change).
pub(crate) fn seam_records_of(ds: &Dataset, path: &Path) -> Result<Vec<SeamRecord>> {
    let mut out = Vec::new();
    let grounding = ds
        .subjects_of_type(GMEOW_GROUNDING_SLICE)
        .map_err(|e| parse_err(path, &e.to_string()))?;
    if grounding.is_empty() {
        return Ok(out);
    }
    for seam_iri in ds
        .subjects_of_type(GMEOW_SEAM)
        .map_err(|e| parse_err(path, &e.to_string()))?
    {
        let name = ds
            .objects(&seam_iri, RDFS_LABEL_TERM)
            .map_err(|e| parse_err(path, &e.to_string()))?
            .into_iter()
            .filter_map(|o| match o {
                Object::Literal { value, .. } => Some(value),
                _ => None,
            })
            .min()
            .unwrap_or_else(|| seam_term_curie(&seam_iri));
        let carrying_term_iris: BTreeSet<String> = ds
            .object_iris(&seam_iri, GMEOW_SEAM_CARRYING_TERM)
            .map_err(|e| parse_err(path, &e.to_string()))?
            .into_iter()
            .collect();
        let carrying_terms: BTreeSet<String> = carrying_term_iris
            .iter()
            .map(|iri| seam_term_curie(iri))
            .collect();
        let owning_docs: BTreeSet<String> = ds
            .objects(&seam_iri, GMEOW_SEAM_OWNING_DOC)
            .map_err(|e| parse_err(path, &e.to_string()))?
            .into_iter()
            .filter_map(|o| match o {
                Object::Literal { value, .. } => Some(value),
                _ => None,
            })
            .collect();
        let mut directions: Vec<(String, String)> = ds
            .objects(&seam_iri, GMEOW_SEAM_DIRECTION)
            .map_err(|e| parse_err(path, &e.to_string()))?
            .into_iter()
            .filter_map(|o| match o {
                Object::Blank(label) => {
                    let subject = Subject::Blank(label);
                    let from = first_named_object_of_subject(ds, &subject, GMEOW_SEAM_FROM_SLICE)?;
                    let to = first_named_object_of_subject(ds, &subject, GMEOW_SEAM_TO_SLICE)?;
                    Some((from, to))
                }
                _ => None,
            })
            .collect();
        directions.sort();
        directions.dedup();
        out.push(SeamRecord {
            iri: seam_iri,
            name,
            carrying_terms,
            carrying_term_iris,
            directions,
            owning_docs,
        });
    }
    Ok(out)
}

// ── Catalog-scoped readers ────────────────────────────────────────────────────

/// Wrap a [`SliceRecord`]'s lossless manifest IR as a query-able [`Dataset`],
/// with no re-*parse* of the on-disk `manifest.ttl` (the catalog already
/// parsed it once from bytes — this engine reads the SAME frozen graph
/// content, never a second Turtle parse or a second scan of the source tree;
/// `Dataset::from_frozen` clones the in-memory graph rather than mutating the
/// catalog's shared `Arc`, a cheap in-memory copy of a small manifest graph).
fn manifest_dataset(record: &SliceRecord) -> Dataset {
    Dataset::from_frozen(Arc::clone(&record.manifest_graph))
}

/// Every slice IRI in `catalog` typed `gmeow:GroundingSlice`.
fn grounding_slice_iris(catalog: &SliceCatalog) -> Result<BTreeSet<SliceIri>> {
    let mut out = BTreeSet::new();
    for record in catalog.records() {
        let ds = manifest_dataset(record);
        let is_grounding = ds
            .has_type(&record.manifest.slice_iri, GMEOW_GROUNDING_SLICE)
            .map_err(|e| parse_err(&record.manifest_path(), &e.to_string()))?;
        if is_grounding {
            out.insert(record.manifest.slice_iri.clone());
        }
    }
    Ok(out)
}

/// Every directed `gmeow:sliceCoFoundationalWith` pair across `catalog`
/// (`(declaring_slice, peer_slice)`) — asymmetric AS AUTHORED; [`classify`]
/// requires BOTH directions present before treating a pair as mutually peered.
fn peerage_pairs(catalog: &SliceCatalog) -> Result<BTreeSet<(SliceIri, SliceIri)>> {
    let mut out = BTreeSet::new();
    for record in catalog.records() {
        let ds = manifest_dataset(record);
        let peers = ds
            .object_iris(&record.manifest.slice_iri, GMEOW_CO_FOUNDATIONAL_WITH)
            .map_err(|e| parse_err(&record.manifest_path(), &e.to_string()))?;
        for peer in peers {
            out.insert((record.manifest.slice_iri.clone(), peer));
        }
    }
    Ok(out)
}

/// The tier priority [`purrdf::slice::is_forbidden_edge`] takes: 0 = core,
/// 1 = extension, 2 = domain/unknown/tierless. Byte-identical to
/// `crates/pipeline/src/stages/carrier.rs`'s `tier_priority` (the shipped
/// `graph/slice-analysis` emitter's own mapping) so this gate and the shipped
/// analysis-graph DATA classify the exact same edges as forbidden.
fn tier_priority(tier: Option<&SliceTier>) -> u8 {
    match tier {
        Some(SliceTier::Core) => 0,
        Some(SliceTier::Extension) => 1,
        Some(SliceTier::Domain) | Some(SliceTier::Unknown(_)) | None => 2,
    }
}

/// Every slice IRI in `catalog`, mapped to its [`tier_priority`].
fn tier_priorities(catalog: &SliceCatalog) -> BTreeMap<SliceIri, u8> {
    catalog
        .records()
        .iter()
        .map(|record| {
            (
                record.manifest.slice_iri.clone(),
                tier_priority(record.manifest.tier.as_ref()),
            )
        })
        .collect()
}

/// Every `gmeow:Seam` individual across every grounding manifest in `catalog`.
fn seam_registry(catalog: &SliceCatalog) -> Result<Vec<SeamRecord>> {
    let mut out = Vec::new();
    for record in catalog.records() {
        let ds = manifest_dataset(record);
        out.extend(seam_records_of(&ds, &record.manifest_path())?);
    }
    Ok(out)
}

/// Every catalogued slice's own IRI (`record.manifest.slice_iri`) — the CLOSED
/// set [`is_genuine_crossing_term`]'s Class 1 filter tests a crossing's
/// `referenced_term` against. A term IRI in this set is never a vocabulary term
/// at all; it is the slice resource itself, cited as DATA (e.g.
/// `slice-quality-rubric`'s `gmeow:ceilingSlice <…/slices/norms>`).
fn slice_iris(catalog: &SliceCatalog) -> BTreeSet<SliceIri> {
    catalog
        .records()
        .iter()
        .map(|record| record.manifest.slice_iri.clone())
        .collect()
}

// ── Class 2: grounding law/formalization meta-predicates ─────────────────────

/// The empirically-discovered, textually-confirmed closed set of `logic:`/
/// `math:` law-authoring / formalization-bridge predicates — asserted FROM a
/// grounding slice's own `module.ttl`/`shapes.ttl`, never a
/// `sliceDependsOn`-reconciling object-level use of the named term.
///
/// Discovered via
/// `grep -oE '^\s*(logic|math):[A-Za-z0-9_]+\s+(gmeow|lang|math):'
/// slices/grounding/{logic,math}/module.ttl | sed -E 's/^\s*//; s/\s+.*$//' |
/// sort | uniq -c`, then reading every candidate predicate's OWN
/// `skos:definition` in `slices/grounding/logic/module.ttl` /
/// `slices/grounding/math/module.ttl`:
///
/// * `logic:formalizes` / `math:formalizes` are each self-documented VERBATIM
///   as "a bridge annotation from a `logic:`/`math:` … term to the `gmeow:`
///   domain concept it formalizes … an annotation property, never a reasoned
///   axiom, so it carries no DL or EL profile weight."
/// * `logic:candidateFormalizes` is the `logic:FormalizationCandidate` harvest
///   back-link ("present exactly when the source IS a term's annotation
///   field … Distinct from `logic:formalizes` … Domain
///   `logic:FormalizationCandidate`") and `logic:proseFieldProperty` names
///   which annotation field (`skos:definition`/`gmeow:useWhen`/
///   `gmeow:avoidWhen`) a harvested candidate's prose came from — pure harvest
///   bookkeeping, never term usage.
/// * The remaining eighteen are FIELDS of one of six `logic:*Assertion`
///   "central record" carriers — `PropertyCharacteristicAssertion`,
///   `RelatumDistinctnessAssertion`, `KeyAssertion`, `ConditionalRangeAssertion`,
///   `MediatedPropertyRequirementAssertion`, `RoleCompositionExclusionAssertion`
///   — whose `skos:definition`s each open with the IDENTICAL "A central record
///   asserting that…" signature and close "The reasoning authority names and
///   enforces it here; the domain slice keeps the OWL-facing declaration" (or
///   the `logic:formalizes`-carrying equivalent): the named class/property is
///   the record's join key for the native coherence gate, never predicated
///   over as live axiom content.
///
/// Deliberately EXCLUDED (real, genuine object-level use, confirmed by reading
/// the same definitions): `logic:relation`/`logic:termIri`/`logic:argument`
/// fill a REAL `logic:Formula` AST (e.g. `logic:coverEntitySortals`'s full-FOL
/// `∀x. Entity(x) → Agent(x) ∨ InformationObject(x) ∨ …` foundational
/// class-covering, and `logic:deceptionHeldProjectedDivergence`'s
/// `eventType(e, eventTypeDeception)` axiom — reasoned content, not
/// documentation); `logic:projectsLadderEdge` / `logic:evidenceProperty` /
/// `logic:evidenceFor` are genuine derivation-rule content (a
/// `logic:DimensionThreshold` row LITERALLY emits the named `gmeow:` ladder
/// property under the coarse-ladder projection rules); and `logic:domain` is a
/// real DL entailment axiom ("it lowers to `rdfs:domain`"), not an annotation.
/// `logic:PropertyCharacteristicAssertion`'s `logic:characteristicSort` field
/// is also never in this set — its object is always a closed `logic:`
/// characteristic-sort marker, never a cross-slice term.
const GROUNDING_META_PREDICATES: &[&str] = &[
    "https://blackcatinformatics.ca/logic/formalizes",
    "https://blackcatinformatics.ca/logic/candidateFormalizes",
    "https://blackcatinformatics.ca/logic/proseFieldProperty",
    "https://blackcatinformatics.ca/logic/characterizes",
    "https://blackcatinformatics.ca/logic/distinctnessTarget",
    "https://blackcatinformatics.ca/logic/distinctnessRole",
    "https://blackcatinformatics.ca/logic/keyClass",
    "https://blackcatinformatics.ca/logic/keyProperty",
    "https://blackcatinformatics.ca/logic/conditionalRangeTarget",
    "https://blackcatinformatics.ca/logic/conditionalRangeSelector",
    "https://blackcatinformatics.ca/logic/conditionalRangeSelectorValue",
    "https://blackcatinformatics.ca/logic/conditionalRangeValue",
    "https://blackcatinformatics.ca/logic/conditionalRangeRequiredType",
    "https://blackcatinformatics.ca/logic/mediatedRequirementTarget",
    "https://blackcatinformatics.ca/logic/mediatedRequirementVia",
    "https://blackcatinformatics.ca/logic/mediatedRequirementProperty",
    "https://blackcatinformatics.ca/logic/compositionExclusionTarget",
    "https://blackcatinformatics.ca/logic/compositionExclusionWhole",
    "https://blackcatinformatics.ca/logic/compositionExclusionPart",
    "https://blackcatinformatics.ca/logic/compositionExclusionVia",
    "https://blackcatinformatics.ca/math/formalizes",
];

/// `gmeow:usesTerm` — the `guides:`-owned documentation-index predicate a
/// `gmeow:Recipe`/`gmeow:LearningPath` uses to point at any documented term
/// across any slice (its own `skos:definition`: "the range is left open
/// (`rdfs:Resource`) because a guide may point at any documented term across
/// any slice"). Folded into [`ReferencePredicateIndex::is_pure_meta`]'s
/// exclusion set alongside [`GROUNDING_META_PREDICATES`]: a documentation
/// back-reference, from ANY slice (not only the three grounding slices), is
/// never genuine object-level term usage.
const GMEOW_USES_TERM: &str = "https://blackcatinformatics.ca/gmeow/usesTerm";

/// `skos:relatedMatch` — a soft, by-reference correspondence between two
/// terms ("never a redeclared range axiom"). Authored FROM a slice's own
/// `mappings/*.ttl` naming another catalogued GMEOW slice's term as object,
/// it is a meta-level "see also" link, never a build dependency — folded into
/// [`ReferencePredicateIndex::is_pure_meta`]'s exclusion set for
/// [`ArtifactRole::Mapping`] artifacts specifically (never for
/// `Module`/`Shapes`, where the identical predicate string would carry a
/// different, non-mapping meaning if it ever appeared there).
const SKOS_RELATED_MATCH: &str = "http://www.w3.org/2004/02/skos/core#relatedMatch";

/// Per-artifact, the set of predicates by which that artifact references each
/// term IRI as the OBJECT of a triple — built ONCE per catalog, over EVERY
/// catalogued slice (not only the grounding three: [`GMEOW_USES_TERM`] is
/// authored from any slice), so [`is_genuine_crossing_term`]'s Class 2/Class 3/
/// Class 5 check never re-parses an artifact per crossing.
/// Every predicate by which an artifact references a term IRI as an object,
/// keyed by term.
type TermPredicates = BTreeMap<String, BTreeSet<String>>;

struct ReferencePredicateIndex {
    /// `(slice IRI, artifact logical path)` -> the artifact's role, and term
    /// IRI -> the set of predicates that reference it as an object anywhere in
    /// that artifact.
    by_artifact: BTreeMap<(SliceIri, String), (ArtifactRole, TermPredicates)>,
}

impl ReferencePredicateIndex {
    /// Index every `Module`/`Shapes`/`Mapping` artifact (the three
    /// ownership-bearing, RDF-parseable roles this engine's meta-predicate
    /// exclusions care about — RFC §10, and `mappings/*.ttl` for Class 5's
    /// `skos:relatedMatch` correspondences) of every catalogued slice.
    fn build(catalog: &SliceCatalog) -> Result<Self> {
        let mut by_artifact = BTreeMap::new();
        for record in catalog.records() {
            for artifact in &record.artifacts {
                if !matches!(
                    artifact.role,
                    ArtifactRole::Module | ArtifactRole::Shapes | ArtifactRole::Mapping
                ) {
                    continue;
                }
                let ds = Dataset::parse_turtle(&artifact.content, &artifact.logical_path)
                    .map_err(|e| parse_err(Path::new(&artifact.logical_path), &e.to_string()))?;
                let mut term_predicates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
                ds.for_each_quad(|_s, p, o, _g| {
                    if let Object::Named(iri) = o {
                        term_predicates.entry(iri).or_default().insert(p.to_string());
                    }
                });
                by_artifact.insert(
                    (record.manifest.slice_iri.clone(), artifact.logical_path.clone()),
                    (artifact.role.clone(), term_predicates),
                );
            }
        }
        Ok(Self { by_artifact })
    }

    /// Whether `term`, as referenced from `from_slice`'s `logical_path`
    /// artifact, is EXCLUSIVELY named via a pure meta-predicate for that
    /// artifact's role — for `Module`/`Shapes`, a [`GROUNDING_META_PREDICATES`]
    /// entry or [`GMEOW_USES_TERM`] (Class 2/3); for `Mapping`,
    /// [`SKOS_RELATED_MATCH`] (Class 5) — i.e. every triple in that artifact
    /// whose object is `term` uses a pure law/formalization,
    /// documentation-index, or internal-correspondence predicate. `false`
    /// (never pure-meta; the no-optionality-safe default is GENUINE) when the
    /// artifact was not indexed (a non-`Module`/`Shapes`/`Mapping` role, e.g. a
    /// `Query` edge) or `term` was never seen as an object at all in that
    /// artifact.
    fn is_pure_meta(&self, from_slice: &str, logical_path: &str, term: &str) -> bool {
        let Some((role, term_predicates)) = self
            .by_artifact
            .get(&(from_slice.to_string(), logical_path.to_string()))
        else {
            return false;
        };
        let Some(preds) = term_predicates.get(term).filter(|preds| !preds.is_empty()) else {
            return false;
        };
        match role {
            ArtifactRole::Module | ArtifactRole::Shapes => preds.iter().all(|p| {
                GROUNDING_META_PREDICATES.contains(&p.as_str()) || p.as_str() == GMEOW_USES_TERM
            }),
            ArtifactRole::Mapping => preds.iter().all(|p| p.as_str() == SKOS_RELATED_MATCH),
            _ => false,
        }
    }
}

/// Whether `term` (referenced from `from_slice`'s `from_artifact` artifact,
/// via `from_artifact_role`) is GENUINE cross-slice term usage — none of:
///
/// * Class 1 — the raw IRI of some other catalogued slice, cited as DATA;
/// * Class 2/3 — a PURE grounding law/formalization back-reference
///   ([`GROUNDING_META_PREDICATES`]) or a PURE `gmeow:usesTerm`
///   documentation-index reference ([`GMEOW_USES_TERM`]);
/// * Class 4 — evidence from an [`ArtifactRole::CompetencyQuery`] artifact (a
///   `queries/competency/*.rq` integration test over the composed ontology,
///   never a build dependency);
/// * Class 5 — a PURE internal `skos:relatedMatch` correspondence authored
///   from a `Mapping` artifact ([`SKOS_RELATED_MATCH`]).
///
/// Used to filter an edge's evidence before it can produce an
/// undeclared/forbidden/peered-unregistered-seam finding.
fn is_genuine_crossing_term(
    from_slice: &SliceIri,
    from_artifact_role: &ArtifactRole,
    from_artifact_logical_path: &str,
    term: &NamedNode,
    slice_iris: &BTreeSet<SliceIri>,
    reference_predicates: &ReferencePredicateIndex,
) -> bool {
    if slice_iris.contains(term.as_str()) {
        return false;
    }
    if *from_artifact_role == ArtifactRole::CompetencyQuery {
        return false;
    }
    !reference_predicates.is_pure_meta(from_slice, from_artifact_logical_path, term.as_str())
}

// ── Classification ────────────────────────────────────────────────────────────

/// The peerage-coverage verdict for one undeclared semantic dependency edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Coverage {
    /// Both grounding, mutually peered, and every referenced term on the edge
    /// is carried by a seam whose direction matches this crossing exactly.
    Covered,
    /// Both grounding and mutually peered, but at least one referenced term is
    /// not carried by any seam covering this crossing direction.
    PeeredUnregisteredSeam {
        /// The offending terms, sorted/deduped.
        offending_terms: Vec<NamedNode>,
    },
    /// Not both grounding peers, or not mutually declared: the ordinary
    /// undeclared-dependency observation applies, unchanged.
    Uncovered,
}

/// One classified undeclared semantic dependency edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndeclaredEdgeVerdict {
    /// The depending slice.
    pub from_slice: SliceIri,
    /// The depended-upon slice.
    pub to_slice: SliceIri,
    /// The artifact-role classification of the undeclared edge.
    pub edge_kind: EdgeKind,
    /// The peerage-coverage verdict.
    pub coverage: Coverage,
}

/// One `(edge, term)` pair a registered seam covers — exposed for a future
/// consumer (e.g. a peerage-coverage report) to project; this engine itself
/// only needs it to suppress the finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossingCoverage {
    /// The depending slice.
    pub from_slice: SliceIri,
    /// The depended-upon slice.
    pub to_slice: SliceIri,
    /// The covering seam's IRI.
    pub seam_iri: String,
    /// The covered term.
    pub term: NamedNode,
}

/// The complete peerage classification of an [`OwnershipReport`] against a
/// [`SliceCatalog`]'s grounding-peerage + seam-registry data.
#[derive(Debug, Clone, Default)]
pub struct PeerageClassification {
    /// One verdict per undeclared *semantic* dependency edge.
    pub verdicts: Vec<UndeclaredEdgeVerdict>,
    /// Every `(edge, term)` pair a registered seam covers.
    pub crossings: Vec<CrossingCoverage>,
}

/// Classify every undeclared *semantic* dependency edge in `report` against
/// the grounding-peerage relation and seam registry read off `catalog`.
///
/// Non-semantic edge kinds (`Test`/`Example`/`Documentation`/`Generated`) never
/// reconcile against `<vocab>sliceDependsOn` (`EdgeKind::is_semantic`), so
/// `OwnershipAnalyzer` never emits an `UndeclaredDependency` diagnostic for one
/// in practice — this still filters them defensively (never trusting an
/// upstream invariant it does not itself also check).
///
/// The join from a diagnostic to its [`DependencyEdge`] is TOTAL: a semantic
/// `UndeclaredDependency` diagnostic with no matching
/// `ReconciliationStatus::Undeclared` edge in `report.edges` is an internal
/// contract violation between the diagnostics and edges the analyzer itself
/// produced — a HARD FAIL (`Err`), never a silently skipped diagnostic.
pub fn classify(report: &OwnershipReport, catalog: &SliceCatalog) -> Result<PeerageClassification> {
    let grounding = grounding_slice_iris(catalog)?;
    let peers = peerage_pairs(catalog)?;
    let seams = seam_registry(catalog)?;
    let all_slice_iris = slice_iris(catalog);
    let reference_predicates = ReferencePredicateIndex::build(catalog)?;

    let mut verdicts = Vec::new();
    let mut crossings = Vec::new();

    for diag in &report.diagnostics {
        let OwnershipDiagnostic::UndeclaredDependency {
            from_slice,
            to_slice,
            edge_kind,
        } = diag
        else {
            continue;
        };
        if !edge_kind.is_semantic() {
            continue;
        }

        let edge: &DependencyEdge = report
            .edges
            .iter()
            .find(|e| {
                &e.from_slice == from_slice
                    && &e.to_slice == to_slice
                    && e.edge_kind == *edge_kind
                    && e.reconciliation == ReconciliationStatus::Undeclared
            })
            .ok_or_else(|| {
                Diag::of_kind(crate::error::Catalog {
                    detail: format!(
                        "peerage classification: OwnershipDiagnostic::UndeclaredDependency \
                         {from_slice} -> {to_slice} ({edge_kind:?}) has no matching \
                         ReconciliationStatus::Undeclared DependencyEdge in \
                         OwnershipReport::edges — the ownership-analysis diagnostic/edge join \
                         must be total"
                    ),
                })
            })?;

        // A crossing's evidence is filtered to GENUINE cross-slice term usage
        // before anything else: neither Class 1 (the raw IRI of some other
        // catalogued slice, cited as data) nor Class 2 (a pure grounding
        // law/formalization back-reference). An edge left with zero genuine
        // crossing terms is not a real dependency at all — suppressed
        // entirely, never even reaching the peerage/seam classification below.
        let mut genuine_terms: Vec<&NamedNode> = edge
            .evidence
            .iter()
            .filter(|e| {
                is_genuine_crossing_term(
                    from_slice,
                    &e.from_artifact.role,
                    &e.from_artifact.logical_path,
                    &e.referenced_term,
                    &all_slice_iris,
                    &reference_predicates,
                )
            })
            .map(|e| &e.referenced_term)
            .collect();
        genuine_terms.sort();
        genuine_terms.dedup();
        if genuine_terms.is_empty() {
            continue;
        }

        let both_grounding = grounding.contains(from_slice) && grounding.contains(to_slice);
        let mutually_peered = peers.contains(&(from_slice.clone(), to_slice.clone()))
            && peers.contains(&(to_slice.clone(), from_slice.clone()));

        let coverage = if both_grounding && mutually_peered {
            let covering_seams: Vec<&SeamRecord> = seams
                .iter()
                .filter(|s| {
                    s.directions
                        .iter()
                        .any(|(f, t)| f == from_slice && t == to_slice)
                })
                .collect();

            let mut offending_terms = Vec::new();
            for term in genuine_terms {
                let covering_seam = covering_seams
                    .iter()
                    .find(|seam| seam.carrying_term_iris.contains(term.as_str()));
                match covering_seam {
                    Some(seam) => crossings.push(CrossingCoverage {
                        from_slice: from_slice.clone(),
                        to_slice: to_slice.clone(),
                        seam_iri: seam.iri.clone(),
                        term: term.clone(),
                    }),
                    None => offending_terms.push(term.clone()),
                }
            }

            if offending_terms.is_empty() {
                Coverage::Covered
            } else {
                Coverage::PeeredUnregisteredSeam { offending_terms }
            }
        } else {
            Coverage::Uncovered
        };

        verdicts.push(UndeclaredEdgeVerdict {
            from_slice: from_slice.clone(),
            to_slice: to_slice.clone(),
            edge_kind: *edge_kind,
            coverage,
        });
    }

    Ok(PeerageClassification {
        verdicts,
        crossings,
    })
}

// ── R5: tier-forbidden edges ─────────────────────────────────────────────────

/// Every computed dependency edge in `report.edges` that violates the tier
/// model (Principle 16 / RFC §10): a core slice depending on an extension, or
/// an extension depending on another extension. Independent of the
/// peerage/seam machinery above and of the edge's [`ReconciliationStatus`] —
/// a forbidden tier crossing is architecturally illegal regardless of
/// grounding peerage or declaration; even a `Matched` (authored
/// `gmeow:sliceDependsOn`) edge between a forbidden tier pair is still
/// forbidden. Grouped by `(from_slice, to_slice)` (one finding per crossing
/// pair, naming every [`EdgeKind`] that crosses it) rather than one per
/// individual edge, since the violation is a property of the SLICE PAIR, not
/// of any one artifact reference.
///
/// This is the ONLY place a tier-forbidden edge is surfaced as a
/// validate-gating [`Finding`]: the `gmeow:graph/slice-analysis` named graph
/// the pipeline ships (`crates/pipeline/src/stages/carrier.rs::build_slice_analysis`,
/// via `purrdf::slice::emit_analysis_graph`) records the identical verdict as
/// shipped DATA (`gmeow:dependencyStatus "forbidden"^^xsd:string`), but
/// nothing read that graph back to gate `make validate` — this function closes
/// that gap directly off `OwnershipReport::edges` + the catalog's own tier
/// data, using the SAME [`is_forbidden_edge`] tier-priority test the emitter
/// uses (byte-identical [`tier_priority`] mapping), so the gate and the
/// shipped data can never classify an edge differently.
///
/// An edge that DOES carry evidence is also filtered to genuine cross-slice
/// term usage (the same Class 1 / Class 2 / Class 3 [`is_genuine_crossing_term`]
/// check [`classify`] applies): an edge whose evidence is 100% slice-IRI-as-data,
/// pure grounding-meta-formalization, or pure `gmeow:usesTerm`
/// documentation-index crossings is not a real dependency and contributes no
/// tier-forbidden pair. An edge with NO evidence at all (a
/// synthetic `ReconciliationStatus::Stale` edge — an authored
/// `sliceDependsOn` with no semantic backing) is a distinct, orthogonal
/// concern this function does not touch: there is no term-usage evidence to
/// classify, so it is judged on tier alone exactly as before.
fn forbidden_tier_findings(report: &OwnershipReport, catalog: &SliceCatalog) -> Result<Vec<Finding>> {
    let tiers = tier_priorities(catalog);
    let all_slice_iris = slice_iris(catalog);
    let reference_predicates = ReferencePredicateIndex::build(catalog)?;

    let mut by_pair: BTreeMap<(SliceIri, SliceIri), BTreeSet<EdgeKind>> = BTreeMap::new();
    for edge in &report.edges {
        // The tier DAG governs SEMANTIC build dependencies only (the same
        // `EdgeKind::is_semantic` set that reconciles against `sliceDependsOn`); a
        // test/example/documentation cross-tier reference is not a build dependency
        // and never violates Principle 16.
        if !edge.edge_kind.is_semantic() {
            continue;
        }
        if !edge.evidence.is_empty() {
            let has_genuine_evidence = edge.evidence.iter().any(|e| {
                is_genuine_crossing_term(
                    &edge.from_slice,
                    &e.from_artifact.role,
                    &e.from_artifact.logical_path,
                    &e.referenced_term,
                    &all_slice_iris,
                    &reference_predicates,
                )
            });
            if !has_genuine_evidence {
                continue;
            }
        }
        let from_tier = *tiers.get(&edge.from_slice).unwrap_or(&2);
        let to_tier = *tiers.get(&edge.to_slice).unwrap_or(&2);
        if is_forbidden_edge(from_tier, to_tier) {
            by_pair
                .entry((edge.from_slice.clone(), edge.to_slice.clone()))
                .or_default()
                .insert(edge.edge_kind);
        }
    }

    let mut findings = Vec::with_capacity(by_pair.len());
    for ((from, to), kinds) in by_pair {
        let kinds_text = kinds
            .iter()
            .map(|k| format!("{k:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        findings.push(crate::slice_ownership::finding(
            Severity::Error,
            crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY,
            format!(
                "{from} depends on {to} ({kinds_text}) — this crossing violates the tier model: \
                 a core slice must not depend on an extension, and an extension must not depend \
                 on another extension (Principle 16)",
            ),
            Some(from.clone()),
        ));
    }
    Ok(findings)
}

// ── Finding projection ────────────────────────────────────────────────────────

/// Project an [`OwnershipReport`] into findings exactly as
/// [`crate::slice_ownership::ownership_findings`] does, EXCEPT that:
///
/// * every `slice-ownership.undeclared-dependency` observation is re-derived
///   from [`classify`]: a `Covered` crossing is suppressed entirely, a
///   `PeeredUnregisteredSeam` crossing becomes a NEW `Error`
///   (`slice-ownership.peered-unregistered-seam`), and an `Uncovered` crossing
///   keeps the ordinary finding at its current (`Error`) severity,
///   byte-for-byte identical to [`crate::slice_ownership::diagnostic_finding`]'s
///   projection;
/// * every tier-forbidden edge (any reconciliation status) is ADDITIONALLY
///   surfaced as a `slice-ownership.forbidden-dependency` `Error`
///   ([`forbidden_tier_findings`], R5).
pub fn peerage_aware_ownership_findings(
    report: &OwnershipReport,
    catalog: &SliceCatalog,
) -> Result<Vec<Finding>> {
    let classification = classify(report, catalog)?;

    let mut findings: Vec<Finding> = crate::slice_ownership::ownership_findings(report)
        .into_iter()
        .filter(|f| f.code != crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY)
        .collect();

    findings.extend(forbidden_tier_findings(report, catalog)?);

    for verdict in &classification.verdicts {
        match &verdict.coverage {
            Coverage::Covered => {}
            Coverage::Uncovered => {
                let diag = OwnershipDiagnostic::UndeclaredDependency {
                    from_slice: verdict.from_slice.clone(),
                    to_slice: verdict.to_slice.clone(),
                    edge_kind: verdict.edge_kind,
                };
                if let Some(f) = crate::slice_ownership::diagnostic_finding(&diag) {
                    findings.push(f);
                }
            }
            Coverage::PeeredUnregisteredSeam { offending_terms } => {
                let terms_text = offending_terms
                    .iter()
                    .map(NamedNode::as_str)
                    .collect::<Vec<_>>()
                    .join(", ");
                findings.push(crate::slice_ownership::finding(
                    Severity::Error,
                    crate::codes::SLICE_OWNERSHIP_PEERED_UNREGISTERED_SEAM,
                    format!(
                        "{from} depends on {to} across a declared gmeow:sliceCoFoundationalWith \
                         peering, but term(s) {terms_text} are not carried by any gmeow:Seam \
                         registered for the {from} -> {to} direction — register the crossing on \
                         a seam or declare an ordinary gmeow:sliceDependsOn edge",
                        from = verdict.from_slice,
                        to = verdict.to_slice,
                    ),
                    Some(verdict.from_slice.clone()),
                ));
            }
        }
    }

    findings.sort_by(|a, b| {
        (a.severity as u8, &a.code, &a.message).cmp(&(b.severity as u8, &b.code, &b.message))
    });
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::slice::{ArtifactEvidence, ArtifactRole, EdgeEvidence};

    const LOGIC: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";
    const LANG: &str = "https://blackcatinformatics.ca/gmeow/slices/lang";
    const MATH: &str = "https://blackcatinformatics.ca/gmeow/slices/math";
    const CORE: &str = "https://blackcatinformatics.ca/gmeow/slices/core";
    const EXT: &str = "https://blackcatinformatics.ca/gmeow/slices/ext";

    fn nn(iri: &str) -> NamedNode {
        NamedNode::new(iri).unwrap()
    }

    fn vocab() -> purrdf::SliceVocab {
        purrdf::SliceVocab::for_namespace("https://blackcatinformatics.ca/gmeow/")
    }

    fn write_manifest(root: &Path, group: &str, name: &str, ttl: &str) {
        let dir = root.join("slices").join(group).join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let prefixed = format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
             @prefix math: <https://blackcatinformatics.ca/math/> .\n\
             {ttl}"
        );
        std::fs::write(dir.join("manifest.ttl"), prefixed).unwrap();
    }

    /// Write a slice's `module.ttl` (an ownership-bearing artifact, unlike
    /// `manifest.ttl`) — needed to exercise [`ReferencePredicateIndex`]
    /// (Class 2 / Class 3) and the slice-IRI-as-data (Class 1) filter, both of
    /// which re-parse REAL artifact content off the catalog rather than
    /// trusting a hand-built [`OwnershipReport`] fixture.
    fn write_module(root: &Path, group: &str, name: &str, ttl: &str) {
        let dir = root.join("slices").join(group).join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let prefixed = format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
             @prefix math: <https://blackcatinformatics.ca/math/> .\n\
             {ttl}"
        );
        std::fs::write(dir.join("module.ttl"), prefixed).unwrap();
    }

    /// Write a slice's `mappings/<file>.ttl` (an [`ArtifactRole::Mapping`]
    /// artifact) — needed to exercise [`ReferencePredicateIndex`]'s Class 5
    /// `skos:relatedMatch` handling, which re-parses REAL mapping content off
    /// the catalog rather than trusting a hand-built [`OwnershipReport`]
    /// fixture.
    fn write_mapping(root: &Path, group: &str, name: &str, file: &str, ttl: &str) {
        let dir = root.join("slices").join(group).join(name).join("mappings");
        std::fs::create_dir_all(&dir).unwrap();
        let prefixed = format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             {ttl}"
        );
        std::fs::write(dir.join(file), prefixed).unwrap();
    }

    /// A catalog with the three grounding slices, mutually peered, `logic:`
    /// hosting one seam `lang -> logic` carrying `logic:Foo`, and `math:`
    /// hosting one seam `lang -> math` carrying `math:Quantity` (the real
    /// `quantity` seam's direction — `math -> lang` is NOT sanctioned).
    fn grounding_catalog(root: &Path) -> SliceCatalog {
        write_manifest(
            root,
            "grounding",
            "logic",
            r#"<https://blackcatinformatics.ca/gmeow/slices/logic>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "logic" ;
                gmeow:sliceCoFoundationalWith <https://blackcatinformatics.ca/gmeow/slices/lang> ,
                    <https://blackcatinformatics.ca/gmeow/slices/math> .

            <https://blackcatinformatics.ca/gmeow/seam/test-seam>
                a gmeow:Seam ;
                rdfs:label "Test seam" ;
                gmeow:seamDirection [
                    gmeow:seamFromSlice <https://blackcatinformatics.ca/gmeow/slices/lang> ;
                    gmeow:seamToSlice <https://blackcatinformatics.ca/gmeow/slices/logic>
                ] ;
                gmeow:seamCarryingTerm logic:Foo ;
                gmeow:seamOwningDoc "TEST.md" .
            "#,
        );
        write_manifest(
            root,
            "grounding",
            "lang",
            r#"<https://blackcatinformatics.ca/gmeow/slices/lang>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "lang" ;
                gmeow:sliceCoFoundationalWith <https://blackcatinformatics.ca/gmeow/slices/logic> ,
                    <https://blackcatinformatics.ca/gmeow/slices/math> .
            "#,
        );
        write_manifest(
            root,
            "grounding",
            "math",
            r#"<https://blackcatinformatics.ca/gmeow/slices/math>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "math" ;
                gmeow:sliceCoFoundationalWith <https://blackcatinformatics.ca/gmeow/slices/logic> ,
                    <https://blackcatinformatics.ca/gmeow/slices/lang> .

            <https://blackcatinformatics.ca/gmeow/seam/quantity-seam>
                a gmeow:Seam ;
                rdfs:label "Quantity seam" ;
                gmeow:seamDirection [
                    gmeow:seamFromSlice <https://blackcatinformatics.ca/gmeow/slices/lang> ;
                    gmeow:seamToSlice <https://blackcatinformatics.ca/gmeow/slices/math>
                ] ;
                gmeow:seamCarryingTerm math:Quantity ;
                gmeow:seamOwningDoc "QUANTITY.md" .
            "#,
        );
        write_manifest(
            root,
            "core",
            "core",
            r#"<https://blackcatinformatics.ca/gmeow/slices/core>
                a gmeow:Slice ;
                rdfs:label "core" .
            "#,
        );
        write_manifest(
            root,
            "core",
            "ext",
            r#"<https://blackcatinformatics.ca/gmeow/slices/ext>
                a gmeow:Slice ;
                rdfs:label "ext" .
            "#,
        );
        SliceCatalog::discover(&root.join("slices"), vocab()).unwrap()
    }

    fn artifact_evidence(slice: &str) -> ArtifactEvidence {
        ArtifactEvidence {
            slice: slice.to_string(),
            role: ArtifactRole::Module,
            logical_path: "module.ttl".to_string(),
            raw_digest: "deadbeef".to_string(),
        }
    }

    /// An [`ArtifactEvidence`] for a specific role + logical path — needed to
    /// exercise Class 4 (`ArtifactRole::CompetencyQuery`) and Class 5
    /// (`ArtifactRole::Mapping`), which [`artifact_evidence`]'s hard-coded
    /// `Module`/`module.ttl` cannot represent.
    fn artifact_evidence_with(slice: &str, role: ArtifactRole, logical_path: &str) -> ArtifactEvidence {
        ArtifactEvidence {
            slice: slice.to_string(),
            role,
            logical_path: logical_path.to_string(),
            raw_digest: "deadbeef".to_string(),
        }
    }

    fn edge(
        from: &str,
        to: &str,
        kind: EdgeKind,
        terms: &[&str],
        reconciliation: ReconciliationStatus,
    ) -> DependencyEdge {
        DependencyEdge {
            from_slice: from.to_string(),
            to_slice: to.to_string(),
            edge_kind: kind,
            evidence: terms
                .iter()
                .map(|t| EdgeEvidence {
                    from_artifact: artifact_evidence(from),
                    referenced_term: nn(t),
                })
                .collect(),
            reconciliation,
        }
    }

    /// A [`DependencyEdge`] built from an explicit evidence list — needed when
    /// a test must control per-evidence-entry `from_artifact` (role/logical
    /// path), unlike [`edge`], which always attaches the uniform
    /// `Module`/`module.ttl` [`artifact_evidence`].
    fn edge_with_evidence(
        from: &str,
        to: &str,
        kind: EdgeKind,
        evidence: Vec<EdgeEvidence>,
        reconciliation: ReconciliationStatus,
    ) -> DependencyEdge {
        DependencyEdge {
            from_slice: from.to_string(),
            to_slice: to.to_string(),
            edge_kind: kind,
            evidence,
            reconciliation,
        }
    }

    fn undeclared_diag(from: &str, to: &str, kind: EdgeKind) -> OwnershipDiagnostic {
        OwnershipDiagnostic::UndeclaredDependency {
            from_slice: from.to_string(),
            to_slice: to.to_string(),
            edge_kind: kind,
        }
    }

    #[test]
    fn covered_peer_seam_crossing_is_suppressed() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                LANG,
                LOGIC,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/logic/Foo"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(LANG, LOGIC, EdgeKind::Ontology)],
        };

        let classification = classify(&report, &catalog).expect("classify must not hard-fail");
        assert_eq!(classification.verdicts.len(), 1);
        assert_eq!(classification.verdicts[0].coverage, Coverage::Covered);
        assert_eq!(classification.crossings.len(), 1);
        assert_eq!(
            classification.crossings[0].seam_iri,
            "https://blackcatinformatics.ca/gmeow/seam/test-seam"
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            findings.is_empty(),
            "a covered peer+seam crossing must not fire any finding: {findings:?}"
        );
    }

    #[test]
    fn peered_crossing_with_an_off_seam_term_fires_error() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                LANG,
                LOGIC,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/logic/Bar"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(LANG, LOGIC, EdgeKind::Ontology)],
        };

        let classification = classify(&report, &catalog).unwrap();
        match &classification.verdicts[0].coverage {
            Coverage::PeeredUnregisteredSeam { offending_terms } => {
                assert_eq!(
                    offending_terms,
                    &vec![nn("https://blackcatinformatics.ca/logic/Bar")]
                );
            }
            other => panic!("expected PeeredUnregisteredSeam, got {other:?}"),
        }

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "slice-ownership.peered-unregistered-seam");
        assert_eq!(findings[0].severity, Severity::Error);
        assert!(findings[0].message.contains("Bar"));
        assert!(findings[0].message.contains(LANG));
        assert!(findings[0].message.contains(LOGIC));
    }

    #[test]
    fn reverse_direction_of_a_registered_seam_is_not_covered() {
        // The quantity seam sanctions lang -> math carrying math:Quantity; the
        // REVERSE crossing (math -> lang) referencing the same term must not
        // ride free on it.
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                MATH,
                LANG,
                EdgeKind::Mapping,
                &["https://blackcatinformatics.ca/math/Quantity"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(MATH, LANG, EdgeKind::Mapping)],
        };

        let classification = classify(&report, &catalog).unwrap();
        assert_ne!(classification.verdicts[0].coverage, Coverage::Covered);
        assert!(matches!(
            classification.verdicts[0].coverage,
            Coverage::PeeredUnregisteredSeam { .. }
        ));

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "slice-ownership.peered-unregistered-seam");
    }

    #[test]
    fn uncovered_non_peer_undeclared_dependency_stays_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                CORE,
                EXT,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/gmeow/SomeTerm"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(CORE, EXT, EdgeKind::Ontology)],
        };

        let classification = classify(&report, &catalog).unwrap();
        assert_eq!(classification.verdicts[0].coverage, Coverage::Uncovered);

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "slice-ownership.undeclared-dependency");
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn a_semantic_undeclared_diagnostic_with_no_matching_edge_hard_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: Vec::new(),
            diagnostics: vec![undeclared_diag(LANG, LOGIC, EdgeKind::Ontology)],
        };

        let result = classify(&report, &catalog);
        assert!(
            result.is_err(),
            "a semantic UndeclaredDependency diagnostic with no matching edge must hard-fail"
        );
    }

    #[test]
    fn a_non_semantic_edge_kind_diagnostic_is_ignored_even_with_no_matching_edge() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: Vec::new(),
            diagnostics: vec![undeclared_diag(LANG, LOGIC, EdgeKind::Test)],
        };

        let classification =
            classify(&report, &catalog).expect("non-semantic edge kinds are filtered before the join, never hard-failing on a missing edge");
        assert!(classification.verdicts.is_empty());

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn seam_records_of_reads_directions_and_raw_term_iris() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_catalog(tmp.path());
        let seams = seam_registry(&catalog).unwrap();
        let test_seam = seams
            .iter()
            .find(|s| s.name == "Test seam")
            .expect("test-seam present");
        assert_eq!(
            test_seam.directions,
            vec![(LANG.to_string(), LOGIC.to_string())]
        );
        assert!(
            test_seam
                .carrying_term_iris
                .contains("https://blackcatinformatics.ca/logic/Foo")
        );
        assert!(test_seam.carrying_terms.contains("logic:Foo"));
    }

    // ── Class 1 / Class 2 / Class 3 genuine-crossing-term exclusion ──────────

    const QUALITY: &str = "https://blackcatinformatics.ca/gmeow/slices/quality";
    const WIDGETS: &str = "https://blackcatinformatics.ca/gmeow/slices/widgets";
    const GUIDES: &str = "https://blackcatinformatics.ca/gmeow/slices/guides";

    /// A catalog with `logic:` (a lone, seam-free grounding slice — the
    /// peerage/seam machinery is irrelevant to these tests) plus three
    /// ordinary domain slices, `quality`, `widgets`, and `guides`. Dedicated
    /// to the Class 1 (slice-IRI-as-data) / Class 2
    /// (grounding-meta-formalization) / Class 3 (`gmeow:usesTerm`
    /// documentation indexing) exclusion tests, which need REAL artifact
    /// content parsed off disk (`slice_iris` and [`ReferencePredicateIndex`]
    /// re-parse the catalog directly) — a hand-built [`OwnershipReport`]
    /// fixture alone can never exercise them. `guides` is deliberately a
    /// PLAIN (non-`GroundingSlice`) domain slice, proving the `gmeow:usesTerm`
    /// exclusion applies from ANY slice, not only the three grounding ones.
    fn class_filter_catalog(root: &Path) -> SliceCatalog {
        write_manifest(
            root,
            "grounding",
            "logic",
            r#"<https://blackcatinformatics.ca/gmeow/slices/logic>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "logic" .
            "#,
        );
        write_module(
            root,
            "grounding",
            "logic",
            r#"logic:widgetFacetCharacteristic
                a owl:NamedIndividual , logic:PropertyCharacteristicAssertion ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> ;
                logic:characterizes gmeow:widgetFacet ;
                logic:formalizes gmeow:widgetFacet .

            logic:widgetOtherTermUsage
                a logic:Formula ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> ;
                logic:relation gmeow:widgetOtherTerm .

            logic:widgetBothTermUsage
                a logic:Formula ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> ;
                logic:relation gmeow:widgetBothTerm ;
                logic:formalizes gmeow:widgetBothTerm .
            "#,
        );
        write_manifest(
            root,
            "core",
            "quality",
            r#"<https://blackcatinformatics.ca/gmeow/slices/quality>
                a gmeow:Slice ;
                rdfs:label "quality" .
            "#,
        );
        write_manifest(
            root,
            "core",
            "widgets",
            r#"<https://blackcatinformatics.ca/gmeow/slices/widgets>
                a gmeow:Slice ;
                rdfs:label "widgets" .
            "#,
        );
        write_mapping(
            root,
            "core",
            "quality",
            "widgets-correspondences.ttl",
            r#"gmeow:qualityRelatedMatchOnlyTerm
                a rdfs:Resource ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/quality> ;
                skos:relatedMatch gmeow:widgetRelatedMatchOnlyTerm .

            gmeow:qualityRelatedMatchAndStructuralTerm
                a rdfs:Resource ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/quality> ;
                skos:relatedMatch gmeow:widgetRelatedMatchAndStructuralTerm ;
                gmeow:relatedTerm gmeow:widgetRelatedMatchAndStructuralTerm .
            "#,
        );
        write_manifest(
            root,
            "core",
            "guides",
            r#"<https://blackcatinformatics.ca/gmeow/slices/guides>
                a gmeow:Slice ;
                rdfs:label "guides" .
            "#,
        );
        write_module(
            root,
            "core",
            "guides",
            r#"gmeow:guideWidgetDocOnly
                a gmeow:Recipe ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/guides> ;
                gmeow:usesTerm gmeow:widgetDocOnlyTerm .

            gmeow:guideWidgetDocAndReal
                a gmeow:Recipe ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/guides> ;
                gmeow:usesTerm gmeow:widgetDocAndRealTerm ;
                gmeow:relatedTerm gmeow:widgetDocAndRealTerm .
            "#,
        );
        SliceCatalog::discover(&root.join("slices"), vocab()).unwrap()
    }

    #[test]
    fn class_1_slice_iri_as_data_crossing_is_suppressed() {
        // `quality`'s module references `widgets`' own SLICE IRI as DATA (the
        // real-world `slice-quality-rubric` `gmeow:ceilingSlice
        // <…/slices/widgets>` shape) — never one of `widgets`' vocabulary
        // terms, so this must never surface as a dependency at all.
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                QUALITY,
                WIDGETS,
                EdgeKind::Ontology,
                &[WIDGETS],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Ontology)],
        };

        let classification = classify(&report, &catalog).expect("classify must not hard-fail");
        assert!(
            classification.verdicts.is_empty(),
            "a slice-IRI-as-data crossing has zero genuine terms and must be suppressed \
             entirely: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            findings.is_empty(),
            "a slice-IRI-as-data crossing must never fire a finding: {findings:?}"
        );
    }

    #[test]
    fn class_2_grounding_meta_formalization_crossing_is_suppressed() {
        // `logic:`'s module names `gmeow:widgetFacet` ONLY via
        // `logic:characterizes`/`logic:formalizes` — a pure law/formalization
        // back-reference (the `logic:assertionFacet` real-world pattern),
        // never an object-level use of `widgets`' term.
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                LOGIC,
                WIDGETS,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/gmeow/widgetFacet"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(LOGIC, WIDGETS, EdgeKind::Ontology)],
        };

        let classification = classify(&report, &catalog).expect("classify must not hard-fail");
        assert!(
            classification.verdicts.is_empty(),
            "a pure grounding-meta-formalization crossing has zero genuine terms and must be \
             suppressed entirely: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            findings.is_empty(),
            "a pure grounding-meta-formalization crossing must never fire a finding: {findings:?}"
        );
    }

    #[test]
    fn genuine_term_use_from_a_grounding_slice_still_fires() {
        // `logic:`'s module names `gmeow:widgetOtherTerm` via `logic:relation`
        // — a REAL `logic:Formula` predication (e.g.
        // `logic:coverEntitySortals`'s class-covering pattern), not a
        // law/formalization back-reference — so this crossing is genuine.
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                LOGIC,
                WIDGETS,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/gmeow/widgetOtherTerm"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(LOGIC, WIDGETS, EdgeKind::Ontology)],
        };

        let classification = classify(&report, &catalog).unwrap();
        assert_eq!(classification.verdicts.len(), 1);
        assert_eq!(classification.verdicts[0].coverage, Coverage::Uncovered);

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, "slice-ownership.undeclared-dependency");
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn a_term_named_via_both_a_meta_predicate_and_a_real_use_is_never_excluded() {
        // `logic:`'s module names `gmeow:widgetBothTerm` via BOTH
        // `logic:formalizes` (meta) AND `logic:relation` (a real `logic:Formula`
        // predication) — per spec, co-presence of a real use means the term is
        // NEVER excluded, even though a meta predicate also names it.
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                LOGIC,
                WIDGETS,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/gmeow/widgetBothTerm"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(LOGIC, WIDGETS, EdgeKind::Ontology)],
        };

        let classification = classify(&report, &catalog).unwrap();
        assert_eq!(
            classification.verdicts.len(),
            1,
            "a term with a genuine co-use must never be suppressed: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, "slice-ownership.undeclared-dependency");
    }

    #[test]
    fn class_3_uses_term_documentation_crossing_is_suppressed_from_any_slice() {
        // `guides` (a PLAIN domain slice — not one of the three grounding
        // slices) names `gmeow:widgetDocOnlyTerm` ONLY via `gmeow:usesTerm` —
        // the documentation-index reference a `gmeow:Recipe` makes to "any
        // documented term across any slice" — never a real build dependency
        // on `widgets`, so this crossing must be suppressed exactly like a
        // pure Class 2 grounding-formalization crossing.
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                GUIDES,
                WIDGETS,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/gmeow/widgetDocOnlyTerm"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(GUIDES, WIDGETS, EdgeKind::Ontology)],
        };

        let classification = classify(&report, &catalog).expect("classify must not hard-fail");
        assert!(
            classification.verdicts.is_empty(),
            "a pure gmeow:usesTerm documentation crossing has zero genuine terms and must be \
             suppressed entirely: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            findings.is_empty(),
            "a pure gmeow:usesTerm documentation crossing must never fire a finding: {findings:?}"
        );
    }

    #[test]
    fn a_term_named_via_both_uses_term_and_a_real_use_is_never_excluded() {
        // `guides`' module names `gmeow:widgetDocAndRealTerm` via BOTH
        // `gmeow:usesTerm` (documentation index) AND `gmeow:relatedTerm` (a
        // stand-in for a real, non-meta object-level predication) — per spec,
        // co-presence of a real use means the term is NEVER excluded, even
        // though `gmeow:usesTerm` also names it.
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                GUIDES,
                WIDGETS,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/gmeow/widgetDocAndRealTerm"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(GUIDES, WIDGETS, EdgeKind::Ontology)],
        };

        let classification = classify(&report, &catalog).unwrap();
        assert_eq!(
            classification.verdicts.len(),
            1,
            "a term with a genuine co-use alongside gmeow:usesTerm must never be suppressed: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, "slice-ownership.undeclared-dependency");
    }

    // ── Class 4 (competency query) / Class 5 (skos:relatedMatch) exclusion ────

    #[test]
    fn class_4_competency_query_only_crossing_is_suppressed() {
        // The crossing's ONLY evidence is a `queries/competency/*.rq`
        // artifact — an integration test over the composed ontology, never a
        // build dependency, so this must never surface as a dependency at
        // all.
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge_with_evidence(
                QUALITY,
                WIDGETS,
                EdgeKind::Query,
                vec![EdgeEvidence {
                    from_artifact: artifact_evidence_with(
                        QUALITY,
                        ArtifactRole::CompetencyQuery,
                        "queries/competency/can-quality-answer.rq",
                    ),
                    referenced_term: nn("https://blackcatinformatics.ca/gmeow/widgetCompetencyTerm"),
                }],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Query)],
        };

        let classification = classify(&report, &catalog).expect("classify must not hard-fail");
        assert!(
            classification.verdicts.is_empty(),
            "a competency-query-only crossing has zero genuine terms and must be suppressed \
             entirely: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            findings.is_empty(),
            "a competency-query-only crossing must never fire a finding: {findings:?}"
        );
    }

    #[test]
    fn a_term_named_via_a_competency_query_and_a_genuine_artifact_is_never_excluded() {
        // The SAME referenced term is named via BOTH a competency-query
        // artifact AND a genuine (`Module`) artifact — per spec, the
        // competency-query exclusion is per-evidence-entry, so co-presence of
        // a genuine reference means the crossing still counts.
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let term = "https://blackcatinformatics.ca/gmeow/widgetCompetencyAndRealTerm";
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge_with_evidence(
                QUALITY,
                WIDGETS,
                EdgeKind::Query,
                vec![
                    EdgeEvidence {
                        from_artifact: artifact_evidence_with(
                            QUALITY,
                            ArtifactRole::CompetencyQuery,
                            "queries/competency/can-quality-answer.rq",
                        ),
                        referenced_term: nn(term),
                    },
                    EdgeEvidence {
                        from_artifact: artifact_evidence(QUALITY),
                        referenced_term: nn(term),
                    },
                ],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Query)],
        };

        let classification = classify(&report, &catalog).unwrap();
        assert_eq!(
            classification.verdicts.len(),
            1,
            "a term also referenced by a genuine non-competency-query artifact must never be \
             suppressed: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, "slice-ownership.undeclared-dependency");
    }

    #[test]
    fn class_5_internal_related_match_only_crossing_is_suppressed() {
        // `quality`'s mapping names `gmeow:widgetRelatedMatchOnlyTerm` ONLY
        // via `skos:relatedMatch` — a soft internal correspondence, never a
        // build dependency.
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge_with_evidence(
                QUALITY,
                WIDGETS,
                EdgeKind::Mapping,
                vec![EdgeEvidence {
                    from_artifact: artifact_evidence_with(
                        QUALITY,
                        ArtifactRole::Mapping,
                        "mappings/widgets-correspondences.ttl",
                    ),
                    referenced_term: nn(
                        "https://blackcatinformatics.ca/gmeow/widgetRelatedMatchOnlyTerm",
                    ),
                }],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Mapping)],
        };

        let classification = classify(&report, &catalog).expect("classify must not hard-fail");
        assert!(
            classification.verdicts.is_empty(),
            "a pure internal skos:relatedMatch crossing has zero genuine terms and must be \
             suppressed entirely: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            findings.is_empty(),
            "a pure internal skos:relatedMatch crossing must never fire a finding: {findings:?}"
        );
    }

    #[test]
    fn a_term_named_via_related_match_and_a_structural_use_is_never_excluded() {
        // `quality`'s mapping names `gmeow:widgetRelatedMatchAndStructuralTerm`
        // via BOTH `skos:relatedMatch` (correspondence) AND `gmeow:relatedTerm`
        // (a stand-in for a real, non-meta object-level predication in the
        // SAME artifact) — per spec, co-presence of a structural use means the
        // term is NEVER excluded, even though `skos:relatedMatch` also names
        // it.
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge_with_evidence(
                QUALITY,
                WIDGETS,
                EdgeKind::Mapping,
                vec![EdgeEvidence {
                    from_artifact: artifact_evidence_with(
                        QUALITY,
                        ArtifactRole::Mapping,
                        "mappings/widgets-correspondences.ttl",
                    ),
                    referenced_term: nn(
                        "https://blackcatinformatics.ca/gmeow/widgetRelatedMatchAndStructuralTerm",
                    ),
                }],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Mapping)],
        };

        let classification = classify(&report, &catalog).unwrap();
        assert_eq!(
            classification.verdicts.len(),
            1,
            "a term with a genuine structural co-use must never be suppressed: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, "slice-ownership.undeclared-dependency");
    }

    // ── R5: tier-forbidden edges ──────────────────────────────────────────────

    const TIER_CORE_SLICE: &str = "https://blackcatinformatics.ca/gmeow/slices/tier-core";
    const TIER_EXT_A: &str = "https://blackcatinformatics.ca/gmeow/slices/tier-ext-a";
    const TIER_EXT_B: &str = "https://blackcatinformatics.ca/gmeow/slices/tier-ext-b";

    /// A catalog with one `tierCore` slice and two `tierExtension` slices —
    /// dedicated to the R5 forbidden-tier gate so it never shares (and can
    /// never accidentally perturb) `grounding_catalog`'s tierless CORE/EXT
    /// fixture the peerage-coverage tests above depend on.
    fn tier_catalog(root: &Path) -> SliceCatalog {
        write_manifest(
            root,
            "core",
            "tier-core",
            r#"<https://blackcatinformatics.ca/gmeow/slices/tier-core>
                a gmeow:Slice ;
                rdfs:label "tier-core" ;
                gmeow:sliceTier gmeow:tierCore .
            "#,
        );
        write_manifest(
            root,
            "extensions",
            "tier-ext-a",
            r#"<https://blackcatinformatics.ca/gmeow/slices/tier-ext-a>
                a gmeow:Slice ;
                rdfs:label "tier-ext-a" ;
                gmeow:sliceTier gmeow:tierExtension .
            "#,
        );
        write_manifest(
            root,
            "extensions",
            "tier-ext-b",
            r#"<https://blackcatinformatics.ca/gmeow/slices/tier-ext-b>
                a gmeow:Slice ;
                rdfs:label "tier-ext-b" ;
                gmeow:sliceTier gmeow:tierExtension .
            "#,
        );
        SliceCatalog::discover(&root.join("slices"), vocab()).unwrap()
    }

    #[test]
    fn core_depending_on_extension_is_forbidden() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = tier_catalog(tmp.path());
        // A MATCHED (authored) edge — declaring it does not license it.
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                TIER_CORE_SLICE,
                TIER_EXT_A,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/gmeow/SomeTerm"],
                ReconciliationStatus::Matched,
            )],
            diagnostics: Vec::new(),
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY);
        assert_eq!(findings[0].severity, Severity::Error);
        assert!(findings[0].message.contains(TIER_CORE_SLICE));
        assert!(findings[0].message.contains(TIER_EXT_A));
    }

    #[test]
    fn extension_depending_on_another_extension_is_forbidden() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = tier_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                TIER_EXT_A,
                TIER_EXT_B,
                EdgeKind::Mapping,
                &["https://blackcatinformatics.ca/gmeow/OtherTerm"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(TIER_EXT_A, TIER_EXT_B, EdgeKind::Mapping)],
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        let forbidden: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY)
            .collect();
        assert_eq!(forbidden.len(), 1, "{findings:?}");
        assert_eq!(forbidden[0].severity, Severity::Error);
        // The ordinary undeclared-dependency observation ALSO fires — a
        // forbidden tier crossing is an additional, independent violation, not
        // a replacement for the undeclared-dependency finding.
        assert!(
            findings
                .iter()
                .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY)
        );
    }

    #[test]
    fn extension_depending_on_core_is_not_forbidden() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = tier_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                TIER_EXT_A,
                TIER_CORE_SLICE,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/gmeow/SomeTerm"],
                ReconciliationStatus::Matched,
            )],
            diagnostics: Vec::new(),
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            findings.is_empty(),
            "extension -> core is the ordinary direction, never forbidden: {findings:?}"
        );
    }
}
