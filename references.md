# Referenced artifacts — issue #1646

Everything the issue names, checked against disk. Facts only: whether a
file is a viable port source, or may be relicensed, is decided in the plan.
Paths resolve against the main checkout, then against a sibling checkout
only when that repo is named (first path component, or in the prose just
before the reference). "not here; exists in sibling(s)" is NOT resolved.

## Paths

| Reference | Exists | Resolved in | Kind | Lines | Licence header |
|---|---|---|---|---|---|
| `packages/python/gmeow_models/` | yes | repo | dir | — | — |
| `__about__.py` | NO | repo | — | — | — |
| `__init__.py` | NO | repo | — | — | — |
| `_base.py` | NO | repo | — | — | — |
| `models.py` | NO | repo | — | — | — |
| `crates/pipeline/src/stages/pydantic.rs:19-35` | yes | repo | file | 494 | SPDX: AGPL-3.0-only |
| `README.md` | yes | repo | file | 901 | — |
| `Blackcat-Informatics/purrdf` | NO | repo | — | — | — |
| `generated/schemas/gmeow.linkml.yaml` | NO | repo | — | — | — |
| `gmeow.schema.json` | NO | repo | — | — | — |
| `packages/python/tests/` | NO | repo | — | — | — |
| `test_conformance.py` | NO | repo | — | — | — |
| `.py` | NO | repo | — | — | — |
| `docs/schema-projections.md` | yes | repo | file | 277 | SPDX: CC-BY-4.0 |
| `.coderabbit.yaml` | NO | repo | — | — | — |
| `origin/main` | NO | repo | — | — | — |
| `CLAUDE.md` | yes | repo | file | 95 | — |
| `packages/python/pyproject.toml` | yes | repo | file | 38 | — |
| `crates/pipeline/src/stages/pydantic.rs` | yes | repo | file | 494 | SPDX: AGPL-3.0-only |
| `crates/pipeline/src/stages/pydantic.rs:29-33` | yes | repo | file | 494 | SPDX: AGPL-3.0-only |
| `pypi-publish-gmeow.yml` | NO | repo | — | — | — |
| `crates/pipeline/src/stages/snapshots/` | yes | repo | dir | — | — |
| `crates/pipeline/src/stages/term_manifest.rs` | yes | repo | file | 690 | SPDX: AGPL-3.0-only |
| `packages/python/gmeow_models/models.py` | yes | repo | file | 25833 | — |

### History: `packages/python/gmeow_models/`

- `057d833c0 fix(slice-quality): declare the harvest axis environment-divergent, like its sibling`
- `b10a1de8a feat(1682): complete owl:->logic: slice-authoring migration closeout`
- `d0f9fb5b1 chore(projection): refresh pipeline cache model`
- `cbcaa45a0 test(1211): re-bless the docs goldens against merged main, and record the coverage delta`
- `8c377a19f fix(1211): stereotype the new failure classes, and translate the vocabulary this branch added`
- `e042c2c87 fix(1672): pay attestation slice-quality debt, ground SubKind, robustify render test`
- `5a47f95e5 fix(1211): a CSS colour is not an issue reference`
- `0ad817cb2 fix(1376): pin the claim reps to the baseline profile positively, and lower the test-reach ratchet`
- `d12382f69 fix(1376): derive the head-id pattern from its logic: constraint, and discharge the seven orphan math classes`
- `ed4597e27 fix(1376): retire the claims dictionary's fanout row and committed path`

### History: `crates/pipeline/src/stages/pydantic.rs:19-35`

- `83e0f7e87 feat: restore ontology-linked Pydantic module exports`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `4ee5359b4 fix(policy): close deficiency-gate gaps (Principles 4 and 7)`
- `74c60199b feat(1682): teach every canonical-store reader the logic: authoring spelling`
- `a76168cc6 build(gate): one producer, one run — remove make regen as an entry point`
- `92e42bea0 docs(1402): sweep all `make sync` references to `make regen`/`make check-sync``
- `9120cd022 feat(1571): cut pydantic package over to purrdf emit_pydantic`
- `24b763209 fix(logic): complete oracle-only orphan removal + fix workspace rustdoc links (#1572)`
- `48aa8c6d8 fix(grounding): close final review gaps (Principles 4, 7, 17)`

### History: `README.md`

