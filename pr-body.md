Rendered site/book caches now use native JSON at both reads and the producer write (Principles 4 and 7). The existing shared digest/verification owner accepts canonical payload bytes: model callers retain their existing Serde bytes, while site callers use native compact sorted string maps. SHA-1 framing, the site label for book artifacts, selector/receipt checks and the tamper guard are preserved.

The native codec keeps digest/files order, exact string escaping and no terminal newline; required kinds and known duplicates fail explicitly. The docs crate drops production serde_json while retaining its dev edge and separate model/receipt/template Serde owners. Direct native record::to_vec calls replace a duplicate local byte wrapper caught by helper hygiene.

Validation: fifteen authored/in-memory controls passed, including an independently computed digest golden and four existing regressions. Strict prebuild Clippy, helpers, source admission and formatting passed; independent review is clear. Original helper failure and the unchanged-expectation repair/rerun are retained.

The user retained no local corpus/fixture production, sync or full gate and sequential merges without waiting for CI. Actual rendered-cache/authenticated/installed/full consumer acceptance remains unrun; the hosted pipeline is separately blocked by the owned schema.org registry scope defect. Normal commit/push hooks remain required. Full migration stays open.

Refs #1776
