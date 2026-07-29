// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single per-surface capability-loss table.
//!
//! Every documentation projection (the static site, the mdbook, the print PDF,
//! the flat per-term snippets) renders the same underlying
//! [`crate::model::DocsModel`] but can
//! only carry a subset of the site's live surfaces; so does the standalone interactive
//! console, which is a capability-bearing surface without being a shipped distribution.
//! This module is the ONE source of truth for which of the six cross-cutting capabilities
//! each surface preserves and which it declares lost. Both the renderers (the PDF loss
//! appendix in `docs-print`) and the pipeline grounding read this table, so a
//! format's loss appendix matches the graph loss ledger *by construction* — the
//! two can never drift because they are the same data.
//!
//! The formal-concept lattice DERIVED from this table (its order, meet, join, bounds, and
//! Duquenne–Guigues implication basis) lives in [`crate::surface_lattice`]; nothing there
//! re-authors an incidence cell.
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
//! body-set — NEITHER is rendered from the other — so this PROVENANCE order carries no
//! edge between them, nor from the site to either. That is **provenance**-incomparability
//! and nothing more: on the CAPABILITY lattice ([`crate::surface_lattice`]) the two are
//! perfectly comparable, with
//!
//! ```text
//! dropped(site) ⊆ dropped(mdbook) ⊆ dropped(pdf) = dropped(snippets)
//! ```
//!
//! because mdbook packs the live engines the pdf cannot. The two orders are distinct and
//! neither is a function of the other — see the [`PROJECTION_DAG_EDGES`] doc comment,
//! which owns that distinction, and
//! `dropped_sets_form_the_capability_refinement_chain`, which proves the chain.
//! [`format_capabilities`] realizes the per-node partition; the DAG-edge monotonicity test
//! in this module gates the provenance half and the capability-chain test gates the other.
//! Nothing a format drops is ever regained by a strictly-poorer format downstream of it.
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

/// A **distribution surface**: anything that carries a capability partition.
///
/// This is the object set of the `Surface × Capability` formal context
/// ([`crate::surface_lattice`]). It is the four rendered [`DocFormat`] distributions PLUS
/// the standalone interactive console, which is a capability-bearing surface but NOT one
/// of the eight shipped documentation distributions — it renders the ontology live in a
/// browser from the bundle rather than being a distributed artifact of its own.
///
/// The serialization distributions (`okf`, `jsonld`, `yamlld`, `pydantic`) are deliberately
/// absent: they are structured re-serializations, not prose renderings, and carry no
/// capability partition at all. A mask sized to the whole distribution catalog rather than
/// to this set would leave the lattice's greatest element unreachable by any join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DistributionSurface {
    /// One of the four rendered documentation formats.
    Format(DocFormat),
    /// The standalone interactive console: a live in-browser surface over the bundle. It
    /// runs SPARQL and the structured-DL chase against the vendored wasm engines, is
    /// interactive by construction, and renders derived diagrams (including the Hasse
    /// diagram of this very lattice). It ships no bundled full-text index and its term
    /// references are console commands rather than resolvable links.
    Console,
}

impl DistributionSurface {
    /// Every capability-bearing surface: the four [`DocFormat::ALL`] renderings plus the
    /// console. The length is *derived* from [`DocFormat::ALL`] so a new format cannot be
    /// added without widening this set (and, with it, the lattice's surface mask).
    pub const ALL: [DistributionSurface; DocFormat::ALL.len() + 1] = [
        DistributionSurface::Format(DocFormat::Site),
        DistributionSurface::Format(DocFormat::Mdbook),
        DistributionSurface::Format(DocFormat::Pdf),
        DistributionSurface::Format(DocFormat::Snippets),
        DistributionSurface::Console,
    ];

