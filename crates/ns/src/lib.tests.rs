// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The four namespaces are distinct, `/`-terminated, and share the GMEOW
/// authority — the shape every consumer's term-concatenation assumes.
#[test]
fn the_registered_namespaces_are_well_formed() {
    let mut seen = std::collections::BTreeSet::new();
    for ns in TERM_NAMESPACES {
        assert!(ns.ends_with('/'), "{ns} must end in `/` for concatenation");
        assert!(
            ns.starts_with(GMEOW_AUTHORITY),
            "{ns} must be a GMEOW-authority namespace"
        );
        assert!(
            ns.len() > GMEOW_AUTHORITY.len(),
            "{ns} must be a proper sub-namespace of the authority"
        );
        assert!(seen.insert(ns), "{ns} is declared twice");
    }
    assert_eq!(seen.len(), TERM_NAMESPACES.len());
}

/// The prefix table and the namespace set are the same set — a prefix bound
/// to an unregistered namespace (or a registered namespace with no prefix)
/// would emit CURIEs the ownership analyzer cannot resolve.
#[test]
fn the_prefix_table_covers_exactly_the_registered_namespaces() {
    let from_prefixes: std::collections::BTreeSet<&str> = TERM_NAMESPACE_PREFIXES
        .into_iter()
        .map(|(_, ns)| ns)
        .collect();
    let registered: std::collections::BTreeSet<&str> = TERM_NAMESPACES.into_iter().collect();
    assert_eq!(from_prefixes, registered);
}

/// The vocab hands purrdf every registered namespace — this is the assertion
/// that would have failed before the four namespaces were declared, and the
/// one that fails again if a fifth is added to `TERM_NAMESPACES` but the
/// constructor stops forwarding it.
#[test]
fn the_slice_vocab_owns_every_registered_namespace() {
    let vocab = gmeow_slice_vocab();
    assert_eq!(vocab.ns(), GMEOW_NS);
    for ns in TERM_NAMESPACES {
        assert!(
            vocab.term_namespaces().contains(ns),
            "{ns} is registered but not declared to purrdf"
        );
        assert!(
            vocab.owns_term(&format!("{ns}SomeTerm")),
            "a term minted in {ns} must be owned"
        );
    }
}

/// A term outside every registered namespace is NOT owned — the property the
/// authoring gate mirrors, and the one that makes the gate non-vacuous.
#[test]
fn a_term_outside_the_registered_namespaces_is_not_owned() {
    let vocab = gmeow_slice_vocab();
    for foreign in [
        "http://www.w3.org/2002/07/owl#Class",
        "https://blackcatinformatics.ca/affect/Valence",
        "https://example.org/math/Quantity",
    ] {
        assert!(!vocab.owns_term(foreign), "{foreign} must not be owned");
        assert_eq!(registered_term_namespace(foreign), None);
    }
}

/// Namespace resolution is by longest match, not first match.
#[test]
fn registered_namespace_resolution_picks_the_longest_match() {
    assert_eq!(
        registered_term_namespace("https://blackcatinformatics.ca/math/Quantity"),
        Some(MATH_NS)
    );
    assert_eq!(
        registered_term_namespace("https://blackcatinformatics.ca/gmeow/slices/math"),
        Some(GMEOW_NS)
    );
}

/// The canonical spellings are minted in [`LOGIC_NS`] and the projected ones
/// are not GMEOW-authored at all — the property that makes "canonical first"
/// mean something rather than being an arbitrary array order.
#[test]
fn the_subsumption_predicates_are_canonical_first() {
    let preds = subsumption_predicates();
    assert_eq!(
        preds,
        [
            LOGIC_SUB_CLASS_OF,
            RDFS_SUB_CLASS_OF,
            LOGIC_SUB_PROPERTY_OF,
            RDFS_SUB_PROPERTY_OF
        ]
    );
    for canonical in [LOGIC_SUB_CLASS_OF, LOGIC_SUB_PROPERTY_OF] {
        assert_eq!(registered_term_namespace(canonical), Some(LOGIC_NS));
    }
    for projected in [RDFS_SUB_CLASS_OF, RDFS_SUB_PROPERTY_OF] {
        assert_eq!(registered_term_namespace(projected), None);
    }
    let distinct: std::collections::BTreeSet<&str> = preds.into_iter().collect();
    assert_eq!(distinct.len(), preds.len(), "a predicate is listed twice");
}

/// The two per-kind arrays partition the accessor: a consumer that needs only
/// class edges (a class hierarchy) and one that needs the whole taxonomy read
/// the SAME spellings, so neither can drift into seeing half the corpus.
#[test]
fn the_per_kind_arrays_partition_the_accessor() {
    let preds = subsumption_predicates();
    let mut joined: Vec<&str> = SUB_CLASS_OF.to_vec();
    joined.extend(SUB_PROPERTY_OF);
    assert_eq!(joined, preds.to_vec());
    for kind in [SUB_CLASS_OF, SUB_PROPERTY_OF] {
        assert!(
            kind[0].starts_with(LOGIC_NS),
            "{} must be canonical",
            kind[0]
        );
        assert!(
            !kind[1].starts_with(GMEOW_AUTHORITY),
            "{} must be foreign",
            kind[1]
        );
    }
}

