// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

pub(super) fn md_landing(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, &model.title);
    line(
        &mut out,
        &format!("Version **{}**.", md_escape(&model.version)),
    );
    line(
        &mut out,
        &format!(
            "This site documents **{}** vocabulary terms across **{}** slices.",
            model.terms.len(),
            model.slices.len()
        ),
    );

    // ── Task-first navigation: promote the authored recipes and learning paths
    // to a prominent "I want to…" block so a goal-oriented reader starts from a
    // task rather than the vocabulary, plus the RDF-to-developer glossary bridge.
    // Rendered only when the guides slice authored at least one recipe / learning
    // path (honest absence otherwise); the glossary link is always offered.
    {
        let from = Page::Landing.dir();
        heading(&mut out, 2, model.ui("body_i_want_to"));
        for recipe in &model.recipes {
            push_line(
                &mut out,
                &format!(
                    "- [{}]({}index.md) — {}",
                    md_escape(&recipe.title),
                    rel(&from, &Page::Recipe(recipe.slug.clone()).dir()),
                    md_escape(&one_line(&recipe.goal)),
                ),
            );
        }
        for path in &model.learning_paths {
            push_line(
                &mut out,
                &format!(
                    "- [{}]({}index.md) — {}",
                    md_escape(&path.title),
                    rel(&from, &Page::LearningPath(path.slug.clone()).dir()),
                    md_escape(&one_line(&path.goal)),
                ),
            );
        }
        push_line(
            &mut out,
            &format!(
                "- [{}]({}index.md) — a bridge for readers new to RDF.",
                md_escape(model.ui("body_glossary")),
                rel(&from, &Page::Glossary.dir()),
            ),
        );
        blank(&mut out);
    }

    heading(&mut out, 2, model.ui("body_vocabulary_by_category"));
    push_line(&mut out, "| Category | Terms |");
    push_line(&mut out, "| --- | --- |");
    for category in [
        DocTermCategory::Class,
        DocTermCategory::Property,
        DocTermCategory::Individual,
        DocTermCategory::Datatype,
        DocTermCategory::Other,
    ] {
        let count = model
            .terms
            .iter()
            .filter(|t| t.category == category)
            .count();
        if count == 0 {
            continue;
        }
        let href = rel(&Page::Landing.dir(), &Page::Category(category).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {count} |",
                md_escape(category_plural(category)),
                href
            ),
        );
    }
    blank(&mut out);

    heading(&mut out, 2, model.ui("body_browse"));
    let from = Page::Landing.dir();
    push_line(
        &mut out,
        &format!(
            "- [All slices]({}index.md) — the {} compilation units.",
            rel(&from, &Page::SliceIndex.dir()),
            model.slices.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Concerns]({}index.md) — {} cross-cutting design concerns.",
            rel(&from, &Page::ConcernIndex.dir()),
            model.concerns.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Linkages]({}index.md) — {} term equivalences to external vocabularies.",
            rel(&from, &Page::LinkageIndex.dir()),
            model.linkages.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Examples]({}index.md) — {} worked examples.",
            rel(&from, &Page::ExampleIndex.dir()),
            model.examples.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [External ontologies]({}index.md) — {} external terms referenced.",
            rel(&from, &Page::ExternalIndex.dir()),
            model.external_terms.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Integrity constraints]({}index.md)",
            rel(&from, &Page::IntegrityIndex.dir())
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [What GMEOW enforces]({}index.md) — {} validation rules the toolchain enforces.",
            rel(&from, &Page::ConstraintCatalog.dir()),
            model.constraint_rules.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Recipes]({}index.md) — {} task-oriented adoption recipes.",
            rel(&from, &Page::RecipeIndex.dir()),
            model.recipes.len()
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Learning paths]({}index.md) — {} curated adoption journeys.",
            rel(&from, &Page::LearningPathIndex.dir()),
            model.learning_paths.len()
        ),
    );
    if model.four_boxes.is_some() {
        push_line(
            &mut out,
            &format!(
                "- [What is this?]({}index.md) — the ABox/TBox/RBox/CBox/ConfigBox doctrine.",
                rel(&from, &Page::FourBoxes.dir())
            ),
        );
    }
    push_line(
        &mut out,
        &format!(
            "- [Documentation health]({}index.md) — per-dimension coverage of the vocabulary surface.",
            rel(&from, &Page::Health.dir())
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- [Getting started]({}index.md)",
            rel(&from, &Page::GettingStarted.dir())
        ),
    );
    push_line(
        &mut out,
        &format!("- [About]({}index.md)", rel(&from, &Page::About.dir())),
    );
    blank(&mut out);
    out
}

