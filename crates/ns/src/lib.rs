// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW's registered ontology term namespaces, and the single vocabulary /
//! profile constructor every consumer builds from them.
//!
//! purrdf is a namespace-neutral toolkit: its slice catalog + ownership analyzer,
//! its slice emitters, the SHACL→JSON-Schema keying, and the JSON-LD-star
//! statement-metadata downcast all take a namespace/vocab from the CONSUMER
//! rather than baking in any ontology. GMEOW is that consumer, and it mints
//! ontology terms into **four** namespaces, not one.
//!
//! ## Why one declaration site, and why this low
//!
//! [`purrdf::SliceVocab`] distinguishes the *framework* namespace (which mints
//! `gmeow:Slice`, `gmeow:sliceTier`, `gmeow:sliceDependsOn`) from the *owned term
//! namespaces* (the namespaces a corpus's slices mint ontology terms into). A
//! namespace GMEOW mints into but never declares is invisible to ownership
//! analysis: `rdfs:isDefinedBy` claims and typed vocabulary terms whose subject
//! lies there are dropped, so every reference to those terms resolves to no
//! owning slice and contributes no dependency edge. Nothing reports this — the
//! analysis simply says the slice has no dependents and nothing depends on it.
//!
//! The `math` slice mints its entire vocabulary into `math:` and nothing into
//! `gmeow:`, so declaring only the framework namespace made every dependency on
//! `math` uncomputable. That is exactly the failure a duplicated constructor
//! hides: six call sites each said `for_namespace(GMEOW_NS)` and each was wrong
//! in the same invisible way. There is now one constructor, and it is stated
//! once — [`TERM_NAMESPACES`].
//!
//! This crate depends on `purrdf` and on no first-party crate at all, because its
//! consumers (`gmeow-validate`, `gmeow-docs`, `gmeow-slice-brief`,
//! `gmeow-pipeline`, `gmeow-dev-cli`) sit at four different heights in the crate
//! layering. A shared constructor placed in any of them would be a layering
//! inversion for the others.
//!
//! ```
//! let vocab = gmeow_ns::gmeow_slice_vocab();
//! assert!(vocab.owns_term("https://blackcatinformatics.ca/gmeow/Slice"));
//! assert!(vocab.owns_term("https://blackcatinformatics.ca/math/Quantity"));
//! assert!(!vocab.owns_term("http://www.w3.org/2002/07/owl#Class"));
//! ```

use purrdf::{Namespaces, OntologyProfile, SliceVocab};

/// GMEOW's canonical ontology namespace (trailing `/` for term concatenation).
/// This is also the slice-FRAMEWORK namespace: `gmeow:Slice`, `gmeow:sliceTier`,
/// `gmeow:sliceDependsOn` and the analysis-graph terms are minted here.
pub const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// GMEOW's logic-core namespace — the canonical reasoning language.
pub const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// GMEOW's language grounding namespace (the semiotic grounding layer, peer of
/// `logic:` and `math:`; the grounding order is `logic:` < `lang:` < `math:`).
pub const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";

/// GMEOW's mathematics grounding namespace, peer of `logic:` and `lang:`.
pub const MATH_NS: &str = "https://blackcatinformatics.ca/math/";

/// **The** registered term namespaces: every namespace a GMEOW slice is allowed
/// to mint ontology terms into.
///
/// This is the single authority. `crates/validate`'s authoring gate asserts no
/// `module.ttl` / `shapes.ttl` mints a term subject outside this set, and
/// [`gmeow_slice_vocab`] hands the identical set to purrdf's ownership analyzer,
/// so "a slice may mint here" and "the analyzer can see terms minted here" are
/// the same fact rather than two facts that can drift apart.
pub const TERM_NAMESPACES: [&str; 4] = [GMEOW_NS, LOGIC_NS, LANG_NS, MATH_NS];

/// The IRI authority prefix every GMEOW-minted IRI shares.
///
/// This is the test for "GMEOW minted this IRI" as opposed to "GMEOW is
/// describing someone else's term": a `dcterms:` or `skos:` IRI redeclared in a
/// module is a foreign term GMEOW does not own, while an IRI under this authority
/// is GMEOW's own even when its namespace is not (yet) registered. Exactly that
/// second case is the invisible-slice defect, so it is what the authoring gate
/// keys on.
///
/// Every entry in [`TERM_NAMESPACES`] starts with this prefix; a registered
/// namespace that did not would mean GMEOW mints under a second authority, and
/// the unit tests below refuse it.
pub const GMEOW_AUTHORITY: &str = "https://blackcatinformatics.ca/";

/// The CURIE prefix each registered term namespace is bound to, in the order
/// emitters declare them.
pub const TERM_NAMESPACE_PREFIXES: [(&str, &str); 4] = [
    ("gmeow", GMEOW_NS),
    ("logic", LOGIC_NS),
    ("lang", LANG_NS),
    ("math", MATH_NS),
];

/// The registered term namespace `iri` lies in, or `None` if it lies in none.
///
/// The longest match wins, so a namespace nested inside another resolves to the
/// more specific one rather than to whichever happens to be tested first.
#[must_use]
pub fn registered_term_namespace(iri: &str) -> Option<&'static str> {
    TERM_NAMESPACES
        .into_iter()
        .filter(|ns| iri.starts_with(ns))
        .max_by_key(|ns| ns.len())
}

/// GMEOW's single ontology profile: the `gmeow:` primary namespace plus the
/// authored `logic:`, `lang:`, and `math:` prefixes. purrdf's builtins
/// (xsd/rdf/rdfs/owl/sh) are always available on top of these, so the profile only
/// carries GMEOW's own vocab.
#[must_use]
pub fn gmeow_profile() -> OntologyProfile {
    OntologyProfile::for_namespace(GMEOW_NS)
        .with_prefix("gmeow")
        .with_prefixes(
            TERM_NAMESPACE_PREFIXES
                .into_iter()
                .map(|(prefix, ns)| (prefix.to_owned(), ns.to_owned()))
                .collect(),
        )
}

/// **The** slice vocabulary: prefix `gmeow`, framework namespace [`GMEOW_NS`],
/// and all four [`TERM_NAMESPACES`] declared as owned term namespaces.
///
/// Every `SliceCatalog::discover` / `OwnershipAnalyzer` construction in the
/// workspace passes this. Constructing a `SliceVocab` any other way re-opens the
/// invisible-namespace hole described at the module level.
#[must_use]
pub fn gmeow_slice_vocab() -> SliceVocab {
    gmeow_profile()
        .slice_vocab()
        .with_term_namespaces(TERM_NAMESPACES)
}

/// The SHACL→JSON-Schema keying namespaces (GMEOW primary + authored prefixes).
///
/// Construction cannot fail: the `gmeow` primary prefix is always declared by
/// [`gmeow_profile`].
///
/// # Panics
///
/// Never in practice — the expectation documents an invariant of
/// [`gmeow_profile`], not a runtime condition.
#[must_use]
pub fn gmeow_json_schema_namespaces() -> Namespaces {
    gmeow_profile()
        .namespaces()
        .expect("gmeow primary prefix is declared in gmeow_profile")
}

#[cfg(test)]
mod tests {
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
}
