// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_docs_model::slug::{pydantic_class_name, pydantic_module_slug};

/// Standard Base64 (RFC 4648, `+`/`/`, `=` padding) — used to carry a fixture's
/// Turtle verbatim inside a `data-` attribute for the in-browser "run validation"
/// control, so newlines/quotes/`<` in the RDF never break the HTML. Small, dep-free,
/// and deterministic.
fn base64_encode(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(A[((n >> 18) & 63) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            A[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub(super) fn md_term(model: &DocsModel, slug: &str, exec: &ExecutableDocsData) -> Result<String> {
    let Some(term) = model.terms.iter().find(|t| term_slug(t) == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_term_not_found"));
        return Ok(out);
    };
    let from = Page::Term(slug.to_string()).dir();

    let mut out = String::new();
    let title = term.label.clone().unwrap_or_else(|| term.curie.clone());
    heading(&mut out, 1, &title);
    line(
        &mut out,
        &format!(
            "`{}` · {}",
            code_escape(&term.curie),
            category_singular(term.category)
        ),
    );

    push_line(&mut out, "| Field | Value |");
    push_line(&mut out, "| --- | --- |");
    push_line(
        &mut out,
        &format!("| CURIE | `{}` |", code_escape(&term.curie)),
    );
    push_line(&mut out, &format!("| IRI | `{}` |", code_escape(&term.iri)));
    push_line(
        &mut out,
        &format!(
            "| Category | {} |",
            md_escape(category_singular(term.category))
        ),
    );
    push_line(
        &mut out,
        &format!(
            "| Slice | {} |",
            slice_link(model, &from, &term.owner_slice)
        ),
    );
    blank(&mut out);

    // ── Badges (the term's small-cardinality categories as color-coded SVGs) ─────
    // A visual summary row: completeness, stability, category, then every box
    // role, logic stereotype, and framework. Each image points at a shared asset
    // emitted in `render_site_lang` from this same source (no dangling path); the
    // detailed, linkable surfaces follow in their own sections below.
    {
        let ctx = crate::coverage::CoverageContext::new(model);
        let badges = crate::badge::term_badges(term, &ctx, model.reasoning.as_ref());
        let row = badges
            .iter()
            .map(|b| {
                format!(
                    "![{}]({}{})",
                    md_escape(&format!("{}: {}", b.family, b.label)),
                    root_href(&from),
                    crate::badge::badge_path(b)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        push_line(&mut out, &row);
        blank(&mut out);
    }

    if let Some(def) = &term.definition {
        heading(&mut out, 2, model.ui("body_definition"));
        line(&mut out, &md_escape(def));
    }

    // ── Progressive disclosure: the DEVELOPER surface first (how to USE the
    // term — quickstart, multi-syntax examples, conformance Do/Don't, diagnostics),
    // then the ACADEMIC surface below (the formal axioms, derivation provenance,
    // projection fidelity, and citation). Every prior section is preserved; the two
    // tiers only reorder them and add the multi-syntax tabs + DL axioms.
    out.push_str(&term_developer_surface(model, term, &from, exec)?);
    out.push_str(&term_academic_surface(model, term, &from, exec));

    // Enriched build-pipeline stage section (only when the term IS a stage).
    append_stage_section(&mut out, model, term, &from);

    Ok(out)
}

/// The DEVELOPER-facing tier of the term page: everything a consumer needs to
/// pick up and USE the term. Leads with the synthesized quickstart and the
/// multi-syntax example tabs (Turtle / JSON-LD / JSON Schema / OpenAPI), then the
/// shape (super-classes, domain, range, SHACL constraints), the conformance
/// Do/Don't pairs, the diagnostics the term might trip, and the advisory / usage
/// context. Returns a Markdown fragment; `from` is the term page's site-relative
/// directory (rebound to an owned `String` so every moved block reads `&from`
/// unchanged).
fn term_developer_surface(
    model: &DocsModel,
    term: &DocTerm,
    from: &str,
    exec: &ExecutableDocsData,
) -> Result<String> {
    let from = from.to_string();
    let mut out = String::new();
    heading(&mut out, 2, model.ui("body_developer_surface"));

    // ── Quickstart (synthesized Turtle skeleton + playground link) ─────────────
    // A pure render of the term's OWN domain/range shape — always present (every
    // term renders some skeleton, or an honest "no skeleton" comment), so a
    // reader gets a copy-paste starting point before reading anything else.
    {
        heading(&mut out, 2, model.ui("body_quickstart"));
        let skeleton = synthesize_quickstart(model, term);
        fenced(&mut out, "turtle", skeleton.trim_end());
        // The `Page::SparqlPlayground` page is emitted only when the pipeline
        // attaches a playground asset (`exec.has_playground()`) — a model-only
        // render never emits `sparql/index.md`, so the link is gated exactly like
        // the sibling "Export" affordance (`append_term_export_section`) to keep
        // the no-dangling-internal-link invariant (`lint::clean_site_has_zero_errors`).
        if exec.has_playground() {
            let playground_href = rel(&from, &Page::SparqlPlayground.dir());
            push_line(
                &mut out,
                &format!(
                    "[Try it in the SPARQL playground]({playground_href}index.md) with \
                     `DESCRIBE <{}>`.",
                    code_escape(&term.iri)
                ),
            );
        }
        blank(&mut out);
    }

    // ── The same example, in several concrete syntaxes (the SyntaxTabProvider
    // seam): Turtle, JSON-LD, and — when the schema digest is attached — the
    // per-term JSON Schema + OpenAPI fragments ("use this term without RDF").
    append_syntax_tabs(&mut out, model, term)?;

    // ── Neighbourhood diagram (the term and its 1-hop relations) ────────────────
    // Gated on the identical predicate as the emission loop in `render_site_lang`
    // so the embedded path always resolves (preserves no-dangling-link).
    if svg::term_has_neighbourhood(term) {
        heading(&mut out, 2, model.ui("body_neighborhood"));
        push_line(
            &mut out,
            &format!(
                "![Term neighborhood]({}diagrams/terms/{}.svg)",
                root_href(&from),
                term_slug(term)
            ),
        );
        blank(&mut out);
    }

    if !term.parents.is_empty() {
        let label = match term.category {
            DocTermCategory::Property => model.ui("body_super_properties"),
            _ => model.ui("body_super_classes"),
        };
        heading(&mut out, 2, label);
        for parent in &term.parents {
            push_line(&mut out, &format!("- {}", term_link(model, &from, parent)));
        }
        blank(&mut out);
    }

    if !term.domain.is_empty() {
        heading(&mut out, 2, model.ui("body_domain"));
        for d in &term.domain {
            push_line(&mut out, &format!("- {}", term_link(model, &from, d)));
        }
        blank(&mut out);
    }

    if !term.range.is_empty() {
        heading(&mut out, 2, model.ui("body_range"));
        for r in &term.range {
            push_line(&mut out, &format!("- {}", term_link(model, &from, r)));
        }
        blank(&mut out);
    }

    // ── Constraints (SHACL node shapes — DISTINCT from the verify-query index) ───
    let constraints: Vec<&str> = model
        .shapes
        .iter()
        .filter(|s| s.target_term == term.iri)
        .flat_map(|s| s.messages.iter().map(String::as_str))
        .collect();
    let mut constraint_msgs: Vec<&str> = constraints;
    constraint_msgs.sort_unstable();
    constraint_msgs.dedup();
    if !constraint_msgs.is_empty() {
        heading(&mut out, 2, model.ui("body_constraints"));
        for msg in constraint_msgs {
            push_line(&mut out, &format!("- {}", md_escape(msg)));
        }
        blank(&mut out);
    }

    // ── Conformance examples (Do/Don't fixtures referencing this term) ──────────
    let mut term_fixtures: Vec<&crate::model::DocFixture> = model
        .fixtures
        .iter()
        .filter(|f| f.terms_referenced.iter().any(|c| c == &term.curie))
        .collect();
    term_fixtures.sort_by(|a, b| {
        a.slice
            .cmp(&b.slice)
            .then_with(|| a.logical_path.cmp(&b.logical_path))
    });
    if !term_fixtures.is_empty() {
        heading(&mut out, 2, model.ui("body_conformance_examples"));
        for fixture in &term_fixtures {
            let label = match fixture.kind {
                crate::model::DocFixtureKind::Wellformed => model.ui("body_label_do"),
                crate::model::DocFixtureKind::CounterExample => model.ui("body_label_dont"),
            };
            push_line(
                &mut out,
                &format!("- **{label}:** `{}`", code_escape(&fixture.logical_path)),
            );
            push_fixture_binding_bullets(&mut out, model, &from, fixture);
            // Live "run validation" control (W1): a counter-example is meant to
            // VIOLATE, so the reader can run the REAL Tier-1 validator in-browser
            // over it and see the actual unified-diagnostics findings (each linking
            // through its helpUri into the constraint catalog). Only when the browser
            // validate asset + bundle ship (`has_bundle()`); the fixture Turtle rides
            // base64 in a data-attribute so its RDF never breaks the HTML.
            if exec.has_bundle()
                && matches!(fixture.kind, crate::model::DocFixtureKind::CounterExample)
            {
                blank(&mut out);
                push_line(
                    &mut out,
                    &format!(
                        "<div class=\"gmeow-validation\"><button class=\"gmeow-run-validation\" \
                         data-turtle=\"{}\" data-origin=\"{}\" data-catalog-href=\"{}index.html\">{}</button>\
                         <div class=\"gmeow-validation-results\"></div></div>",
                        base64_encode(fixture.text.as_bytes()),
                        fixture
                            .logical_path
                            .replace('&', "&amp;")
                            .replace('"', "&quot;"),
                        rel(&from, &Page::ConstraintCatalog.dir()),
                        model.ui("body_run_validation"),
                    ),
                );
                blank(&mut out);
            }
        }
        let fixture_index_href = rel(&from, &Page::FixtureIndex.dir());
        push_line(
            &mut out,
            &format!("- See the [conformance fixtures index]({fixture_index_href}index.md)."),
        );
        blank(&mut out);
    }

    // ── Diagnostics you might hit (present only when a diagnostics digest is attached)
    // Live counts + code list joined from `stage-validate` + `stage-compile-logic` (B1);
    // a term with a genuine `by_term` entry lists each finding (deep-linked into the
    // constraint catalog ONLY when a real `help_uri` resolved); an attached-but-empty
    // digest renders the honest "no diagnostics" line rather than omitting the section —
    // never conflated with the section being absent because no digest was attached at all.
    if let Some(digest) = &model.diagnostics {
        heading(&mut out, 2, model.ui("body_diagnostics_you_might_hit"));
        match digest.by_term.get(&term.iri) {
            Some(findings) if !findings.is_empty() => {
                let mut by_severity: BTreeMap<&str, usize> = BTreeMap::new();
                for finding in findings {
                    *by_severity.entry(finding.severity.as_str()).or_default() += 1;
                }
                let counts = by_severity
                    .iter()
                    .map(|(severity, count)| format!("{count} {severity}"))
                    .collect::<Vec<_>>()
                    .join(" · ");
                push_line(
                    &mut out,
                    &format!("- **{}:** {counts}", model.ui("body_label_severity")),
                );
                for finding in findings {
                    let code_display = match &finding.help_uri {
                        // The display text is md-escaped; the link *target* is the raw
                        // URL (escaping a target corrupts the href).
                        Some(uri) => format!("[`{}`]({uri})", code_escape(&finding.code)),
                        None => format!("`{}`", code_escape(&finding.code)),
                    };
                    push_line(
                        &mut out,
                        &format!(
                            "- {code_display} ({}): {}",
                            md_escape(&finding.category),
                            md_escape(&finding.message)
                        ),
                    );
                }
            }
            _ => {
                push_line(
                    &mut out,
                    &format!("- {}", model.ui("body_diagnostics_none")),
                );
            }
        }
        blank(&mut out);
    }

    // ── Usage Advice (per-term advisory metadata) ───────────────────────────────
    // Field order mirrors the retired Python `_append_usage_advice`: prose first,
    // then consumer-profile guidance. The whole section is suppressed when empty.
    let advice_text: [(&str, &[String]); 5] = [
        (model.ui("body_advice_scope"), &term.scope_notes),
        (model.ui("body_advice_example"), &term.examples),
        (model.ui("body_advice_use_when"), &term.use_when),
        (model.ui("body_advice_avoid_when"), &term.avoid_when),
        (model.ui("body_advice_how_to_use"), &term.how_to_use),
    ];
    let advice_consumer: [(&str, &[String]); 2] = [
        (
            model.ui("body_advice_use_for_consumers"),
            &term.use_for_consumer,
        ),
        (
            model.ui("body_advice_avoid_for_consumers"),
            &term.avoid_for_consumer,
        ),
    ];
    let has_advice = advice_text.iter().any(|(_, v)| !v.is_empty())
        || advice_consumer.iter().any(|(_, v)| !v.is_empty());
    if has_advice {
        heading(&mut out, 2, model.ui("body_usage_advice"));
        for (label, values) in advice_text {
            if !values.is_empty() {
                let joined = values
                    .iter()
                    .map(|v| md_escape(v))
                    .collect::<Vec<_>>()
                    .join(" ");
                push_line(&mut out, &format!("- **{label}:** {joined}"));
            }
        }
        for (label, values) in advice_consumer {
            if !values.is_empty() {
                let joined = values
                    .iter()
                    .map(|c| consumer_link(model, &from, c))
                    .collect::<Vec<_>>()
                    .join(", ");
                push_line(&mut out, &format!("- **{label}:** {joined}"));
            }
        }
        blank(&mut out);
    }

    // ── Related terms (bidirectional: skos:related / pairsWith / seeAlso) ────────
    if !term.related_terms.is_empty() {
        heading(&mut out, 2, model.ui("body_related_terms"));
        for related in &term.related_terms {
            push_line(&mut out, &format!("- {}", term_link(model, &from, related)));
        }
        blank(&mut out);
    }

    // ── Examples using this term (cross-links to the full source on slice pages) ─
    let mut term_examples: Vec<&crate::model::DocExample> = model
        .examples
        .iter()
        .filter(|e| e.terms_referenced.iter().any(|c| c == &term.curie))
        .collect();
    term_examples.sort_by(|a, b| {
        a.slice
            .cmp(&b.slice)
            .then_with(|| a.logical_path.cmp(&b.logical_path))
    });
    if !term_examples.is_empty() {
        heading(&mut out, 2, model.ui("body_examples_using_this_term"));
        for example in term_examples {
            let slice_href = model
                .slices
                .iter()
                .find(|s| s.iri == example.slice)
                .map(|s| rel(&from, &Page::Slice(slice_slug(s)).dir()));
            match slice_href {
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

    // ── Notation grammars (whole-slice cross-link, see GMEOW_LANG_SLICE_IRI) ────
    if term.owner_slice == GMEOW_LANG_SLICE_IRI && !model.grammars.is_empty() {
        heading(&mut out, 2, model.ui("body_notation_grammars"));
        let notation_index_href = rel(&from, &Page::NotationIndex.dir());
        push_line(
            &mut out,
            &format!(
                "- See the [notation grammars index]({notation_index_href}index.md) for the \
                 GMN / GTS / Turtle W3C EBNF surface-syntax exhibits."
            ),
        );
        blank(&mut out);
    }

    // ── Profiles (named profiles whose membership includes this term) ─────────────
    if !term.profiles.is_empty() {
        heading(&mut out, 2, model.ui("body_profiles"));
        let chips = term
            .profiles
            .iter()
            .map(|p| format!("`{}`", code_escape(p)))
            .collect::<Vec<_>>()
            .join(" · ");
        push_line(&mut out, &format!("- {chips}"));
        blank(&mut out);
    }

    Ok(out)
}

/// The ACADEMIC-facing tier of the term page: the formal structure and its
/// provenance. Leads with the Description-Logic axioms the term genuinely
/// carries, then the cross-vocabulary alignments and their preservation caveats,
/// the lowered-logic disciplines, the reasoning verdict and the reasoner-derived
/// "inferred facts" / "unsatisfiable because" panels, the per-term
/// projection-fidelity table, coverage/stability/changelog metadata, and finally
/// the content-addressed cite-this-term citation. Returns a Markdown fragment.
fn term_academic_surface(
    model: &DocsModel,
    term: &DocTerm,
    from: &str,
    exec: &ExecutableDocsData,
) -> String {
    let from = from.to_string();
    let mut out = String::new();
    heading(&mut out, 2, model.ui("body_academic_surface"));

    // ── Description-Logic axioms (the formal reading of the asserted structure) ──
    // Only the axioms the model GENUINELY carries: subsumption from each parent,
    // and — for a property — the standard domain/range axioms. Never a fabricated
    // value restriction or intersection (see `dl_axioms`).
    {
        let axioms = dl_axioms(model, term);
        if !axioms.is_empty() {
            heading(&mut out, 2, model.ui("body_dl_axioms"));
            fenced(&mut out, "text", &axioms.join("\n"));
        }
    }

    // ── Alignments (per-term cross-walks projected from the slice mappings) ──────
    let mut aligns: Vec<&crate::model::DocLinkage> = model
        .linkages
        .iter()
        .filter(|l| l.subject == term.iri)
        .collect();
    aligns.sort_by(|a, b| {
        a.predicate
            .cmp(&b.predicate)
            .then_with(|| a.object.cmp(&b.object))
    });
    if !aligns.is_empty() {
        heading(&mut out, 2, model.ui("body_alignments"));
        let mut any_lossy = false;
        for link in aligns {
            let tag = code_escape(&align_tag(&link.predicate));
            let target = term_link(model, &from, &link.object);
            match approximate_match_note(model, &link.predicate) {
                Some(note) => {
                    any_lossy = true;
                    push_line(&mut out, &format!("- `{tag}` → {target} — *{note}.*"));
                }
                None => push_line(&mut out, &format!("- `{tag}` → {target}")),
            }
        }
        let ledger_href = rel(&from, &Page::LogicLossLedger.dir());
        // One section-level disclosure when any crosswalk is an approximate
        // (lossy) SKOS match, cross-linking the preservation loss ledger that
        // records the per-target structural drops. The prose halves resolve through
        // the UI-chrome catalog; the link target is the language-independent path.
        if any_lossy {
            push_line(
                &mut out,
                &format!(
                    "- *{}({ledger_href}index.md) {}*",
                    model.ui("body_caveat_disclosure_pre"),
                    model.ui("body_caveat_disclosure_post"),
                ),
            );
        }
        // Even an EXACT SKOS/OWL match is a lossy projection once it is LOWERED to
        // the EDOAL / FnO alignment formats — those targets under-approximate the
        // canonical correspondence (they drop the SOL caveats + preservation
        // judgment). Disclose that per-term whenever the term has any crosswalk and
        // the loss ledger declares those lowerings lossy (sourced from the ledger,
        // never hardcoded — an exact EDOAL/FnO row would suppress this note).
        if edoal_fno_lowering_is_lossy() {
            push_line(
                &mut out,
                &format!(
                    "- *{}({ledger_href}index.md) {}*",
                    model.ui("body_caveat_edoal_fno_pre"),
                    model.ui("body_caveat_disclosure_post"),
                ),
            );
        }
        blank(&mut out);
    }

    // ── Logic stereotypes (lowered OntoUML/UFO discipline) ──────────────────────
    if !term.logic_stereotypes.is_empty() {
        heading(&mut out, 2, model.ui("body_logic_stereotypes"));
        let badges = term
            .logic_stereotypes
            .iter()
            .map(|s| format!("`{}`", code_escape(s)))
            .collect::<Vec<_>>()
            .join(" · ");
        let logic_href = rel(&from, &Page::Logic.dir());
        line(
            &mut out,
            &format!("{badges} — see the [Logic & Reasoning]({logic_href}index.md) index."),
        );
    }

    // ── Frameworks (the logical disciplines the term traffics in) ───────────────
    // Rendered as `logic:`-prefixed chips linking the Logic & Reasoning index — the
    // per-term counterpart of the logic-stereotype chips (the framework individuals
    // live in the logic: vocabulary, which is documented via that index).
    if !term.frameworks.is_empty() {
        heading(&mut out, 2, model.ui("body_frameworks"));
        let chips = term
            .frameworks
            .iter()
            .map(|f| format!("`{}`", code_escape(f)))
            .collect::<Vec<_>>()
            .join(" · ");
        let logic_href = rel(&from, &Page::Logic.dir());
        line(
            &mut out,
            &format!("{chips} — see the [Logic & Reasoning]({logic_href}index.md) index."),
        );
    }

    // ── Box role badge (links the four-boxes doctrine when that page exists) ─────
    if let Some(role) = &term.box_role {
        heading(&mut out, 2, model.ui("body_box_role"));
        let label = box_role_label(role);
        if model.four_boxes.is_some() {
            let href = rel(&from, &Page::FourBoxes.dir());
            line(
                &mut out,
                &format!(
                    "[{} (`{}`)]({}index.md)",
                    md_escape(&label),
                    code_escape(role),
                    href
                ),
            );
        } else {
            line(
                &mut out,
                &format!("{} (`{}`)", md_escape(&label), code_escape(role)),
            );
        }
    }

    // ── Formalized by (reverse logic:formalizes back-refs) ──────────────────────
    if !term.formalized_by.is_empty() {
        heading(&mut out, 2, model.ui("body_formalized_by"));
        for subject in &term.formalized_by {
            push_line(&mut out, &format!("- {}", term_link(model, &from, subject)));
        }
        blank(&mut out);
    }

    // ── Tested by (competency questions that exercise this term) ─────────────────
    let mut tested_by: Vec<&crate::model::DocCompetency> = model
        .competencies
        .iter()
        .filter(|c| c.exercises.iter().any(|t| t == &term.iri))
        .collect();
    tested_by.sort_by(|a, b| a.iri.cmp(&b.iri));
    if !tested_by.is_empty() {
        heading(&mut out, 2, model.ui("body_tested_by"));
        for cq in tested_by {
            if cq.rationales.is_empty() {
                let text = md_escape(local_name(&cq.iri));
                match &cq.query_file {
                    Some(qf) => push_line(&mut out, &format!("- {text} (`{}`)", code_escape(qf))),
                    None => push_line(&mut out, &format!("- {text}")),
                }
            }
            for rationale in &cq.rationales {
                let text = md_escape(&one_line(&rationale.lexical_form));
                match &cq.query_file {
                    Some(qf) => push_line(&mut out, &format!("- {text} (`{}`)", code_escape(qf))),
                    None => push_line(&mut out, &format!("- {text}")),
                }
            }
        }
        let competency_index_href = rel(&from, &Page::CompetencyIndex.dir());
        push_line(
            &mut out,
            &format!("- See the [competency questions index]({competency_index_href}index.md)."),
        );
        blank(&mut out);
    }

    // ── Reasoning status (present only when the native reasoner verdict is attached)
    // The textual counterpart of the shared evidence-based reasoning status.
    // Global consistency does not establish the realizability of each class.
    if let Some(verdict) = &model.reasoning {
        heading(&mut out, 2, model.ui("body_reasoning_status"));
        let unsatisfiable =
            term.category == DocTermCategory::Class && verdict.unsatisfiable.contains(&term.iri);
        let status = match verdict.term_status(term) {
            "unsatisfiable" => model.ui("body_reasoning_unsatisfiable"),
            "not-evaluated" => model.ui("body_reasoning_not_evaluated"),
            _ => model.ui("body_reasoning_undetermined"),
        };
        push_line(&mut out, &format!("- {status}"));
        // ── Unsatisfiable because (B3): the derivation chain(s) proving this
        // class necessarily empty, read off the SAME per-term entailment panel
        // "Inferred facts" reads below — never a second join. A term the
        // reasoner proved unsatisfiable but whose witnessing derivation is not
        // (yet) attached renders no extra lines here, rather than a fabricated
        // "because" claim.
        if unsatisfiable {
            let because: Vec<&crate::exec::Entailment> = exec
                .term_entailments
                .get(&term.iri)
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|e| is_unsatisfiable_conclusion(&e.conclusion))
                        .collect()
                })
                .unwrap_or_default();
            if !because.is_empty() {
                push_line(
                    &mut out,
                    &format!(
                        "- **{}:**",
                        model.ui("body_reasoning_unsatisfiable_because")
                    ),
                );
                for entailment in because {
                    push_line(
                        &mut out,
                        &format!(
                            "  - `{}` (via `{}`)",
                            code_escape(&entailment.conclusion),
                            code_escape(&entailment.rule)
                        ),
                    );
                }
            }
        }
        blank(&mut out);
    }

    // ── Inferred facts (present only when the pipeline attached B3 entailment data
    // AND this term has a matching derivation) ─────────────────────────────────
    // The reasoner-derived "why" panel: for a documented term appearing in the
    // subject/predicate/object position of a materialized derivation's conclusion
    // or premises, list the firing rule, the concluded axiom, and its premises.
    // `exec.term_entailments` is empty in a model-only render (the genuine
    // `ExecutableDocsData` layering seam — see `exec.rs`), so the panel is simply
    // absent there, never a fabricated "no entailments" claim.
    if let Some(entailments) = exec.term_entailments.get(&term.iri) {
        heading(&mut out, 2, model.ui("body_term_entailments"));
        for entailment in entailments {
            push_line(
                &mut out,
                &format!(
                    "- **{}** ⟹ `{}`",
                    code_escape(&entailment.rule),
                    code_escape(&entailment.conclusion)
                ),
            );
            for premise in &entailment.premises {
                push_line(&mut out, &format!("  - via `{}`", code_escape(premise)));
            }
        }
        blank(&mut out);
    }

    // ── How this term degrades under projection (present only when the dynamic
    // per-term loss digest is attached) ──────────────────────────────────────────
    // The dynamic per-shape join (B2): DISTINCT from the static whole-program rows
    // on `Page::LogicLossLedger` (`owl-dl`, `datalog`, …) — this surfaces ONLY the
    // `property-path:<shape-iri>` rows the live `stage-mappings` projection ledger
    // emits for a `logic:PathShape` this term owns. An attached-but-empty join
    // renders the honest "carried exactly by every projection" text rather than
    // omitting the section — never conflated with the section being absent
    // because no digest was attached at all.
    if let Some(digest) = &model.term_loss {
        heading(&mut out, 2, model.ui("body_term_projection_degradation"));
        match digest.by_term.get(&term.iri) {
            Some(rows) if !rows.is_empty() => {
                push_line(
                    &mut out,
                    "| Target | Preservation kind | Complexity class | Lossy drops |",
                );
                push_line(&mut out, "| --- | --- | --- | --- |");
                for row in rows {
                    let drops = if row.lossy_drops.is_empty() {
                        "—".to_string()
                    } else {
                        row.lossy_drops
                            .iter()
                            .map(|d| md_escape(&one_line(d)))
                            .collect::<Vec<_>>()
                            .join("<br>")
                    };
                    push_line(
                        &mut out,
                        &format!(
                            "| `{}` | `{}` | `{}` | {} |",
                            code_escape(&row.target),
                            code_escape(&row.preservation_kind),
                            code_escape(&row.complexity_class),
                            drops,
                        ),
                    );
                }
            }
            _ => {
                push_line(
                    &mut out,
                    &format!("- {}", model.ui("body_term_projection_degradation_none")),
                );
            }
        }
        blank(&mut out);
    }

    // ── Documentation coverage (always present) ──────────────────────────────────
    // The six richness dimensions this term carries, read from the shared coverage
    // source — exactly the predicates behind the `docs/missing-*` lint, so the page
    // and the gate can never disagree about what a term is missing.
    {
        let ctx = crate::coverage::CoverageContext::new(model);
        let cov = crate::coverage::term_coverage(term, &ctx);
        heading(&mut out, 2, model.ui("body_documentation_coverage"));
        let badges = crate::coverage::DIMENSIONS
            .iter()
            .zip(cov.flags())
            .map(|(dim, present)| format!("{} {}", if present { "✓" } else { "✗" }, dim.label))
            .collect::<Vec<_>>()
            .join(" · ");
        push_line(
            &mut out,
            &format!(
                "- **{} of {} dimensions present.** {}",
                cov.present_count(),
                crate::coverage::TermCoverage::TOTAL,
                badges
            ),
        );
        push_line(
            &mut out,
            &format!(
                "- See [Documentation health]({}index.md) for coverage across the whole vocabulary.",
                rel(&Page::Term(term_slug(term)).dir(), &Page::Health.dir())
            ),
        );
        blank(&mut out);
    }

    // ── Stability (always present; tier-derived default or explicit) ──────────────
    heading(&mut out, 2, model.ui("body_stability"));
    push_line(
        &mut out,
        &format!(
            "- **{}:** {}",
            model.ui("body_label_status"),
            term.stability.label()
        ),
    );
    blank(&mut out);

    // ── Changelog (added-in version + reified per-release entries) ──────────────
    if term.added_in_version.is_some() || !term.changelog.is_empty() {
        heading(&mut out, 2, model.ui("body_changelog"));
        if let Some(version) = &term.added_in_version {
            push_line(
                &mut out,
                &format!(
                    "- **{}:** {}",
                    model.ui("body_label_added_in"),
                    md_escape(version)
                ),
            );
        }
        for entry in &term.changelog {
            let source = entry.source.wire();
            match &entry.note {
                Some(note) => push_line(
                    &mut out,
                    &format!(
                        "- **{}** ({source}) — {}",
                        md_escape(&entry.version),
                        md_escape(note)
                    ),
                ),
                None => push_line(
                    &mut out,
                    &format!("- **{}** ({source})", md_escape(&entry.version)),
                ),
            }
        }
        blank(&mut out);
    }

    // ── Citation (permalink + genuine content address + cite-this affordance) ───
    // The term IRI is the dereferenceable permalink. The content address is the
    // RDFC-1.0 canonical digest of the term's defining triples (gmeow:definitionDigest),
    // so `<iri>@<digest>` pins the exact definition this page describes. The concept
    // DOI (read from metadata/gmeow-self.ttl) cites the whole ontology; the owner
    // slice's identifier cites the slice when one is registered. This is the
    // per-term instance of the page-level cite-this-surface (`append_cite_this_surface`).
    heading(&mut out, 2, model.ui("body_citation"));
    push_line(
        &mut out,
        &format!("- **{}:** <{}>", model.ui("body_label_permalink"), term.iri),
    );
    if !term.content_digest.is_empty() {
        push_line(
            &mut out,
            &format!(
                "- **{}:** `{}@{}`",
                model.ui("body_label_content_address"),
                term.iri,
                code_escape(&term.content_digest)
            ),
        );
    }
    if let Some(doi) = &model.concept_doi {
        push_line(
            &mut out,
            &format!(
                "- **{}:** [{}](https://doi.org/{})",
                model.ui("body_label_cite_ontology"),
                md_escape(doi),
                doi
            ),
        );
    }
    if let Some(identifier) = model
        .slices
        .iter()
        .find(|s| s.iri == term.owner_slice)
        .and_then(|s| s.identifier.as_ref())
    {
        push_line(
            &mut out,
            &format!(
                "- **{}:** {}",
                model.ui("body_label_cite_slice"),
                md_escape(identifier)
            ),
        );
    }
    blank(&mut out);

    out
}

// ── Multi-syntax example tabs (the concrete `SyntaxTabProvider` OCP seam) ─────
//
// A term's example is rendered in several concrete serializations. Each is
// produced by a `SyntaxTabProvider`: a PURE `fn(&DocTerm, &DocsModel) ->
// Result<Option<DocSyntaxTab>>` yielding one labeled, language-tagged code block, or
// `None` when this term carries no honest content for that syntax (the tab is
// then simply absent — the seam's honest layering, never a fabricated
// placeholder). The providers run in the fixed order of `SYNTAX_TAB_PROVIDERS`.
//
// A concrete syntax adds its provider to `SYNTAX_TAB_PROVIDERS`; the term-page
// renderer and the shared tab container consume that declared provider list.

/// One rendered example-syntax tab: a language-tagged code block with a label.
struct DocSyntaxTab {
    /// Stable slug id for the tab (part of the tab identity a future interactive
    /// renderer keys on; the static markdown render uses `label`/`lang`/`body`).
    #[allow(dead_code)]
    id: String,
    /// The human tab label (a format proper noun, e.g. `Turtle`, or a localized
    /// UI-chrome string for the schema panels).
    label: String,
    /// The fenced-code language tag (e.g. `turtle`, `json`).
    lang: String,
    /// The code-block body.
    body: String,
}

/// A pure example-syntax tab producer. `None` = this term has no honest content
/// for the syntax (absent tab).
type SyntaxTabProvider = fn(&DocTerm, &DocsModel) -> Result<Option<DocSyntaxTab>>;

/// The ordered example-syntax providers consumed by the term-page renderer.
const SYNTAX_TAB_PROVIDERS: &[SyntaxTabProvider] = &[
    turtle_syntax_tab,
    jsonld_syntax_tab,
    json_schema_syntax_tab,
    openapi_syntax_tab,
    python_syntax_tab,
    rust_syntax_tab,
];

/// Escape `text` for embedding as a non-raw, double-quoted Python string
/// literal: backslash and `"` are the only two characters a Python string
/// escape needs to neutralize, and an embedded literal newline (the
/// pretty-printer's structural whitespace, not a JSON string's *own*
/// characters — JSON already `\n`-escapes those) must become `\n` too, since a
/// non-raw, non-triple-quoted literal cannot contain one. Unlike a raw
/// triple-quoted literal (`r'''...'''`), this form has no forbidden
/// subsequence: `json.loads` decoding the result always reproduces `text`
/// exactly, regardless of its content.
pub(super) fn python_str_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

/// The Python (Pydantic) example tab: construct-and-validate the term's worked
/// instance against its generated `gmeow_models` model — importing the model IS
/// reading the term. Present only for a modeled class (one carrying a schema
/// fragment); the payload is the SAME quickstart instance the Turtle / JSON-LD
/// tabs render, so the code never drifts from the RDF example.
fn python_syntax_tab(term: &DocTerm, model: &DocsModel) -> Result<Option<DocSyntaxTab>> {
    // Only a modeled class has a Pydantic model + a JSON-Schema fragment.
    if model
        .schema_fragments
        .as_ref()
        .and_then(|fragments| fragments.schema_by_term.get(&term.iri))
        .is_none()
    {
        return Ok(None);
    }
    let Some(skeleton) = syntax_skeleton(model, term)? else {
        return Ok(None);
    };
    let payload = quickstart_turtle_to_jsonld(&skeleton)
        .map_err(|error| error.with_context(format!("python_syntax_tab for {}", term.iri)))?;
    // A raw triple-quoted literal (`r'''...'''`) cannot escape a `'''` run, so
    // an unlucky payload would terminate the string early and emit broken
    // Python; a non-raw double-quoted literal has no such forbidden
    // subsequence.
    let payload_literal = python_str_escape(&payload);
    let module = pydantic_module_slug(&term.owner_slice);
    let class = pydantic_class_name(&term.iri);
    let body = format!(
        "import json\n\n\
         from gmeow_models.{module} import {class}\n\n\
         # The same worked instance shown in the Turtle / JSON-LD tabs, validated\n\
         # against the ontology-derived model:\n\
         payload = json.loads(\"{payload_literal}\")\n\
         obj = {class}.model_validate(payload)\n\
         print(obj.model_dump(by_alias=True, exclude_none=True))"
    );
    Ok(Some(DocSyntaxTab {
        id: "python".to_string(),
        label: "Python".to_string(),
        lang: "python".to_string(),
        body,
    }))
}

/// The narrowest Rust raw-string hash-fence width that is safe for `text`: one
/// more `#` than the longest run of `#` immediately following any `"` in
/// `text`. A raw string `r###"..."###` terminates at the first `"` followed by
/// AT LEAST as many `#` as the fence width, so a fixed one-`#` fence
/// (`r#"..."#`) is unsafe whenever `text` itself contains a `"#` sequence;
/// widening past every such run in `text` makes early termination impossible.
pub(super) fn rust_raw_fence_width(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut max_run = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut run = 0usize;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] == b'#' {
                run += 1;
                j += 1;
            }
            max_run = max_run.max(run);
        }
        i += 1;
    }
    max_run + 1
}

/// Render `text` as a Rust raw-string literal (delimiters included) using a
/// hash fence wide enough that `text` cannot terminate it early — see
/// [`rust_raw_fence_width`]. For the common case (no `"#`-like run in `text`)
/// this is the familiar `r#"..."#`.
pub(super) fn rust_raw_string_literal(text: &str) -> String {
    let fence = "#".repeat(rust_raw_fence_width(text));
    format!("r{fence}\"{text}\"{fence}")
}

/// The Rust example tab: parse the SAME quickstart instance and validate it with
/// the native GMEOW validator — the shipped Rust surface paired with the Python
/// tab, from the one worked instance. Present under the same modeled-class
/// condition as [`python_syntax_tab`].
fn rust_syntax_tab(term: &DocTerm, model: &DocsModel) -> Result<Option<DocSyntaxTab>> {
    if model
        .schema_fragments
        .as_ref()
        .and_then(|fragments| fragments.schema_by_term.get(&term.iri))
        .is_none()
    {
        return Ok(None);
    }
    let Some(skeleton) = syntax_skeleton(model, term)? else {
        return Ok(None);
    };
    let turtle = format!("{QUICKSTART_TURTLE_PREAMBLE}{skeleton}");
    // A fixed one-`#` fence breaks if `turtle` ever contains `"#`; widen the
    // fence to whatever this particular worked instance needs.
    let turtle_literal = rust_raw_string_literal(&turtle);
    let body = format!(
        "// Parse the same worked instance shown in the Turtle tab and validate it\n\
         // with the native GMEOW validator (the shipped Rust surface).\n\
         let turtle = {turtle_literal};\n\
         let dataset = purrdf::parse_turtle(turtle)?;\n\
         let report = gmeow_validate::validate(&dataset)?;\n\
         assert!(report.conforms());"
    );
    Ok(Some(DocSyntaxTab {
        id: "rust".to_string(),
        label: "Rust".to_string(),
        lang: "rust".to_string(),
        body: body.trim_end().to_string(),
    }))
}

/// Whether a synthesized quickstart skeleton carries at least one Turtle triple
/// line (a non-comment, non-blank line) — the class/property skeletons do; the
/// individual/datatype/other skeletons are comment-only, so their example tabs
/// are honestly absent.
fn skeleton_has_triple(skeleton: &str) -> bool {
    skeleton
        .lines()
        .any(|l| !l.trim_start().is_empty() && !l.trim_start().starts_with('#'))
}

/// Select the term's honest example surface; a selected class/property must
/// produce RDF rows rather than quietly losing every executable syntax tab.
fn syntax_skeleton(model: &DocsModel, term: &DocTerm) -> Result<Option<String>> {
    let skeleton = synthesize_quickstart(model, term);
    if skeleton_has_triple(&skeleton) {
        return Ok(Some(skeleton));
    }
    if matches!(
        term.category,
        DocTermCategory::Class | DocTermCategory::Property
    ) {
        return Err(Diag::of_kind(crate::render_error::Syntax::new(format!(
            "selected class/property {} produced no quickstart RDF rows",
            term.iri,
        ))));
    }
    Ok(None)
}

/// The Turtle example tab: the term's synthesized quickstart skeleton verbatim.
/// Present for a class/property (they synthesize a triple skeleton); absent for
/// an individual/datatype/other whose skeleton is comment-only.
fn turtle_syntax_tab(term: &DocTerm, model: &DocsModel) -> Result<Option<DocSyntaxTab>> {
    let Some(body) = syntax_skeleton(model, term)? else {
        return Ok(None);
    };
    Ok(Some(DocSyntaxTab {
        id: "turtle".to_string(),
        label: "Turtle".to_string(),
        lang: "turtle".to_string(),
        body: body.trim_end().to_string(),
    }))
}

/// The JSON-LD example tab: the SAME quickstart Turtle, transcoded to a
/// deterministic JSON-LD document via the native `purrdf` codec. `None` when the
/// term has no triple-bearing skeleton. Parse or serialization failure is a
/// typed error retaining the codec cause.
fn jsonld_syntax_tab(term: &DocTerm, model: &DocsModel) -> Result<Option<DocSyntaxTab>> {
    let Some(skeleton) = syntax_skeleton(model, term)? else {
        return Ok(None);
    };
    let body = quickstart_turtle_to_jsonld(&skeleton)
        .map_err(|error| error.with_context(format!("jsonld_syntax_tab for {}", term.iri)))?;
    Ok(Some(DocSyntaxTab {
        id: "jsonld".to_string(),
        label: "JSON-LD".to_string(),
        lang: "json".to_string(),
        body,
    }))
}

/// The JSON Schema fragment tab ("use this term without RDF"): the class's
/// `$defs` fragment from the attached schema digest. `None` when no digest is
/// attached (source-only render) or the term has no fragment — honest absence.
fn json_schema_syntax_tab(term: &DocTerm, model: &DocsModel) -> Result<Option<DocSyntaxTab>> {
    let Some(body) = model
        .schema_fragments
        .as_ref()
        .and_then(|fragments| fragments.schema_by_term.get(&term.iri))
    else {
        return Ok(None);
    };
    let body = body.clone();
    Ok(Some(DocSyntaxTab {
        id: "json-schema".to_string(),
        label: model.ui("body_schema_fragment").to_string(),
        lang: "json".to_string(),
        body,
    }))
}

/// The OpenAPI fragment tab: the class's `components/schemas` fragment from the
/// attached schema digest. `None` under the same honest-absence conditions as
/// [`json_schema_syntax_tab`].
fn openapi_syntax_tab(term: &DocTerm, model: &DocsModel) -> Result<Option<DocSyntaxTab>> {
    let Some(body) = model
        .schema_fragments
        .as_ref()
        .and_then(|fragments| fragments.openapi_by_term.get(&term.iri))
    else {
        return Ok(None);
    };
    let body = body.clone();
    Ok(Some(DocSyntaxTab {
        id: "openapi".to_string(),
        label: model.ui("body_openapi_fragment").to_string(),
        lang: "json".to_string(),
        body,
    }))
}

/// The canonical `@prefix`/`@base` preamble under which a synthesized quickstart
/// skeleton (which uses bare `gmeow:`/`logic:`/`math:`/`owl:`… CURIEs and the
/// relative `<subject>`/`<...>` placeholders) parses as a self-contained Turtle
/// document: a base resolves the relative placeholders, and every prefix a
/// canonical grounding prefix used by documented terms is declared.
const QUICKSTART_TURTLE_PREAMBLE: &str = concat!(
    "@base <https://blackcatinformatics.ca/gmeow/example/> .\n",
    "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
    "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n",
    "@prefix math: <https://blackcatinformatics.ca/math/> .\n",
    "@prefix lang: <https://blackcatinformatics.ca/lang/> .\n",
    "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n",
    "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n",
    "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
    "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n",
    "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n",
);

/// Transcode one synthesized quickstart Turtle skeleton to a deterministic
/// JSON-LD document using the native `purrdf` codecs: parse the skeleton (under
/// [`QUICKSTART_TURTLE_PREAMBLE`]) into an in-memory dataset, then serialize with
/// `serialize_dataset_to_jsonld` (sorted keys / `@graph`, byte-stable). A selected
/// skeleton must parse and yield RDF rows; failures retain their cause. A pure
/// in-memory transcode — no `generated/` disk read.
fn quickstart_turtle_to_jsonld(skeleton: &str) -> Result<String> {
    let doc = format!("{QUICKSTART_TURTLE_PREAMBLE}{skeleton}");
    let dataset = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None).map_err(|cause| {
        Diag::of_kind(crate::render_error::Syntax::caused_by(
            "parse selected quickstart Turtle".into(),
            cause,
        ))
    })?;
    if dataset.quad_count() == 0 {
        return Err(Diag::of_kind(crate::render_error::Syntax::new(
            "selected quickstart produced no RDF rows".into(),
        )));
    }
    purrdf::native_codecs::jsonld::serialize_dataset_to_jsonld(&dataset).map_err(|cause| {
        Diag::of_kind(crate::render_error::Syntax::caused_by(
            "serialize selected quickstart as JSON-LD".into(),
            cause,
        ))
    })
}

/// Render the ordered example-syntax tabs for a term under the "Example in
/// multiple syntaxes" heading. Since the site is static markdown/HTML, "tabs" are
/// sequential labeled fenced code blocks (the house style — cf. the loss-ledger's
/// labeled blocks). Emits nothing when no provider yields a tab (a source-only
/// render of an individual/datatype term produces no tabs — honest absence).
fn append_syntax_tabs(out: &mut String, model: &DocsModel, term: &DocTerm) -> Result<()> {
    let tabs: Vec<DocSyntaxTab> = SYNTAX_TAB_PROVIDERS
        .iter()
        .map(|provider| provider(term, model))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    if tabs.is_empty() {
        return Ok(());
    }
    heading(out, 2, model.ui("body_example_syntaxes"));
    for tab in &tabs {
        push_line(out, &format!("**{}**", md_escape(&tab.label)));
        blank(out);
        fenced(out, &tab.lang, &tab.body);
    }
    Ok(())
}

// ── Description-Logic axiom rendering (academic surface) ─────────────────────
//
// A small, HONEST DL-notation formatter over the model data a term genuinely
// carries: subsumption (`⊑`) from each parent, and — for a property — the
// standard domain (`∃R.⊤ ⊑ D`) and range (`⊤ ⊑ ∀R.C`) axioms that `rdfs:domain`
// / `rdfs:range` denote. It never invents a value restriction the model does not
// carry (no fabricated `∃r.C` filler, no `⊓` intersection), so every line is a
// faithful DL reading of an asserted triple. Uses the standard glyphs (`⊑ ∃ ∀
// ⊤`); `lang-bridge::gmn_symbology` is a glyph-cost table, not a DL renderer, so
// this formatter owns the notation.

/// The compact DL name for an IRI: its CURIE when it names a documented term or a
/// GMEOW-family IRI, else the bare local name — never the full IRI.
fn dl_name(model: &DocsModel, iri: &str) -> String {
    if let Some(term) = model.terms.iter().find(|t| t.iri == iri) {
        return term.curie.clone();
    }
    let curie = to_curie(iri);
    if curie != iri {
        return curie;
    }
    local_name(iri).to_string()
}

/// The Description-Logic axioms a term GENUINELY carries, one per line (empty
/// when none): `C ⊑ P` per parent (class subsumption / property sub-role), plus,
/// for a property, the standard domain (`∃R.⊤ ⊑ D`) and range (`⊤ ⊑ ∀R.C`) axioms.
fn dl_axioms(model: &DocsModel, term: &DocTerm) -> Vec<String> {
    let self_name = term.curie.clone();
    let mut lines: Vec<String> = Vec::new();
    for parent in &term.parents {
        lines.push(format!("{self_name} ⊑ {}", dl_name(model, parent)));
    }
    if term.category == DocTermCategory::Property {
        for d in &term.domain {
            lines.push(format!("∃{self_name}.⊤ ⊑ {}", dl_name(model, d)));
        }
        for r in &term.range {
            lines.push(format!("⊤ ⊑ ∀{self_name}.{}", dl_name(model, r)));
        }
    }
    lines
}

// ── Page-level cite-this-surface (the generalized citation affordance) ────────
//
// The per-term Citation section (blake3 content digest + slice DOI + ontology
// concept DOI) generalized to a page-level "cite this page" block on every OTHER
// durable surface (fixtures index, competency index, notation index / grammar,
// pipeline DAG, glossary). The term page keeps its richer content-addressed form
// inline (`term_academic_surface`); this block is distinct from the
// per-page provenance footer.

/// Whether `page` is a durable, citable surface carrying the generalized
/// "cite this page" block. The term page is excluded here — its cite-this-term
/// surface is the richer inline Citation section.
fn page_is_citable(page: &Page) -> bool {
    matches!(
        page,
        Page::FixtureIndex
            | Page::CompetencyIndex
            | Page::NotationIndex
            | Page::Grammar(_)
            | Page::PipelineDag
            | Page::Glossary
    )
}

/// Append the generalized page-level "cite this page" block: the whole-ontology
/// concept DOI (when the model carries one) and the page's stable site locator.
/// A no-op on a non-citable page (see [`page_is_citable`]).
pub(super) fn append_cite_this_surface(out: &mut String, model: &DocsModel, page: &Page) {
    if !page_is_citable(page) {
        return;
    }
    blank(out);
    heading(out, 2, model.ui("body_cite_this_page"));
    if let Some(doi) = &model.concept_doi {
        push_line(
            out,
            &format!(
                "- **{}:** [{}](https://doi.org/{})",
                model.ui("body_label_cite_ontology"),
                md_escape(doi),
                doi
            ),
        );
    }
    push_line(
        out,
        &format!(
            "- **{}:** `{}`",
            model.ui("body_label_permalink"),
            code_escape(&page.html_path())
        ),
    );
    blank(out);
}
