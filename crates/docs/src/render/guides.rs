// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Recipes, learning paths, and the four-boxes guide.

use crate::model::DocsModel;

use super::{
    Page, blank, code_escape, curie_link, fenced, heading, line, md_escape, one_line, push_line,
    rel, synthesize_composed_quickstart,
};

// ── Guides: recipes / learning paths / four boxes ──────────────────────────────

pub(super) fn md_recipe_index(model: &DocsModel) -> String {
    let from = Page::RecipeIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_recipes"));
    line(
        &mut out,
        &format!(
            "**{}** task-oriented adoption recipes — short, goal-named guides that show how to \
             model one recurring task in GMEOW, each backed by canonical example files and the \
             vocabulary terms it exercises.",
            model.recipes.len()
        ),
    );
    if model.recipes.is_empty() {
        line(&mut out, model.ui("body_no_recipes"));
        return out;
    }
    push_line(&mut out, "| Recipe | Goal |");
    push_line(&mut out, "| --- | --- |");
    // model.recipes is already sorted by slug.
    for recipe in &model.recipes {
        let href = rel(&from, &Page::Recipe(recipe.slug.clone()).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {} |",
                md_escape(&recipe.title),
                href,
                md_escape(&one_line(&recipe.goal)),
            ),
        );
    }
    blank(&mut out);
    out
}

pub(super) fn md_recipe(model: &DocsModel, slug: &str) -> String {
    let Some(recipe) = model.recipes.iter().find(|r| r.slug == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_recipe_not_found"));
        return out;
    };
    let from = Page::Recipe(slug.to_string()).dir();
    let mut out = String::new();
    heading(&mut out, 1, &recipe.title);
    line(
        &mut out,
        &format!("`{}` · recipe", code_escape(&recipe.slug)),
    );

    heading(&mut out, 2, model.ui("body_goal"));
    line(&mut out, &md_escape(&recipe.goal));

    // ── Quickstart (composed over every member term, so a promoted recipe is
    // copy-paste-runnable as a whole, not just per-term) ────────────────────────
    if !recipe.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_quickstart"));
        let skeleton = synthesize_composed_quickstart(model, &recipe.term_curies);
        fenced(&mut out, "turtle", skeleton.trim_end());
        blank(&mut out);
    }

    if !recipe.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_terms_used"));
        for curie in &recipe.term_curies {
            push_line(&mut out, &format!("- {}", curie_link(model, &from, curie)));
        }
        blank(&mut out);
    }

    if !recipe.example_paths.is_empty() {
        heading(&mut out, 2, model.ui("body_example_files"));
        for path in &recipe.example_paths {
            push_line(&mut out, &format!("- `{}`", code_escape(path)));
        }
        blank(&mut out);
    }

    if !recipe.follow_pages.is_empty() {
        heading(&mut out, 2, model.ui("body_read_next"));
        for page in &recipe.follow_pages {
            push_line(&mut out, &format!("- `{}`", code_escape(page)));
        }
        blank(&mut out);
    }

    // Learning paths that fold this recipe in.
    let mut hosting: Vec<&crate::model::DocLearningPath> = model
        .learning_paths
        .iter()
        .filter(|p| p.recipe_slugs.iter().any(|s| s == slug))
        .collect();
    hosting.sort_by(|a, b| a.slug.cmp(&b.slug));
    if !hosting.is_empty() {
        heading(&mut out, 2, model.ui("body_part_of"));
        for path in hosting {
            let href = rel(&from, &Page::LearningPath(path.slug.clone()).dir());
            push_line(
                &mut out,
                &format!("- [{}]({}index.md)", md_escape(&path.title), href),
            );
        }
        blank(&mut out);
    }
    out
}

