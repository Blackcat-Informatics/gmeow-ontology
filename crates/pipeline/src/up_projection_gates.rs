// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Re-derive the up-projection invertibility audit through the **correspondence gates**.
//!
//! The legacy audit ([`crate::up_projection::run_audit_nt`]) assigned each external target
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

use std::collections::BTreeMap;

use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeVerdict,
    LawClaimIr, LegPath, MorphismClass, MorphismKind, PreservationKind, TransactionProgramIr,
};
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::projections::correspondence_gates::{
    evaluate_gates, liftability, GateReport,
};

use crate::up_projection::{canon_qname, edoalpath_pairs, prefix, run_audit_nt, AuditReport};

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
    if liftable {
        if let (Some(d), Some(i)) = (direct, inverse) {
            return LiftEvidence::VerifiableRoundTrip {
                direct: d.to_owned(),
                inverse: i.to_owned(),
            };
        }
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
) -> Result<AuditLedger, String> {
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
) -> Result<AuditLedger, String> {
    let (corrs, legs, cells, unsupported_by_vocab) =
        correspondences_from_audit(audit, direct_q, inverse_q);

    let program = CorrespondenceProgram::new(corrs, Vec::new(), PreservationKind::SoundUnder)
        .with_leg_programs(legs);
    // No-op for supplied-put (proved-candidate) cells; the claimed/asserted cells carry no get
    // leg, so nothing is fabricated. `evaluate_gates` (NOT `assert_gates`) records REDs.
    let (gated, _outcomes) = program.with_derived_puts()?;
    let report = evaluate_gates(&gated, &[]);

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
        return Err(format!(
            "gate-derived audit inconsistency: proved tier {} != gate ledger lawful {}",
            totals.proved, ledger.lawful
        ));
    }

    Ok(AuditLedger {
        totals,
        per_vocab,
        gaps: audit.gaps.clone(),
    })
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
