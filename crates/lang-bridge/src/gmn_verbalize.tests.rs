// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn form(term: &str, label: &str, glyph: &str, fixity: &str, arity: u32) -> GmnOperatorForm {
    scoped(term, label, glyph, fixity, arity, "")
}

fn scoped(
    term: &str,
    label: &str,
    glyph: &str,
    fixity: &str,
    arity: u32,
    sigil: &str,
) -> GmnOperatorForm {
    GmnOperatorForm {
        term_iri: term.to_owned(),
        term_label: label.to_owned(),
        gmn_glyph: glyph.to_owned(),
        fixity: fixity.to_owned(),
        arity,
        sigil: sigil.to_owned(),
    }
}

/// The four fixity templates arrange the operator token and operands as documented, and
/// each NL string round-trips through the inverse template to its own `(fixity, label,
/// arity)`.
#[test]
fn templates_arrange_per_fixity_and_invert() {
    let cases = [
        (FIXITY_PREFIX, "not", 1u32, "not arg1"),
        (FIXITY_INFIX, "subsumes", 2, "arg1 subsumes arg2"),
        (FIXITY_INFIX, "for all", 2, "arg1 for all arg2"),
        (FIXITY_POSTFIX, "factorial", 1, "arg1 factorial"),
        (FIXITY_BRACKETING, "abs", 1, "abs [ arg1 ]"),
        (FIXITY_BRACKETING, "tuple", 2, "tuple [ arg1 , arg2 ]"),
    ];
    for (fixity, label, arity, expected) in cases {
        let nl = arrange(fixity, label, arity).expect("arrange");
        assert_eq!(nl, expected, "fixity {fixity} arrangement");
        let parsed = parse_nl(&nl).expect("inverse parses");
        assert_eq!(parsed.fixity, fixity, "recovered fixity for {nl:?}");
        assert_eq!(parsed.label, label, "recovered label for {nl:?}");
        assert_eq!(parsed.arity, arity, "recovered arity for {nl:?}");
        assert_eq!(parsed.curie, None);
    }
}

/// Perturbing ONLY the fixity changes the verbalization — the template is genuinely
/// fixity-driven, not a constant caption (falsifiability teeth).
#[test]
fn fixity_perturbation_changes_verbalization() {
    let infix = arrange(FIXITY_INFIX, "op", 2).unwrap();
    let prefix = arrange(FIXITY_PREFIX, "op", 2).unwrap();
    let postfix = arrange(FIXITY_POSTFIX, "op", 2).unwrap();
    assert_ne!(infix, prefix);
    assert_ne!(infix, postfix);
    assert_ne!(prefix, postfix);
}

/// A homograph (two distinct terms sharing one `rdfs:label`) is disambiguated by CURIE so
/// the controlled-NL map stays injective, and the tagged strings still round-trip.
#[test]
fn homograph_labels_are_disambiguated_and_still_invert() {
    let forms = vec![
        form("math:supersetRel", "contains", "⊃", FIXITY_INFIX, 2),
        form("math:hasElement", "contains", "∋", FIXITY_INFIX, 2),
    ];
    let pairs = build_verbalization_pairs(&forms).expect("pairs build");
    // Both NL strings are distinct (disambiguated) — injectivity held.
    assert_ne!(pairs[0].nl, pairs[1].nl);
    assert!(
        pairs.iter().all(|p| p.nl.contains("⟪")),
        "both carry a CURIE tag"
    );
    // And each still round-trips to its OWN form through the inverse + index.
    assert!(round_trip_holds(&pairs), "disambiguated pairs round-trip");
    let index = forward_index(&pairs);
    assert_eq!(invert_nl(&pairs[0].nl, &index), Some(&pairs[0].form));
    assert_eq!(invert_nl(&pairs[1].nl, &index), Some(&pairs[1].form));
}

