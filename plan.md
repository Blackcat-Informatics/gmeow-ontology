# Benchmark JSON owner

Replace the binary-only bench-engines JSON dependency with the selected native
PurRDF lex JSON owner. Preserve sorted free-form objects, integer/bool/string
artifact and descriptor bytes, vector order and writer newline policies. Keep
all JSON construction outside measured engine calls; do not alter profiles,
allocation bands, jitter floor, peak direction or engine execution.

Share strict case admission between replay and prior-baseline comparison:
object cases/native, nonempty identity, actual fragment-specific required counts,
unsigned common counters and cost-vector arrays, duplicate-case refusal. Compare
deterministic identity as sorted compact tokens, including backward answers,
while permitting peak improvements and advisory allocation changes. Preserve
ledger identities, refusing missing/empty finding and anchor IDs in each report.

Move existing pure unit tests to an external module and add independent literal
wire, parse/refusal, identity and ledger controls. No corpus, harness or producer
execution is authorized in this source lane. Public pipeline CostArtifact reader
and Cargo.lock remain owned elsewhere.

Current-source audit: this package has one binary, no library/examples/extra
targets; JSON values consist of local primitive/container values. Make benchmark
and soak targets call the binary; pipeline bench stage reads its committed cost
artifact. Memory recall did not return within ten seconds; current source is the
evidence. Graph navigation was unavailable during the prior bounded intake.
