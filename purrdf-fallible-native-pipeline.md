# PurRDF Git-main native and pipeline migration checkpoint

The full logic issue/PR goal remains active and incomplete. No external blocker
prevents further work. Main is untouched; HEAD remains
`b797a315e09cf6a118e0ca60975007a65f43a138`. No commit, push, merge, issue closure,
PR creation or external progress message was made during this continuation.
Preserve the preexisting combined integration and sibling worktrees.

## Dependency identity

The user authorized temporarily pinning PurRDF main. Root and fuzz manifests and
lockfiles select the official Git revision
`7734b013d19b68600ad24c4e8aa5a40955a8e89e`. Both lockfiles were rechecked: all 31
PurRDF packages use that exact official Git source, without a registry duplicate.
This is source integration, not registry publication or a qualified release.
The earlier producer, corpus, browser and hosted-CI evidence predates this pin.

## Source changes

Native storage failures now propagate through WorldStore construction,
materialization, native closure emission, goal-directed term construction,
recovery graph construction, synthetic workloads, affect ingestion, bundle
exports, slice scoring/runner inputs, MCP graph caches and pipeline assembly.
The previous math/GMN and projection handoff remains relevant.

Materialization retains shared diagnostics instead of reducing native errors to
semantic chase-error strings. NativeClosureSink writes fail immediately. Empty
world factories freeze a native empty builder directly rather than freezing,
unwrapping and rewrapping an empty union. Graph-result consumers retain the
admitted DatasetHandle until consumption; the slice RDFS closure merges borrowed
handles with its original base while preserving blank identity.

Native exact Integer/Rational interpretation owns literal equality, datatype
membership, decimal validation and interval facets. Cardinality retains its
existing full u128 domain after native lexical validation. No i128 fallback
repairs malformed decimals. The RdfTriple Drop migration uses into_parts and
retains locations instead of copying or partially moving statements.

Recovery seed algebra handles ProjectStar and Apply. Mandatory Apply substitutes
declared input variables with driver variables throughout quoted terms and graph
positions; optional Apply requires only its driver. Existing admission limits
remain unchanged. Refinement promotion now preserves the complete supported
literal value, including datatype, language and direction, in its selected world.

TPTP and OntoUML execution failures carry RecordedDiag observations, separately
from honest unsupported-fragment gaps. Source collisions and invalid native
worlds never become gap rows. Touched canonical exporters use the native fallible
canonicalization door rather than the legacy wrapper that now panics on refusal.
Deep validation projects new native admission failures as hard diagnostic
findings rather than advisory unavailability.

Pipeline provenance registration, graph digests, source unions, witness
publication and graph reconstruction propagate native errors. Superset graph
reconstruction distinguishes a genuinely empty graph from failed reconstruction.
Live and receipt-backed action entity identity return Result<Option<_>> so a
failed digest cannot become an absent entity. GMN candidate construction,
tagging, canonical comparison and certificate text are fallible; native failures
terminate production instead of becoming semantic candidate rejections.

Successful graph projections alone enter MCP OnceLocks. The slice-quality
retraction test helper no longer replaces a failed rebuild with an empty dataset.
Native fixture construction asserts success explicitly without weakening its
semantic expectations. Existing frozen ordinal conversions assert their admitted
table invariant; operational builder writes are not converted to panics.

The synthetic test Make target clears corpus credentials with shell unset and
fails if that clearance fails. It does not invoke the banned env command, probe
the host, select a resource profile, or grant corpus production/access.
No host probes, resource controls, new third-party runtime dependencies, new caps or weaker
semantic fallback were introduced. No pipeline speedup is claimed from these
source changes without measurement.

## Verification

- `make clippy-prebuild`: PASS, terminal exit 0. Final-source inventory is
  `raw/purrdf-main-fallible-clippy-v35.log` (10.60 seconds). It checks the full
  producer-independent workspace, all targets, with warnings denied. Its four
  excluded corpus-dependent consumer packages still require qualification.
- Final-source source lint v17: PASS, terminal exit 0. v16 also passed before
  the flat-view test repair; v15 failed only on the last fixture helper's
  formatting, subsequently corrected. See raw/purrdf-main-fallible-lint-v17.log.
- Formatting v53, final lint's cargo fmt check and final diff whitespace check
  passed. The final root/fuzz exact-pin checks also passed for all 31 packages.
- Native runtime controls v1: 31 run, 29 PASS, 2 FAIL, after a 7m02s test build;
  nextest `76c2ee99-85ec-44ac-b1e4-c1da48cf17be`. New numeric, Apply, refinement
  literal, reserved-collision, diagnostic-roundtrip and native hard-error controls
  passed. See `raw/purrdf-main-native-runtime-controls-v1.log`.
- The flat-view failure was an outdated owned-model expectation: this PurRDF
  revision deliberately omits the implied datatype on plain/language literals in
  RdfTerm, while native TermValue retains it (dataset.rs to_owned_term docs).
  The repair retains explicit source objects and adds native value-identity
  comparison across the projection. v2 exposed the difference between Arc's
  fallible DatasetView method and RdfDataset's inherent term_value; explicit
  as_ref calls select the admitted native table. v3: 1 run, 1 PASS, terminal exit
  0, nextest `f1bbc507-6c4c-45b9-9b34-26660f7296ac`, 22.45s build, 0.051s run.
  See `raw/purrdf-main-native-flat-view-controls-v3.log`.

These are focused local checks, not a full gate, corpus producer qualification,
installed-binary proof acceptance, browser/wasm parity, hosted CI or merge.

## Required next work

The list-feedback regression remains a fresh failure on this exact Git pin:
`list_membership_feedback_cannot_hide_an_existential_cycle` refuses
NonStratifiable at its unbounded NonTerminatingExistential assertion. Its bounded
32-step branch therefore did not execute. Preserve its required Exhausted result,
multiple witnesses and nonadmitted certificates; do not replace the expectation
with NonStratifiable. The earlier three-effect cycle diagnosis remains relevant.
Source admission currently waits on whole-family completed grammar reads; a
sound local completeness proof must account for every overlapping writer,
witness identity, source collision and world. No such proof was implemented here.
The current source observation is precise: dl.rs caches pending reads by family,
then groups source occurrences by (family, owner); joint/families.rs creates
completion-only admission arms from those whole-family grammar reads. Atomic
effect groups add positive scheduling edges, so merely grouping the existential
producer with admission cannot remove a genuine completed-read cycle safely.

The broader canonical source/context admission, class/nominal reasoning,
embedded prepared-prover and blinded consumer acceptance, and native CLI error
envelope requirements remain unfinished. No issue can be closed from this port.
Correspondence execution errors can still be mislabeled as semantic law
refutations; retain this concrete diagnostic-fidelity seam as actionable work.
The previous CLI GMN serializer and reasoner-service error-string seams remain.

The live PR1802 read retained in `raw/pr1802-current-pin-migration.json` confirms
OPEN, published head b797a315, UNSTABLE; generation(b), fixture-prefix and quality
failed on that older published source. No current WIP qualification was pushed.
Before integration, refresh complete review/issue evidence and final base state,
finish the actual gate and acceptance work, then use only the required ghprsq
merge path. Do not collapse distinct acceptance surfaces into compiler success.