- `d51b04c14 docs: align callable authoring guidance with canonical slice ownership`
- `4f3411112 fix(1768): ship one-file prover consumer`
- `108d350d2 docs(1668): rename gmeow-rdf references to purrdf in documentation`
- `7fa96d0ac docs: correct the claims that name crates and tools this repo does not have`
- `247e3d81a fix(1211): close the pre-merge audit's five blocking findings`
- `1a14ea0d2 docs: add the gate and pipeline doctrine, grounded in this branch's defects`
- `311ca56fe refactor(logic): author the workflow groundings in the kernel, not a domain slice`
- `a76168cc6 build(gate): one producer, one run — remove make regen as an entry point`
- `6bb22778d build(gate): schedule check on accurate dependencies, and move breadth lanes to CI`
- `ce9c69840 docs(1211): document the console, and gate the documents against rot`

### History: `docs/schema-projections.md`

- `83e0f7e87 feat: restore ontology-linked Pydantic module exports`
- `6b93fc910 build: authenticate the O3 full-LTO pipeline producer`
- `4ee5359b4 fix(policy): close deficiency-gate gaps (Principles 4 and 7)`
- `f6a780c98 feat(1667): purrdf 0.12.0 and own the docs playground engine`
- `042c866d9 docs(1571): retire pydantic-conformance lane + correct schema-projections doc`
- `79573bbee perf(sync): complete unified projection DAG (Principles 4, 11)`
- `90d37700b feat(pipeline): SHACL-derived Pydantic v2 package emitter (gmeow_models); retire flat gmeow.py`
- `de2191df5 docs: repoint CLI references from the retired Python surface to the Rust bins`
- `dfcd5f08f refactor(slices): move core/logic → grounding/logic + path sweep`
- `41a933fee feat(769): factor claim modality into six orthogonal axes + lossless projection (ME3 of #766) (#1070)`

### History: `CLAUDE.md`

- `fae3d17e2 docs: distinguish logic semantics from generic execution`
- `01848ca4b fix(ci): repair producer receipt portability`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `7514b578e chore(policy): make deficiency failures impossible to authorize`
- `d6513324b docs(1682): align derive-source doctrine to logic: (owl:/RDFS is a generated projection)`
- `1a14ea0d2 docs: add the gate and pipeline doctrine, grounded in this branch's defects`
- `a76168cc6 build(gate): one producer, one run — remove make regen as an entry point`
- `92e42bea0 docs(1402): sweep all `make sync` references to `make regen`/`make check-sync``
- `abb442471 docs(1600): rewrite committed-artifact/merge=ours wording to the ignored-projection model`
- `0251c4aef chore: purge process-narration from .deficiencies; document it is code-only`

### History: `packages/python/pyproject.toml`

- `a76168cc6 build(gate): one producer, one run — remove make regen as an entry point`
- `92e42bea0 docs(1402): sweep all `make sync` references to `make regen`/`make check-sync``
- `042c866d9 docs(1571): retire pydantic-conformance lane + correct schema-projections doc`
- `9120cd022 feat(1571): cut pydantic package over to purrdf emit_pydantic`
- `79573bbee perf(sync): complete unified projection DAG (Principles 4, 11)`
- `fad0131a0 feat(pipeline,packaging): derive the wheel version from the ontology owl:versionInfo`
- `6f832f3c2 feat(packaging): packages/python gmeow-ontology wheel + re-created publish workflow`

### History: `crates/pipeline/src/stages/pydantic.rs`

- `83e0f7e87 feat: restore ontology-linked Pydantic module exports`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `4ee5359b4 fix(policy): close deficiency-gate gaps (Principles 4 and 7)`
- `74c60199b feat(1682): teach every canonical-store reader the logic: authoring spelling`
- `a76168cc6 build(gate): one producer, one run — remove make regen as an entry point`
- `92e42bea0 docs(1402): sweep all `make sync` references to `make regen`/`make check-sync``
- `9120cd022 feat(1571): cut pydantic package over to purrdf emit_pydantic`
- `24b763209 fix(logic): complete oracle-only orphan removal + fix workspace rustdoc links (#1572)`
- `48aa8c6d8 fix(grounding): close final review gaps (Principles 4, 7, 17)`

### History: `crates/pipeline/src/stages/pydantic.rs:29-33`