/// The number of PER-TERM coverage dimensions a projected term record covers —
/// counted over the canonical [`crate::coverage::DIMENSIONS`] set so a stray
/// local name in the read-back can never inflate the completeness distribution.
fn present_dimension_count(term: &crate::rdf::DocTermFacts) -> usize {
    crate::coverage::DIMENSIONS
        .iter()
        .filter(|dim| term.covers.contains(dim.dimension.local_name()))
        .count()
}

/// The short human title of a maturity anchor (`Minimal` / `Basic` / `Full` /
/// `Maximal`) for the health dashboard.
fn anchor_title(anchor: crate::maturity::MaturityAnchor) -> String {
    use crate::maturity::MaturityAnchor::*;
    match anchor {
        Minimal => "Minimal",
        Basic => "Basic",
        Full => "Full",
        Maximal => "Maximal",
    }
    .to_owned()
}

/// The gap-to-next-tier burn-down for a slice: the next anchor up the derived
/// ladder and the dimensions its intent requires that the slice's covered set
/// (`covers`, read back from `gmeow:docCoversDimension`) does not yet carry. At
/// the ceiling (earns `Maximal`) there is no next tier.
fn maturity_gap(
    earned: Option<crate::maturity::MaturityAnchor>,
    covers: &BTreeSet<String>,
) -> String {
    use crate::maturity::MaturityAnchor;
    // The next tier above the earned floor; when nothing is earned yet the first
    // rung (Minimal) is the target.
    let next = match earned {
        Some(a) => a.next(),
        None => Some(MaturityAnchor::Minimal),
    };
    let Some(next) = next else {
        return "at ceiling (Maximal)".to_owned();
    };
    let missing: Vec<String> = next
        .intent()
        .iter()
        .filter(|dim| !covers.contains(dim.local_name()))
        .map(|dim| dimension_label(*dim))
        .collect();
    if missing.is_empty() {
        // The intent is already covered — the slice is one closure step from the
        // next tier (the emitter reports the largest satisfied anchor as earned).
        format!("→ {}", anchor_title(next))
    } else {
        format!("→ {}: missing {}", anchor_title(next), missing.join(", "))
    }
}

/// The human display label for a coverage dimension, resolved from the single
/// [`crate::coverage`] label authority (per-term and slice-scoped dimensions).
fn dimension_label(dim: crate::maturity::Dimension) -> String {
    crate::coverage::DIMENSIONS
        .iter()
        .chain(crate::coverage::SLICE_DIMENSIONS.iter())
        .find(|d| d.dimension == dim)
        .map_or_else(|| dim.local_name().to_owned(), |d| d.label.to_owned())
}