pub(super) fn md_learning_path_index(model: &DocsModel) -> String {
    let from = Page::LearningPathIndex.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_learning_paths"));
    line(
        &mut out,
        &format!(
            "**{}** curated adoption journeys — ordered itineraries that sequence recipes, \
             example files, and vocabulary terms so a developer learns to model a whole area end \
             to end.",
            model.learning_paths.len()
        ),
    );
    if model.learning_paths.is_empty() {
        line(&mut out, model.ui("body_no_learning_paths"));
        return out;
    }
    push_line(&mut out, "| Learning path | Audience | Goal |");
    push_line(&mut out, "| --- | --- | --- |");
    // model.learning_paths is already sorted by slug.
    for path in &model.learning_paths {
        let href = rel(&from, &Page::LearningPath(path.slug.clone()).dir());
        push_line(
            &mut out,
            &format!(
                "| [{}]({}index.md) | {} | {} |",
                md_escape(&path.title),
                href,
                md_escape(&one_line(&path.audience)),
                md_escape(&one_line(&path.goal)),
            ),
        );
    }
    blank(&mut out);
    out
}

pub(super) fn md_learning_path(model: &DocsModel, slug: &str) -> String {
    let Some(path) = model.learning_paths.iter().find(|p| p.slug == slug) else {
        let mut out = String::new();
        heading(&mut out, 1, slug);
        line(&mut out, model.ui("body_learning_path_not_found"));
        return out;
    };
    let from = Page::LearningPath(slug.to_string()).dir();
    let mut out = String::new();
    heading(&mut out, 1, &path.title);
    line(
        &mut out,
        &format!("`{}` · learning path", code_escape(&path.slug)),
    );

    push_line(&mut out, "| Field | Value |");
    push_line(&mut out, "| --- | --- |");
    push_line(
        &mut out,
        &format!("| Audience | {} |", md_escape(&one_line(&path.audience))),
    );
    blank(&mut out);

    heading(&mut out, 2, model.ui("body_goal"));
    line(&mut out, &md_escape(&path.goal));

    // ── Quickstart (composed over every member term, so a promoted learning
    // path is copy-paste-runnable as a whole, not just per-term) ───────────────
    if !path.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_quickstart"));
        let skeleton = synthesize_composed_quickstart(model, &path.term_curies);
        fenced(&mut out, "turtle", skeleton.trim_end());
        blank(&mut out);
    }

    if !path.recipe_slugs.is_empty() {
        heading(&mut out, 2, model.ui("body_recipes"));
        for recipe_slug in &path.recipe_slugs {
            let title = model
                .recipes
                .iter()
                .find(|r| &r.slug == recipe_slug)
                .map(|r| r.title.clone())
                .unwrap_or_else(|| recipe_slug.clone());
            // Link only when the recipe page exists in the model.
            if model.recipes.iter().any(|r| &r.slug == recipe_slug) {
                let href = rel(&from, &Page::Recipe(recipe_slug.clone()).dir());
                push_line(
                    &mut out,
                    &format!("- [{}]({}index.md)", md_escape(&title), href),
                );
            } else {
                push_line(&mut out, &format!("- {}", md_escape(&title)));
            }
        }
        blank(&mut out);
    }

    if !path.term_curies.is_empty() {
        heading(&mut out, 2, model.ui("body_terms_used"));
        for curie in &path.term_curies {
            push_line(&mut out, &format!("- {}", curie_link(model, &from, curie)));
        }
        blank(&mut out);
    }

    if !path.example_paths.is_empty() {
        heading(&mut out, 2, model.ui("body_example_files"));
        for p in &path.example_paths {
            push_line(&mut out, &format!("- `{}`", code_escape(p)));
        }
        blank(&mut out);
    }

    if !path.adoption_targets.is_empty() {
        heading(&mut out, 2, model.ui("body_projects_toward"));
        let cells: Vec<String> = path
            .adoption_targets
            .iter()
            .map(|t| format!("`{}`", code_escape(t)))
            .collect();
        line(&mut out, &cells.join(", "));
    }
    out
}

pub(super) fn md_four_boxes(model: &DocsModel) -> String {
    let mut out = String::new();
    // The curated prose is authored Markdown; emit it verbatim (its SPDX comment
    // header is an HTML comment and renders inertly). Fall back to a stub when
    // absent so the page is never empty.
    match &model.four_boxes {
        Some(prose) => {
            out.push_str(prose);
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        None => {
            heading(&mut out, 1, model.ui("body_what_is_this"));
            line(
                &mut out,
                "The four-boxes doctrine prose is not available in this build.",
            );
        }
    }
    out
}
