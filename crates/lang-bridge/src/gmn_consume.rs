// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN **consume-path security-ring filter** — the production surface behind
//! requirement #9's verbatim consume order: *canonicalize → SECURITY-RING FILTER →
//! GMN-1 → token-budget fit*.
//!
//! ## What this is
//!
//! A pure library function ([`consume_project`]) that takes a ring-tagged GMN-0 model, a
//! TARGET ring, and an optional token budget, and returns the GMN-1 projection of exactly
//! the content ADMISSIBLE into that target — out-of-ring content EXCLUDED, budget overflow
//! ELIDED-WITH-DISCLOSURE, and any condition under which exclusion cannot be done without
//! leaking raised as the named [`GmnConsumeError`] (`lang:GmnRingLeak`). It is called by the
//! shipped `gmeow gmn project` CLI subcommand and is the same function the MCP consume path
//! can call — the one place the security ring is actually enforced on the serve/decode surface.
//!
//! ## The content-ring-tagging convention
//!
//! A CONTENT SUBJECT `S` carries its admission class with `gmeow:gmnContentRing` — the
//! CLAIM-scoped classification predicate:
//!
//! ```turtle
//! ex:claimAlpha gmeow:gmnContentRing gmeow:gmnRingCore .
//! ```
//!
//! classifies the CLAIM rooted at `S` — every quad whose subject is `S`, the same
//! canonical-subject group the codec's per-claim witness uses — at that ring. Exactly one ring
//! per content subject (zero, or two distinct, is a [`GmnConsumeError::Unclassified`] leak: the
//! admission class was never unambiguously checked). `gmeow:gmnContentRing` is deliberately
//! distinct from `gmeow:gmnSecurityRing`: the latter classifies an ENVELOPE crossing and its own
//! `gmeow:avoidWhen` forbids using it on an inner claim, so the consume filter reads the
//! claim-scoped `gmeow:gmnContentRing` instead — both range over the SAME open
//! `gmeow:GmnSecurityRing` lattice, the envelope's ring being the admission TARGET and a content
//! subject's ring what is being admitted. The ring INDIVIDUALS and their lattice coordinates are
//! NOT read from the tagged dataset — they are resolved checkout-free from the shipped
//! `gmeow.gts` bundle (or an authored `module.ttl`) via [`RingLattice`].
//!
//! ## The admission rule (the DERIVED product-order, computed here, not asserted)
//!
//! Content ring `C` is admissible into target ring `T` iff `C == T` OR `gmnRingWithin(C, T)`,
//! where the within-closure is computed DIRECTLY from the authored coordinates exactly as
//! `gmeow:ruleGmnRingWithinDerive` derives it — the Denning product-order dominance test:
//!
//! * `C`'s level DOMINATES `T`'s level (`gmeow:gmnRingLevelDominates`, read as authored — the
//!   full reflexive-transitive closure is hand-asserted per level, so no transitive computation
//!   is needed here), AND
//! * `C`'s compartment set CONTAINS `T`'s (`⊇`, over `gmeow:gmnRingCompartment`).
//!
//! This is an INTEGRITY (Biba-style) lattice: `gmeow:gmnRingCore` is the innermost, highest-
//! dominance ring whose (empty-compartment) content flows OUTWARD to every enclosing ring,
//! while `gmeow:gmnRingRestricted` content flows nowhere but itself. A compartment is a second,
//! `⊇`-tested axis: `gmeow:gmnRingNato` (level trusted, compartments {NATO, partner}) is a
//! genuinely different lattice point from `gmeow:gmnRingTrusted` (level trusted, no compartment).
//! Because admission requires `compartments(C) ⊇ compartments(T)`, a NATO-compartmented TARGET
//! admits ONLY content that also carries those compartments: plain same-level content
//! (`gmnRingTrusted`) — and even higher-level `gmnRingCore` content — is EXCLUDED from a
//! `gmnRingNato` target, the compartment axis creating an exclusion two rings at the same level
//! would not. `gmnRingWithin` is NEVER hand-authored; a zero-authored-triples structural gate
//! fails the build on one. This function reads only the authored coordinates and reproduces the
//! same relation the native reasoner materializes — it never requires a reasoner pass.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::{RdfDataset, RdfQuad, RdfTerm};

use crate::gmn1_codec::{Gmn0Model, Gmn1Error, GmnDictionary, gmn1_write};

/// `gmeow:gmnContentRing` — the claim-scoped content-subject → ring classification tag the
/// consume filter reads (distinct from the envelope-scoped `gmeow:gmnSecurityRing`).
const PRED_CONTENT_RING: &str = "https://blackcatinformatics.ca/gmeow/gmnContentRing";
/// `gmeow:gmnRingLevel` — the (functional) classification-level coordinate of a ring.
const PRED_RING_LEVEL: &str = "https://blackcatinformatics.ca/gmeow/gmnRingLevel";
/// `gmeow:gmnRingCompartment` — the (non-functional) compartment/caveat coordinate of a ring.
const PRED_RING_COMPARTMENT: &str = "https://blackcatinformatics.ca/gmeow/gmnRingCompartment";
/// `gmeow:gmnRingLevelDominates` — the ordered level axis (reflexive-transitive as authored).
const PRED_LEVEL_DOMINATES: &str = "https://blackcatinformatics.ca/gmeow/gmnRingLevelDominates";

