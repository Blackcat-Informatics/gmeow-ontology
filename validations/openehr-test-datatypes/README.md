# openEHR "Test all datatypes" OPT — constraints-axis law fixture

`TestAllDatatypes.opt` is the CaboLabs EHRServer *Test all datatypes* Operational
Template (Apache-2.0; see `TestAllDatatypes.opt.license`). It is a synthetic template
that constrains one instance of every openEHR RM datatype.

It exists here for one reason: it is a real, permissively-licensed OPT that carries
**both** constraint node kinds no clinical OPT carries together —

- `C_DATE_TIME` — a validity pattern `<pattern>yyyy-mm-ddTHH:MM:SS</pattern>`, and
- `C_DV_ORDINAL` — an ordinal `<list>` of `<value>` integer + coded `<symbol>` pairs.

The native OPT reader (`crates/logic-compile/src/openehr_opt.rs`) lifts these through the
canonical `logic:` validation-shape IR, and the `walker_tests` prove `recover∘lift = id`
(the section/retraction law) on every constraint family parsed from this real XML.

This is a **law fixture only** — a synthetic datatype template is not shippable ontology
content, so it is not wired as a production generator input and its constraints do not
flow into `gmeow.gts`. The production constraint facts flow from the clinical
`validations/openehr-bloodpressure/Blutdruck.opt` (all families the real blood-pressure
OPT carries).
