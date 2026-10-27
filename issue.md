# Issue #2005: Fail closed when schema vocabulary source discovery is unavailable

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, quality, rust

## Body

Fail closed when required schema value-vocabulary source discovery is unavailable. The current collector silently returns from directory-open failures and drops directory-entry errors, allowing declared ontology inputs to disappear without a diagnostic. Principles 4 and 7 require the selected input universe to be complete and verified.

Acceptance:

- Change ontology_module_files to return Result<Vec<PathBuf>, Diag>. Propagate failure through JsonSchemaStage::input_files, SchemasStage::input_files, PydanticStage::input_files and load_ontology_store. No caller may convert discovery failure into an empty list or weaker enrichment.
- Require a readable slices source root. Directory-open, entry and path-classification failures must name the affected path and fail the operation. Selected module sources must be readable ordinary files. Preserve following existing symlinks; do not add a blanket symlink exclusion.
- Preserve the current recursive selection of every exact module.ttl name and its deterministic, per-directory sorted traversal. Do not replace it with a two-level collector or narrow the selected source universe. Unrelated filenames remain outside the selection.
- An existing readable tree with no modules, or valid modules with no qualifying vocabulary members, is legitimate empty enrichment. Missing or unreadable required discovery is not. Keep selected-source read and Turtle parse failures terminal, without partially returning a dataset.
- Add two bounded authored controls: a tiny nested temporary tree protecting recursive selection, deterministic order, ignored filenames and legitimate empty vocabularies; and required-source refusal covering absent slices, slices as a regular file and malformed selected Turtle. Use real temporary data; avoid permission-dependent cases, races, mocks or a new filesystem abstraction solely for testing. No test reads or constructs the repository corpus.
- Compile the actual pipeline and developer CLI callers and use normal hooks. Preserve healthy discovery, vocabulary derivation and every output format; this is an input-admission repair, not the native schema migration.

Remaining contracts: #1776, #1777, #1336 and #1962. Complete native enriched-schema documents, upstream LinkML adoption with persistent mapping/loss evidence, grammar/correspondence owners, workspace Serde retirement and actual corpus/release acceptance remain unfinished in their existing owners. The standing user exception prohibits local optimized producer, canonical corpus/fixture production, synchronization and full-gate execution and authorizes sequential merges with CI waived. Report unverified acceptance accurately.


## Comments (0)

