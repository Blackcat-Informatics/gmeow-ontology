<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Generation settings design

The existing AI slice owns `ModelInvocation`, `Prompt`, model identity and text
sampling parameters. This slice contributes a small extension for generation
archives and requested profiles, reusing that owner rather than defining a
parallel run record (Principles 4 and 5).

Canonical `logic:domain` and `logic:range` axioms describe the new relations.
Class-local `logic:allValuesFrom` restrictions constrain only supplied values.
There are no closure entries, existential obligations, functional carriers or
new integrity-ledger records. Existing invocations gain no additional mandatory
paths. Datatype ranges retain integer seeds, nonnegative configured steps and
decimal classifier-free guidance without inventing engine-independent bounds.

The exact sampler designation admits `xsd:string` and explicitly supplied
`xsd:token` through a canonical datatype union. Keep its supplied datatype and
lexical form; converting a string to token could lose source whitespace. There is no closed
sampler registry or speculative alignment to an external execution vocabulary:
the supplied name alone does not establish an external sampler identity.

Requested profiles are open quality values. The four initial individuals denote
intents rather than ranked output quality, mandatory parameter recipes or
execution outcomes. Serving layers may supply additional profiles. An alias may
identify a model endpoint while `modelMode` states what the caller requested;
these remain separate relations. No profile implies any numerical setting,
success, completion or reproducibility.

All authored axioms remain in `module.ttl`; downstream validation and external
formal surfaces are projections of this source (Principle 17). The module adds
no hand-authored shapes, modifies no legacy AI validation and introduces no
runtime or model lifecycle authority.
