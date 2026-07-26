// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic, color-coded SVG status badges for the documentation model.
//!
//! The SINGLE authority for the badge category→color map and the badge SVG
//! shape. Both the per-term page ([`crate::slug`], which embeds a badge row)
//! and the documentation-health legend (`render::md_health`) read their
//! colors from here, so a badge and its legend swatch can never disagree.
//!
//! Like [`crate::svg`], every function is a pure, byte-reproducible function of
//! its inputs — no JavaScript, no randomness, no hashing. A badge's geometry is
//! derived from its label length; every label is XML-escaped; each badge carries
//! `role="img"` + an `aria-label` for assistive technology. The fills are chosen
//! for WCAG-AA contrast against the badge text at the rendered size (dark text on
//! the light amber, white text on the saturated fills).
//!
//! Badges are emitted ONCE per distinct `(family, value)` as shared site assets
//! under `badges/<family>/<value>.svg` and referenced by many term pages, so the
//! per-page cost is a single `<img>` reference, not an inlined SVG. The set a
//! page references and the set [`site_badge_assets`] emits are both derived from
//! [`term_badges`], so the two never disagree (no dangling image path).

use std::collections::BTreeMap;

use crate::coverage::{TermCoverage, term_coverage};
use crate::model::{DocTerm, DocTermCategory, DocTermStability, DocsModel, ReasoningVerdict};

/// A resolved badge: its display label, the colors, and the stable family/value
/// slugs from which its shared asset path is derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    /// The badge family slug (URL-safe, stable): e.g. `"framework"`, `"box"`.
    pub family: &'static str,
    /// The per-value slug (URL-safe, stable): e.g. `"holonic"`, `"tbox"`.
    pub value: String,
    /// The short human label rendered inside the badge: e.g. `"Holonic"`.
    pub label: String,
    /// The badge fill (background) color, `#rrggbb`.
    pub fill: &'static str,
    /// The badge text color, `#rrggbb` (paired with `fill` for AA contrast).
    pub text: &'static str,
}

/// White badge text, for the saturated fills.
const WHITE: &str = "#ffffff";
/// Dark badge text (the svg.rs ink), for the light amber fill.
const INK: &str = "#1b2436";

// ── Family base colors (the single authority) ────────────────────────────────
// The saturated fills below all pair with WHITE text and so must clear the
// WCAG-AA 4.5:1 contrast floor at the 11px-bold rendered size; the red, green,
// teal, and grey are chosen on that floor. The amber pairs with INK (dark) text
// and is light by design.
const C_COMPLETE_LOW: &str = "#cf222e"; // red  — 0–2 of 6 dimensions
const C_COMPLETE_MID: &str = "#d9a514"; // amber — 3–4 of 6
const C_COMPLETE_HIGH: &str = "#1a7f37"; // green — 5–6 of 6
const C_STABILITY_STABLE: &str = "#1a7f37"; // green
const C_STABILITY_EXPERIMENTAL: &str = "#d9a514"; // amber
const C_STABILITY_DEPRECATED: &str = "#cf222e"; // red
const C_CATEGORY: &str = "#5b6b8c"; // slate
const C_BOX: &str = "#1b7369"; // teal
const C_STEREOTYPE: &str = "#8250df"; // purple
const C_FRAMEWORK: &str = "#4b56b8"; // indigo
const C_REASON_SAT: &str = "#1a7f37"; // green — satisfiable
const C_REASON_UNSAT: &str = "#cf222e"; // red — unsatisfiable
const C_REASON_NA: &str = "#6b7280"; // grey — not evaluated

/// One badge family for the health-page legend: its label, swatch fill, and a
/// one-line description of what the family encodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadgeFamily {
    /// The family display label, e.g. `"Completeness"`.
    pub label: &'static str,
    /// A representative swatch fill `#rrggbb` (the mid value for graded families).
    pub swatch: &'static str,
    /// A one-line description of what a badge in this family means.
    pub description: &'static str,
}

/// The badge families in stable display order — the source the health-page
/// legend renders from, so the documented colors are exactly the emitted ones.
pub const FAMILIES: [BadgeFamily; 7] = [
    BadgeFamily {
        label: "Completeness",
        swatch: C_COMPLETE_HIGH,
        description: "How many of the six documentation dimensions the term carries (red 0–2, amber 3–4, green 5–6).",
    },
    BadgeFamily {
        label: "Reasoning",
        swatch: C_REASON_SAT,
        description: "The native reasoner's per-class verdict: green satisfiable, red unsatisfiable, grey not-evaluated (non-class terms).",
    },
    BadgeFamily {
        label: "Stability",
        swatch: C_STABILITY_STABLE,
        description: "The term's maturity: green stable, amber experimental, red deprecated.",
    },
    BadgeFamily {
        label: "Category",
        swatch: C_CATEGORY,
        description: "The vocabulary category: class, property, individual, or datatype.",
    },
    BadgeFamily {
        label: "Box role",
        swatch: C_BOX,
        description: "The four-boxes graph role(s) the term declares (TBox, ABox, RBox, …).",
    },
    BadgeFamily {
        label: "Logic stereotype",
        swatch: C_STEREOTYPE,
        description: "The lowered OntoUML/UFO stereotype(s): Kind, Role, Relator, …",
    },
    BadgeFamily {
        label: "Framework",
        swatch: C_FRAMEWORK,
        description: "The logical framework(s) the term traffics in: holonic, deontic, modal, …",
    },
];

