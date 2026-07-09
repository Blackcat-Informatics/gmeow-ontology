// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Self-hosting RDF projection of the documentation model (PyO3-free).
//!
//! [`to_gmeow_rdf`] dogfoods the doc model: it projects [`DocsModel`] into the
//! `gmeow:` vocabulary as deterministic N-Quads, all in the
//! `gmeow:graph/documentation` named graph, so the documentation surface is
//! itself SPARQL-queryable RDF folded into the offline `gmeow.gts` bundle beside
//! the ontology it describes (Principle 4). This mirrors the discipline of
//! `gmeow-errors`'s `to_gmeow_rdf`: N-Quads (no TriG/prefix handling),
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
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const GMEOW_DEFINITION_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/definitionDigest";
const GMEOW_ADDED_IN_VERSION: &str = "https://blackcatinformatics.ca/gmeow/addedInVersion";
const GMEOW_HAS_CHANGELOG_ENTRY: &str = "https://blackcatinformatics.ca/gmeow/hasChangelogEntry";
const GMEOW_ENTRY_VERSION: &str = "https://blackcatinformatics.ca/gmeow/entryVersion";
const GMEOW_ENTRY_NOTE: &str = "https://blackcatinformatics.ca/gmeow/entryNote";
const GMEOW_CHANGELOG_ENTRY: &str = "https://blackcatinformatics.ca/gmeow/ChangelogEntry";

