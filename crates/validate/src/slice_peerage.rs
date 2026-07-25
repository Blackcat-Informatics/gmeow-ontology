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
//! # R6: grounding doctrine (a grounding slice never consumes a grounding concept downward)
//!
//! [`peerage_aware_ownership_findings`] ALSO folds in
//! [`grounding_doctrine_findings`]: `docs/GROUNDING.md`'s **tier rule** — "a
//! grounding slice never depends on a non-grounding slice **for a grounding
//! concept**" — is invisible to [`is_forbidden_edge`], because all three
//! grounding slices are `gmeow:tierCore`, so `logic → cognition` reads as an
//! ordinary core→core crossing. This gate keys on the `gmeow:GroundingSlice`
//! marker plus the referenced TERM's authored `gmeow:groundingConceptDomain`
//! marker: a grounding slice must not reference a term that is declared a
//! grounding concept while a non-grounding slice owns it.
//!
//! The "for a grounding concept" qualifier is load-bearing. A grounding slice
//! consuming ordinary domain vocabulary by reference is sanctioned — `lang:`
//! subclasses `gmeow:AttestationArtifact` precisely so it need not re-mint the
//! attestation vocabulary, and `logic:` names domain predicates inside
//! `logic:Formula` ASTs because formalizing slice vocabulary is what `logic:`
//! is for. Dropping the qualifier makes both violations and admits only a
//! corpus in which every formalized term has been swallowed into `logic:`.
//! Which terms ARE grounding concepts is a judgment about subject matter that
//! no graph shape yields, so it is authored as ontology data on the term
//! ([`GroundingConceptIndex`]), never as a list in this file.
//! Grounding→grounding peer crossings are exactly the Principle 19 peerage
//! grant above and never fire here.
//!
//! # Genuine cross-slice TERM usage only
//!
//! `purrdf`'s [`OwnershipReport::edges`] mines EVERY IRI in an artifact (subject,
//! predicate, object, datatype, graph — RFC §10) that happens to be a
//! validated-owned term of some other slice, so an edge's raw evidence
//! over-counts two shapes that are not genuine term USAGE at all. Both filters
//! are **derived from the corpus's own declarations** — never a hand-maintained
//! IRI allow-list, which is how a gate gets quietly tuned until it is green:
//!
//! * **Class A — slice-IRI-as-data.** A slice referencing another slice's own
//!   IRI as DATA (e.g. `slice-quality-rubric`'s ABox quality records naming
//!   `gmeow:ceilingSlice`/`gmeow:floorSlice <…/slices/norms>` — the assessment
//!   TARGET, not one of `norms`' vocabulary terms). This is REACHABLE, not
//!   dead: every slice `module.ttl` declares its own slice IRI as the module's
//!   `owl:Ontology` header carrying `rdfs:isDefinedBy <itself>`, and that IRI
//!   sits inside the `gmeow:` vocab namespace, so `purrdf`'s Phase-1
//!   `is_defined_by` harvest (`subject.starts_with(vocab_ns)`) admits it, and
//!   Phase 2 validates it (physical origin == declared owner). It therefore
//!   enters `validated_owner` and produces real edges. [`slice_iris`] collects
//!   the closed set of every catalogued slice's own IRI off the catalog; a
//!   crossing whose `referenced_term` IS one of those IRIs names the module
//!   header, never a vocabulary term.
//! * **Class B — non-coupling predicates.** A term named EXCLUSIVELY through a
//!   predicate the ontology ITSELF declares as carrying no object-level,
//!   slice-coupling force. [`NonCouplingPredicates`] reads that set out of the
//!   corpus at runtime, by exactly two declaration tests:
//!   1. **`owl:AnnotationProperty`.** By OWL 2 semantics an annotation
//!      contributes no logical axiom, so it can never make its object's slice
//!      a build dependency. `logic:formalizes` / `math:formalizes` self-document
//!      this VERBATIM — "an annotation property, never a reasoned axiom, so it
//!      carries no DL or EL profile weight".
//!   2. **`rdfs:range rdfs:Resource`.** An explicitly, authoredly OPEN range is
//!      the declaration's own statement that the property deliberately declines
//!      to constrain its object to any vocabulary — an index/pointer, not a
//!      typed structural link. `gmeow:usesTerm` self-documents this VERBATIM —
//!      "the range is left open (`rdfs:Resource`) because a guide may point at
//!      any documented term across any slice".
//!
//!   The test is a property of the PREDICATE, not of the authoring slice, so it
//!   is deliberately NOT scoped to the grounding slices: an annotation carries
//!   the same (zero) logical weight whoever writes it, and `logic:formalizes`
//!   is in fact authored from ~50 non-grounding `module.ttl`s. Scoping the
//!   exclusion to grounding authors would make the identical triple genuine in
//!   one file and meta in another, which is not a semantics.
//!
//!   Symmetric by construction: a term ALSO named via any predicate outside the
//!   derived set (a real object-level use) is NEVER excluded. A predicate with
//!   no corpus declaration at all — every external vocabulary predicate, e.g.
//!   `skos:relatedMatch` — passes no test and is therefore always coupling; the
//!   no-optionality-safe default is GENUINE.
//!
//! [`classify`], [`forbidden_tier_findings`] and [`grounding_doctrine_findings`]
//! all apply [`is_genuine_crossing_term`] to every piece of an edge's evidence;
//! an edge left with zero genuine crossing terms is suppressed entirely — this
//! is a filter on WHICH terms count as a crossing, never a license to skip an
//! edge that also carries genuine evidence. A DECLARED `gmeow:sliceDependsOn`
//! edge needs no evidence at all: it is judged by [`forbidden_tier_findings`]
//! and [`grounding_doctrine_findings`] on the declaration alone, because
//! declaring an architecturally illegal crossing is a pure authoring defect.

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
/// `gmeow:GroundingDomain` — one of the three external-grounding subject-matter
/// domains of `docs/GROUNDING.md`'s "External grounding ownership" table.
const GMEOW_GROUNDING_DOMAIN: &str = "https://blackcatinformatics.ca/gmeow/GroundingDomain";
/// `gmeow:groundingDomainOwner` — the grounding slice a domain's concepts belong to.
const GMEOW_GROUNDING_DOMAIN_OWNER: &str =
    "https://blackcatinformatics.ca/gmeow/groundingDomainOwner";
