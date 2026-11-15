# Recheck review: PR 2805 (`paudley/2745-gmn-literal-identifier`)

- Head reviewed: `395ffe5e88b9d916f9f630a4cc3c5c15e07275ba` (`git rev-parse HEAD`; the PR `headRefOid` is the same)
- Delta reviewed: `git diff e0dc12e0e 395ffe5e8`. One commit: `gmn1_codec.rs` +23/-24, `gmn1_codec.tests.rs` +72/-3
- Timestamp (`date`): 2026-11-14 20:33:53 CST
- Mode: read-only. No builds, cargo, make or test runs by the reviewer. The hashes below were
  computed with `sha256sum`, using the codec's `digest16` formula (`emit.rs:22-25`).
- Prior review: `.stage/reviews/review-2745-gmn-at-e0dc12e0e.md` (findings F1 to F5).

## Read / not read

| File | Lines | Commit |
|---|---|---|
| crates/lang-bridge/src/gmn1_codec.rs | full delta; 276-300, 2030-2100, 2195-2215, 2395-2465, 2555-2615; every `datatype` occurrence | 395ffe5e8 |
| crates/lang-bridge/src/gmn1_codec.tests.rs | full delta (742-838) | 395ffe5e8 |
| crates/lang-bridge/src (all non-test) | `git grep '\.datatype\b'` | 395ffe5e8 |
| crates/lang-bridge/src/registry.rs | 1296 (the only other `.datatype` read) | 395ffe5e8 |
| slices/grounding/lang/tests/gmn1-vectors/** | every `.gmn` (positive and negative), literal lines of every `.in.ttl` | 395ffe5e8 |
| Repository | `git grep 'r_[0-9a-f]{16}'` over all tracked text; `git ls-files '*.gmn'` | 395ffe5e8 |
| crates/gmn-wasm/js/index.mjs, js/.gitignore, Makefile 903-909 | grep level | 395ffe5e8 |
| crates/mcp/src/tests/segment_deferral.rs | 69-76, 226-250; crates/pipeline/.../gmn_migration.rs 18-23; crates/gmeow-cli/tests/gmn_migrate.rs 85-112 | 395ffe5e8 |
| purrdf-core-4.0.0 `model.rs` 137-168 (`language_datatype_iri`, `datatype_iri`) | as listed | registry |
| purrdf-core-2.0.2 `model.rs` 103-109, `ir/dataset.rs` 596-626, `ir/builder.rs` intern_literal | as listed | registry |
| Codec at `ec2a0380a` (introduced the classifier) | `from_dataset` 121-146; Cargo.lock purrdf version | ec2a0380a |
| Cargo.lock at `56adc1cc8` (golden vectors committed) | purrdf entries | 56adc1cc8 |
| PR body (`gh pr view 2805`), commit message | whole | 395ffe5e8 |
| Coder evidence `.stage/issue-2745-gmn/` 10, 13 (+ probe script), 18, 19, 20, 21, issue-direction.md | headers, summaries, exits | n/a |

Not read: the rest of `gmn1_codec.rs` and its tests (unchanged since the prior review),
`crates/lang-bridge/tests/gmn1_roundtrip.rs` beyond a grep, and the compiled wasm images.
purrdf 0.4.0 and 0.6.0, the versions pinned when the classifier and the vectors were introduced,
are not in the local registry, so I could not read their literal representation. That gap is
noted under question 2.

## Findings

Prior REQUIRED findings at e0dc12e0e: F1 and F2 are resolved at this head (questions 1 and 2).
F3 is reassessed under question 5. F4 is resolved. F5's out-of-scope note (base direction) now
has an owner: issue 2798, OPEN, "GMN-1 codec drops the base direction of a by-reference literal".

New findings at this head:

- **N1. OPTIONAL. The PR body overstates the decoder's use of the accessor slightly.** The body
  says "The classifier, the record order, the by-reference key and payload, and the decoder all
  use it." `decode_value` (`gmn1_codec.rs:2555-2591`) builds literals from the constants
  `XSD_INTEGER`, `XSD_DECIMAL` and `XSD_STRING`. `payload_to_term` (`:2596-2610`) reads the
  stored payload string, which `literal_datatype` produced at `:2461`. Neither calls
  `literal_datatype`. Saying the decoder "restores the payload the accessor wrote" would be exact.
  Not a code defect.
- **N2. OPTIONAL. F3 (stale prebuilt wasm images) stays open, with no named owner.** The PR
  states the problem truthfully. It does not name the owner of the refresh, so the red Node lane
  on `main` has no visible tracker in this PR. See question 5.

No REQUIRED findings at this head.

## Answers

### 1. F1: one datatype accessor, and byte idempotence

- `literal_datatype` (`:287-289`) returns `lit.datatype_iri()`. In PurRDF 4 (`model.rs:149-168`)
  that resolves as follows: a language tag gives `rdf:dirLangString` with a direction and
  `rdf:langString` without one. Otherwise an explicit datatype is returned as is, and an absent
  one becomes `xsd:string`.
- Every datatype read in `gmn1_codec.rs` now goes through it:
  - `term_sort_string` (`:297`), which covers record order and dedup through `quad_sort_key`;
  - `encode_embedded_literal` (`:2204`, `:2207`);
  - `classify_literal` (`:2402`, `:2416`, `:2425`);
  - `intern_literal_ref`, both the key (`:2453`) and the payload (`:2461`).
- `payload_to_term` (`:2606`) uses the payload's `String` and drops it when a language is present.
- No `lit.datatype` field read is left in the codec. In all of `crates/lang-bridge/src`
  (non-test) the only other read is `registry.rs:1296`. That line renders the schema-version tag
  as N-Triples and never reaches GMN encoding, so it is not a codec site.

Hand trace at this head: `gmeow:gate1 gmeow:statusLabel "open", "open"@en` from a dataset.

1. The dataset gives `"open"` (datatype `None`, no language), with sort string
   `open␟http://www.w3.org/2001/XMLSchema#string␟`.
2. It gives `"open"@en` (datatype `None`, language `en`), with sort string
   `open␟http://www.w3.org/1999/02/22-rdf-syntax-ns#langString␟en`.
3. `…/1999/…` sorts before `…/2001/…`, so the `@en` record comes first.
4. The writer emits `@c{…, v: r_K}` with K = digest(`open␟rdf:langString␟en`), then
   `@c{…, v: open}`.
5. Decode: `r_K` gives payload `{open, rdf:langString, en}`, which becomes `"open"@en` with
   datatype `None`. `open` becomes `"open"^^xsd:string`.
6. Re-sort: `datatype_iri` gives the same two strings as before, so the order is unchanged.
7. Re-encode: the `@en` literal gets the same key K and an equal payload. `"open"^^xsd:string`
   is still a plain string, so it is written inline as `open` again.
8. Text and reference table are byte-equal: `encode(decode(doc)) == doc`.

`"true"` beside `"true"^^xsd:boolean` works the same way. `#boolean` sorts before `#string` in
both the original and the decoded model, and the boolean payload decodes to `Some(xsd:boolean)`.
The two spellings of a literal now also dedup to one quad, which closes the hand-built
duplicate-record edge noted in the prior review.

### 2. F2: reference keys against every committed pin; the language-tagged contract

- **`by-ref-annotation.gmn`**: the input `"…"^^xsd:string` comes back from the dataset as `None`,
  and `datatype_iri` gives `xsd:string`.
  `sha256("gmn1-literal␟Mint one entry per aliased term; changelog and provenance notes ride by reference.␟http://www.w3.org/2001/XMLSchema#string␟")[..8]`
  is `45c4ffaf2bdc2a4f`, which equals the committed `r_45c4ffaf2bdc2a4f`.
- **`by-ref-confidence.gmn`**: `"0.9025"^^xsd:decimal` is not a canonical two-digit decimal, so
  it goes by reference with datatype `xsd:decimal`. The key is `07f8605e3b31c6df`, which equals
  the committed `r_07f8605e3b31c6df`. This is unchanged from base.
- **Other positive vectors**: there are no other `r_` keys. The inline repair ids `c42`, `c17`
  and `cTT` match the head, since plain identifiers are inline. The rest are IRI-only.
- **Negative vectors** (`negative-codec/*.gmn`) contain no `r_` keys. Their failure classes do
  not depend on literal keys.
- **Elsewhere**: `git grep 'r_[0-9a-f]{16}'` over all tracked text finds only those two vectors
  and the new unit test. No other tracked `.gmn` exists. No snapshot, corpus file or document
  pins a language-tagged or directional key. The two `.in.ttl` comments read `r_<hash>`
  generically.

**The language-tagged contract.**

- Base (`331c554f8`, PurRDF 4) hashed a dataset-read language-tagged literal with an empty
  datatype: `lex␟␟lang`.
- The head hashes `lex␟rdf:langString␟lang`, or `rdf:dirLangString` when a direction is present.
- Under PurRDF 2.x the dataset reader returned `Some(datatype_iri)` for every literal
  (`purrdf-core-2.0.2/src/ir/dataset.rs:612-617`). The interned datatype for a language tag is
  `datatype_iri()`, which is `rdf:langString` or `rdf:dirLangString` (`model.rs:103-109`,
  `ir/builder.rs` intern_literal). The base `unwrap_or("")` codec therefore emitted
  `lex␟rdf:langString␟lang` under 2.x, which is exactly what the head computes now.
- The codec at `ec2a0380a` already took literals straight from `ds.owned_quads()` with the same
  key formula.
- The one surviving pin from that era (`by-ref-annotation`, explicit `xsd:string`) is consistent
  with "the dataset hands out the expanded datatype". But purrdf 0.4/0.6 are not in the
  registry, so the language case before 2.x is inferred, not read.
- **Recommendation: keep the language-tagged change. Do not narrow it to plain strings.**
  - It is the same single rule (the datatype PurRDF interns as the term's identity) applied
    without a special case.
  - It reproduces the keys the codec emitted before the library changed its representation.
  - No committed artifact pins the base `lex␟␟lang` form.
  - Narrowing would bring F1's two-spelling divergence back for language-tagged literals: a
    hand-built `Some(rdf:langString)` and a dataset `None` would get different keys and orders.
  - Side effect: `"x"@en--ltr` and `"x"@en` no longer share one key, which reduces issue 2798.

### 3. Payload and decode equality

- **Plain**: the payload stores `xsd:string` and decodes as `Some(xsd:string)`. Canonically
  equal: PurRDF interns `None` and `Some(xsd:string)` to one id. Sort and key now agree too.
- **Typed**: the payload stores the IRI and decodes as `Some(IRI)`. Identical.
- **Language-tagged**: the payload stores `rdf:langString` and decodes with datatype `None` and
  the original language. Identical to the dataset spelling.
- **Directional**: the payload stores `rdf:dirLangString`, but `direction` is still restored as
  `None` (`:2607`). The decoded term is `"x"@en`, the canonical comparison fails, and
  `round_trip_check` hard-fails. This was already true at base, where direction was dropped as
  well. Issue 2798 tracks it. Nothing gets worse silently.
- **Payload format**: `RefPayload` is private and has no serde derive. `Gmn1Document.refs` is
  private, and the only public constructor is `from_text`, which builds an empty table
  (`:2087-2092`). The reference table therefore exists only in memory, alongside the writer's own
  output. No serialized form exists, so a base-written document has no stored payload to misread.
- No other reader of the payload exists: no `RefPayload` use outside `gmn1_codec.rs` and its
  tests. The wasm `from_gmn1` and the mcp `gmn_expand` both go through `from_text`.
- What persists is the text, whose `r_` keys changed. Base-written text with a by-reference
  literal could never be decoded from text alone anyway, because its key is dangling against an
  empty table.

### 4. The two new tests

- **`sibling_literals_sharing_a_lexical_form_reencode_to_identical_bytes`**
  - At e0dc12e0e: `round_trip_with_claims_check` fails inside `check_reencoding` (the F1 trace),
    so the test panics. The coder's log `10-new-tests-on-prior-codec.log` (HEAD `e0dc12e0e`,
    `EXIT=100`) shows it failing at line 803.
  - At the head it passes per the trace above, and log `19` (head, clean tree, 251 passed,
    `EXIT=0`) agrees.
  - It goes through production `round_trip_with_claims_check` and `idempotence_check` on a
    dataset-derived model.
- **`by_reference_plain_string_has_one_key_for_implied_and_explicit_datatype`**
  - At e0dc12e0e: the implied key is `a86d449bbee7531b` and the explicit key is
    `45c4ffaf2bdc2a4f`, so the documents differ and the `assert_eq!` fails (log `10`, line 826).
  - At the head both give `45c4ffaf2bdc2a4f` with equal payloads, so it passes.
  - It goes through `from_dataset`, `classify_model`, `gmn1_write`, `gmn1_read` and the
    canonical comparison. It pins the frozen vector's key at unit level, which is sound because
    the vector test itself needs the producer.
- **F4 comment** (`:745-749`): now accurate. Only `r_` collides in a value slot, and the `x_`/`_b`
  exclusion is described as conservative.

### 5. F3: what is red on main, and whether this commit turns anything red

The committed image (`crates/docs/assets/gmn/gmeow_gmn_wasm_bg.wasm`) and its assets are
unchanged between `331c554f8` and `395ffe5e8` (`git diff --stat` is empty for `crates/docs`,
`crates/gmn-wasm` and `slices`). The coder's probe (log `13`, node over the committed image)
printed `v: r_66636c2db6b9ecc4` for the witness input. That equals
`sha256("gmn1-literal␟open␟␟")[..8]`, the base `None`/empty-datatype convention, confirming the
image embeds the base codec with PurRDF 4.

- **Red on `main` and still red at the head:**
  `crates/gmn-wasm/js/tests/witness.test.mjs` "committed GMN bindings and image reproduce the
  native attestation" (`:52-68`). The image emits `r_66636c…`; `WITNESS.gmn1.txt` says `v: open`.
- **Red on `main`, green at the head by reading** (built from source; `js/pkg/` is git-ignored
  and built by `make gmn-wasm-pkg`):
  - native `crates/gmn-wasm/tests/witness_gmn.rs`;
  - Node "wasm GMN-1 transcode is byte-identical to the native witness attestation";
  - Node "wasm GMN-1 round-trip reproduces …". On main the text-only `from_gmn1` meets a
    dangling `r_` key.
- **This commit turns nothing red**, by reading:
  - The frozen-vector corpus test goes from red at base to consistent at the head (question 2).
  - The grounding round-trip and coverage tests are consistent.
  - `gmeow-cli` gmn tests are IRI- or glyph-only (`gmn_conformance.rs` FROZEN, `gmn_migrate.rs`).
  - The mcp `encode_gmn1` test uses an IRI-only N-Triples input and asserts only
    `round_trip: true`.
  - `gmeow-lang-bridge` has 251 passed (log `19`).
  - Not run by anyone: the corpus tests, `gmn-wasm` native and Node, and mcp at this head.

### 6. PR description and commit message

- The commit message is true at this head: one accessor; order, dedup, classification, key and
  payload all use it; plain-string keys go back to the `xsd:string` key; language-tagged keys
  move to the language-datatype key; sibling order follows the expanded datatype.
- PR body:
  - "only place the codec reads a literal's datatype": true.
  - "decoder … use[s] it": loose (N1).
  - "returns to the key … `by-ref-annotation.gmn` pins": true (recomputed).
  - "no committed vector pins a language-tagged key": true.
  - "251 passed, clippy clean": logs `19` and `20` at `395ffe5e8`, clean tree, `EXIT=0`.
  - "last two fail on the previous codec": log `10` at `e0dc12e0e`.
  - "committed GMN image already disagrees with the witness on `main` … red before and after":
    true (question 5).
  - "Base direction … is #2798": issue open, title matches.
  - "Not shown: no producer pass": accurate disclosure.
- No process references in repository files in the delta. Issue numbers appear only in the PR
  body, which is allowed on the forge.

VERDICT: PASS at 395ffe5e88b9d916f9f630a4cc3c5c15e07275ba