/// The shared-asset path for a badge: `badges/<family>/<value>.svg`.
pub fn badge_path(badge: &Badge) -> String {
    format!("badges/{}/{}.svg", badge.family, badge.value)
}

/// The fill for a coverage fraction `num/den` on the shared red/amber/green scale
/// — the SINGLE authority the health-page coverage heatmap and the completeness
/// badge both read, so a heatmap cell and a badge of the same coverage agree.
/// `< 50%` red, `< 80%` amber, else green; an empty denominator is red.
pub fn coverage_fraction_color(num: usize, den: usize) -> &'static str {
    if den == 0 {
        return C_COMPLETE_LOW;
    }
    match num * 100 / den {
        p if p < 50 => C_COMPLETE_LOW,
        p if p < 80 => C_COMPLETE_MID,
        _ => C_COMPLETE_HIGH,
    }
}

/// The text color paired with a coverage/badge `fill` for WCAG-AA contrast: dark
/// [`INK`] on the light amber mid, white on the saturated fills. The SINGLE
/// authority both the badges and the coverage heatmap read, so a heatmap cell and
/// a badge of the same fill never disagree on legibility.
pub fn text_color_for(fill: &str) -> &'static str {
    if fill == C_COMPLETE_MID { INK } else { WHITE }
}

/// The completeness badge for a coverage score (0..=6).
fn completeness_badge(present: usize, total: usize) -> Badge {
    let fill = if present <= 2 {
        C_COMPLETE_LOW
    } else if present <= 4 {
        C_COMPLETE_MID
    } else {
        C_COMPLETE_HIGH
    };
    let text = text_color_for(fill);
    Badge {
        family: "completeness",
        value: present.to_string(),
        label: format!("Docs {present}/{total}"),
        fill,
        text,
    }
}

/// The stability badge.
fn stability_badge(stability: DocTermStability) -> Badge {
    let (value, label, fill) = match stability {
        DocTermStability::Stable => ("stable", "stable", C_STABILITY_STABLE),
        DocTermStability::Experimental => {
            ("experimental", "experimental", C_STABILITY_EXPERIMENTAL)
        }
        DocTermStability::Deprecated => ("deprecated", "deprecated", C_STABILITY_DEPRECATED),
    };
    let text = text_color_for(fill);
    Badge {
        family: "stability",
        value: value.to_string(),
        label: label.to_string(),
        fill,
        text,
    }
}

/// The native-reasoner status badge — a three-state verdict that never collapses
/// not-evaluated into satisfiable.
///
/// Satisfiability is a CLASS notion, so a documented class is *evaluated*:
/// unsatisfiable when its IRI is in the verdict's unsat set (the reasoner entailed
/// `rdfs:subClassOf owl:Nothing`), satisfiable otherwise. A property, individual,
/// or datatype is *not-evaluated* — the reasoner decides no satisfiability for it,
/// and the badge says so rather than implying it is fine.
fn reasoning_badge(term: &DocTerm, verdict: &ReasoningVerdict) -> Badge {
    let (value, label, fill) = match term.category {
        DocTermCategory::Class => {
            if verdict.unsatisfiable.contains(&term.iri) {
                ("unsatisfiable", "unsatisfiable", C_REASON_UNSAT)
            } else {
                ("satisfiable", "satisfiable", C_REASON_SAT)
            }
        }
        _ => ("not-evaluated", "not reasoned", C_REASON_NA),
    };
    Badge {
        family: "reasoning",
        value: value.to_string(),
        label: label.to_string(),
        fill,
        text: WHITE,
    }
}

/// The vocabulary-category badge.
fn category_badge(category: DocTermCategory) -> Badge {
    let (value, label) = match category {
        DocTermCategory::Class => ("class", "Class"),
        DocTermCategory::Property => ("property", "Property"),
        DocTermCategory::Individual => ("individual", "Individual"),
        DocTermCategory::Datatype => ("datatype", "Datatype"),
        DocTermCategory::Other => ("other", "Term"),
    };
    Badge {
        family: "category",
        value: value.to_string(),
        label: label.to_string(),
        fill: C_CATEGORY,
        text: WHITE,
    }
}

/// The short local name of a CURIE or IRI: the tail after the last `:`, `/`, `#`.
fn local_name(curie: &str) -> &str {
    let cut = curie.rfind([':', '/', '#']).map(|i| i + 1).unwrap_or(0);
    &curie[cut..]
}

