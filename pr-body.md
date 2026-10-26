The fragment registry observation channel now uses PurRDF's native JSON codecs from its existing registry owner through the producer and authenticated corpus reader. All eight inventories and characterizations retain their named fields, order, sorted collections and exact strings; complete source refusals use the shared recorded-diagnostic codec. The unused registry Serde derives are removed.

Named-record admission rejects duplicate known fields and populated positional arrays. Unknown fields and checked last-wins string maps retain their existing behavior. The observer, channel identity, authenticated selection and separate CLI projection are preserved.

Validation: five authored component cases passed after a 3 minute 58 second build, with 0.027 seconds of test execution. They cover exact success bytes, complete recorded refusals, semantic source rejection and malformed JSON admission. Independent review passed across all five paths. Strict all-target pipeline/dev CLI diagnostics passed in 49.38 seconds. Normal commit hooks passed.

The recorded campaign exception excludes local optimized production, corpus/fixture construction, sync and the full local gate; merges proceed sequentially with CI pending. Full authenticated corpus execution, excluded embedded consumers, installed behavior and #1776 acceptance remain unverified. No corpus or canonical authority was produced or refreshed.

Closes #1971. Principles 2, 3, 4, 7, 17 and 18 apply.
