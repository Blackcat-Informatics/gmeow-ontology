// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single per-format capability-loss table.
//!
//! Every documentation projection (the static site, the mdbook, the print PDF,
//! the flat per-term snippets) renders the same underlying
//! [`crate::model::DocsModel`] but can
//! only carry a subset of the site's live surfaces. This module is the ONE
//! source of truth for which of the six cross-cutting capabilities each format
//! preserves and which it declares lost. Both the renderers (the PDF loss
//! appendix in `docs-print`) and the pipeline grounding read this table, so a
//! format's loss appendix matches the graph loss ledger *by construction* — the
//! two can never drift because they are the same data.
//!
//! ## The capability poset
//!
//! The formats are the nodes of the projection DAG the pipeline declares —
//! `canonical → body-set → {site → snippets, mdbook, pdf}` — NOT a linear chain.
//! Dropped-capability sets grow monotonically along the DAG's covering edges
//! ([`PROJECTION_DAG_EDGES`]): the site is refined into the flat snippets, so
//!
//! ```text
//! dropped(site) ⊆ dropped(snippets)      (the one format→format refinement)
//! ```
//!
//! mdbook and pdf are independent siblings projected directly off the shared
//! body-set — comparable to neither the site nor each other — so no chain relates
//! them (mdbook packs live interactivity the pdf never carries; they are genuinely
//! incomparable). [`format_capabilities`] realizes the per-node partition and the
//! DAG-edge monotonicity test in this module gates it. Nothing a format drops is
//! ever regained by a strictly-poorer format downstream of it.
//!
//! Pure / std-only: no I/O and no graph dependency.

/// A documentation output format (projection surface). `ALL` lists them richest
/// first as the canonical iteration order; the loss structure among them is the
/// [`PROJECTION_DAG_EDGES`] poset, not this linear listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DocFormat {
    /// The live static HTML site — the richest surface (all capabilities).
    Site,
    /// The mdbook `src/` tree: packs the vendored wasm engines + controller + an
    /// interactive host chapter, so it carries live SPARQL / reasoning / transcode;
    /// it ships no bundled full-text index of our making.
    Mdbook,
    /// The single print PDF: static text + tables + bibliography only.
    Pdf,
    /// Flat per-term Markdown snippets: no cross-links, search, or diagrams.
    Snippets,
}

impl DocFormat {
    /// Every format, richest first — the canonical iteration order (also the
    /// declared-loss chain order).
    pub const ALL: [DocFormat; 4] = [
        DocFormat::Site,
        DocFormat::Mdbook,
        DocFormat::Pdf,
        DocFormat::Snippets,
    ];

    /// The stable machine slug (kebab-case) identifying this format.
    pub fn slug(&self) -> &'static str {
        match self {
            DocFormat::Site => "site",
            DocFormat::Mdbook => "mdbook",
            DocFormat::Pdf => "pdf",
            DocFormat::Snippets => "snippets",
        }
    }

    /// A human-readable label for this format.
    pub fn label(&self) -> &'static str {
        match self {
            DocFormat::Site => "Static site",
            DocFormat::Mdbook => "mdbook",
            DocFormat::Pdf => "Print PDF",
            DocFormat::Snippets => "Term snippets",
        }
    }
}

/// A cross-cutting documentation capability. Each format either represents a
/// capability or declares it a loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// A full-text search index over the whole corpus.
    SearchIndex,
    /// Live, executable SPARQL queries against the ontology.
    LiveSparql,
    /// Interactive surfaces (the query playground, collapsible/JS widgets).
    Interactivity,
    /// Live in-browser reasoning: the native GMEOW structured-DL chase and the
    /// GMN-0↔GMN-1 transcode, run client-side over the vendored wasm engines.
    LiveReasoning,
    /// Rendered diagrams (the SVG dependency / four-boxes / DAG figures).
    Diagrams,
    /// High-fidelity cross-links (every term/slice reference is a live link that
    /// resolves inside the artifact).
    CrossLinkFidelity,
}

impl Capability {
    /// Every capability, in the canonical (sorted) order.
    pub const ALL: [Capability; 6] = [
        Capability::SearchIndex,
        Capability::LiveSparql,
        Capability::Interactivity,
        Capability::LiveReasoning,
        Capability::Diagrams,
        Capability::CrossLinkFidelity,
    ];