/// `lang:GmnRingLeak` — the named runtime consume-path leakage failure class.
pub const CLASS_RING_LEAK: &str = "https://blackcatinformatics.ca/lang/GmnRingLeak";
/// `lang:GmnRingLatticeMalformed` — the class for an unresolvable / ill-formed ring reference.
pub const CLASS_RING_LATTICE_MALFORMED: &str =
    "https://blackcatinformatics.ca/lang/GmnRingLatticeMalformed";

// ── The ring lattice, resolved from the authored coordinates ─────────────────────────

/// One ring's two authored coordinates: its classification level and its compartment set.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RingCoord {
    level: String,
    compartments: BTreeSet<String>,
}

/// The GMN security-ring lattice resolved from authored `(level, compartment)` coordinates —
/// the checkout-free substrate the consume filter tests admissibility against.
///
/// Built by [`RingLattice::from_dataset`] over the shipped bundle (or a `module.ttl`): it reads
/// `gmeow:gmnRingLevel` / `gmeow:gmnRingCompartment` off each ring individual and the
/// `gmeow:gmnRingLevelDominates` ladder off each level individual, and computes the DERIVED
/// `gmnRingWithin` order on demand — never reading a hand-authored within edge (there are none;
/// a structural gate forbids them).
#[derive(Debug, Clone, Default)]
pub struct RingLattice {
    /// ring IRI → its `(level, compartments)` coordinates.
    rings: BTreeMap<String, RingCoord>,
    /// level IRI → the set of levels it dominates (reflexive-transitive, exactly as authored).
    level_dominates: BTreeMap<String, BTreeSet<String>>,
}

impl RingLattice {
    /// Resolve the lattice from a dataset carrying the authored ring / level coordinates
    /// (the shipped `gmeow.gts` bundle, or a lang `module.ttl`). Any subject carrying a
    /// `gmeow:gmnRingLevel` is a ring; any subject carrying `gmeow:gmnRingLevelDominates` is a
    /// level. Reads only IRI coordinates; a coordinate pointing at a non-IRI is ignored (the
    /// authoring-time `lang:GmnRingLatticeMalformed` gate is what forbids that in the carrier).
    #[must_use]
    pub fn from_dataset(ds: &RdfDataset) -> Self {
        let mut rings: BTreeMap<String, RingCoord> = BTreeMap::new();
        let mut level_dominates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for quad in ds.owned_quads() {
            let RdfTerm::Iri(subject) = &quad.subject else {
                continue;
            };
            match quad.predicate.as_str() {
                PRED_RING_LEVEL => {
                    if let RdfTerm::Iri(level) = &quad.object {
                        rings
                            .entry(subject.clone())
                            .or_insert_with(|| RingCoord {
                                level: String::new(),
                                compartments: BTreeSet::new(),
                            })
                            .level = level.clone();
                    }
                }
                PRED_RING_COMPARTMENT => {
                    if let RdfTerm::Iri(comp) = &quad.object {
                        rings
                            .entry(subject.clone())
                            .or_insert_with(|| RingCoord {
                                level: String::new(),
                                compartments: BTreeSet::new(),
                            })
                            .compartments
                            .insert(comp.clone());
                    }
                }
                PRED_LEVEL_DOMINATES => {
                    if let RdfTerm::Iri(dominated) = &quad.object {
                        level_dominates
                            .entry(subject.clone())
                            .or_default()
                            .insert(dominated.clone());
                    }
                }
                _ => {}
            }
        }
        Self {
            rings,
            level_dominates,
        }
    }

    /// Whether `ring` resolves to authored coordinates in this lattice.
    #[must_use]
    pub fn contains(&self, ring: &str) -> bool {
        self.rings.contains_key(ring)
    }

    /// The number of rings resolved (provenance for callers / tests).
    #[must_use]
    pub fn len(&self) -> usize {
        self.rings.len()
    }

    /// Whether the lattice resolved no rings at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rings.is_empty()
    }

    /// The DERIVED `gmeow:gmnRingWithin(x, y)`: `x`'s level dominates `y`'s AND
    /// `compartments(x) ⊇ compartments(y)` — the Denning product-order the derivation rule
    /// encodes, computed directly from the authored coordinates. `None` when either ring is
    /// unresolvable (an out-of-lattice reference).
    #[must_use]
    pub fn within(&self, x: &str, y: &str) -> Option<bool> {
        let cx = self.rings.get(x)?;
        let cy = self.rings.get(y)?;
        let level_ok = self
            .level_dominates
            .get(&cx.level)
            .is_some_and(|dominated| dominated.contains(&cy.level));
        let compartments_ok = cy.compartments.is_subset(&cx.compartments);
        Some(level_ok && compartments_ok)
    }

    /// Whether content classified at ring `content` is ADMISSIBLE into target ring `target`:
    /// `content == target` OR `gmnRingWithin(content, target)`. `None` when either ring is
    /// unresolvable.
    #[must_use]
    pub fn admissible(&self, content: &str, target: &str) -> Option<bool> {
        if content == target {
            // Both must still resolve — an unknown ring equal to itself is not admissible data.
            return Some(self.contains(content));
        }
        self.within(content, target)
    }
}

