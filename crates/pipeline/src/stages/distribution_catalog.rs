// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The canonical **distribution catalog** — a meta-level named graph declaring WHICH
//! documentation distributions exist, their FAMILY, their CONSUMER class, and (for the
//! doc-render family) their declared capability LOSS.
//!
//! This is a SCHEMA, not a release record: it declares the eight distributions gmeow
//! ships and their static properties. It carries NO per-release digests — a shipped
//! release's concrete `gmeow:contentDigest` is a later release-time instance, never
//! folded into this carrier-time graph (a render-derived digest here would be a
//! non-converging fixpoint: the digest of THIS bundle would have to be known before
//! THIS bundle is serialized).
//!
//! ## The two families
//!
//! * **doc-render** — `site`, `mdbook`, `pdf`, `snippets`. These are EXACTLY the
//!   [`gmeow_docs::formats::DocFormat`] variants, and their declared loss is read from
//!   the SINGLE authority ([`gmeow_docs::formats::format_capabilities`]) — the same
//!   table [`crate::stages::docs_format_rendering`] reads — so the catalog's loss
//!   claims can never drift from the renderer's.
//! * **serialization** — `okf`, `jsonld`, `yamlld`, `pydantic`. These are structured
//!   re-serializations of the ontology, not lossy prose renderings; they carry no
//!   `gmeow:declaredLoss` here (their own loss lattice, if any, is a future
//!   declaration on their own graphs, never fabricated here).
//!
//! Every subject this module emits (distribution nodes, family nodes, loss nodes)
//! carries the proven generated-aBox skeleton (mirroring
//! [`crate::stages::carrier::build_fanout_opaque_manifest`]): `rdf:type` +
//! `rdfs:isDefinedBy <graph/distribution-catalog>` + `gmeow:graphBoxRole
//! gmeow:boxABox` + `rdfs:label`, which the whole-bundle structural lint accepts as an
//! assertional individual without a `skos:definition`.
//!
//! The N-Triples are sorted + deduped and no clock/randomness rides here, so the
//! catalog is byte-reproducible.

use gmeow_docs::formats::{DocFormat, format_capabilities};

use crate::stages::carrier::{GRAPH_DISTRIBUTION_CATALOG, parse_into_graph};

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";

/// The instance subject base every distribution/family/loss IRI this module mints
/// lives under.
const DISTRIBUTION_BASE: &str = "https://blackcatinformatics.ca/gmeow/distribution/";

/// A distribution family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Family {
    DocRender,
    Serialization,
}

impl Family {
    fn slug(&self) -> &'static str {
        match self {
            Family::DocRender => "doc-render",
            Family::Serialization => "serialization",
        }
    }
}

/// A serialization-family distribution (the doc-render family is exactly
/// [`DocFormat::ALL`]; this covers the remaining four).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SerializationDist {
    Okf,
    Jsonld,
    Yamlld,
    Pydantic,
}

impl SerializationDist {
    const ALL: [SerializationDist; 4] = [
        SerializationDist::Okf,
        SerializationDist::Jsonld,
        SerializationDist::Yamlld,
        SerializationDist::Pydantic,
    ];

    fn slug(&self) -> &'static str {
        match self {
            SerializationDist::Okf => "okf",
            SerializationDist::Jsonld => "jsonld",
            SerializationDist::Yamlld => "yamlld",
            SerializationDist::Pydantic => "pydantic",
        }
    }

    fn media_type(&self) -> &'static str {
        match self {
            SerializationDist::Okf => "application/json",
            SerializationDist::Jsonld => "application/ld+json",
            SerializationDist::Yamlld => "application/yaml",
            SerializationDist::Pydantic => "text/x-python",
        }
    }

    fn consumer(&self) -> &'static str {
        match self {
            SerializationDist::Okf => "consumerKnowledgeFederation",
            SerializationDist::Jsonld => "consumerLinkedDataTooling",
            SerializationDist::Yamlld => "consumerLinkedDataTooling",
            SerializationDist::Pydantic => "consumerTypedModelClient",
        }
    }
}

