# `generated/` history rewrite ledger

This ledger records the repository-wide history rewrite prepared on
2026-07-18 for issue #1600. The rewrite removes the `generated/` path from every
reachable commit while preserving the repository's source history, branch and
tag trees outside that path, active worktree tips, GitHub squash-audit refs, and
Git notes.

The rewrite implements One Canonical Source (Principle 4): generated output is
reproducible from canonical sources and is no longer a second, historical copy.
The maps and fingerprints in this ledger make the destructive migration
auditable under Principle 7.

## Rewrite contract

- Tool: `git-filter-repo` `c1511bf3728f`
- Command: `git filter-repo --path generated/ --invert-paths --prune-empty never --prune-degenerate never --no-ff --preserve-commit-hashes --preserve-commit-encoding --force`
- Commits mapped one-to-one: 7,167; no commit was dropped.
- Commits still containing `generated/`: 0.
- Heads preserved: 11.
- Tags preserved: 10.
- `refs/ghprsq/pr/*` refs preserved and remapped: 693.
- `refs/notes/ghprsq` notes preserved and remapped: 461.
- Remote refs captured before rewriting: 1,575.
- Original GitHub repository size snapshot: 18,612,162 KiB.
- Filtered mirror pack size before publication: 90,147,462 bytes.

Pruning empty or degenerate commits was deliberately disabled. This keeps the
commit map total and removes ambiguity when remapping signed squash-audit notes.
Historical commit IDs necessarily changed because their trees changed.

## Preserved in-progress worktree tips

| Branch | Original tip | Rewritten tip |
| --- | --- | --- |
| `paudley/1344-chase-termination-ladder` | `70783cbd375b23101596009943e57d402d920b38` | `f14c40e73345c60c22c137be0ba434db7f07d192` |
| `paudley/1428-native-numeric-builtins` | `d4b2b38ce5dfc8552b01d8f39b244502a851fc3d` | `dc76a07b9e0fc19499ad5a842fa9a7265d511c99` |
| `paudley/1551-logic-only-shape-retirement` | `7fd6eacc70e607de1a3bf91cd7b9e489195aa700` | `7b32a53977826971366a9d98c76de88e82a8975a` |

The first branch had 42 clean local commits beyond its remote tip
`de0243df9bdeeb2d002d28ff97c00a482ca7f3c3`. Its exact local tip was imported
before filtering and is the branch tip represented here.

## Principal refs

- `main`: `9032c2f6e6b8803488842e85478612ce3944d242` -> `1361ab1dfcaba673c64b5a47b49c195524344291`
- `refs/notes/ghprsq`: `0ce958e6a9cd6dba11d53802795ce2b60d944f83` -> `d72940f185289c547a2ec3bcf9e6523e236c9818`

The rewritten notes commit is signed and reuses every original note blob while
placing it at the rewritten target commit ID. The complete old/new target map is
included as `audit/notes-map.tsv`.

## Signatures and tags

Removing a path from a commit changes that commit's object ID, so historical
commit signatures cannot remain valid. Rewriting annotated tag targets also
requires new tag objects; `v0.2.0` and `v1.0.2-last-apache` remain annotated but
their old signatures are not retained on the rewritten tag objects. The exact
original tag objects remain in the immutable pre-rewrite mirror, and their IDs
are recoverable from the ref and fingerprint manifests in this ledger.

The ledger commit and the rewritten `ghprsq` notes commit are newly signed with
the maintainer's configured Git signing key.

## File semantics

- `maps/commit-map.tsv` maps every old commit ID to its rewritten ID.
- `maps/ghprsq-ref-map.tsv` maps every squash-audit ref target.
- `maps/notes-map.tsv` maps every note target and records the reused note blob.
- `refs/` captures the authoritative remote refs and the augmented pre-filter
  refs after importing the clean local worktree tip.
- `fingerprints/` proves that every head and tag tree is byte-identical outside
  `generated/` before and after filtering.
- `audit/` captures exact pre/post `ghprsq` refs and note listings.
- `github/` captures the pre-rewrite repository, open-PR, and ruleset state.

SHA-256 digests for the key source manifests are recorded in `manifest.json`.