// ── The named consume-path failure ───────────────────────────────────────────────────

/// A consume-path filter failure — every variant a case where admitting or dropping content
/// would leak the boundary the ring model enforces, so the filter HARD-FAILS instead. Its
/// [`failure_class`](GmnConsumeError::failure_class) names the shipped `lang:` failure class a
/// meta-fold joins by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GmnConsumeError {
    /// The requested TARGET ring does not resolve in the lattice (`lang:GmnRingLatticeMalformed`).
    UnknownTargetRing { target: String },
    /// A content subject carrying data reaches the filter with no single resolvable
    /// `gmeow:gmnContentRing` tag — zero, or two distinct (`lang:GmnRingLeak`).
    Unclassified { subject: String },
    /// A content subject is tagged with a ring that does not resolve in the lattice
    /// (`lang:GmnRingLeak` — its admission class cannot be computed, so admitting it leaks).
    UnresolvableContentRing { subject: String, ring: String },
    /// An ADMITTED claim references, in object position, another content subject whose ring is
    /// NOT admissible into the target: the excluded subject's identity would leak into the
    /// admitted projection (`lang:GmnRingLeak`).
    ReferenceLeak {
        admitted: String,
        admitted_ring: String,
        excluded: String,
        excluded_ring: String,
        target: String,
    },
    /// Shared structure (a blank node) is reachable from BOTH admissible and non-admissible
    /// content, so neither dropping nor keeping it preserves the boundary (`lang:GmnRingLeak`).
    EntangledStructure { node: String },
    /// Projecting the admitted content to GMN-1 failed — the codec's own typed failure, whose
    /// class is delegated (a GMN-1 coverage / grammar failure, not a ring leak).
    Codec(Gmn1Error),
}

impl GmnConsumeError {
    /// The shipped `lang:` failure class this consume-path failure raises — the taxonomy a
    /// finding meta-fold joins by, never a free-text reason.
    #[must_use]
    pub fn failure_class(&self) -> &str {
        match self {
            Self::UnknownTargetRing { .. } => CLASS_RING_LATTICE_MALFORMED,
            Self::Unclassified { .. }
            | Self::UnresolvableContentRing { .. }
            | Self::ReferenceLeak { .. }
            | Self::EntangledStructure { .. } => CLASS_RING_LEAK,
            Self::Codec(e) => e.failure_class(),
        }
    }
}

impl std::fmt::Display for GmnConsumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTargetRing { target } => write!(
                f,
                "lang:GmnRingLatticeMalformed: target ring <{target}> does not resolve to authored \
                 lattice coordinates"
            ),
            Self::Unclassified { subject } => write!(
                f,
                "lang:GmnRingLeak: content subject <{subject}> carries no single \
                 gmeow:gmnContentRing tag — its admission class was never checked"
            ),
            Self::UnresolvableContentRing { subject, ring } => write!(
                f,
                "lang:GmnRingLeak: content subject <{subject}> is tagged ring <{ring}>, which does \
                 not resolve in the lattice — its admission class cannot be computed"
            ),
            Self::ReferenceLeak {
                admitted,
                admitted_ring,
                excluded,
                excluded_ring,
                target,
            } => write!(
                f,
                "lang:GmnRingLeak: admitted claim <{admitted}> (ring <{admitted_ring}>) references \
                 out-of-ring subject <{excluded}> (ring <{excluded_ring}>), which is not \
                 admissible into target <{target}> — its identity would leak into the projection"
            ),
            Self::EntangledStructure { node } => write!(
                f,
                "lang:GmnRingLeak: shared structure {node} is reachable from both admissible and \
                 non-admissible content — neither keeping nor dropping it preserves the boundary"
            ),
            Self::Codec(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for GmnConsumeError {}

// ── The projection product ───────────────────────────────────────────────────────────

/// The result of a successful consume-path projection: the ring-filtered, budget-fit GMN-1
/// text plus the counts that make its admission and elision decisions auditable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumeProjection {
    /// The GMN-1 document text of the admitted-and-fitted content (the shippable payload).
    pub text: String,
    /// The target ring IRI the content was admitted into.
    pub target: String,
    /// The number of content claims (canonical-subject groups) present in the input.
    pub input_claims: usize,
    /// The number admitted into the target (before any budget elision).
    pub admitted_claims: usize,
    /// The number EXCLUDED as out-of-ring (cleanly dropped — the designed non-failure path).
    pub excluded_claims: usize,
    /// The number actually EMITTED after the token-budget fit.
    pub emitted_claims: usize,
    /// The number of admitted claims ELIDED to fit the budget (`admitted − emitted`).
    pub elided_claims: usize,
    /// The token budget applied, if any.
    pub budget: Option<u64>,
    /// The estimated token count of the emitted GMN-1 text.
    pub tokens: u64,
    /// The elision disclosure — `Some(..)` iff [`elided_claims`](Self::elided_claims) `> 0`.
    /// Never a silent truncation: the elided remainder is disclosed, never dropped in silence.
    pub disclosure: Option<String>,
}

