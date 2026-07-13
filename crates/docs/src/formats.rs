// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single per-format capability-loss table.
//!
//! Every documentation projection (the static site, the mdbook, the print PDF,
//! the flat per-term snippets) renders the same underlying
//! [`crate::model::DocsModel`] but can
//! only carry a subset of the site's live surfaces. This module is the ONE
//! source of truth for which of the five cross-cutting capabilities each format
//! preserves and which it declares lost. Both the renderers (the PDF loss
//! appendix in `docs-print`) and the pipeline grounding read this table, so a
//! format's loss appendix matches the graph loss ledger *by construction* — the
//! two can never drift because they are the same data.
//!
//! ## The capability lattice
//!
//! The formats form a monotone loss chain: as the surface degrades from the live
//! site down to flat snippets, the set of DROPPED capabilities only grows. The
//! declared-loss sets therefore satisfy
//!
//! ```text
//! site ⊆ mdbook ⊆ pdf ⊆ snippets      (in DROPPED capabilities)
//! ```
//!
//! which [`format_capabilities`] realizes and the monotonicity test in this
//! module gates. Nothing a richer format drops is ever regained by a poorer one.
//!
//! Pure / std-only: no I/O and no graph dependency.

/// A documentation output format (projection surface). Ordered from richest to
/// poorest along the capability-loss chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DocFormat {
    /// The live static HTML site — the richest surface (all capabilities).
    Site,
    /// The mdbook `src/` tree: no live SPARQL, no search index, no playground.
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
    /// Rendered diagrams (the SVG dependency / four-boxes / DAG figures).
    Diagrams,
    /// High-fidelity cross-links (every term/slice reference is a live link that
    /// resolves inside the artifact).
    CrossLinkFidelity,
}

impl Capability {
    /// Every capability, in the canonical (sorted) order.
    pub const ALL: [Capability; 5] = [
        Capability::SearchIndex,
        Capability::LiveSparql,
        Capability::Interactivity,
        Capability::Diagrams,
        Capability::CrossLinkFidelity,
    ];

    /// The stable machine slug (kebab-case) identifying this capability.
    pub fn slug(&self) -> &'static str {
        match self {
            Capability::SearchIndex => "search-index",
            Capability::LiveSparql => "live-sparql",
            Capability::Interactivity => "interactivity",
            Capability::Diagrams => "diagrams",
            Capability::CrossLinkFidelity => "cross-link-fidelity",
        }
    }
}

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
///   live SPARQL playground, every interactive widget, the rendered diagrams,
///   and full cross-link fidelity.
/// * **Mdbook** — drops `{SearchIndex, LiveSparql, Interactivity}`. The book has
///   no bundled full-text index of our making, no live SPARQL endpoint, and no
///   query playground / interactive surfaces; it keeps the rendered diagrams
///   (SVGs render inline) and cross-link fidelity (chapter links resolve, and
///   dropped-surface links are externalized to the published site, never
///   dangling).
/// * **Pdf** — drops `{SearchIndex, LiveSparql, Interactivity, Diagrams,
///   CrossLinkFidelity}`. This print PDF renders term text, tables, and the
///   bibliography. It embeds **no** diagrams, exposes no search index or live
///   SPARQL, has no interactive surfaces, and its cross-references are prose (not
///   live resolvable links), so cross-link fidelity is a declared loss. The set
///   is deliberately conservative: a capability is listed as representable only
///   when the PDF genuinely renders it.
/// * **Snippets** — drops the same five as the PDF. Flat per-term Markdown blocks
///   carry no cross-links, no search, no diagrams, no live SPARQL, and no
///   interactivity.
///
/// The chain is monotone in dropped capabilities: `site ⊆ mdbook ⊆ pdf ⊆
/// snippets`.
pub fn format_capabilities(fmt: DocFormat) -> FormatCapabilities {
    let dropped: Vec<Capability> = match fmt {
        DocFormat::Site => Vec::new(),
        DocFormat::Mdbook => vec![
            Capability::SearchIndex,
            Capability::LiveSparql,
            Capability::Interactivity,
        ],
        DocFormat::Pdf | DocFormat::Snippets => vec![
            Capability::SearchIndex,
            Capability::LiveSparql,
            Capability::Interactivity,
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

    /// The declared-loss chain is monotone: `site ⊆ mdbook ⊆ pdf ⊆ snippets` in
    /// dropped capabilities. Nothing a richer format drops is regained by a
    /// poorer one.
    #[test]
    fn dropped_capabilities_are_monotone_along_the_chain() {
        use std::collections::BTreeSet;
        let dropped = |fmt| -> BTreeSet<Capability> {
            format_capabilities(fmt).dropped.into_iter().collect()
        };
        let site = dropped(DocFormat::Site);
        let mdbook = dropped(DocFormat::Mdbook);
        let pdf = dropped(DocFormat::Pdf);
        let snippets = dropped(DocFormat::Snippets);

        assert!(site.is_subset(&mdbook), "site ⊄ mdbook");
        assert!(mdbook.is_subset(&pdf), "mdbook ⊄ pdf");
        assert!(pdf.is_subset(&snippets), "pdf ⊄ snippets");

        // The concrete expected sets, pinned so a future edit that breaks
        // monotonicity fails loudly here too.
        assert!(site.is_empty());
        assert_eq!(mdbook.len(), 3);
        assert_eq!(pdf.len(), 5);
        assert_eq!(snippets, pdf);
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