    /// The stable machine slug (kebab-case) identifying this surface. For a rendered
    /// format it IS the format's slug, so the catalog's distribution slugs and the
    /// lattice's surface slugs are the same strings by construction.
    pub fn slug(&self) -> &'static str {
        match self {
            DistributionSurface::Format(fmt) => fmt.slug(),
            DistributionSurface::Console => "console",
        }
    }

    /// A human-readable label for this surface.
    pub fn label(&self) -> &'static str {
        match self {
            DistributionSurface::Format(fmt) => fmt.label(),
            DistributionSurface::Console => "Interactive console",
        }
    }

    /// The capabilities this surface declares lost, in [`Capability::ALL`] order.
    ///
    /// This `const fn` is the SINGLE authored incidence table — [`surface_capabilities`]
    /// derives its owned partition from it, and [`crate::surface_lattice`] derives the
    /// `const` bit masks its [`gmeow_errors::grade::BoundedLattice`] bounds are built from.
    /// A `Vec`-returning authority could not be read in a `const` context, so the bounds
    /// would have had to be hand-written — exactly the second source of truth this avoids.
    ///
    /// The reasoning behind each set:
    ///
    /// * **Site** — drops nothing. The live HTML site carries the search index, the live
    ///   SPARQL playground, every interactive widget, the in-browser reasoner + GMN
    ///   transcode, the rendered diagrams, and full cross-link fidelity.
    /// * **Mdbook** — drops `{SearchIndex}` only. The book packs the SAME vendored wasm
    ///   engines + controller the site does (validate / purrdf-SPARQL / reason / GMN) and an
    ///   interactive host chapter, wired through `book.toml` `additional-js`, so once built
    ///   it carries live SPARQL, the interactive widgets, and the in-browser reasoning +
    ///   transcode — plus the rendered diagrams (SVGs render inline) and cross-link fidelity
    ///   (chapter links resolve; dropped-surface links externalize to the published site,
    ///   never dangling). It keeps no bundled full-text index of our making (mdbook ships
    ///   its own client search), so `SearchIndex` stays a declared loss.
    /// * **Pdf** — drops all six. This print PDF renders term text, tables, and the
    ///   bibliography. It embeds **no** diagrams, exposes no search index or live SPARQL,
    ///   has no interactive surfaces or in-browser reasoning, and its cross-references are
    ///   prose (not live resolvable links). The set is deliberately conservative: a
    ///   capability is representable only when the PDF genuinely renders it.
    /// * **Snippets** — drops the same six as the PDF. Flat per-term Markdown blocks carry
    ///   no cross-links, no search, no diagrams, no live SPARQL, no interactivity, and no
    ///   in-browser reasoning.
    /// * **Console** — drops `{SearchIndex, CrossLinkFidelity}`. It runs the vendored
    ///   engines, so live SPARQL, interactivity, and live reasoning are all genuinely
    ///   present, and `Diagrams` is REPRESENTABLE: the console renders the derived Hasse
    ///   diagram of the concept lattice (declaring `Diagrams` dropped while requiring that
    ///   diagram would be a self-contradiction). It bundles no full-text index, and a term
    ///   reference inside a console session is a command to re-query, not a link that
    ///   resolves inside the artifact.
    pub const fn dropped(self) -> &'static [Capability] {
        match self {
            DistributionSurface::Format(DocFormat::Site) => &[],
            DistributionSurface::Format(DocFormat::Mdbook) => &[Capability::SearchIndex],
            DistributionSurface::Format(DocFormat::Pdf)
            | DistributionSurface::Format(DocFormat::Snippets) => &[
                Capability::SearchIndex,
                Capability::LiveSparql,
                Capability::Interactivity,
                Capability::LiveReasoning,
                Capability::Diagrams,
                Capability::CrossLinkFidelity,
            ],
            DistributionSurface::Console => {
                &[Capability::SearchIndex, Capability::CrossLinkFidelity]
            }
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

    /// This capability's position in [`Capability::ALL`] — the bit index the
    /// [`crate::surface_lattice`] intent mask encodes it at. A `const fn` so the lattice's
    /// bounds can be computed in a `const` context from the authored incidence.
    pub const fn index(self) -> usize {
        match self {
            Capability::SearchIndex => 0,
            Capability::LiveSparql => 1,
            Capability::Interactivity => 2,
            Capability::LiveReasoning => 3,
            Capability::Diagrams => 4,
            Capability::CrossLinkFidelity => 5,
        }
    }

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
///
/// # Two distinct orders, neither derived from the other
///
/// This constant is the **provenance** order — WHICH artifact is rendered from WHICH — and
/// it is HAND-DECLARED, mirroring the pipeline's composition legs. [`crate::surface_lattice`]
/// carries the **capability** order — WHICH surface represents MORE — and it is DERIVED
/// from the `Surface × Capability` formal context, never enumerated. Neither order is a
/// function of the other: mdbook and pdf are capability-comparable but provenance-
/// incomparable, and the console is capability-comparable to both while appearing in no
/// provenance edge at all. Because this edge set is a SINGLE edge, replacing the deleted
/// hand chain with a "reproduce PROJECTION_DAG_EDGES" check would be near-vacuous — so both
/// proofs are kept, separately gated, and each fails on its own regression.
pub const PROJECTION_DAG_EDGES: &[(DocFormat, DocFormat)] =
    &[(DocFormat::Site, DocFormat::Snippets)];

/// One surface's capability partition: which capabilities it represents and which it
/// declares lost. Both vectors are sorted (by [`Capability`]'s derived order) and are a
/// total, disjoint partition of [`Capability::ALL`].
///
/// This is the ONLY capability-partition type. There is no separate format-only partition:
/// a `DocFormat` is simply a [`DistributionSurface::Format`], and [`format_capabilities`]
/// is a thin wrapper over [`surface_capabilities`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceCapabilities {
    /// The surface this partition describes.
    pub surface: DistributionSurface,
    /// The capabilities this surface faithfully represents (sorted).
    pub representable: Vec<Capability>,
    /// The capabilities this surface declares lost (sorted).
    pub dropped: Vec<Capability>,
}

