Refreshed the upstream delivery state against the current forge and official
registry index:

- PurRDF issues #451 and #453 close against
  `866765b3e88bb6ae5108e387708d29a85c101b17`; #452 closes against
  `eb1fd88b0283a8a53efea6374243c4e5c2ff3161`. These are no longer open upstream
  defects as the earlier benchmark discussion recorded.
- Both closure commits are strictly after `rust-v3.0.1`: 783 and 800 commits ahead,
  respectively, with zero commits behind. The official crates.io Git index still
  lists 3.0.1 as its newest non-yanked `purrdf` entry. GMEOW locks 3.0.0, and a
  registry update to 3.0.1 would not bring in these fixes.
- This is publication evidence, not a new performance or semantic benchmark.
  The old large-world timings are not measurements of the current upstream
  implementation. Exact production-world decisions, source-bound proofs,
  world/budget/stop semantics and store seams still need actual consumer evidence
  before delegation and deletion can qualify.

No dependency, compiler profile, fallback, selected world or verification gate
was changed during this check. The current GMEOW source qualification and normal
optimized producer build continue; no production completion is claimed.
