Part of #2756. That issue is closed once the hosted `helpers` check has passed on `main` after this merge.

The hosted `helpers` check passed on `main` on 2026-11-12 and has failed on every run since, with three lines.
This change removes all three without raising any recorded debt, adding any sanctioned group, or touching the
checker.

## Changes

1. `helpers-census-debt.json`: one line removed. The recorded group of `Default` implementations still listed
   `gmeow_pipeline::stages::medium_dictionaries::MediumDictionariesStage::default`, which no longer exists. The
   group is otherwise unchanged. This clears two of the three lines (the unsanctioned group and the stale
   record are the same group seen from both sides).
2. `crates/gmeow-cli/src/commands/prove.rs`: the constant `PROVE_DOMAIN_AUTHORITY` is renamed
   `PROVE_WORLD_AUTHORITY`. Its value and its single use are unchanged. This clears the third line
   (`hash-domain`: 46 copies, debt allows 45) and returns the count to 45.

## Why a rename is the right fix for the third line, and where a reviewer should push back

The `hash-domain` job counts a constant whose name contains `DOMAIN` and whose value has the shape of a domain
label. A rename is also how one would dodge that rule, so the reasoning is set out here to be checked.

- The constant is the `authority` argument of `SelectedLogicalWorld::new`
  (`crates/logic/src/physical/domain.rs`). It names the authority for the logical world that `prove` selects. It
  is not passed to a hasher as a domain separator, so the job's home helper for hash domains does not apply to
  it.
- Four other constants have the same role and the same value shape, and are named `AUTHORITY`:
  `crates/logic/src/entail.rs` (`gmeow.entail.default-refutation.v1`), `crates/logic/src/reasoning_graphs.rs`
  (`gmeow.pipeline.object-level.v1`), `crates/pipeline/src/stages/conformance/native_scene.rs` and
  `crates/pipeline/src/stages/reason_test_support.rs`. None of them is counted by the job. The constant in
  `prove.rs` was the only one of the five whose name said "domain".
- The allowance stays at 45. `helpers-ledger.toml` is unchanged.

If these authority labels ought to be registered hash domains, that applies to all five and is a separate
decision; this change does not make it.

## Verification

Run at the head of this branch:

- `make helpers-hygiene`: exit 0, no `FAIL` line. Before the change it exited non-zero with the three lines.
- A listing of the sites the `hash-domain` job counts: 46 before the rename, 45 after; the site removed is
  `prove.rs` line 51. The earlier guesses (`sha256_hex` in `commands.rs`, the self-sufficiency test) are not
  among the counted sites.
- `cargo clippy --locked -p gmeow-cli --all-targets -- -D warnings`: exit 0. The embedded bundle the build
  script needs came from an earlier producer pass, so this shows that the crate compiles and lints, and
  nothing about the bundle.
- The commit hooks passed.

Not run:

- The tests of `gmeow-cli`. The change renames a private constant and leaves its value alone.
- The full repository gate (`make check`).

## Hosted checks

Every check has finished (run 43537767869, head `b442f200d`).

Passed: `helpers`, `lint`, `rust-prebuild`, `producer-build`. `helpers` had failed on every run of `main` since
2026-11-12.

Failed, each beside the same job on `main` for the same source tree (run 43463810521 at `4c60225bb`, whose tree
is identical to the head of `main`):

| Job | This pull request | `main` |
|---|---|---|
| `generation (a)` | The operation was canceled. | The hosted runner lost communication with the server. |
| `generation (b)` | The operation was canceled. | The operation was canceled. |
| `fixture-prefix` | The hosted runner lost communication with the server. | The operation was canceled. |
| `quality` | a quality job did not succeed (the roll-up of the three above) | the same roll-up |

These three jobs run the producer, which exhausts the memory of the hosted runner; which of them loses its
runner and which is cancelled varies from run to run. This change does not touch what they run. No job fails
here that passes on `main`.

## Review

- An independent reader other than the author passed the exact head with no required finding, including the
  rename.
- The review bot reviewed the head. It left no inline comment and one finding, from its linked-issues check:
  #2756 said the counted site was to be routed through the job's home helper, or the work stopped and reported
  on the issue, and this change renames the constant instead. That is answered on #2756: the site is not a hash
  domain, so the route the issue named does not exist; the issue's item is withdrawn there with the reason.
- The limit of the `hash-domain` rule that the rename exposes is filed as #2769.
