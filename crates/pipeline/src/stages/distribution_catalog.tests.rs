// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
fn ntriples_text() -> String {
    // Building via the real entry point exercises parse_into_graph end-to-end (a
    // parse failure here would fail the test), then the string-content assertions
    // below grep the exact bytes the emitter produced (what parse_into_graph parsed).
    let _ds = build_distribution_catalog().expect("build distribution catalog");
    String::from_utf8(emit_ntriples().expect("emit catalog")).expect("utf8 n-triples")
}

#[test]
fn catalog_is_byte_reproducible() {
    let a = emit_ntriples().expect("emit catalog");
    let b = emit_ntriples().expect("emit catalog");
    assert_eq!(a, b, "distribution catalog N-Triples must be deterministic");
}

#[test]
fn every_slug_appears_as_a_distribution_format() {
    let nt = ntriples_text();
    let pred = iri(GMEOW_NS, "distributionFormat");
    for row in DISTRIBUTIONS {
        let needle = format!("<{}> \"{}\" .", pred, row.slug);
        assert!(
            nt.lines().any(|l| l.contains(&needle)),
            "missing gmeow:distributionFormat for slug {:?}",
            row.slug
        );
    }
}

/// The table's own internal consistency: slugs are unique, a row that claims a surface
/// agrees with that surface's slug, and EVERY declared surface is a row. Without this,
/// `surface_iri` could mint a subject no distribution row backs (or two rows could
/// claim the same one) and the concept lattice's extents would point at nothing.
#[test]
fn the_table_agrees_with_the_surface_vocabulary() {
    let slugs = declared_distribution_slugs();
    assert_eq!(
        slugs.len(),
        DISTRIBUTIONS.len(),
        "DISTRIBUTIONS carries a duplicate slug: {slugs:?}"
    );
    for row in DISTRIBUTIONS {
        if let Some(surface) = row.surface {
            assert_eq!(
                row.slug,
                surface.slug(),
                "row {:?} claims surface {surface:?}, whose slug disagrees",
                row.slug
            );
        }
        assert_eq!(
            row.rel_path,
            format!("dist/gmeow-docs/{}", row.slug),
            "row {:?} must ship under the shared docs-distribution base",
            row.slug
        );
    }
    // Every capability-bearing surface is a shipped distribution — which is what makes
    // `surface_iri` = `dist_iri` sound.
    for surface in DistributionSurface::ALL {
        assert!(
            slugs.contains(surface.slug()),
            "surface {surface:?} is not a declared distribution; surface_iri would mint \
                 a subject no row backs"
        );
    }
    // …and only a serialization row lacks one.
    for row in DISTRIBUTIONS {
        assert_eq!(
            row.surface.is_none(),
            row.family == Family::Serialization,
            "row {:?}: a surface-free row must be exactly a serialization row",
            row.slug
        );
    }
}

/// `Family::ALL` really is every family the table uses — the D-j guard. A row naming a
/// family absent from `ALL` would emit a dangling `gmeow:distributionFamily` IRI.
#[test]
fn every_row_family_has_an_emitted_family_node() {
    let nt = ntriples_text();
    for row in DISTRIBUTIONS {
        assert!(
            Family::ALL.contains(&row.family),
            "row {:?} names family {:?}, which is absent from Family::ALL — its family \
                 node would never be emitted",
            row.slug,
            row.family
        );
        assert!(
            nt.contains(&triple(
                &family_iri(row.family),
                RDF_TYPE,
                &iri(GMEOW_NS, "DistributionFamily")
            )),
            "family {:?} has no emitted gmeow:DistributionFamily node",
            row.family.slug()
        );
    }
    for family in Family::ALL {
        assert!(
            nt.contains(&triple(
                &family_iri(family),
                RDF_TYPE,
                &iri(GMEOW_NS, "DistributionFamily")
            )),
            "Family::ALL member {:?} emits no family node",
            family.slug()
        );
    }
}