    /// The stable machine slug (kebab-case) identifying this capability.
    pub fn slug(&self) -> &'static str {
        match self {
            Capability::SearchIndex => "search-index",
            Capability::LiveSparql => "live-sparql",
            Capability::Interactivity => "interactivity",
            Capability::LiveReasoning => "live-reasoning",
            Capability::Diagrams => "diagrams",
            Capability::CrossLinkFidelity => "cross-link-fidelity",
        }
    }
}

/// The format→format covering edges of the projection DAG — the Hasse edges of the
/// capability-loss poset among the emitted formats, projected from the composition DAG
/// the pipeline declares (`canonical → body-set → {site → snippets, mdbook, pdf}`).
///
/// The site is refined into the flat per-term snippets (`site → snippets`), so snippets
/// must drop a superset of what the site drops. mdbook and pdf are **provenance
/// siblings** — each projected directly off the shared body-set, NEITHER derived from the
/// other — so this PROVENANCE poset carries no `site → mdbook`, `mdbook → pdf`, or
/// `site → pdf` edge. That is distinct from the CAPABILITY lattice
/// ([`format_capabilities`]), where the dropped sets DO form a chain
/// `dropped(site) ⊆ dropped(mdbook) ⊆ dropped(pdf)`: mdbook is strictly richer than the
/// pdf (it packs the live engines the pdf cannot), so the two are provenance-incomparable
/// but capability-comparable — NOT "genuinely incomparable." The deleted linear
/// `site ⊆ mdbook ⊆ pdf ⊆ snippets` chain conflated the two, reading that capability
/// nesting as a PROVENANCE refinement it is not. Monotonicity is proved BOTH along these
/// provenance edges AND along the capability lattice (see the tests). The pipeline
/// cross-checks this edge set against its composition legs so the two cannot drift.
pub const PROJECTION_DAG_EDGES: &[(DocFormat, DocFormat)] =
    &[(DocFormat::Site, DocFormat::Snippets)];

/// One format's capability partition: which capabilities it represents and which
/// it declares lost. Both vectors are sorted (by [`Capability`]'s derived order)
/// and are a total, disjoint partition of [`Capability::ALL`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatCapabilities {
    /// The format this partition describes.
    pub format: DocFormat,
    /// The capabilities this format faithfully represents (sorted).
    pub representable: Vec<Capability>,
    /// The capabilities this format declares lost (sorted).
    pub dropped: Vec<Capability>,
}

