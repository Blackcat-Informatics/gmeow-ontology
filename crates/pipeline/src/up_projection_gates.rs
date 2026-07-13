// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Re-derive the up-projection invertibility audit through the **correspondence gates**.
//!
//! The legacy audit ([`crate::up_projection_corpus::run_audit_nt`]) assigned each external target
//! term a heuristic bucket (`clean | liftable-with-claim | hard-mint | down-only | GAP`) and
//! reported the raw count of `clean + liftable-with-claim` as the "liftable" headline. That
//! number was never *verified*: nothing checked that a term's reverse (up-lift) rule actually
//! inverts the forward (down) projection that defines it.
//!
//! This module realizes every liftable target term as a `logic:Correspondence` and runs the
//! whole set through [`evaluate_gates`] + [`liftability`], so the headline becomes a
//! gate-verdict ledger. The gate adds verification the bucket cannot, and it does so without
//! either of the two tautology traps:
//!
//! 1. **Bucket→class tautology** — the morphism rung is derived from the term's lift evidence,
//!    but the *proof* of liftability is the round-trip gate over real leg bodies, which the
//!    bucket does not encode.
//! 2. **Fabricated-leg-body** — a proved cell's two legs are **independently sourced** real
//!    projection rules: the forward leg is the term's EDOAL *direct* path (anchor as the
//!    subject of the projection atom) and the reverse leg is its EDOAL *inverse* path (anchor
//!    as the object). The round-trip gate's `put == get.invert()` therefore reduces to "is the
//!    inverse-path predicate the same as the direct-path predicate" — a genuine question, NOT
//!    a put minted as `get.invert()` (which would pass by construction).
//!
//! # The four liftability tiers (a strict partition of every audited term)
//!
//! * **proved** — the round-trip (or mnemomorphism) gate PASSES: a structurally verified
//!   inverse. Reachable only for a term carrying both a direct and an inverse EDOAL path whose
//!   predicates match.
//! * **claimed** — a liftable term (a `clean` or `liftable-with-claim` bucket) realized as a
//!   gate-passing correspondence that makes no full round-trip claim: the lift is asserted by
//!   the alignment relation, not proved by inversion (`ObligationUnknown`).
//! * **red_excluded** — a liftable bucket whose correspondence trips a gate (e.g. a term whose
//!   inverse path does NOT invert its direct path, or an equivalence overclaim). It is *not*
//!   lawful-liftable "even if its bucket said clean", and the residue is surfaced — never
//!   silently dropped.
//! * **unsupported** — a non-liftable bucket (`hard-mint`, `down-only`, `GAP`): no lift rule,
//!   so no correspondence is minted.
//!
//! The audit headline is `(proved + claimed) / total`, with the proved/claimed split disclosed
//! — preserving the coverage story while distinguishing proved-invertible from claimed-liftable.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::RdfTerm;

use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeVerdict,
    LawClaimIr, LegPath, MorphismClass, MorphismKind, PreservationKind, TransactionProgramIr,
};
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::projections::correspondence_gates::{
    GateReport, evaluate_gates, liftability,
};

use crate::up_projection_corpus::{
    AuditReport, GM_ANCHOR, GM_ATOM, GM_EDOAL_SOURCE, GM_HAS_BINDING, GM_HAS_MAPPING_PATTERN,
    GM_MNEMOMORPHIC, GM_OBJECT_VAR, GM_PREDICATE, GM_PROJECTION_MAPPING, GM_RELATION,
    GM_SUBJECT_VAR, GM_TO_CLASS, GM_TO_PREDICATE, Graph, RDF_TYPE, canon_qname, combined_class,
    decimal_confidence, edoalpath_pairs, in_projection_ns, objects, prefix, rdf_list, run_audit_nt,
    sssom_best_buckets_pub, sssom_clean_pairs, sssom_closematch_pairs, structural_best_classes_pub,
    structural_pairs, subjects, value, value_lexical, value_named,
};

/// The `logic:` namespace the minted audit-correspondence and leg IRIs live under.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The four-tier liftability counts for one vocabulary (or the whole audit).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct TierCounts {
    /// Round-trip / mnemomorphism gate PASS: a structurally verified inverse.
    pub proved: usize,
    /// A gate-passing liftable cell that makes no full round-trip claim (asserted, not proved).
    pub claimed: usize,
    /// A liftable bucket whose correspondence trips a gate (excluded from liftable).
    pub red_excluded: usize,
    /// A non-liftable bucket: no lift rule, so no correspondence is minted.
    pub unsupported: usize,
}

impl TierCounts {
    /// The liftable headline numerator for this group: proved + claimed.
    pub fn liftable(&self) -> usize {
        self.proved + self.claimed
    }

    /// The total audited terms in this group across all four tiers.
    pub fn total(&self) -> usize {
        self.proved + self.claimed + self.red_excluded + self.unsupported
    }
}

/// The gate-derived up-projection liftability ledger: the honest replacement for the
/// SSSOM-heuristic "81% liftable" headline. Every count is a gate verdict, never a bucket
/// re-read.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct AuditLedger {
    /// The whole-audit tier counts.
    pub totals: TierCounts,
    /// Per-vocabulary tier counts (gate-derived), keyed by the term's qname prefix.
    pub per_vocab: BTreeMap<String, TierCounts>,
    /// The coverage-gap terms (the `GAP` bucket), de-duplicated and sorted — carried through
    /// from the underlying audit so the markdown can keep its gaps section.
    pub gaps: Vec<String>,
}

impl AuditLedger {
    /// The headline liftable count (proved + claimed) over the whole audit.
    pub fn liftable(&self) -> usize {
        self.totals.liftable()
    }

    /// The total audited terms over the whole audit (the partition sum).
    pub fn total(&self) -> usize {
        self.totals.total()
    }
}

