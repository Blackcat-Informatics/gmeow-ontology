// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Grounding the four documentation-output formats as lossy projections.
//!
//! The site, mdbook, print PDF, and per-term snippets all render one shared
//! documentation body-set. This module TYPES that fan-out as first-class crossing
//! records whose preservation is DERIVED over a composition DAG rather than
//! hand-asserted, and whose per-format capability losses are read from the SINGLE
//! honest source ([`gmeow_docs::formats::format_capabilities`]) — the same table the
//! print PDF's loss appendix reads, so the appendix and the graph ledger match by
//! construction.
//!
//! ## The projection DAG (A1)
//!
//! One `logic:Correspondence` per leg of the composition graph:
//!
//! ```text
//!   canonical ──▶ body-set ──▶ site ──▶ snippets
//!                    │  ├──▶ mdbook
//!                    │  └──▶ pdf
//! ```
//!
//! Each format's `logic:preservationKind` is the weakest-dominates
//! [`PreservationKind`] join over the legs it composes through — never a flat grade.
//! A leg whose target still carries the live-SPARQL surface (sound query answering)
//! is a `logic:SoundUnderApproximation`; a leg whose target drops it falls to the
//! `logic:ValidationOnly` floor (rendered prose, no live entailment). The base
//! `canonical → body-set` extraction is a faithful-but-incomplete
//! `logic:SoundUnderApproximation`. `PreservationKind` derives `Ord` STRONGEST-FIRST,
//! so the weakest kind is the `.max()`.
//!
//! ## The loss lattice (A9 / F2)
//!
//! Each format's dropped capabilities are enumerated AS DATA — a
//! `gmeow:NotationProjectionProfile` per format with a `gmeow:representableParameter`
//! per carried capability and a `gmeow:declaredLoss` → `gmeow:ProjectionLoss`
//! (`gmeow:accountsForParameter` the dropped capability) per lost one — and the same
//! capability slugs are folded into the projection loss ledger by
//! [`fold_docs_format_loss`]. The dropped sets are monotone along this DAG's covering
//! edges (`gmeow_docs::formats::PROJECTION_DAG_EDGES` — `site → snippets`; mdbook and
//! pdf are incomparable siblings off the body-set), NOT a linear chain; the A3 gate
//! ([`crate::docs_loss_lattice`]) proves totality + DAG-edge monotonicity over the
//! same table.
//!
//! ## Blob self-description (F4)
//!
//! The packed `docs-book` / `docs-print` blobs are self-described by a
//! `gmeow:contentDigest "blake3:<hex>"` on a stable blob-descriptor IRI, so the graph
//! records the byte identity of the docs artifacts it grounds.
//!
//! Every identity is content-addressed and the N-Triples are sorted + deduped, so the
//! corpus is byte-reproducible (no clock, no randomness). The loss-ledger fold is
//! blob-free and lands in the mappings stage (where the single report loss store
//! lives); the RDF graph — which additionally content-addresses the packed blobs —
//! lands in the carrier assembly (where the blobs are built).

use sha2::{Digest, Sha256};

use gmeow_docs::formats::{Capability, DocFormat, format_capabilities};
use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::ProjectionResult;

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// The instance base every minted grounding IRI lives under. Kept OUT of the `gmeow:`
/// term namespace (mirroring the sibling `lang:` corpora, which scope under
/// `example.org`) so the grounding is unambiguously carried instance data, never
/// vocabulary surface.
const EXAMPLE_BASE: &str = "http://example.org/docs-format/";

/// The assembled docs-format grounding corpus.
pub struct DocsFormatCorpus {
    /// The deterministic, sorted, byte-stable N-Triples graph
    /// (`graph/docs-format-rendering`), including the blob-digest self-description.
    pub ntriples: Vec<u8>,
    /// One `ProjectionResult` per format — the derived preservation + the capability
    /// drops. Interned into [`loss`](Self::loss) by target focus.
    pub ledger: Vec<ProjectionResult>,
    /// The loss store the per-format capability drops are interned into.
    pub loss: LossLedger,
}

/// One leg of the composition DAG: a source node crossing to a target node. The
/// `target_fmt` is `Some` for a leg whose target is a concrete output format, `None`
/// for the base `canonical → body-set` extraction.
struct Leg {
    key: &'static str,
    source: String,
    target: String,
    target_fmt: Option<DocFormat>,
}