/// `media_type_for_slug` and `declared_distribution_slugs` are FOLDS over the table,
/// not parallel copies: every row answers with its own media type, and nothing else
/// answers at all.
#[test]
fn the_by_slug_lookups_fold_over_the_table() {
    for row in DISTRIBUTIONS {
        assert_eq!(media_type_for_slug(row.slug), Some(row.media_type));
        assert_eq!(distribution_row(row.slug), Some(&row));
    }
    assert_eq!(media_type_for_slug("not-a-distribution"), None);
    assert_eq!(distribution_row("not-a-distribution"), None);
    for slug in declared_site_sub_asset_slugs() {
        assert_eq!(
            media_type_for_slug(slug),
            None,
            "sub-asset {slug:?} must not answer as a distribution"
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
        subjects.len() >= DISTRIBUTIONS.len() + Family::ALL.len(),
        "expected at least {} distributions + {} families, got {}",
        DISTRIBUTIONS.len(),
        Family::ALL.len(),
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
fn sub_assets_are_priced_digest_free_and_outside_the_bijection() {
    let nt = ntriples_text();
    let bijection = declared_distribution_slugs();
    assert_eq!(
        bijection.len(),
        9,
        "the nine-slug bijection must hold: {bijection:?}"
    );

    // Ownership is SHARED: `site`, packed `mdbook`, and `console` ship the identical
    // engine set, so all three must link to the same subjects. Omitting an owner here
    // leaves that distribution's copy of a 7 MB wasm image unpriced on the release
    // path.
    let owners = sub_asset_owner_slugs();
    assert_eq!(
        owners,
        std::collections::BTreeSet::from(["site", "mdbook", "console"]),
        "the shared sub-assets must be owned by exactly the three interactive distributions"
    );

    for slug in declared_site_sub_asset_slugs() {
        // NOT a top-level distribution — the bijection is preserved.
        assert!(
            !bijection.contains(slug),
            "sub-asset {slug:?} must NOT be a top-level distribution slug"
        );
        let node = sub_asset_iri(slug);
        // Typed as a SiteSubAsset and hung off EVERY owning distribution.
        assert!(
            nt.contains(&triple(&node, RDF_TYPE, &iri(GMEOW_NS, "SiteSubAsset"))),
            "sub-asset {slug:?} missing rdf:type gmeow:SiteSubAsset"
        );
        for owner in &owners {
            assert!(
                nt.contains(&triple(
                    &dist_iri(owner),
                    &iri(GMEOW_NS, "hasSubAsset"),
                    &node
                )),
                "{owner} distribution must declare gmeow:hasSubAsset {slug:?}"
            );
        }
        // The subject sits OUTSIDE the `dist/` namespace, so the consumer-side
        // `verify_docs_distribution` slug strip cannot mistake it for a distribution
        // directory (it used to, and hard-failed on every real release manifest).
        assert!(
            !node.starts_with(&format!("{DISTRIBUTION_BASE}dist/")),
            "sub-asset subject {node} must not live under the distribution namespace"
        );
        // Schema row present, DIGEST-FREE (no contentDigest in the carrier catalog).
        assert!(
            nt.contains(&format!(
                "<{node}> <{}>",
                iri(GMEOW_NS, "artifactMediaType")
            )),
            "sub-asset {slug:?} missing artifactMediaType"
        );
        // DIGEST-FREE: no line about this sub-asset mentions a content digest (the
        // per-release digest rides only in the dist/ instance manifest).
        assert!(
            !nt.lines()
                .any(|l| l.starts_with(&format!("<{node}>")) && l.contains("Digest")),
            "sub-asset {slug:?} must be digest-free in the carrier catalog (digests \
                 live only in the dist/ instance manifest)"
        );
    }
}

#[test]
fn doc_render_declared_loss_matches_format_capabilities_exactly() {
    let nt = ntriples_text();
    for fmt in DocFormat::ALL {
        let slug = fmt.slug();
        let dist = dist_iri(slug);
        let caps = surface_capabilities(DistributionSurface::Format(fmt));
        for cap in Capability::ALL {
            let loss_node = loss_iri(slug, cap.slug());
            let declares = nt.contains(&triple(&dist, &iri(GMEOW_NS, "declaredLoss"), &loss_node));
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
                        &capability_iri(cap)
                    )),
                    "{slug}/{:?} loss node missing accountsForParameter",
                    cap
                );
            }
        }
    }
}