/// The lift evidence for one audited term — the input to [`classify_term`]. Derived from the
/// term's heuristic bucket and its EDOAL direct/inverse path predicates; the *policy* of how
/// evidence maps to a correspondence shape is isolated here so it is unit-testable on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiftEvidence {
    /// Both a direct (forward) and an inverse (reverse) EDOAL path exist for a liftable term:
    /// a proved-candidate whose two legs are independently-sourced real predicates. The
    /// round-trip gate verifies they invert (`direct == inverse`).
    VerifiableRoundTrip {
        /// The direct-path predicate (the forward leg's single step).
        direct: String,
        /// The inverse-path predicate (the reverse leg's single step, traversed inverted).
        inverse: String,
    },
    /// A `clean` bucket with no verifiable round-trip: an injective lift asserted by an exact
    /// alignment, but not proved by structural inversion.
    CleanAsserted,
    /// A `liftable-with-claim` bucket with no verifiable round-trip: a lossy lift carried under
    /// a claim (closeMatch / generalizing), honestly `ObligationUnknown`.
    ClaimedLift,
    /// A non-liftable bucket (`hard-mint`, `down-only`, `GAP`): no lift rule.
    Unsupported,
}

/// The correspondence shape a piece of lift evidence licenses: the typed relation/rung/kind,
/// the mnemomorphic bit, the authored law claims, and (for a proved candidate) the two real
/// leg-body predicates to register. `None` means no correspondence is minted (unsupported).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermShape {
    /// The alignment relation on the lattice.
    pub relation: CorrespondenceRelation,
    /// The rung on the law-spine.
    pub class: MorphismClass,
    /// The satisfaction-preserving / commitment-shifting qualifier.
    pub kind: MorphismKind,
    /// Whether the cell retains a source witness (always `false` here — the audit never
    /// fabricates a witness to force a PASS).
    pub mnemomorphic: bool,
    /// The authored law claims.
    pub laws: Vec<LawClaimIr>,
    /// For a proved candidate: `(direct, inverse)` predicate IRIs to register as the get/put
    /// leg bodies (`get = Step(direct)`, `put = Inverse(Step(inverse))`). `None` otherwise.
    pub legs: Option<(String, String)>,
}

/// Map a term's lift evidence to the correspondence shape it licenses — the single policy
/// function, isolated for unit testing. **Never** sets `mnemomorphic = true` (no fabricated
/// witness), and **never** maps a bucket straight to a proved rung: only `VerifiableRoundTrip`
/// produces the injective [`MorphismClass::SectionRetraction`] that makes a round-trip claim.
pub fn classify_term(evidence: &LiftEvidence) -> Option<TermShape> {
    match evidence {
        LiftEvidence::VerifiableRoundTrip { direct, inverse } => Some(TermShape {
            // A section/retraction: the forward leg has a structural inverse to verify. The
            // gate REDs it if the inverse path is not the inverse of the direct path.
            relation: CorrespondenceRelation::Equiv,
            class: MorphismClass::SectionRetraction,
            kind: MorphismKind::InstitutionMorphism,
            mnemomorphic: false,
            laws: Vec::new(),
            legs: Some((direct.clone(), inverse.clone())),
        }),
        LiftEvidence::CleanAsserted => Some(TermShape {
            // Injective by the exact alignment, but it makes NO full round-trip claim (no
            // verifiable inverse leg), so the round-trip gate is NotApplicable → claimed.
            relation: CorrespondenceRelation::Equiv,
            class: MorphismClass::WellBehavedLens,
            kind: MorphismKind::InstitutionMorphism,
            mnemomorphic: false,
            laws: Vec::new(),
            legs: None,
        }),
        LiftEvidence::ClaimedLift => Some(TermShape {
            // A lossy / co-projection lift carried under an honest unknown claim. The relation
            // is Overlaps (never Equiv), so the overclaim gate is satisfied by construction.
            relation: CorrespondenceRelation::Overlaps,
            class: MorphismClass::AffineCorrespondence,
            kind: MorphismKind::InstitutionMorphism,
            mnemomorphic: false,
            laws: vec![LawClaimIr {
                law: CorrespondenceLaw::GetPut,
                verdict: DischargeVerdict::ObligationUnknown,
                condition: None,
            }],
            legs: None,
        }),
        LiftEvidence::Unsupported => None,
    }
}

/// Derive the lift evidence for one term from its heuristic bucket and the EDOAL direct/inverse
/// path predicates available for it. A `clean`/`liftable-with-claim` term with exactly one
/// direct AND one inverse path is a verifiable round-trip; otherwise the bucket decides the
/// asserted/claimed/unsupported tier.
fn evidence_for(bucket: &str, direct: Option<&str>, inverse: Option<&str>) -> LiftEvidence {
    let liftable = matches!(bucket, "clean" | "liftable-with-claim");
    if liftable && let (Some(d), Some(i)) = (direct, inverse) {
        return LiftEvidence::VerifiableRoundTrip {
            direct: d.to_owned(),
            inverse: i.to_owned(),
        };
    }
    match bucket {
        "clean" => LiftEvidence::CleanAsserted,
        "liftable-with-claim" => LiftEvidence::ClaimedLift,
        _ => LiftEvidence::Unsupported,
    }
}

/// A single audited (file, term) cell carried alongside its minted correspondence IRI and the
/// vocabulary it belongs to, so the gate report can be attributed back to vocabularies.
struct AuditedCell {
    corr_iri: String,
    vocab: String,
}

