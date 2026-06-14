<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Citations

GMEOW keeps one canonical citation ledger at `metadata/references.ttl`.
Generated bibliography formats live under `generated/references/` and are never
hand-edited.

This follows Principle 4: the Turtle ledger is the source of truth; CSL JSON,
BibTeX, and Markdown are lossy projections for tools and readers.

## What Goes In The Ledger

Record durable references that GMEOW cites in authored files, docs, code
comments, issues, PRs, review comments, or examples:

- external standards, vocabularies, schemas, ontologies, and specifications
- DOI-backed papers, datasets, software releases, or reports
- web pages used as external authority or source material
- explicit bibliographic citation strings already present in data fixtures or docs
- GitHub issue, PR, or review text that cites an external source

Do not treat every internal `#123` issue reference as a bibliography item. An
internal issue or PR becomes a citing carrier when its body or comments cite an
external work.

## Authoring Rule

Use the existing citation model:

- The cited thing is a `gmeow:CreativeWork`.
- The citation relationship is a `gmeow:CitationAct`.
- Use `gmeow:viaSelector` with a `gmeow:Selector` when the file path, line,
  comment URL, page, or quote matters.
- Use `gmeow:bibliographicCitation` only as a display string on the cited work.
  It is not the canonical citation relationship.
- Add source locations as `gmeow:sourceLocation`; use stable URLs, file paths,
  or original filenames.

Prefer `gmeow:intentBridgedByReference` for standards and vocabularies aligned
by reference. Use `gmeow:intentCitesAsDataSource` when the cited work is evidence
or source material for a claim.

## Updating The Ledger

Run the backfill tool when adding or refreshing citation coverage:

```bash
uv run --package gmeow-dev gmeow-dev references-backfill
```

The command reads authored local files and accessible GitHub issue/PR/review
text through the `gh` CLI. It writes:

- `metadata/references.ttl` — the canonical curated ledger
- `dist/reference-candidates.jsonl` — an audit trail of harvested candidates

After changing `metadata/references.ttl`, regenerate exports:

```bash
make regenerate
make check-generated
```

For a complete PR, run `make check`.

## Generated Exports

The `references` generator owns:

- `generated/references/references.csl.json`
- `generated/references/references.bib`
- `generated/references/references.md`

If these drift, update `metadata/references.ttl` or rerun the generator. Do not
patch generated reference files directly.
