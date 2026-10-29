## Summary

Fix the producer source-admission failure introduced in #2065. Ordinary Rust compilation accepted relative JSON macro paths, but the producer recipe correctly refused them because a local module could shadow that namespace. The existing resolver grants the pinned JSON owner through an absolute external-prelude path.

Use that supported path for all sixteen affected invocations across ten files: fourteen production invocations and two test invocations. Macro bodies, fields, fixture bytes, serialization recipes and resolver policy are unchanged. No capability exception or compatibility path is added. Principles 4 and 7.

## Validation

Static `make producer-recipe` passed without building or running the optimized producer. All-target warnings-denied checks passed for the eight affected owner and consumer packages. Independent review verified the complete prefix-only diff and the existing provenance rule. Whitespace, prose and full-postimage publication scans passed.

All three existing key and complete-envelope identity controls passed: twenty-two cases excluded, 0.014 second summary. Normal signed commit hooks passed. Full CI is explicitly waived; no local canonical corpus or fixture production, synchronization or full gate ran.

Part of #1776. This corrects producer source admission; the parent and authenticated corpus, installed/browser/release and performance acceptance remain open.