/// Project the documentation model into the `gmeow:` RDF vocabulary as N-Quads,
/// all in the `gmeow:graph/documentation` named graph.
///
/// Vocabulary (every resource is an IRI under the documentation namespace):
/// - each term → `gmeow:documentation/term/{slug}` `a gmeow:DocumentedTerm`,
///   with `gmeow:documents <term-iri>`, `gmeow:docCategory "Class|…"`,
///   `gmeow:docHasDefinition "true|false"^^xsd:boolean`, `gmeow:docUrl
///   "terms/{slug}/index.html"`, and `gmeow:docOwnerSlice <slice-iri>`.
/// - each term's REAL IRI → its content-address provenance (from the term content
///   manifest): `gmeow:definitionDigest "blake3:…"`, `gmeow:addedInVersion "…"`,
///   and one `gmeow:hasChangelogEntry <changelog-iri>` per release — each entry a
///   deterministically-minted `gmeow:documentation/changelog/{term}/{version}` `a
///   gmeow:ChangelogEntry` with `gmeow:entryVersion` and optional `gmeow:entryNote`.
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

    // Every projected subject is generated A-Box instance data, not vocabulary
    // surface: tag it with a human label, a derived definition-equivalent, its
    // named-graph provenance anchor, and the assertional `gmeow:boxABox` role so
    // the folded bundle satisfies the assertional-tier validation contract while
    // staying genuinely self-describing (the validator requires the box role and
    // a provenance link on every materialized individual).
    let role_object = format!("<{GMEOW}boxABox>");
    let isdefinedby_object = format!("<{DOCUMENTATION_GRAPH}>");
    let annotate = |subject: &str, label: &str, definition: &str, lines: &mut Vec<String>| {
        triple(subject, RDFS_LABEL, &literal(label), lines);
        triple(subject, SKOS_DEFINITION, &literal(definition), lines);
        triple(subject, RDFS_IS_DEFINED_BY, &isdefinedby_object, lines);
        triple(
            subject,
            &format!("{GMEOW}graphBoxRole"),
            &role_object,
            lines,
        );
    };

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
        // Per-term native-reasoner status — projected ONLY when a verdict is
        // attached (the production docs-graph stage consumes stage-reason), so the
        // SPARQL surface never carries a fabricated satisfiability claim. A class
        // is satisfiable unless proven unsatisfiable; a non-class is not-evaluated.
        if let Some(verdict) = &model.reasoning {
            let status = if term.category == crate::model::DocTermCategory::Class {
                if verdict.unsatisfiable.contains(&term.iri) {
                    "unsatisfiable"
                } else {
                    "satisfiable"
                }
            } else {
                "not-evaluated"
            };
            triple(
                &subject,
                &format!("{GMEOW}docReasoningStatus"),
                &literal(status),
                &mut lines,
            );
        }
        annotate(
            &subject,
            &format!("Documentation entry: {}", term.curie),
            &format!(
                "Documentation projection for {} ({}).",
                term.iri,
                category_name(term.category)
            ),
            &mut lines,
        );

        // Per-term content-address provenance, projected on the REAL term IRI (not
        // the documentation-entry subject): the content digest, first-seen version,
        // and a reified changelog entry per release. Blank nodes are forbidden here
        // (the graph round-trips through GTS as IRIs), so each changelog entry is a
        // deterministically-minted IRI under the documentation namespace.
        let real = format!("<{}>", term.iri);
        if !term.content_digest.is_empty() {
            triple(
                &real,
                GMEOW_DEFINITION_DIGEST,
                &literal(&term.content_digest),
                &mut lines,
            );
        }
        if let Some(version) = &term.added_in_version {
            triple(&real, GMEOW_ADDED_IN_VERSION, &literal(version), &mut lines);
        }
        for entry in &term.changelog {
            let entry_iri = format!(
                "<{GMEOW}documentation/changelog/{}/{}>",
                term_slug(term),
                set_slug(&entry.version)
            );
            triple(&real, GMEOW_HAS_CHANGELOG_ENTRY, &entry_iri, &mut lines);
            triple(
                &entry_iri,
                RDF_TYPE,
                &format!("<{GMEOW_CHANGELOG_ENTRY}>"),
                &mut lines,
            );
            // A changelog entry is generated A-Box instance data like every other doc
            // subject: annotate it (label / definition / provenance anchor / gmeow:boxABox
            // role) so the folded bundle satisfies the assertional-tier validation contract
            // — otherwise every minted changelog entry trips the four structural annotation
            // lints. The note (when present) IS the entry's definition-equivalent.
            annotate(
                &entry_iri,
                &format!("Changelog entry: {} {}", term.curie, entry.version),
                &entry.note.clone().unwrap_or_else(|| {
                    format!(
                        "Changelog entry recording the {} release of {}.",
                        entry.version, term.iri
                    )
                }),
                &mut lines,
            );
            triple(
                &entry_iri,
                GMEOW_ENTRY_VERSION,
                &literal(&entry.version),
                &mut lines,
            );
            if let Some(note) = &entry.note {
                triple(&entry_iri, GMEOW_ENTRY_NOTE, &literal(note), &mut lines);
            }
        }
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
        annotate(
            &subject,
            &format!("Documentation entry: slice {slug}"),
            &format!("Documentation projection for slice {}.", slice.iri),
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
        annotate(
            &subject,
            &format!("Documentation entry: concern {slug}"),
            &format!("Documentation projection for concern {}.", concern.iri),
            &mut lines,
        );
    }

    // Mapping sets (model.mapping_sets is IRI-sorted). All link to the single
    // linkages index page.
    for set in &model.mapping_sets {
        let slug = set_slug(&set.iri);
        let subject = format!("<{GMEOW}documentation/mapping-set/{slug}>");
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
        annotate(
            &subject,
            &format!("Documentation entry: mapping set {slug}"),
            &format!("Documentation projection for mapping set {}.", set.iri),
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
/// `gmeow_errors::render::nq_escape`).
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
            // rdflib/oxigraph reject the graph (mirrors diagnostics).
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
                    scope_notes: Vec::new(),
                    examples: Vec::new(),
                    use_when: Vec::new(),
                    avoid_when: Vec::new(),
                    how_to_use: Vec::new(),
                    use_for_consumer: Vec::new(),
                    avoid_for_consumer: Vec::new(),
                    ..Default::default()
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
                    scope_notes: Vec::new(),
                    examples: Vec::new(),
                    use_when: Vec::new(),
                    avoid_when: Vec::new(),
                    how_to_use: Vec::new(),
                    use_for_consumer: Vec::new(),
                    avoid_for_consumer: Vec::new(),
                    ..Default::default()
                },
            ],
            dependency_edges: Vec::new(),
            mapping_sets: Vec::new(),
            linkages: Vec::new(),
            examples: Vec::new(),
            fixtures: Vec::new(),
            shapes: Vec::new(),
            competencies: Vec::new(),
            grammars: Vec::new(),
            loss_targets: Vec::new(),
            worked_instances: Vec::new(),
            concerns: Vec::new(),
            external_terms: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            constraint_rules: Vec::new(),
            four_boxes: None,
            concept_doi: None,

            available_languages: vec!["english".to_string()],

            translations: crate::i18n::Translations::default(),

            ui_catalog: crate::i18n::UiCatalog::default(),
            reasoning: None,
            diagnostics: None,
            lang: String::new(),
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
            fixtures: Vec::new(),
            shapes: Vec::new(),
            competencies: Vec::new(),
            grammars: Vec::new(),
            loss_targets: Vec::new(),
            worked_instances: Vec::new(),
            concerns: Vec::new(),
            external_terms: Vec::new(),
            recipes: Vec::new(),
            learning_paths: Vec::new(),
            constraint_rules: Vec::new(),
            four_boxes: None,
            concept_doi: None,

            available_languages: vec!["english".to_string()],

            translations: crate::i18n::Translations::default(),

            ui_catalog: crate::i18n::UiCatalog::default(),
            reasoning: None,
            diagnostics: None,
            lang: String::new(),
        };
        assert_eq!(to_gmeow_rdf(&model), "");
    }
}