impl SurfaceCapabilities {
    /// The rendered format this partition describes, when the surface is one — `None` for
    /// the console, which is a capability-bearing surface but not a shipped format.
    pub fn format(&self) -> Option<DocFormat> {
        match self.surface {
            DistributionSurface::Format(fmt) => Some(fmt),
            DistributionSurface::Console => None,
        }
    }
}

/// The honest per-surface capability partition, derived from the single authored incidence
/// table [`DistributionSurface::dropped`]. This is the single authority the loss appendix,
/// the graph loss ledger, the distribution catalog, and the concept lattice all read.
///
/// Dropped sets are monotone along the projection DAG's covering edges
/// ([`PROJECTION_DAG_EDGES`]): `dropped(site) ⊆ dropped(snippets)`. Separately — and gated
/// by its own test — the dropped sets form a capability-refinement chain
/// `dropped(site) ⊆ dropped(mdbook) ⊊ dropped(console) ⊊ dropped(pdf) = dropped(snippets)`.
/// Both properties are machine-checked below, and the SECOND is re-derived from the formal
/// context rather than asserted in [`crate::surface_lattice`].
pub fn surface_capabilities(surface: DistributionSurface) -> SurfaceCapabilities {
    let dropped: Vec<Capability> = surface.dropped().to_vec();
    let representable: Vec<Capability> = Capability::ALL
        .into_iter()
        .filter(|c| !dropped.contains(c))
        .collect();

    // Both partitions sorted by the derived Capability order (ALL is already in that
    // order, and each authored arm lists in that order); asserted in the test.
    SurfaceCapabilities {
        surface,
        representable,
        dropped,
    }
}