/// Build one `logic:Correspondence` per liftable (file, term) cell from the audit, plus the
/// leg-program registry for the proved candidates. Returns the correspondences, the leg
/// registry, the per-cell attribution (corr IRI → vocab), and the per-vocab `unsupported`
/// counts (the non-liftable buckets, which mint no correspondence).
fn correspondences_from_audit(
    audit: &AuditReport,
    direct_q: &BTreeMap<String, String>,
    inverse_q: &BTreeMap<String, String>,
) -> (
    Vec<Correspondence>,
    Vec<TransactionProgramIr>,
    Vec<AuditedCell>,
    BTreeMap<String, usize>,
) {
    let mut corrs = Vec::new();
    let mut legs = Vec::new();
    let mut cells = Vec::new();
    let mut unsupported_by_vocab: BTreeMap<String, usize> = BTreeMap::new();

    for file in &audit.files {
        for (term, bucket) in &file.per_term {
            let vocab = prefix(term).to_owned();
            let evidence = evidence_for(
                bucket,
                direct_q.get(term).map(String::as_str),
                inverse_q.get(term).map(String::as_str),
            );
            let Some(shape) = classify_term(&evidence) else {
                *unsupported_by_vocab.entry(vocab).or_insert(0) += 1;
                continue;
            };
            // One stable correspondence IRI per (file, term): the audit counts terms per file,
            // so a term used in two corpora is two cells (matching the legacy `total`).
            let corr_iri = format!("{LOGIC_NS}up-projection-audit/{}/{}", file.name, slug(term));
            let (get_leg, put_leg) = match &shape.legs {
                Some((direct, inverse)) => {
                    let get_iri = format!("{corr_iri}/get");
                    let put_iri = format!("{corr_iri}/put");
                    legs.push(TransactionProgramIr {
                        iri: get_iri.clone(),
                        body: LegPath::Step(direct.clone()),
                    });
                    legs.push(TransactionProgramIr {
                        iri: put_iri.clone(),
                        body: LegPath::Inverse(Box::new(LegPath::Step(inverse.clone()))),
                    });
                    (Some(get_iri), Some(put_iri))
                }
                None => (None, None),
            };
            let determinacy = match shape.relation {
                CorrespondenceRelation::Overlaps => Some(Determinacy::Vague),
                _ => Some(Determinacy::Crisp),
            };
            let correspondence = Correspondence::new(
                corr_iri.clone(),
                shape.relation,
                shape.class,
                shape.kind,
                shape.mnemomorphic,
                determinacy,
                get_leg,
                put_leg,
                shape.laws,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("audit correspondence is well-formed by construction");
            cells.push(AuditedCell { corr_iri, vocab });
            corrs.push(correspondence);
        }
    }
    (corrs, legs, cells, unsupported_by_vocab)
}

/// A collision-free, URL-safe slug for a term qname, used to mint a stable
/// correspondence IRI per term. Alphanumerics plus `-`/`_` pass through verbatim
/// (both are URL-safe and must stay distinct so e.g. `foo-bar` and `foo_bar` mint
/// different IRIs); every other byte is percent-encoded, which is injective.
fn slug(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for &b in term.as_bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Which tier a gate report places a correspondence in. A PASS on the round-trip OR
/// mnemomorphism gate is *proved*; any RED gate is *red_excluded* (criterion #3 — not
/// lawful-liftable even if the bucket said clean); otherwise the cell is a gate-passing
/// *claimed* lift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    Proved,
    Claimed,
    RedExcluded,
}

fn tier_of(report: &GateReport) -> Tier {
    use gmeow_logic_compile::projections::correspondence_gates::GateVerdict;
    if matches!(report.round_trip, GateVerdict::Pass)
        || matches!(report.mnemomorphism, GateVerdict::Pass)
    {
        Tier::Proved
    } else if report.law.is_red()
        || report.overclaim.is_red()
        || report.round_trip.is_red()
        || report.mnemomorphism.is_red()
    {
        Tier::RedExcluded
    } else {
        Tier::Claimed
    }
}

/// Compute the gate-derived [`AuditLedger`] from the audit inputs: build a correspondence per
/// liftable term, derive the put legs, run the five gates (recording — never throwing — so a
/// non-inverting term is excluded rather than aborting the pipeline), and tally the four
/// tiers per vocabulary and overall.
pub fn gate_derived_audit(
    sssom_texts: &[String],
    projection_ttls: &[String],
    corpus_nts: &[(String, String)],
) -> gmeow_errors::Result<AuditLedger> {
    let audit = run_audit_nt(sssom_texts, projection_ttls, corpus_nts)?;
    let (direct, inverse) = edoalpath_pairs(projection_ttls)?;
    // EDOAL maps key on the full target IRI; the audit keys terms by canonical qname. Match by
    // qname, and only when the term resolves to exactly ONE direct / ONE inverse predicate (an
    // ambiguous multi-predicate path is not a clean single-step round-trip to verify).
    let direct_q = unique_qname_map(&direct);
    let inverse_q = unique_qname_map(&inverse);
    ledger_from_audit(&audit, &direct_q, &inverse_q)
}

/// The gate-derivation core, factored out of [`gate_derived_audit`] so the tier logic is
/// unit-testable over a synthetic [`AuditReport`] and qname→predicate maps (no TTL fixtures).
pub(crate) fn ledger_from_audit(
    audit: &AuditReport,
    direct_q: &BTreeMap<String, String>,
    inverse_q: &BTreeMap<String, String>,
) -> gmeow_errors::Result<AuditLedger> {
    let (corrs, legs, cells, unsupported_by_vocab) =
        correspondences_from_audit(audit, direct_q, inverse_q);

    let program = CorrespondenceProgram::new(corrs, Vec::new(), PreservationKind::SoundUnder)
        .with_leg_programs(legs);
    // No-op for supplied-put (proved-candidate) cells; the claimed/asserted cells carry no get
    // leg, so nothing is fabricated. `evaluate_gates` (NOT `assert_gates`) records REDs.
    let (gated, _outcomes) = program.with_derived_puts().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::UpProjection {
            message: e.to_string(),
        })
    })?;
    let verdicts = gmeow_logic::correspondence_exec::program_verdicts(&gated);
    let report = evaluate_gates(&gated, &[], &verdicts);

    // Attribute each gate verdict back to its vocabulary via the corr IRI → vocab map.
    let vocab_of: BTreeMap<&str, &str> = cells
        .iter()
        .map(|c| (c.corr_iri.as_str(), c.vocab.as_str()))
        .collect();
    let mut per_vocab: BTreeMap<String, TierCounts> = BTreeMap::new();
    for (vocab, count) in &unsupported_by_vocab {
        per_vocab.entry(vocab.clone()).or_default().unsupported += *count;
    }
    for r in &report.per_correspondence {
        let vocab = vocab_of
            .get(r.correspondence.as_str())
            .copied()
            .unwrap_or("");
        let counts = per_vocab.entry(vocab.to_owned()).or_default();
        match tier_of(r) {
            Tier::Proved => counts.proved += 1,
            Tier::Claimed => counts.claimed += 1,
            Tier::RedExcluded => counts.red_excluded += 1,
        }
    }

    // The whole-audit totals: the gate ledger's lawful count is the proved tier; the rest are
    // tallied from the per-correspondence verdicts and the unsupported buckets.
    let mut totals = TierCounts::default();
    for counts in per_vocab.values() {
        totals.proved += counts.proved;
        totals.claimed += counts.claimed;
        totals.red_excluded += counts.red_excluded;
        totals.unsupported += counts.unsupported;
    }
    // The proved tier MUST equal the gate ledger's lawful count — an internal consistency check
    // that the tier classification and the ledger never diverge (no-optionality hard-fail).
    let ledger = liftability(&report);
    if totals.proved != ledger.lawful {
        return Err(gmeow_errors::Diag::of_kind(crate::error::UpProjection {
            message: format!(
                "gate-derived audit inconsistency: proved tier {} != gate ledger lawful {}",
                totals.proved, ledger.lawful
            ),
        }));
    }

    Ok(AuditLedger {
        totals,
        per_vocab,
        gaps: audit.gaps.clone(),
    })
}

