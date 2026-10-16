# Exact upstream source pin

Authorization: "YOu can pin to purrdf git main for now if that's useful".
The source-selection step was completed while the broader goal was paused.
The user has since resumed the full goal; source API migration is now in progress.

The live official main ref resolved to
7734b013d19b68600ad24c4e8aa5a40955a8e89e. The five direct production dependencies
and standalone fuzz facade now select that exact revision with their existing
major-3 compatibility requirement. No moving branch or local path is selected.
The upstream workspace reports version 3.0.1; this Git source is distinct from
the registry release with the same version.

make maint-pin-purrdf completed successfully. Both committed lockfiles select
exactly the same 31 PurRDF packages, all version 3.0.1 at that complete Git source
identity, with no registry PurRDF duplicates. Direct TOML inspection confirmed
identical production/fuzz identities. The substrate validator now admits this
exact official-source form, validates manifest/lock agreement and complete
resolved revisions, and retains checksum verification for registry releases.
Controls cover moving/short/foreign source refusals, commit mismatch, and invalid
Git checksum metadata. The prescribed workspace test selection reports three
E0308 compilation failures in crates/errors/src/render/finding_rdf.rs: upstream
intern_iri/intern_literal now return Result<TermId, StorageError>, while the
existing native diagnostic sink expects infallible IDs. No validator test has
executed. raw/purrdf-main-pin-substrate-tests.log retains the errors; handle
44231 is terminal exit 2. The resumed migration propagates those storage errors
through the shared renderer and its callers; current build evidence is indexed
in validation.md. No unwrap, fallback, or gate bypass was introduced here.

make lint passed, terminal exit 0, in raw/purrdf-main-pin-lint.log. make fmt and
git diff --check also passed. The Git pin is resolved but not build-qualified.

All previous tests, producer receipts, generated bundles, browser builds and
hosted CI predate this substrate selection. They do not qualify the Git pin.
No producer, corpus regeneration, browser rebuild, full gate, commit, push or
merge is included in this paused-goal source-selection step.

The prior cycle diagnostic finished with the exact three-producer path:
dl:oneOf-member -> authored seed rule -> authored existential producer, all
positive return edges, and a completed source-admission edge back to membership.
raw/native-list-feedback-cycle-diagnostic-v2.log records its failed 1-test result,
nextest 1187b0bb-2950-4b83-a655-6514d5a445f2. The temporary print was removed.
The original bounded assertions remain unchanged and unresolved.
