// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Peerage-aware projection of an [`OwnershipReport`]'s undeclared-dependency
//! diagnostics: an undeclared cross-slice edge between two mutually declared
//! `gmeow:sliceCoFoundationalWith` grounding peers (`lang:`/`math:`/`logic:`) is
//! not automatically an ownership violation — Principle 19's peerage grant
//! deliberately lets the three grounding slices reference each other — but that
//! grant is not a blank cheque either: docs/GROUNDING.md's "seam registry" is the
//! CLOSED set of sanctioned cross-grounding reference channels, and every peered
//! crossing must land on one of them. (The registry's SIZE is governance data
//! this engine never hard-codes — it reads whatever the manifests authorize; the
//! deliberate-change gate on that size is
//! `crates/pipeline/src/stages/carrier.rs`.)
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
/// grounding layers (`lang:`/`math:`/`logic:`). Shared with
/// [`crate::authoring_integrity`]: one declaration, not a per-module copy.
pub(crate) const GMEOW_GROUNDING_SLICE: &str =
    "https://blackcatinformatics.ca/gmeow/GroundingSlice";
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
/// Shared with [`crate::authoring_integrity`]: one declaration, not a copy.
pub(crate) const GMEOW_CO_FOUNDATIONAL_WITH: &str =
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
            // module.ttl is authored in canonical `logic:` after the owl:→logic: flip, so recognize
            // the `logic:AnnotationProperty` marker alongside its `owl:` view (canonical first).
            if p == RDF_TYPE
                && (object == gmeow_ns::LOGIC_ANNOTATION_PROPERTY
                    || object == OWL_ANNOTATION_PROPERTY)
            {
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
                let ds = Dataset::parse_turtle(&artifact.content, None, &artifact.logical_path)
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

#[path = "slice_peerage.tests.rs"]
#[cfg(test)]
mod tests;
