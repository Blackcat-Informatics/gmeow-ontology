// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reference indexes and detail pages for the ontology.

use std::collections::BTreeMap;

use crate::model::DocsModel;

use super::overview::dimension_label;
use super::{
    Page, blank, code_escape, concern_display, concern_slug, confidence_cell, curie_link, fenced,
    heading, line, local_name, md_advice_section, md_escape, namespace_count, one_line, push_line,
    rel, root_href, set_display, short_digest, slice_display, slice_link, slice_name, slice_slug,
    subject_link, term_link, to_curie,
};

// ── Linkage / example / concern / external / integrity pages ──────────────────

pub(super) fn md_linkage_index(model: &DocsModel) -> String {
    let from = Page::LinkageIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_linkages"));
    line(
        &mut out,
        &format!(
            "**{}** term equivalences across **{}** mapping set(s), cross-walking GMEOW terms to \
             external vocabularies.",
            model.linkages.len(),
            model.mapping_sets.len()
        ),
    );

    for set in &model.mapping_sets {
        heading(&mut out, 2, &set_display(set));
        push_line(&mut out, "| Field | Value |");
        push_line(&mut out, "| --- | --- |");
        push_line(
            &mut out,
            &format!("| CURIE | `{}` |", code_escape(&set.curie)),
        );
        if let Some(id) = &set.set_id {
            push_line(&mut out, &format!("| Set ID | `{}` |", code_escape(id)));
        }
        if let Some(file) = &set.sssom_file {
            push_line(
                &mut out,
                &format!("| SSSOM file | `{}` |", code_escape(file)),
            );
        }
        if let Some(license) = &set.license {
            push_line(
                &mut out,
                &format!(
                    "| License | [{}]({}) |",
                    md_escape(license),
                    md_escape(license)
                ),
            );
        }
        push_line(
            &mut out,
            &format!(
                "| Owner slice | {} |",
                slice_link(model, &from, &set.owner_slice)
            ),
        );
        push_line(
            &mut out,
            &format!("| Equivalences | {} |", set.equivalence_count),
        );
        blank(&mut out);

        if let Some(comment) = &set.comment {
            line(&mut out, &md_escape(&one_line(comment)));
        }

        let mut links: Vec<&crate::model::DocLinkage> = model
            .linkages
            .iter()
            .filter(|l| l.mapping_set.as_deref() == Some(set.iri.as_str()))
            .collect();
        links.sort_by(|a, b| {
            a.subject_curie
                .cmp(&b.subject_curie)
                .then_with(|| a.object.cmp(&b.object))
        });
        if !links.is_empty() {
            push_line(
                &mut out,
                "| Subject | Predicate | External object | Justification | Conf. |",
            );
            push_line(&mut out, "| --- | --- | --- | --- | --- |");
            for link in links {
                push_line(
                    &mut out,
                    &format!(
                        "| {} | `{}` | [{}]({}) | {} | {} |",
                        subject_link(model, &from, link),
                        code_escape(&link.predicate),
                        md_escape(&link.object),
                        md_escape(&link.object),
                        link.justification
                            .as_deref()
                            .map(|j| format!("`{}`", code_escape(j)))
                            .unwrap_or_default(),
                        confidence_cell(link.confidence),
                    ),
                );
            }
            blank(&mut out);
        }
    }

    // Any linkages not attached to a known mapping set (defensive completeness).
    let mut orphans: Vec<&crate::model::DocLinkage> = model
        .linkages
        .iter()
        .filter(|l| {
            l.mapping_set
                .as_deref()
                .map(|m| !model.mapping_sets.iter().any(|s| s.iri == m))
                .unwrap_or(true)
        })
        .collect();
    orphans.sort_by(|a, b| {
        a.subject_curie
            .cmp(&b.subject_curie)
            .then_with(|| a.object.cmp(&b.object))
    });
    if !orphans.is_empty() {
        heading(&mut out, 2, model.ui("body_other_equivalences"));
        push_line(
            &mut out,
            "| Subject | Predicate | External object | Conf. |",
        );
        push_line(&mut out, "| --- | --- | --- | --- |");
        for link in orphans {
            push_line(
                &mut out,
                &format!(
                    "| {} | `{}` | [{}]({}) | {} |",
                    subject_link(model, &from, link),
                    code_escape(&link.predicate),
                    md_escape(&link.object),
                    md_escape(&link.object),
                    confidence_cell(link.confidence),
                ),
            );
        }
        blank(&mut out);
    }

    out
}

