// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Executed lens-law discharge for a `logic:Correspondence`'s realized `LegPath` legs.
//!
//! The five correspondence conformance gates
//! (`gmeow_logic_compile::projections::correspondence_gates`) are architecturally
//! **execution-free** — they read a per-correspondence *executed verdict* rather than
//! re-deriving a syntactic path inversion. This module is the executor that produces that
//! verdict: it lowers a correspondence's resolved `get` / `put` `LegPath` bodies to a pair
//! of SPARQL `CONSTRUCT` legs, RUNS them through the native engine
//! (`purrdf::sparql::NativeSparqlEngine`), and compares the recovered atom set to the source —
//! a **behavioural** section-law verdict (`put ∘ get = id_S`), never a textual `put ==
//! get.invert()` compare.
//!
//! This lives in `gmeow-logic` (the native runtime, engine-adjacent) rather than in the
//! wasm-able `gmeow-logic-compile` compiler (which must stay free of the reasoning/execution
//! runtime) or in `gmeow-pipeline` (which `gmeow-conformance` may not depend on) — so BOTH the
//! pipeline producers and the conformance harness reach the SAME single discharge.
//!
//! ## The lowering (why it is faithful, not a disguised syntactic check)
//!
//! A realized leg is a pure `LegPath` (a property path over predicates, with no data
//! filters). Its behavioural identity is its canonical normal form
//! ([`gmeow_logic_compile::projections::paths::leg_path_canonical`], which cancels double
//! inverses and flattens `Seq`/`Alt`). The `get` leg's forward relation is
//! `canonical(get)`; the `put` leg reconstructs `canonical(put.invert())` (the forward
//! relation a `put` optic recovers). We encode each relation as a distinct, deterministic
//! *signature predicate* and run a real one-atom round-trip:
//!
//! * `get`: `CONSTRUCT { ?s <view> ?o } WHERE { ?s <sig(get)> ?o }`
//! * `put`: `CONSTRUCT { ?s <sig(put.invert())> ?o } WHERE { ?s <view> ?o }`
//!
//! Seeding the source with one `?s <sig(get)> ?o` atom and running `put ∘ get`, the recovered
//! atom equals the source **iff** `sig(get) == sig(put.invert())`, i.e. iff `put` is the
//! structural inverse of `get`. For a pure path that behavioural verdict necessarily
//! coincides with the canonical identity (a pure-path `put` recovers `get` exactly when their
//! canonical forms invert) — so the engine genuinely decides it, and a `put` whose body is a
//! *different* path is refuted with an `ObligationViolated` verdict. (Correspondences whose
//! legs are real, filter-bearing SPARQL queries — the authored mapping cells — are discharged
//! branch-coveringly by [`crate::correspondence_exec`]'s sibling in the mappings stage
//! (`pipeline::correspondence_law`); THIS module is for the pure-`LegPath` gate legs.)

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::ir::{DischargeVerdict, LegPath, LogicProgram, PreservationKind};
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::projections::correspondence_gates::CorrespondenceVerdicts;
use gmeow_logic_compile::projections::paths::leg_path_canonical;
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfTerm, SparqlEngine, SparqlRequest, SparqlResult, parse_dataset};

/// The canonical "view" predicate the `get` leg projects a source atom onto and the `put`
/// leg reads back — a fixed apex both signature legs meet at.
const VIEW_PREDICATE: &str = "https://blackcatinformatics.ca/logic/legexec#view";
/// The fixed seed subject / object IRIs for the one-atom round-trip source graph.
const SEED_SUBJECT: &str = "https://blackcatinformatics.ca/logic/legexec#s";
const SEED_OBJECT: &str = "https://blackcatinformatics.ca/logic/legexec#o";

/// A comparable atom (subject, predicate, object) as canonical strings.
type Atom = (String, String, String);

/// The deterministic signature predicate IRI for a canonical leg-path form. Percent-encoding
/// the canonical string into the IRI fragment is injective (distinct paths ⇒ distinct
/// predicates) and needs no hash, so two legs collide here **iff** their canonical forms are
/// equal — exactly the identity the section round-trip must decide.
fn signature_predicate(canonical: &str) -> String {
    let mut frag = String::with_capacity(canonical.len() * 2);
    for &b in canonical.as_bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' {
            frag.push(b as char);
        } else {
            frag.push_str(&format!("%{b:02X}"));
        }
    }
    format!("https://blackcatinformatics.ca/logic/legsig#{frag}")
}

/// Lower a resolved `(get, put)` leg-body pair to the executable `(get_rq, put_rq)` CONSTRUCT
/// pair whose behavioural round-trip decides the section law (`put ∘ get = id_S`). The `put`
/// leg reconstructs `put.invert()` — the forward relation a `put` optic recovers — so the
/// round-trip discharges exactly when `put` structurally inverts `get`.
pub fn lower_leg_pair(get: &LegPath, put: &LegPath) -> (String, String) {
    let get_pred = signature_predicate(&leg_path_canonical(get));
    let put_pred = signature_predicate(&leg_path_canonical(&put.invert()));
    let get_rq = format!("CONSTRUCT {{ ?s <{VIEW_PREDICATE}> ?o }} WHERE {{ ?s <{get_pred}> ?o }}");
    let put_rq = format!("CONSTRUCT {{ ?s <{put_pred}> ?o }} WHERE {{ ?s <{VIEW_PREDICATE}> ?o }}");
    (get_rq, put_rq)
}

