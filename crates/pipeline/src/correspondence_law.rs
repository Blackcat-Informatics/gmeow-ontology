// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Executed lens-law discharge for a `logic:Correspondence`.
//!
//! A correspondence is an asymmetric lens: a forward `get` leg (down-projection to an
//! external vocabulary) and an inverse `put` leg (the ingest up-lift). The prior
//! round-trip gate compared the two legs' `LegPath` *bodies* syntactically — a purely
//! textual inversion audit that a re-authored cell carrying an unrecoverable guard atom
//! could slip past (the `mapSiocTopic` failure mode). This module discharges the laws by
//! EXECUTION instead: it RUNS both SPARQL `CONSTRUCT` legs through the single native
//! authority in [`gmeow_logic::correspondence_exec`] and compares the resulting atom sets.
//! A verdict is behavioural, not textual.
//!
//! Two laws are discharged whenever both legs already run:
//!
//! * [`CorrespondenceLaw::SectionLaw`] — `put ∘ get = id_S`. For each source seed `s`:
//!   run `get` over `s` → the forward image `v`; run `put` over `v` → the recovered source
//!   `s'`; the law holds on that seed iff `s' == s` (no spurious atom fabricated, no source
//!   atom dropped). Discharged iff every seed round-trips exactly; otherwise Violated with a
//!   [`Countermodel`] naming the failing seed and its spurious/missing atoms.
//! * [`CorrespondenceLaw::PutGet`] — `get ∘ put = id_V` on the forward image. For each seed:
//!   `get(put(v)) == v`. Both legs already run for the section check, so this is computed for
//!   free from the same executions.
//!
//! ## Why the seed corpus is branch-covering (the load-bearing move)
//!
//! A single happy-path seed is a *test*, not a *proof*: a `put` atom that fabricates only on
//! inputs the seed never exercises would round-trip cleanly and pass. So [`derive_seeds`]
//! synthesises one seed per `UNION` branch of the `get` leg's `WHERE` clause — instantiating
//! every positive triple pattern of that branch with fresh, deterministic per-variable IRIs
//! (`http://seed.example/vN`) — PLUS one combined seed unioning all branches. Every guard
//! atom and every variable position of `get` is therefore exercised at least once. A `put`
//! branch that fabricates an atom keyed to a specific `get` branch's data is forced to fire
//! under that branch's dedicated seed, where its fabricated atom is not among the seed's
//! source atoms — so the round-trip inequality surfaces it. The combined seed additionally
//! exercises cross-branch interference. Nothing here reads the clock or a random source; the
//! seed IRIs vary only by a deterministic index, so a verdict (and its countermodel bytes)
//! are reproducible.

pub use gmeow_logic::correspondence_exec::{
    Atom, Countermodel, DischargeOutcome, SeedGraph, derive_seeds, discharge_laws,
    discharge_put_get_law, discharge_section_law,
};

#[cfg(test)]
use gmeow_logic_compile::ir::{CorrespondenceLaw, DischargeVerdict, MorphismClass};
#[cfg(test)]
use std::collections::BTreeSet;