pub(super) fn md_example_index(model: &DocsModel) -> String {
    let from = Page::ExampleIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_examples"));
    line(
        &mut out,
        &format!(
            "**{}** worked example(s). Each example's Turtle source is shown in full on its \
             owning slice page.",
            model.examples.len()
        ),
    );

    // Group examples by slice (model.examples is slice/path-sorted).
    let mut by_slice: BTreeMap<String, Vec<&crate::model::DocExample>> = BTreeMap::new();
    for example in &model.examples {
        by_slice
            .entry(example.slice.clone())
            .or_default()
            .push(example);
    }
    for (slice_iri, mut examples) in by_slice {
        examples.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
        heading(&mut out, 2, &slice_name(model, &slice_iri));
        let slice_href = model
            .slices
            .iter()
            .find(|s| s.iri == slice_iri)
            .map(|s| rel(&from, &Page::Slice(slice_slug(s)).dir()));
        for example in examples {
            match &slice_href {
                Some(href) => push_line(
                    &mut out,
                    &format!(
                        "- [{}]({}index.md) — `{}`",
                        md_escape(&example.title),
                        href,
                        code_escape(&example.logical_path)
                    ),
                ),
                None => push_line(
                    &mut out,
                    &format!(
                        "- {} — `{}`",
                        md_escape(&example.title),
                        code_escape(&example.logical_path)
                    ),
                ),
            }
        }
        blank(&mut out);
    }

    // ── Worked math instances (ℚ⁷ SI-dimension worked examples) ─────────────
    // Distinct from the example listing above: every example subject (in any
    // slice) carrying `math:hasDimension`, resolved generically by
    // `gmeow_docs::model::extract_worked_instances` — not special-cased to
    // `measure-and-dimension.ttl` (today's only author). Placed on THIS page
    // (rather than a new dedicated `Page` variant) because a worked instance
    // IS a worked example — the same `examples/*.ttl` scan the loss-row
    // section reuses, and `Page::ExampleIndex` is already the page readers
    // reach for "show me a concrete instance", so it stays their next stop
    // rather than a fourth example-adjacent page.
    heading(&mut out, 2, model.ui("body_worked_instances"));
    line(
        &mut out,
        "Concrete quantities/measures/functions carrying `math:hasDimension`, resolved to their \
         ℚ⁷ SI base-dimension exponent vector (mass · length · time · electric current · \
         temperature · amount of substance · luminous intensity) when the dimension object \
         breaks one down — a dimensionless subject (e.g. `math:dimensionless`) honestly renders \
         with no exponent rows, not a fabricated breakdown.",
    );
    if model.worked_instances.is_empty() {
        line(&mut out, model.ui("body_no_worked_instances"));
    } else {
        for instance in &model.worked_instances {
            let type_suffix = if instance.types.is_empty() {
                String::new()
            } else {
                format!(" ({})", instance.types.join(", "))
            };
            heading(
                &mut out,
                3,
                &format!(
                    "{}{}",
                    md_escape(&instance.subject),
                    md_escape(&type_suffix)
                ),
            );
            line(
                &mut out,
                &format!(
                    "`{}` — `{}`",
                    code_escape(&slice_name(model, &instance.slice)),
                    code_escape(&instance.logical_path)
                ),
            );
            if let Some(label) = &instance.label {
                line(&mut out, &md_escape(label));
            }
            if let Some(dimension_label) = &instance.dimension_label {
                line(
                    &mut out,
                    &format!("Dimension: {}", md_escape(dimension_label)),
                );
            }
            if instance.dimension_exponents.is_empty() {
                line(
                    &mut out,
                    "Dimensionless — no base-dimension exponent breakdown.",
                );
            } else {
                push_line(&mut out, "| Base dimension | Numerator | Denominator |");
                push_line(&mut out, "| --- | --- | --- |");
                for exponent in &instance.dimension_exponents {
                    push_line(
                        &mut out,
                        &format!(
                            "| `{}` | {} | {} |",
                            code_escape(&exponent.base_dimension),
                            exponent.numerator,
                            exponent.denominator
                        ),
                    );
                }
                blank(&mut out);
            }
            fenced(&mut out, "turtle", &instance.turtle);
        }
    }

    out
}