/// The per-FORMAT capability partition — a thin wrapper over [`surface_capabilities`] for
/// the four rendered distributions. Every caller that speaks in `DocFormat` reads the same
/// one table the console reads.
pub fn format_capabilities(fmt: DocFormat) -> SurfaceCapabilities {
    surface_capabilities(DistributionSurface::Format(fmt))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every capability appears in exactly one of `representable` / `dropped`, for every
    /// SURFACE — the console included, not just the four rendered formats — a total,
    /// disjoint partition, with both sides sorted.
    #[test]
    fn partition_is_total_and_disjoint() {
        for surface in DistributionSurface::ALL {
            let caps = surface_capabilities(surface);
            assert_eq!(caps.surface, surface);

            // Sorted.
            let mut sorted_repr = caps.representable.clone();
            sorted_repr.sort();
            assert_eq!(
                sorted_repr, caps.representable,
                "{surface:?} representable unsorted"
            );
            let mut sorted_drop = caps.dropped.clone();
            sorted_drop.sort();
            assert_eq!(sorted_drop, caps.dropped, "{surface:?} dropped unsorted");

            // Total + disjoint: each capability is in exactly one side.
            for cap in Capability::ALL {
                let in_repr = caps.representable.contains(&cap);
                let in_drop = caps.dropped.contains(&cap);
                assert!(
                    in_repr ^ in_drop,
                    "{surface:?}/{cap:?}: must be in exactly one of representable/dropped"
                );
            }
            assert_eq!(
                caps.representable.len() + caps.dropped.len(),
                Capability::ALL.len(),
                "{surface:?}: partition size mismatch"
            );
        }
    }

    /// The surface set is exactly the four formats plus the console, and
    /// `format_capabilities` is genuinely a wrapper (never a second table).
    #[test]
    fn every_format_is_a_surface_and_the_console_is_the_only_extra() {
        assert_eq!(DocFormat::ALL.len(), 4);
        assert_eq!(DistributionSurface::ALL.len(), DocFormat::ALL.len() + 1);
        for fmt in DocFormat::ALL {
            assert!(
                DistributionSurface::ALL.contains(&DistributionSurface::Format(fmt)),
                "{fmt:?} is not a distribution surface"
            );
            assert_eq!(
                format_capabilities(fmt),
                surface_capabilities(DistributionSurface::Format(fmt)),
                "format_capabilities must be a wrapper over surface_capabilities"
            );
            assert_eq!(format_capabilities(fmt).format(), Some(fmt));
        }
        assert!(DistributionSurface::ALL.contains(&DistributionSurface::Console));
        assert_eq!(
            surface_capabilities(DistributionSurface::Console).format(),
            None,
            "the console is a surface but NOT one of the shipped distributions"
        );
    }

    /// The console's authored incidence, pinned. `Diagrams` is REPRESENTABLE: the console
    /// renders the derived Hasse diagram of the concept lattice, so declaring it dropped
    /// while requiring that diagram would be a self-contradiction.
    #[test]
    fn the_console_incidence_is_the_authored_one() {
        let caps = surface_capabilities(DistributionSurface::Console);
        assert_eq!(
            caps.representable,
            vec![
                Capability::LiveSparql,
                Capability::Interactivity,
                Capability::LiveReasoning,
                Capability::Diagrams,
            ]
        );
        assert_eq!(
            caps.dropped,
            vec![Capability::SearchIndex, Capability::CrossLinkFidelity]
        );
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
        // The console sits STRICTLY between mdbook and the pdf on this chain: it drops
        // cross-link fidelity the packed mdbook keeps, and keeps the live engines and the
        // rendered diagrams the pdf cannot carry. Both strictness directions are checked,
        // so a console that silently collapsed onto either neighbour reds here.
        let console: BTreeSet<Capability> = surface_capabilities(DistributionSurface::Console)
            .dropped
            .into_iter()
            .collect();
        assert!(
            dropped(DocFormat::Mdbook).is_subset(&console) && dropped(DocFormat::Mdbook) != console,
            "capability lattice: dropped(mdbook) ⊊ dropped(console) must be PROPER"
        );
        assert!(
            console.is_subset(&dropped(DocFormat::Pdf)) && console != dropped(DocFormat::Pdf),
            "capability lattice: dropped(console) ⊊ dropped(pdf) must be PROPER"
        );
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
        let surface_slugs: BTreeSet<&str> =
            DistributionSurface::ALL.iter().map(|s| s.slug()).collect();
        assert_eq!(surface_slugs.len(), DistributionSurface::ALL.len());
        // A surface's slug IS its format's slug — the catalog and the lattice address the
        // same four rendered distributions by the same strings.
        for fmt in DocFormat::ALL {
            assert_eq!(DistributionSurface::Format(fmt).slug(), fmt.slug());
        }
    }

    /// `Capability::index` is the bit position the lattice masks encode, so it must be a
    /// bijection onto `0..Capability::ALL.len()` agreeing with `ALL`'s order.
    #[test]
    fn capability_index_is_the_position_in_all() {
        for (position, cap) in Capability::ALL.into_iter().enumerate() {
            assert_eq!(cap.index(), position, "{cap:?} index disagrees with ALL");
        }
    }
}
