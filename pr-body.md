Class-source-admission observations now use native JSON through their per-input cache, aggregate publisher, authenticated aggregate reader and frozen expected-record reader. The full five-record closure and both complete boundary enums preserve digest widths, required-nullable graph fields, quoted terms, typed counts, maps, sets and causal errors.

Both cache paths share one lazy parser. Input/profile/operation identities, explicit validation, expected-record comparison, corpus assertions and terminal CBOR evidence remain unchanged. Shared Serde remains for mixed execution JSON, broad conformance verdicts and CBOR consumers. The only dependency change adds the existing workspace-pinned native lex package directly to conformance.

All six authored behavioral controls passed, including actual native cache reuse, malformed-source refusal, complete wire and causal evidence, required graph fields and the retained CBOR receipt. Independent complete source/design review, formatting, diff hygiene, strict five-package caller compilation, helpers and producer-recipe admission passed. Normal signed hooks run before publication.

The standing user exception prohibits local optimized producer, corpus/fixture production, synchronization and full-gate execution and authorizes sequential merges with CI waived. Actual corpus and release acceptance remain unverified. Remaining contracts: #1776, #1777, #1336 and #1962. Principles 4 and 7.

Closes #1993.
