// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::formats::{DocFormat, PROJECTION_DAG_EDGES, surface_capabilities};

fn surface(s: DistributionSurface) -> usize {
    surface_index(s)
}

/// The mask widths are derived from the authored vocabularies, never hand-sized.
#[test]
fn mask_widths_are_derived_from_the_capability_bearing_surfaces() {
    assert_eq!(SURFACE_COUNT, DocFormat::ALL.len() + 1);
    assert_eq!(SURFACE_COUNT, 5);
    assert_eq!(CAPABILITY_COUNT, 6);
    assert_eq!(ALL_SURFACES.0.count_ones() as usize, SURFACE_COUNT);
    assert_eq!(ALL_CAPS.0.count_ones() as usize, CAPABILITY_COUNT);
}

/// The derived incidence agrees, cell for cell, with the owned partition the rest of
/// the codebase reads — one authority, two encodings.
#[test]
fn the_incidence_masks_agree_with_the_owned_partitions() {
    for (index, surface) in DistributionSurface::ALL.into_iter().enumerate() {
        let owned = surface_capabilities(surface);
        assert_eq!(
            AUTHORED_INCIDENCE[index].members(),
            owned.representable,
            "{surface:?} representable mask disagrees with the owned partition"
        );
        let dropped = CapMask(ALL_CAPS.0 & !AUTHORED_INCIDENCE[index].0);
        assert_eq!(
            dropped.members(),
            owned.dropped,
            "{surface:?} dropped mask disagrees with the owned partition"
        );
    }
}

/// The lattice has exactly the concepts the authored intents admit. The intents form a
/// chain (site = all six, mdbook = five, console = four, pdf = snippets = none), so the
/// lattice is the four-element chain of those distinct closed sets.
#[test]
fn the_authored_lattice_has_exactly_four_concepts() {
    let nodes = authored_concepts();
    assert_eq!(
        nodes.len(),
        4,
        "{:?}",
        nodes.iter().map(|c| concept_slugs(*c)).collect::<Vec<_>>()
    );

    let rendered: Vec<(Vec<&str>, Vec<&str>)> = nodes.iter().map(|c| concept_slugs(*c)).collect();
    assert_eq!(
        rendered,
        vec![
            (
                vec!["site"],
                vec![
                    "search-index",
                    "live-sparql",
                    "interactivity",
                    "live-reasoning",
                    "diagrams",
                    "cross-link-fidelity"
                ]
            ),
            (
                vec!["site", "mdbook"],
                vec![
                    "live-sparql",
                    "interactivity",
                    "live-reasoning",
                    "diagrams",
                    "cross-link-fidelity"
                ]
            ),
            (
                vec!["site", "mdbook", "console"],
                vec!["live-sparql", "interactivity", "live-reasoning", "diagrams"]
            ),
            (vec!["site", "mdbook", "pdf", "snippets", "console"], vec![]),
        ]
    );
}

/// Every emitted node really is a formal concept: `extent′ = intent` and
/// `intent′ = extent`.
#[test]
fn every_emitted_node_is_galois_closed() {
    for node in authored_concepts() {
        assert_eq!(
            intent_of(node.extent, &AUTHORED_INCIDENCE),
            node.intent,
            "{node:?} extent′ ≠ intent"
        );
        assert_eq!(
            extent_of(node.intent, &AUTHORED_INCIDENCE),
            node.extent,
            "{node:?} intent′ ≠ extent"
        );
    }
}

