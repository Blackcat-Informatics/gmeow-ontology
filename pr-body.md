Repair both post-merge findings on #1891, related to #1816.

State the original claim vantage explicitly in the signature example. Give DisclosureDecision local typed reader/result restrictions with qualified maxima and class-scoped closed-world required paths. Ordinary observations keep multivalued results, and different reader decisions remain separate. Required presence uses allValuesFrom plus ClosureEntry; it does not rely on an open-world existential. Principles 4, 7, 9, 12 and 17.

Add a read-only consumer of the existing authenticated observations validation fixture. It checks the actual produced shape and nine positive/negative cases for missing, duplicate and mistyped fields, unrelated observations and separate decisions. Tests never compile source modules or derive corpus shapes.

Strict producer-independent Clippy passed, including compilation of the new consumer. All four focused synthetic lowering tests passed, covering required paths, qualified counts and absence of global closure leakage. Source audit and diff hygiene passed. The actual corpus-backed nine-case consumer remains unrun locally without an authenticated selector; no local optimized producer, corpus construction, sync or full gate ran. CI will not delay merging under the explicit user instruction.
