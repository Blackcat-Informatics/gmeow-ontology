// SPDX-License-Identifier: AGPL-3.0-only

//! Instance-level (A-Box) entailment conformance for the epistemics slice.
//!
//! The complement to the structural `crates/validate` twins: these tests drive the
//! native `logic:` reasoner over injected instance data and assert what the closure
//! does — and does not — derive. Three concerns:
//!
//!  * the keystone `knowsThat ⊑ believes` fires at the instance level (asserting
//!    `knowsThat` materialises `believes`);
//!  * the factivity boundary holds: the deliberately non-factive siblings
//!    (`knowsThatIn`, `claimsToKnowThat`, `takesAsKnown`) do NOT collapse into
//!    `believes` / `knowsThat`;
//!  * union-class membership is entailed — a `DoxasticState` / `StandpointClaim` /
//!    `EvidenceSpan` / `Attestation` / `Argument` instance is classified under the
//!    justification/defeat union it belongs to, and a `Proposition` is not.
//!
//! The union semantics are pinned here as a reasoner entailment rather than by
//! reading the class expression, so the `logic:` closure (the canonical superset)
//! is the authority.

use std::path::PathBuf;

use gmeow_logic::reason::{RlClosure, rl_closure};
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm, parse_dataset};

/// The gmeow ontology namespace.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The example / test A-Box namespace.
const EX: &str = "https://example.org/test/";
/// `rdf:type`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// `https://blackcatinformatics.ca/gmeow/<local>`.
fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// `https://example.org/test/<local>`.
fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

/// Repo root (`CARGO_MANIFEST_DIR` = `<repo>/crates/logic`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The canonical terms file of a slice, by `<group>/<name>`.
fn module(slice: &str) -> String {
    format!("slices/{slice}/module.ttl")
}

/// Parse the given repo-relative Turtle files into default-world quads. HARD-FAIL
/// (panic) on a missing or unparsable file — no skip, no optional fallback.
fn turtle_quads(rel_paths: &[String]) -> Vec<RdfQuad> {
    let root = repo_root();
    let mut quads = Vec::new();
    for rel in rel_paths {
        let path = root.join(rel);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("missing ontology source {}: {e}", path.display()));
        let dataset = parse_dataset(&bytes, "text/turtle", None)
            .unwrap_or_else(|e| panic!("Turtle parse failed for {}: {e}", path.display()));
        quads.extend(dataset.owned_quads());
    }
    quads
}

fn dataset_from_quads(quads: Vec<RdfQuad>) -> std::sync::Arc<purrdf::RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("valid scoped test dataset")
}

/// An RL closure of the named slice modules plus injected `abox` quads.
fn scoped_closure(slices: &[&str], abox: &[RdfQuad]) -> RlClosure {
    let mut paths: Vec<String> = slices.iter().map(|s| module(s)).collect();
    paths.sort();
    let mut quads = turtle_quads(&paths);
    quads.extend_from_slice(abox);
    let dataset = dataset_from_quads(quads);
    rl_closure(dataset.as_ref()).expect("scoped OWL 2 RL closure should succeed")
}

/// An IRI-subject / IRI-object quad in the single default world.
fn iri_quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o))
}

/// Strip an optional surrounding `<…>` so closure terms compare against bare IRIs.
fn unwrap_iri(term: &str) -> &str {
    term.strip_prefix('<')
        .and_then(|t| t.strip_suffix('>'))
        .unwrap_or(term)
}

/// `true` iff the closure contains the IRI triple `s p o`.
fn contains(closure: &RlClosure, s: &str, p: &str, o: &str) -> bool {
    closure.triples.iter().any(|t| {
        unwrap_iri(&t.subject) == s && unwrap_iri(&t.predicate) == p && unwrap_iri(&t.object) == o
    })
}

/// `true` iff the closure types `individual` as `class`.
fn has_type(closure: &RlClosure, individual: &str, class: &str) -> bool {
    contains(closure, individual, RDF_TYPE, class)
}