/// `gmeow:groundingConceptDomain` — the authored marker naming a term as a
/// GROUNDING CONCEPT and placing it in one grounding domain.
const GMEOW_GROUNDING_CONCEPT_DOMAIN: &str =
    "https://blackcatinformatics.ca/gmeow/groundingConceptDomain";
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
/// slice's `manifest.ttl` — the single reader the R7 seam-registry drift gate
/// (`crate::authoring_integrity`), this peerage-coverage engine, and the shipped
/// `graph/grounding-seams` bundle graph
/// (`crates/pipeline/src/stages/carrier.rs::grounding_seams_turtle`) all share.
///
/// The field set is LOSSLESS over the authored registry: label (with its language
/// tag), directed legs, carrying terms, and owning docs — so the bundle graph the
/// pipeline emits from these records reconstructs the whole registry, and the gate
/// and the shipped data can never disagree about what a seam says.
/// Field order is load-bearing for the derived [`Ord`]: `iri` leads, so sorting a
/// registry orders it by seam IRI first (what the shipped-graph emitter relies on)
/// while still totalling over the whole record.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SeamRecord {
    /// The seam's own IRI (`a gmeow:Seam` subject).
    pub iri: String,
    /// `rdfs:label` (lexically-lowest, deterministic), falling back to the
    /// seam's CURIE when unlabeled. A convenience projection of the first
    /// [`SeamRecord::labels`] entry — never a second read.
    pub name: String,
    /// EVERY `rdfs:label` of the seam as a `(lexical form, language tag)` pair,
    /// sorted and deduped. Carried in full (not collapsed to [`SeamRecord::name`])
    /// so a re-emission of this record loses no authored label and no language tag.
    pub labels: Vec<(String, Option<String>)>,
    /// `gmeow:seamCarryingTerm` objects, reduced to `family:Local` CURIEs (for
    /// the R7 page-drift text comparison ONLY).
    pub carrying_terms: BTreeSet<String>,
    /// `gmeow:seamCarryingTerm` objects, as raw term IRIs — the exact-IRI set
    /// [`classify`] matches a crossing's referenced term against.
    pub carrying_term_iris: BTreeSet<String>,
    /// `gmeow:seamDirection` legs, each a `(from_slice_iri, to_slice_iri)` pair,
    /// sorted and deduped.
    pub directions: Vec<(String, String)>,
    /// `gmeow:seamOwningDoc` literal values.
    pub owning_docs: BTreeSet<String>,
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
pub fn seam_records_of(ds: &Dataset, path: &Path) -> Result<Vec<SeamRecord>> {
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
        // EVERY authored `rdfs:label`, with its language tag, sorted so the
        // lexically-lowest lexical form leads (the historical, deterministic
        // `name`) and so a re-emission of the record is byte-stable.
        let mut labels: Vec<(String, Option<String>)> = ds
            .objects(&seam_iri, RDFS_LABEL_TERM)
            .map_err(|e| parse_err(path, &e.to_string()))?
            .into_iter()
            .filter_map(|o| match o {
                Object::Literal {
                    value, language, ..
                } => Some((value, language)),
                _ => None,
            })
            .collect();
        labels.sort();
        labels.dedup();
        let name = labels
            .first()
            .map(|(value, _)| value.clone())
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
            labels,
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

/// Every `gmeow:Seam` individual across every grounding manifest in `catalog`,
/// sorted by seam IRI.
///
/// This is the SINGLE catalog-scoped seam reader: [`classify`]'s coverage join
/// reads it, and `crates/pipeline`'s `graph/grounding-seams` emitter reads it to
/// build the shipped registry graph — so the gate and the bundle data can never
/// disagree about what the registry contains. The sort makes the returned order a
/// function of the authored data alone (never of catalog discovery order), which
/// the emitter's byte determinism rests on.
pub fn seam_registry(catalog: &SliceCatalog) -> Result<Vec<SeamRecord>> {
    let mut out = Vec::new();
    for record in catalog.records() {
        let ds = manifest_dataset(record);
        out.extend(seam_records_of(&ds, &record.manifest_path())?);
    }
    out.sort();
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

// ── Class B: corpus-derived non-coupling predicates ──────────────────────────

/// `rdf:type`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `owl:AnnotationProperty` — declaration test 1 (see [`NonCouplingPredicates`]).
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
/// `rdfs:range`.
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
/// `rdfs:Resource` — declaration test 2's open-range marker.
const RDFS_RESOURCE: &str = "http://www.w3.org/2000/01/rdf-schema#Resource";

/// Every predicate the CORPUS ITSELF declares as carrying no object-level,
/// slice-coupling force, read out of the catalogued slices' own
/// `module.ttl`/`shapes.ttl` at runtime. There is deliberately NO hand-written
/// IRI list here: a hard-coded allow-list is exactly how a dependency gate gets
/// tuned to whatever reds happened to be open on the day it was written.
///
/// Membership is decided by exactly two authored-declaration tests, each of
/// which is a statement the ontology makes about the predicate itself:
///
/// 1. **`<p> a owl:AnnotationProperty`.** Under OWL 2 Direct Semantics an
///    annotation assertion contributes no logical axiom, so it cannot make its
///    object's slice a build dependency of the asserting slice. Both
///    `logic:formalizes` and `math:formalizes` are declared this way and
///    self-document it VERBATIM: "an annotation property, never a reasoned
///    axiom, so it carries no DL or EL profile weight."
/// 2. **`<p> rdfs:range rdfs:Resource`.** An explicitly authored OPEN range is
///    the declaration's own statement that the property deliberately declines
///    to constrain its object to any vocabulary — a documentation index or
///    pointer, not a typed structural link. `gmeow:usesTerm` is declared this
///    way and self-documents it VERBATIM: "the range is left open
///    (`rdfs:Resource`) because a guide may point at any documented term across
///    any slice."
///
/// A predicate the corpus does not declare at all (every purely external
/// vocabulary predicate, e.g. `skos:relatedMatch`, `rdfs:subClassOf`,
/// `owl:onProperty`) satisfies neither test and is therefore always coupling —
/// the no-optionality-safe default is GENUINE.
///
/// Note what this deliberately does NOT do: it does not scope membership to the
/// grounding slices. The tests above are properties of the PREDICATE; an
/// annotation carries the same (zero) logical weight whoever authors it, and
/// `logic:formalizes` is in fact authored from ~50 non-grounding `module.ttl`s.
/// Scoping by author would make the identical triple meta in one file and
/// genuine in another, which is not a semantics — and would be a second,
/// undeclared source of truth about what an annotation means.
#[derive(Debug, Default)]
struct NonCouplingPredicates {
    /// Predicates declared `a owl:AnnotationProperty` anywhere in the corpus.
    annotation: BTreeSet<String>,
    /// Predicates declared `rdfs:range rdfs:Resource` anywhere in the corpus.
    open_range: BTreeSet<String>,
}

impl NonCouplingPredicates {
    /// Whether `predicate` passes either declaration test.
    fn contains(&self, predicate: &str) -> bool {
        self.annotation.contains(predicate) || self.open_range.contains(predicate)
    }

    /// Harvest both declaration tests from one already-parsed artifact graph.
    fn absorb(&mut self, ds: &Dataset) {
        ds.for_each_quad(|s, p, o, _g| {
            let Subject::Named(subject) = s else {
                return;
            };
            let Object::Named(object) = o else {
                return;
            };
            if p == RDF_TYPE && object == OWL_ANNOTATION_PROPERTY {
                self.annotation.insert(subject);
            } else if p == RDFS_RANGE && object == RDFS_RESOURCE {
                self.open_range.insert(subject);
            }
        });
    }
}

/// Every predicate by which an artifact references a term IRI as an object,
/// keyed by term.
type TermPredicates = BTreeMap<String, BTreeSet<String>>;

/// Per-artifact, the set of predicates by which that artifact references each
/// term IRI as the OBJECT of a triple, plus the corpus-derived
/// [`NonCouplingPredicates`] set — built ONCE per catalog, over EVERY
/// catalogued slice, so [`is_genuine_crossing_term`]'s Class B check never
/// re-parses an artifact per crossing.
#[derive(Debug, Default)]
struct ReferencePredicateIndex {
    /// `(slice IRI, artifact logical path)` -> term IRI -> the set of
    /// predicates that reference it as an object anywhere in that artifact.
    by_artifact: BTreeMap<(SliceIri, String), TermPredicates>,
    /// The corpus's own declaration of which predicates do not couple slices.
    non_coupling: NonCouplingPredicates,
}

impl ReferencePredicateIndex {
    /// Absorb one already-parsed artifact graph: the per-artifact reference
    /// predicates and the corpus-wide [`NonCouplingPredicates`] declarations.
    fn absorb_artifact(&mut self, slice: &SliceIri, logical_path: &str, ds: &Dataset) {
        self.non_coupling.absorb(ds);
        let mut term_predicates: TermPredicates = BTreeMap::new();
        ds.for_each_quad(|_s, p, o, _g| {
            if let Object::Named(iri) = o {
                term_predicates
                    .entry(iri)
                    .or_default()
                    .insert(p.to_string());
            }
        });
        self.by_artifact
            .insert((slice.clone(), logical_path.to_string()), term_predicates);
    }

    /// Whether `term`, as referenced from `from_slice`'s `logical_path`
    /// artifact, is named EXCLUSIVELY via corpus-declared
    /// [`NonCouplingPredicates`] — i.e. every triple in that artifact whose
    /// object is `term` uses a predicate the ontology itself declares as an
    /// `owl:AnnotationProperty` or as having an explicitly open
    /// `rdfs:range rdfs:Resource`.
    ///
    /// `false` (never non-coupling; the no-optionality-safe default is GENUINE)
    /// when the artifact was not indexed (a non-RDF role, e.g. a
    /// `queries/**/*.rq` `Query` edge) or `term` was never seen as an object at
    /// all in that artifact.
    fn is_pure_non_coupling(&self, from_slice: &str, logical_path: &str, term: &str) -> bool {
        let Some(term_predicates) = self
            .by_artifact
            .get(&(from_slice.to_string(), logical_path.to_string()))
        else {
            return false;
        };
        let Some(preds) = term_predicates.get(term).filter(|preds| !preds.is_empty()) else {
            return false;
        };
        preds.iter().all(|p| self.non_coupling.contains(p))
    }
}

// ── Grounding concepts: the authored subject-matter judgment ─────────────────

/// One `gmeow:GroundingDomain` individual: one of the three external-grounding
/// subject-matter domains of `docs/GROUNDING.md`'s "External grounding
/// ownership" table, read off the grounding manifest that declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GroundingDomainRecord {
    /// The domain individual's own IRI.
    iri: String,
    /// Its `rdfs:label` — the domain named in a finding message. Falls back to
    /// the IRI's CURIE when unlabelled, never to a hard-coded English string.
    label: String,
    /// The grounding slice `gmeow:groundingDomainOwner` names as the sole owner
    /// of every grounding concept in this domain.
    owner: SliceIri,
}

/// Which terms the corpus DECLARES to be grounding concepts, and in which
/// grounding domain.
///
/// `docs/GROUNDING.md`'s tier rule is qualified — "a grounding slice never
/// depends on a non-grounding slice **for a grounding concept**" — and whether
/// a term's subject matter is linguistic, mathematical or logical is a
/// judgment about MEANING that no graph shape can decide. It is therefore
/// authored as DATA on the term (`gmeow:groundingConceptDomain`), exactly like
/// the seam registry and the grounding-slice marker, and read back here. There
/// is deliberately no hard-coded IRI list: a gate that carries its own opinion
/// about which concepts are foundational is a second source of truth, and one
/// nobody can amend by authoring ontology.
///
/// Domains come from the `gmeow:GroundingSlice` manifests (the same place the
/// `gmeow:Seam` registry lives); term markers come from the object-level
/// artifacts, wherever the marked term physically lives — so a domain slice
/// that mints a grounding concept and honestly marks it is caught, and so is
/// the term after it has been promoted (the marker travels with the block).
#[derive(Debug, Default)]
struct GroundingConceptIndex {
    /// Domain IRI -> its record. Read from grounding manifests.
    domains: BTreeMap<String, GroundingDomainRecord>,
    /// Term IRI -> the domain IRI its authored marker names.
    term_domain: BTreeMap<String, String>,
}

impl GroundingConceptIndex {
    /// Absorb every `<term> gmeow:groundingConceptDomain <domain>` marker in one
    /// already-parsed artifact graph.
    fn absorb_markers(&mut self, ds: &Dataset) {
        ds.for_each_quad(|s, p, o, _g| {
            if p != GMEOW_GROUNDING_CONCEPT_DOMAIN {
                return;
            }
            let (Subject::Named(term), Object::Named(domain)) = (s, o) else {
                return;
            };
            self.term_domain.insert(term, domain);
        });
    }

    /// Absorb every `gmeow:GroundingDomain` individual declared in one manifest
    /// graph. A domain with no `gmeow:groundingDomainOwner` is skipped rather
    /// than guessed at: the owner is what makes the reconciliation direction
    /// nameable, and the R6 finding below states it.
    fn absorb_domains(&mut self, ds: &Dataset, path: &Path) -> Result<()> {
        for iri in ds
            .subjects_of_type(GMEOW_GROUNDING_DOMAIN)
            .map_err(|e| parse_err(path, &e.to_string()))?
        {
            let Some(owner) = ds
                .object_iris(&iri, GMEOW_GROUNDING_DOMAIN_OWNER)
                .map_err(|e| parse_err(path, &e.to_string()))?
                .into_iter()
                .min()
            else {
                continue;
            };
            let label = ds
                .objects(&iri, RDFS_LABEL_TERM)
                .map_err(|e| parse_err(path, &e.to_string()))?
                .into_iter()
                .filter_map(|o| match o {
                    Object::Literal { value, .. } => Some(value),
                    _ => None,
                })
                .min()
                .unwrap_or_else(|| seam_term_curie(&iri));
            self.domains
                .insert(iri.clone(), GroundingDomainRecord { iri, label, owner });
        }
        Ok(())
    }

    /// The domain record `term`'s authored marker places it in, or `None` when
    /// `term` carries no marker (ordinary domain vocabulary — the case the
    /// unqualified reading of the tier rule got wrong).
    ///
    /// A marker naming a domain no grounding manifest declares resolves to
    /// `None`: an undeclared domain has no owner, so there is no reconciliation
    /// direction to state. That combination is itself caught, as an
    /// `authoring.undeclared-term` finding on the dangling domain IRI.
    fn domain_of(&self, term: &str) -> Option<&GroundingDomainRecord> {
        self.domains.get(self.term_domain.get(term)?)
    }
}

/// Everything the architectural gates read out of the catalogued artifacts,
/// harvested in ONE parse of each `Module`/`Shapes`/`Mapping` artifact (the
/// three RDF-parseable roles that produce a *semantic* dependency edge — RFC
/// §10) plus one read of each manifest's frozen graph.
///
/// Both consumers ([`classify`] and [`peerage_aware_ownership_findings`]) build
/// exactly one of these, so no artifact is ever parsed twice per gate run.
struct CorpusIndex {
    /// Class-B evidence filtering (see [`is_genuine_crossing_term`]).
    reference_predicates: ReferencePredicateIndex,
    /// The authored grounding-concept judgment R6 is keyed on.
    grounding_concepts: GroundingConceptIndex,
}

impl CorpusIndex {
    /// Parse every semantic artifact once, feeding both indexes, then read the
    /// grounding-domain declarations off the already-parsed manifest graphs.
    fn build(catalog: &SliceCatalog) -> Result<Self> {
        let mut reference_predicates = ReferencePredicateIndex::default();
        let mut grounding_concepts = GroundingConceptIndex::default();
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
                reference_predicates.absorb_artifact(
                    &record.manifest.slice_iri,
                    &artifact.logical_path,
                    &ds,
                );
                grounding_concepts.absorb_markers(&ds);
            }
            let manifest = manifest_dataset(record);
            grounding_concepts.absorb_domains(&manifest, &record.manifest_path())?;
        }
        Ok(Self {
            reference_predicates,
            grounding_concepts,
        })
    }
}

