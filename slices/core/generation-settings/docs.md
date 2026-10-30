<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Generation settings

This slice extends the existing `ModelInvocation` record with typed generation
parameters and the caller’s requested settings profile. The invocation remains
an activity; its model is a `SoftwareAgent`, its concrete instruction a `Prompt`,
and its output links back through `wasGeneratedBy`. No new execution mechanism
or output relation is introduced (Principle 5).

| Property | Value | Meaning |
| --- | --- | --- |
| `generationSeed` | `xsd:integer` | Seed used to initialize this run’s sampler. |
| `generationSteps` | `xsd:nonNegativeInteger` | Sampling or denoising steps the run was given. |
| `guidanceScale` | `xsd:decimal` | Classifier-free guidance parameter supplied for the run. |
| `samplerName` | `xsd:string` | Engine’s exact sampler name, including whitespace. |
| `modelMode` | `ModelMode` | Named settings profile requested by the caller. |

The step setting does not report how many steps actually completed and is not
an agent’s step budget. Guidance scale is neither confidence nor a guidance
rubric. Its numerical interpretation depends on the model and engine.

`samplerName` deliberately uses `xsd:string`: `xsd:token` collapses whitespace
and could alter an engine’s exact designation. Preserve case, spaces and
punctuation. This string names the sampler, not the model, and does not imply
that differently named samplers are equivalent.

`ModelMode` is an open `logic:QualityValue` vocabulary. Its four initial values
express requested intent:

- `modelModeDeepThinking`: extended reasoning when no immediate reply is needed.
- `modelModeCreative`: inventive exploration with room for alternatives.
- `modelModeExtremelyCreative`: the serving layer’s widest creative range.
- `modelModeNarrative`: prose integrating supplied material.

A serving layer can mint further `ModelMode` individuals with their own labels
and definitions. These values have no universal mapping to temperature, top-p,
step count or guidance scale. Record the requested profile and actual supplied
parameters independently; neither implies that the request was fulfilled or
that the output achieved a quality level.

The new properties constrain supplied values without requiring every invocation
to have them. They impose no global uniqueness: attributed records and claims
can coexist, and no value is chosen as a privileged winner. Absence does not
supply a default. Retain conflicting reports with the existing statement
provenance machinery rather than silently replacing them (Principles 3 and 9).

A seed alone cannot establish reproducibility. Model identity, engine behavior,
other parameters and execution conditions also matter. The ordinary
[image-generation scene](examples/image-generation.ttl) records a requested
creative profile alongside actual parameters and the existing provenance links;
it makes no replay or fulfillment claim.
