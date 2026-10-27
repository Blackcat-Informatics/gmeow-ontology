The GMN codebook producer and bounded reader now use PurRDF's native CBOR codec for the complete prepared transport. The writer streams borrowed tables; the reader consumes native values and enforces all four named record schemas. The obsolete Serde records, serializers and lang-bridge Ciborium dependency are removed.

The complete 1,080-byte authored packet was captured with the original encoder before replacement. Full-packet equality protects declaration order, optional nulls, wider unsigned values, source identity and checksum. Domain admission now explicitly rejects duplicate/unknown/missing fields, wrong row widths, tag wrappers, undefined options and byte strings used as table sequences. Existing Merkle, ordering and glyph checks remain in place. Principles 4 and 7.

Validation: all four authored transport controls PASS, including complete original-packet equality. Strict compilation of lang-bridge, pipeline, developer CLI and MCP callers, helper hygiene, static producer-recipe, formatting and independent review PASS. CI is waived under the standing instruction. Embedded/browser execution, authenticated corpus and installed/release acceptance remain unverified; no local optimized producer, corpus construction, synchronization or full gate was run.

Closes #2007.

Remaining contracts: #1777, #1776, #1336 and #1962.
