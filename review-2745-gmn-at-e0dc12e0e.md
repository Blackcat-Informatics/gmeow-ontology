# Delivery review: GMN-1 plain identifier literal (`2745-gmn-literal-identifier`)

- Head reviewed: `e0dc12e0e0b9bc272191851e62b86de35085a245` (`git rev-parse HEAD` in the worktree)
- Base: `331c554f8`; diff `331c554f8..e0dc12e0e` is one commit touching
  `crates/lang-bridge/src/gmn1_codec.rs` (+4/-1) and `crates/lang-bridge/src/gmn1_codec.tests.rs` (+85)
- Timestamp (`date`): 2026-11-14 19:02:46 CST
- Mode: read-only. No builds, no cargo, no make, no tests run by the reviewer.

## Scope

The requirement as stated by the caller, checked against the code. There is no `issue.md` or
`plan.md` with rulings in this brief, so the contract reviewed is the caller's REQUIREMENT text.

## Coverage

| File | Lines read | Commit |
|---|---|---|
| crates/lang-bridge/src/gmn1_codec.rs | 1-320, 1910-2000, 2180-2700, 2740-3335, 3337-4398 | e0dc12e0e |
| crates/lang-bridge/src/gmn1_codec.tests.rs | 680-770 (the whole added hunk) | e0dc12e0e |
| crates/lang-bridge/src/gmn1_witness.rs | 60-140 | e0dc12e0e |
| crates/lang-bridge/src/gmn_conformance.rs | 1-100 | e0dc12e0e |
| crates/lang-bridge/src/emit.rs | 22-25 (`digest16`) | e0dc12e0e |
| crates/lang-bridge/src/gmn_migrate.rs | 520-545 | e0dc12e0e |
| crates/lang-bridge/src/gmn_consume.rs | 500-570 (grep level) | e0dc12e0e |
| crates/pipeline/src/stages/conformance/gmn_grounding.rs | 30-100, 150-275 | e0dc12e0e |
| crates/pipeline/src/stages/conformance/corpus_tests/gmn_grounding.rs | whole | e0dc12e0e |
| crates/pipeline/src/stages/conformance/gmn_vectors.rs | whole | e0dc12e0e |
| crates/pipeline/src/stages/conformance/corpus_tests/gmn_vectors.rs | whole | e0dc12e0e |
| crates/pipeline/src/stages/conformance/execution.rs | 340-470 | e0dc12e0e |
| crates/pipeline/src/stages/conformance/gmn_dictionary/costs.rs | 90-125 | e0dc12e0e |
| crates/pipeline/src/stages/gmn1_gate.rs | 270-300, grep for coverage | e0dc12e0e |
| crates/pipeline/src/run.rs | grep only (58-75, 760-790) | e0dc12e0e |
| crates/gmn-wasm/tests/witness_gmn.rs, crates/gmn-wasm/js/tests/witness.test.mjs | whole | e0dc12e0e |
| crates/docs/src/vendored_asset.rs | 760-860 | e0dc12e0e |
| crates/docs/assets/gmn/{SUBSTRATE.txt,DIGESTS.blake3,PROVENANCE.md,WITNESS.gmn1.txt} | whole | e0dc12e0e |
| crates/mcp/src/lib.rs | 4060-4100, 4395-4440 | e0dc12e0e |
| crates/gmeow-cli/src/gmn.rs | 455-560 | e0dc12e0e |
| slices/grounding/lang/tests/gmn1-vectors/*.gmn and literal lines of *.in.ttl | whole / grep | e0dc12e0e |
| .config/nextest.toml | 55-95 | e0dc12e0e |
| PurRDF purrdf-core-4.0.0 `model.rs` 140-200, `ir/term.rs` 1840-1870, 1960-1970, `ir/builder.rs` 1605-1640, 3630-3650; purrdf-rdf-4.0.0 `native_quads.rs` 141-170; purrdf-core-2.0.2 `ir/dataset.rs` 596-626 | as listed | registry |
| .stage/issue-2745-gmn/*.log (the coder's evidence) | 01, 02, 03 (grep), 04, 07, 09 | n/a |

Not read: the rest of `gmn1_codec.tests.rs`, `crates/lang-bridge/tests/gmn1_roundtrip.rs`, the
rest of `gmn1_gate.rs` and `run.rs`, `gmn_training_corpus.rs`, `registry.rs`, the
`lang_projection` fixtures beyond a grep, the console/mcp asset tests beyond a grep, and the
compiled wasm images (their behaviour is an inference from the source they were built from).

## Evidence

- Coder logs: `03-nextest-lang-bridge.log` shows `cargo nextest run -p gmeow-lang-bridge`, 249
  passed, `EXIT=0`, recorded against a dirty tree at base `331c554f8` with the change applied, not
  at the committed head. `02-…` is an earlier run of a different test text, with the fix already
  in place. It is not a base-classifier run. `04-nextest-gmn-wasm.log`: compile failure
  (`generated/projections/lang/gmn-codebook.cbor` missing), `EXIT=101`, so the native witness
  test never ran. `01-…`: `make verify-test-fixtures` failed with `EXIT=2` because there was no
  producer or fixtures, so none of the corpus tests (grounding coverage, frozen vectors) ran.
- So no execution evidence exists for the grounding coverage audit observing LiteralIdentifier,
  for the frozen-vector byte comparison, or for the wasm witness. Every such claim below comes
  from reading the code.

## Findings

### F1 — REQUIRED: new byte-idempotence break for dataset input (record order depends on the datatype spelling)

`gmn1_codec.rs:279-295` (`term_sort_string`) and `:256-268` (`quad_sort_key`) order literals by
`datatype.as_deref().unwrap_or("")`. `gmn1_read` decodes an inline identifier as
`RdfLiteral::typed(token, XSD_STRING)` (`:2575-2577`) and re-sorts with that key (`:3940`). A
dataset model carries the same literal with `datatype: None`. Before this commit no dataset
literal was inlined: it went by reference, and its payload kept `None`, so decode and re-encode
used the same sort key. After the commit the inline literal comes back as `Some(xsd:string)`, so
it sorts differently from the original whenever two objects under the same subject and predicate
share a lexical form. Then `check_reencoding` (`gmn1_witness.rs:120-135`) fails.

Failing input (dataset-derived, e.g. `gmeow gmn encode`, the grounding gate, `observe_positive`):

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:gate1 gmeow:statusLabel "open", "open"@en .
```

- Original sort strings: `open\x1f\x1f` < `open\x1f\x1fen`, so the plain string's record comes first.
- Decoded: `open\x1fhttp://www.w3.org/2001/XMLSchema#string\x1f` > `open\x1f\x1fen`, so the
  language-tagged record comes first.
- Two primaries means no fold, so there are two flat `@c` records. The re-encoded text has them
  swapped, `encode(decode(doc)) != doc`, and `round_trip_with_claims_check` /
  `per_claim_round_trip_check` / `idempotence_check` return `NonDecodableGrammar`
  ("idempotence break"). At base the same input passes.
- The same thing happens with `"true"` next to `"true"^^xsd:boolean`, or any datatype IRI that
  sorts below `…XMLSchema#string`.

These checks are production entry points: the grounding gate (`gmn_grounding.rs:225`), the
frozen-vector observations (`gmn_conformance.rs:90-91`) and `gmeow gmn verify` (`gmn.rs:541`).
A rough scan of the grounding slices found no subject+predicate pair with this shape, so the
current gate should not go red. The scan matched identifier-shaped plain strings against
same-file tagged ones; every hit used a different predicate. Even so, the requirement "encoding
must be deterministic" and the codec's invariant "every value has exactly one spelling" are now
broken for valid input the base handled.

Fix direction: canonicalize the representation inside the codec rather than at one call site.
In `term_sort_string` and `intern_literal_ref`, treat `Some(XSD_STRING)` with no language as
`""`, or use `RdfLiteral::datatype_iri()` in both. Alternatively, have `decode_value` return
`RdfLiteral::simple`. Then add a test with `"x"` and `"x"@en` under one subject and predicate
that runs `per_claim_round_trip_check`.

### F2 — REQUIRED: committed frozen vector `by-ref-annotation.gmn` pins a key that neither base nor head can produce. Not accounted for.

`slices/grounding/lang/tests/gmn1-vectors/by-ref-annotation.gmn:2` pins `v: r_45c4ffaf2bdc2a4f`.
With `digest16 = sha256("gmn1-literal\x1f" + lex + "\x1f" + dt + "\x1f" + lang)[..8]`
(`emit.rs:22-25`, `gmn1_codec.rs:2442-2454`), I recomputed:

- `dt = http://www.w3.org/2001/XMLSchema#string` gives `45c4ffaf2bdc2a4f`, the pinned value.
- `dt = ""` gives `a86d449bbee7531b`.

The input is `"…"^^xsd:string`. PurRDF 4 interns it with the same id as a plain string and hands
it back with `datatype: None` (`purrdf-core-4.0.0/src/ir/term.rs:1854-1863`; builder test
`datatype_expansion_equality`). The head writer therefore emits `r_a86d449bbee7531b`. The
required-lane test `corpus_tests::gmn_vectors::complete_frozen_vectors_preserve_bytes_witnesses_and_negative_classes`
compares bytes exactly (`gmn_conformance.rs` `failures`), and so does `gmeow gmn verify`. Both
fail on this vector at head.

This is the same defect as the commit: one RDF literal, two representations, two spellings. The
commit fixed it for the inline path and left the by-reference path as it was. The commit message
says "the by-reference key derivation is unchanged" but never mentions this pinned artifact. The
vectors date from 2026-07-17, under PurRDF 2.x, whose `to_owned_term` always returned
`Some(datatype)` (`purrdf-core-2.0.2/src/ir/dataset.rs:596-618`). This explains every stale pin.

At head four of the five literal-bearing vectors become consistent again:
`err-repair`, `patch-repair`, `retract-repair`, `patch-on-triple-term` (`id: c42/c17/cTT`
inline). `by-ref-annotation` stays red. Whichever way F1 is fixed (canonical `""` or expanded
IRI), this vector has to be re-blessed or the key derivation chosen to match it. Either way it
must be explicit.

### F3 — REQUIRED: committed wasm images embed the old classifier and disagree with the head source and with the witness

`crates/docs/assets/gmn/gmeow_gmn_wasm_bg.wasm` (and `DIGESTS.blake3`, `SUBSTRATE.txt` =
"purrdf 4.0.0") were refreshed in `ad2b5c117` (2026-11-09) from codec source whose classifier was
`== Some(XSD_STRING)`. That line is unchanged since `ec2a0380a`. With PurRDF 4 the image encodes
the witness input `gmeow:statusLabel "open"` as `v: r_<hash>`, not the committed
`WITNESS.gmn1.txt` line `v: open`. This is an inference from source, since the binary was not
executed. The Node lane `committed GMN bindings and image reproduce the native attestation`
(`crates/gmn-wasm/js/tests/witness.test.mjs:52-68`) therefore cannot pass at head. The head native
source now agrees with the witness and the shipped image does not, so the docs-site transcoder
emits different bytes from `gmeow gmn encode`. The mcp and mcp-core images
(`crates/docs/assets/mcp*/pkg/*_bg.wasm`, same refresh) embed `encode_gmn1` (`mcp/src/lib.rs:4077`)
with the old classifier too. I found no committed pin of affected bytes in their fixtures, but
they diverge from native output just the same. The fix is the coordinated refresh
(`make maint-prepare-wasm-assets`, per `PROVENANCE.md`) after F1/F2 settle the bytes, plus
evidence from the Node lanes.

### F4 — OPTIONAL: the test doc comment overstates why reserved prefixes are excluded

`gmn1_codec.tests.rs` (doc of `plain_string_outside_the_inline_identifier_form_stays_by_reference`):
"a reserved prefix would be read back as a blank node or reference key". In a value slot
`decode_value` (`:2555-2591`) has no blank-node or `x_` branch. `_babc` and `x_abc` would decode
as plain strings, and only `r_` collides. The exclusion is conservative and harmless, but the
sentence is not accurate. Separately, the classifier comment says "a hand-built or decoded literal
spells it out". That is true for inline decodes only: a by-reference decode keeps whatever the
payload carried.

### F5 — OPTIONAL: requirement mentions directional literals; no directional case

A directional literal always has a language tag (RDF 1.2), so `language.is_none()` already sends
it by reference, and the language-tagged case covers the branch. A one-line directional case would
pin it explicitly. Related but outside this commit: `intern_literal_ref` does not hash `direction`
and `payload_to_term` drops it (`:2442-2460`, `:2593-2604`), so `"x"@en--ltr` and `"x"@en` share
one key first-wins and lose direction on decode. The round-trip gate would catch that on gated
paths. It needs its own owner.

## Answers

1. **Correctness and the two representations.**
   - The inline path is now correct for both spellings: same token, same category, no ref entry.
   - Remaining split, **reference keys**: `intern_literal_ref` still hashes
     `datatype.unwrap_or("")`, so `None` and `Some(xsd:string)` of one non-identifier string give
     different `r_` keys. Production writers all start from `Gmn0Model::from_dataset`, so they
     always see `None`: CLI encode/verify, wasm, mcp `encode_gmn1`, the grounding gate, vectors,
     `gmn1_gate`, the training corpus, metrics, the cost matrix and `gmn_consume`. Decoded models
     (`check_reencoding`, mcp `gmn_expand`) keep the payload's original spelling, so by-reference
     keys stay self-consistent. The split shows up only with hand-built models (tests,
     `gmn_migrate::tag_schema_version`, which is rendered to N-Triples and never GMN-encoded) and
     with artifacts pinned under PurRDF 2 (F2). This part is pre-existing and unchanged.
   - Remaining split, **record order**: worse at head (F1). Inline decodes now come back as
     `Some(xsd:string)` and sort differently from the dataset's `None`, which breaks byte
     idempotence for same-lexical-form siblings.
   - Round-trip *canonical* comparison is unaffected. Both spellings intern to one PurRDF id, so
     RDFC N-Quads agree. The added test's `gmn0_canonically_equal` assertion is sound.
   - Hand-built edge: a model holding both `"x"` and `"x"^^xsd:string` is not deduplicated by
     `quad_sort_key` and now emits two identical `v: x` records. Tests only.
2. **Tests.**
   - `identifier_shaped_plain_string_is_inline_for_implied_and_explicit_datatype` fails on the base
     classifier: the implied literal (`None`) went to `LiteralByReference`, so the first
     `assert_eq!` fails. It goes through `RdfDatasetBuilder`, `from_dataset`, `classify_model`,
     `gmn1_write` and `gmn1_read`, which is the production path, and it does not mirror the
     implementation.
   - `plain_string_outside_the_inline_identifier_form_stays_by_reference` **passes on base**: every
     case went by reference there. It guards against broadening the new rule, which is a
     legitimate boundary, but it is not a regression test for the defect.
   - Untested: a directional literal (F5), and the multi-literal idempotence property (F1). The
     test says "both spellings must emit the same bytes" but proves it only for a one-statement
     model, which is exactly where F1 cannot show.
3. **Decode soundness.**
   - `v`, `q` and `id` slots go only through `decode_value`. The order is integer, then decimal,
     then `r_` lookup, then `is_identifier` → `xsd:string`. The decoder never consults the
     dictionary, glyphs, the prefix table, booleans or blank labels in a value slot.
   - `is_identifier` (`:2328-2335`) requires an ASCII letter or `_` first. Integer/decimal tokens
     start with a digit or `-`, so they are disjoint. `is_number_shaped` returns false for
     identifiers, so the malformed-number pass 2 cannot fire.
   - `true`, `false`, `a`, dictionary aliases, prefix names, `NaN`, `INF` and `e5` therefore decode
     back as the same `xsd:string`.
   - The only real collision is `r_`, which the writer excludes. `_b` and `x_` are excluded too,
     although not strictly needed in a value slot.
   - Identifiers contain no `,`, `:`, `}`, `{` or whitespace, so the record and tabular lexers
     cannot split them.
   - Embedded triple-term literals still never inline (`encode_embedded_literal`, unchanged).
   - Verdict: sound.
4. **Witness and pinned artifacts.**
   - `WITNESS.gmn1.txt` (both copies, identical, from July) is checked by
     `crates/gmn-wasm/tests/witness_gmn.rs`. Its input is Turtle text parsed by PurRDF, so the
     representation is `None`. On base the native test cannot have passed: it would emit `r_…`,
     and the text-only reader would hit a dangling key. At head it matches. The coder's run did not
     execute it (`04` log, `EXIT=101`). The Node lane also checks the committed image.
   - Inventory, against head:
     - `crates/gmn-wasm/tests/WITNESS.gmn1.txt`: consistent.
     - `crates/docs/assets/gmn/WITNESS.gmn1.txt`: consistent.
     - `crates/docs/assets/gmn/gmeow_gmn_wasm_bg.wasm` + `DIGESTS.blake3` + `SUBSTRATE.txt`: stale (F3).
     - `crates/docs/assets/mcp{,-core}/pkg/*_bg.wasm`: stale codec, no affected pin found.
     - Golden vectors `err-repair`, `patch-repair`, `retract-repair`, `patch-on-triple-term`:
       consistent at head (inconsistent at base).
     - `by-ref-annotation`: inconsistent at both (F2).
     - `by-ref-confidence` (decimal, explicit): consistent.
     - IRI-only vectors (`all-slots`, `claim-basic`, `defeater`, `evidence-span`, `header-only`,
       `lang-ast`, `logic-record`, `math-record`, `modal-force`, `proof`, `reifier-folded`,
       `standpoint`, `triple-term-object`): unaffected.
     - Codebook digest pin `blake3:26a6…`: unaffected (codebook content).
     - Token-cost probes (`gmn_dictionary/costs.rs`, IRI objects only): unaffected.
     - `gmeow-cli/tests/gmn_conformance.rs` FROZEN (IRIs only): unaffected.
     - `LANG-GMN.md` examples (`v: open`) and `gmn-heldout-emission-tasks.ttl` (IRIs): consistent.
     - `lang_projection` channel fixtures: no `v:` tokens found.
     - Shipped `generated/` GMN projections: regenerated by the producer, not committed.
   - No insta snapshots pin GMN text.
5. **Same defect elsewhere** (`git grep`): every `== Some(XSD_INTEGER|XSD_DECIMAL)` site
   (`gmn1_codec.rs:2192,2198,2413,2422`, `expression_substrate.rs:283`,
   `logic/src/physical/builtin_eval.rs:208`) is safe, because PurRDF keeps those datatypes
   explicit. `logic/src/conjecture/tests.rs:380` and `logic-compile/.../acceptance.rs:325` are
   test code; the latter already normalizes. `loss_ledger.rs:126,497` (anyURI) and
   `research_objects.rs:238` (dateTime) are safe. The same defect class exists in
   `intern_literal_ref` and `term_sort_string` in this file (F1, F2).
6. **Text.**
   - The commit-message statements are true as far as they go: dataset literals carry no implied
     datatype, no dataset literal reached the inline form, and the reference derivation is
     unchanged. They omit that the change restores four frozen vectors and the witness, that one
     vector stays stale (F2), and that the wasm images are stale (F3).
   - The comment inaccuracies are in F4.
   - No issue numbers or process references in the diff or the message.
   - Prose check was not run; this is a read-only review.

VERDICT: BLOCKED at e0dc12e0e0b9bc272191851e62b86de35085a245