/// The console is the NINTH shipped distribution: a full catalog row (family, media
/// type, audience, format slug) whose capability ledger is DERIVED from the surface
/// lattice, exactly like `site`'s — not authored anywhere in this module.
#[test]
fn the_console_is_the_ninth_distribution_with_a_derived_ledger() {
    let nt = ntriples_text();
    let console = dist_iri("console");
    let row = distribution_row("console").expect("the console is a declared distribution");
    assert_eq!(row.family, Family::InteractiveRuntime);
    assert_eq!(row.media_type, "text/html");
    assert_eq!(row.consumer, "consumerInteractiveConsole");
    assert_eq!(row.surface, Some(DistributionSurface::Console));

    assert!(
        nt.contains(&triple(
            &console,
            RDF_TYPE,
            &iri(GMEOW_NS, "DocumentationDistribution")
        )),
        "the console must be typed as a shipped distribution"
    );
    assert!(
        nt.contains(&triple_lit(
            &console,
            &iri(GMEOW_NS, "distributionFormat"),
            "console"
        )),
        "the console must carry its distribution format slug"
    );
    assert!(
        nt.contains(&triple(
            &console,
            &iri(GMEOW_NS, "distributionFamily"),
            &family_iri(Family::InteractiveRuntime)
        )),
        "the console must belong to the interactive-runtime family"
    );
    assert!(
        nt.contains(&triple_lit(
            &console,
            &iri(GMEOW_NS, "artifactMediaType"),
            "text/html"
        )),
        "the console must declare its media type"
    );
    assert!(
        nt.contains(&triple(
            &console,
            &iri(GMEOW_NS, "eligibleForConsumer"),
            &consumer_iri("consumerInteractiveConsole")
        )),
        "the console must name its declared audience"
    );
    assert_eq!(
        declared_distribution_slugs().len(),
        9,
        "the nine-slug bijection must include the console"
    );

    // Its ledger is the DERIVED partition, both halves — read from the same authority
    // `site` and `pdf` read, never restated in this table.
    let caps = surface_capabilities(DistributionSurface::Console);
    assert!(
        !caps.dropped.is_empty() && !caps.representable.is_empty(),
        "a vacuous console partition would make this gate meaningless: {caps:?}"
    );
    for cap in &caps.dropped {
        let loss_node = loss_iri("console", cap.slug());
        assert!(
            nt.contains(&triple(
                &console,
                &iri(GMEOW_NS, "declaredLoss"),
                &loss_node
            )),
            "the console's derived loss of {:?} is missing from the catalog",
            cap.slug()
        );
        assert!(nt.contains(&triple(
            &loss_node,
            &iri(GMEOW_NS, "accountsForParameter"),
            &capability_iri(*cap)
        )));
    }
    for cap in &caps.representable {
        assert!(nt.contains(&triple(
            &console,
            &iri(GMEOW_NS, "representableParameter"),
            &capability_iri(*cap)
        )));
    }
}

/// Every surface's ledger is TOTAL over the capability vocabulary: each capability is
/// either represented or accounted for as a loss, never both and never neither. That
/// pairing is exactly what makes the derived laws below checkable against this graph.
#[test]
fn every_surface_ledger_is_total_over_the_capabilities() {
    let nt = ntriples_text();
    for surface in DistributionSurface::ALL {
        let subject = surface_iri(surface);
        for cap in Capability::ALL {
            let represents = nt.contains(&triple(
                &subject,
                &iri(GMEOW_NS, "representableParameter"),
                &capability_iri(cap),
            ));
            let drops = nt.contains(&triple(
                &subject,
                &iri(GMEOW_NS, "declaredLoss"),
                &loss_iri(surface.slug(), cap.slug()),
            ));
            assert!(
                represents ^ drops,
                "{}/{cap:?}: the catalog ledger must place it in exactly one side \
                     (represents={represents}, drops={drops})",
                surface.slug()
            );
        }
    }
}

