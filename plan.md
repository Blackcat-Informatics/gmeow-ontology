# Producer compiler-input identity repair

The merged main CI producer compiled successfully, then failed the compiler-read
seal for docs-print/src/doc_render.rs. That module is live and already wired in
lib.rs/render.rs. Keep it. Source selection normalizes Cargo package IDs through
metadata; artifact collection and external-root selection used raw IDs. A
version-only fragment therefore loses a selected owner. Principles 4, 7 and 18
require repairing identity binding, not weakening authentication.

## Task 1: Normalize all compiler evidence owners

Use the existing exact Cargo metadata map for compiler-artifact and
build-script-executed messages. Preserve workspace substitution, controller and
selected-package filters. Reject unknown IDs. Apply the same explicit identity
to external-source root selection; reject contradictory metadata. Preserve source
inventory, actual dep-info verification, post-build recipe equality, linked
identity and receipt checks. No dependency or build-profile changes.

## Task 2: Validate the real binding boundary

Synthetic tests call the production message collector, then match an actual
synthetic dep-info record and invoke the compiler-read seal. Cover implicit and
explicit workspace IDs, generated owners, unknown IDs and unselected artifacts.
Cover external Git version-only/explicit IDs, selected versus excluded roots and
contradictory metadata. Existing metadata-name checks remain required. Reuse the
unchanged patch's focused checks from the 1770 checkout with their exact scope;
the class-law changes there do not participate in these pure producer helpers.
Normal hooks run fully. Strict prebuild Clippy is running there over both changes.
No local production fingerprint execution is authorized under the host-probing
ban; no local full gate or hosted success is claimed.

## Task 3: Stop failed docs prerequisites immediately

Pages exposed the same authentication error, then continued from the failed
generated prerequisite into a second producer build. Issue #1826 owns that
failure-propagation defect. Chain docs prerequisites with explicit success
dependency. A synthetic Makefile containing the real recipe uses only inert
producer stubs: verify first-prerequisite failure, second-prerequisite failure
and successful generated/build/docs ordering. The old recipe must fail this
regression. The existing lint/check-lint targets require the control; it reads
only authored Makefile/script sources and has no sync dependency. No corpus,
host probe, producer authentication or generated artifact is accessed by tests.

## Task 4: Publish, review and integrate the isolated repair

Commit and push only the two producer files on this issue branch. Create the
issue PR, obtain available CodeRabbit feedback, and inspect exact-source hosted
producer/downstream CI. Preserve the full-LTO recipe and authentication seal.
Use ghprsq for final merge and retain evidence separately from source. The issue
remains open until its real qualification is established or the user explicitly
directs integration with pending CI. This branch excludes the class-law change.

## Coverage and process state

Brief covers the complete issue and one comment, fourteen title-search results,
and two hundred commit trailers. No prior issue branch/PR was found. Two
repeated-unshared-reasoning-analysis and one consumer-artifact-path-coupling
instances were counted; unsearched recurrence is unknown. No independent agent
API exists here, so no independent plan/implementation verdict is claimed.
CodeRabbit and hosted CI are still pending. The worktree is temporary and will
be cleaned after integration; the existing reasoning checkout is preserved.
