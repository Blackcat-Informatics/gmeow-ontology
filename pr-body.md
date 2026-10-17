Canonical node-kind integrity laws could not lower into executable violation rules, blocking generation and fixture production. This change evaluates the existing node-kind predicates on native RDF values and preserves the original law, focus, and failure marker.

It also executes equality and distinctness through the native SPARQL implementation, retains typed operational failures, and prevents quantified variables from being captured during violation lowering. Canonical ontology and structural-query sources are unchanged.

All 54 focused logic tests, both read-only witness tests, all 13 provenance controls, and strict Clippy for both logic crates and xtask passed. The explicit tiny witness refresh preserved the semantic records and verified the proof and refutation. All normal commit hooks passed. Hosted lint and helpers also passed.

The provenance gate distinguishes local audit material from authored files while still checking tracked audit records and nested lookalikes. ShellCheck now follows declared helper sources as well.

Hosted generation and fixture-prefix acceptance remain required. The issue stays open until those production checks pass.

Refs #1843. Principles 4, 7, and 17.
