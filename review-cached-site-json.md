# Independent CachedSite source review

Verdict: no blocking correctness finding in the frozen six-file source unit.
Read-only source review only; no formatter, compilation, tests, host/resource
probe, action store, render, discovery, fixture/corpus production or model run.

## Verified source boundaries

- Both actual CachedSite reads and the single publication writer use the native
  codec. Selected context/receipt verification and warm receipt validation retain
  their original positions; payload verification still precedes Site construction.
  Read-only missing/corrupt paths do not acquire a renderer/build branch.
- CachedSite emits digest then files; BTreeMap provides lexical string-map order.
  Native Record preserves required fields/known duplicate refusal, native map
  decoding preserves last decoded string on repeated names. Strings remain strict.
- Pinned native COMPACT/ShortForms and selected Serde1.0.151 writers match for
  quote, reverse solidus, short C0 escapes, lowercase u00xx for remaining C0, raw
  slash, DEL/C1, U2028/U2029 and other Unicode. The actual new calls select that
  writer. New literal controls cover these cases in both keys and values.
- Shared SHA1 framing is unchanged: exact prefix ending US, UTF8 label, RS,
  canonical bytes, lowercase hex. Both site and book still use label site.
  There is one hashing and one verification authority, not a copied renderer fold.
- CachedModelBody.canonical_bytes uses the same Serde to_vec body and exact
  existing serialization panic. Both model digest/verification calls delegate
  through these bytes; model fields, codec, receipts and selector are unchanged.
- Caller audit confirms all shared payload_digest/verify_payload callers are
  updated within this family. No production serde_json reference remains under
  docs/src outside test modules. The maintainer example and integration/unit
  readers retain their valid dev dependency. Normal Serde/template/receipt and
  docs-model JSON owners remain intact.

## Controls and acceptance limits

All11new json_cached_site_ controls construct plain Site/maps/JSON only.
They exercise actual codecs and shared guards, with independent literal envelope,
payload bytes and SHA1 supplied by root; no expected bytes are generated through
the renderer under test. Unicode/empty/null/type/duplicate/pointer/tamper/
malformed/trailing/nonUTF8 and explicit native-object admission are covered.
Intentional positional-form narrowing and ignored metadata numeric admission
are documented. No numeric conversion participates in the file payload.

The four selected existing controls are genuinely in-memory: docs'
cached_site_round_trips_to_identical_bytes, an_edited_site_envelope_is_refused,
cached_site_rejects_non_utf8_files; docs-model's
an_edited_model_envelope_is_refused. None calls discovery, rendering, prime,
action stores, source inventories or corpus loaders. Other fixture tests are
not implicitly included by this review.

These source observations do not establish executable success. Root's planned
15-control run, actual caller compilation, formatting, strict prebuild and
mandatory hooks remain pending. Actual selected cache/receipt/blob production,
installed/hosted consumers and full corpus/full-gate acceptance are not proved
by the authored in-memory controls.

## Helper repair and formatted final diff

Reviewed the final formatted source after the helper-hygiene finding. Disposition:
the duplicate site_payload_bytes wrapper is deleted, not exempted or relocated.
Both site digest/verification calls now use native json::record::to_vec on the
string BTreeMap; the actual producer writer uses the same native to_vec on
CachedSite. Pinned record.rs639–641 implements exactly
write_compact(value.to_json()).into_bytes(), so this removes an isomorphic local
helper without changing byte ownership, order, escaping, framing or failure paths.
No sibling GMN helper, debt ceiling, suppression, dependency or bridge changed.
Three payload assertions use the native owner directly and all15expectations
remain intact. No new correctness finding in the repaired/formatted diff.

Root reports the initial15controls passed (440outside selection skipped), strict
prebuild29.84s, source admission and formatting passed; helper hygiene then
failed on the duplicate wrapper. Those initial logs are retained with
before-helper-fix names. These are root-run results, not checks executed by this
reviewer. Root reports formatting passed after repair and the15-control rerun is
active. Final rerun/helper/commit-hook results remain pending; earlier runtime
passes do not qualify the changed final source by themselves.
