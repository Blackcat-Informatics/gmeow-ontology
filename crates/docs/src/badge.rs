// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic, color-coded SVG status badges for the documentation model.
//!
//! The SINGLE authority for the badge category→color map and the badge SVG
//! shape. Both the per-term page ([`crate::render`], which embeds a badge row)
//! and the documentation-health legend ([`crate::render::md_health`]) read their
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

use std::collections::{BTreeMap, HashSet};

use crate::coverage::{term_coverage, TermCoverage};
use crate::model::{DocTerm, DocTermCategory, DocTermStability, DocsModel};

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
const C_COMPLETE_LOW: &str = "#e5534b"; // red  — 0–2 of 6 dimensions
const C_COMPLETE_MID: &str = "#d9a514"; // amber — 3–4 of 6
const C_COMPLETE_HIGH: &str = "#2da44e"; // green — 5–6 of 6
const C_STABILITY_STABLE: &str = "#2da44e"; // green
const C_STABILITY_EXPERIMENTAL: &str = "#d9a514"; // amber
const C_STABILITY_DEPRECATED: &str = "#e5534b"; // red
const C_CATEGORY: &str = "#5b6b8c"; // slate
const C_BOX: &str = "#2a9d8f"; // teal
const C_STEREOTYPE: &str = "#8250df"; // purple
const C_FRAMEWORK: &str = "#4b56b8"; // indigo

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
pub const FAMILIES: [BadgeFamily; 6] = [
    BadgeFamily {
        label: "Completeness",
        swatch: C_COMPLETE_HIGH,
        description: "How many of the six documentation dimensions the term carries (red 0–2, amber 3–4, green 5–6).",
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

/// The completeness badge for a coverage score (0..=6).
fn completeness_badge(present: usize, total: usize) -> Badge {
    let fill = if present <= 2 {
        C_COMPLETE_LOW
    } else if present <= 4 {
        C_COMPLETE_MID
    } else {
        C_COMPLETE_HIGH
    };
    let text = if fill == C_COMPLETE_MID { INK } else { WHITE };
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
    let text = if fill == C_STABILITY_EXPERIMENTAL {
        INK
    } else {
        WHITE
    };
    Badge {
        family: "stability",
        value: value.to_string(),
        label: label.to_string(),
        fill,
        text,
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

/// Every badge a term carries, in stable display order: completeness, stability,
/// category, then each box role, logic stereotype, and framework. The single
/// source both the page embed and the [`site_badge_assets`] emission read, so the
/// referenced and emitted asset sets are identical.
pub fn term_badges(term: &DocTerm, aligned: &HashSet<&str>) -> Vec<Badge> {
    let cov: TermCoverage = term_coverage(term, aligned);
    let mut badges = vec![
        completeness_badge(cov.present_count(), TermCoverage::TOTAL),
        stability_badge(term.stability),
        category_badge(term.category),
    ];
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
    let aligned = crate::coverage::alignment_subjects(model);
    let mut assets = BTreeMap::new();
    for term in &model.terms {
        for badge in term_badges(term, &aligned) {
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
        let aligned = HashSet::new();
        let badges = term_badges(&term, &aligned);
        assert_eq!(badges[0].family, "completeness");
        assert_eq!(badges[1].family, "stability");
        assert_eq!(badges[2].family, "category");
        assert!(badges
            .iter()
            .any(|b| b.family == "framework" && b.label == "Holonic"));
    }
}