/// The documentation-health dashboard, a PURE projection of the emitted
/// `graph/documentation` incidence: per-dimension coverage of the vocabulary
/// surface, a completeness distribution, and the per-slice earned-maturity floor
/// with a gap-to-next-tier burn-down. Every coverage number is read back from
/// `gmeow:docCoversDimension` / `gmeow:coverageFraction` / `gmeow:docEarnedMaturity`
/// (never a second recompute from `crate::coverage`), so the dashboard and the
/// reasoned graph cannot silently disagree.
pub(super) fn md_health(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_documentation_health"));
    line(
        &mut out,
        &format!(
            "Coverage of the {} documentation dimensions across **{}** vocabulary terms. Each \
             dimension mirrors a `docs/missing-*` lint code; the covered counts grow as source \
             prose, examples, scope notes, and external alignments land. Per-term detail lives \
             on each term page's *Documentation coverage* section.",
            crate::coverage::TermCoverage::TOTAL,
            model.terms.len()
        ),
    );

    let aligned = crate::coverage::alignment_subjects(model);

    // PURE PROJECTION: the health dashboard reads its coverage numbers back from
    // the emitted `graph/documentation` incidence (`gmeow:docCoversDimension` per
    // record), NEVER a second recompute from `crate::coverage`. The page and the
    // reasoned graph are therefore the same bytes read two ways — they cannot
    // silently diverge.
    let graph = crate::rdf::documentation_graph(model);
    let total = graph.terms.len();

    // Per-dimension coverage — covered count = the number of documented terms whose
    // projected incidence COVERS the dimension (the complement of the
    // `docs/missing-*` gap).
    heading(&mut out, 2, model.ui("body_coverage_by_dimension"));
    push_line(&mut out, "| Dimension | Covered | Total | % |");
    push_line(&mut out, "| --- | --- | --- | --- |");
    for dim in crate::coverage::DIMENSIONS.iter() {
        let local = dim.dimension.local_name();
        let covered = graph
            .terms
            .iter()
            .filter(|t| t.covers.contains(local))
            .count();
        let pct = (covered * 100).checked_div(total).unwrap_or(0);
        push_line(
            &mut out,
            &format!("| {} | {covered} | {total} | {pct}% |", dim.label),
        );
    }
    blank(&mut out);

    // Completeness distribution: how many terms carry exactly k of the dimensions,
    // counted from the projected per-term covered set.
    heading(&mut out, 2, model.ui("body_completeness_distribution"));
    push_line(&mut out, "| Dimensions present | Terms |");
    push_line(&mut out, "| --- | --- |");
    let dims_total = crate::coverage::TermCoverage::TOTAL;
    for k in (0..=dims_total).rev() {
        let count = graph
            .terms
            .iter()
            .filter(|t| present_dimension_count(t) == k)
            .count();
        push_line(&mut out, &format!("| {k} / {dims_total} | {count} |"));
    }
    blank(&mut out);

    // ── Maturity by slice (earned floor + gap-to-next-tier burn-down) ────────────
    // Projected from the per-slice incidence: `gmeow:docEarnedMaturity` (the FCA
    // floor), `gmeow:coverageFraction`, and any asserted `gmeow:sliceDocMaturity`.
    // The gap column names the exact dimensions standing between the slice and its
    // next tier — the burn-down a slice author reads straight off the dashboard.
    if !graph.slices.is_empty() {
        heading(&mut out, 2, model.ui("body_maturity_by_slice"));
        line(&mut out, model.ui("body_maturity_legend"));
        push_line(
            &mut out,
            "| Slice | Earns | Coverage | Claims | Gap to next tier |",
        );
        push_line(&mut out, "| --- | --- | --- | --- | --- |");
        for slice in &graph.slices {
            let earned = slice
                .earned
                .as_deref()
                .and_then(crate::maturity::MaturityAnchor::from_local);
            let asserted = slice
                .asserted
                .as_deref()
                .and_then(crate::maturity::MaturityAnchor::from_local);
            let earned_label = earned.map_or("—".to_owned(), anchor_title);
            // The claim column flags an over-claim inline: a slice that asserts a
            // tier above the earned floor trips the `asserted ⊄ earned` gate.
            let claim_label = match asserted {
                None => "—".to_owned(),
                Some(a) => {
                    let over = crate::maturity::asserted_exceeds_earned(a, earned);
                    if over {
                        format!("⚠ {} (unsupported)", anchor_title(a))
                    } else {
                        anchor_title(a)
                    }
                }
            };
            let gap = maturity_gap(earned, &slice.covers);
            push_line(
                &mut out,
                &format!(
                    "| `{}` | {earned_label} | {}% | {claim_label} | {gap} |",
                    code_escape(local_name(&slice.documents)),
                    (slice.coverage_fraction * 100.0).round() as i64,
                ),
            );
        }
        blank(&mut out);
    }

    // ── Reasoning (present only when the native-reasoner verdict is attached) ────
    if let Some(verdict) = &model.reasoning {
        heading(&mut out, 2, model.ui("body_reasoning"));
        let classes = model
            .terms
            .iter()
            .filter(|t| t.category == DocTermCategory::Class)
            .count();
        let unsat = model
            .terms
            .iter()
            .filter(|t| {
                t.category == DocTermCategory::Class && verdict.unsatisfiable.contains(&t.iri)
            })
            .count();
        let consistency = match verdict.consistency {
            crate::model::ConsistencyAssessment::Consistent => {
                model.ui("body_reasoning_consistent")
            }
            crate::model::ConsistencyAssessment::Inconsistent => {
                model.ui("body_reasoning_inconsistent")
            }
            crate::model::ConsistencyAssessment::Undetermined => "undetermined",
        };
        push_line(
            &mut out,
            &format!(
                "- Native DL reasoning: the ontology is {consistency}. **{unsat}** of {classes} \
                documented classes are proved unsatisfiable.",
            ),
        );
        if verdict.consistency != crate::model::ConsistencyAssessment::Consistent {
            line(
                &mut out,
                "Satisfiability of the other documented classes is undetermined.",
            );
        }
        push_line(
            &mut out,
            &format!(
                "- Native observation: input={}, evaluation={}, completeness={}, information={}. Unsupported observations: **{}**.",
                verdict.input,
                verdict.evaluation,
                verdict.completeness,
                verdict.information,
                verdict.unsupported_constructs.len(),
            ),
        );
        blank(&mut out);
    }

    // ── Coverage heatmap by slice (deterministic SVG, the shared color scale) ────
    heading(&mut out, 2, model.ui("body_coverage_by_slice"));
    line(&mut out, model.ui("body_health_heatmap_legend"));
    push_line(
        &mut out,
        &format!(
            "![Documentation coverage by slice]({}diagrams/coverage-heatmap.svg)",
            root_href(&Page::Health.dir())
        ),
    );
    blank(&mut out);

    // ── Linkage: alignment density + orphan terms ───────────────────────────────
    heading(&mut out, 2, model.ui("body_linkage"));
    let aligned_count = aligned.len();
    let orphan_count = model
        .terms
        .iter()
        .filter(|t| {
            t.parents.is_empty() && t.related_terms.is_empty() && !aligned.contains(t.iri.as_str())
        })
        .count();
    push_line(
        &mut out,
        &format!(
            "- **{}:** {aligned_count} of {total} terms ({}%) are the subject of at \
             least one external alignment.",
            model.ui("body_label_alignment_density"),
            (aligned_count * 100).checked_div(total).unwrap_or(0)
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- **{}:** {orphan_count} term(s) carry no parent, related term, or alignment.",
            model.ui("body_label_orphan_terms")
        ),
    );
    blank(&mut out);

    // ── Framework distribution (term count per logic:LogicalFramework) ──────────
    let mut framework_counts: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for term in &model.terms {
        for framework in &term.frameworks {
            *framework_counts.entry(framework.as_str()).or_default() += 1;
        }
    }
    if !framework_counts.is_empty() {
        heading(&mut out, 2, model.ui("body_framework_distribution"));
        push_line(&mut out, "| Framework | Terms |");
        push_line(&mut out, "| --- | --- |");
        for (framework, count) in &framework_counts {
            push_line(
                &mut out,
                &format!("| `{}` | {count} |", code_escape(framework)),
            );
        }
        blank(&mut out);
    }

    // ── Badge legend (rendered from the single badge color authority) ───────────
    heading(&mut out, 2, model.ui("body_badge_legend"));
    push_line(&mut out, "| Family | What it encodes |");
    push_line(&mut out, "| --- | --- |");
    for family in &crate::badge::FAMILIES {
        push_line(
            &mut out,
            &format!("| **{}** | {} |", family.label, family.description),
        );
    }
    blank(&mut out);

    out
}

