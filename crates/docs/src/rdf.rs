// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Self-hosting RDF projection of the documentation model (PyO3-free).
//!
//! [`to_gmeow_rdf`] dogfoods the doc model: it projects [`DocsModel`] into the
//! `gmeow:` vocabulary as deterministic N-Quads, all in the
//! `gmeow:graph/documentation` named graph, so the documentation surface is
//! itself SPARQL-queryable RDF folded into the offline `gmeow.gts` bundle beside
//! the ontology it describes (Principle 4). This mirrors the discipline of
//! `gmeow-diagnostics`'s `to_gmeow_rdf`: N-Quads (no TriG/prefix handling),
//! `nq_escape`d literals, IRIs (never blank nodes) so the graph round-trips
//! through GTS fold without bnode relabeling, sorted iteration over the
//! already-sorted model collections, and a trailing newline.

use crate::model::DocsModel;
use crate::render::{concern_slug, slice_slug, term_slug};

/// The GMEOW namespace IRI prefix.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The named graph the documentation projection lives in.
const DOCUMENTATION_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/documentation";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// Project the documentation model into the `gmeow:` RDF vocabulary as N-Quads,
/// all in the `gmeow:graph/documentation` named graph.
///
/// Vocabulary (every resource is an IRI under the documentation namespace):
/// - each term → `gmeow:documentation/term/{slug}` `a gmeow:DocumentedTerm`,
///   with `gmeow:documents <term-iri>`, `gmeow:docCategory "Class|…"`,
///   `gmeow:docHasDefinition "true|false"^^xsd:boolean`, `gmeow:docUrl
///   "terms/{slug}/index.html"`, and `gmeow:docOwnerSlice <slice-iri>`.
/// - each slice → `gmeow:documentation/slice/{slug}` `a gmeow:DocumentedSlice`,
///   `gmeow:documents <slice-iri>`, `gmeow:docUrl "slices/{slug}/index.html"`.
/// - each concern → `gmeow:documentation/concern/{slug}` `a
///   gmeow:DocumentedConcern`, `gmeow:documents <concern-iri>`, `gmeow:docUrl
///   "concerns/{slug}/index.html"`.
/// - each mapping set → `gmeow:documentation/mapping-set/{n}` `a
///   gmeow:DocumentedMappingSet`, `gmeow:documents <set-iri>`, `gmeow:docUrl
///   "linkages/index.html"`.
///
/// Output is deterministic: the model collections are already sorted by IRI, and
/// every subject's triples are emitted in a fixed order.
pub fn to_gmeow_rdf(model: &DocsModel) -> String {
    let graph = format!("<{DOCUMENTATION_GRAPH}>");
    let mut lines: Vec<String> = Vec::new();

    let triple = |s: &str, p: &str, o: &str, lines: &mut Vec<String>| {
        lines.push(format!("{s} <{p}> {o} {graph} ."));
    };
    let literal = |value: &str| format!("\"{}\"", nq_escape(value));

    // Terms (model.terms is IRI-sorted).
    for term in &model.terms {
        let slug = term_slug(term);
        let subject = format!("<{GMEOW}documentation/term/{slug}>");
        triple(
            &subject,
            RDF_TYPE,
            &format!("<{GMEOW}DocumentedTerm>"),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}documents"),
            &format!("<{}>", term.iri),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docCategory"),
            &literal(category_name(term.category)),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docHasDefinition"),
            &boolean(term.definition.is_some()),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docUrl"),
            &literal(&format!("terms/{slug}/index.html")),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docOwnerSlice"),
            &format!("<{}>", term.owner_slice),
            &mut lines,
        );
    }

    // Slices (model.slices is IRI-sorted).
    for slice in &model.slices {
        let slug = slice_slug(slice);
        let subject = format!("<{GMEOW}documentation/slice/{slug}>");
        triple(
            &subject,
            RDF_TYPE,
            &format!("<{GMEOW}DocumentedSlice>"),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}documents"),
            &format!("<{}>", slice.iri),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docUrl"),
            &literal(&format!("slices/{slug}/index.html")),
            &mut lines,
        );
    }

    // Concerns (model.concerns is IRI-sorted).
    for concern in &model.concerns {
        let slug = concern_slug(concern);
        let subject = format!("<{GMEOW}documentation/concern/{slug}>");
        triple(
            &subject,
            RDF_TYPE,
            &format!("<{GMEOW}DocumentedConcern>"),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}documents"),
            &format!("<{}>", concern.iri),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docUrl"),
            &literal(&format!("concerns/{slug}/index.html")),
            &mut lines,
        );
    }

    // Mapping sets (model.mapping_sets is IRI-sorted). All link to the single
    // linkages index page.
    for set in &model.mapping_sets {
        let subject = format!("<{GMEOW}documentation/mapping-set/{}>", set_slug(&set.iri));
        triple(
            &subject,
            RDF_TYPE,
            &format!("<{GMEOW}DocumentedMappingSet>"),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}documents"),
            &format!("<{}>", set.iri),
            &mut lines,
        );
        triple(
            &subject,
            &format!("{GMEOW}docUrl"),
            &literal("linkages/index.html"),
            &mut lines,
        );
    }

    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// The stable `gmeow:docCategory` label for a term category.