/// ACCEPTANCE 6: `BOTTOM` and `TOP` satisfy the bounded-lattice laws over the EMITTED
/// concept set — exhaustively, because the carrier is a four-element finite set.
#[test]
fn bounded_lattice_laws_hold_over_the_emitted_concept_set() {
    let nodes = authored_concepts();

    // Trap 1: the bottom is `(M′, M)` = ({site}, ALL_CAPS), NOT (∅, ALL_CAPS).
    assert_eq!(SurfaceConcept::BOTTOM.intent, ALL_CAPS);
    assert_eq!(
        SurfaceConcept::BOTTOM.extent,
        SurfaceMask(1u16 << surface(DistributionSurface::Format(DocFormat::Site))),
        "the bottom concept's extent is the representable-total surface set, not ∅"
    );
    assert_ne!(
        SurfaceConcept::BOTTOM.extent,
        SurfaceMask(0),
        "(∅, ALL_CAPS) is not a formal concept and would break the least-element law"
    );

    // Trap 2: the top's extent is EVERY capability-bearing surface, and its intent is
    // computed (∅ here), so it is reachable and not a hand-sized constant.
    assert_eq!(SurfaceConcept::TOP.extent, ALL_SURFACES);
    assert_eq!(SurfaceConcept::TOP.intent, NO_CAPS);

    // Both bounds are members of the emitted set — a bound outside it is meaningless.
    assert!(
        nodes.contains(&SurfaceConcept::BOTTOM),
        "BOTTOM is not one of the emitted concepts"
    );
    assert!(
        nodes.contains(&SurfaceConcept::TOP),
        "TOP is not one of the emitted concepts"
    );

    for a in &nodes {
        // Least / greatest element laws.
        assert_eq!(
            SurfaceConcept::BOTTOM.join(*a),
            *a,
            "BOTTOM ∨ {a:?} ≠ {a:?}"
        );
        assert_eq!(SurfaceConcept::BOTTOM.meet(*a), SurfaceConcept::BOTTOM);
        assert_eq!(SurfaceConcept::TOP.meet(*a), *a, "TOP ∧ {a:?} ≠ {a:?}");
        assert_eq!(SurfaceConcept::TOP.join(*a), SurfaceConcept::TOP);
        assert!(SurfaceConcept::BOTTOM.leq(*a) && a.leq(SurfaceConcept::TOP));

        // Idempotence.
        assert_eq!(a.join(*a), *a);
        assert_eq!(a.meet(*a), *a);

        for b in &nodes {
            // Closure: the lattice operations never leave the emitted set.
            assert!(
                nodes.contains(&a.join(*b)),
                "{a:?} ∨ {b:?} left the lattice"
            );
            assert!(
                nodes.contains(&a.meet(*b)),
                "{a:?} ∧ {b:?} left the lattice"
            );
            // Commutativity + absorption.
            assert_eq!(a.join(*b), b.join(*a));
            assert_eq!(a.meet(*b), b.meet(*a));
            assert_eq!(a.join(a.meet(*b)), *a);
            assert_eq!(a.meet(a.join(*b)), *a);
            // The order agrees on both sides.
            assert_eq!(a.leq(*b), a.meet(*b) == *a);
            assert_eq!(a.leq(*b), a.extent.is_subset_of(b.extent));
            for c in &nodes {
                assert_eq!(a.join(b.join(*c)), a.join(*b).join(*c));
                assert_eq!(a.meet(b.meet(*c)), a.meet(*b).meet(*c));
            }
        }
    }
}

/// The surface order is not a second definition: it IS the concept order restricted to
/// the object concepts `γ(g)`.
#[test]
fn surface_leq_is_the_object_concept_order() {
    for lesser in DistributionSurface::ALL {
        for greater in DistributionSurface::ALL {
            let by_definition = surface_leq(lesser, greater, &AUTHORED_INCIDENCE);
            let by_lattice = SurfaceConcept::of_surface(lesser, &AUTHORED_INCIDENCE)
                .leq(SurfaceConcept::of_surface(greater, &AUTHORED_INCIDENCE));
            assert_eq!(
                by_definition, by_lattice,
                "{lesser:?} ≤ {greater:?}: definition {by_definition} vs lattice {by_lattice}"
            );
        }
    }
}