// ── The deterministic, model-agnostic token estimate ─────────────────────────────────

/// One token per ~4 characters, rounded up — the standard rough byte-pair ratio, identical to
/// [`crate::gmn_metrics`]' `estimate_tokens` and `gmeow_docs::llms::estimate_tokens` (re-declared
/// rather than depended on: `gmeow-docs` sits downstream of this crate). Empty in → `0`.
#[must_use]
fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

// ── Claim partitioning (the canonical-subject group unit) ────────────────────────────

/// A stable string key for a subject term — the partition key a claim groups under. IRIs and
/// blank nodes key by their label; the codec's GMN-0 subjects are only IRIs or blank nodes.
fn subject_key(term: &RdfTerm) -> Option<(bool, String)> {
    match term {
        RdfTerm::Iri(iri) => Some((false, iri.clone())),
        RdfTerm::BlankNode(b) => Some((true, b.clone())),
        _ => None,
    }
}

// ── The consume-path filter ──────────────────────────────────────────────────────────

/// The consume-path security-ring filter: canonicalize → ring-filter → GMN-1 → budget-fit.
///
/// 1. **Canonicalize.** The input is already a [`Gmn0Model`] (dedup + canonical sort); its
///    quads are partitioned into claims by canonical subject.
/// 2. **Ring-filter.** Each content subject's `gmeow:gmnContentRing` tag is read; admissibility
///    into `target` is the DERIVED `gmnRingWithin` product-order over `lattice`. Admissible
///    claims are KEPT, out-of-ring claims EXCLUDED (dropped). Any condition under which
///    exclusion would leak — unclassified content, an admitted→excluded reference, entangled
///    shared structure — is a HARD [`GmnConsumeError`] (`lang:GmnRingLeak`), never a silent drop.
/// 3. **GMN-1.** The admitted quads are projected to GMN-1 via [`gmn1_write`]; an uncovered
///    construct is the codec's own typed failure, delegated.
/// 4. **Budget-fit.** If `budget` is `Some` and the full projection exceeds it, whole claims
///    are elided from the tail of canonical order (always at least one emitted) and the elided
///    remainder DISCLOSED — never a silent mid-document truncation.
///
/// The `gmeow:gmnContentRing` tag triples themselves ride with their (admitted) claim into the
/// projection — they are content classification the consumer sees, not stripped in silence; a
/// tag triple's ring-IRI object is lattice vocabulary, never itself a tagged content subject, so
/// it never triggers the reference-leak check.
///
/// # Errors
///
/// Returns [`GmnConsumeError`] on an unresolvable target ring, any ring-leak condition, or a
/// codec failure projecting the admitted content.
pub fn consume_project(
    model: &Gmn0Model,
    lattice: &RingLattice,
    target: &str,
    budget: Option<u64>,
    dict: &GmnDictionary,
) -> Result<ConsumeProjection, GmnConsumeError> {
    if !lattice.contains(target) {
        return Err(GmnConsumeError::UnknownTargetRing {
            target: target.to_owned(),
        });
    }

    // ── (1) read the content-subject ring tags ──────────────────────────────────────
    // subject IRI → the distinct rings tagged on it (exactly one is well-formed).
    let mut tags: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for quad in &model.quads {
        if quad.predicate != PRED_CONTENT_RING {
            continue;
        }
        if let (RdfTerm::Iri(subject), RdfTerm::Iri(ring)) = (&quad.subject, &quad.object) {
            tags.entry(subject.clone())
                .or_default()
                .insert(ring.clone());
        }
    }
    // The single resolved ring per subject (else this subject is a leak once it carries data).
    let mut subject_ring: BTreeMap<String, String> = BTreeMap::new();
    for (subject, rings) in &tags {
        if rings.len() == 1 {
            subject_ring.insert(
                subject.clone(),
                rings.iter().next().expect("len==1").clone(),
            );
        }
    }

    // ── (2) partition into claims by canonical subject; classify each IRI claim ──────
    // A "content subject" carries a DATA quad (predicate other than the ring tag). Every IRI
    // content subject MUST carry exactly one resolvable, in-lattice ring, else it is a leak.
    let mut claims: BTreeMap<(bool, String), Vec<&RdfQuad>> = BTreeMap::new();
    for quad in &model.quads {
        let Some(key) = subject_key(&quad.subject) else {
            continue;
        };
        claims.entry(key).or_default().push(quad);
    }

    // Admissibility per IRI content subject.
    let mut admitted_iri: BTreeSet<String> = BTreeSet::new();
    let mut excluded_iri: BTreeMap<String, String> = BTreeMap::new(); // subject → its ring
    let mut input_claims = 0usize;
    for ((is_blank, label), quads) in &claims {
        if *is_blank {
            continue; // blank-node structure is resolved by reachability below
        }
        let carries_data = quads.iter().any(|q| q.predicate != PRED_CONTENT_RING);
        if !carries_data {
            continue; // a bare tag-only subject contributes no payload claim
        }
        input_claims += 1;
        let Some(ring) = subject_ring.get(label) else {
            return Err(GmnConsumeError::Unclassified {
                subject: label.clone(),
            });
        };
        match lattice.admissible(ring, target) {
            None => {
                return Err(GmnConsumeError::UnresolvableContentRing {
                    subject: label.clone(),
                    ring: ring.clone(),
                });
            }
            Some(true) => {
                admitted_iri.insert(label.clone());
            }
            Some(false) => {
                excluded_iri.insert(label.clone(), ring.clone());
            }
        }
    }

    // ── (3) reference-leak: an admitted claim must not point at an excluded content subject ──
    for admitted in &admitted_iri {
        let key = (false, admitted.clone());
        for quad in claims.get(&key).into_iter().flatten() {
            if let RdfTerm::Iri(object) = &quad.object
                && let Some(excluded_ring) = excluded_iri.get(object)
            {
                let admitted_ring = subject_ring.get(admitted).cloned().unwrap_or_default();
                return Err(GmnConsumeError::ReferenceLeak {
                    admitted: admitted.clone(),
                    admitted_ring,
                    excluded: object.clone(),
                    excluded_ring: excluded_ring.clone(),
                    target: target.to_owned(),
                });
            }
        }
    }

    // ── (3b) blank-node reachability: shared structure must not straddle the boundary ──
    // Compute, per blank node, the set of admissibility verdicts of the IRI claims that reach
    // it (transitively via object edges). Mixed ⇒ entangled leak; all-admissible ⇒ include;
    // all-excluded ⇒ drop; unreachable data-bearing blank ⇒ unclassified.
    let blank_verdicts = blank_reachability(&claims, &admitted_iri, &excluded_iri);
    let mut admitted_blank: BTreeSet<String> = BTreeSet::new();
    for ((is_blank, label), quads) in &claims {
        if !*is_blank {
            continue;
        }
        let carries_data = quads.iter().any(|q| q.predicate != PRED_CONTENT_RING);
        match blank_verdicts.get(label) {
            Some(BlankVerdict::Admissible) => {
                admitted_blank.insert(label.clone());
            }
            Some(BlankVerdict::Excluded) => {}
            Some(BlankVerdict::Mixed) => {
                return Err(GmnConsumeError::EntangledStructure {
                    node: format!("_:{label}"),
                });
            }
            None => {
                if carries_data {
                    return Err(GmnConsumeError::Unclassified {
                        subject: format!("_:{label}"),
                    });
                }
            }
        }
    }

    let excluded_claims = excluded_iri.len();

    // ── (4) assemble the admitted model in canonical order, then budget-fit by whole claim ──
    // Emit admitted IRI claims in canonical subject order; each carries its own blank-node
    // structure (already verdicted admissible) with it.
    let mut admitted_claim_keys: Vec<String> = admitted_iri.iter().cloned().collect();
    admitted_claim_keys.sort();
    let admitted_claims = admitted_claim_keys.len();

    // The quads each admitted claim contributes (its own quads + the admissible blank structure
    // it introduces). A blank node is attributed to the FIRST admitted claim (canonical order)
    // that references it, so each admissible blank rides exactly one emitted claim.
    let claim_quads = assemble_claim_quads(&claims, &admitted_claim_keys, &admitted_blank);

    // Greedy prefix fit: include whole claims in canonical order while the running GMN-1 token
    // estimate stays within budget (always at least one), disclosing any elided remainder.
    let mut emitted_quads: Vec<RdfQuad> = Vec::new();
    let mut emitted_claims = 0usize;
    let mut last_text = String::new();
    let mut last_tokens = 0u64;
    for key in &admitted_claim_keys {
        let mut candidate = emitted_quads.clone();
        for q in claim_quads.get(key).into_iter().flatten() {
            candidate.push((*q).clone());
        }
        let candidate_model = Gmn0Model {
            quads: candidate.clone(),
        };
        let doc = gmn1_write(&candidate_model, dict).map_err(GmnConsumeError::Codec)?;
        let tokens = estimate_tokens(&doc.text);
        if emitted_claims > 0
            && let Some(b) = budget
            && tokens > b
        {
            break; // stop before the budget is exceeded — a hard cap, never a mid-claim cut
        }
        emitted_quads = candidate;
        emitted_claims += 1;
        last_text = doc.text;
        last_tokens = tokens;
    }

    // An all-excluded (or empty) input emits an empty projection — still a valid GMN-1 document.
    if emitted_claims == 0 {
        let empty = Gmn0Model { quads: Vec::new() };
        let doc = gmn1_write(&empty, dict).map_err(GmnConsumeError::Codec)?;
        last_tokens = estimate_tokens(&doc.text);
        last_text = doc.text;
    }

    let elided_claims = admitted_claims - emitted_claims;
    let disclosure = (elided_claims > 0).then(|| {
        let budget_note = budget.map_or_else(
            || "the applied token budget".to_owned(),
            |b| format!("the {b}-token budget"),
        );
        format!(
            "{elided_claims} of {admitted_claims} admitted claims elided to fit {budget_note} \
             (target ring <{target}>); the elided remainder is available at a higher budget or a \
             broader target ring, never silently dropped."
        )
    });

    Ok(ConsumeProjection {
        text: last_text,
        target: target.to_owned(),
        input_claims,
        admitted_claims,
        excluded_claims,
        emitted_claims,
        elided_claims,
        budget,
        tokens: last_tokens,
        disclosure,
    })
}

