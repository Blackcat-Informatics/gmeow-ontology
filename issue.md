# Issue #2932: Retain valid claim contributors beside qualified carriers

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, logic, rust

## Body

Part of #2781 and #2748. Blocked by: nothing. Blocks: #2781 and #2748.

## Problem

`ClaimResolver::statement_from_sources` returns a refusal as soon as any contributing reifier is qualified. Canonical axiom deduplication can combine an independently valid carrier with a different qualified carrier for the same statement and scope. The latter then cancels the valid premise.

This is a source-backed missed-refutation path. No runtime reproducer has been executed yet.

## Required behavior

- Resolve eligibility and refusals per original carrier through canonical deduplication and claim ingress.
- Retain every independently admitted contribution. One qualified carrier cannot erase another carrier's valid assertion, and a valid assertion cannot erase the qualified carrier's boundary.
- Execution and witness provenance identify actual admitted contributors. Preserve module/context selection, evidence coordinates and the invocation-bound source/preparation authority.
- Keep conflicting or unsupported carriers visible; do not choose a preferred source or flatten qualified context.

## Acceptance

- Two valid module-selected carriers asserting disjoint memberships yield a native contradiction. Adding a separately qualified carrier for one identical statement does not suppress that contradiction.
- Qualified-only input remains refused or undecided with its precise reason, and its rows are not falsely reported as admitted through another carrier.
- Existing source-authority, mixed-contributor and context controls remain valid.
- Focused real-consumer checks and updated reasoning doctrine cover the resulting contract.

Principles 4, 7, 9, 10 and 17.


## Captured comment summary

The captured comment links the module-composition change in pull request 2929 and offers planning controls. It contains no technical finding or additional implementation requirement. This summary omits interface instructions and promotional text; the issue body and all other captured fields are retained.

Source: https://github.com/Blackcat-Informatics/gmeow-ontology/issues/2932#issuecomment-6980040627

Original Markdown SHA-256: `30d7dea4be8217256ff1f9536b98284988574d98f8985c50cda30d4f584b42e2`. Original JSON SHA-256: `6cc9256a5b72efd73dda3d6a48650e8b6ecf20de2dd3d7efbfa8306a12d97a27`.
