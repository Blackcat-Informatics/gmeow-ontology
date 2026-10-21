# Issue 1776 — complete rendered-site cache JSON family

Base `a24647340`, after cost PR 1921 integration. This source delivery owns only
the CachedSite envelope and the shared canonical-payload byte boundary. Parent
issue 1776 remains open; model, receipt, diagnostic and template owners are not
reclassified as native.

## Actual source and caller closure

`docs/src/fixture.rs` has one CachedSite producer write and two reads: the
read-only producer-selected loader and the explicit producer's warm-cache path.
Site languages and book artifacts already share this envelope. Both reads still
reach `into_site`'s integrity guard. SelectedAction/context/receipt validation,
source identities and missing/corrupt fail-closed behavior stay in place.

The sole payload digest/verification owner is in `docs-model/src/fixture.rs`.
Its API now receives canonical payload bytes, keeping the exact SHA-1 prefix,
label, separators and content framing. Its two model callers explicitly supply
their unchanged Serde bytes and preserve the existing model serialization panic.
Its two site callers supply native compact sorted string-map bytes. The `site`
label stays in force for book artifacts too; no duplicated hashing/verification
owner or native-to-Serde bridge is introduced.

## Implementation and wire boundaries

- Native ToJson/FromJson owns CachedSite: `digest`, then `files`; inner paths are
  sorted by the existing BTreeMap. Compact UTF-8/no terminal LF and string escape
  bytes preserve existing output and digest identity.
- Require both members with strict string/object-of-string types. Known envelope
  duplicates refuse; unknown fields remain ignored; file-map duplicates retain
  last-string behavior. Positional records are explicitly unsupported. Ignored
  metadata may contain native RFC numbers; numbers never become file text.
- Keep exact UTF-8 file reconstruction and non-UTF-8 refusal. Changed values or
  declared digest still panic through the same tamper guard before reconstruction.
- Remove only docs' production serde_json dependency. Keep dev serde_json,
  template/receipt Serde, CachedModel Serde and existing native dependencies.
- Eleven external authored JSON/Site controls use no store, real render,
  discovery, prime, fixture, corpus or producer construction. Existing fixture
  test logic is unchanged; one obsolete Serde-specific comment is corrected.

## Exact source inventory

1. `crates/docs/Cargo.toml`
2. `crates/docs/src/fixture.rs`
3. `crates/docs/src/fixture.tests.rs` (comment only)
4. `crates/docs/src/fixture.json.tests.rs` (new)
5. `crates/docs/CACHED-SITE-JSON.md` (new)
6. `crates/docs-model/src/fixture.rs`

Root owns lock resolution, formatting, verification and publication. No pipeline
cost/timing/merger, baseline, cache format/version or authenticated selector edits.
