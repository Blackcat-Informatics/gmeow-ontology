// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
    let dropped =
        |fmt| -> BTreeSet<Capability> { format_capabilities(fmt).dropped.into_iter().collect() };
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
    let dropped =
        |fmt| -> BTreeSet<Capability> { format_capabilities(fmt).dropped.into_iter().collect() };
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
    let surface_slugs: BTreeSet<&str> = DistributionSurface::ALL.iter().map(|s| s.slug()).collect();
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
