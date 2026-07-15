<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Citations

GMEOW keeps one canonical citation ledger at `metadata/references.ttl`.
Generated bibliography formats live under `generated/references/` and are never
hand-edited.

This follows Principle 4: the Turtle ledger is the source of truth; CSL JSON,
BibTeX, and Markdown are lossy projections for tools and readers.

## Citing GMEOW Itself

GMEOW has a registered DOI — the **concept DOI**
[`10.67342/26w4o`](https://doi.org/10.67342/26w4o) — the always-latest citation
anchor. Cite it with the metadata in [`CITATION.cff`](../CITATION.cff):

> Blackcat Informatics® Inc. and Patrick Audley. *GMEOW — Global Metadata and
> Entity Ontology for the Web.* doi:10.67342/26w4o

The DOI denotes the lineage (the FRBR Work); to pin the exact release you reasoned
over, also cite its `owl:versionIRI` (`https://blackcatinformatics.ca/gmeow/<version>`)
and, for byte-exact identity, the release's GTS head id / SWHID. See
[`docs/dois.md`](./dois.md) for the single-anchor DOI strategy.

## What Goes In The Ledger

Record durable references that GMEOW cites in authored files, docs, code
comments, issues, PRs, review comments, or examples:

- external standards, vocabularies, schemas, ontologies, and specifications
- DOI-backed papers, datasets, software releases, or reports
- web pages used as external authority or source material
- explicit bibliographic citation strings already present in data fixtures or docs
- GitHub issue, PR, or review text that cites an external source

Do not treat every internal tracker reference as a bibliography item. An
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
cargo run -p gmeow-dev-cli -- references-backfill
```

The command reads authored local files and accessible GitHub issue/PR/review
text through the `gh` CLI. It writes:

- `metadata/references.ttl` — the canonical curated ledger
- `dist/reference-candidates.jsonl` — an audit trail of harvested candidates

After changing `metadata/references.ttl`, regenerate exports:

```bash
make sync
make sync SYNC_MODE=check SYNC_OUTPUTS=generated
```

For a complete PR, run `make check`.

## Generated Exports

The `references` generator owns:

- `generated/references/references.csl.json`
- `generated/references/references.bib`
- `generated/references/references.md`

If these drift, update `metadata/references.ttl` or rerun the generator. Do not
patch generated reference files directly.
