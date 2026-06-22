<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Guides

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/guides` · **tier: core**

The pedagogy and adoption-guides slice. It dogfoods the curated **recipes** and **learning paths**
that were formerly hardcoded in the documentation tooling, modelling them as `gmeow:Recipe` and
`gmeow:LearningPath` individuals so they become authored-once GMEOW data sourced through the slice
catalog rather than literals embedded in a Python module. The documentation renderer projects them
into the `recipes/`, `learning-paths/`, and `four-boxes/` pages of the ontology docs site.

## Recipes

A `gmeow:Recipe` is a task-oriented adoption guide: a short, goal-named recipe that shows how to model
one recurring modelling task in GMEOW, backed by canonical example files and the vocabulary terms they
exercise. Each recipe carries:

| Property | Meaning |
|---|---|
| `gmeow:guideSlug` | the stable, filesystem-safe slug (the docs page key) |
| `gmeow:guideTitle` | the human headline |
| `gmeow:guideGoal` | the modelling outcome the recipe helps a developer achieve |
| `gmeow:usesExamplePath` | slice-relative paths of the canonical example files it builds on |
| `gmeow:usesTerm` | the GMEOW vocabulary terms it exercises (by IRI) |
| `gmeow:followsGuidePath` | documentation-relative follow-on pages to read next |

The six seeded recipes cover person names without a preferred-name slot, contested or attributed
facts, events and participants, documents for schema.org consumers, offline GTS distribution, and
graph-RAG dataset lineage.

## Learning paths

A `gmeow:LearningPath` is a curated adoption journey for a named audience: an itinerary that sequences
recipes, example files, and terms so a developer learns to model a whole area end to end and sees
which external vocabularies the native model projects toward. Each path carries `gmeow:guideSlug`,
`gmeow:guideTitle`, `gmeow:learningAudience`, `gmeow:guideGoal`, one or more `gmeow:includesRecipe`
edges, `gmeow:usesExamplePath` / `gmeow:usesTerm`, and `gmeow:adoptionTarget` (external-vocabulary
prefixes). The five seeded paths cover modelling a person, modelling a contested claim, publishing web
structured data, shipping offline GTS docs, and auditing AI / graph-RAG pipelines.

## Doctrine

- **Curated pedagogy, not domain axioms.** A recipe or path sequences *existing* examples and terms;
  nothing here asserts a fact about the modelled world.
- **By reference (Principle 5).** Guides name example files and follow-on pages by path strings and
  point at terms by IRI — they never copy content.
- **Authored once (Principle 4).** Each recipe and path is authored here once and projected outward
  by the documentation renderer, replacing the retired hardcoded tooling defaults.

## Dependencies

| Slice | Why |
|---|---|
| `kernel` | the foundational vocabulary the guides slice grafts onto; the only declared dependency, since the guides layer references other slices' terms only as open `gmeow:usesTerm` objects, never by import |