/// The COMPLETE concept lattice is emitted — every derived concept, with its extent and
/// its intent — and nothing beyond it.
#[test]
fn the_complete_concept_lattice_is_emitted() {
    let nt = ntriples_text();
    let derived = authored_concepts();
    assert_eq!(derived.len(), 4, "{derived:?}");

    for concept in &derived {
        let node = concept_iri(*concept);
        assert!(
            nt.contains(&triple(&node, RDF_TYPE, &iri(GMEOW_NS, "FormalConcept"))),
            "{node} is not typed gmeow:FormalConcept — the reader would drop it"
        );
        for surface in concept.extent.members() {
            assert!(nt.contains(&triple(
                &node,
                &iri(GMEOW_NS, "conceptExtent"),
                &surface_iri(surface)
            )));
        }
        for cap in concept.intent.members() {
            assert!(nt.contains(&triple(
                &node,
                &iri(GMEOW_NS, "conceptIntent"),
                &capability_iri(cap)
            )));
        }
    }

    // No EXTRA concept: the count of typed nodes matches the derived set exactly.
    let emitted = nt
        .lines()
        .filter(|line| line.ends_with(&format!("<{}> .", iri(GMEOW_NS, "FormalConcept"))))
        .count();
    assert_eq!(
        emitted,
        derived.len(),
        "the emitted lattice is not complete"
    );
}

/// The Duquenne–Guigues basis rides as `logic:Formula` ASTs — the repo's one law
/// representation — with `logic:antecedent` / `logic:consequent`. No implication
/// vocabulary is minted.
#[test]
fn the_implication_basis_rides_as_logic_formula_asts() {
    let nt = ntriples_text();
    let basis = authored_dg_basis();
    assert_eq!(basis.len(), 6, "{basis:?}");

    for implication in &basis {
        let law = law_iri(*implication);
        let body = format!("{law}/implication");
        assert!(nt.contains(&triple(&law, RDF_TYPE, &iri(LOGIC_NS, "Formula"))));
        assert!(nt.contains(&triple(
            &law,
            &iri(LOGIC_NS, "quantifiedVariable"),
            &law_term_iri("surface")
        )));
        assert!(nt.contains(&triple(&law, &iri(LOGIC_NS, "forall"), &body)));
        assert!(
            nt.lines().any(
                |line| line.starts_with(&format!("<{body}> <{}>", iri(LOGIC_NS, "antecedent")))
            ),
            "law {law} has no logic:antecedent"
        );
        assert!(
            nt.lines().any(
                |line| line.starts_with(&format!("<{body}> <{}>", iri(LOGIC_NS, "consequent")))
            ),
            "law {law} has no logic:consequent"
        );
        // Every atom argues the shared surface variable against a capability carrier,
        // over the ONE existing predicate — no minted implication vocabulary.
        for cap in implication.premise.members() {
            let atom = format!("{law}/premise/{}", cap.slug());
            assert!(nt.contains(&triple(
                &atom,
                &iri(LOGIC_NS, "relation"),
                &iri(GMEOW_NS, "representableParameter")
            )));
            assert!(nt.contains(&triple(
                &atom,
                &iri(LOGIC_NS, "argument"),
                &law_term_iri(cap.slug())
            )));
        }
    }

    // The one honest-gap marker, and only where the derivation says so: a law whose
    // premise no surface exhibits. Over the authored incidence there is none, and the
    // emitted marker set must agree with that derivation rather than with a guess.
    let unrealized: Vec<_> = basis
        .iter()
        .filter(|i| i.is_unrealized(&AUTHORED_INCIDENCE))
        .collect();
    let emitted_markers = nt
        .lines()
        .filter(|line| line.contains(&iri(LOGIC_NS, "expressivenessBoundary")))
        .count();
    assert_eq!(
        emitted_markers,
        unrealized.len(),
        "the expressiveness-boundary markers must be exactly the vacuously-true laws"
    );
    for implication in unrealized {
        assert!(nt.contains(&triple(
            &law_iri(*implication),
            &iri(LOGIC_NS, "expressivenessBoundary"),
            &iri(LOGIC_NS, "FirstOrder")
        )));
    }
}