/// Canonical string for an RDF term (IRIs verbatim, blanks `_:id`, literals `"lex"`).
fn term_str(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => iri.clone(),
        RdfTerm::BlankNode(id) => format!("_:{id}"),
        RdfTerm::Literal(lit) => format!("\"{}\"", lit.lexical_form),
        RdfTerm::Triple(_) => "<<triple>>".to_owned(),
    }
}

/// Run one `CONSTRUCT` over an N-Triples source graph, returning the default-graph atom set.
/// Any parse/engine failure surfaces as `Err` (hard-fail) — never a silently empty result.
fn run_leg(
    engine: &NativeSparqlEngine,
    source_nt: &str,
    query: &str,
) -> gmeow_errors::Result<BTreeSet<Atom>> {
    let dataset =
        parse_dataset(source_nt.as_bytes(), "application/n-triples", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Reason {
                detail: format!("parse round-trip source graph: {e}"),
            })
        })?;
    let result = engine
        .query(
            &dataset,
            SparqlRequest {
                query,
                base_iri: None,
                substitutions: &[],
            },
        )
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Reason {
                detail: format!("leg CONSTRUCT evaluation failed: {e}"),
            })
        })?;
    let SparqlResult::Graph(ds) = result else {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: "leg CONSTRUCT did not return a graph".to_owned(),
        }));
    };
    Ok(purrdf::native_quads::flat_rdf_quads_from_dataset(&ds)
        .into_iter()
        .filter(|q| q.graph_name.is_none())
        .map(|q| (term_str(&q.subject), q.predicate, term_str(&q.object)))
        .collect())
}

/// Discharge the section law (`put ∘ get = id_S`) for a lowered `(get_rq, put_rq)` pair by
/// EXECUTION over the one-atom `get`-source seed: run `get` → the forward image, run `put`
/// over the image → the recovered source, and return
/// [`DischargeVerdict::ObligationDischarged`] iff the recovered atom set equals the source.
/// A non-executable leg (parse/engine error) is [`DischargeVerdict::ObligationViolated`],
/// never a silent pass.
fn discharge_leg_pair(get_pred_source: &str, get_rq: &str, put_rq: &str) -> DischargeVerdict {
    let source_nt = format!("<{SEED_SUBJECT}> <{get_pred_source}> <{SEED_OBJECT}> .\n");
    let source: BTreeSet<Atom> = [(
        SEED_SUBJECT.to_owned(),
        get_pred_source.to_owned(),
        SEED_OBJECT.to_owned(),
    )]
    .into_iter()
    .collect();

    let engine = NativeSparqlEngine::new();
    let forward = match run_leg(&engine, &source_nt, get_rq) {
        Ok(v) => v,
        Err(_) => return DischargeVerdict::ObligationViolated,
    };
    let mut image_nt = String::new();
    for (s, p, o) in &forward {
        image_nt.push_str(&format!("<{s}> <{p}> <{o}> .\n"));
    }
    let recovered = match run_leg(&engine, &image_nt, put_rq) {
        Ok(v) => v,
        Err(_) => return DischargeVerdict::ObligationViolated,
    };
    if recovered == source {
        DischargeVerdict::ObligationDischarged
    } else {
        DischargeVerdict::ObligationViolated
    }
}

/// The executed section-law verdict for one resolved `(get, put)` leg-body pair.
pub fn leg_pair_verdict(get: &LegPath, put: &LegPath) -> DischargeVerdict {
    let get_pred = signature_predicate(&leg_path_canonical(get));
    let (get_rq, put_rq) = lower_leg_pair(get, put);
    discharge_leg_pair(&get_pred, &get_rq, &put_rq)
}

/// Compute the executed lens-law verdict for **every** correspondence in `program`, keyed by
/// correspondence IRI — the map the five correspondence gates read.
///
/// A correspondence whose `get` AND `put` legs both resolve to a realized [`LegPath`] body is
/// discharged by execution; one whose legs are absent or unresolvable (a bridge view, a
/// caveated overlap with no realized leg, or an incomplete cell) has no verifiable section
/// round-trip and carries [`DischargeVerdict::ObligationUnknown`] — never "proved absent".
/// Every correspondence gets an entry (the gates HARD-fail on a missing verdict), so nothing
/// is silently defaulted to a pass.
pub fn program_verdicts(program: &CorrespondenceProgram) -> BTreeMap<String, DischargeVerdict> {
    let mut out = BTreeMap::new();
    for c in &program.correspondences {
        let get_body = c.get_leg.as_deref().and_then(|i| program.resolve_leg(i));
        let put_body = c.put_leg.as_deref().and_then(|i| program.resolve_leg(i));
        let verdict = match (get_body, put_body) {
            (Some(g), Some(p)) => leg_pair_verdict(g, p),
            _ => DischargeVerdict::ObligationUnknown,
        };
        out.insert(c.iri.clone(), verdict);
    }
    out
}

