The agreement-tally producer and dashboard reader now share native JSON record ownership. The producer keeps sorted corpus and shape keys, declared field order, omitted empty maps and the exact pretty-printed trailing-newline format. Declared fields reject duplicates and invalid unsigned counts; missing maps remain empty and unknown members remain ignored. Typed records now require objects, rejecting the former decoder's incidental positional-array form.

The dashboard also computes rates with wider intermediates and refuses overflowing headline or shape totals with the offending corpus or shape identified. Divergence separation and rounded agreement rates retain their existing behavior.

Validation: 17 focused authored-data tests passed, including the unchanged dashboard controls, wire bytes, malformed inputs, integer boundaries, rounding and all aggregate overflow paths. Strict prebuild, helper/source checks and normal hooks are recorded in the selected Stage evidence. Independent source review is clear.

Refs #1776. Principles 4 and 7. This is one complete record-family migration; the parent remains open. Authenticated corpus, installed consumers and the full gate were not run under the recorded campaign exception. CI will not be awaited under the user's sequential-merge exception.
