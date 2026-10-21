# Independent primer source review

Reviewed the seven files in `validation-primer.md` against the prior source,
the actual producer/consumer calls, and pinned native JSON owner `017f552`.
This reviewer did not implement the primer unit and ran no checks.

## Concrete finding

**Unsigned signed-zero admission is not implemented as documented.**
`Gmn1Primer::from_json` reads both counters through generic `usize` decoding.
Pinned `crates/lex/src/json/record.rs:1209` checks integer syntax then calls
`Number::as_i128`, followed by `usize::try_from`. Pinned
`crates/lex/src/json/number.rs:416` explicitly maps `-0` to zero. Consequently
both fields admit `-0`, contradicting `PRIMER-JSON.md` and the new
`json_gmn_primer_unsigned_counters_require_exact_bounded_integer_tokens`
negative control. Preserve the promised unsigned token contract through a
narrow native-number admission at these two fields, with target-width checked
conversion; do not remove the control or weaken its expectation. Native
`Number::as_u64` already explicitly refuses negative lexemes including `-0`.
This is source-derived evidence, not an executed failure.

## Other reviewed boundaries

- The five codecs preserve field order and the three external Pascal tags.
  The direct producer now uses those codecs and compact output without adding
  a newline. No rendering, task observation or graph construction changed.
- Native `Record::raw` refuses duplicate known fields; required fields retain
  strict types, unknown record fields remain ignored, and optional `ast_error`
  preserves absent/null versus a present empty string. The enum separately
  requires exactly one tag and an object payload; positional forms are an
  explicit documented admission change.
- Task vectors retain order; the existing `BTreeSet<String>` owners provide
  sorted/deduplicated set output. There is no float-valued typed field or
  numeric identity conversion.
- Both authenticated readers still load the same selected artifact before
  decoding. The docs reader's selector-bound `OnceLock`, identity assertion
  and all corpus assertions remain unchanged. The bundle reader retains its
  authenticated loader and cached primer. No new corpus production or fallback
  path appears in the diff.
- The nine new tests construct private plain records and authored JSON only.
  Their helpers call codecs and pure rendering/accessors; they do not call
  graph/dictionary/observation/fixture/producer APIs. Literal complete bytes,
  duplicate/type/variant refusals and malformed inputs provide meaningful
  coverage, subject to execution.

No additional concrete defect was found in this bounded source review. Build,
pure test results, producer/authenticated consumer acceptance and full parent
acceptance remain unestablished by this review. This source review grants no
additional waiver and does not establish broader acceptance.

## Repair disposition

Re-reviewed the three narrow repairs after source refreeze. The original
signed-zero finding is addressed in source: both counters now use
`required_with`, native `Value::as_u64` and checked `usize` conversion. Pinned
`value.rs:407` delegates to `Number::as_u64`, which rejects negative lexemes
including `-0`, fractions, exponents and overflow. The existing negative test
remains unchanged. Root reported the initial run as 18 executed, 17 passed and
one signed-zero failure, retained in `native-tests-initial.log`; this reviewer
did not execute that run or the rerun.

Direct native `BTreeSet` codecs preserve the prior array-of-strings admission,
sorted output and deduplication: pinned `record.rs:1375` reads typed items into
the set and `record.rs:1384` emits the set's ordered iteration. The authenticated
docs reader now converts the native `DecodeError` directly into `Diag`, retaining
its error rather than wrapping it in an unrelated I/O error. Authentication,
selector identity checks, test expectations and primitive-reader source remain
unchanged. No new concrete issue found in these repairs; executable and broader
acceptance remain root-owned and unestablished by this source review.
