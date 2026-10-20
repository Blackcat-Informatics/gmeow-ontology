# Requirement candidates — issue #1646

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 37 | checkbox | - [ ] Decide per capability: recover in-repo, wait on the upstream issue, or ratify the loss in `docs/schema-projections.md`. |
| B2 | body | 38 | checkbox | - [ ] Restore some form of term-to-model linkage (`definitionDigest` and a docs back-link are the minimum that makes the wheel navigable). |
| B3 | body | 39 | checkbox | - [ ] Re-establish a live import + `model_json_schema()` agreement check somewhere — off-gate is fine, absent is not. |
| B4 | body | 40 | checkbox, scope-out, modal | - [ ] Decide whether OWL-only/RBox properties should be surfaced in the developer schemas or documented as deliberately out of scope. |
| B5 | body | 44 | list-under-requirement-heading | - A consumer of the wheel can get from a model class back to its ontology term. |
| B6 | body | 45 | list-under-requirement-heading | - The generated package is proven to import, in some lane. |
| C1.1 | comment 1 (coderabbitai, 2026-07-27T00:16:37Z) | 18 | checkbox | - [ ] <!-- {"checkboxId": "8d4f2b9c-3e1a-4f7c-a9b2-d5e8f1c4a7b9"} --> Create Plan |
| C3.1 | comment 3 (paudley, 2026-09-05T07:34:11Z) | 21 | modal | The fourth item — whether OWL-only / RBox properties like `gmeow:resentDate` should appear in the developer schemas at all — is unaffected by any of this and remains a real open question on a non-Python surface. |
| C3.2 | comment 3 (paudley, 2026-09-05T07:34:11Z) | 23 | modal | *Correcting the record rather than leaving a recommendation that would have sent the next reader into authorization-required work.* |
| C4.1 | comment 4 (paudley, 2026-09-05T07:34:52Z) | 19 | modal | The gap it addresses is still real — *"a package that structurally agrees yet fails to import passes every gate"* — but the remedy is a decision, not a task to pick up. Worth noting an alternative that stays Rust-side: the emitter could assert its own output parses as valid Python via a Rust-side syntax check, which catches the "fails to import" class of defect without a Python interpreter in the loop. That is weaker than a real import, and the difference should be stated rather than glossed. |
| C5.1 | comment 5 (paudley, 2026-09-06T15:30:33Z) | 27 | modal | 1. **The topology must be a total partition.** `PydanticPackageTopology::new` rejects unknown routes, unused modules, and prefix collisions, so *every* definition must be routed — including the synthesized `…Enum` / `_…EnumValue` helpers and shared types like `Annotation`, which have no single owning slice and need a declared home. |
| C5.2 | comment 5 (paudley, 2026-09-06T15:30:33Z) | 28 | modal | 2. **The wheel layout changes.** `gmeow_models.models` stops being the import surface. Per #1654 no real publish has ever run, so there are no existing consumers to break — but the packaging assertions in `pypi-publish-gmeow.yml` (`__init__.py`, `py.typed`) and `emitted_models_agree_with_packed_schema_defs` both need to hold against a routed tree. |
| C6.1 | comment 6 (paudley, 2026-09-06T15:32:39Z) | 40 | modal | So they should not be deleted as dead weight until the restoration lands — they are the oracle to restore against, and then they become live goldens again. |
| C7.1 | comment 7 (paudley, 2026-10-20T09:06:20Z) | 18 | modal | must fail. Emit the package README from the same selected topology. |
| C9.1 | comment 9 (paudley, 2026-10-20T13:09:36Z) | 17 | modal | The fetched base has no intervening changes and the predicted integration is clean. No conflict resolution or source synchronization is required. Focused qualification applies to the unchanged candidate. |

## Comments that matched no rule

- comment 2 (paudley, 2026-09-05T03:06:28Z)
- comment 8 (paudley, 2026-10-20T12:55:05Z)