/// The competency-questions index, grouped by slice. Each question renders its
/// rationale, the terms it exercises (linked), the full copy-paste-runnable
/// SPARQL query, and its expected result — either an enumerated row table or a
/// pinned row count.
pub(super) fn md_competency_index(model: &DocsModel) -> String {
    let from = Page::CompetencyIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_competency_questions"));
    line(
        &mut out,
        &format!(
            "**{}** competency question(s) — declarative SPARQL questions the ontology must \
             answer, each pinning an expected result over the asserted merged ontology (or its \
             RDFS/native-reasoned closure, when the question opts in).",
            model.competencies.len(),
        ),
    );
    if model.competencies.is_empty() {
        line(&mut out, model.ui("body_no_competency_questions"));
        return out;
    }

    let mut by_slice: BTreeMap<&str, Vec<&crate::model::DocCompetency>> = BTreeMap::new();
    for cq in &model.competencies {
        by_slice
            .entry(cq.owner_slice.as_str())
            .or_default()
            .push(cq);
    }

    for (slice_iri, cqs) in by_slice {
        heading(&mut out, 2, &slice_name(model, slice_iri));
        for cq in cqs {
            heading(&mut out, 3, local_name(&cq.iri));

            for rationale in &cq.rationales {
                let text = md_escape(&one_line(&rationale.lexical_form));
                match &cq.query_file {
                    Some(qf) => push_line(&mut out, &format!("- {text} (`{}`)", code_escape(qf))),
                    None => push_line(&mut out, &format!("- {text}")),
                }
            }

            if !cq.exercises.is_empty() {
                let links: Vec<String> = cq
                    .exercises
                    .iter()
                    .map(|t| term_link(model, &from, t))
                    .collect();
                push_line(
                    &mut out,
                    &format!(
                        "- **{}:** {}",
                        model.ui("body_terms_used"),
                        links.join(", ")
                    ),
                );
            }

            match (cq.exact_rows, cq.expected_row_count) {
                (Some(true), _) => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** exactly {} row(s) (closed set)",
                        model.ui("body_expected_rows"),
                        cq.expected_rows.len()
                    ),
                ),
                (Some(false), _) => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** at least {} row(s) (subset)",
                        model.ui("body_expected_rows"),
                        cq.expected_rows.len()
                    ),
                ),
                (None, Some(n)) => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** exactly {n} row(s)",
                        model.ui("body_expected_rows")
                    ),
                ),
                (None, None) if !cq.expected_rows.is_empty() => push_line(
                    &mut out,
                    &format!(
                        "- **{}:** {} row(s)",
                        model.ui("body_expected_rows"),
                        cq.expected_rows.len()
                    ),
                ),
                (None, None) => {}
            }
            blank(&mut out);

            push_line(&mut out, &format!("**{}:**", model.ui("body_query")));
            blank(&mut out);
            match &cq.query_text {
                Some(text) => fenced(&mut out, "sparql", text),
                None => line(&mut out, model.ui("body_no_query_text")),
            }

            if !cq.expected_rows.is_empty() {
                let mut vars: Vec<&str> = cq
                    .expected_rows
                    .iter()
                    .flat_map(|row| row.cells.iter().filter_map(|c| c.var.as_deref()))
                    .collect();
                vars.sort();
                vars.dedup();
                if !vars.is_empty() {
                    push_line(&mut out, &format!("| {} |", vars.join(" | ")));
                    push_line(
                        &mut out,
                        &format!(
                            "| {} |",
                            vars.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")
                        ),
                    );
                    for row in &cq.expected_rows {
                        let cells: Vec<String> = vars
                            .iter()
                            .map(|v| {
                                let Some(cell) =
                                    row.cells.iter().find(|c| c.var.as_deref() == Some(*v))
                                else {
                                    return String::new();
                                };
                                if let Some(iri) = &cell.value_iri {
                                    term_link(model, &from, iri)
                                } else if let Some(lit) = &cell.value_literal {
                                    format!("`{}`", code_escape(lit))
                                } else {
                                    String::new()
                                }
                            })
                            .collect();
                        push_line(&mut out, &format!("| {} |", cells.join(" | ")));
                    }
                    blank(&mut out);
                }
            }
        }
    }
    out
}

