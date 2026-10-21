# Independent GMN verdict JSON source review

Reviewed all eleven source/doc/manifest files in validation.md, including the
new external owner and producer-adapter tests and JSON.md, plus Stage plan and
validation. This reviewer did not implement the unit. No formatter, build, test,
host probe, codebook/corpus producer, model operation or posting was performed.

No blocking source finding identified.

- The native verdict owner preserves compact declaration order, the failureClass
  spelling and independent omission of absent strings. Record.required/optional
  supplies boolean/string kinds, missing/null semantics and known duplicate
  refusal without free-form normalization erasing evidence. Unknown fields stay
  ignored. Object-only admission is documented explicitly.
- Both actual production callers consume that same ToJson owner. The pipeline
  adapter uses String-keyed BTreeMap ToJson for sorted outer paths while nested
  verdict objects retain declaration order. It adds no newline or recursive
  sorting. Pinned native API source confirms these map/writer behaviors.
- The verdict type has no other current Rust embeddings found by focused source
  reference search. Unrelated Serde/CBOR records remain unchanged. Both wasm
  package JSON dependency removals cover their actual reader/writer sites.
- Authenticated observation and legend readers keep source selection, bytes,
  failure behavior and assertions, including the source glyph token-cost check.
  No codebook placeholder, weaker dictionary, changed pin, witness refresh or
  test-triggered corpus production was introduced.
- Eight owner controls exercise literal wire bytes, optionals, duplicate/type
  refusals and complete JSON admission. The ninth exercises the actual producer
  adapter using independently authored verdicts without source loading or codec
  execution. Existing tiny-dictionary wrapper coverage remains separate.

Limits: source review does not establish compilation or executed controls. In
particular, excluded validate-wasm embeds a real generated codebook and was not
compiled by this reviewer; host owner tests do not qualify that target, wasm/Node
packaging, source-pinned codebook behavior or installed acceptance. Actual pipeline
observation byte parity and authenticated witness execution remain unrun here.
Root owns focused qualification and final dependency/format checks.