/// Every DG law actually HOLDS of the emitted ledger — the laws are checkable against
/// this graph, not floating above it.
#[test]
fn every_emitted_law_holds_of_the_emitted_ledger() {
    let nt = ntriples_text();
    let represents = |surface: DistributionSurface, cap: Capability| {
        nt.contains(&triple(
            &surface_iri(surface),
            &iri(GMEOW_NS, "representableParameter"),
            &capability_iri(cap),
        ))
    };
    for implication in authored_dg_basis() {
        for surface in DistributionSurface::ALL {
            let premise_holds = implication
                .premise
                .members()
                .into_iter()
                .all(|cap| represents(surface, cap));
            if !premise_holds {
                continue;
            }
            for cap in implication.conclusion.members() {
                assert!(
                    represents(surface, cap),
                    "law {:?} is violated by the emitted ledger of {}",
                    implication,
                    surface.slug()
                );
            }
        }
    }
}

#[test]
fn serialization_family_has_no_declared_loss() {
    let nt = ntriples_text();
    let serializations: Vec<&DistRow> = DISTRIBUTIONS
        .iter()
        .filter(|row| row.family == Family::Serialization)
        .collect();
    assert!(
        !serializations.is_empty(),
        "the serialization family must be non-empty, or this gate is vacuous"
    );
    for row in serializations {
        let dist = dist_iri(row.slug);
        let pred = iri(GMEOW_NS, "declaredLoss");
        let needle = format!("<{dist}> <{pred}>");
        assert!(
            !nt.lines().any(|l| l.starts_with(&needle)),
            "{} (serialization family) must not declare loss",
            row.slug
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
    for row in DISTRIBUTIONS {
        let dist = dist_iri(row.slug);
        assert!(
            nt.contains(&triple(
                &dist,
                &iri(GMEOW_NS, "distributionFamily"),
                &family_iri(row.family)
            )),
            "{} missing distributionFamily {}",
            row.slug,
            row.family.slug()
        );
        assert!(
            nt.contains(&triple(
                &dist,
                &iri(GMEOW_NS, "eligibleForConsumer"),
                &consumer_iri(row.consumer)
            )),
            "{} missing eligibleForConsumer {}",
            row.slug,
            row.consumer
        );
        assert!(
            nt.contains(&triple_lit(
                &dist,
                &iri(GMEOW_NS, "artifactMediaType"),
                row.media_type
            )),
            "{} missing artifactMediaType {}",
            row.slug,
            row.media_type
        );
    }
}

/// The distribution-parameterized pricing set is exactly `owners × sub-assets`, and it
/// stays disjoint from the distribution bijection.
#[test]
fn sub_asset_pricing_is_owner_parameterized_and_complete() {
    let owners = sub_asset_owner_slugs();
    let subs = declared_site_sub_asset_slugs();
    let priced = sub_asset_pricing();
    assert_eq!(
        priced.len(),
        owners.len() * subs.len(),
        "pricing must cover every (owner, sub-asset) pair: {priced:?}"
    );
    for owner in &owners {
        for sub in &subs {
            assert!(
                priced
                    .iter()
                    .any(|row| row.owner == *owner && row.slug == *sub),
                "no pricing row for owner {owner:?} sub-asset {sub:?}"
            );
        }
    }
    // Every priced media type agrees with the emitted schema row, and every priced
    // owner really is a declared distribution.
    let nt = ntriples_text();
    for row in &priced {
        assert!(
            distribution_row(row.owner).is_some(),
            "pricing owner {:?} is not a declared distribution",
            row.owner
        );
        assert!(
            !row.tree_path_prefix.is_empty() && !row.canonical_path_prefix.is_empty(),
            "sub-asset {:?} has an empty tree or canonical prefix",
            row.slug
        );
        if row.owner == "mdbook" {
            assert!(
                row.tree_path_prefix.starts_with("src/assets/"),
                "mdBook sub-assets must name their emitted source-tree layout: {row:?}"
            );
        } else {
            assert!(
                row.tree_path_prefix.starts_with("assets/"),
                "site/console sub-assets must name their rendered-tree layout: {row:?}"
            );
        }
        assert!(
            nt.contains(&triple_lit(
                &sub_asset_iri(row.slug),
                &iri(GMEOW_NS, "artifactMediaType"),
                row.media_type
            )),
            "priced media type for {:?} disagrees with the emitted schema row",
            row.slug
        );
    }
}