/// ACCEPTANCE 4: the DERIVED order reproduces every hand-declared provenance edge and
/// the capability chain, with BOTH strictness directions on the console's two
/// neighbours.
#[test]
fn the_derived_order_reproduces_every_declared_edge_and_chain() {
    // Every declared PROVENANCE covering edge is also a capability-order relation.
    for &(src, tgt) in PROJECTION_DAG_EDGES {
        assert!(
            surface_leq(
                DistributionSurface::Format(src),
                DistributionSurface::Format(tgt),
                &AUTHORED_INCIDENCE
            ),
            "declared DAG edge {src:?} → {tgt:?} is not reproduced by the derived order"
        );
    }

    let site = DistributionSurface::Format(DocFormat::Site);
    let mdbook = DistributionSurface::Format(DocFormat::Mdbook);
    let pdf = DistributionSurface::Format(DocFormat::Pdf);
    let snippets = DistributionSurface::Format(DocFormat::Snippets);
    let console = DistributionSurface::Console;

    // The full authored chain, derived.
    for (lower, upper) in [
        (site, mdbook),
        (mdbook, console),
        (console, pdf),
        (pdf, snippets),
    ] {
        assert!(
            surface_leq(lower, upper, &AUTHORED_INCIDENCE),
            "{lower:?} ≤ {upper:?} is not derived"
        );
    }
    // pdf and snippets are order-EQUIVALENT (identical partitions), not strict.
    assert!(surface_leq(snippets, pdf, &AUTHORED_INCIDENCE));

    // dropped(mdbook) ⊊ dropped(console) ⊊ dropped(pdf), both strictness directions.
    let dropped = |s: DistributionSurface| CapMask(ALL_CAPS.0 & !AUTHORED_INCIDENCE[surface(s)].0);
    assert!(dropped(mdbook).is_subset_of(dropped(console)));
    assert!(!dropped(console).is_subset_of(dropped(mdbook)));
    assert_ne!(dropped(mdbook), dropped(console));
    assert!(dropped(console).is_subset_of(dropped(pdf)));
    assert!(!dropped(pdf).is_subset_of(dropped(console)));
    assert_ne!(dropped(console), dropped(pdf));
}

/// ACCEPTANCE 5, the perturbation negative test: flipping ONE incidence cell changes the
/// derived order. Giving the pdf a bundled `SearchIndex` — the one capability the
/// console drops — makes the two incomparable, breaks the `console ≤ pdf` chain link,
/// and moves the concept lattice, its basis, and its Hasse diagram.
#[test]
fn flipping_one_incidence_cell_changes_the_derived_order() {
    let mut perturbed = AUTHORED_INCIDENCE;
    let pdf = surface_index(DistributionSurface::Format(DocFormat::Pdf));
    assert!(
        perturbed[pdf].0 & (1u8 << Capability::SearchIndex.index()) == 0,
        "the pdf must not already carry a search index, or this test proves nothing"
    );
    perturbed[pdf] = CapMask(perturbed[pdf].0 | (1u8 << Capability::SearchIndex.index()));

    // The order fact that HELD under the authored incidence now FAILS.
    assert!(surface_leq(
        DistributionSurface::Console,
        DistributionSurface::Format(DocFormat::Pdf),
        &AUTHORED_INCIDENCE
    ));
    assert!(
        !surface_leq(
            DistributionSurface::Console,
            DistributionSurface::Format(DocFormat::Pdf),
            &perturbed
        ),
        "flipping pdf/search-index must break console ≤ pdf — otherwise the order is \
             not derived from the incidence at all"
    );
    // …and the reverse does not silently take its place: the two become incomparable.
    assert!(!surface_leq(
        DistributionSurface::Format(DocFormat::Pdf),
        DistributionSurface::Console,
        &perturbed
    ));

    // The lattice itself moves: the concept set and the DG basis both change.
    assert_ne!(
        concepts(&perturbed),
        authored_concepts(),
        "a flipped cell must move the concept lattice"
    );
    assert_ne!(
        dg_basis(&perturbed),
        authored_dg_basis(),
        "a flipped cell must move the implication basis"
    );
    assert_ne!(
        concept_hasse_edges(&perturbed),
        concept_hasse_edges(&AUTHORED_INCIDENCE),
        "a flipped cell must move the Hasse diagram the console renders"
    );
}