// --------------------------------------------------------------------------- //
// The gate-verified lift program — the single source of truth the executor lifts.
// --------------------------------------------------------------------------- //

/// The orientation of a gate-verified lift, single-sourced from the shared
/// [`TermShape`] the gate classifies (`shape.legs` distinguishes the two).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// A forward rename: source `?s <ext> ?o` lifts to `?s <gmeow> ?o` (a `LegPath::Step`).
    Direct,
    /// An inverse rename: source `?s <ext> ?o` lifts to `?o <gmeow> ?s`
    /// (a `LegPath::Inverse(Step)` put-leg body).
    Inverse,
}

/// Whether a gate-verified lift lands as a plain FACT (a crisp `Equiv` rename) or a lossy
/// reified CLAIM cell (an `Overlaps` / vague / close-match generalization).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiftKind {
    /// A crisp rename asserted as a fact.
    Fact,
    /// A lossy lift disclosed as a `gm:StatementMetadata` claim cell, with its confidence lexeme.
    Claim {
        /// The decimal confidence lexeme (may be empty for a structural/close-match with none).
        confidence: String,
    },
}

/// One gate-verified lift rule for one external-vocabulary term: the gmeow target it lifts to,
/// the single-sourced orientation, and whether it lands as a fact or a reified claim. Only
/// terms whose gate tier is `proved` or `claimed` ever appear here — a `red_excluded`
/// (non-inverting) or `unsupported` term is dropped, exactly as the audit ledger drops it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiftRule {
    /// The gmeow target IRI (predicate or class).
    pub gmeow: String,
    /// The orientation, taken from the shared gate classification.
    pub orientation: Orientation,
    /// Fact vs reified claim, taken from the shared correspondence relation.
    pub kind: LiftKind,
}

/// The gate-verified lift program the executor consumes: the per-term lift rules (keyed by the
/// external-vocabulary term IRI) that survive the correspondence gates, plus the honest residue
/// counts (ambiguous multi-candidate targets and gate-excluded non-inverting terms) so nothing
/// is dropped silently.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiftProgram {
    /// external term IRI → the single gate-verified lift rule for it.
    pub rules: BTreeMap<String, LiftRule>,
    /// Count of terms with multiple candidate gmeow targets, dropped rather than guessed.
    pub ambiguous_dropped: usize,
    /// Count of terms the correspondence gate RED-excluded (a reverse path that does not invert
    /// its forward path); surfaced as residue, never silently lifted as a fact.
    pub gate_excluded: usize,
}

/// A candidate lift for one external term, resolved from the projection/SSSOM pairs *before*
/// the gate decides whether it is lawful. `gmeow` is the single unique target (multi-candidate
/// terms are recorded as `ambiguous` and never become a candidate).
struct CandidateLift {
    gmeow: String,
    confidence: Option<String>,
}