/// The notation-grammars index — every first-class `lang:Grammar` rendering
/// (GMN / GTS / Turtle) authored as plain W3C EBNF text under
/// `slices/grounding/lang/grammars/*.ebnf`, listed with its title and
/// license, each linking to the full source on its own [`Page::Grammar`] page.
pub(super) fn md_notation_index(model: &DocsModel) -> String {
    let from = Page::NotationIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_notation_grammars"));
    line(
        &mut out,
        &format!(
            "**{}** notation grammar(s) — first-class W3C EBNF renderings of the sign systems \
             this project's own serialization notations use (the GMN compact record notation, \
             the GTS textual surface, and the RDF 1.1 Turtle grammar the native codec \
             interprets). Each is a RENDERING of the corresponding `lang:Grammar` object, never \
             a second parser: the grammar object itself carries the normative claims.",
            model.grammars.len()
        ),
    );
    if model.grammars.is_empty() {
        line(&mut out, model.ui("body_no_notation_grammars"));
        return out;
    }
    push_line(&mut out, "| Grammar | License |");
    push_line(&mut out, "| --- | --- |");
    for grammar in &model.grammars {
        let href = rel(&from, &Page::Grammar(grammar.slug.clone()).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | `{}` |",
                md_escape(&grammar.title),
                href,
                code_escape(&grammar.license),
            ),
        );
    }
    blank(&mut out);
    out
}

/// A single notation-grammar detail page: title, slug, license, and the full
/// W3C EBNF source in a fenced code block.
pub(super) fn md_grammar(model: &DocsModel, slug: &str) -> String {
    let Some(grammar) = model.grammars.iter().find(|g| g.slug == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_grammar_not_found"));
        return out;
    };
    let mut out = String::new();
    heading(&mut out, 1, &grammar.title);
    line(
        &mut out,
        &format!(
            "`{}` · notation grammar · License: `{}`",
            code_escape(&grammar.slug),
            code_escape(&grammar.license)
        ),
    );
    heading(&mut out, 2, model.ui("body_grammar_source"));
    fenced(&mut out, "ebnf", &grammar.source);
    out
}

pub(super) fn md_concern_index(model: &DocsModel) -> String {
    let from = Page::ConcernIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_concerns"));
    line(
        &mut out,
        &format!(
            "**{}** cross-cutting documentation concern(s). Concerns group vocabulary terms by \
             the design question they answer, across slice boundaries.",
            model.concerns.len()
        ),
    );

    heading(&mut out, 2, model.ui("body_by_term_count"));
    push_line(
        &mut out,
        &format!(
            "![Concerns by term count]({}diagrams/concerns.svg)",
            root_href(&from)
        ),
    );
    blank(&mut out);

    push_line(&mut out, "| Concern | Terms | Slices |");
    push_line(&mut out, "| --- | --- | --- |");
    for concern in &model.concerns {
        let href = rel(&from, &Page::Concern(concern_slug(concern)).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {} | {} |",
                md_escape(&concern_display(concern)),
                href,
                concern.terms.len(),
                concern.slices.len(),
            ),
        );
    }
    blank(&mut out);
    out
}

pub(super) fn md_concern(model: &DocsModel, slug: &str) -> String {
    let Some(concern) = model.concerns.iter().find(|c| concern_slug(c) == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_concern_not_found"));
        return out;
    };
    let from = Page::Concern(slug.to_string()).dir();

    let mut out = String::new();
    heading(&mut out, 1, &concern_display(concern));
    line(&mut out, &format!("`{}`", code_escape(&concern.curie)));

    if let Some(def) = &concern.definition {
        heading(&mut out, 2, model.ui("body_definition"));
        line(&mut out, &md_escape(def));
    }

    if !concern.terms.is_empty() {
        heading(&mut out, 2, &format!("Terms ({})", concern.terms.len()));
        for curie in &concern.terms {
            push_line(&mut out, &format!("- {}", curie_link(model, &from, curie)));
        }
        blank(&mut out);
    }

    if !concern.slices.is_empty() {
        heading(&mut out, 2, &format!("Slices ({})", concern.slices.len()));
        for slice_iri in &concern.slices {
            push_line(
                &mut out,
                &format!("- {}", slice_link(model, &from, slice_iri)),
            );
        }
        blank(&mut out);
    }
    out
}