/// Assert `s p o` is ENTAILED but not asserted: present in `closure`, absent from
/// the injected A-Box.
fn assert_entailed(closure: &RlClosure, asserted: &[RdfQuad], s: &str, p: &str, o: &str) {
    let asserted_here = asserted.iter().any(|q| {
        matches!(&q.subject, RdfTerm::Iri(qs) if qs == s)
            && q.predicate == p
            && matches!(&q.object, RdfTerm::Iri(qo) if qo == o)
    });
    assert!(
        !asserted_here,
        "{s} {p} {o} is authored in the A-Box; it cannot be 'entailed not asserted'"
    );
    assert!(
        contains(closure, s, p, o),
        "{s} {p} {o} must be ENTAILED by the closure (authored nowhere)"
    );
}

/// Guard: a scoped closure over the epistemics module is non-trivial, so a
/// silently-empty / mis-parsed closure cannot pass the positive checks by vacuity.
fn assert_non_trivial(closure: &RlClosure) {
    assert!(
        closure.triples.len() > 10,
        "the scoped epistemics RL closure should be non-trivial; got {}",
        closure.triples.len()
    );
}

// ── (a) Keystone: knowsThat ⊑ believes at the instance level ───────────────────

#[test]
fn knows_that_entails_believes() {
    let (llm, prop) = (ex("llm"), ex("propRecall"));
    let abox = vec![iri_quad(&llm, &gmeow("knowsThat"), &prop)];
    let closure = scoped_closure(&["core/epistemics"], &abox);
    assert_non_trivial(&closure);
    // Knowledge entails belief: the safe subproperty entailment materialises.
    assert_entailed(&closure, &abox, &llm, &gmeow("believes"), &prop);
}

// ── (b) Factivity boundary: the non-factive siblings do NOT collapse ────────────

#[test]
fn non_factive_siblings_do_not_collapse_into_belief() {
    // Each sibling is deliberately NOT a subproperty of believes/knowsThat, so
    // asserting it must NOT materialise either doxastic-spine edge.
    for sibling in ["knowsThatIn", "claimsToKnowThat", "takesAsKnown"] {
        let (agent, prop) = (ex("agentFor"), ex("propFor"));
        let abox = vec![iri_quad(&agent, &gmeow(sibling), &prop)];
        let closure = scoped_closure(&["core/epistemics"], &abox);
        assert_non_trivial(&closure);
        assert!(
            !contains(&closure, &agent, &gmeow("believes"), &prop),
            "{sibling} must NOT collapse into gmeow:believes (factivity boundary)"
        );
        assert!(
            !contains(&closure, &agent, &gmeow("knowsThat"), &prop),
            "{sibling} must NOT collapse into gmeow:knowsThat (factivity boundary)"
        );
    }
}

// ── (c) Union-class membership as a reasoner entailment ────────────────────────

/// Positive: a member-typed instance is classified under the union it belongs to.
#[test]
fn union_membership_is_entailed() {
    // (member local name, expected union local names)
    let cases: &[(&str, &[&str])] = &[
        ("DoxasticState", &["JustificationSubject", "JustificationGround"]),
        ("StandpointClaim", &["JustificationSubject"]),
        ("EvidenceSpan", &["JustificationGround"]),
        ("Attestation", &["JustificationGround"]),
        ("Argument", &["Defeater"]),
    ];
    for (idx, (member, unions)) in cases.iter().enumerate() {
        let individual = ex(&format!("member{idx}"));
        let abox = vec![iri_quad(&individual, RDF_TYPE, &gmeow(member))];
        let closure = scoped_closure(&["core/epistemics"], &abox);
        assert_non_trivial(&closure);
        for union in *unions {
            assert!(
                has_type(&closure, &individual, &gmeow(union)),
                "a {member} instance must be classified as {union}"
            );
        }
    }
}

/// Negative: a `Proposition` is neither a justification subject nor a ground — the
/// unions do not over-classify.
#[test]
fn proposition_is_not_a_justification_union_member() {
    let prop = ex("standaloneProp");
    let abox = vec![iri_quad(&prop, RDF_TYPE, &gmeow("Proposition"))];
    let closure = scoped_closure(&["core/epistemics"], &abox);
    assert_non_trivial(&closure);
    assert!(
        !has_type(&closure, &prop, &gmeow("JustificationSubject")),
        "a Proposition must NOT classify as a JustificationSubject"
    );
    assert!(
        !has_type(&closure, &prop, &gmeow("JustificationGround")),
        "a Proposition must NOT classify as a JustificationGround"
    );
}