/// The honest per-leg preservation. The base extraction is a faithful-but-incomplete
/// [`PreservationKind::SoundUnder`]; a format leg is `SoundUnder` only while the target
/// still carries live SPARQL (sound query answering), and falls to
/// [`PreservationKind::ValidationOnly`] — the rendered-prose floor — once it drops it.
fn leg_preservation(target_fmt: Option<DocFormat>) -> PreservationKind {
    match target_fmt {
        None => PreservationKind::SoundUnder,
        Some(fmt) => {
            if format_capabilities(fmt)
                .representable
                .contains(&Capability::LiveSparql)
            {
                PreservationKind::SoundUnder
            } else {
                PreservationKind::ValidationOnly
            }
        }
    }
}

/// The five legs of the composition DAG, in a fixed order.
fn legs() -> Vec<Leg> {
    let canonical = node("canonical");
    let body_set = node("body-set");
    vec![
        Leg {
            key: "canonical->body-set",
            source: canonical,
            target: body_set.clone(),
            target_fmt: None,
        },
        Leg {
            key: "body-set->site",
            source: body_set.clone(),
            target: surface(DocFormat::Site),
            target_fmt: Some(DocFormat::Site),
        },
        Leg {
            key: "body-set->mdbook",
            source: body_set.clone(),
            target: surface(DocFormat::Mdbook),
            target_fmt: Some(DocFormat::Mdbook),
        },
        Leg {
            key: "body-set->pdf",
            source: body_set,
            target: surface(DocFormat::Pdf),
            target_fmt: Some(DocFormat::Pdf),
        },
        Leg {
            key: "site->snippets",
            source: surface(DocFormat::Site),
            target: surface(DocFormat::Snippets),
            target_fmt: Some(DocFormat::Snippets),
        },
    ]
}

/// The leg keys each format's rendering composes THROUGH, richest-first. Every format
/// composes the base `canonical->body-set` extraction; snippets composes through the
/// site surface (`site->snippets`), the others directly off the body-set.
fn composition_leg_keys(fmt: DocFormat) -> &'static [&'static str] {
    match fmt {
        DocFormat::Site => &["canonical->body-set", "body-set->site"],
        DocFormat::Mdbook => &["canonical->body-set", "body-set->mdbook"],
        DocFormat::Pdf => &["canonical->body-set", "body-set->pdf"],
        DocFormat::Snippets => &["canonical->body-set", "body-set->site", "site->snippets"],
    }
}

/// The DERIVED weakest-dominates preservation join over the legs a format composes
/// through — never a flat asserted grade.
fn derived_preservation(fmt: DocFormat) -> PreservationKind {
    composition_leg_keys(fmt)
        .iter()
        .map(|k| leg_preservation(leg_target_fmt(k)))
        .max()
        .unwrap_or(PreservationKind::ValidationOnly)
}

/// The `target_fmt` of a leg by key (drives [`leg_preservation`] in the join). Looked up
/// against [`legs`] — the SINGLE source of truth for the key->format mapping — rather than
/// reconstructed in a parallel `match`, so the two can never drift apart. An unknown key
/// (or the base `canonical->body-set` extraction) falls to the base-extraction `None`.
fn leg_target_fmt(key: &str) -> Option<DocFormat> {
    legs()
        .into_iter()
        .find(|leg| leg.key == key)
        .and_then(|leg| leg.target_fmt)
}

/// The rendering-kind individual for a format.
fn rendering_kind(fmt: DocFormat) -> &'static str {
    match fmt {
        DocFormat::Site => "renderingDocsPage",
        DocFormat::Mdbook => "renderingBook",
        DocFormat::Pdf => "renderingPrint",
        DocFormat::Snippets => "renderingSnippet",
    }
}

/// Build the full docs-format grounding corpus. `book_digest` / `print_digest` are the
/// `blake3:<hex>` content digests of the packed `docs-book` / `docs-print` blobs,
/// content-addressed into the graph (F4) so it self-describes the docs it grounds.
/// `print_pdf_digest` is the `blake3:<hex>` digest of the RAW `gmeow.pdf` bytes (before
/// they are packed into the `docs-print` tar) — it grounds the shipped bundle's
/// `application/pdf` attestation so a consumer can verify the PDF's byte identity
/// straight from the committed `gmeow.gts`, not only from the GPG-gated release fold.
pub fn build_docs_format_corpus(
    book_digest: &str,
    print_digest: &str,
    print_pdf_digest: &str,
) -> DocsFormatCorpus {
    let ntriples = emit_ntriples(book_digest, print_digest, print_pdf_digest);
    let mut loss = LossLedger::new();
    let mut ledger: Vec<ProjectionResult> = Vec::new();
    fold_docs_format_loss(&mut ledger, &mut loss);
    DocsFormatCorpus {
        ntriples,
        ledger,
        loss,
    }
}