/// The doc-render family's per-format media type.
fn doc_render_media_type(fmt: DocFormat) -> &'static str {
    match fmt {
        DocFormat::Site => "text/html",
        DocFormat::Mdbook => "text/markdown",
        DocFormat::Pdf => "application/pdf",
        DocFormat::Snippets => "text/markdown",
    }
}

/// The doc-render family's per-format consumer.
fn doc_render_consumer(fmt: DocFormat) -> &'static str {
    match fmt {
        DocFormat::Site => "consumerPublicSite",
        DocFormat::Mdbook => "consumerOfflineBook",
        DocFormat::Pdf => "consumerPrintArchive",
        DocFormat::Snippets => "consumerAgentMemory",
    }
}

/// Build the distribution catalog graph: the eight distributions, their two families,
/// and (doc-render only) their declared capability loss, folded into
/// [`GRAPH_DISTRIBUTION_CATALOG`].
pub fn build_distribution_catalog()
-> Result<std::sync::Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    let nt = emit_ntriples();
    parse_into_graph(&nt, "application/n-triples", GRAPH_DISTRIBUTION_CATALOG)
}

// ── identity helpers ────────────────────────────────────────────────────────────────

fn dist_iri(slug: &str) -> String {
    format!("{DISTRIBUTION_BASE}dist/{slug}")
}

fn family_iri(family: Family) -> String {
    format!("{DISTRIBUTION_BASE}family/{}", family.slug())
}

fn loss_iri(slug: &str, cap_slug: &str) -> String {
    format!("{DISTRIBUTION_BASE}loss/{slug}/{cap_slug}")
}

fn capability_iri(cap_slug: &str) -> String {
    format!("{DISTRIBUTION_BASE}capability/{cap_slug}")
}

fn consumer_iri(name: &str) -> String {
    format!("{GMEOW_NS}{name}")
}

fn iri(ns: &str, local: &str) -> String {
    format!("{ns}{local}")
}

// ── N-Triples helpers (mirroring docs_format_rendering.rs / carrier.rs) ─────────────

fn triple(subject: &str, predicate: &str, object: &str) -> String {
    format!("<{subject}> <{predicate}> <{object}> .")
}

fn triple_lit(subject: &str, predicate: &str, literal: &str) -> String {
    format!("<{subject}> <{predicate}> {} .", nt_literal(literal))
}

/// Escape a string as an N-Triples quoted literal (UTF-8 passes through verbatim).
fn nt_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// The four a-box skeleton triples every subject this module emits carries:
/// `rdf:type`, `rdfs:isDefinedBy <graph/distribution-catalog>`, `gmeow:graphBoxRole
/// gmeow:boxABox`, and `rdfs:label`.
fn skeleton(lines: &mut Vec<String>, subject: &str, rdf_type: &str, label: &str) {
    lines.push(triple(subject, RDF_TYPE, rdf_type));
    lines.push(triple(
        subject,
        RDFS_IS_DEFINED_BY,
        GRAPH_DISTRIBUTION_CATALOG,
    ));
    lines.push(triple(
        subject,
        &iri(GMEOW_NS, "graphBoxRole"),
        &iri(GMEOW_NS, "boxABox"),
    ));
    lines.push(triple_lit(subject, RDFS_LABEL, label));
}