/// Whether `term` (referenced from `from_slice`'s `from_artifact_logical_path`
/// artifact) is GENUINE cross-slice term usage — neither:
///
/// * **Class A** — the raw IRI of some other catalogued slice (its
///   `module.ttl`'s `owl:Ontology` header), cited as DATA; nor
/// * **Class B** — a term named EXCLUSIVELY via corpus-declared
///   [`NonCouplingPredicates`].
///
/// Used to filter an edge's evidence before it can produce an
/// undeclared/forbidden/grounding-downward/peered-unregistered-seam finding.
fn is_genuine_crossing_term(
    from_slice: &SliceIri,
    from_artifact_logical_path: &str,
    term: &NamedNode,
    slice_iris: &BTreeSet<SliceIri>,
    reference_predicates: &ReferencePredicateIndex,
) -> bool {
    if slice_iris.contains(term.as_str()) {
        return false;
    }
    !reference_predicates.is_pure_non_coupling(
        from_slice,
        from_artifact_logical_path,
        term.as_str(),
    )
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
    /// Every referenced term on the edge that survived
    /// [`is_genuine_crossing_term`] — the exact set the coverage verdict was
    /// computed over, sorted and deduped, and never empty (an edge with zero
    /// genuine terms produces no verdict at all).
    ///
    /// Carried so the verdict EXPLAINS ITSELF: a dependency gate that reports
    /// only a from/to pair cannot be audited against the corpus, and the whole
    /// failure mode this engine guards against is a filter quietly deciding
    /// nothing crossed. A reader can join these IRIs straight back to
    /// `DependencyEdge::evidence` to recover the naming artifacts.
    pub genuine_terms: Vec<NamedNode>,
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
    let corpus = CorpusIndex::build(catalog)?;
    let reference_predicates = &corpus.reference_predicates;

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
        // before anything else: neither Class A (the raw IRI of some other
        // catalogued slice's module header, cited as data) nor Class B (a term
        // named exclusively via corpus-declared non-coupling predicates). An
        // edge left with zero genuine crossing terms is not a real dependency
        // at all — suppressed entirely, never even reaching the peerage/seam
        // classification below.
        let mut genuine_terms: Vec<&NamedNode> = edge
            .evidence
            .iter()
            .filter(|e| {
                is_genuine_crossing_term(
                    from_slice,
                    &e.from_artifact.logical_path,
                    &e.referenced_term,
                    &all_slice_iris,
                    reference_predicates,
                )
            })
            .map(|e| &e.referenced_term)
            .collect();
        genuine_terms.sort();
        genuine_terms.dedup();
        if genuine_terms.is_empty() {
            continue;
        }
        let genuine_term_list: Vec<NamedNode> =
            genuine_terms.iter().map(|t| (*t).clone()).collect();

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
            genuine_terms: genuine_term_list,
            coverage,
        });
    }

    Ok(PeerageClassification {
        verdicts,
        crossings,
    })
}

// ── Crossing table (shared by R5 + R6) ───────────────────────────────────────

/// How one `(from_slice, to_slice)` crossing is witnessed: by an authored
/// `gmeow:sliceDependsOn` declaration, by computed term-usage evidence, or
/// both. Both witnesses are independently sufficient for the architectural
/// gates below — a DECLARED illegal crossing is a pure authoring defect that
/// needs no evidence at all, and a COMPUTED illegal crossing is a real coupling
/// whether or not anyone declared it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct CrossingWitness {
    /// The slice's `manifest.ttl` authors `gmeow:sliceDependsOn <to>`.
    declared: bool,
    /// The [`EdgeKind`]s of every computed semantic edge carrying at least one
    /// genuine crossing term.
    computed_kinds: BTreeSet<EdgeKind>,
    /// Every referenced term (owned by `to`) that survived
    /// [`is_genuine_crossing_term`] on any computed semantic edge of this
    /// crossing, sorted by IRI. Empty for a declaration-only crossing.
    ///
    /// Carried because [`grounding_doctrine_findings`] judges the tier rule
    /// PER TERM — `docs/GROUNDING.md` forbids a grounding slice depending on a
    /// non-grounding slice *for a grounding concept*, not in general — so the
    /// slice pair alone is not enough to decide the verdict.
    terms: BTreeSet<NamedNode>,
}

