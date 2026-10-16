The native rule migration also changed the original language module's byte
identity. The producer's shared source catalog emits that identity into the
native browser codebook, but both browser consumers still expected the previous
pin. Current original bytes hash to
`9127a8f94adc3dad1c8ca96fa54c6f6a995b6f737a62352ef99a2b2d0bbe2bb3`.

The combined candidate now refreshes that source pin and strengthens the existing
read-only `verify-wasm-codebook` operation: after authenticating the selected
producer artifact and comparing bytes, it invokes the exact native decoder used
by GMN and validation. This catches an authenticated packet that the compiled
browser consumers would refuse, retaining the original typed diagnostic.

Focused controls passed 4/4, including the original-byte identity check, complete
table round trip, wrong identity/tamper/inconsistent tables, trailing data and
size refusal. Source lint passed. Current-source producer-independent Clippy is
still running. The original standalone producer build was intentionally stopped
with exit 130 for this required source correction; the normal replacement build
is running. Neither invocation is being reported as a full qualification pass.

The authenticated codebook-verifier runtime, all six fresh browser assets, parity,
full corpus/gate acceptance, hosted cold generations, current-head review and
ghprsq merge remain required. These source corrections are local and uncommitted;
the published PR head is unchanged. No new PR or worktree was opened.