/// The admissibility verdict a blank node inherits from the IRI claims that reach it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlankVerdict {
    Admissible,
    Excluded,
    Mixed,
}

/// Propagate each IRI claim's admissibility verdict to the blank nodes it reaches, transitively
/// through object edges, to a fixpoint. A blank reached only from admissible claims is
/// `Admissible`, only from excluded is `Excluded`, from both is `Mixed`.
fn blank_reachability(
    claims: &BTreeMap<(bool, String), Vec<&RdfQuad>>,
    admitted_iri: &BTreeSet<String>,
    excluded_iri: &BTreeMap<String, String>,
) -> BTreeMap<String, BlankVerdict> {
    // Seed: blanks referenced directly by an admissible / excluded IRI claim.
    let mut admissible_reach: BTreeSet<String> = BTreeSet::new();
    let mut excluded_reach: BTreeSet<String> = BTreeSet::new();
    for ((is_blank, label), quads) in claims {
        let seed = if !*is_blank && admitted_iri.contains(label) {
            Some(&mut admissible_reach)
        } else if !*is_blank && excluded_iri.contains_key(label) {
            Some(&mut excluded_reach)
        } else {
            None
        };
        if let Some(set) = seed {
            for q in quads {
                if let RdfTerm::BlankNode(b) = &q.object {
                    set.insert(b.clone());
                }
            }
        }
    }
    // Propagate blank → blank object edges to a fixpoint.
    propagate_blank_edges(claims, &mut admissible_reach);
    propagate_blank_edges(claims, &mut excluded_reach);

    let mut verdicts: BTreeMap<String, BlankVerdict> = BTreeMap::new();
    for b in admissible_reach.union(&excluded_reach) {
        let a = admissible_reach.contains(b);
        let e = excluded_reach.contains(b);
        let verdict = match (a, e) {
            (true, true) => BlankVerdict::Mixed,
            (true, false) => BlankVerdict::Admissible,
            (false, true) => BlankVerdict::Excluded,
            (false, false) => continue,
        };
        verdicts.insert(b.clone(), verdict);
    }
    verdicts
}