pub(super) fn md_external_index(model: &DocsModel) -> String {
    let from = Page::ExternalIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_external_ontologies"));
    line(
        &mut out,
        &format!(
            "**{}** external term(s) referenced across **{}** namespace(s). These are the \
             non-GMEOW IRIs GMEOW terms link to (via mappings) or inherit from / constrain (via \
             domain, range, super-class edges).",
            model.external_terms.len(),
            namespace_count(model),
        ),
    );

    // Group by namespace (deterministic: external_terms is IRI-sorted).
    let mut by_ns: BTreeMap<String, Vec<&crate::model::DocExternalTerm>> = BTreeMap::new();
    for term in &model.external_terms {
        by_ns.entry(term.namespace.clone()).or_default().push(term);
    }
    for (namespace, terms) in by_ns {
        heading(&mut out, 2, &namespace);
        push_line(&mut out, "| External term | Referenced by | Via |");
        push_line(&mut out, "| --- | --- | --- |");
        for term in terms {
            let referencers = term
                .referenced_by
                .iter()
                .map(|c| curie_link(model, &from, c))
                .collect::<Vec<_>>()
                .join(", ");
            push_line(
                &mut out,
                &format!(
                    "| [{}]({}) | {} | {} |",
                    md_escape(&term.iri),
                    md_escape(&term.iri),
                    referencers,
                    md_escape(&term.via_predicate.join(", ")),
                ),
            );
        }
        blank(&mut out);
    }
    out
}

pub(super) fn md_integrity_index(model: &DocsModel) -> String {
    let from = Page::IntegrityIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_integrity_constraints"));
    line(
        &mut out,
        "Per-slice SPARQL verification queries. Each query asserts an integrity constraint the \
         slice's data must satisfy; a non-empty result is a violation.",
    );

    let mut any = false;
    for slice in &model.slices {
        let mut queries: Vec<&crate::model::DocArtifact> = slice
            .artifacts
            .iter()
            .filter(|a| a.role == purrdf::slice::ArtifactRole::VerifyQuery)
            .collect();
        queries.sort_by(|a, b| a.logical_path.cmp(&b.logical_path));
        if queries.is_empty() {
            continue;
        }
        any = true;
        heading(&mut out, 2, &slice_display(slice));
        let href = rel(&from, &Page::Slice(slice_slug(slice)).dir());
        line(&mut out, &format!("[Slice page]({href}index.md)"));
        for query in queries {
            heading(&mut out, 3, &query.logical_path);
            // The query text is not carried on DocArtifact (by-reference); note the
            // path and digest so the constraint is locatable in the slice tree.
            push_line(
                &mut out,
                &format!(
                    "- Path: `{}`  ·  digest `{}`",
                    code_escape(&query.logical_path),
                    code_escape(&short_digest(&query.raw_digest)),
                ),
            );
            blank(&mut out);
        }
    }
    if !any {
        line(&mut out, model.ui("body_no_verify_queries"));
    }
    out
}