#[cfg(test)]
fn term_str(term: &purrdf::RdfTerm) -> String {
    gmeow_logic::correspondence_exec::term_key(term)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
    const EX: &str = "http://example.org/";
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    fn repo_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repo root resolves")
    }

    fn read_query(name: &str) -> String {
        let path = repo_root().join("generated").join("queries").join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    // ── The real committed SIOC fixture: the three CompleteOver cells round-trip exactly. ──
    #[test]
    fn sioc_section_law_discharged_on_the_complete_over_cells() {
        let get_rq = read_query("sioc.rq");
        let put_rq = read_query("sioc.put.rq");
        // The exact three recoverable source atoms (per the shipped CompleteOver up-lift).
        let seed = SeedGraph {
            label: "sioc-complete-over".to_owned(),
            atoms: vec![
                (
                    format!("{EX}t1"),
                    RDF_TYPE.to_owned(),
                    format!("{GMEOW}Thread"),
                ),
                (
                    format!("{EX}m1"),
                    format!("{GMEOW}partOfThread"),
                    format!("{EX}th1"),
                ),
                (
                    format!("{EX}r1"),
                    format!("{GMEOW}inReplyTo"),
                    format!("{EX}p1"),
                ),
            ],
        };
        let outcome = discharge_section_law(&get_rq, &put_rq, std::slice::from_ref(&seed));
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "the three SIOC CompleteOver cells must discharge the section law\n{outcome:#?}"
        );
        assert!(outcome.countermodel.is_none());
    }

    // ── Inter-leg carrier round-trips non-IRI terms (literal / blank node). ──
    //
    // The forward image of these correspondences carries a term that is NOT an IRI — a literal in
    // one case, a fresh blank node in the other. The seed is all-IRI (so its own serialization is
    // untouched); the non-IRI term appears ONLY in the carrier between the get and put legs. The
    // pre-fix `atoms_to_ntriples` blanket-wrapped every component in `<...>`, so it fed the put
    // leg a malformed line (`<...> <...> <"foo">` / `<...> <...> <_:b>`) that the N-Triples parser
    // REJECTS → `run_leg` returned `Err` → a spurious `ObligationViolated`. Threading typed quads
    // through the canonical serializer makes the carrier well-formed, so the seed round-trips and
    // the law discharges. These tests FAIL on the old all-IRI serializer and PASS now.

    // get mints a constant LITERAL object into the forward image; put matches that literal and
    // reconstructs the exact source atom. The literal lives only in the carrier.
    const LITERAL_GET: &str = "\
CONSTRUCT { ?s <http://ext.example/label> \"foo\" }
WHERE { ?s <http://src.example/p> ?o }";
    const LITERAL_PUT: &str = "\
CONSTRUCT { ?s <http://src.example/p> <http://o.example/y> }
WHERE { ?s <http://ext.example/label> \"foo\" }";

    #[test]
    fn literal_object_in_the_carrier_round_trips_and_discharges() {
        let seed = SeedGraph {
            label: "literal-carrier".to_owned(),
            atoms: vec![(
                "http://s.example/x".to_owned(),
                "http://src.example/p".to_owned(),
                "http://o.example/y".to_owned(),
            )],
        };
        let outcome = discharge_section_law(LITERAL_GET, LITERAL_PUT, std::slice::from_ref(&seed));
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "a literal in the inter-leg carrier must round-trip, not produce a false \
             ObligationViolated\n{outcome:#?}"
        );
        assert!(outcome.countermodel.is_none());
    }

    // A datatyped literal (the datatype must survive serialization → parse → match too).
    const TYPED_LITERAL_GET: &str = "\
CONSTRUCT { ?s <http://ext.example/n> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> }
WHERE { ?s <http://src.example/p> ?o }";
    const TYPED_LITERAL_PUT: &str = "\
CONSTRUCT { ?s <http://src.example/p> <http://o.example/y> }
WHERE { ?s <http://ext.example/n> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> }";

    #[test]
    fn datatyped_literal_in_the_carrier_round_trips_and_discharges() {
        let seed = SeedGraph {
            label: "typed-literal-carrier".to_owned(),
            atoms: vec![(
                "http://s.example/x".to_owned(),
                "http://src.example/p".to_owned(),
                "http://o.example/y".to_owned(),
            )],
        };
        let outcome = discharge_section_law(
            TYPED_LITERAL_GET,
            TYPED_LITERAL_PUT,
            std::slice::from_ref(&seed),
        );
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "a datatyped literal in the carrier must round-trip with its datatype intact\n{outcome:#?}"
        );
    }

    // get mints a fresh BLANK NODE that joins two forward-image triples; put joins on it and
    // reconstructs the source. The blank lives only in the carrier.
    const BLANK_GET: &str = "\
CONSTRUCT { ?s <http://ext.example/r> _:b . _:b <http://ext.example/v> ?o }
WHERE { ?s <http://src.example/p> ?o }";
    const BLANK_PUT: &str = "\
CONSTRUCT { ?s <http://src.example/p> ?o }
WHERE { ?s <http://ext.example/r> ?b . ?b <http://ext.example/v> ?o }";

    #[test]
    fn blank_node_in_the_carrier_round_trips_and_discharges() {
        let seed = SeedGraph {
            label: "blank-carrier".to_owned(),
            atoms: vec![(
                "http://s.example/x".to_owned(),
                "http://src.example/p".to_owned(),
                "http://o.example/y".to_owned(),
            )],
        };
        let outcome = discharge_section_law(BLANK_GET, BLANK_PUT, std::slice::from_ref(&seed));
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "a fresh blank node in the inter-leg carrier must round-trip, not produce a false \
             ObligationViolated\n{outcome:#?}"
        );
        assert!(outcome.countermodel.is_none());

        let put_get = discharge_put_get_law(BLANK_GET, BLANK_PUT, &[seed]);
        assert_eq!(
            put_get.verdict,
            DischargeVerdict::ObligationDischarged,
            "fresh blank-node labels must compare by RDF graph isomorphism in get∘put\n{put_get:#?}"
        );
    }

    // The quoted-triple comparison key must be injective: two DISTINCT RDF-star quoted triples
    // must NOT render to the same `Atom` string (the pre-fix `<<triple>>` placeholder collapsed
    // them, so a fabricated/dropped quoted-triple atom could hide in the set comparison).
    #[test]
    fn distinct_quoted_triples_do_not_collapse_to_equal_atoms() {
        use purrdf::{RdfTerm, RdfTriple};
        let qt1 = RdfTerm::triple(RdfTriple::new(
            RdfTerm::iri("http://s.example/a"),
            "http://p.example/rel",
            RdfTerm::iri("http://o.example/b"),
        ));
        let qt2 = RdfTerm::triple(RdfTriple::new(
            RdfTerm::iri("http://s.example/a"),
            "http://p.example/rel",
            RdfTerm::iri("http://o.example/c"),
        ));
        assert_ne!(
            term_str(&qt1),
            term_str(&qt2),
            "distinct quoted triples must render to distinct atom keys, not a collapsing placeholder"
        );
    }

    // A get leg with two independent branches; a put leg that recovers both AND fabricates a
    // type-guard atom whenever branch-2 data is present. A single happy-path seed touching only
    // branch-1 MISSES the fabrication; the branch-covering corpus CATCHES it. (AC2 integrity.)
    const FAB_GET: &str = "\
PREFIX src: <http://src.example/>
PREFIX ext: <http://ext.example/>
CONSTRUCT {
  ?a ext:p1 ?b .
  ?c ext:p2 ?d .
} WHERE {
  { ?a src:rel1 ?b . }
  UNION
  { ?c src:rel2 ?d . }
}";

    const FAB_PUT: &str = "\
PREFIX src: <http://src.example/>
PREFIX ext: <http://ext.example/>
CONSTRUCT {
  ?a src:rel1 ?b .
  ?c src:rel2 ?d .
  ?c a src:GuardType .
} WHERE {
  { ?a ext:p1 ?b . }
  UNION
  { ?c ext:p2 ?d . }
}";

    fn happy_path_branch1_seed() -> SeedGraph {
        SeedGraph {
            label: "happy".to_owned(),
            atoms: vec![(
                "http://seed.example/a".to_owned(),
                "http://src.example/rel1".to_owned(),
                "http://seed.example/b".to_owned(),
            )],
        }
    }

    #[test]
    fn single_happy_path_seed_misses_the_fabricated_guard_atom() {
        // Branch-1 only: the fabricating put branch (keyed on ext:p2) never fires, so the seed
        // round-trips cleanly — a lone happy-path seed would wrongly report the law discharged.
        let seed = happy_path_branch1_seed();
        let outcome = discharge_section_law(FAB_GET, FAB_PUT, std::slice::from_ref(&seed));
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "a single branch-1 seed MUST miss the branch-2 fabrication (that is the blind spot)\n{outcome:#?}"
        );
    }

    #[test]
    fn branch_covering_corpus_catches_the_fabricated_guard_atom() {
        let seeds = derive_seeds(FAB_GET);
        // The corpus must exercise branch-2 (and the combined seed): at least three seeds.
        assert!(
            seeds.len() >= 3,
            "expected one seed per branch plus combined, got {}: {seeds:#?}",
            seeds.len()
        );
        let outcome = discharge_section_law(FAB_GET, FAB_PUT, &seeds);
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationViolated,
            "the branch-covering corpus MUST catch the branch-2 fabrication\n{outcome:#?}"
        );
        let cm = outcome.countermodel.expect("a countermodel is present");
        // The spurious atom is the fabricated `?c a src:GuardType`.
        assert!(
            cm.spurious
                .iter()
                .any(|(_, p, o)| p == RDF_TYPE && o == "http://src.example/GuardType"),
            "the countermodel must name the fabricated GuardType atom\n{cm:#?}"
        );
        assert!(
            cm.missing.is_empty(),
            "nothing was dropped, only fabricated\n{cm:#?}"
        );
    }

    #[test]
    fn discharge_is_deterministic_in_verdict_and_countermodel_bytes() {
        let seeds = derive_seeds(FAB_GET);
        let a = discharge_section_law(FAB_GET, FAB_PUT, &seeds);
        let b = discharge_section_law(FAB_GET, FAB_PUT, &seeds);
        assert_eq!(
            a, b,
            "same inputs must yield an identical outcome (verdict + countermodel)"
        );
        // Countermodel bytes are stable across independent seed-derivation runs too.
        let seeds2 = derive_seeds(FAB_GET);
        assert_eq!(seeds, seeds2, "seed derivation must be deterministic");
        let c = discharge_section_law(FAB_GET, FAB_PUT, &seeds2);
        assert_eq!(a, c);
    }

    #[test]
    fn derive_seeds_is_branch_covering_with_fresh_distinct_iris() {
        let seeds = derive_seeds(FAB_GET);
        // branch-0, branch-1, combined.
        let labels: Vec<&str> = seeds.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["branch-0", "branch-1", "combined"],
            "{seeds:#?}"
        );
        // Each per-branch seed carries exactly its one positive pattern.
        assert_eq!(seeds[0].atoms.len(), 1);
        assert_eq!(seeds[1].atoms.len(), 1);
        // The combined seed unions both branches (two distinct atoms, distinct IRIs).
        assert_eq!(seeds[2].atoms.len(), 2, "{:#?}", seeds[2]);
        let all_iris: BTreeSet<&String> = seeds
            .iter()
            .flat_map(|s| s.atoms.iter().flat_map(|(a, _, c)| [a, c]))
            .collect();
        // v0..v3 across the two branches — all fresh and distinct, deterministic.
        assert!(all_iris.contains(&"http://seed.example/v0".to_owned()));
        assert!(all_iris.contains(&"http://seed.example/v3".to_owned()));
    }

    #[test]
    fn non_executable_leg_is_violated_never_a_silent_pass() {
        let seed = happy_path_branch1_seed();
        let broken = "this is not valid SPARQL {{{";
        let outcome = discharge_section_law(broken, FAB_PUT, std::slice::from_ref(&seed));
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationViolated,
            "a malformed get leg must hard-fail to Violated, never pass\n{outcome:#?}"
        );
        assert!(outcome.countermodel.is_some());
    }

    #[test]
    fn empty_corpus_is_unknown_not_discharged() {
        let outcome = discharge_section_law(FAB_GET, FAB_PUT, &[]);
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationUnknown,
            "an unchecked law is Unknown — never proved absent"
        );
    }

    #[test]
    fn discharge_laws_gates_claims_by_rung() {
        // BridgeView (floor) claims no injective law.
        let none = discharge_laws(FAB_GET, FAB_PUT, MorphismClass::BridgeView);
        assert!(
            none.is_empty(),
            "a non-injective rung claims no section/put-get law\n{none:#?}"
        );

        // SectionRetraction claims BOTH SectionLaw and PutGet; the fabrication makes them Violated.
        let claims = discharge_laws(FAB_GET, FAB_PUT, MorphismClass::SectionRetraction);
        let laws: BTreeSet<CorrespondenceLaw> = claims.iter().map(|c| c.law).collect();
        assert!(laws.contains(&CorrespondenceLaw::SectionLaw), "{claims:#?}");
        assert!(laws.contains(&CorrespondenceLaw::PutGet), "{claims:#?}");
        let section = claims
            .iter()
            .find(|c| c.law == CorrespondenceLaw::SectionLaw)
            .expect("section claim present");
        assert_eq!(
            section.verdict,
            DischargeVerdict::ObligationViolated,
            "{section:#?}"
        );
    }

    #[test]
    fn discharge_laws_on_real_sioc_produces_claims() {
        // The shipped SIOC get leg has lossy branches (mapSiocTopic), so the auto-derived
        // branch corpus does NOT globally discharge the section law — but the service must run
        // end-to-end on the real queries and return a claim per permitted law.
        let get_rq = read_query("sioc.rq");
        let put_rq = read_query("sioc.put.rq");
        let claims = discharge_laws(&get_rq, &put_rq, MorphismClass::SectionRetraction);
        assert_eq!(claims.len(), 2, "SectionLaw + PutGet\n{claims:#?}");
        for c in &claims {
            assert_ne!(
                c.verdict,
                DischargeVerdict::ObligationUnknown,
                "a non-empty SIOC corpus must yield a decided verdict\n{c:#?}"
            );
        }
    }
}