/// Build the **gate-verified lift program**: the single authority for which external terms the
/// executor lifts, their orientation, and their gmeow target. Every candidate term (resolved
/// from the SSSOM/EDOAL/structural pairs) is classified through the SAME
/// [`evidence_for`]/[`classify_term`] + correspondence-gate path the audit ledger uses, and only
/// the gate-surviving (`proved` + `claimed`) terms are kept. A term whose reverse EDOAL path does
/// not invert its forward path is RED-excluded here exactly as the ledger red-excludes it, so the
/// executor can never lift a non-invertible term the audit certifies as unlawful.
///
/// Corpus-independent: the tier depends only on the term's bucket and its EDOAL direct/inverse
/// paths, so the program is derived ONCE and applied to every source file (no per-file recompute).
///
/// `discharged_section_cells` is the A→B authorization channel (issue Deliverable A → B): the set
/// of `gmeow:ProjectionMapping` cell IRIs whose EXECUTED lens-law discharge (the mappings stage's
/// `stages::mappings::discharge_correspondence_laws`, folded into
/// `graph/correspondence-laws`) carried a `logic:SectionLaw` verdict of `ObligationDischarged`.
/// A mnemomorphic `=` cell so authorized is promoted to a LAWFUL rename rule — it lifts as a FACT,
/// not a lossy close-match claim — because the executed round-trip PROVED its `put ∘ get = id`. A
/// mnemomorphic `=` cell that is NOT authorized is a HARD FAIL (no optional fallback): the discharge
/// verdict is a required input, never silently missing.
pub fn gate_verified_lift_program(
    sssom_texts: &[String],
    projection_ttls: &[String],
    discharged_section_cells: &BTreeSet<String>,
) -> gmeow_errors::Result<LiftProgram> {
    // The buckets + EDOAL paths that define each term's gate tier (corpus-independent), keyed by
    // full target IRI — the SAME inputs the audit's `combined_class` / `unique_qname_map` read.
    let sssom_buckets = sssom_best_buckets_pub(sssom_texts)?;
    let structural = structural_best_classes_pub(projection_ttls)?;
    let (direct_edoal, inverse_edoal) = edoalpath_pairs(projection_ttls)?;
    let direct_one = unique_iri_map(&direct_edoal);
    let inverse_one = unique_iri_map(&inverse_edoal);

    // The candidate targets — the SAME resolution the retired ungated map used, but now only a
    // source of the gmeow target / confidence; the tier + orientation are decided by the gate.
    let (direct_candidates, inverse_candidates, claim_candidates, ambiguous_dropped) =
        candidate_lifts(sssom_texts, projection_ttls, &direct_edoal, &inverse_edoal)?;

    // Every term that has any candidate lift is put through the gate; the shared classification
    // decides the tier (drop red_excluded/unsupported) AND the orientation (direct vs inverse).
    let mut terms: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    terms.extend(direct_candidates.keys().cloned());
    terms.extend(inverse_candidates.keys().cloned());
    terms.extend(claim_candidates.keys().cloned());

    let mut rules: BTreeMap<String, LiftRule> = BTreeMap::new();
    let mut gate_excluded = 0usize;
    for term in terms {
        let bucket = combined_class(&term, &sssom_buckets, &structural);
        let evidence = evidence_for(
            &bucket,
            direct_one.get(&term).map(String::as_str),
            inverse_one.get(&term).map(String::as_str),
        );
        let Some(shape) = classify_term(&evidence) else {
            // Unsupported bucket: no lift rule at all (matches the audit's unsupported tier).
            continue;
        };
        // Run this one term's correspondence through the SAME gates the ledger uses. A term whose
        // reverse path does not invert its forward path REDs the round-trip gate → excluded.
        let tier = gate_tier_for(&term, &shape)?;
        match tier {
            Tier::RedExcluded => {
                gate_excluded += 1;
                continue;
            }
            Tier::Proved | Tier::Claimed => {}
        }
        // Resolve the surviving term's target, orientation, AND fact-vs-claim from the candidate
        // maps — the SAME `=`/`<=`/closeMatch relation substrate the audit buckets read. A crisp
        // rename (SSSOM exact / `=` structural / direct EDOAL) is a FACT; a generalizing (`<=`) or
        // closeMatch candidate is a lossy reified CLAIM. Orientation follows the EDOAL anchor
        // (subject-anchored = direct; object-anchored = inverse). The gate tier above has already
        // decided survival; this only shapes the surviving rule. (Crisp candidates take precedence
        // over claim candidates, exactly as `candidate_lifts` resolves them.)
        let rule = if let Some(c) = direct_candidates.get(&term) {
            LiftRule {
                gmeow: c.gmeow.clone(),
                orientation: Orientation::Direct,
                kind: LiftKind::Fact,
            }
        } else if let Some(c) = inverse_candidates.get(&term) {
            LiftRule {
                gmeow: c.gmeow.clone(),
                orientation: Orientation::Inverse,
                kind: LiftKind::Fact,
            }
        } else if let Some(c) = claim_candidates.get(&term) {
            LiftRule {
                gmeow: c.gmeow.clone(),
                // A claim rides the predicate position of the source triple (direct shape); the
                // lossy relation is carried by the reified cell, not by an inverted put-leg.
                orientation: Orientation::Direct,
                kind: LiftKind::Claim {
                    confidence: c.confidence.clone().unwrap_or_default(),
                },
            }
        } else {
            // No target resolved for this gate-surviving term (all candidates were ambiguous):
            // it stays residue, never lifted.
            continue;
        };
        rules.insert(term, rule);
    }

    // A→B consumption: promote every mnemomorphic `=` cell whose EXECUTED SectionLaw was
    // discharged (Deliverable A) to a LAWFUL rename FACT. This is what makes a real SIOC image
    // bearing `sioc:has_container` / `sioc:reply_of` lift to `gmeow:partOfThread` /
    // `gmeow:inReplyTo` (both `rdfs:domain gmeow:Message`) so the reasoned harvest recovers
    // `gmeow:Message`. The promotion overrides any lossy close-match CLAIM the candidate resolution
    // produced for the same term (a discharged section is strictly stronger than an asserted
    // close-match). `mapSiocTopic` is `=` but NOT mnemomorphic (no discharged SectionLaw), so it is
    // never promoted — the honest floor stays a reified claim.
    for promo in discharged_renames(projection_ttls, discharged_section_cells)? {
        rules.insert(
            promo.ext,
            LiftRule {
                gmeow: promo.gmeow,
                orientation: promo.orientation,
                kind: LiftKind::Fact,
            },
        );
    }

    Ok(LiftProgram {
        rules,
        ambiguous_dropped,
        gate_excluded,
    })
}

/// A lawful rename promoted from a mnemomorphic `=` cell whose executed SectionLaw is discharged.
struct DischargedRename {
    /// The `gmeow:ProjectionMapping` cell IRI that authorized this rename — retained so a
    /// same-`ext` collision between two DISTINCT discharged cells names both offenders.
    cell: String,
    /// The external-vocabulary term the source triple carries (a `toPredicate` / `toClass`).
    ext: String,
    /// The gmeow term it lifts to (the cell's `edoalSource`).
    gmeow: String,
    /// The rename orientation, derived from the cell's single source atom (subject-anchored =
    /// direct; object-anchored = inverse). Every shipped mnemomorphic `=` cell is subject-anchored,
    /// so this is `Direct` today, but the derivation is general so a future object-anchored cell
    /// inverts correctly rather than silently mis-lifting.
    orientation: Orientation,
}

