Record the canonical profile beside the bundle and envelope digests

The medium envelope and the coherence certificate now record the canonical
profile id and version and the flat presentation id and version they were
produced under. A reader refuses an envelope recorded under another profile
before any digest comparison, because a digest taken over canonical output
moves when the profile moves. The MCP reader requires the marker. The
envelope contract is stated as eleven fields, and docs/GATE-AND-PIPELINE.md
is corrected to match. The reviewed-coverage golden is regenerated.

Hosted run red only on the standing jobs red on main (generation (a),
generation (b), fixture-prefix), tracked on the standing-reds issue.
