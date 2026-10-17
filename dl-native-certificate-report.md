# Native DL evidence boundary

Commit 10c6e175f removes CertifiedAnswer entirely. Every DL service returns the
unchanged PurRDF Certified value instead of calling into_parts and copying only
six observations. The complete certificate, work/search-shape/preparation/storage
observations and any selected ServiceProof now cross the GMEOW boundary intact.
There is no compatibility alias, duplicate certificate or new generic engine.
The actual consistency/classification/realization CLI callers read answer and
certificate directly, preserving their existing grading rules.

Explicit recording and native proof-context construction use the same typed
GMEOW refusal adapter. The boundary control checks a retained proof and full
certificate against a separately prepared consumer context and the exact source,
and rejects a different source. Unrecorded selection retains absence and cannot
fabricate an empty checking context. No-model, malformed and execution categories
and original causes remain distinct. Default execution and dependency identity
7734b013 are unchanged. Principles 4, 6, 7 and 18 apply.

All nine selected service tests passed in 0.056s. Strict producer-independent
workspace/all-target Clippy passed with warnings denied in 7m06s. All normal
signed commit hooks passed. Logs: dl-native-certificate-tests.log,
dl-native-certificate-clippy.log and dl-native-certificate-commit.log. Push output
is in dl-native-certificate-push.log.

Four authenticated consumer packages, including the CLI, are excluded by the
local producer-independent targets; their final type/runtime qualification remains
hosted. No local optimized producer, full gate or machine probe ran. This is a
prerequisite for the owner split in 1803 and the full 1770 plan, not completed
class-engine delegation, a prepared native canon, zero admission blockers, an
installed full-canon proof, or measured pipeline speedup.