fn category_name(category: crate::model::DocTermCategory) -> &'static str {
    use crate::model::DocTermCategory;
    match category {
        DocTermCategory::Class => "Class",
        DocTermCategory::Property => "Property",
        DocTermCategory::Individual => "Individual",
        DocTermCategory::Datatype => "Datatype",
        DocTermCategory::Other => "Other",
    }
}

/// An `xsd:boolean`-typed N-Quads literal object.
fn boolean(value: bool) -> String {
    format!("\"{value}\"^^<{XSD_BOOLEAN}>")
}

/// A filesystem-safe slug from a mapping-set IRI's local name (tail after the
/// last `/` or `#`, lowercased + reduced to `[a-z0-9-]`).
fn set_slug(iri: &str) -> String {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    let name = &iri[cut..];
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Escape a string literal for N-Triples/N-Quads (mirrors
/// `gmeow_diagnostics::render::nq_escape`).
fn nq_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Any remaining C0 control character must be escaped as \uXXXX, else
            // the literal is illegal raw in an N-Quads STRING_LITERAL_QUOTE and
            // rdflib/oxigraph reject the graph (mirrors diagnostics #654).
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DocTerm, DocTermCategory};

    fn tiny_model() -> DocsModel {
        DocsModel {
            title: "T".to_string(),
            version: "2".to_string(),
            slices: Vec::new(),
            terms: vec![
                DocTerm {
                    iri: format!("{GMEOW}Cat"),
                    curie: "gmeow:Cat".to_string(),
                    label: Some("Cat".to_string()),
                    definition: Some("A cat.".to_string()),
                    category: DocTermCategory::Class,
                    owner_slice: format!("{GMEOW}slice/zoo"),
                    parents: Vec::new(),
                    domain: Vec::new(),
                    range: Vec::new(),
                },
                DocTerm {
                    iri: format!("{GMEOW}hasOwner"),
                    curie: "gmeow:hasOwner".to_string(),
                    label: None,
                    definition: None,
                    category: DocTermCategory::Property,
                    owner_slice: format!("{GMEOW}slice/zoo"),
                    parents: Vec::new(),
                    domain: Vec::new(),
                    range: Vec::new(),
                },
            ],
            dependency_edges: Vec::new(),
            mapping_sets: Vec::new(),
            linkages: Vec::new(),
            examples: Vec::new(),
            concerns: Vec::new(),
            external_terms: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            four_boxes: None,

            available_languages: vec!["english".to_string()],

            translations: crate::i18n::Translations::default(),

            ui_catalog: crate::i18n::UiCatalog::default(),
        }
    }

    #[test]
    fn projection_is_well_formed_and_deterministic() {
        let model = tiny_model();
        let a = to_gmeow_rdf(&model);
        let b = to_gmeow_rdf(&model);
        assert_eq!(a, b, "projection must be deterministic");

        // Every line is a 4-term N-Quad in the documentation graph.
        for line in a.lines() {
            assert!(
                line.ends_with(&format!("<{DOCUMENTATION_GRAPH}> .")),
                "line not in documentation graph: {line}"
            );
        }
        assert!(a.contains("DocumentedTerm"));
        assert!(a.contains("docCategory"));
        // The definition-less property records false; the cat records true.
        assert!(a.contains(&format!("\"true\"^^<{XSD_BOOLEAN}>")));
        assert!(a.contains(&format!("\"false\"^^<{XSD_BOOLEAN}>")));
        assert!(a.ends_with('\n'));
    }

    #[test]
    fn empty_model_yields_empty_string() {
        let model = DocsModel {
            title: "T".to_string(),
            version: "2".to_string(),
            slices: Vec::new(),
            terms: Vec::new(),
            dependency_edges: Vec::new(),
            mapping_sets: Vec::new(),
            linkages: Vec::new(),
            examples: Vec::new(),
            concerns: Vec::new(),
            external_terms: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            four_boxes: None,

            available_languages: vec!["english".to_string()],

            translations: crate::i18n::Translations::default(),

            ui_catalog: crate::i18n::UiCatalog::default(),
        };
        assert_eq!(to_gmeow_rdf(&model), "");
    }
}
