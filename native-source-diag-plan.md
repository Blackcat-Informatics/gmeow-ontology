# Preserve native source diagnostics through proof preparation

Scope: issue 1762's sole diagnostic substrate and issue 1770's source-bound
preparation. SourceCatalog::record_document converted an original Diag into
LogicParseError(String), then into StageFailed. Prepared source construction,
frontend compilation and dialect readers also returned the string-only wrapper.
The actual consumer prover accepted that wrapper through its foreign-error path.

Replace the wrapper throughout its real callers, without an alias or parallel
compatibility API. Native Diag values propagate with their original kind, grade,
focus and evidence. Foreign codec and file errors get registered compiler kinds
holding their original live causes, selected format and exact path. Source
composition attaches document context and cannot publish a receipt after failure.
Keep the prover's existing outer reason/decision contract and input identities;
use its existing preserved-diagnostic adapter for the original frontend refusal.
Move round-trip and explicit fixture-producer consumers to this same substrate.
Never invoke that producer from a test.

Prove the changed public boundary with tiny synthetic source mapping, empty input,
missing path, malformed Turtle/XML and all three Common Logic round-trip/control
suites. Reuse unchanged source-admission behavior only after the real FOF tests
pass on the new API. Add a real consumer empty-theory regression, noting that the
consumer requires an authenticated embedded product and cannot run in the local
producer-independent inventory. Compile all locally admitted targets with strict
Clippy, and run helper hygiene plus normal hooks. Full hosted consumer/gate
qualification remains required. No runtime dependency or producer/gate weakening.

This does not replace the full canon semantics, prepared native base, installed
blinded proof acceptance, production-world measurements or final ghprsq integration.
The standing plan.md and the full logic issue/PR goal remain in force.