/// A concept of the perturbed context, built exactly as the perturbation test builds
/// its lattice: `({pdf}″, {pdf}′)` where the pdf has been given a bundled search index.
fn foreign_context_concept() -> (SurfaceConcept, [CapMask; SURFACE_COUNT]) {
    let mut perturbed = AUTHORED_INCIDENCE;
    let pdf = surface_index(DistributionSurface::Format(DocFormat::Pdf));
    perturbed[pdf] = CapMask(perturbed[pdf].0 | (1u8 << Capability::SearchIndex.index()));
    let concept =
        SurfaceConcept::of_surface(DistributionSurface::Format(DocFormat::Pdf), &perturbed);
    (concept, perturbed)
}

/// The formal CONTEXT of a concept is decidable, not assumed: a concept of the
/// perturbed incidence is provably not one of the authored incidence.
#[test]
fn a_perturbed_context_concept_is_not_a_concept_of_the_authored_context() {
    let (foreign, perturbed) = foreign_context_concept();
    assert!(
        foreign.is_closed_under(&perturbed),
        "{foreign:?} must be Galois-closed in the context it was derived from"
    );
    assert!(
        !foreign.is_closed_under(&AUTHORED_INCIDENCE),
        "{foreign:?} must NOT be a concept of the authored context, or this test — and \
             the refusal it backs — proves nothing"
    );
}

/// …and the bounded-lattice operations REFUSE it rather than silently closing it
/// against the authored context. Before the refusal, `join` returned a well-formed
/// concept of a lattice neither operand belonged to.
#[test]
#[should_panic(expected = "is not a concept of the incidence it is being combined under")]
fn joining_a_foreign_context_concept_under_the_authored_lattice_is_refused() {
    let (foreign, _) = foreign_context_concept();
    let _ = foreign.join(SurfaceConcept::TOP);
}

/// The same for `meet` — both halves of the lattice are guarded, not just one.
#[test]
#[should_panic(expected = "is not a concept of the incidence it is being combined under")]
fn meeting_a_foreign_context_concept_under_the_authored_lattice_is_refused() {
    let (foreign, _) = foreign_context_concept();
    let _ = foreign.meet(SurfaceConcept::BOTTOM);
}

/// The explicit-context operations are the supported way to work in ANY context: over
/// the perturbed incidence, the perturbed bounds and the perturbed operations satisfy
/// the same bounded-lattice laws the authored ones do.
#[test]
fn the_explicit_context_operations_form_a_lattice_over_the_perturbed_incidence() {
    let (_, perturbed) = foreign_context_concept();
    let bottom = SurfaceConcept::bottom_in(&perturbed);
    let top = SurfaceConcept::top_in(&perturbed);
    let nodes = concepts(&perturbed);
    assert!(nodes.contains(&bottom) && nodes.contains(&top));
    for a in &nodes {
        assert_eq!(bottom.join_in(*a, &perturbed), *a);
        assert_eq!(top.meet_in(*a, &perturbed), *a);
        for b in &nodes {
            assert!(nodes.contains(&a.join_in(*b, &perturbed)));
            assert!(nodes.contains(&a.meet_in(*b, &perturbed)));
            assert_eq!(a.join_in(*b, &perturbed), b.join_in(*a, &perturbed));
            assert_eq!(a.meet_in(*b, &perturbed), b.meet_in(*a, &perturbed));
        }
    }
}

/// The DG basis is COMPLETE and SOUND over the authored incidence: every implication
/// holds in the context, and iterating the basis to a fixpoint reproduces the true
/// closure of every attribute set.
#[test]
fn the_dg_basis_is_sound_and_complete() {
    let basis = authored_dg_basis();
    assert!(!basis.is_empty());

    // Sound: every law holds of every surface.
    for implication in &basis {
        for intent in AUTHORED_INCIDENCE {
            if implication.premise.is_subset_of(intent) {
                assert!(
                    implication.conclusion.is_subset_of(intent),
                    "{implication:?} is violated by a surface with intent {intent:?}"
                );
            }
        }
    }

    // Complete: basis-closure == Galois closure, for every attribute set.
    for bits in 0u16..(1u16 << CAPABILITY_COUNT) {
        let start = CapMask(bits as u8);
        let mut current = start;
        loop {
            let mut next = current;
            for implication in &basis {
                if implication.premise.is_subset_of(next) {
                    next = CapMask(next.0 | implication.conclusion.0);
                }
            }
            if next == current {
                break;
            }
            current = next;
        }
        let galois = intent_of(extent_of(start, &AUTHORED_INCIDENCE), &AUTHORED_INCIDENCE);
        assert_eq!(
            current, galois,
            "basis closure of {start:?} disagrees with the Galois closure"
        );
    }
}