/// The honest per-format capability partition. This is the single authority the
/// loss appendix and the graph ledger both read.
///
/// The dropped sets, and the reasoning behind each:
///
/// * **Site** — drops nothing. The live HTML site carries the search index, the
///   live SPARQL playground, every interactive widget, the in-browser reasoner +
///   GMN transcode, the rendered diagrams, and full cross-link fidelity.
/// * **Mdbook** — drops `{SearchIndex}` only. The book packs the SAME vendored wasm
///   engines + controller the site does (validate / purrdf-SPARQL / reason / GMN) and
///   an interactive host chapter, wired through `book.toml` `additional-js`, so once
///   built it carries live SPARQL, the interactive widgets, and the in-browser
///   reasoning + transcode — plus the rendered diagrams (SVGs render inline) and
///   cross-link fidelity (chapter links resolve; dropped-surface links externalize to
///   the published site, never dangling). It keeps no bundled full-text index of our
///   making (mdbook ships its own client search), so `SearchIndex` stays a declared
///   loss.
/// * **Pdf** — drops `{SearchIndex, LiveSparql, Interactivity, LiveReasoning,
///   Diagrams, CrossLinkFidelity}` (all of them). This print PDF renders term text,
///   tables, and the bibliography. It embeds **no** diagrams, exposes no search index
///   or live SPARQL, has no interactive surfaces or in-browser reasoning, and its
///   cross-references are prose (not live resolvable links). The set is deliberately
///   conservative: a capability is representable only when the PDF genuinely renders it.
/// * **Snippets** — drops the same six as the PDF. Flat per-term Markdown blocks
///   carry no cross-links, no search, no diagrams, no live SPARQL, no interactivity,
///   and no in-browser reasoning.
///
/// Dropped sets are monotone along the projection DAG's covering edges
/// ([`PROJECTION_DAG_EDGES`]): `dropped(site) ⊆ dropped(snippets)`. Separately — and
/// gated by its own test — the dropped sets form a capability-refinement chain
/// `dropped(site) ⊆ dropped(mdbook) ⊆ dropped(pdf) = dropped(snippets)`: mdbook and pdf
/// are provenance siblings but NOT capability-incomparable (mdbook represents a superset
/// of the pdf). Both properties are machine-checked below.
pub fn format_capabilities(fmt: DocFormat) -> FormatCapabilities {
    let dropped: Vec<Capability> = match fmt {
        DocFormat::Site => Vec::new(),
        DocFormat::Mdbook => vec![Capability::SearchIndex],
        DocFormat::Pdf | DocFormat::Snippets => vec![
            Capability::SearchIndex,
            Capability::LiveSparql,
            Capability::Interactivity,
            Capability::LiveReasoning,
            Capability::Diagrams,
            Capability::CrossLinkFidelity,
        ],
    };

    let representable: Vec<Capability> = Capability::ALL
        .into_iter()
        .filter(|c| !dropped.contains(c))
        .collect();

    // Both partitions sorted by the derived Capability order (ALL is already in
    // that order, and each match arm lists in that order); assert in the test.
    FormatCapabilities {
        format: fmt,
        representable,
        dropped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every capability appears in exactly one of `representable` / `dropped`,
    /// for every format (a total, disjoint partition), and both are sorted.
    #[test]
    fn partition_is_total_and_disjoint() {
        for fmt in DocFormat::ALL {
            let caps = format_capabilities(fmt);
            assert_eq!(caps.format, fmt);

            // Sorted.
            let mut sorted_repr = caps.representable.clone();
            sorted_repr.sort();
            assert_eq!(
                sorted_repr, caps.representable,
                "{fmt:?} representable unsorted"
            );
            let mut sorted_drop = caps.dropped.clone();
            sorted_drop.sort();
            assert_eq!(sorted_drop, caps.dropped, "{fmt:?} dropped unsorted");

            // Total + disjoint: each capability is in exactly one side.
            for cap in Capability::ALL {
                let in_repr = caps.representable.contains(&cap);
                let in_drop = caps.dropped.contains(&cap);
                assert!(
                    in_repr ^ in_drop,
                    "{fmt:?}/{cap:?}: must be in exactly one of representable/dropped"
                );
            }
            assert_eq!(
                caps.representable.len() + caps.dropped.len(),
                Capability::ALL.len(),
                "{fmt:?}: partition size mismatch"
            );
        }
    }

    /// Dropped-capability sets are monotone along the projection DAG's covering
    /// edges ([`PROJECTION_DAG_EDGES`]) — NOT a linear chain. For each edge
    /// `src → tgt`, `dropped(src) ⊆ dropped(tgt)`: nothing the source format drops
    /// is regained by the strictly-poorer format it refines into. mdbook and pdf are
    /// provenance siblings off the body-set, so no provenance EDGE constrains them —
    /// their capability nesting is checked separately by the capability-lattice test.
    #[test]
    fn dropped_capabilities_are_monotone_along_the_dag_edges() {
        use std::collections::BTreeSet;
        let dropped = |fmt| -> BTreeSet<Capability> {
            format_capabilities(fmt).dropped.into_iter().collect()
        };
        // There is at least one real refinement edge, and every edge is monotone.
        assert!(
            !PROJECTION_DAG_EDGES.is_empty(),
            "the projection DAG must declare its format→format refinement edges"
        );
        for &(src, tgt) in PROJECTION_DAG_EDGES {
            assert!(
                dropped(src).is_subset(&dropped(tgt)),
                "DAG edge {src:?} → {tgt:?} is not monotone: {tgt:?} regains {:?}",
                dropped(src).difference(&dropped(tgt)).collect::<Vec<_>>()
            );
        }

        // The concrete partitions, pinned so a future edit that breaks the poset
        // (e.g. a linear-chain regression, or mdbook silently losing interactivity)
        // fails loudly here. The site is lossless; mdbook drops ONLY the bundled
        // search index (it packs the live engines); pdf and snippets drop everything.
        assert!(dropped(DocFormat::Site).is_empty());
        assert_eq!(
            dropped(DocFormat::Mdbook),
            BTreeSet::from([Capability::SearchIndex])
        );
        assert_eq!(dropped(DocFormat::Pdf).len(), Capability::ALL.len());
        assert_eq!(dropped(DocFormat::Snippets), dropped(DocFormat::Pdf));

        // The STRUCTURAL fact the DAG re-derivation encodes: mdbook and pdf are
        // sibling PROVENANCE projections off the body-set, so the provenance poset
        // carries NO edge between them (nor `site → mdbook`). This is about DERIVATION,
        // not capability — their capability nesting is a separate, genuine invariant
        // gated by `dropped_sets_form_the_capability_refinement_chain` below.
        assert!(
            !PROJECTION_DAG_EDGES.contains(&(DocFormat::Mdbook, DocFormat::Pdf)),
            "mdbook and pdf are provenance siblings — the DAG must NOT relate them by a provenance edge"
        );
        assert!(
            !PROJECTION_DAG_EDGES.contains(&(DocFormat::Site, DocFormat::Mdbook)),
            "mdbook refines the body-set, not the site — no `site → mdbook` edge"
        );
        assert!(
            PROJECTION_DAG_EDGES.contains(&(DocFormat::Site, DocFormat::Snippets)),
            "snippets is the site's flat refinement — that edge MUST be present"
        );
    }

    /// The CAPABILITY-lattice invariant, distinct from the provenance DAG: the dropped
    /// sets form a refinement chain `dropped(site) ⊆ dropped(mdbook) ⊆ dropped(pdf) =
    /// dropped(snippets)`. mdbook is strictly richer than the pdf (it packs the live
    /// engines the pdf cannot), so a regression where the pdf REGAINS a capability mdbook
    /// drops — or where mdbook silently loses interactivity so it no longer represents a
    /// superset of the pdf — is caught here. This restores the coverage the deleted linear
    /// `site ⊆ mdbook ⊆ pdf ⊆ snippets` chain carried, WITHOUT asserting a false provenance
    /// edge (the two structures are gated independently).
    #[test]
    fn dropped_sets_form_the_capability_refinement_chain() {
        use std::collections::BTreeSet;
        let dropped = |fmt| -> BTreeSet<Capability> {
            format_capabilities(fmt).dropped.into_iter().collect()
        };
        assert!(
            dropped(DocFormat::Site).is_subset(&dropped(DocFormat::Mdbook)),
            "capability lattice: dropped(site) ⊄ dropped(mdbook)"
        );
        assert!(
            dropped(DocFormat::Mdbook).is_subset(&dropped(DocFormat::Pdf)),
            "capability lattice: mdbook must represent a SUPERSET of the pdf (it packs the \
             live engines the pdf cannot) — dropped(mdbook) ⊄ dropped(pdf): pdf regains {:?}",
            dropped(DocFormat::Mdbook)
                .difference(&dropped(DocFormat::Pdf))
                .collect::<Vec<_>>()
        );
        assert!(
            dropped(DocFormat::Site).is_subset(&dropped(DocFormat::Snippets)),
            "capability lattice: dropped(site) ⊄ dropped(snippets)"
        );
        assert_eq!(
            dropped(DocFormat::Pdf),
            dropped(DocFormat::Snippets),
            "capability lattice: the flat pdf and snippets drop the identical set"
        );
        // mdbook is STRICTLY richer than the pdf — the nesting is proper, not equality
        // (else the "mdbook packs the live engines" claim would be vacuous).
        assert!(
            dropped(DocFormat::Mdbook) != dropped(DocFormat::Pdf),
            "capability lattice: mdbook must be STRICTLY richer than the pdf"
        );
    }

    #[test]
    fn slugs_are_stable_and_unique() {
        use std::collections::BTreeSet;
        let fmt_slugs: BTreeSet<&str> = DocFormat::ALL.iter().map(|f| f.slug()).collect();
        assert_eq!(fmt_slugs.len(), DocFormat::ALL.len());
        let cap_slugs: BTreeSet<&str> = Capability::ALL.iter().map(|c| c.slug()).collect();
        assert_eq!(cap_slugs.len(), Capability::ALL.len());
    }
}
