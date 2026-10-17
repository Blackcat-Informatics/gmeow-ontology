# Issue #1834: build: repair shared-helper ownership and lower stale debt on main

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Reproducer

On current main fb73513b8, `make helpers-hygiene` fails after its scanner self-tests pass. This blocks the full pipeline gate independently of the generation/fixture failures. No ceiling was changed.

## Findings to resolve

- Identical body group `gmeow_affect_ingest::Sink::string` / `gmeow_math_lift::sink::Sink::string` lacks an owning shared implementation or justified distinct-role contract.
- The recorded default-constructor shim cohort is stale after removal of `WorldStore::default`; adjudicate the remaining actual cohort and lower obsolete debt rather than preserve nonexistent members.
- Diagnostic `ReasoningInputFailure` and `ProverFailed` code/register shims need correct shared-macro ownership or demonstrated distinct-role classification. Do not suppress findings merely because they are small.
- The recorded `correspondence_lower::parse_turtle` / `correspondence_soundness::parse_turtle` debt group no longer exists as recorded; remove obsolete debt only after verifying the production shared path.
- `fixed-hasher-everywhere`: 706 copies outside its owner against ceiling 702.
- `frame-le`: 53 copies against ceiling 52.
- `hash-domain`: 46 copies against ceiling 45.

## Acceptance

Route actual duplicate work through the existing semantic owner; preserve every hash-domain/framing and diagnostic invariant. Lower genuinely retired debt. Never raise ceilings, blanket-bless isomorphic code or weaken the scanner. `make helpers-hygiene` must pass, with relevant byte/identity and failure-path controls for changed callers. Full qualification remains separately required by #1757. Principles 4, 7, 12, 17 and 18 apply.


## Comments (0)

