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

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use gmeow_logic::reason::{RlClosure, rl_closure};
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm, parse_dataset};

/// The gmeow ontology namespace.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The example / test A-Box namespace.
const EX: &str = "https://example.org/test/";
/// `rdf:type`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdfs:subClassOf`.
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
/// `rdf:first` — the member cell of an RDF list (an `owl:unionOf` list).
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";

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

/// The three justification/defeat union classes.
const UNIONS: &[&str] = &["JustificationSubject", "JustificationGround", "Defeater"];

/// The expected union classification of EVERY named gmeow class the epistemics
/// module declares — the conformance target the `logic:` closure must reproduce.
/// Each entry lists the union classes an instance of that class MUST be classified
/// under (transitively, including inherited membership through `rdfs:subClassOf`);
/// the three union classes themselves classify only as themselves (`owl:unionOf` in
/// superclass position is not an OWL 2 RL entailment, so a union instance is not
/// pushed down to its members). Memberships in the slice:
///   JustificationSubject <= { DoxasticState, StandpointClaim }
///   JustificationGround  <= { EvidenceSpan, Attestation, DoxasticState }
///   Defeater             <= { Argument (inference-slice, not declared here),
///                             StandpointClaim, EvidenceSpan }
/// `DoxasticStandpointClaim <= StandpointClaim` inherits StandpointClaim's unions.
const EXPECTED_UNION_MEMBERSHIP: &[(&str, &[&str])] = &[
    ("AdequacyAssessment", &[]),
    ("AdequacyVerdict", &[]),
    ("Argument", &["Defeater"]),
    ("Attestation", &["JustificationGround"]),
    ("ClaimEvaluation", &[]),
    ("ClaimToken", &[]),
    ("Defeater", &["Defeater"]),
    (
        "DoxasticStandpointClaim",
        &["JustificationSubject", "Defeater"],
    ),
    (
        "DoxasticState",
        &["JustificationSubject", "JustificationGround"],
    ),
    ("DoxasticTenure", &[]),
    ("EpistemicContext", &[]),
    ("EpistemicStandard", &[]),
    ("EvidenceSpan", &["JustificationGround", "Defeater"]),
    ("JustificationGround", &["JustificationGround"]),
    ("JustificationStatus", &[]),
    ("JustificationSubject", &["JustificationSubject"]),
    ("KnowledgeAttribution", &[]),
    ("KnowledgeClaim", &[]),
    ("Proposition", &[]),
    ("StandpointClaim", &["JustificationSubject", "Defeater"]),
    ("SupportAssessment", &[]),
];

/// Every gmeow class the epistemics module makes a structural claim about: the
/// subject of any `rdfs:subClassOf` (every class the module positions in the
/// hierarchy — note some union members such as `StandpointClaim`/`EvidenceSpan`/
/// `Attestation` are declared in other slices and only *augmented* here with a
/// subclass edge, so keying on `a owl:Class` would miss exactly the members we must
/// pin) UNION every gmeow class named in an `owl:unionOf` list (an `rdf:first`
/// member, which pulls in `Argument`, the inference-slice `Defeater` member).
/// Blank restriction/union wrappers are excluded (they are not named IRIs).
fn epistemics_class_universe(quads: &[RdfQuad]) -> BTreeSet<String> {
    let mut classes = BTreeSet::new();
    for quad in quads {
        if quad.predicate == RDFS_SUBCLASS_OF {
            if let RdfTerm::Iri(subject) = &quad.subject {
                if subject.starts_with(GMEOW) {
                    classes.insert(subject.clone());
                }
            }
        }
        if quad.predicate == RDF_FIRST {
            if let RdfTerm::Iri(object) = &quad.object {
                if object.starts_with(GMEOW) {
                    classes.insert(object.clone());
                }
            }
        }
    }
    classes
}

/// Union membership pinned as a `logic:` reasoner entailment over the WHOLE named
/// class universe of the epistemics module, not a hand-picked candidate roster:
/// enumerate every class the module declares, inject one probe instance of each,
/// take a single RL closure, and assert its classification across
/// `{JustificationSubject, JustificationGround, Defeater}` matches
/// `EXPECTED_UNION_MEMBERSHIP` exactly. This recovers open-universe stray detection
/// through the closure — a stray member added to any union classifies some probe
/// unexpectedly; a dropped member leaves an expected probe unclassified; and a
/// newly-declared class the map does not cover is itself a hard failure, forcing it
/// to be consciously classified. The reasoner is the authority; no `owl:unionOf`
/// list is read.
#[test]
fn union_membership_extensions_are_exact() {
    let expected: BTreeMap<String, BTreeSet<String>> = EXPECTED_UNION_MEMBERSHIP
        .iter()
        .map(|(class, unions)| (gmeow(class), unions.iter().map(|u| gmeow(u)).collect()))
        .collect();

    let module_quads = turtle_quads(&[module("core/epistemics")]);
    let universe = epistemics_class_universe(&module_quads);
    assert!(
        !universe.is_empty(),
        "no gmeow classes discovered in the epistemics module"
    );

    // Open-universe guard: every class in the module's class universe MUST be
    // classified in the map, and the map MUST NOT carry a class the module no longer
    // mentions.
    let unmapped: Vec<&String> = universe.iter().filter(|c| !expected.contains_key(*c)).collect();
    assert!(
        unmapped.is_empty(),
        "epistemics classes with no union-membership expectation (classify each in EXPECTED_UNION_MEMBERSHIP): {unmapped:?}"
    );
    let phantom: Vec<&String> = expected.keys().filter(|c| !universe.contains(*c)).collect();
    assert!(
        phantom.is_empty(),
        "EXPECTED_UNION_MEMBERSHIP names classes the module no longer mentions (remove them): {phantom:?}"
    );

    // One distinct probe instance per class in the universe, one closure over all.
    let probes: Vec<(String, String)> = universe
        .iter()
        .enumerate()
        .map(|(idx, class)| (ex(&format!("probe{idx}")), class.clone()))
        .collect();
    let abox: Vec<RdfQuad> = probes
        .iter()
        .map(|(individual, class)| iri_quad(individual, RDF_TYPE, class))
        .collect();
    let closure = scoped_closure(&["core/epistemics"], &abox);
    assert_non_trivial(&closure);

    for (individual, class) in &probes {
        let expected_unions = &expected[class];
        for union in UNIONS {
            let classified = has_type(&closure, individual, &gmeow(union));
            let should = expected_unions.contains(&gmeow(union));
            assert_eq!(
                classified, should,
                "a {class} instance {} classify as gmeow:{union}",
                if should { "MUST" } else { "must NOT" }
            );
        }
    }
}