/// The authored basis, pinned by its rendered content so a silent incidence edit is
/// legible in the diff rather than a bare count change.
#[test]
fn the_authored_dg_basis_is_the_six_singleton_laws() {
    let rendered: Vec<(Vec<&str>, Vec<&str>)> = authored_dg_basis()
        .into_iter()
        .map(|i| {
            (
                i.premise.members().iter().map(|c| c.slug()).collect(),
                i.conclusion.members().iter().map(|c| c.slug()).collect(),
            )
        })
        .collect();
    assert_eq!(
        rendered,
        vec![
            (
                vec!["search-index"],
                vec![
                    "live-sparql",
                    "interactivity",
                    "live-reasoning",
                    "diagrams",
                    "cross-link-fidelity"
                ]
            ),
            (
                vec!["live-sparql"],
                vec!["interactivity", "live-reasoning", "diagrams"]
            ),
            (
                vec!["interactivity"],
                vec!["live-sparql", "live-reasoning", "diagrams"]
            ),
            (
                vec!["live-reasoning"],
                vec!["live-sparql", "interactivity", "diagrams"]
            ),
            (
                vec!["diagrams"],
                vec!["live-sparql", "interactivity", "live-reasoning"]
            ),
            (
                vec!["cross-link-fidelity"],
                vec!["live-sparql", "interactivity", "live-reasoning", "diagrams"]
            ),
        ]
    );
}

/// Over the AUTHORED incidence every law is witnessed by at least one surface, so the
/// unrealized subset is empty. The predicate is nonetheless live: a context in which a
/// premise has an empty extent marks its law as a vacuous-truth boundary.
#[test]
fn unrealized_laws_are_the_vacuously_true_ones() {
    for implication in authored_dg_basis() {
        assert!(
            !implication.is_unrealized(&AUTHORED_INCIDENCE),
            "{implication:?} is vacuous over the authored incidence — every authored law \
                 must have a witnessing surface"
        );
    }

    // A synthetic context where a premise IS unwitnessed: two surfaces that split the
    // attributes, so `{search-index, live-sparql}` has an empty extent.
    let split = [
        CapMask(1 << Capability::SearchIndex.index()),
        CapMask(1 << Capability::LiveSparql.index()),
        CapMask(0),
        CapMask(0),
        CapMask(0),
    ];
    let unrealized: Vec<Implication> = dg_basis(&split)
        .into_iter()
        .filter(|i| i.is_unrealized(&split))
        .collect();
    assert!(
        !unrealized.is_empty(),
        "the vacuous-law predicate must be reachable, or it is dead branch"
    );
    for implication in unrealized {
        assert_eq!(extent_of(implication.premise, &split), SurfaceMask(0));
    }
}

/// The Hasse edges of the authored four-element chain.
#[test]
fn the_authored_hasse_diagram_is_the_three_chain_edges() {
    let edges = concept_hasse_edges(&AUTHORED_INCIDENCE);
    assert_eq!(edges.len(), 3, "{edges:?}");
    for (lower, upper) in &edges {
        assert!(lower.extent.is_subset_of(upper.extent));
        assert_ne!(lower, upper);
    }
}

#[test]
fn intents_by_slug_reports_every_surface() {
    let map = intents_by_slug(&AUTHORED_INCIDENCE);
    assert_eq!(map.len(), SURFACE_COUNT);
    assert_eq!(map["pdf"], Vec::<&str>::new());
    assert_eq!(map["console"].len(), 4);
}