/// Emit the sorted, deduped, byte-stable N-Triples for the whole catalog.
fn emit_ntriples() -> Vec<u8> {
    let mut lines: Vec<String> = Vec::new();

    // ── family nodes ──
    for family in [Family::DocRender, Family::Serialization] {
        let fam = family_iri(family);
        skeleton(
            &mut lines,
            &fam,
            &iri(GMEOW_NS, "DistributionFamily"),
            &format!("documentation distribution family {}", family.slug()),
        );
    }

    // ── doc-render distributions: EXACTLY the DocFormat variants ──
    for fmt in DocFormat::ALL {
        let slug = fmt.slug();
        let dist = dist_iri(slug);
        skeleton(
            &mut lines,
            &dist,
            &iri(GMEOW_NS, "DocumentationDistribution"),
            &format!("documentation distribution {slug}"),
        );
        lines.push(triple_lit(
            &dist,
            &iri(GMEOW_NS, "distributionFormat"),
            slug,
        ));
        lines.push(triple(
            &dist,
            &iri(GMEOW_NS, "distributionFamily"),
            &family_iri(Family::DocRender),
        ));
        lines.push(triple_lit(
            &dist,
            &iri(GMEOW_NS, "artifactMediaType"),
            doc_render_media_type(fmt),
        ));
        lines.push(triple(
            &dist,
            &iri(GMEOW_NS, "eligibleForConsumer"),
            &consumer_iri(doc_render_consumer(fmt)),
        ));

        // Declared loss, SOURCED FROM format_capabilities (single authority — never
        // re-authored here).
        let caps = format_capabilities(fmt);
        for cap in &caps.dropped {
            let cap_slug = cap.slug();
            let loss_node = loss_iri(slug, cap_slug);
            lines.push(triple(
                &dist,
                &iri(GMEOW_NS, "declaredLoss"),
                &loss_node,
            ));
            skeleton(
                &mut lines,
                &loss_node,
                &iri(GMEOW_NS, "ProjectionLoss"),
                &format!("distribution {slug} declared loss {cap_slug}"),
            );
            lines.push(triple(
                &loss_node,
                &iri(GMEOW_NS, "accountsForParameter"),
                &capability_iri(cap_slug),
            ));
        }
    }

    // ── serialization distributions: no declaredLoss ──
    for dist_enum in SerializationDist::ALL {
        let slug = dist_enum.slug();
        let dist = dist_iri(slug);
        skeleton(
            &mut lines,
            &dist,
            &iri(GMEOW_NS, "DocumentationDistribution"),
            &format!("documentation distribution {slug}"),
        );
        lines.push(triple_lit(
            &dist,
            &iri(GMEOW_NS, "distributionFormat"),
            slug,
        ));
        lines.push(triple(
            &dist,
            &iri(GMEOW_NS, "distributionFamily"),
            &family_iri(Family::Serialization),
        ));
        lines.push(triple_lit(
            &dist,
            &iri(GMEOW_NS, "artifactMediaType"),
            dist_enum.media_type(),
        ));
        lines.push(triple(
            &dist,
            &iri(GMEOW_NS, "eligibleForConsumer"),
            &consumer_iri(dist_enum.consumer()),
        ));
    }

    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    out.push('\n');
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_docs::formats::Capability;

    fn ntriples_text() -> String {
        // Building via the real entry point exercises parse_into_graph end-to-end (a
        // parse failure here would fail the test), then the string-content assertions
        // below grep the exact bytes the emitter produced (what parse_into_graph parsed).
        let _ds = build_distribution_catalog().expect("build distribution catalog");
        String::from_utf8(emit_ntriples()).expect("utf8 n-triples")
    }

    #[test]
    fn catalog_is_byte_reproducible() {
        let a = emit_ntriples();
        let b = emit_ntriples();
        assert_eq!(a, b, "distribution catalog N-Triples must be deterministic");
    }

    #[test]
    fn every_slug_appears_as_a_distribution_format() {
        let nt = ntriples_text();
        let pred = iri(GMEOW_NS, "distributionFormat");
        let expected = [
            "site", "mdbook", "pdf", "snippets", "okf", "jsonld", "yamlld", "pydantic",
        ];
        for slug in expected {
            let needle = format!("<{}> \"{slug}\" .", pred);
            assert!(
                nt.lines().any(|l| l.contains(&needle)),
                "missing gmeow:distributionFormat for slug {slug:?}"
            );
        }
    }

    #[test]
    fn every_subject_carries_the_four_skeleton_triples() {
        let nt = ntriples_text();
        // Collect every distinct subject IRI under DISTRIBUTION_BASE.
        let mut subjects: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for line in nt.lines() {
            if let Some(rest) = line.strip_prefix('<')
                && let Some(end) = rest.find('>')
            {
                let subject = &rest[..end];
                if subject.starts_with(DISTRIBUTION_BASE) {
                    subjects.insert(subject.to_string());
                }
            }
        }
        assert!(
            subjects.len() >= 8 + 2,
            "expected at least 8 distributions + 2 families, got {}",
            subjects.len()
        );
        for subject in &subjects {
            assert!(
                nt.contains(&format!("<{subject}> <{RDF_TYPE}>")),
                "{subject} missing rdf:type"
            );
            assert!(
                nt.contains(&triple(
                    subject,
                    RDFS_IS_DEFINED_BY,
                    GRAPH_DISTRIBUTION_CATALOG
                )),
                "{subject} missing rdfs:isDefinedBy <{GRAPH_DISTRIBUTION_CATALOG}>"
            );
            assert!(
                nt.contains(&triple(
                    subject,
                    &iri(GMEOW_NS, "graphBoxRole"),
                    &iri(GMEOW_NS, "boxABox")
                )),
                "{subject} missing gmeow:graphBoxRole gmeow:boxABox"
            );
            assert!(
                nt.contains(&format!("<{subject}> <{RDFS_LABEL}>")),
                "{subject} missing rdfs:label"
            );
        }
    }

    #[test]
    fn doc_render_declared_loss_matches_format_capabilities_exactly() {
        let nt = ntriples_text();
        for fmt in DocFormat::ALL {
            let slug = fmt.slug();
            let dist = dist_iri(slug);
            let caps = format_capabilities(fmt);
            for cap in Capability::ALL {
                let loss_node = loss_iri(slug, cap.slug());
                let declares = nt.contains(&triple(
                    &dist,
                    &iri(GMEOW_NS, "declaredLoss"),
                    &loss_node,
                ));
                let is_dropped = caps.dropped.contains(&cap);
                assert_eq!(
                    declares, is_dropped,
                    "{slug}/{:?}: catalog declaredLoss ({declares}) disagrees with \
                     format_capabilities().dropped ({is_dropped}) — single-authority drift",
                    cap
                );
                if is_dropped {
                    assert!(
                        nt.contains(&triple(
                            &loss_node,
                            &iri(GMEOW_NS, "accountsForParameter"),
                            &capability_iri(cap.slug())
                        )),
                        "{slug}/{:?} loss node missing accountsForParameter",
                        cap
                    );
                }
            }
        }
    }

    #[test]
    fn serialization_family_has_no_declared_loss() {
        let nt = ntriples_text();
        for dist_enum in SerializationDist::ALL {
            let dist = dist_iri(dist_enum.slug());
            let pred = iri(GMEOW_NS, "declaredLoss");
            let needle = format!("<{dist}> <{pred}>");
            assert!(
                !nt.lines().any(|l| l.starts_with(&needle)),
                "{} (serialization family) must not declare loss",
                dist_enum.slug()
            );
        }
    }

    #[test]
    fn catalog_is_digest_free() {
        let nt = ntriples_text();
        assert!(
            !nt.contains("contentDigest"),
            "distribution catalog schema must stay digest-free (release-time instance, not schema)"
        );
    }

    #[test]
    fn every_distribution_has_a_family_and_consumer() {
        let nt = ntriples_text();
        let all_slugs = [
            ("site", Family::DocRender),
            ("mdbook", Family::DocRender),
            ("pdf", Family::DocRender),
            ("snippets", Family::DocRender),
            ("okf", Family::Serialization),
            ("jsonld", Family::Serialization),
            ("yamlld", Family::Serialization),
            ("pydantic", Family::Serialization),
        ];
        for (slug, family) in all_slugs {
            let dist = dist_iri(slug);
            assert!(
                nt.contains(&triple(
                    &dist,
                    &iri(GMEOW_NS, "distributionFamily"),
                    &family_iri(family)
                )),
                "{slug} missing distributionFamily {}",
                family.slug()
            );
            let pred = iri(GMEOW_NS, "eligibleForConsumer");
            let needle = format!("<{dist}> <{pred}>");
            assert!(
                nt.lines().any(|l| l.starts_with(&needle)),
                "{slug} missing eligibleForConsumer"
            );
        }
    }
}