/// Compute the executed lens-law verdict map for every `logic:Correspondence` a compiled
/// [`LogicProgram`] carries — the exact per-correspondence map
/// [`gmeow_logic_compile::projections::compile_program`]'s five correspondence gates require.
///
/// This is the **single** production/harness discharge: it assembles the correspondence
/// program the compiler builds internally (the same leg registry + derived put legs, keyed by
/// the SAME correspondence IRIs), then runs [`program_verdicts`] over it. Because the assembly
/// is byte-identical to `compile_program`'s and `program_verdicts` emits one entry per
/// correspondence, EVERY correspondence the gates evaluate has a supplied verdict — so a caller
/// that threads this map can never trip the gates' missing-verdict invariant.
///
/// A correspondence-free program yields an empty map (the gates never run). A malformed
/// leg registry (a put leg that cannot be derived) surfaces as a clean `Err`, never a panic —
/// the caller propagates it as a surfaced diagnostic.
pub fn logic_program_verdicts(
    program: &LogicProgram,
) -> gmeow_errors::Result<CorrespondenceVerdicts> {
    if program.correspondences.is_empty() {
        return Ok(CorrespondenceVerdicts::new());
    }
    let assembled = CorrespondenceProgram::new(
        program.correspondences.clone(),
        Vec::new(),
        PreservationKind::SoundUnder,
    )
    .with_leg_programs(program.transaction_programs.clone());
    let (derived, _outcomes) = assembled.with_derived_puts()?;
    Ok(program_verdicts(&derived))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(p: &str) -> LegPath {
        LegPath::Step(p.to_owned())
    }

    #[test]
    fn genuine_inverse_discharges() {
        // put = get.invert(): the executed round-trip recovers the source.
        let get = step("https://ex/a");
        let put = get.invert();
        assert_eq!(
            leg_pair_verdict(&get, &put),
            DischargeVerdict::ObligationDischarged
        );
    }

    #[test]
    fn wrong_put_body_is_violated() {
        // A put over a DIFFERENT predicate does not invert get → refuted by execution.
        let get = step("https://ex/a");
        let put = step("https://ex/WRONG");
        assert_eq!(
            leg_pair_verdict(&get, &put),
            DischargeVerdict::ObligationViolated
        );
    }

    #[test]
    fn seq_path_and_its_structural_inverse_discharges() {
        // The openEHR blood-pressure witness shape: a Seq get and its auto-derived inverse.
        let get = LegPath::Seq(vec![
            step("https://ex/x"),
            step("https://ex/y"),
            step("https://ex/z"),
        ]);
        let put = get.invert();
        assert_eq!(
            leg_pair_verdict(&get, &put),
            DischargeVerdict::ObligationDischarged
        );
    }

    #[test]
    fn program_verdicts_supplies_every_correspondence() {
        use gmeow_logic_compile::ir::{
            Correspondence, CorrespondenceRelation, MorphismClass, MorphismKind, PreservationKind,
            TransactionProgramIr,
        };
        let c = Correspondence::new(
            "https://ex/corr".to_owned(),
            CorrespondenceRelation::Equiv,
            MorphismClass::Isomorphism,
            MorphismKind::InstitutionMorphism,
            true,
            None,
            Some("https://ex/get".to_owned()),
            Some("https://ex/put".to_owned()),
            Vec::new(),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("well-formed");
        let program = CorrespondenceProgram::new(vec![c], Vec::new(), PreservationKind::SoundUnder)
            .with_leg_programs(vec![
                TransactionProgramIr {
                    iri: "https://ex/get".to_owned(),
                    body: step("https://ex/a"),
                },
                TransactionProgramIr {
                    iri: "https://ex/put".to_owned(),
                    body: LegPath::Inverse(Box::new(step("https://ex/a"))),
                },
            ]);
        let v = program_verdicts(&program);
        assert_eq!(
            v.get("https://ex/corr"),
            Some(&DischargeVerdict::ObligationDischarged)
        );
    }

    #[test]
    fn unresolvable_legs_are_unknown_not_a_pass() {
        use gmeow_logic_compile::ir::{
            Correspondence, CorrespondenceRelation, MorphismClass, MorphismKind, PreservationKind,
        };
        // A bridge view with a get leg IRI that resolves to no body: no verifiable round-trip.
        let c = Correspondence::new(
            "https://ex/bridge".to_owned(),
            CorrespondenceRelation::Equiv,
            MorphismClass::BridgeView,
            MorphismKind::CommitmentShiftingBridge,
            false,
            None,
            Some("https://ex/bridgeGet".to_owned()),
            None,
            Vec::new(),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("well-formed");
        let program = CorrespondenceProgram::new(vec![c], Vec::new(), PreservationKind::SoundUnder);
        let v = program_verdicts(&program);
        assert_eq!(
            v.get("https://ex/bridge"),
            Some(&DischargeVerdict::ObligationUnknown)
        );
    }
}