/// Fold the per-format capability drops into the projection loss ledger. Blob-free
/// (a pure function of the capability table), so the mappings stage — where the single
/// report loss store lives — folds it exactly like every sibling `lang:` corpus, while
/// the RDF graph (which additionally content-addresses the blobs) rides in the carrier.
pub fn fold_docs_format_loss(ledger: &mut Vec<ProjectionResult>, loss: &mut LossLedger) {
    for fmt in DocFormat::ALL {
        let caps = format_capabilities(fmt);
        let kind = derived_preservation(fmt);
        let target = format!("docs-format:{}", fmt.slug());
        // The actual per-run drops are EXACTLY the dropped capability slugs the print
        // PDF's loss appendix prints — the appendix ↔ ledger join holds by construction.
        let mut actual_drops: Vec<String> = caps
            .dropped
            .iter()
            .map(|c| {
                format!(
                    "docs format '{}' drops capability '{}' (derived preservation = logic:{})",
                    fmt.slug(),
                    c.slug(),
                    kind.as_str(),
                )
            })
            .collect();
        if actual_drops.is_empty() {
            actual_drops.push(format!(
                "docs format '{}' drops no capability; derived preservation = logic:{}",
                fmt.slug(),
                kind.as_str(),
            ));
        }
        loss.record_projection_drops(&target, kind, &[], &actual_drops);
        ledger.push(ProjectionResult {
            target,
            content: String::new(),
            is_rdf: false,
            preservation: kind,
            complexity: "n/a".to_string(),
        });
    }
}