/// The "What GMEOW enforces" page: the constraint catalog — every
/// `gmeow:ValidationRule` the toolchain enforces — grouped by finding category
/// and, within each category, sorted by rule code. Each rule entry carries an
/// explicit `<a id="{slug}">` anchor whose id is
/// `gmeow_validate::rule_catalog::slugify(code)` — the identical slug the
/// validator stamps into a finding's `helpUri` fragment — so a finding's help
/// link deep-links straight to its rule. Deterministic: categories, rules, and
/// applies-to terms are all sorted.
pub(super) fn md_constraint_catalog(model: &DocsModel) -> String {
    let from = Page::ConstraintCatalog.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_enforced_constraints"));
    line(
        &mut out,
        &format!(
            "The **{}** validation rules the GMEOW toolchain enforces, grouped by finding \
             category. Each rule is a `gmeow:ValidationRule` individual from the constraint \
             catalog; a finding's `helpUri` deep-links to its entry here. **Binding** rules fail \
             the gate; **advisory** rules report without failing. The distinct **{}** section \
             below documents the non-gating *recommendations* (`avoidWhen` / `useWhen` / \
             `howToUse`) that advisory `advice.*` findings resolve to.",
            model.constraint_rules.len(),
            model.ui("body_usage_advice"),
        ),
    );

    if model.constraint_rules.is_empty() {
        line(&mut out, model.ui("body_no_enforced_constraints"));
        return out;
    }

    // Group by category IRI (rules are already sorted by code); the category map
    // is a BTreeMap so category headings emit in sorted IRI order. The `advice.`
    // family rule is EXCLUDED here — it heads the distinct Advice section below and
    // carries the single `#advice-` anchor, which must appear exactly once.
    let mut by_category: std::collections::BTreeMap<&str, Vec<&crate::model::ConstraintRule>> =
        std::collections::BTreeMap::new();
    for rule in &model.constraint_rules {
        if rule.code == gmeow_validate::codes::ADVICE_FAMILY {
            continue;
        }
        by_category
            .entry(rule.category.as_str())
            .or_default()
            .push(rule);
    }

    for (category, rules) in &by_category {
        heading(&mut out, 2, &finding_category_display(category));
        for rule in rules {
            // Explicit anchor so the finding helpUri fragment (`#{slug}`) resolves
            // regardless of the Markdown heading-id derivation. The slug is
            // `[a-z0-9-]` only (the validator's transform), so it is inert inside the
            // HTML `id` attribute — never md-escape it, or the id would gain
            // backslashes and stop matching the helpUri fragment.
            push_line(&mut out, &format!("<a id=\"{}\"></a>", rule.slug));
            blank(&mut out);
            heading(&mut out, 3, &rule.code);

            let severity = if rule.severity.is_empty() {
                "—".to_string()
            } else {
                rule.severity.clone()
            };
            push_line(
                &mut out,
                &format!(
                    "- {}: **{}**",
                    model.ui("body_label_severity"),
                    md_escape(&severity)
                ),
            );
            push_line(
                &mut out,
                &format!(
                    "- {}: `{}`",
                    model.ui("body_label_rule_code"),
                    code_escape(&rule.code)
                ),
            );
            if !rule.help_uri.is_empty() {
                // The display text is md-escaped; the link *target* is the raw URL
                // (escaping a target corrupts the href).
                push_line(
                    &mut out,
                    &format!(
                        "- {}: [{}]({})",
                        model.ui("body_label_help_link"),
                        md_escape(&rule.help_uri),
                        rule.help_uri,
                    ),
                );
            }
            if let Some(formalizes) = &rule.formalizes {
                push_line(
                    &mut out,
                    &format!(
                        "- {}: {}",
                        model.ui("body_formalized_by"),
                        curie_link(model, &from, &to_curie(formalizes)),
                    ),
                );
            }
            blank(&mut out);

            if let Some(definition) = &rule.definition {
                line(&mut out, &md_escape(definition));
            } else if let Some(label) = &rule.label {
                line(&mut out, &md_escape(label));
            }

            if !rule.applies_to_terms.is_empty() {
                push_line(&mut out, &format!("**{}**", model.ui("body_applies_to")));
                blank(&mut out);
                // applies_to_terms is already sorted by the model reader.
                for term in &rule.applies_to_terms {
                    push_line(
                        &mut out,
                        &format!("- {}", curie_link(model, &from, &to_curie(term))),
                    );
                }
                blank(&mut out);
            }
        }
    }

    // ── The Advice section: the recommendation tier, a distinct top-level
    // section headed by the `advice.` family rule's `#advice-` anchor. ────────────
    md_advice_section(&mut out, model, &from);
    out
}

/// A human-readable display name for a `logic:FindingCategory` IRI (e.g.
/// `…/logic/FindingPolicyWarning` → `Finding: Policy Warning`). Falls back to the
/// raw IRI's local name when it is not a recognized `Finding…` class.
pub(super) fn finding_category_display(iri: &str) -> String {
    let local = iri.rsplit(['/', '#']).next().unwrap_or(iri);
    let stem = local.strip_prefix("Finding").unwrap_or(local);
    // Split the CamelCase stem into spaced words.
    let mut words = String::new();
    for (i, ch) in stem.char_indices() {
        if i > 0 && ch.is_ascii_uppercase() {
            words.push(' ');
        }
        words.push(ch);
    }
    if stem == local {
        // Not a `Finding…` class — surface the local name verbatim.
        words
    } else {
        format!("Finding: {words}")
    }
}
