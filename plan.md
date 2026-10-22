# Native glyph legend transport

Preserve the existing String API and compact row bytes while moving assembly into
one public native Value builder in lang-bridge. It retains graph-derived token
order, the exact pinned-cost owner and named missing-pin refusal, without enabling
glyph-cost or invoking the tokenizer. The String API and real MCP response use
that same builder; MCP emits native legend/ok fields directly and retires its
own-string parse and unreachable parse-error branch. Outer RPC arguments and
envelopes, dictionary admission, selected source artifacts and wasm forwarding stay
unchanged.

Add one small real MCP response helper accepting a registry, plus external pure
tests of admitted tiny RDF registries: exact row/field/envelope bytes, ordering,
empty output and original missing-pin identity. Reuse a single independently
authored tiny RDF fixture across both crates, never the canonical codebook or
selected corpus. Port the existing authenticated contract's payload/shim parsing
to native Value while retaining its RPC Serde boundary, assertions and source auth.
Adapt its structural assertion to the actual shared builder path.

Source-only work; no builds, formatting, checks, corpus/BPE/model/producer runs or
host changes. Root owns independent review, serial helper/strict/runtime/hooks,
publication and cleanup. Existing seven pin controls and the authored graph-derived
registry ordering control are safe. Actual source/bundle/shim/wasm agreement remains
unrun without its authenticated producer artifacts. Parent dependency migration
and installed acceptance remain open.