/// Emit the sorted, deduped, byte-stable N-Triples for the whole grounding graph.
fn emit_ntriples(book_digest: &str, print_digest: &str, print_pdf_digest: &str) -> Vec<u8> {
    let mut lines: Vec<String> = Vec::new();

    // ── the composition-DAG nodes ──
    let canonical = node("canonical");
    let body_set = node("body-set");
    lines.push(triple(&canonical, RDF_TYPE, &iri(LANG_NS, "Form")));
    lines.push(triple(&body_set, RDF_TYPE, &iri(LANG_NS, "Form")));
    for fmt in DocFormat::ALL {
        lines.push(triple(
            &surface(fmt),
            RDF_TYPE,
            &iri(LANG_NS, "SurfaceForm"),
        ));
    }

    // The capability parameters (one stable IRI per Capability) are the canonical
    // parameters the profiles represent or account-for; like the notation worked
    // example's `ex:pitch` / `ex:microtiming` they are open (owl:Thing) parameter
    // instances carried only as objects, never re-typed.

    // ── one logic:Correspondence per DAG leg (A1 + A2 law-spine) ──
    for leg in legs() {
        let corr = correspondence(leg.key);
        let kind = leg_preservation(leg.target_fmt);
        lines.push(triple(&corr, RDF_TYPE, &iri(LOGIC_NS, "Correspondence")));
        lines.push(triple(
            &corr,
            &iri(LOGIC_NS, "preservationKind"),
            &kind.iri(),
        ));
        lines.push(triple(
            &corr,
            &iri(LOGIC_NS, "correspondenceRelation"),
            &iri(LOGIC_NS, "Subsumes"),
        ));
        lines.push(triple(
            &corr,
            &iri(LOGIC_NS, "morphismClass"),
            &iri(LOGIC_NS, "LossyLens"),
        ));
        lines.push(triple(
            &corr,
            &iri(LOGIC_NS, "hasDeterminacy"),
            &iri(LOGIC_NS, "Crisp"),
        ));
        // Non-injective get: a docs surface cannot recover the canonical source, so the
        // crossing is never a mnemomorphism. The enumerated residue (the dropped
        // capabilities) lives on the format profile's gmeow:declaredLoss.
        lines.push(triple_typed(
            &corr,
            &iri(LOGIC_NS, "mnemomorphic"),
            "false",
            XSD_BOOLEAN,
        ));
        // The crossing endpoints, so the DAG is queryable.
        lines.push(triple(&corr, &iri(LANG_NS, "renderedContent"), &leg.source));
        lines.push(triple(&corr, &iri(LANG_NS, "renderingForm"), &leg.target));
    }

    // ── per-format profile, function, rendering (A9 lattice + derived preservation) ──
    for fmt in DocFormat::ALL {
        let caps = format_capabilities(fmt);
        let profile = profile_iri(fmt);
        let function = function_iri(fmt);
        let rendering = rendering_iri(fmt);
        let kind = derived_preservation(fmt);

        // The projection profile: the per-format loss ledger.
        lines.push(triple(
            &profile,
            RDF_TYPE,
            &iri(GMEOW_NS, "NotationProjectionProfile"),
        ));
        lines.push(triple(
            &profile,
            &iri(GMEOW_NS, "projectionFunction"),
            &function,
        ));
        for cap in &caps.representable {
            lines.push(triple(
                &profile,
                &iri(GMEOW_NS, "representableParameter"),
                &capability_param(*cap),
            ));
        }
        for cap in &caps.dropped {
            let loss_node = loss_iri(fmt, *cap);
            lines.push(triple(&profile, &iri(GMEOW_NS, "declaredLoss"), &loss_node));
            lines.push(triple(
                &loss_node,
                RDF_TYPE,
                &iri(GMEOW_NS, "ProjectionLoss"),
            ));
            lines.push(triple(
                &loss_node,
                &iri(GMEOW_NS, "accountsForParameter"),
                &capability_param(*cap),
            ));
        }

        // The projection function (the FnO-backed render).
        lines.push(triple(
            &function,
            RDF_TYPE,
            &iri(GMEOW_NS, "ProjectionFunction"),
        ));

        // The reified rendering: content is the body-set, form is the surface, kind is
        // the matching RenderingKind individual, convention is the profile, and the
        // DERIVED preservation join rides on lang:renderingPreservation.
        lines.push(triple(&rendering, RDF_TYPE, &iri(LANG_NS, "Rendering")));
        lines.push(triple(
            &rendering,
            &iri(LANG_NS, "renderingKind"),
            &iri(LANG_NS, rendering_kind(fmt)),
        ));
        lines.push(triple(
            &rendering,
            &iri(LANG_NS, "renderedContent"),
            &body_set,
        ));
        lines.push(triple(
            &rendering,
            &iri(LANG_NS, "renderingForm"),
            &surface(fmt),
        ));
        lines.push(triple(
            &rendering,
            &iri(LANG_NS, "renderingConvention"),
            &profile,
        ));
        lines.push(triple(
            &rendering,
            &iri(LANG_NS, "renderingPreservation"),
            &kind.iri(),
        ));
        // The composition: the rendering is built THROUGH the abstract body-set and its
        // leg correspondences, so the derived preservation join is queryable rather than
        // asserted. `lang:composedThroughAbstract` is the closest existing composition
        // predicate (its prose domain is lang:Translation/Unit — used here on the
        // sibling lang:Rendering, no formal rdfs:domain constrains it).
        lines.push(triple(
            &rendering,
            &iri(LANG_NS, "composedThroughAbstract"),
            &body_set,
        ));
        for key in composition_leg_keys(fmt) {
            lines.push(triple(
                &rendering,
                &iri(LANG_NS, "composedThroughAbstract"),
                &correspondence(key),
            ));
        }
    }

    // ── the packed-blob self-description (F4) ──
    for (segment, digest) in [("docs-book", book_digest), ("docs-print", print_digest)] {
        let blob = blob_descriptor(segment);
        lines.push(triple(
            &blob,
            RDF_TYPE,
            &iri(GMEOW_NS, "AttestationArtifact"),
        ));
        lines.push(triple_lit(&blob, &iri(GMEOW_NS, "contentDigest"), digest));
        lines.push(triple_lit(
            &blob,
            &iri(GMEOW_NS, "artifactMediaType"),
            "application/x-tar",
        ));
    }
    // The raw `gmeow.pdf` bytes get their OWN attestation, distinct from the docs-print
    // archive that carries them — this is what lets a consumer verify the PDF's byte
    // identity directly off the committed bundle (the shippable deliverable), mirroring
    // the release fold's `application/pdf` attestation over the same PDF bytes.
    let pdf_blob = blob_descriptor("docs-print-pdf");
    lines.push(triple(
        &pdf_blob,
        RDF_TYPE,
        &iri(GMEOW_NS, "AttestationArtifact"),
    ));
    lines.push(triple_lit(
        &pdf_blob,
        &iri(GMEOW_NS, "contentDigest"),
        print_pdf_digest,
    ));
    lines.push(triple_lit(
        &pdf_blob,
        &iri(GMEOW_NS, "artifactMediaType"),
        "application/pdf",
    ));

    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    out.push('\n');
    out.into_bytes()
}