pub(super) fn md_getting_started(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_getting_started"));
    line(
        &mut out,
        "The GMEOW ontology is organized into self-contained *slices*. Each slice owns a \
         vocabulary module, optional SHACL shapes, mappings, queries, and tests, and declares \
         its manifest identity and dependencies.",
    );
    heading(&mut out, 2, model.ui("body_where_to_go_next"));
    let from = Page::GettingStarted.dir();
    push_line(
        &mut out,
        &format!(
            "- Browse the [slice index]({}index.md) to see every compilation unit.",
            rel(&from, &Page::SliceIndex.dir())
        ),
    );
    push_line(
        &mut out,
        &format!(
            "- Browse [classes]({}index.md) and [properties]({}index.md) to explore the vocabulary.",
            rel(&from, &Page::Category(DocTermCategory::Class).dir()),
            rel(&from, &Page::Category(DocTermCategory::Property).dir())
        ),
    );
    blank(&mut out);
    out
}

pub(super) fn md_about(model: &DocsModel) -> String {
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_about"));
    line(
        &mut out,
        &format!(
            "**{}** is generated directly from the slice catalog by the Rust `gmeow-docs` \
             renderer. Every page is a deterministic projection of the typed documentation \
             model — there is no hand-authored HTML.",
            md_escape(&model.title)
        ),
    );
    heading(&mut out, 2, model.ui("body_at_a_glance"));
    push_line(
        &mut out,
        &format!(
            "- Documentation model version: **{}**",
            md_escape(&model.version)
        ),
    );
    push_line(&mut out, &format!("- Slices: **{}**", model.slices.len()));
    push_line(
        &mut out,
        &format!("- Vocabulary terms: **{}**", model.terms.len()),
    );
    push_line(
        &mut out,
        &format!(
            "- Cross-slice dependency edges: **{}**",
            model.dependency_edges.len()
        ),
    );
    blank(&mut out);
    out
}