/// Resolve the lawful renames the discharged-SectionLaw `=` cells authorize.
///
/// For every `gmeow:ProjectionMapping` binding in `projection_ttls` whose relation is `=` AND which
/// is `gmeow:mnemomorphic true`, the cell MUST carry an `ObligationDischarged` `logic:SectionLaw`
/// verdict (present in `discharged`) — otherwise this is a HARD FAIL: the executed discharge
/// (Deliverable A) is a required authorization for the lawful lift (Deliverable B), never silently
/// absent. An authorized cell resolves to a `(ext, gmeow, orientation)` rename: the external target
/// is the binding's `toPredicate` / `toClass`, the gmeow term is the cell's `edoalSource`, and the
/// orientation follows the single source atom's anchor position.
fn discharged_renames(
    projection_ttls: &[String],
    discharged: &BTreeSet<String>,
) -> gmeow_errors::Result<Vec<DischargedRename>> {
    // Keyed by external target so two discharged cells cannot silently promote conflicting renames
    // for the SAME `ext` (a later `rules.insert(ext, …)` would otherwise last-wins overwrite the
    // earlier one — a nondeterministic, soundness-losing drop).
    let mut out: BTreeMap<String, DischargedRename> = BTreeMap::new();
    for ttl in projection_ttls {
        let graph = Graph::parse(ttl.as_bytes(), "text/turtle").map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::UpProjection {
                message: e.to_string(),
            })
        })?;
        let q = &graph.quads;
        for cell in subjects(q, RDF_TYPE, GM_PROJECTION_MAPPING) {
            let RdfTerm::Iri(cell_iri) = &cell else {
                continue;
            };
            let Some(pattern) = value(q, &cell, GM_HAS_MAPPING_PATTERN) else {
                continue;
            };
            let Some(gmeow_src) = value_named(q, &pattern, GM_EDOAL_SOURCE) else {
                continue;
            };
            for binding in objects(q, &cell, GM_HAS_BINDING) {
                let rel = value_lexical(q, &binding, GM_RELATION).unwrap_or_default();
                let mnemomorphic =
                    value_lexical(q, &binding, GM_MNEMOMORPHIC).as_deref() == Some("true");
                if rel != "=" || !mnemomorphic {
                    continue;
                }
                // A→B authorization: a mnemomorphic `=` cell MUST have discharged its section law.
                if !discharged.contains(cell_iri.as_str()) {
                    return Err(gmeow_errors::Diag::of_kind(crate::error::UpProjection {
                        message: format!(
                            "up-projection lift program: mnemomorphic `=` cell <{cell_iri}> has no \
                             discharged logic:SectionLaw verdict — the executed correspondence-law \
                             discharge (Deliverable A) did not authorize its lawful lift; refusing \
                             to build the lift program (no optional fallback)"
                        ),
                    }));
                }
                let Some((ext, orientation)) = resolve_rename(q, &pattern, &binding, &gmeow_src)?
                else {
                    continue;
                };
                if in_projection_ns(&ext) {
                    let promo = DischargedRename {
                        cell: cell_iri.to_string(),
                        ext: ext.clone(),
                        gmeow: gmeow_src.clone(),
                        orientation,
                    };
                    // Collision guard (no optional fallback, no last-wins overwrite): a second
                    // discharged rename for an `ext` already claimed is only tolerated when it is a
                    // byte-identical no-op (same gmeow target AND orientation — the resulting rule is
                    // unchanged). Any collision that WOULD change the resulting rule — a different
                    // gmeow target or orientation, i.e. two discharged cells disagreeing on where the
                    // same external term lifts — is a genuine ambiguity and a HARD FAIL naming both
                    // cells, never a silent pick-last.
                    if let Some(existing) = out.get(&ext) {
                        if existing.gmeow != promo.gmeow
                            || existing.orientation != promo.orientation
                        {
                            return Err(gmeow_errors::Diag::of_kind(crate::error::UpProjection {
                                message: format!(
                                    "up-projection lift program: external term <{ext}> is claimed \
                                     by TWO discharged mnemomorphic `=` cells with conflicting \
                                     lawful renames — <{first_cell}> lifts it to <{first_gmeow}> \
                                     ({first_orient:?}) but <{second_cell}> lifts it to \
                                     <{second_gmeow}> ({second_orient:?}); the promoted rename is \
                                     ambiguous. Refusing to silently overwrite one lawful rename \
                                     with the other (no optional fallback, no last-wins).",
                                    first_cell = existing.cell,
                                    first_gmeow = existing.gmeow,
                                    first_orient = existing.orientation,
                                    second_cell = promo.cell,
                                    second_gmeow = promo.gmeow,
                                    second_orient = promo.orientation,
                                ),
                            }));
                        }
                        // Identical resulting rule: an idempotent no-op, keep the first.
                        continue;
                    }
                    out.insert(ext, promo);
                }
            }
        }
    }
    Ok(out.into_values().collect())
}

/// Resolve one discharged cell's `(external target, orientation)`. A `toClass` binding is a type
/// retype — always subject-preserving (`Direct`). A `toPredicate` binding's orientation follows the
/// single source atom whose predicate is the cell's `edoalSource`: subject-anchored ⇒ `Direct`,
/// object-anchored ⇒ `Inverse`. A discharged cell whose orientation cannot be resolved is a HARD
/// FAIL (an authorized rename we cannot orient is a real inconsistency, never a silent drop).
fn resolve_rename(
    q: &[purrdf::RdfQuad],
    pattern: &RdfTerm,
    binding: &RdfTerm,
    gmeow_src: &str,
) -> gmeow_errors::Result<Option<(String, Orientation)>> {
    if let Some(cls) = value_named(q, binding, GM_TO_CLASS) {
        return Ok(Some((cls, Orientation::Direct)));
    }
    let Some(pred) = value_named(q, binding, GM_TO_PREDICATE) else {
        return Ok(None);
    };
    let Some(anchor) = value(q, pattern, GM_ANCHOR) else {
        return Err(gmeow_errors::Diag::of_kind(crate::error::UpProjection {
            message: format!(
                "discharged `=` cell binding for target <{pred}> has no mapping-pattern anchor"
            ),
        }));
    };
    for atom in rdf_list(q, value(q, pattern, GM_ATOM).as_ref()) {
        if value_named(q, &atom, GM_PREDICATE).as_deref() != Some(gmeow_src) {
            continue;
        }
        if value(q, &atom, GM_SUBJECT_VAR).as_ref() == Some(&anchor) {
            return Ok(Some((pred, Orientation::Direct)));
        }
        if value(q, &atom, GM_OBJECT_VAR).as_ref() == Some(&anchor) {
            return Ok(Some((pred, Orientation::Inverse)));
        }
    }
    Err(gmeow_errors::Diag::of_kind(crate::error::UpProjection {
        message: format!(
            "discharged `=` cell binding for target <{pred}>: no source atom on <{gmeow_src}> \
             anchors the rename — cannot orient the lawful lift"
        ),
    }))
}

