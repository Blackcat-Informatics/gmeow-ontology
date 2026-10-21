# Independent review: installer action pin update

Disposition: no actionable source finding. Reviewed the merge-base-to-HEAD workflow diff for the existing bot head `18cd5cd05defea8dc2679dacb20c6b21c7f68d0e`; the branch-selection record explains the local scaffold rename without changing source/history. No tests, builds, linters, host configuration or source changes were performed by this reviewer.

## Scope and caller behavior

The entire source diff is fourteen replacements of `taiki-e/install-action@f7e5d7c961414b23f5b25b2da9294395d08513ad` by the full commit pin `1168f6810a3cb30a3514bf5756a3b7f0422c6df8`: eleven in `ci.yml`, two in `release.yml`, and one in `suite-quality.yml`. No other workflow content changes. Tool inputs, explicit versions, conditions, runners, permissions, task/profile selection and failure behavior remain unchanged. In particular, the explicit nextest `0.9.137` and wasm-bindgen-cli `0.2.125` pins remain intact. Existing unversioned inputs remain unversioned; this PR does not introduce that policy.

## Bounded primary upstream inspection

The [upstream comparison](https://github.com/taiki-e/install-action/compare/f7e5d7c961414b23f5b25b2da9294395d08513ad...1168f6810a3cb30a3514bf5756a3b7f0422c6df8) reports 32 commits between the selected revisions and the release changelog identifies the target as 2.87.28. Changed files are tool manifests, changelog, an upstream manifest-maintenance workflow, and the codegen development dependency `minisign-verify` requirement. No action definition or runtime installer script is in the changed-file set. This is a bounded source review, not an audit or execution of downloaded tool binaries.

One real effective version change remains visible: the upstream nextest manifest moves `latest`/`0.9` from `0.9.146` to `0.9.148`, so existing unversioned nextest sites select that newer version. The explicitly pinned nextest sites retain their old selected version. Ripgrep and wasm-bindgen manifests are absent from the upstream diff.

## Acceptance snapshot and limits

Read-only hosted status inspection for the exact PR head found `producer-build`, `helpers`, `lint`, and `rust-prebuild` completed successfully. `generation (a)`, `generation (b)` and `fixture-prefix` were still in progress. This snapshot does not claim completion of dependent consumers, release execution, suite-quality execution or the full CI graph. Local checks were not run because qualification is owned serially by root. No pending job was inferred to have passed.

Review frozen after writing this artifact; no publication, commit or merge action was taken.