impl CrossingWitness {
    /// Human-readable witness list for a finding message, e.g.
    /// `declared gmeow:sliceDependsOn; computed Ontology, Shape`.
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.declared {
            parts.push("declared gmeow:sliceDependsOn".to_string());
        }
        if !self.computed_kinds.is_empty() {
            let kinds = self
                .computed_kinds
                .iter()
                .map(|k| format!("{k:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("computed {kinds}"));
        }
        parts.join("; ")
    }
}

/// Every cross-slice crossing the architectural gates must judge: the union of
/// the authored `gmeow:sliceDependsOn` declarations (straight off each
/// manifest, needing no evidence) and the computed *semantic* dependency edges
/// that survive the [`is_genuine_crossing_term`] evidence filter.
///
/// Grouped by `(from_slice, to_slice)` rather than by edge, since every
/// violation below is a property of the SLICE PAIR, not of any one artifact
/// reference.
///
/// A computed edge with NO evidence at all is a synthetic
/// `ReconciliationStatus::Stale` edge (an authored `sliceDependsOn` with no
/// semantic backing); it contributes no computed kind, but the declaration that
/// produced it is already carried by the `declared` leg, so a stale forbidden
/// declaration is still judged.
fn crossing_table(
    report: &OwnershipReport,
    catalog: &SliceCatalog,
    all_slice_iris: &BTreeSet<SliceIri>,
    reference_predicates: &ReferencePredicateIndex,
) -> BTreeMap<(SliceIri, SliceIri), CrossingWitness> {
    let mut by_pair: BTreeMap<(SliceIri, SliceIri), CrossingWitness> = BTreeMap::new();

    // Authored declarations — evidence-free by design (RFC §10 Phase 4 reads
    // exactly this set to reconcile computed edges against).
    for record in catalog.records() {
        let from = &record.manifest.slice_iri;
        for to in &record.manifest.depends_on {
            if to == from {
                continue;
            }
            by_pair
                .entry((from.clone(), to.clone()))
                .or_default()
                .declared = true;
        }
    }

    // Computed semantic edges with at least one genuine crossing term. The
    // architectural gates govern SEMANTIC build dependencies only (the same
    // `EdgeKind::is_semantic` set that reconciles against `sliceDependsOn`); a
    // test/example/documentation cross reference is not a build dependency.
    for edge in &report.edges {
        if !edge.edge_kind.is_semantic() {
            continue;
        }
        let genuine: BTreeSet<NamedNode> = edge
            .evidence
            .iter()
            .filter(|e| {
                is_genuine_crossing_term(
                    &edge.from_slice,
                    &e.from_artifact.logical_path,
                    &e.referenced_term,
                    all_slice_iris,
                    reference_predicates,
                )
            })
            .map(|e| e.referenced_term.clone())
            .collect();
        if genuine.is_empty() {
            continue;
        }
        let witness = by_pair
            .entry((edge.from_slice.clone(), edge.to_slice.clone()))
            .or_default();
        witness.computed_kinds.insert(edge.edge_kind);
        witness.terms.extend(genuine);
    }

    by_pair
}

// ── R5: tier-forbidden crossings ─────────────────────────────────────────────

/// A slice's authored `gmeow:sliceTier`, rendered for a finding message.
fn tier_label(tier: Option<&SliceTier>) -> &str {
    match tier {
        Some(SliceTier::Core) => "tierCore",
        Some(SliceTier::Extension) => "tierExtension",
        Some(SliceTier::Domain) => "tierDomain",
        Some(SliceTier::Unknown(iri)) => iri.as_str(),
        None => "(no gmeow:sliceTier)",
    }
}

/// Every slice IRI in `catalog`, mapped to its [`tier_label`].
fn tier_labels(catalog: &SliceCatalog) -> BTreeMap<SliceIri, String> {
    catalog
        .records()
        .iter()
        .map(|record| {
            (
                record.manifest.slice_iri.clone(),
                tier_label(record.manifest.tier.as_ref()).to_string(),
            )
        })
        .collect()
}

/// Every crossing in `crossings` that violates the tier model (Principle 16 /
/// RFC §10): a core slice depending on an extension, or an extension depending
/// on another extension.
///
/// Independent of the peerage/seam machinery above and of any edge's
/// [`ReconciliationStatus`] — a forbidden tier crossing is architecturally
/// illegal regardless of grounding peerage or declaration. Per
/// `crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY`, "even a MATCHED,
/// authored `gmeow:sliceDependsOn` declaration between a forbidden tier pair is
/// still architecturally forbidden — declaring it does not license it", so
/// [`crossing_table`] feeds this gate the DECLARED set as well as the computed
/// one: a declared-forbidden crossing fires here on the declaration alone, with
/// no evidence required and no way for an evidence filter to hide it.
///
/// This is the ONLY place a tier-forbidden crossing is surfaced as a
/// validate-gating [`Finding`]: the `gmeow:graph/slice-analysis` named graph
/// the pipeline ships (`crates/pipeline/src/stages/carrier.rs::build_slice_analysis`,
/// via `purrdf::slice::emit_analysis_graph`) records the identical verdict as
/// shipped DATA (`gmeow:dependencyStatus "forbidden"^^xsd:string`), but
/// nothing read that graph back to gate `make validate` — this function closes
/// that gap directly off [`crossing_table`] + the catalog's own tier data,
/// using the SAME [`is_forbidden_edge`] tier-priority test the emitter uses
/// (byte-identical [`tier_priority`] mapping), so the gate and the shipped data
/// can never classify a crossing differently.
fn forbidden_tier_findings(
    crossings: &BTreeMap<(SliceIri, SliceIri), CrossingWitness>,
    catalog: &SliceCatalog,
) -> Vec<Finding> {
    let tiers = tier_priorities(catalog);
    let labels = tier_labels(catalog);
    let unknown = "(uncatalogued)".to_string();

    let mut findings = Vec::new();
    for ((from, to), witness) in crossings {
        let from_tier = *tiers.get(from).unwrap_or(&2);
        let to_tier = *tiers.get(to).unwrap_or(&2);
        if !is_forbidden_edge(from_tier, to_tier) {
            continue;
        }
        findings.push(crate::slice_ownership::finding(
            Severity::Error,
            crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY,
            format!(
                "{from} ({from_label}) depends on {to} ({to_label}) [{witness}] — this crossing \
                 violates the tier model: a core slice must not depend on an extension, and an \
                 extension must not depend on another extension (Principle 16). Declaring the \
                 crossing does not license it.",
                from_label = labels.get(from).unwrap_or(&unknown),
                to_label = labels.get(to).unwrap_or(&unknown),
                witness = witness.describe(),
            ),
            Some(from.clone()),
        ));
    }
    findings
}

// ── R6: grounding doctrine ───────────────────────────────────────────────────

/// Every `(crossing, term)` pair where a `gmeow:GroundingSlice` references a
/// term owned by a non-grounding slice AND that term is an authored GROUNDING
/// CONCEPT (`gmeow:groundingConceptDomain`).
///
/// This encodes `docs/GROUNDING.md`'s **tier rule** verbatim — "a grounding
/// slice never depends on a non-grounding slice **for a grounding concept**.
/// Where a grounding concept is found split across a grounding and a
/// non-grounding slice, the reconciliation direction is fixed: the grounding
/// slice owns the concept and the non-grounding slice consumes it" — which
/// [`is_forbidden_edge`] can never see, because all three grounding slices are
/// authored `gmeow:tierCore` and a `logic → cognition` crossing therefore reads
/// as an ordinary, legal core→core edge.
///
/// # The qualifier is load-bearing
///
/// "For a grounding concept" is not decoration. A grounding slice consuming
/// ordinary DOMAIN vocabulary by reference is exactly what the rule's own
/// standing example prescribes on the other side of the seam: `lang:` names
/// `gmeow:AttestationArtifact` because a GMN envelope IS an attestation
/// artifact and lang "reuses the attestation vocabulary by reference rather
/// than re-minting it", and `logic:` names `gmeow:claimModalForce` inside a
/// `logic:Formula` because formalizing a slice's own vocabulary is what the
/// `logic:` layer is FOR. Dropping the qualifier turns both into violations and
/// makes the only conforming corpus one in which every formalized term has been
/// swallowed into `logic:` — the reductio that shows the unqualified reading is
/// not the rule.
///
/// So the gate cannot key on the slice pair alone. Whether a term's subject
/// matter falls in one of the three grounding domains is a judgment about
/// MEANING, unavailable from graph shape, and it is authored as data on the
/// term ([`GroundingConceptIndex`]) rather than hard-coded here.
///
/// Grounding→grounding crossings are the Principle 19 peerage grant and never
/// fire here; they are governed instead by the seam registry ([`classify`]'s
/// `Coverage::PeeredUnregisteredSeam`). Non-grounding→grounding is the ordinary,
/// sanctioned consumption direction and never fires either.
///
/// Judged on the COMPUTED crossing terms only. A bare
/// `gmeow:sliceDependsOn <a domain slice>` declaration in a grounding manifest
/// is NOT itself a breach under the qualified rule — `lang:` legitimately
/// depends on `versions`, `citations` and `documents` for domain vocabulary and
/// must declare it — so unlike [`forbidden_tier_findings`], where the crossing
/// is illegal whatever rides it, there is nothing to judge until a term does.
fn grounding_doctrine_findings(
    crossings: &BTreeMap<(SliceIri, SliceIri), CrossingWitness>,
    grounding: &BTreeSet<SliceIri>,
    concepts: &GroundingConceptIndex,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for ((from, to), witness) in crossings {
        if !grounding.contains(from) || grounding.contains(to) {
            continue;
        }
        for term in &witness.terms {
            let Some(domain) = concepts.domain_of(term.as_str()) else {
                continue;
            };
            findings.push(crate::slice_ownership::finding(
                Severity::Error,
                crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY,
                format!(
                    "{from} is a gmeow:GroundingSlice but depends on the non-grounding slice \
                     {to} [{witness}] for the grounding concept {term} — that term is authored \
                     gmeow:groundingConceptDomain <{domain_iri}> ({domain_label}), whose \
                     gmeow:groundingDomainOwner is {owner}. docs/GROUNDING.md's tier rule fixes \
                     the reconciliation direction: {owner} must own {term} (re-point its \
                     rdfs:isDefinedBy and move its block there) and {to} must consume it. The \
                     IRI does not change; ownership is by rdfs:isDefinedBy, never by namespace.",
                    term = term.as_str(),
                    witness = witness.describe(),
                    domain_iri = domain.iri,
                    domain_label = domain.label,
                    owner = domain.owner,
                ),
                Some(from.clone()),
            ));
        }
    }
    findings
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
/// * every tier-forbidden crossing — declared OR computed, any reconciliation
///   status — is ADDITIONALLY surfaced as a
///   `slice-ownership.forbidden-dependency` `Error`
///   ([`forbidden_tier_findings`], R5);
/// * every grounding→non-grounding crossing — declared OR computed — is
///   ADDITIONALLY surfaced as a
///   `slice-ownership.grounding-downward-dependency` `Error`
///   ([`grounding_doctrine_findings`], R6).
pub fn peerage_aware_ownership_findings(
    report: &OwnershipReport,
    catalog: &SliceCatalog,
) -> Result<Vec<Finding>> {
    let classification = classify(report, catalog)?;

    let mut findings: Vec<Finding> = crate::slice_ownership::ownership_findings(report)
        .into_iter()
        .filter(|f| f.code != crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY)
        .collect();

    let all_slice_iris = slice_iris(catalog);
    let corpus = CorpusIndex::build(catalog)?;
    let crossings = crossing_table(
        report,
        catalog,
        &all_slice_iris,
        &corpus.reference_predicates,
    );
    findings.extend(forbidden_tier_findings(&crossings, catalog));
    findings.extend(grounding_doctrine_findings(
        &crossings,
        &grounding_slice_iris(catalog)?,
        &corpus.grounding_concepts,
    ));

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
        gmeow_ns::gmeow_slice_vocab()
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
    fn artifact_evidence_with(
        slice: &str,
        role: ArtifactRole,
        logical_path: &str,
    ) -> ArtifactEvidence {
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

    // ── Class A / Class B genuine-crossing-term exclusion ────────────────────

    const QUALITY: &str = "https://blackcatinformatics.ca/gmeow/slices/quality";
    const WIDGETS: &str = "https://blackcatinformatics.ca/gmeow/slices/widgets";
    const GUIDES: &str = "https://blackcatinformatics.ca/gmeow/slices/guides";

    /// A catalog with `logic:` (a lone, seam-free grounding slice — the
    /// peerage/seam machinery is irrelevant to these tests) plus three
    /// ordinary domain slices, `quality`, `widgets`, and `guides`. Dedicated
    /// to the Class A (slice-IRI-as-data) and Class B (corpus-declared
    /// non-coupling predicates) exclusion tests, which need REAL artifact
    /// content parsed off disk (`slice_iris` and [`ReferencePredicateIndex`]
    /// re-parse the catalog directly) — a hand-built [`OwnershipReport`]
    /// fixture alone can never exercise them.
    ///
    /// The predicate DECLARATIONS here mirror the real corpus exactly, because
    /// [`NonCouplingPredicates`] derives its whole set from them and from
    /// nothing else:
    ///
    /// * `logic:formalizes` — `owl:AnnotationProperty` (real corpus: same);
    /// * `logic:characterizes` — `owl:ObjectProperty` carrying
    ///   `gmeow:graphBoxRole gmeow:boxRBox` (real corpus: same), i.e. a reasoned
    ///   RBox axiom, which is therefore COUPLING;
    /// * `logic:relation` — `owl:ObjectProperty` with a narrow range (real
    ///   corpus: a `logic:Formula` AST slot), also coupling;
    /// * `gmeow:usesTerm` — `owl:ObjectProperty` with `rdfs:range rdfs:Resource`
    ///   (real corpus: same), i.e. an explicitly open range, non-coupling.
    ///
    /// `guides` is deliberately a PLAIN (non-`GroundingSlice`) domain slice,
    /// proving the exclusion is a property of the PREDICATE and applies from any
    /// slice, not only the three grounding ones.
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
            r#"logic:formalizes
                a owl:AnnotationProperty ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> .

            logic:characterizes
                a owl:ObjectProperty ;
                rdfs:range gmeow:Facet ;
                gmeow:graphBoxRole gmeow:boxRBox ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> .

            logic:relation
                a owl:ObjectProperty ;
                rdfs:range gmeow:Relation ;
                gmeow:graphBoxRole gmeow:boxRBox ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> .

            logic:widgetFacetFormalization
                a owl:NamedIndividual ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> ;
                logic:formalizes gmeow:widgetFacet .

            logic:widgetFacetCharacteristic
                a owl:NamedIndividual , logic:PropertyCharacteristicAssertion ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> ;
                logic:characterizes gmeow:widgetCharacterizedTerm ;
                logic:formalizes gmeow:widgetCharacterizedTerm .

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
            r#"gmeow:usesTerm
                a owl:ObjectProperty ;
                rdfs:range rdfs:Resource ;
                gmeow:graphBoxRole gmeow:boxRBox ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/guides> .

            gmeow:relatedTerm
                a owl:ObjectProperty ;
                rdfs:range gmeow:Term ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/guides> .

            gmeow:guideWidgetDocOnly
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

    /// Class A is REACHABLE, not dead code: prove that `purrdf`'s ownership
    /// analyzer really does admit a slice's own IRI into `validated_owner`, so
    /// the [`slice_iris`] filter is live. Every slice `module.ttl` declares its
    /// own slice IRI as the module's `owl:Ontology` header with
    /// `rdfs:isDefinedBy <itself>`, and that IRI is inside the `gmeow:` vocab
    /// namespace, so Phase 1's `subject.starts_with(vocab_ns)` harvest admits it
    /// and Phase 2 validates it (physical origin == declared owner). Without
    /// this, `is_ownership_bearing == Module | Shapes` would suggest a slice IRI
    /// can never become an owned term — it can.
    #[test]
    fn a_slice_iri_really_is_a_validated_owned_term_so_class_a_is_reachable() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_manifest(
            root,
            "core",
            "owner",
            r#"<https://blackcatinformatics.ca/gmeow/slices/owner>
                a gmeow:Slice ;
                rdfs:label "owner" .
            "#,
        );
        // The real shape every slice module.ttl carries: the slice IRI as the
        // module's owl:Ontology header, defined by itself.
        write_module(
            root,
            "core",
            "owner",
            r#"<https://blackcatinformatics.ca/gmeow/slices/owner>
                a owl:Ontology ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/owner> ;
                rdfs:label "Owner module" .
            "#,
        );
        write_manifest(
            root,
            "core",
            "citer",
            r#"<https://blackcatinformatics.ca/gmeow/slices/citer>
                a gmeow:Slice ;
                rdfs:label "citer" .
            "#,
        );
        // The real `slice-quality-rubric` shape: an ABox record naming ANOTHER
        // slice's IRI as the assessment target.
        write_module(
            root,
            "core",
            "citer",
            r#"<https://blackcatinformatics.ca/gmeow/slices/citer>
                a owl:Ontology ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/citer> .

            gmeow:citerRubricRecord
                a owl:NamedIndividual ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/citer> ;
                gmeow:ceilingSlice <https://blackcatinformatics.ca/gmeow/slices/owner> .
            "#,
        );
        let catalog = SliceCatalog::discover(&root.join("slices"), vocab()).unwrap();
        let report = purrdf::slice::OwnershipAnalyzer::new(&catalog)
            .analyze()
            .unwrap();

        let owner_iri = "https://blackcatinformatics.ca/gmeow/slices/owner";
        let record = report
            .ownership
            .get(&nn(owner_iri))
            .expect("the slice IRI must be an owned term at all — Class A's whole premise");
        assert_eq!(
            record.status,
            purrdf::slice::OwnershipStatus::Validated,
            "the slice IRI must be VALIDATED-owned, which is what puts it into \
             validated_owner and lets it produce real dependency edges"
        );
        // And it really did produce an edge whose only evidence is that IRI.
        let edge = report
            .edges
            .iter()
            .find(|e| e.to_slice == owner_iri)
            .expect("the slice-IRI citation produced a real dependency edge");
        assert!(
            edge.evidence
                .iter()
                .all(|e| e.referenced_term.as_str() == owner_iri),
            "the edge's evidence is the slice IRI itself: {:?}",
            edge.evidence
        );
        // Class A is what keeps it from gating.
        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            findings.is_empty(),
            "Class A must suppress a pure slice-IRI-as-data crossing: {findings:?}"
        );
    }

    #[test]
    fn class_a_slice_iri_as_data_crossing_is_suppressed() {
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

    /// The Class B set must come out of the CORPUS, not out of a source-level
    /// list: assert the two declaration tests actually pick up exactly the
    /// fixture's declarations, and — load-bearing — that a `boxRBox`
    /// `owl:ObjectProperty` is NOT in it.
    #[test]
    fn class_b_non_coupling_predicates_are_derived_from_the_corpus_declarations() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let index = CorpusIndex::build(&catalog).unwrap().reference_predicates;

        assert!(
            index
                .non_coupling
                .annotation
                .contains("https://blackcatinformatics.ca/logic/formalizes"),
            "logic:formalizes is declared owl:AnnotationProperty: {:?}",
            index.non_coupling.annotation
        );
        assert!(
            index
                .non_coupling
                .open_range
                .contains("https://blackcatinformatics.ca/gmeow/usesTerm"),
            "gmeow:usesTerm is declared rdfs:range rdfs:Resource: {:?}",
            index.non_coupling.open_range
        );
        // The whole point of the derivation: a reasoned RBox object property is
        // NOT meta, however much it looks like law bookkeeping.
        for reasoned in [
            "https://blackcatinformatics.ca/logic/characterizes",
            "https://blackcatinformatics.ca/logic/relation",
            "https://blackcatinformatics.ca/gmeow/relatedTerm",
        ] {
            assert!(
                !index.non_coupling.contains(reasoned),
                "{reasoned} is a declared owl:ObjectProperty carrying real axiom weight and must \
                 never be treated as non-coupling"
            );
        }
        // An undeclared, purely external predicate satisfies neither test.
        assert!(
            !index
                .non_coupling
                .contains("http://www.w3.org/2004/02/skos/core#relatedMatch"),
            "an external predicate the corpus never declares can never be non-coupling"
        );
    }

    #[test]
    fn class_b_annotation_property_crossing_is_suppressed() {
        // `logic:`'s module names `gmeow:widgetFacet` ONLY via
        // `logic:formalizes`, declared `a owl:AnnotationProperty` in the same
        // corpus — no logical axiom, so no build dependency.
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
            "a pure owl:AnnotationProperty crossing has zero genuine terms and must be \
             suppressed entirely: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            findings.is_empty(),
            "a pure owl:AnnotationProperty crossing must never fire a finding: {findings:?}"
        );
    }