/// The JSON-Schema keying view carries the same four prefixes.
#[test]
fn the_json_schema_namespaces_carry_the_registered_prefixes() {
    let ns = gmeow_json_schema_namespaces();
    for (prefix, namespace) in TERM_NAMESPACE_PREFIXES {
        assert_eq!(
            ns.expand_iri(&format!("{prefix}:Term")).as_deref(),
            Ok(format!("{namespace}Term").as_str()),
            "prefix {prefix} must expand to {namespace}"
        );
    }
}

#[test]
fn owl_view_of_type_marker_lowers_the_axiom_types() {
    // These three type markers are used in `rdf:type` object position,
    // each grounded by a `logic:corr…Grounding` law (exact structural rename).
    assert_eq!(
        owl_view_of_type_marker("https://blackcatinformatics.ca/logic/AllDisjointClasses"),
        Some("http://www.w3.org/2002/07/owl#AllDisjointClasses")
    );
    assert_eq!(
        owl_view_of_type_marker("https://blackcatinformatics.ca/logic/AllDisjointProperties"),
        Some("http://www.w3.org/2002/07/owl#AllDisjointProperties")
    );
    assert_eq!(
        owl_view_of_type_marker("https://blackcatinformatics.ca/logic/Axiom"),
        Some("http://www.w3.org/2002/07/owl#Axiom")
    );
    // A non-marker still passes through as None (no false lowering).
    assert_eq!(
        owl_view_of_type_marker("https://blackcatinformatics.ca/gmeow/Cat"),
        None
    );
}

#[test]
fn owl_view_of_predicate_covers_the_axiom_restriction_vocab() {
    // Subsumption + domain/range project onto the RDFS surface.
    assert_eq!(
        owl_view_of_predicate(LOGIC_SUB_CLASS_OF),
        Some(RDFS_SUB_CLASS_OF)
    );
    assert_eq!(owl_view_of_predicate(LOGIC_DOMAIN), Some(RDFS_DOMAIN));
    assert_eq!(owl_view_of_predicate(LOGIC_RANGE), Some(RDFS_RANGE));
    // A representative spread of the OWL-surface edges (namespace swaps).
    for (logic_local, owl_local) in [
        ("members", "members"),
        ("unionOf", "unionOf"),
        ("complementOf", "complementOf"),
        ("intersectionOf", "intersectionOf"),
        ("disjointWith", "disjointWith"),
        ("onProperty", "onProperty"),
        ("someValuesFrom", "someValuesFrom"),
        ("allValuesFrom", "allValuesFrom"),
        ("onClass", "onClass"),
        ("hasValue", "hasValue"),
        ("oneOf", "oneOf"),
        ("inverseOf", "inverseOf"),
        ("equivalentClass", "equivalentClass"),
        ("equivalentProperty", "equivalentProperty"),
        ("sameAs", "sameAs"),
        ("differentFrom", "differentFrom"),
        ("propertyChainAxiom", "propertyChainAxiom"),
        ("hasKey", "hasKey"),
        ("qualifiedCardinality", "qualifiedCardinality"),
        ("minQualifiedCardinality", "minQualifiedCardinality"),
    ] {
        assert_eq!(
            owl_view_of_predicate(&format!(
                "https://blackcatinformatics.ca/logic/{logic_local}"
            )),
            Some(format!("http://www.w3.org/2002/07/owl#{owl_local}").as_str())
        );
    }
    // A canonical typing marker is NOT a predicate — the predicate accessor
    // returns None so the two accessors stay disjoint.
    assert_eq!(owl_view_of_predicate(LOGIC_CLASS), None);
    assert_eq!(
        owl_view_of_predicate("https://blackcatinformatics.ca/gmeow/name"),
        None
    );
}

#[test]
fn class_position_predicate_spans_both_spellings_but_excludes_endpoints() {
    // Class-valued positions where a `logic:Thing` / `logic:Nothing` filler must lower —
    // canonical AND already-projected spellings both count.
    for p in [
        LOGIC_SUB_CLASS_OF,
        RDFS_SUB_CLASS_OF,
        LOGIC_DOMAIN,
        RDFS_DOMAIN,
        LOGIC_RANGE,
        RDFS_RANGE,
        "https://blackcatinformatics.ca/logic/complementOf",
        "http://www.w3.org/2002/07/owl#complementOf",
        "https://blackcatinformatics.ca/logic/onClass",
        "http://www.w3.org/2002/07/owl#onClass",
        "https://blackcatinformatics.ca/logic/someValuesFrom",
        "http://www.w3.org/2002/07/owl#allValuesFrom",
    ] {
        assert!(is_class_position_predicate(p), "{p} must be class-valued");
    }
    // A GroundingCorrespondence endpoint NAMES a term as data — it is NOT a class
    // position, so a `logic:Thing` object under it must be left untouched.
    assert!(!is_class_position_predicate(
        "https://blackcatinformatics.ca/logic/sourceEndpoint"
    ));
    assert!(!is_class_position_predicate(
        "https://blackcatinformatics.ca/logic/members"
    ));
    assert!(!is_class_position_predicate(
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
    ));
}
