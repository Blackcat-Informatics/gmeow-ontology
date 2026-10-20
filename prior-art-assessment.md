# Existing implementation assessment

Self-review, not independent approval. Completed hosted generation jobs 118742222497, 118742222689 and 118752733577 fail at stage-export-pydantic on the same imported Blutdruck-Diastolic-cardinality target. Source confirms the OPT reader deliberately mints artifact-specific target classes beneath the two source namespaces allocated by compile_logic.rs. Those types do not come from authored slice modules and therefore have no DocTerm.

The exporter currently interprets every target whose IRI starts with a registered term namespace as a mandatory documented ontology class. That includes the source-specific OPT namespaces. The existing shared/helper route already supports generated schema definitions, but the broad required-documentation set prevents it from receiving these imported types. Reuse that route and the actual producer-owned namespace declarations; retain source class/artifact identity, ordinary missing-doc guards, validated ownership and all native rendering.

No new schema engine, term coat, dependency, Python infrastructure or upstream change is needed. Identity-key collision checks must prevent local-name aliasing from attaching another term's documentation to an imported class. Actual corpus acceptance remains unverified until a later authenticated producer completes.
