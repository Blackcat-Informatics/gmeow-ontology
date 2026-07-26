# Enactment kernel conformance corpus

## What this corpus pins today

Each case is a kernel A-Box driven through the same conformance harness the rest
of `conformance/logic/` uses, and each blesses the full projection fan-out:
canonical RDF 1.2, Datalog, N3, OWL-DL, OWL-EL, gUFO, the preservation ledger and
the projection report. That fan-out is substantial — a quarter of a megabyte of
projection report per case — and it is genuine coverage: it pins how the new
vocabulary lowers into every supported surface, so a change that silently drops
or mis-lowers a kernel term fails here.

## What it does NOT pin, and why that is written down

The derived artifacts (`materialized.nq`, `verdicts.json`) are **empty**, and the
budget case saturates trivially. This is not an oversight in the cases; it is a
consequence of the native derivation contracts (the frontier derivation, dispatch
gating, reconciliation and compensation classification, the typed-outcome fold)
not being implemented — recorded in `.deficiencies`. With no rules over the kernel
vocabulary, the closure is empty and every case's reasoning verdict is trivially
the same.

So a reader should take these cases as **projection-level** conformance, not as
evidence that the kernel's reasoning behaves correctly. The behavioural claims in
each case header describe the structure the A-Box encodes, not a derivation the
engine performed.

The cases are committed in this state deliberately. When the derivations land,
these goldens move — and the diff is exactly the review surface someone will need
in order to check that the frontier, the reconciliation routing and the budget
honesty came out right.