- `83e0f7e87 feat: restore ontology-linked Pydantic module exports`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `4ee5359b4 fix(policy): close deficiency-gate gaps (Principles 4 and 7)`
- `74c60199b feat(1682): teach every canonical-store reader the logic: authoring spelling`
- `a76168cc6 build(gate): one producer, one run — remove make regen as an entry point`
- `92e42bea0 docs(1402): sweep all `make sync` references to `make regen`/`make check-sync``
- `9120cd022 feat(1571): cut pydantic package over to purrdf emit_pydantic`
- `24b763209 fix(logic): complete oracle-only orphan removal + fix workspace rustdoc links (#1572)`
- `48aa8c6d8 fix(grounding): close final review gaps (Principles 4, 7, 17)`

### History: `crates/pipeline/src/stages/snapshots/`

- `83e0f7e87 feat: restore ontology-linked Pydantic module exports`
- `b3bc3c8e4 test: align carrier goldens with typed result and loss contracts`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `820383b89 feat(1638): retire the process-model slice in favour of the canonical logic: spine`
- `b672bf11a feat(1410): route provenance generator through the A-Box annotation contract`
- `ede04129a test(1600): re-bless goldens after 0.2.0 bump + origin/main glossary integration`
- `5c5dea554 chore(1600): bump workspace version to 0.2.0 (breaking untrack-generated change)`
- `5cdd1492b fix(1527): resolve make-check gate failures — frame, examples, gufo grounding, baselines (#1527)`
- `070098802 test(snapshots): rebaseline goldens for the authored reasoning-program vocabulary`
- `9fde49ad6 test(snapshots): re-bless coverage-ratchet + pydantic-tags after grounding`

### History: `crates/pipeline/src/stages/term_manifest.rs`

- `83e0f7e87 feat: restore ontology-linked Pydantic module exports`
- `a159730f8 perf(logic): integrate the certified native pipeline`
- `22842c8c8 fix: close optimized producer admission gaps`
- `f01e5fc7c feat(deps): repin purrdf to the 1.1.0 crates.io release (#1645)`
- `a319905e6 fix(pipeline): stabilize release history and GTS sink (#1699)`
- `184a4257d perf(test): cache authenticated suite intermediates (Principles 4, 7)`
- `fea7e1b54 feat(errors): forward diagnostics fold + DiagNode cache lane (advances #1336)`
- `036673f9b feat(errors): dissolve pipeline error enums into Diag substrate DiagKinds (advances #1336)`
- `d095bd1af build(deps): adopt purrdf 0.3.0 + migrate workspace to Rust edition 2024`
- `aeaf3e5e5 feat(validate): on-gate guard banning generated/ disk reads in pipeline stages (#1278)`

### History: `packages/python/gmeow_models/models.py`

- `057d833c0 fix(slice-quality): declare the harvest axis environment-divergent, like its sibling`
- `b10a1de8a feat(1682): complete owl:->logic: slice-authoring migration closeout`
- `d0f9fb5b1 chore(projection): refresh pipeline cache model`
- `cbcaa45a0 test(1211): re-bless the docs goldens against merged main, and record the coverage delta`
- `e042c2c87 fix(1672): pay attestation slice-quality debt, ground SubKind, robustify render test`
- `0ad817cb2 fix(1376): pin the claim reps to the baseline profile positively, and lower the test-reach ratchet`
- `d12382f69 fix(1376): derive the head-id pattern from its logic: constraint, and discharge the seven orphan math classes`
- `ed4597e27 fix(1376): retire the claims dictionary's fanout row and committed path`
- `242693255 fix(1373): close the merge's integration gaps — fragment, seam, dependency, catalogs, pins`
- `7a0d5ea15 chore(docs): regenerate goldens + pydantic models for Stage-2 vocab/annotation changes (PR #1683)`

## Identifiers (files containing each at HEAD)

- `main`: 2078
- `CompiledSchema`: 9
- `py.typed`: 6
- `json_schema_extra`: 4
- `emitted_models_agree_with_packed_schema_defs`: 0
- `definitionDigest`: 24
- `__about__`: 5
- `__init__`: 8
- `_base`: 150
- `models`: 265
- `Makefile`: 54
- `docstring`: 13
- `PydanticClassConfig`: 1
- `PydanticPackageTopology`: 1
- `emit_pydantic`: 6
- `PydanticPackageTopology::new`: 1
- `Annotation`: 323
- `gmeow_models.models`: 0
- `models_python_tags_module.snap`: 0
- `AccessibilityAssertion`: 29
- `enriched_compiled_schema`: 6

## Workspace packages named

- `gmeow-ontology`

## URLs

- https://discord.gg/coderabbit
- https://blackcatinformatics.ca/gmeow/slices/tags
- https://blackcatinformatics.ca/gmeow/Tagging
