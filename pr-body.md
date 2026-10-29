Complete scoped witness receipts now use native PurRDF JSON for writing and intrinsic reading, including selected-world records and retained RDF term adapters. The actual RDF publisher preserves the v2 wire format, member order and full proof data. Identity algorithms and current publisher bindings stay unchanged; malformed records fail with typed native causes. Principles 4 and 7.

Two complete receipts captured from the original publisher are replayed byte-for-byte with their original IDs. Current execution is checked separately. The existing publisher fixture now supplies an explicit native source rule, and its version and derivation-literal expectations match the actual publisher. Existing controls cover semantic tampering and the selected native record boundaries.

Validation passed: original publisher capture; all four selected behavioral controls (4,495 skipped); warnings-denied all-target diagnostics for errors, logic, pipeline, validation and developer CLI; formatting/prose, helpers, static producer recipe and independent source/integration reviews. Normal signed commit hooks passed. Broader CI is explicitly waived; producer, authenticated corpus, installed/browser/release and performance acceptance remain unverified. Parent #1776 stays open.

Closes #2056.