// ── content-addressed identity helpers ─────────────────────────────────────────────

fn node(name: &str) -> String {
    format!("{EXAMPLE_BASE}node/{name}")
}

fn surface(fmt: DocFormat) -> String {
    format!("{EXAMPLE_BASE}surface/{}", fmt.slug())
}

fn capability_param(cap: Capability) -> String {
    format!("{EXAMPLE_BASE}capability/{}", cap.slug())
}

fn profile_iri(fmt: DocFormat) -> String {
    format!("{EXAMPLE_BASE}profile/{}", fmt.slug())
}

fn function_iri(fmt: DocFormat) -> String {
    format!("{EXAMPLE_BASE}function/{}", fmt.slug())
}

fn rendering_iri(fmt: DocFormat) -> String {
    format!("{EXAMPLE_BASE}rendering/{}", fmt.slug())
}

fn loss_iri(fmt: DocFormat, cap: Capability) -> String {
    format!("{EXAMPLE_BASE}loss/{}/{}", fmt.slug(), cap.slug())
}

fn blob_descriptor(segment: &str) -> String {
    format!("{EXAMPLE_BASE}blob/{segment}")
}

fn correspondence(key: &str) -> String {
    format!(
        "{EXAMPLE_BASE}correspondence/{}",
        digest16("docs-format-corr", key)
    )
}