    /// The regression the hard-coded 21-IRI `GROUNDING_META_PREDICATES` list
    /// hid: `logic:characterizes` is an `owl:ObjectProperty` carrying
    /// `gmeow:graphBoxRole gmeow:boxRBox` — a reasoned RBox axiom feeding the
    /// live DL consistency gate, NOT an annotation. A term named via it (even
    /// alongside a genuine annotation) is real term usage and must surface.
    #[test]
    fn a_reasoned_rbox_object_property_is_no_longer_exempt() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                LOGIC,
                WIDGETS,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/gmeow/widgetCharacterizedTerm"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(LOGIC, WIDGETS, EdgeKind::Ontology)],
        };

        let classification = classify(&report, &catalog).unwrap();
        assert_eq!(
            classification.verdicts.len(),
            1,
            "logic:characterizes is a reasoned RBox object property, never a meta annotation: \
             {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY),
            "{findings:?}"
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
        let undeclared: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY)
            .collect();
        assert_eq!(undeclared.len(), 1, "{findings:?}");
        assert_eq!(undeclared[0].severity, Severity::Error);
        // `logic:` is a gmeow:GroundingSlice and `widgets` is not, but
        // `gmeow:widgetOtherTerm` carries no gmeow:groundingConceptDomain
        // marker: it is ordinary domain vocabulary, which docs/GROUNDING.md's
        // tier rule explicitly permits a grounding slice to consume. The
        // crossing is an UNDECLARED dependency (declare it) and nothing more.
        assert!(
            !findings
                .iter()
                .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
            "the tier rule is qualified 'for a grounding concept': {findings:?}"
        );
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
        assert!(
            findings
                .iter()
                .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY),
            "{findings:?}"
        );
    }

    #[test]
    fn class_b_open_range_uses_term_crossing_is_suppressed_from_any_slice() {
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

    // ── Previously-blanket exclusions that are now GONE ──────────────────────

    /// The `ArtifactRole::CompetencyQuery` blanket exclusion is DELETED.
    /// `purrdf` maps `CompetencyQuery | VerifyQuery -> EdgeKind::Query`, one of
    /// only four semantic edge kinds, so excluding the role removed essentially
    /// the whole `Query` edge kind from both the dependency rule and the tier
    /// rule — and `VerifyQuery`, the identical shape, was never excluded. A
    /// competency query that references another slice's term is a real crossing.
    #[test]
    fn a_competency_query_crossing_now_surfaces() {
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
                    referenced_term: nn(
                        "https://blackcatinformatics.ca/gmeow/widgetCompetencyTerm",
                    ),
                }],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Query)],
        };

        let classification = classify(&report, &catalog).expect("classify must not hard-fail");
        assert_eq!(
            classification.verdicts.len(),
            1,
            "a competency-query crossing is a real Query-kind dependency: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(
            findings[0].code,
            crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY
        );
    }

    /// A `VerifyQuery` crossing behaves IDENTICALLY to a `CompetencyQuery` one
    /// — the two roles map to the same `EdgeKind::Query`, and neither is
    /// role-exempt any more. This is the asymmetry the deleted exclusion
    /// created.
    #[test]
    fn a_verify_query_crossing_surfaces_identically_to_a_competency_query() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = class_filter_catalog(tmp.path());
        let build = |role: ArtifactRole, path: &str| OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge_with_evidence(
                QUALITY,
                WIDGETS,
                EdgeKind::Query,
                vec![EdgeEvidence {
                    from_artifact: artifact_evidence_with(QUALITY, role, path),
                    referenced_term: nn("https://blackcatinformatics.ca/gmeow/widgetQueryTerm"),
                }],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Query)],
        };
        let competency = peerage_aware_ownership_findings(
            &build(ArtifactRole::CompetencyQuery, "queries/competency/thing.rq"),
            &catalog,
        )
        .unwrap();
        let verify = peerage_aware_ownership_findings(
            &build(ArtifactRole::VerifyQuery, "queries/verify/thing.rq"),
            &catalog,
        )
        .unwrap();
        assert_eq!(competency.len(), 1, "{competency:?}");
        assert_eq!(
            competency.iter().map(|f| &f.code).collect::<Vec<_>>(),
            verify.iter().map(|f| &f.code).collect::<Vec<_>>(),
            "CompetencyQuery and VerifyQuery are the same EdgeKind::Query shape and must gate \
             identically"
        );
    }

    /// The `skos:relatedMatch`-in-`Mapping` blanket exclusion is DELETED.
    /// `skos:relatedMatch` is a purely EXTERNAL predicate the corpus declares
    /// nowhere: it is neither an `owl:AnnotationProperty` nor open-ranged in
    /// this ontology, so it passes neither [`NonCouplingPredicates`] test.
    /// Keeping it exempt would mean re-hard-coding an IRI — exactly the defect
    /// the derived set removes. It is also a real symmetric SKOS object
    /// property, and `ArtifactRole::Mapping -> EdgeKind::Mapping` is a
    /// `is_semantic()` kind by construction.
    #[test]
    fn an_internal_related_match_crossing_now_surfaces() {
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
        assert_eq!(
            classification.verdicts.len(),
            1,
            "an internal skos:relatedMatch crossing is a real Mapping-kind dependency: {:?}",
            classification.verdicts
        );

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(
            findings[0].code,
            crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY
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
        assert_eq!(
            findings[0].code,
            crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY
        );
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

    /// A catalog whose `tier-core` slice AUTHORS `gmeow:sliceDependsOn` on a
    /// `tierExtension` slice, with no computed edge and no evidence at all.
    fn declared_forbidden_catalog(root: &Path) -> SliceCatalog {
        write_manifest(
            root,
            "core",
            "tier-core",
            r#"<https://blackcatinformatics.ca/gmeow/slices/tier-core>
                a gmeow:Slice ;
                rdfs:label "tier-core" ;
                gmeow:sliceTier gmeow:tierCore ;
                gmeow:sliceDependsOn <https://blackcatinformatics.ca/gmeow/slices/tier-ext-a> .
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
        SliceCatalog::discover(&root.join("slices"), vocab()).unwrap()
    }

    /// R5 must judge the DECLARED `gmeow:sliceDependsOn` set, not only computed
    /// edges. Before this, an edge whose evidence was fully exempted was
    /// `continue`d BEFORE the tier test, so a declared-forbidden crossing was
    /// invisible to the forbidden gate — contradicting the code's own doc
    /// ("declaring it does not license it"). A declaration needs no evidence.
    #[test]
    fn a_declared_forbidden_crossing_fires_with_no_evidence_at_all() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = declared_forbidden_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: Vec::new(),
            diagnostics: Vec::new(),
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        let forbidden: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY)
            .collect();
        assert_eq!(
            forbidden.len(),
            1,
            "a declared core -> extension crossing is forbidden on the declaration alone: \
             {findings:?}"
        );
        assert!(
            forbidden[0]
                .message
                .contains("declared gmeow:sliceDependsOn"),
            "the finding must name the declaration as its witness: {}",
            forbidden[0].message
        );
        assert!(
            forbidden[0].message.contains("tierCore")
                && forbidden[0].message.contains("tierExtension"),
            "the finding must name both tiers: {}",
            forbidden[0].message
        );
    }

    /// Even when EVERY piece of an edge's evidence is exempt (a pure
    /// slice-IRI-as-data crossing), a matching DECLARATION still fires R5. This
    /// is the exact hole the pre-declaration gate had: the evidence filter
    /// `continue`d the edge before the tier test could see it.
    #[test]
    fn a_declared_forbidden_crossing_fires_even_when_all_evidence_is_exempt() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = declared_forbidden_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            // The only evidence is the target slice's own IRI — Class A, fully
            // exempt, so the computed leg contributes nothing.
            edges: vec![edge(
                TIER_CORE_SLICE,
                TIER_EXT_A,
                EdgeKind::Ontology,
                &[TIER_EXT_A],
                ReconciliationStatus::Matched,
            )],
            diagnostics: Vec::new(),
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY)
                .count(),
            1,
            "{findings:?}"
        );
    }

    // ── R6: grounding doctrine ────────────────────────────────────────────────

    const GROUNDING_LOGIC: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";
    const GROUNDING_MATH: &str = "https://blackcatinformatics.ca/gmeow/slices/math";
    const DOMAIN_COGNITION: &str = "https://blackcatinformatics.ca/gmeow/slices/cognition";
    /// The `gmeow:GroundingDomain` individual `logic:` declares in its manifest.
    const DOMAIN_LOGICAL: &str = "https://blackcatinformatics.ca/gmeow/groundingDomainLogical";
    /// A term in `cognition` that IS a grounding concept: a knowledge-base
    /// partition role, i.e. a logical formalism. Marked as such in the fixture.
    const MARKED_CONCEPT: &str = "https://blackcatinformatics.ca/gmeow/kbPartitionRole";
    /// A term in `cognition` that is ordinary domain vocabulary — unmarked.
    const ORDINARY_TERM: &str = "https://blackcatinformatics.ca/gmeow/beliefState";

    /// Two mutually-peered `gmeow:GroundingSlice`s and one ordinary domain
    /// slice, ALL `gmeow:tierCore` — exactly the real corpus's shape, which is
    /// why `is_forbidden_edge` can never see this violation: every crossing
    /// among them is core -> core and therefore tier-legal.
    ///
    /// `logic:`'s manifest declares the logical `gmeow:GroundingDomain` (as the
    /// real one does), and `cognition`'s `module.ttl` owns TWO terms: one
    /// carrying the `gmeow:groundingConceptDomain` marker and one without it.
    /// The pair is what makes the gate's discrimination testable — the same
    /// slice pair, the same crossing direction, opposite verdicts, decided
    /// ONLY by the authored marker.
    fn grounding_doctrine_catalog(root: &Path) -> SliceCatalog {
        write_manifest(
            root,
            "grounding",
            "logic",
            r#"<https://blackcatinformatics.ca/gmeow/slices/logic>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "logic" ;
                gmeow:sliceTier gmeow:tierCore ;
                gmeow:sliceCoFoundationalWith <https://blackcatinformatics.ca/gmeow/slices/math> .

            <https://blackcatinformatics.ca/gmeow/groundingDomainLogical>
                a gmeow:GroundingDomain ;
                rdfs:label "logical grounding domain" ;
                gmeow:groundingDomainOwner <https://blackcatinformatics.ca/gmeow/slices/logic> .
            "#,
        );
        write_manifest(
            root,
            "grounding",
            "math",
            r#"<https://blackcatinformatics.ca/gmeow/slices/math>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "math" ;
                gmeow:sliceTier gmeow:tierCore ;
                gmeow:sliceCoFoundationalWith <https://blackcatinformatics.ca/gmeow/slices/logic> .
            "#,
        );
        write_manifest(
            root,
            "core",
            "cognition",
            r#"<https://blackcatinformatics.ca/gmeow/slices/cognition>
                a gmeow:Slice ;
                rdfs:label "cognition" ;
                gmeow:sliceTier gmeow:tierCore .
            "#,
        );
        write_module(
            root,
            "core",
            "cognition",
            r#"gmeow:kbPartitionRole
                a owl:ObjectProperty ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/cognition> ;
                rdfs:label "kb partition role" ;
                gmeow:groundingConceptDomain <https://blackcatinformatics.ca/gmeow/groundingDomainLogical> .

            gmeow:beliefState
                a owl:ObjectProperty ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/cognition> ;
                rdfs:label "belief state" .
            "#,
        );
        SliceCatalog::discover(&root.join("slices"), vocab()).unwrap()
    }

    /// Proof the gate is NEEDED: the identical crossing is tier-legal, so R5
    /// alone would let it through silently.
    #[test]
    fn a_grounding_to_marked_concept_crossing_is_tier_legal_but_breaks_the_grounding_doctrine() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_doctrine_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                GROUNDING_LOGIC,
                DOMAIN_COGNITION,
                EdgeKind::Ontology,
                &[MARKED_CONCEPT],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(
                GROUNDING_LOGIC,
                DOMAIN_COGNITION,
                EdgeKind::Ontology,
            )],
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            !findings
                .iter()
                .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY),
            "both slices are tierCore, so the tier gate is blind to this: {findings:?}"
        );
        let doctrine: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY)
            .collect();
        assert_eq!(doctrine.len(), 1, "{findings:?}");
        assert_eq!(doctrine[0].severity, Severity::Error);
        assert!(doctrine[0].message.contains(GROUNDING_LOGIC));
        assert!(doctrine[0].message.contains(DOMAIN_COGNITION));
        // The message must NAME the offending concept, the grounding domain its
        // subject matter falls in, and the grounding slice that must own it —
        // a from/to pair alone is not an actionable finding.
        assert!(
            doctrine[0].message.contains(MARKED_CONCEPT),
            "{}",
            doctrine[0].message
        );
        assert!(
            doctrine[0].message.contains(DOMAIN_LOGICAL),
            "{}",
            doctrine[0].message
        );
        assert!(
            doctrine[0].message.contains("logical grounding domain"),
            "the finding must name the domain by its authored rdfs:label: {}",
            doctrine[0].message
        );
    }

    /// The mutation twin of the test above, and the whole point of this
    /// correction: the SAME grounding slice, the SAME non-grounding slice, the
    /// SAME crossing direction, differing ONLY in that the referenced term
    /// carries no `gmeow:groundingConceptDomain` marker — ordinary domain
    /// vocabulary a grounding slice may consume by reference. It must NOT fire.
    #[test]
    fn a_grounding_slice_consuming_ordinary_domain_vocabulary_never_fires_the_doctrine_gate() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_doctrine_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                GROUNDING_LOGIC,
                DOMAIN_COGNITION,
                EdgeKind::Ontology,
                &[ORDINARY_TERM],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(
                GROUNDING_LOGIC,
                DOMAIN_COGNITION,
                EdgeKind::Ontology,
            )],
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            !findings
                .iter()
                .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
            "docs/GROUNDING.md forbids the downward dependency only FOR A GROUNDING CONCEPT; \
             gmeow:beliefState carries no gmeow:groundingConceptDomain marker: {findings:?}"
        );
    }

    /// The discrimination is decided by the MARKER, not by the term IRI: one
    /// edge naming both terms yields exactly ONE finding, and it names the
    /// marked term. Guards against a gate that fires per EDGE (which would
    /// report a single generic violation) or per TERM without filtering (which
    /// would report two).
    #[test]
    fn one_edge_naming_both_a_marked_and_an_unmarked_term_fires_once_on_the_marked_one() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_doctrine_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                GROUNDING_LOGIC,
                DOMAIN_COGNITION,
                EdgeKind::Ontology,
                &[ORDINARY_TERM, MARKED_CONCEPT],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(
                GROUNDING_LOGIC,
                DOMAIN_COGNITION,
                EdgeKind::Ontology,
            )],
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        let doctrine: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY)
            .collect();
        assert_eq!(doctrine.len(), 1, "{findings:?}");
        assert!(
            doctrine[0].message.contains(MARKED_CONCEPT),
            "{}",
            doctrine[0].message
        );
        assert!(
            !doctrine[0].message.contains(ORDINARY_TERM),
            "{}",
            doctrine[0].message
        );
    }

    /// A bare `gmeow:sliceDependsOn` from a grounding manifest onto a domain
    /// slice is NOT a breach under the qualified rule — `lang:` legitimately
    /// declares `versions`, `citations` and `documents` — so a declaration with
    /// no grounding concept crossing it must stay silent. (The unqualified
    /// reading fired here, which is exactly the over-fire being corrected.)
    #[test]
    fn a_declared_grounding_to_domain_dependency_alone_is_not_a_doctrine_breach() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_manifest(
            root,
            "grounding",
            "logic",
            r#"<https://blackcatinformatics.ca/gmeow/slices/logic>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "logic" ;
                gmeow:sliceTier gmeow:tierCore ;
                gmeow:sliceDependsOn <https://blackcatinformatics.ca/gmeow/slices/cognition> .

            <https://blackcatinformatics.ca/gmeow/groundingDomainLogical>
                a gmeow:GroundingDomain ;
                rdfs:label "logical grounding domain" ;
                gmeow:groundingDomainOwner <https://blackcatinformatics.ca/gmeow/slices/logic> .
            "#,
        );
        write_manifest(
            root,
            "core",
            "cognition",
            r#"<https://blackcatinformatics.ca/gmeow/slices/cognition>
                a gmeow:Slice ;
                rdfs:label "cognition" ;
                gmeow:sliceTier gmeow:tierCore .
            "#,
        );
        let catalog = SliceCatalog::discover(&root.join("slices"), vocab()).unwrap();
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: Vec::new(),
            diagnostics: Vec::new(),
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            !findings
                .iter()
                .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
            "a declaration is not itself a grounding-concept crossing: {findings:?}"
        );
    }

    /// The Principle 19 peerage grant: a grounding -> grounding crossing is
    /// legitimate and must NEVER fire R6 (it is governed by the seam registry
    /// instead).
    #[test]
    fn a_grounding_to_grounding_peer_crossing_never_fires_the_doctrine_gate() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_doctrine_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                GROUNDING_LOGIC,
                GROUNDING_MATH,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/math/Quantity"],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(
                GROUNDING_LOGIC,
                GROUNDING_MATH,
                EdgeKind::Ontology,
            )],
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            !findings
                .iter()
                .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
            "grounding -> grounding is the peerage grant, never a doctrine breach: {findings:?}"
        );
    }

    /// The sanctioned direction: a domain slice CONSUMING a grounding term is
    /// exactly what the doctrine prescribes and must never fire R6 — even when
    /// the consumed term is a marked grounding concept, which is the normal,
    /// correct state of the world after a promotion.
    #[test]
    fn a_domain_to_grounding_crossing_never_fires_the_doctrine_gate() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = grounding_doctrine_catalog(tmp.path());
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                DOMAIN_COGNITION,
                GROUNDING_MATH,
                EdgeKind::Ontology,
                &["https://blackcatinformatics.ca/math/Quantity"],
                ReconciliationStatus::Matched,
            )],
            diagnostics: Vec::new(),
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            !findings
                .iter()
                .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
            "non-grounding -> grounding is the sanctioned consumption direction: {findings:?}"
        );
    }

    /// A marker naming a `gmeow:GroundingDomain` no grounding manifest declares
    /// resolves to nothing: an undeclared domain has no
    /// `gmeow:groundingDomainOwner`, so there is no reconciliation direction the
    /// finding could state, and inventing one would be the gate carrying its own
    /// opinion. The dangling domain IRI is caught by `authoring.undeclared-term`
    /// instead. Guards against a future `domain_of` that returns a synthetic
    /// record rather than `None`.
    #[test]
    fn a_marker_naming_an_undeclared_grounding_domain_does_not_fire() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_manifest(
            root,
            "grounding",
            "logic",
            r#"<https://blackcatinformatics.ca/gmeow/slices/logic>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "logic" ;
                gmeow:sliceTier gmeow:tierCore .
            "#,
        );
        write_manifest(
            root,
            "core",
            "cognition",
            r#"<https://blackcatinformatics.ca/gmeow/slices/cognition>
                a gmeow:Slice ;
                rdfs:label "cognition" ;
                gmeow:sliceTier gmeow:tierCore .
            "#,
        );
        write_module(
            root,
            "core",
            "cognition",
            r#"gmeow:kbPartitionRole
                a owl:ObjectProperty ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/cognition> ;
                rdfs:label "kb partition role" ;
                gmeow:groundingConceptDomain <https://blackcatinformatics.ca/gmeow/groundingDomainNeverDeclared> .
            "#,
        );
        let catalog = SliceCatalog::discover(&root.join("slices"), vocab()).unwrap();
        let report = OwnershipReport {
            ownership: std::collections::HashMap::new(),
            edges: vec![edge(
                GROUNDING_LOGIC,
                DOMAIN_COGNITION,
                EdgeKind::Ontology,
                &[MARKED_CONCEPT],
                ReconciliationStatus::Undeclared,
            )],
            diagnostics: vec![undeclared_diag(
                GROUNDING_LOGIC,
                DOMAIN_COGNITION,
                EdgeKind::Ontology,
            )],
        };

        let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
        assert!(
            !findings
                .iter()
                .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
            "an unowned domain names no reconciliation direction: {findings:?}"
        );
    }

    /// The real corpus's own promotion, asserted as data rather than as prose:
    /// the graph-box-role cluster IS marked as a logical grounding concept, the
    /// logical domain IS owned by `logic:`, and every marked term IS owned by
    /// the grounding slice its domain names. This is what makes the live gate's
    /// zero honest — a zero produced by an EMPTY marker set would be vacuous.
    #[test]
    fn the_real_corpus_marks_the_box_role_cluster_and_owns_every_marked_concept() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../slices");
        let catalog = SliceCatalog::discover(&dir, vocab()).unwrap();
        let corpus = CorpusIndex::build(&catalog).unwrap();
        let concepts = &corpus.grounding_concepts;
        let report = purrdf::slice::OwnershipAnalyzer::new(&catalog)
            .analyze()
            .unwrap();

        // All three grounding domains are declared, each owned by its slice.
        let owners: BTreeMap<&str, &str> = concepts
            .domains
            .values()
            .map(|d| (d.iri.as_str(), d.owner.as_str()))
            .collect();
        assert_eq!(
            owners.get("https://blackcatinformatics.ca/gmeow/groundingDomainLogical"),
            Some(&"https://blackcatinformatics.ca/gmeow/slices/logic"),
            "{owners:?}"
        );
        assert_eq!(
            owners.get("https://blackcatinformatics.ca/gmeow/groundingDomainLinguistic"),
            Some(&"https://blackcatinformatics.ca/gmeow/slices/lang"),
            "{owners:?}"
        );
        assert_eq!(
            owners.get("https://blackcatinformatics.ca/gmeow/groundingDomainMathematical"),
            Some(&"https://blackcatinformatics.ca/gmeow/slices/math"),
            "{owners:?}"
        );

        // The marker set is NON-EMPTY and contains the whole box-role cluster:
        // the value type, its five role individuals, and the property.
        for local in [
            "GraphBoxRole",
            "boxABox",
            "boxCBox",
            "boxConfigBox",
            "boxRBox",
            "boxTBox",
            "graphBoxRole",
        ] {
            let iri = format!("https://blackcatinformatics.ca/gmeow/{local}");
            let domain = concepts
                .domain_of(&iri)
                .unwrap_or_else(|| panic!("gmeow:{local} must carry gmeow:groundingConceptDomain"));
            assert_eq!(
                domain.iri,
                "https://blackcatinformatics.ca/gmeow/groundingDomainLogical"
            );
        }

        // And the standing invariant the gate's zero rests on: EVERY marked
        // term is owned by the grounding slice its domain names. A marked term
        // owned elsewhere is precisely the R6 violation.
        let mut misowned: Vec<(String, String, String)> = Vec::new();
        for (term, domain_iri) in &concepts.term_domain {
            let Some(domain) = concepts.domains.get(domain_iri) else {
                continue;
            };
            let Some(owned) = NamedNode::new(term)
                .ok()
                .and_then(|n| report.ownership.get(&n))
            else {
                continue;
            };
            if owned.declared_owner != domain.owner {
                misowned.push((
                    term.clone(),
                    owned.declared_owner.clone(),
                    domain.owner.clone(),
                ));
            }
        }
        assert!(
            misowned.is_empty(),
            "every gmeow:groundingConceptDomain-marked term must be owned by its domain's \
             gmeow:groundingDomainOwner; these are not: {misowned:?}"
        );
    }
}
