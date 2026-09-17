// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn badge_svg_is_pure_and_escaped() {
    let badge = Badge {
        family: "framework",
        value: "holonic".to_string(),
        label: "Holonic".to_string(),
        fill: C_FRAMEWORK,
        text: WHITE,
    };
    let svg = badge_svg(&badge);
    assert_eq!(svg, badge_svg(&badge)); // byte-identical across calls
    assert!(svg.contains(">Holonic</text>"));
    assert!(svg.contains("role=\"img\""));
    assert!(svg.contains("aria-label=\"framework: Holonic\""));
}

#[test]
fn labels_derive_from_curies() {
    assert_eq!(box_label("gmeow:boxTBox"), "TBox");
    assert_eq!(framework_label("logic:HolonicFramework"), "Holonic");
    assert_eq!(slug("logic:HolonicFramework"), "holonicframework");
    assert_eq!(local_name("logic:Kind"), "Kind");
}

#[test]
fn completeness_grades_by_score() {
    assert_eq!(completeness_badge(1, 6).fill, C_COMPLETE_LOW);
    assert_eq!(completeness_badge(3, 6).fill, C_COMPLETE_MID);
    assert_eq!(completeness_badge(6, 6).fill, C_COMPLETE_HIGH);
    // The amber mid uses dark ink for AA contrast; the saturated fills use white.
    assert_eq!(completeness_badge(3, 6).text, INK);
    assert_eq!(completeness_badge(6, 6).text, WHITE);
}

#[test]
fn term_badges_lead_with_completeness_stability_category() {
    let mut term = DocTerm {
        iri: "https://x/Foo".to_string(),
        category: DocTermCategory::Class,
        ..Default::default()
    };
    term.frameworks = vec!["logic:HolonicFramework".to_string()];
    let model = crate::model::DocsModel::default();
    let ctx = crate::coverage::CoverageContext::new(&model);
    let badges = term_badges(&term, &ctx, None);
    assert_eq!(badges[0].family, "completeness");
    // No reasoning verdict attached → no reasoning badge (never fabricated).
    assert!(!badges.iter().any(|b| b.family == "reasoning"));
    assert_eq!(badges[1].family, "stability");
    assert_eq!(badges[2].family, "category");
    assert!(
        badges
            .iter()
            .any(|b| b.family == "framework" && b.label == "Holonic")
    );
}

#[test]
fn reasoning_badge_is_three_state_and_never_collapses() {
    let class = DocTerm {
        iri: "https://x/Klass".to_string(),
        category: DocTermCategory::Class,
        ..Default::default()
    };
    let property = DocTerm {
        iri: "https://x/prop".to_string(),
        category: DocTermCategory::Property,
        ..Default::default()
    };
    let mut unsat = std::collections::BTreeSet::new();
    unsat.insert("https://x/Klass".to_string());
    let bad = ReasoningVerdict {
        is_consistent: false,
        unsatisfiable: unsat,
    };
    let good = ReasoningVerdict::default();

    // An evaluated class flips satisfiable → unsatisfiable on the unsat set.
    assert_eq!(reasoning_badge(&class, &good).value, "satisfiable");
    assert_eq!(reasoning_badge(&class, &bad).value, "unsatisfiable");
    // A non-class is not-evaluated — never silently "satisfiable".
    assert_eq!(reasoning_badge(&property, &good).value, "not-evaluated");

    // The badge only appears when a verdict is attached.
    let model = crate::model::DocsModel::default();
    let ctx = crate::coverage::CoverageContext::new(&model);
    assert!(
        term_badges(&class, &ctx, None)
            .iter()
            .all(|b| b.family != "reasoning")
    );
    assert_eq!(
        term_badges(&class, &ctx, Some(&good))
            .iter()
            .filter(|b| b.family == "reasoning")
            .count(),
        1
    );
}