/// A total-order sort key for a version string: the parsed `(major, minor,
/// patch)` numeric triple when the string is dotted-numeric, else a sentinel that
/// sorts unparsable versions last; the original string is the final tiebreak so
/// the order is total and deterministic (never a branchy comparator).
fn version_sort_key(version: &str) -> ((u64, u64, u64, u8), String) {
    let mut parts = version.split('.');
    let mut next = || parts.next().and_then(|p| p.parse::<u64>().ok());
    match (next(), next(), next()) {
        (Some(major), Some(minor), Some(patch)) if parts.next().is_none() => {
            ((major, minor, patch, 0), version.to_string())
        }
        // Unparsable / non-triple versions sort after every semver triple (the
        // trailing sentinel byte = 1), ordered among themselves by their string.
        _ => ((u64::MAX, u64::MAX, u64::MAX, 1), version.to_string()),
    }
}

/// The changelog surface, derived from the per-term content-address provenance:
/// every term's `added_in_version` seeds an "Added" row under that release, and
/// every reified changelog entry (a content-digest divergence) a "Changed" row.
/// Releases are listed newest-first; within a release, terms are CURIE-sorted.
pub(super) fn md_changelog(model: &DocsModel) -> String {
    let from = Page::Changelog.dir();
    // release version → (added terms, changed (term, entry) rows). BTreeMap keeps
    // the collection deterministic; the explicit version_sort_key drives display.
    let mut added: BTreeMap<String, Vec<&DocTerm>> = BTreeMap::new();
    let mut changed: BTreeMap<String, Vec<(&DocTerm, &DocChangelogEntry)>> = BTreeMap::new();
    for term in &model.terms {
        if let Some(version) = &term.added_in_version {
            added.entry(version.clone()).or_default().push(term);
        }
        for entry in &term.changelog {
            changed
                .entry(entry.version.clone())
                .or_default()
                .push((term, entry));
        }
    }

    // Every release that carries either an addition or a change, newest-first.
    let mut versions: Vec<String> = added.keys().chain(changed.keys()).cloned().collect();
    versions.sort_by_key(|v| std::cmp::Reverse(version_sort_key(v)));
    versions.dedup();

    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_changelog"));
    line(
        &mut out,
        "Each release below is derived from the per-term content-address manifest: a term is \
         listed under the release it was first seen in, and again whenever its canonical \
         definition digest changed.",
    );

    let term_link = |from: &str, term: &DocTerm| {
        format!(
            "[`{}`]({}index.md){}",
            code_escape(&term.curie),
            rel(from, &Page::Term(term_slug(term)).dir()),
            label_suffix(term)
        )
    };

    for version in &versions {
        heading(&mut out, 2, &md_escape(version));
        if let Some(terms) = added.get(version) {
            let mut terms = terms.clone();
            terms.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));
            heading(&mut out, 3, model.ui("body_changelog_added"));
            for term in terms {
                push_line(&mut out, &format!("- {}", term_link(&from, term)));
            }
            blank(&mut out);
        }
        if let Some(rows) = changed.get(version) {
            let mut rows = rows.clone();
            rows.sort_by(|a, b| {
                a.0.curie
                    .cmp(&b.0.curie)
                    .then_with(|| a.0.iri.cmp(&b.0.iri))
                    .then_with(|| a.1.source.cmp(&b.1.source))
                    .then_with(|| a.1.note.cmp(&b.1.note))
            });
            heading(&mut out, 3, model.ui("body_changelog_changed"));
            for (term, entry) in rows {
                let source = entry.source.wire();
                match &entry.note {
                    Some(note) => push_line(
                        &mut out,
                        &format!(
                            "- {} ({source}) — {}",
                            term_link(&from, term),
                            md_escape(note)
                        ),
                    ),
                    None => push_line(
                        &mut out,
                        &format!("- {} ({source})", term_link(&from, term)),
                    ),
                }
            }
            blank(&mut out);
        }
    }
    blank(&mut out);
    out
}

pub(super) fn md_category(model: &DocsModel, category: DocTermCategory) -> String {
    let from = Page::Category(category).dir();
    let mut terms: Vec<&DocTerm> = model
        .terms
        .iter()
        .filter(|t| t.category == category)
        .collect();
    terms.sort_by(|a, b| a.curie.cmp(&b.curie).then_with(|| a.iri.cmp(&b.iri)));

    let mut out = String::new();
    heading(&mut out, 1, category_plural(category));
    line(&mut out, &format!("{} term(s).", terms.len()));

    push_line(&mut out, "| Term | Definition |");
    push_line(&mut out, "| --- | --- |");
    for term in &terms {
        let href = rel(&from, &Page::Term(term_slug(term)).dir());
        let def = term.definition.as_deref().map(one_line).unwrap_or_default();
        push_line(
            &mut out,
            &format!(
                "| [`{}`]({}index.md){} | {} |",
                code_escape(&term.curie),
                href,
                label_suffix(term),
                md_escape(&def)
            ),
        );
    }
    blank(&mut out);
    out
}
