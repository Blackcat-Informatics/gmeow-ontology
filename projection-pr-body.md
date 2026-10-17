The recorded-world benchmark previously selected the authored default before
object-level filtering: 180062 assertions instead of the assembled world's
179177. It now completes the production object-world assembly first, then uses
the native graph projection for every selected world. The broader default
shortcut is removed, and the full EDB is released before native reasoning.

Pin PurRDF to `af8936e6f22c70dcbc3f3dc1224be887aba51cc5`, the paired native
implementation in Blackcat-Informatics/purrdf#567. Migrate all named graph
callers to the unified API without retaining the old method or an alias. Recorded
input metadata is checked before hydration; the read-only native cache remains
the sole product admission authority. Native certificates and question-bound
proof replay remain intact.

Advances #1803. This PR supplies the recorded-input consumer and graph API
integration; the complete class-engine delegation and deletion remain open in
that issue. The upstream PR has landed through ghprsq as ff302c091.

Validation: the corrected consumer type-check and normal signed commit hooks
passed. Eight named recorded worlds completed with source-checked proofs:
four Decided and four DecidedWithinBoundaries. The largest, relational-core,
has 377397 assertions and took 921/654/515ms for preparation/decision/replay;
these concurrent observations are not a controlled speedup comparison.
The earlier authenticated world listing, imports and
logic proof replays and six refusal controls are recorded against the earlier
consumer commit, not claimed as executions of this corrected default path.
Strict producer-independent workspace Clippy passed on the final pin in 3m 32s,
including the source corpus-purity seal. The optimized corrected-default
operation selected exactly 179177 assertions, then native preparation refused
an invalid object property (private term 9336); it produced no verdict. The final
listing and imports/logic proof replays completed on the corrected consumer.
Both named answers remain DecidedWithinBoundaries; no full canonical admission
is claimed. Six current-executable refusal controls passed. CodeRabbit's listing
finding is fixed in 646bf451f and its thread resolved. Initial hosted generation
caught a fuzz dependency-pin mismatch; both fuzz inputs are corrected and all
31 native package identities agree with production. Full repair CI is pending.
No test or local producer rebuilt a corpus. The benchmark is an explicit
consumer excluded from Cargo test execution. Its recorded historical inputs
do not establish complete admission of today's canonical sources.
