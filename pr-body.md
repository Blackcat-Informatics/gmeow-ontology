Projection-profile inventory still used Serde between the actual mappings producer and its authenticated selected consumer. Move both complete records and both ends to native JSON, keeping source discovery, inventory checks, selected-artifact authentication and receipt commitments intact.

Preserve compact field order, declaration-vector order and sorted measured-map output. Count admission requires strict unsigned tokens within host width. Duplicate record members refuse; duplicate map keys retain their last valid value, while every occurrence must pass admission. Native map decoding owns that distinction. Principles 4 and 7.

Qualification: all seven authored controls passed, covering literal bytes and escaping, required/null/duplicate/unknown policies, integer widths and nested pointers, map duplicates and actual plain-model missing/unregistered/floor checks. Strict producer-independent all-target prebuild, helpers, source admission, lint, formatting, diff checks and normal signed commit/push hooks passed. Source and all controls were independently reviewed.

The campaign exception excludes local optimized producer/corpus/fixture/sync/fullgate and permits sequential merge ignoring CI. The three existing corpus-backed inventory tests were not run. Actual authenticated shipped fixture, excluded embedded consumers, full corpus/installed and remaining dependency-removal acceptance stay open.

Refs #1776
