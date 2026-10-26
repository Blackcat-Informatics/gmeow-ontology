The complete internal GTS snapshot receipt now uses PurRDF native JSON at both its terminal writer and typed reuse reader. All five fields, compact declaration order and exact internal sink bytes are preserved. The obsolete receipt Serde derives are removed.

Native admission requires every field, tolerates unknown members and refuses duplicate known fields, positional arrays and invalid u32 tokens. Existing schema, algorithm, current-input and canonical digest checks still fail actionably; a present terminal never converts malformed evidence into recomputation. Hashing, canonicalization, receipt version, GTS emission, compression and source/action authentication retain their existing behavior.

Independent source review passed. The extended existing authored receipt control passed after a 4 minute 15 second build and 0.021 seconds execution. Strict pipeline/developer CLI all-target diagnostics passed in 1 minute 2 seconds; helper ownership, producer-recipe admission, formatting, diff hygiene and normal signed commit hooks passed. Helper, recipe and runtime checks used Cargo offline.

The standing exception excludes local optimized production, corpus construction, sync and the full gate. Actual terminal emission, authenticated corpus, installed/browser and cold parity acceptance remain unverified. Remaining contracts: #1776, #1962 and #1757. Principles 4, 7, 12 and 18 apply.

Closes #1981.