/// A lower-case, URL-safe slug of a CURIE's local name (ASCII alphanumerics kept,
/// everything else dropped) — deterministic and collision-free across the small,
/// fixed vocabulary of box roles, stereotypes, and frameworks.
fn slug(curie: &str) -> String {
    local_name(curie)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The four-boxes label for a `gmeow:box*` role CURIE (`gmeow:boxTBox` → `TBox`);
/// the local name unchanged when it does not match the expected shape.
fn box_label(role: &str) -> String {
    local_name(role)
        .strip_prefix("box")
        .filter(|rest| !rest.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| local_name(role).to_string())
}

/// The framework label for a `logic:*Framework` individual CURIE
/// (`logic:HolonicFramework` → `Holonic`); the local name unchanged otherwise.
fn framework_label(framework: &str) -> String {
    local_name(framework)
        .strip_suffix("Framework")
        .filter(|rest| !rest.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| local_name(framework).to_string())
}

/// Every badge a term carries, in stable display order: completeness, the
/// reasoning verdict (when one is attached), stability, category, then each box
/// role, logic stereotype, and framework. The single source both the page embed
/// and the [`site_badge_assets`] emission read, so the referenced and emitted
/// asset sets are identical.
///
/// `reasoning` is the model's attached [`ReasoningVerdict`] (`None` in source-only
/// contexts): the reasoning badge renders ONLY when a verdict is present, so an
/// unevaluated model never fabricates a satisfiability claim.
pub fn term_badges(
    term: &DocTerm,
    ctx: &crate::coverage::CoverageContext,
    reasoning: Option<&ReasoningVerdict>,
) -> Vec<Badge> {
    let cov: TermCoverage = term_coverage(term, ctx);
    let mut badges = vec![completeness_badge(cov.present_count(), TermCoverage::TOTAL)];
    if let Some(verdict) = reasoning {
        badges.push(reasoning_badge(term, verdict));
    }
    badges.push(stability_badge(term.stability));
    badges.push(category_badge(term.category));
    for role in &term.box_roles {
        badges.push(Badge {
            family: "box",
            value: slug(role),
            label: box_label(role),
            fill: C_BOX,
            text: WHITE,
        });
    }
    for stereotype in &term.logic_stereotypes {
        badges.push(Badge {
            family: "stereotype",
            value: slug(stereotype),
            label: local_name(stereotype).to_string(),
            fill: C_STEREOTYPE,
            text: WHITE,
        });
    }
    for framework in &term.frameworks {
        badges.push(Badge {
            family: "framework",
            value: slug(framework),
            label: framework_label(framework),
            fill: C_FRAMEWORK,
            text: WHITE,
        });
    }
    badges
}

/// XML-escape a string for safe inclusion in SVG text / attribute content.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// Render a single badge as a deterministic, self-contained SVG.
///
/// A rounded rectangle whose width is derived from the label length (so the text
/// always fits), the family-coded fill, and centered label text. Pure: identical
/// bytes for identical input.
pub fn badge_svg(badge: &Badge) -> String {
    const HEIGHT: i64 = 20;
    const PAD: i64 = 8;
    const CHAR_W: i64 = 7; // conservative advance for the 11px bold sans label
    let label = xml_escape(&badge.label);
    let width = PAD * 2 + CHAR_W * badge.label.chars().count() as i64;
    let aria = xml_escape(&format!("{}: {}", badge.family, badge.label));
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{HEIGHT}\" \
         viewBox=\"0 0 {width} {HEIGHT}\" role=\"img\" aria-label=\"{aria}\">\n  \
         <title>{aria}</title>\n  \
         <rect width=\"{width}\" height=\"{HEIGHT}\" rx=\"4\" fill=\"{fill}\" />\n  \
         <text x=\"{cx}\" y=\"14\" text-anchor=\"middle\" font-family=\"sans-serif\" \
         font-size=\"11\" font-weight=\"600\" fill=\"{text}\">{label}</text>\n</svg>\n",
        fill = badge.fill,
        text = badge.text,
        cx = width / 2,
    )
}

/// Every distinct badge SVG the model references, keyed by its shared-asset path.
///
/// Walks every term's [`term_badges`] and dedups by path, so the emitted set is
/// exactly the set the term pages reference — the no-dangling-image guarantee.
pub fn site_badge_assets(model: &DocsModel) -> BTreeMap<String, String> {
    let ctx = crate::coverage::CoverageContext::new(model);
    let mut assets = BTreeMap::new();
    for term in &model.terms {
        for badge in term_badges(term, &ctx, model.reasoning.as_ref()) {
            assets
                .entry(badge_path(&badge))
                .or_insert_with(|| badge_svg(&badge));
        }
    }
    assets
}

#[cfg(test)]
mod tests {
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
}