/// Expand a blank-node reach set along blank-subject → blank-object edges until fixpoint.
fn propagate_blank_edges(
    claims: &BTreeMap<(bool, String), Vec<&RdfQuad>>,
    reach: &mut BTreeSet<String>,
) {
    loop {
        let mut added = false;
        for ((is_blank, label), quads) in claims {
            if !*is_blank || !reach.contains(label) {
                continue;
            }
            for q in quads {
                if let RdfTerm::BlankNode(b) = &q.object
                    && reach.insert(b.clone())
                {
                    added = true;
                }
            }
        }
        if !added {
            break;
        }
    }
}

/// Gather, per admitted IRI claim (in canonical order), the quads it contributes: its own quads
/// plus each admissible blank node's quads, attributed to the FIRST admitted claim that reaches
/// the blank (so each blank rides exactly one emitted claim, never duplicated).
fn assemble_claim_quads<'a>(
    claims: &BTreeMap<(bool, String), Vec<&'a RdfQuad>>,
    admitted_claim_keys: &[String],
    admitted_blank: &BTreeSet<String>,
) -> BTreeMap<String, Vec<&'a RdfQuad>> {
    let mut out: BTreeMap<String, Vec<&'a RdfQuad>> = BTreeMap::new();
    let mut blank_claimed: BTreeSet<String> = BTreeSet::new();
    for key in admitted_claim_keys {
        let mut bucket: Vec<&RdfQuad> = Vec::new();
        let mut frontier: Vec<String> = Vec::new();
        if let Some(quads) = claims.get(&(false, key.clone())) {
            for q in quads {
                bucket.push(*q);
                if let RdfTerm::BlankNode(b) = &q.object
                    && admitted_blank.contains(b)
                    && blank_claimed.insert(b.clone())
                {
                    frontier.push(b.clone());
                }
            }
        }
        // Pull in the (admissible) blank structure this claim introduces, transitively.
        while let Some(bn) = frontier.pop() {
            if let Some(quads) = claims.get(&(true, bn.clone())) {
                for q in quads {
                    bucket.push(*q);
                    if let RdfTerm::BlankNode(b) = &q.object
                        && admitted_blank.contains(b)
                        && blank_claimed.insert(b.clone())
                    {
                        frontier.push(b.clone());
                    }
                }
            }
        }
        out.insert(key.clone(), bucket);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::parse_dataset;

    const CORE: &str = "https://blackcatinformatics.ca/gmeow/gmnRingCore";
    const TRUSTED: &str = "https://blackcatinformatics.ca/gmeow/gmnRingTrusted";
    const RESTRICTED: &str = "https://blackcatinformatics.ca/gmeow/gmnRingRestricted";
    const NATO: &str = "https://blackcatinformatics.ca/gmeow/gmnRingNato";

    /// The SHIPPED authored lang carrier — the real (level, compartment) ring coordinates and
    /// the pinned GMN dictionary the codec projects against.
    fn lang_module_dataset() -> std::sync::Arc<RdfDataset> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../slices/grounding/lang/module.ttl"
        );
        let bytes = std::fs::read(path).expect("lang module.ttl is readable");
        parse_dataset(&bytes, "text/turtle", None).expect("lang module.ttl parses")
    }

    fn lattice() -> RingLattice {
        RingLattice::from_dataset(&lang_module_dataset())
    }

    fn dict() -> GmnDictionary {
        GmnDictionary::from_dataset(&lang_module_dataset()).expect("dictionary loads")
    }

    /// The ring-tagged consume-path demonstrator (four claims at four rings).
    fn demonstrator() -> Gmn0Model {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../slices/grounding/lang/examples/gmn-ring-consume.ttl"
        );
        let bytes = std::fs::read(path).expect("demonstrator is readable");
        let ds = parse_dataset(&bytes, "text/turtle", None).expect("demonstrator parses");
        Gmn0Model::from_dataset(&ds)
    }

    // ── the within-closure resolves from the authored coordinates ────────────────────

    #[test]
    fn within_closure_matches_the_authored_lattice() {
        let l = lattice();
        assert!(
            l.contains(CORE) && l.contains(TRUSTED) && l.contains(RESTRICTED) && l.contains(NATO)
        );
        // Level axis: core flows outward to every ring; restricted flows nowhere but itself.
        assert_eq!(l.within(CORE, TRUSTED), Some(true), "core within trusted");
        assert_eq!(
            l.within(CORE, RESTRICTED),
            Some(true),
            "core within restricted"
        );
        assert_eq!(
            l.within(TRUSTED, CORE),
            Some(false),
            "trusted not within core"
        );
        assert_eq!(
            l.within(RESTRICTED, TRUSTED),
            Some(false),
            "restricted not within trusted"
        );
        // Compartment axis (⊇): nato (level trusted, {NATO,partner}) is within trusted, but
        // plain trusted is NOT within nato — the compartment set is not contained.
        assert_eq!(l.within(NATO, TRUSTED), Some(true), "nato within trusted");
        assert_eq!(
            l.within(TRUSTED, NATO),
            Some(false),
            "trusted lacks nato's compartments"
        );
        assert_eq!(
            l.within(CORE, NATO),
            Some(false),
            "even core lacks nato's compartments"
        );
        // An out-of-lattice reference is unknown, never silently false-as-admissible.
        assert_eq!(l.within("https://example.org/notARing", CORE), None);
    }

    // ── consume_path_excludes_out_of_ring_content (the flagship exclusion tooth) ──────

    /// Given content at four rings and a TARGET ring, the emitted GMN EXCLUDES every
    /// out-of-ring claim (its datum token is absent) and ADMITS every admissible one. Covers a
    /// level-axis exclusion (restricted excluded from trusted) AND a compartment-axis exclusion
    /// (plain trusted excluded from a nato target at the SAME level). Falsifiable: were the
    /// filter a no-op, the excluded tokens would be present.
    #[test]
    fn consume_path_excludes_out_of_ring_content() {
        let l = lattice();
        let d = dict();
        let model = demonstrator();

        // Target = trusted: admits core + trusted + nato, excludes restricted (level axis).
        let p = consume_project(&model, &l, TRUSTED, None, &d).expect("trusted projection");
        assert!(
            p.text.contains("ringDemoCoreDatum"),
            "core admitted into trusted"
        );
        assert!(
            p.text.contains("ringDemoTrustedDatum"),
            "trusted admitted into trusted"
        );
        assert!(
            p.text.contains("ringDemoNatoDatum"),
            "nato (⊇ compartments) admitted into trusted"
        );
        assert!(
            !p.text.contains("ringDemoRestrictedDatum"),
            "restricted EXCLUDED from trusted — its level dominates nothing above restricted"
        );
        assert_eq!(p.excluded_claims, 1);
        assert_eq!(p.admitted_claims, 3);

        // Target = nato: the compartment axis excludes plain same-level trusted content AND
        // higher-level core content — only nato-compartmented content is admitted.
        let p = consume_project(&model, &l, NATO, None, &d).expect("nato projection");
        assert!(
            p.text.contains("ringDemoNatoDatum"),
            "nato admitted into nato (equal ring)"
        );
        assert!(
            !p.text.contains("ringDemoTrustedDatum"),
            "plain trusted EXCLUDED from nato target — compartment set not ⊇ {{NATO,partner}}"
        );
        assert!(
            !p.text.contains("ringDemoCoreDatum"),
            "even core EXCLUDED from nato target — lacks the NATO compartments"
        );
        assert!(!p.text.contains("ringDemoRestrictedDatum"));
        assert_eq!(
            p.admitted_claims, 1,
            "only nato content is admissible into nato"
        );
        assert_eq!(p.excluded_claims, 3);

        // Target = core: the strictest bar — only core content (empty compartments, top level).
        let p = consume_project(&model, &l, CORE, None, &d).expect("core projection");
        assert!(p.text.contains("ringDemoCoreDatum"));
        assert!(!p.text.contains("ringDemoTrustedDatum"));
        assert!(!p.text.contains("ringDemoNatoDatum"));
        assert!(!p.text.contains("ringDemoRestrictedDatum"));
        assert_eq!(p.admitted_claims, 1);
    }

    // ── consume_path_fit_discloses_elision (never a silent truncation) ────────────────

    /// When the admitted content exceeds the token budget, the projection EMITS a prefix of
    /// whole claims and DISCLOSES the elided remainder ("N of M admitted claims elided …"),
    /// never silently truncating. Falsifiable: a silent truncation would leave `disclosure`
    /// `None` while `emitted_claims < admitted_claims`.
    #[test]
    fn consume_path_fit_discloses_elision() {
        let l = lattice();
        let d = dict();
        let model = demonstrator();

        // Restricted target admits only restricted (one claim) — measure the unbudgeted size.
        let full = consume_project(&model, &l, TRUSTED, None, &d).expect("full trusted projection");
        assert_eq!(full.elided_claims, 0);
        assert!(full.disclosure.is_none(), "no elision ⇒ no disclosure");
        assert!(
            full.tokens > 1,
            "the full projection has a measurable token size"
        );

        // A budget that fits fewer than all admitted claims forces elision.
        let tight = full.tokens - 1;
        let p = consume_project(&model, &l, TRUSTED, Some(tight), &d).expect("budgeted projection");
        assert!(
            p.emitted_claims < p.admitted_claims,
            "a tight budget elides at least one admitted claim"
        );
        assert!(
            p.emitted_claims >= 1,
            "always emits at least one claim (a hard cap)"
        );
        assert_eq!(p.elided_claims, p.admitted_claims - p.emitted_claims);
        let disclosure = p.disclosure.expect("elision must be disclosed");
        assert!(
            disclosure.contains(&format!(
                "{} of {} admitted claims elided",
                p.elided_claims, p.admitted_claims
            )),
            "disclosure names the elided remainder: {disclosure}"
        );
        assert!(disclosure.contains("never silently dropped"));
    }

    // ── the named leakage failure (lang:GmnRingLeak) ─────────────────────────────────

    /// Unclassified content — a subject carrying data but no ring tag — is a HARD
    /// `lang:GmnRingLeak`, never silently admitted or dropped.
    #[test]
    fn unclassified_content_raises_the_named_leak_class() {
        let l = lattice();
        let d = dict();
        let ttl = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
                   gmeow:ringDemoOrphan gmeow:ringDemoField gmeow:ringDemoOrphanDatum .\n";
        let ds = parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parses");
        let model = Gmn0Model::from_dataset(&ds);
        let err = consume_project(&model, &l, TRUSTED, None, &d).expect_err("must leak");
        assert!(matches!(err, GmnConsumeError::Unclassified { .. }), "{err}");
        assert_eq!(err.failure_class(), CLASS_RING_LEAK);
    }

    /// An admitted claim that references, in object position, an excluded content subject is a
    /// HARD `lang:GmnRingLeak` — the excluded subject's identity would otherwise leak into the
    /// admitted projection.
    #[test]
    fn admitted_reference_to_excluded_content_raises_the_named_leak_class() {
        let l = lattice();
        let d = dict();
        // Core content (admissible into trusted) points at restricted content (excluded).
        let ttl = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
                   gmeow:ringDemoCore gmeow:gmnContentRing gmeow:gmnRingCore ;\n\
                       gmeow:ringDemoRefers gmeow:ringDemoRestricted .\n\
                   gmeow:ringDemoRestricted gmeow:gmnContentRing gmeow:gmnRingRestricted ;\n\
                       gmeow:ringDemoField gmeow:ringDemoRestrictedDatum .\n";
        let ds = parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parses");
        let model = Gmn0Model::from_dataset(&ds);
        let err = consume_project(&model, &l, TRUSTED, None, &d).expect_err("must leak");
        assert!(
            matches!(err, GmnConsumeError::ReferenceLeak { .. }),
            "{err}"
        );
        assert_eq!(err.failure_class(), CLASS_RING_LEAK);
    }

    /// An unresolvable TARGET ring is a HARD `lang:GmnRingLatticeMalformed`, never a degraded
    /// default.
    #[test]
    fn unknown_target_ring_hard_fails() {
        let l = lattice();
        let d = dict();
        let model = demonstrator();
        let err = consume_project(&model, &l, "https://example.org/notARing", None, &d)
            .expect_err("unknown target");
        assert!(
            matches!(err, GmnConsumeError::UnknownTargetRing { .. }),
            "{err}"
        );
        assert_eq!(err.failure_class(), CLASS_RING_LATTICE_MALFORMED);
    }
}