/// A stable 16-hex-char content address over a domain-separated key.
fn digest16(domain: &str, key: &str) -> String {
    let digest = Sha256::digest(format!("{domain}\u{1f}{key}").as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

// ── N-Triples helpers (mirroring the sibling lang: producers) ──────────────────────

fn iri(ns: &str, local: &str) -> String {
    format!("{ns}{local}")
}

fn triple(subject: &str, predicate: &str, object: &str) -> String {
    format!("<{subject}> <{predicate}> <{object}> .")
}

fn triple_lit(subject: &str, predicate: &str, literal: &str) -> String {
    format!("<{subject}> <{predicate}> {} .", nt_literal(literal))
}

fn triple_typed(subject: &str, predicate: &str, literal: &str, datatype: &str) -> String {
    format!(
        "<{subject}> <{predicate}> {}^^<{datatype}> .",
        nt_literal(literal)
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    const BOOK_DIGEST: &str =
        "blake3:1111111111111111111111111111111111111111111111111111111111111111";
    const PRINT_DIGEST: &str =
        "blake3:2222222222222222222222222222222222222222222222222222222222222222";
    const PRINT_PDF_DIGEST: &str =
        "blake3:3333333333333333333333333333333333333333333333333333333333333333";

    #[test]
    fn corpus_is_byte_reproducible() {
        let a = build_docs_format_corpus(BOOK_DIGEST, PRINT_DIGEST, PRINT_PDF_DIGEST).ntriples;
        let b = build_docs_format_corpus(BOOK_DIGEST, PRINT_DIGEST, PRINT_PDF_DIGEST).ntriples;
        assert_eq!(a, b, "docs-format corpus N-Triples must be deterministic");
    }

    #[test]
    fn derived_preservation_matches_the_honest_join() {
        // The site AND the mdbook keep live SPARQL (the book packs the vendored engines)
        // → SoundUnderApproximation; the print PDF and flat snippets drop it → the
        // ValidationOnly floor. Never a flat asserted grade.
        assert_eq!(
            derived_preservation(DocFormat::Site),
            PreservationKind::SoundUnder
        );
        assert_eq!(
            derived_preservation(DocFormat::Mdbook),
            PreservationKind::SoundUnder
        );
        assert_eq!(
            derived_preservation(DocFormat::Pdf),
            PreservationKind::ValidationOnly
        );
        assert_eq!(
            derived_preservation(DocFormat::Snippets),
            PreservationKind::ValidationOnly
        );
        // No docs rendering may ever claim ExactPreservation (it is prose, not the canon).
        for fmt in DocFormat::ALL {
            assert_ne!(derived_preservation(fmt), PreservationKind::Exact);
        }
    }

    /// The format→format DAG edges gmeow_docs declares must be exactly the format
    /// surface-legs the composition DAG here carries: a `src → tgt` edge exists iff
    /// `tgt` composes THROUGH `src`'s output surface (the `<src>-><tgt>` leg). This is
    /// the machine cross-check that the loss-poset edge set and the projection legs
    /// cannot drift — the linear-chain assumption is gone; only real legs are edges.
    #[test]
    fn declared_dag_edges_match_the_composition_legs() {
        use gmeow_docs::formats::PROJECTION_DAG_EDGES;
        use std::collections::BTreeSet;

        // Derive the format→format edges from the legs: for each format `tgt`, the leg
        // it composes through whose SOURCE is another format's output surface.
        let surface_to_fmt: Vec<(String, DocFormat)> =
            DocFormat::ALL.iter().map(|&f| (surface(f), f)).collect();
        let mut derived: BTreeSet<(DocFormat, DocFormat)> = BTreeSet::new();
        for tgt in DocFormat::ALL {
            for key in composition_leg_keys(tgt) {
                if let Some(leg) = legs().into_iter().find(|l| &l.key == key)
                    && let Some(&(_, src_fmt)) =
                        surface_to_fmt.iter().find(|(s, _)| *s == leg.source)
                    && leg.target_fmt == Some(tgt)
                {
                    derived.insert((src_fmt, tgt));
                }
            }
        }
        let declared: BTreeSet<(DocFormat, DocFormat)> =
            PROJECTION_DAG_EDGES.iter().copied().collect();
        assert_eq!(
            declared, derived,
            "PROJECTION_DAG_EDGES drifted from the composition legs: the loss poset and \
             the projection DAG must declare the SAME format→format refinement edges"
        );
    }

    #[test]
    fn every_format_has_a_rendering_profile_and_derived_preservation() {
        let nt = String::from_utf8(
            build_docs_format_corpus(BOOK_DIGEST, PRINT_DIGEST, PRINT_PDF_DIGEST).ntriples,
        )
        .unwrap();
        for fmt in DocFormat::ALL {
            let rendering = rendering_iri(fmt);
            assert!(
                nt.contains(&triple(
                    &rendering,
                    &iri(LANG_NS, "renderingKind"),
                    &iri(LANG_NS, rendering_kind(fmt))
                )),
                "{fmt:?} has no lang:Rendering of the matching kind"
            );
            assert!(
                nt.contains(&triple(
                    &rendering,
                    &iri(LANG_NS, "renderingConvention"),
                    &profile_iri(fmt)
                )),
                "{fmt:?} rendering has no NotationProjectionProfile convention"
            );
            assert!(
                nt.contains(&triple(
                    &rendering,
                    &iri(LANG_NS, "renderingPreservation"),
                    &derived_preservation(fmt).iri()
                )),
                "{fmt:?} rendering has no derived preservation"
            );
        }
    }

    #[test]
    fn dropped_capabilities_are_enumerated_as_queryable_data() {
        let nt = String::from_utf8(
            build_docs_format_corpus(BOOK_DIGEST, PRINT_DIGEST, PRINT_PDF_DIGEST).ntriples,
        )
        .unwrap();
        // Every dropped capability appears as a gmeow:ProjectionLoss accountsForParameter
        // the matching capability parameter — the residue enumerated as data.
        for fmt in DocFormat::ALL {
            for cap in &format_capabilities(fmt).dropped {
                let loss_node = loss_iri(fmt, *cap);
                assert!(
                    nt.contains(&triple(
                        &loss_node,
                        &iri(GMEOW_NS, "accountsForParameter"),
                        &capability_param(*cap)
                    )),
                    "{fmt:?} dropped {cap:?} is not enumerated as a ProjectionLoss"
                );
                assert!(
                    nt.contains(&triple(
                        &profile_iri(fmt),
                        &iri(GMEOW_NS, "declaredLoss"),
                        &loss_node
                    )),
                    "{fmt:?} profile does not declare the loss for {cap:?}"
                );
            }
        }
    }

    #[test]
    fn leg_target_fmt_matches_the_single_source_of_truth() {
        // leg_target_fmt must agree with legs() — the single source of truth — for every
        // leg it defines. This is guaranteed by construction now that leg_target_fmt looks
        // legs() up rather than reconstructing the key->format map in a parallel `match`;
        // this assertion pins that invariant so a future edit back to a parallel map (which
        // would let the two silently drift) is caught here.
        for leg in legs() {
            assert_eq!(
                leg_target_fmt(leg.key),
                leg.target_fmt,
                "leg_target_fmt({:?}) disagrees with legs() for the SAME key",
                leg.key
            );
        }
        // Every leg key any format composes THROUGH must be a real leg key, never an
        // unknown key that would silently fall through to the base-extraction `None` and
        // desync `derived_preservation` from the emitted correspondence spine.
        let known_keys: std::collections::HashSet<&str> = legs().iter().map(|l| l.key).collect();
        for fmt in DocFormat::ALL {
            for key in composition_leg_keys(fmt) {
                assert!(
                    known_keys.contains(key),
                    "composition leg key {key:?} for {fmt:?} is not a leg in legs() — \
                     leg_target_fmt would silently fall to the base-extraction None"
                );
            }
        }
    }

    #[test]
    fn each_leg_carries_the_full_correspondence_law_spine() {
        let nt = String::from_utf8(
            build_docs_format_corpus(BOOK_DIGEST, PRINT_DIGEST, PRINT_PDF_DIGEST).ntriples,
        )
        .unwrap();
        let mut leg_count = 0usize;
        for leg in legs() {
            let corr = correspondence(leg.key);
            for pred in [
                "preservationKind",
                "correspondenceRelation",
                "morphismClass",
                "hasDeterminacy",
                "mnemomorphic",
            ] {
                assert!(
                    nt.contains(&format!("<{corr}> <{LOGIC_NS}{pred}>")),
                    "leg {} missing logic:{pred}",
                    leg.key
                );
            }
            leg_count += 1;
        }
        assert_eq!(leg_count, 5, "the composition DAG has five legs");
    }

    #[test]
    fn packed_blobs_are_self_described_by_content_digest() {
        let nt = String::from_utf8(
            build_docs_format_corpus(BOOK_DIGEST, PRINT_DIGEST, PRINT_PDF_DIGEST).ntriples,
        )
        .unwrap();
        assert!(nt.contains(&triple_lit(
            &blob_descriptor("docs-book"),
            &iri(GMEOW_NS, "contentDigest"),
            BOOK_DIGEST
        )));
        assert!(nt.contains(&triple_lit(
            &blob_descriptor("docs-print"),
            &iri(GMEOW_NS, "contentDigest"),
            PRINT_DIGEST
        )));
        // The two packed archives carry the tar media type.
        for segment in ["docs-book", "docs-print"] {
            assert!(nt.contains(&triple_lit(
                &blob_descriptor(segment),
                &iri(GMEOW_NS, "artifactMediaType"),
                "application/x-tar"
            )));
        }
        // The raw PDF has its OWN application/pdf attestation carrying the raw-PDF blake3.
        let pdf_blob = blob_descriptor("docs-print-pdf");
        assert!(nt.contains(&triple(
            &pdf_blob,
            RDF_TYPE,
            &iri(GMEOW_NS, "AttestationArtifact")
        )));
        assert!(nt.contains(&triple_lit(
            &pdf_blob,
            &iri(GMEOW_NS, "contentDigest"),
            PRINT_PDF_DIGEST
        )));
        assert!(nt.contains(&triple_lit(
            &pdf_blob,
            &iri(GMEOW_NS, "artifactMediaType"),
            "application/pdf"
        )));
    }

    #[test]
    fn loss_ledger_carries_every_format_and_its_dropped_capabilities() {
        let mut ledger: Vec<ProjectionResult> = Vec::new();
        let mut loss = LossLedger::new();
        fold_docs_format_loss(&mut ledger, &mut loss);
        assert_eq!(ledger.len(), DocFormat::ALL.len());
        for fmt in DocFormat::ALL {
            let target = format!("docs-format:{}", fmt.slug());
            let drops = loss.projection_drops_for(&target);
            assert!(!drops.is_empty(), "{fmt:?} has no ledger residue");
            for cap in &format_capabilities(fmt).dropped {
                assert!(
                    drops.iter().any(|d| d.contains(cap.slug())),
                    "{fmt:?} ledger residue omits dropped capability {cap:?}"
                );
            }
        }
    }
}
