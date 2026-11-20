// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;

use crate::model::{DocFixture, DocFixtureKind, DocsModel};

use super::{
    Page, blank, code_escape, heading, line, md_escape, one_line, push_line, rel, slice_name,
};

/// The filename stem of a fixture's slice-relative logical path (no directory,
/// no `.ttl` extension) — the basis for the best-effort Do/Don't pairing.
fn fixture_stem(logical_path: &str) -> &str {
    logical_path
        .rsplit('/')
        .next()
        .unwrap_or(logical_path)
        .trim_end_matches(".ttl")
}

/// The number of leading `-`-separated tokens two fixture stems share (e.g.
/// `plan-wellformed` / `plan-missing-successmode` share 1: `plan`). Used to
/// pick, for a counter-example, the best-matching well-formed fixture in the
/// same slice — the repo's fixture-naming convention puts the shared concept
/// stem first and the wellformed/violating descriptor last.
fn shared_prefix_tokens(a: &str, b: &str) -> usize {
    a.split('-')
        .zip(b.split('-'))
        .take_while(|(x, y)| x == y)
        .count()
}

/// Render a fixture's expected outcome as a Markdown link into the constraint
/// catalog when [`DocFixture::catalog_slug`] resolved a genuine match, else a
/// plain code span (never a dead link).
fn fixture_violation_code_display(from: &str, fixture: &DocFixture) -> Option<String> {
    let code = fixture.violation_code.as_ref()?;
    Some(match &fixture.catalog_slug {
        Some(slug) => {
            let catalog_href = rel(from, &Page::ConstraintCatalog.dir());
            format!("[`{}`]({catalog_href}index.md#{slug})", code_escape(code))
        }
        None => format!("`{}`", code_escape(code)),
    })
}

/// Push the Do/Don't detail bullets shared by the fixture index and the
/// per-term conformance-examples section: the violation code (catalog-linked
/// when resolved) and the rationale, when the fixture carries a binding.
pub(super) fn push_fixture_binding_bullets(
    out: &mut String,
    model: &DocsModel,
    from: &str,
    fixture: &DocFixture,
) {
    if let Some(code_display) = fixture_violation_code_display(from, fixture) {
        push_line(
            out,
            &format!(
                "  - **{}:** {code_display}",
                model.ui("body_label_violation_code")
            ),
        );
    }
    for rationale in &fixture.rationales {
        let text = rationale.display_text(&fixture.slice, &model.translations, &model.lang);
        // Escape the ampersand so Markdown displays entity references as literal text.
        let text = md_escape(&one_line(text)).replace('&', "\\&");
        push_line(out, &format!("  - {text}"));
    }
}

/// The conformance Do/Don't fixtures index, grouped by slice. Each
/// counter-example is paired with its best-matching well-formed fixture in the
/// same slice (by shared filename-stem prefix tokens) when one exists; a
/// well-formed fixture never picked as a pair partner is listed once on its
/// own so none are lost.
pub(super) fn md_fixture_index(model: &DocsModel) -> String {
    let from = Page::FixtureIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_conformance_fixtures"));
    line(
        &mut out,
        &format!(
            "**{}** conformance fixture(s) — well-formed instances and counter-examples \
             that check the owning slice's validation behaviour. The \"{}\" entry shows \
             the available violation code and rationale from `tests/example-conformance.ttl`.",
            model.fixtures.len(),
            model.ui("body_label_dont"),
        ),
    );
    if model.fixtures.is_empty() {
        line(&mut out, model.ui("body_no_conformance_fixtures"));
        return out;
    }

    let mut by_slice: BTreeMap<&str, Vec<&DocFixture>> = BTreeMap::new();
    for fixture in &model.fixtures {
        by_slice
            .entry(fixture.slice.as_str())
            .or_default()
            .push(fixture);
    }

    for (slice_iri, fixtures) in by_slice {
        heading(&mut out, 2, &slice_name(model, slice_iri));

        let wellformed: Vec<&DocFixture> = fixtures
            .iter()
            .copied()
            .filter(|f| f.kind == DocFixtureKind::Wellformed)
            .collect();
        let counter_examples: Vec<&DocFixture> = fixtures
            .iter()
            .copied()
            .filter(|f| f.kind == DocFixtureKind::CounterExample)
            .collect();

        let mut paired_wellformed: std::collections::BTreeSet<&str> =
            std::collections::BTreeSet::new();

        for ce in &counter_examples {
            let ce_stem = fixture_stem(&ce.logical_path);
            let best = wellformed
                .iter()
                .copied()
                .filter_map(|wf| {
                    let n = shared_prefix_tokens(fixture_stem(&wf.logical_path), ce_stem);
                    (n > 0).then_some((n, wf))
                })
                .max_by(|(n1, wf1), (n2, wf2)| {
                    n1.cmp(n2)
                        .then_with(|| wf2.logical_path.cmp(&wf1.logical_path))
                });

            heading(&mut out, 3, &ce.title);
            if let Some((_, wf)) = best {
                paired_wellformed.insert(wf.logical_path.as_str());
                push_line(
                    &mut out,
                    &format!(
                        "- **{}:** `{}`",
                        model.ui("body_label_do"),
                        code_escape(&wf.logical_path)
                    ),
                );
                push_fixture_binding_bullets(&mut out, model, &from, wf);
            }
            push_line(
                &mut out,
                &format!(
                    "- **{}:** `{}`",
                    model.ui("body_label_dont"),
                    code_escape(&ce.logical_path)
                ),
            );
            push_fixture_binding_bullets(&mut out, model, &from, ce);
            blank(&mut out);
        }

        // Well-formed fixtures never picked as a pair partner (e.g. a standalone
        // flagship-scenario fixture with no counter-example twin) — listed once
        // so none are silently dropped from the index.
        for wf in &wellformed {
            if paired_wellformed.contains(wf.logical_path.as_str()) {
                continue;
            }
            heading(&mut out, 3, &wf.title);
            push_line(
                &mut out,
                &format!(
                    "- **{}:** `{}`",
                    model.ui("body_label_do"),
                    code_escape(&wf.logical_path)
                ),
            );
            push_fixture_binding_bullets(&mut out, model, &from, wf);
            blank(&mut out);
        }
    }
    out
}
