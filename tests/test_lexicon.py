"""Structural + DL-safety guards for the lexicon layer (#171).

Pins the evidence-vs-claim separation: UsageAttestation is evidence, not truth;
EtymologicalDerivation is a standpointed claim graph, not a flat property.

All asserted-TBox structural assertions have been migrated to
slices/extensions/lexicon/tests/structural.ttl (declarative slicetest cells).
No pytest functions remain in this file.
"""