/// An unbreakable collision (same label AND same CURIE on two distinct forms) is a HARD
/// FAIL — the verbalizer never ships a non-injective map.
#[test]
fn unbreakable_collision_hard_fails() {
    let forms = vec![
        form("logic:x", "same", "a", FIXITY_INFIX, 2),
        // A different glyph but the SAME term IRI + label collides irreparably.
        form("logic:x", "same", "b", FIXITY_INFIX, 2),
    ];
    let err = build_verbalization_pairs(&forms).expect_err("must hard-fail");
    assert!(err.0.contains("injectivity"), "{err}");
}

/// The codebook's ONE lawful cross-plane codepoint reuse: `→` denotes material
/// implication under the `@ℒ` sigil and the morphism arrow under `@μ`. The two forms are
/// distinct, so their surfaces must be distinct — the sigil the surface carries is what
/// makes the corpus injective in substance, not a narrowed check.
#[test]
fn one_glyph_under_two_sigils_is_lawful_and_surfaces_stay_distinct() {
    let forms = vec![
        scoped("logic:consequent", "implies", "→", FIXITY_INFIX, 2, "@ℒ"),
        scoped("math:Morphism", "maps to", "→", FIXITY_INFIX, 2, "@μ"),
    ];
    let pairs = build_verbalization_pairs(&forms).expect("cross-scope reuse is lawful");
    assert_eq!(pairs[0].gmn_surface, "@ℒ arg1 → arg2");
    assert_eq!(pairs[1].gmn_surface, "@μ arg1 → arg2");
    assert_ne!(pairs[0].gmn_surface, pairs[1].gmn_surface);
    assert!(round_trip_holds(&pairs), "both round-trip");
}

/// Dropping the sigil is what made the corpus ambiguous: with the scope erased the SAME
/// two lawful forms collapse onto one GMN surface. This pins the defect the scope carry
/// fixes — if `scoped_surface` ever stops carrying the sigil, this reds.
#[test]
fn erasing_the_sigil_collapses_the_two_readings_onto_one_surface() {
    let bare = arrange(FIXITY_INFIX, "→", 2).expect("arrange");
    assert_eq!(scoped_surface("", &bare), bare);
    assert_eq!(scoped_surface("@ℒ", &bare), "@ℒ arg1 → arg2");
    // Both planes' bare skeletons are identical — the ambiguity, stated.
    assert_eq!(scoped_surface("", &bare), scoped_surface("", &bare));
}

/// One glyph twice INSIDE a single sigil scope is still a HARD FAIL — the scope carry
/// distinguishes planes, it never licenses a collision within one plane.
#[test]
fn one_glyph_twice_in_one_sigil_scope_hard_fails() {
    let forms = vec![
        scoped("logic:consequent", "implies", "→", FIXITY_INFIX, 2, "@ℒ"),
        scoped("logic:entails", "entails", "→", FIXITY_INFIX, 2, "@ℒ"),
    ];
    let err = build_verbalization_pairs(&forms).expect_err("must hard-fail");
    assert!(err.0.contains("injectivity"), "{err}");
    assert!(err.0.contains("GMN operator surface"), "{err}");
}

/// Distinct forms round-trip and the corpus is injective + deterministic.
#[test]
fn distinct_forms_round_trip_and_are_injective() {
    let forms = vec![
        form("logic:not", "not", "¬", FIXITY_PREFIX, 1),
        form("logic:subClassOf", "subsumes", "⊑", FIXITY_INFIX, 2),
        form("math:Addition", "plus", "+", FIXITY_INFIX, 2),
    ];
    let a = build_verbalization_pairs(&forms).expect("a");
    let b = build_verbalization_pairs(&forms).expect("b");
    assert_eq!(a, b, "deterministic");
    assert!(round_trip_holds(&a), "all round-trip");
    // No CURIE tags needed (labels distinct).
    assert!(a.iter().all(|p| !p.nl.contains("⟪")));
}
