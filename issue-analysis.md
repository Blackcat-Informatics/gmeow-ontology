# Requirements analysis

Primary-agent analysis; no independent dispatcher is available and no independent approval is claimed.

The complete live issue and its empty comment set were read. It asks for actionable consumer guidance, not a change to WEMI identity or an automatic memory entailment. The body contains five questions across three headings. Wording needs its own language-bearing value, distinct from a naming label. Media identifiers need an explanation of current domains. Extent needs a size-string versus quantity distinction. BodyValue needs an object-versus-literal and decoded-versus-carrier distinction. Memory needs a claim/object bridge that is queryable without erasing their different identity criteria.

The current source establishes MediaObject -> Manifestation -> CreativeWork, so widening identifier is unnecessary and would alter the requested explanation. hasIdentifier already supplies generic entity identifiers. MemoryItem -> Observation and memoryOf name the existing remembered claim and its remembering agent; remembers names a subject. A query can join those explicit observations without a rule deriving either assertion.

The wording slot introduces one real semantic invariant: values carry language, including RDF 1.2 direction. The existing termHasLang projection lacks total negative behavior on non-literals and has no native intrinsic registration. Both must work before documenting a guarded wording property. Native and projected synthetic failure cases, plus canonical vocabulary coverage and bilingual annotations, are the applicable focused checks. The source-owned corpus lane remains intact and is not produced locally. See plan.md for the five-row completeness contract and full publication/cleanup scope.