/// The set of `gmeow:ProjectionMapping` cell IRIs whose EXECUTED lens-law discharge carried a
/// `logic:SectionLaw` verdict of `ObligationDischarged`, extracted from the `graph/correspondence-laws`
/// projection (the mappings stage's `discharge_correspondence_laws`
/// output) presented as `(subject, predicate, object)` value-string triples. A correspondence's
/// `logic:getLeg` IS its cell IRI; a discharged section-law claim is a `logic:lawClaimed =
/// logic:SectionLaw` node whose `logic:lawDischargeVerdict` is `logic:ObligationDischarged`. This is
/// the single extractor both the production bundle consumer and the acceptance harness route
/// through, so the A→B channel has one shape.
pub fn discharged_section_cells_from_triples(
    triples: &[(String, String, String)],
) -> BTreeSet<String> {
    let get_leg = format!("{LOGIC_NS}getLeg");
    let has_law_claim = format!("{LOGIC_NS}hasLawClaim");
    let law_claimed = format!("{LOGIC_NS}lawClaimed");
    let discharge_verdict = format!("{LOGIC_NS}lawDischargeVerdict");
    let section_law = format!("{LOGIC_NS}SectionLaw");
    let obligation_discharged = format!("{LOGIC_NS}ObligationDischarged");

    let objects_of = |subject: &str, predicate: &str| -> Vec<&str> {
        triples
            .iter()
            .filter(|(s, p, _)| s == subject && p == predicate)
            .map(|(_, _, o)| o.as_str())
            .collect()
    };
    let is_discharged_section = |claim: &str| -> bool {
        objects_of(claim, &law_claimed).contains(&section_law.as_str())
            && objects_of(claim, &discharge_verdict).contains(&obligation_discharged.as_str())
    };

    let mut cells = BTreeSet::new();
    for (corr, p, claim) in triples {
        if p != &has_law_claim || !is_discharged_section(claim) {
            continue;
        }
        for cell in objects_of(corr, &get_leg) {
            cells.insert(cell.to_owned());
        }
    }
    cells
}

/// The [`discharged_section_cells_from_triples`] extractor over a `graph/correspondence-laws`
/// N-Triples projection (the acceptance-harness / root-recompute path). Every relevant term is an
/// IRI, so the term value strings are compared directly.
pub fn discharged_section_cells_from_corpus(
    corr_laws_nt: &str,
) -> gmeow_errors::Result<BTreeSet<String>> {
    let graph = Graph::parse(corr_laws_nt.as_bytes(), "application/n-triples").map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::UpProjection {
            message: e.to_string(),
        })
    })?;
    let triples: Vec<(String, String, String)> = graph
        .quads
        .iter()
        .map(|q| {
            (
                term_value(&q.subject),
                q.predicate.clone(),
                term_value(&q.object),
            )
        })
        .collect();
    Ok(discharged_section_cells_from_triples(&triples))
}

/// The bare value of an RDF term for graph-shape comparison: the IRI, blank-node label, or literal
/// lexical form (no delimiters). The discharged-cell extraction only matches IRI-valued positions,
/// so a literal/blank simply fails to match — never mis-attributed.
fn term_value(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => iri.clone(),
        RdfTerm::BlankNode(id) => id.clone(),
        RdfTerm::Literal(lit) => lit.lexical_form.clone(),
        RdfTerm::Triple(_) => String::new(),
    }
}

/// The gate tier for a single term's correspondence shape, computed through the EXACT gate
/// machinery the audit ledger uses ([`classify_term`] → [`CorrespondenceProgram::with_derived_puts`]
/// → [`evaluate_gates`] → [`tier_of`]). This is the single classification; the producer never
/// re-implements a second copy of the direct/inverse orientation-and-inversion logic.
fn gate_tier_for(term: &str, shape: &TermShape) -> gmeow_errors::Result<Tier> {
    let corr_iri = format!("{LOGIC_NS}up-projection-lift/{}", slug(term));
    let (get_leg, put_leg, legs) = match &shape.legs {
        Some((direct, inverse)) => {
            let get_iri = format!("{corr_iri}/get");
            let put_iri = format!("{corr_iri}/put");
            let legs = vec![
                TransactionProgramIr {
                    iri: get_iri.clone(),
                    body: LegPath::Step(direct.clone()),
                },
                TransactionProgramIr {
                    iri: put_iri.clone(),
                    body: LegPath::Inverse(Box::new(LegPath::Step(inverse.clone()))),
                },
            ];
            (Some(get_iri), Some(put_iri), legs)
        }
        None => (None, None, Vec::new()),
    };
    let determinacy = match shape.relation {
        CorrespondenceRelation::Overlaps => Some(Determinacy::Vague),
        _ => Some(Determinacy::Crisp),
    };
    let correspondence = Correspondence::new(
        corr_iri,
        shape.relation,
        shape.class,
        shape.kind,
        shape.mnemomorphic,
        determinacy,
        get_leg,
        put_leg,
        shape.laws.clone(),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::UpProjection {
            message: format!("gate-verified lift correspondence for {term} is malformed: {e}"),
        })
    })?;
    let program = CorrespondenceProgram::new(
        vec![correspondence],
        Vec::new(),
        PreservationKind::SoundUnder,
    )
    .with_leg_programs(legs);
    let (gated, _outcomes) = program.with_derived_puts().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::UpProjection {
            message: e.to_string(),
        })
    })?;
    let verdicts = gmeow_logic::correspondence_exec::program_verdicts(&gated);
    let report = evaluate_gates(&gated, &[], &verdicts);
    let r = report.per_correspondence.first().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::UpProjection {
            message: format!("gate report empty for {term}"),
        })
    })?;
    Ok(tier_of(r))
}

/// Resolve the candidate gmeow target for each external term from the SSSOM/EDOAL/structural
/// pairs — a unique-target-wins resolution (the retired executor's `build_lawful_rules` logic,
/// now confined to *target resolution* only; the tier and orientation are the gate's job).
/// Returns `(direct, inverse, claim)` candidate maps plus the count of multi-candidate terms
/// dropped as ambiguous residue.
#[allow(clippy::type_complexity)]
fn candidate_lifts(
    sssom_texts: &[String],
    projection_ttls: &[String],
    direct_edoal: &BTreeMap<String, std::collections::BTreeSet<String>>,
    inverse_edoal: &BTreeMap<String, std::collections::BTreeSet<String>>,
) -> gmeow_errors::Result<(
    BTreeMap<String, CandidateLift>,
    BTreeMap<String, CandidateLift>,
    BTreeMap<String, CandidateLift>,
    usize,
)> {
    let identity = sssom_clean_pairs(sssom_texts)?;
    let (exact_struct, generalizing_struct) = structural_pairs(projection_ttls)?;
    let closematch = sssom_closematch_pairs(sssom_texts)?;

    let mut ambiguous_dropped = 0usize;

    // Direct rename target: SSSOM exact ∪ structural exact ∪ direct EDOAL path (single unique).
    let mut direct_union: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for layer in [&identity, &exact_struct, direct_edoal] {
        for (target, gmeows) in layer {
            direct_union
                .entry(target.clone())
                .or_default()
                .extend(gmeows.iter().cloned());
        }
    }
    // A target dropped as ambiguous at any layer must stay honest residue: a lower-priority
    // layer may never recover it. `direct_ambiguous` blocks both the inverse and the claim
    // layers; `inverse_ambiguous` blocks the claim layer. Each ambiguous target is counted in
    // `ambiguous_dropped` exactly once (at the highest-priority layer that saw it), because the
    // block short-circuits before the later layer can re-count it.
    let mut direct: BTreeMap<String, CandidateLift> = BTreeMap::new();
    let mut direct_ambiguous: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for (target, gmeows) in direct_union {
        match unique_of(&gmeows) {
            Some(gmeow) => {
                direct.insert(
                    target,
                    CandidateLift {
                        gmeow,
                        confidence: None,
                    },
                );
            }
            None => {
                ambiguous_dropped += 1;
                direct_ambiguous.insert(target);
            }
        }
    }

    // Inverse rename target: inverse EDOAL path only, and only when no direct rename (lawful or
    // ambiguous) covers the term.
    let mut inverse: BTreeMap<String, CandidateLift> = BTreeMap::new();
    let mut inverse_ambiguous: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for (target, gmeows) in inverse_edoal {
        if direct.contains_key(target) || direct_ambiguous.contains(target) {
            continue;
        }
        match unique_of(gmeows) {
            Some(gmeow) => {
                inverse.insert(
                    target.clone(),
                    CandidateLift {
                        gmeow,
                        confidence: None,
                    },
                );
            }
            None => {
                ambiguous_dropped += 1;
                inverse_ambiguous.insert(target.clone());
            }
        }
    }

    // Claim target: generalizing structural + SSSOM closeMatch, only when neither a direct nor an
    // inverse rename (lawful or ambiguous) covers the term.
    //
    // Each of the two claim layers first resolves to its OWN unique winner (a layer that is
    // internally ambiguous — more than one distinct gmeow target — contributes no winner, exactly
    // as the retired resolver treated a multi-candidate cell). The per-target claim is then the
    // union of those per-layer winners, decided ONCE:
    //   * a single agreed winner (one layer, or both layers naming the same gmeow) → a claim lift;
    //   * two clean layers whose unique winners DISAGREE → a cross-layer conflict → ambiguous
    //     (the guard the retired shared `ambiguous` set enforced — a first-layer-wins accept here
    //     would silently pick one honest disagreement over the other);
    //   * no winner at all, yet the target had candidates in some layer → ambiguous residue.
    // `ambiguous_dropped` counts each such target ONCE (never the retired code's per-layer
    // double-count of a target internally ambiguous in both layers).
    let mut claim_targets: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    claim_targets.extend(generalizing_struct.keys().cloned());
    claim_targets.extend(closematch.keys().cloned());

    let mut claim: BTreeMap<String, CandidateLift> = BTreeMap::new();
    for target in claim_targets {
        if direct.contains_key(&target)
            || direct_ambiguous.contains(&target)
            || inverse.contains_key(&target)
            || inverse_ambiguous.contains(&target)
        {
            continue;
        }
        // The per-layer unique winners: (gmeow, confidence-lexeme). A layer with != 1 distinct
        // gmeow target has no unique winner and contributes nothing to the union.
        let gen_win = generalizing_struct
            .get(&target)
            .filter(|cands| cands.len() == 1)
            .and_then(|cands| cands.iter().next());
        let cm_win = closematch
            .get(&target)
            .filter(|cands| cands.len() == 1)
            .and_then(|cands| cands.iter().next());
        match (gen_win, cm_win) {
            // Both layers name a unique winner: agree → lift; disagree → cross-layer conflict.
            (Some((g_gmeow, g_conf)), Some((c_gmeow, c_conf))) => {
                if g_gmeow == c_gmeow {
                    // Same gmeow: keep the higher-confidence lexeme (the closeMatch producer's
                    // own dedup rule), so the reified claim carries the strongest evidence.
                    let (gmeow, conf) = if decimal_confidence(c_conf) > decimal_confidence(g_conf) {
                        (c_gmeow, c_conf)
                    } else {
                        (g_gmeow, g_conf)
                    };
                    claim.insert(
                        target,
                        CandidateLift {
                            gmeow: gmeow.clone(),
                            confidence: Some(conf.clone()),
                        },
                    );
                } else {
                    ambiguous_dropped += 1;
                }
            }
            // Exactly one layer names a unique winner → the generalizing/closeMatch pick stands.
            (Some((gmeow, conf)), None) | (None, Some((gmeow, conf))) => {
                claim.insert(
                    target,
                    CandidateLift {
                        gmeow: gmeow.clone(),
                        confidence: Some(conf.clone()),
                    },
                );
            }
            // No unique winner in either layer, yet the target appeared with candidates → the
            // internally-ambiguous residue, counted once.
            (None, None) => ambiguous_dropped += 1,
        }
    }

    Ok((direct, inverse, claim, ambiguous_dropped))
}

/// The single unique member of a set, or `None` when it is empty or ambiguous.
fn unique_of(set: &std::collections::BTreeSet<String>) -> Option<String> {
    (set.len() == 1).then(|| set.iter().next().expect("one member").clone())
}

/// Re-key a full-IRI → predicate-set map keeping only targets that resolve to exactly one
/// predicate — the full-IRI twin of [`unique_qname_map`], used by the lift producer (which keys
/// on target IRIs, not qnames).
fn unique_iri_map(
    m: &BTreeMap<String, std::collections::BTreeSet<String>>,
) -> BTreeMap<String, String> {
    m.iter()
        .filter(|(_, preds)| preds.len() == 1)
        .map(|(target, preds)| {
            (
                target.clone(),
                preds.iter().next().expect("one predicate").clone(),
            )
        })
        .collect()
}

/// Re-key a full-IRI → predicate-set map by canonical qname, keeping only targets that resolve
/// to exactly one predicate (an ambiguous multi-predicate path is not a single clean step).
fn unique_qname_map(
    m: &BTreeMap<String, std::collections::BTreeSet<String>>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (target, preds) in m {
        if preds.len() == 1 {
            out.insert(
                canon_qname(target),
                preds.iter().next().expect("one predicate").clone(),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests;
