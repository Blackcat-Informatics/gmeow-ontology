# Slice brief JSON owner

Migrate all nine slice-brief packet types from serde to the pinned native JSON record/enum owners. Preserve the public AuthoringPacket::to_json() -> String, record/vector order, Pascal enum names, externally tagged ObjTerm objects, explicit null output optionals, absent/null optional decoding, ignored unknown record fields and known duplicate refusals. Remove both leaf dependencies completely after auditing every crate target and narrow actual callers. Keep semantic digest, assembly and Turtle owners untouched.

Implement type-owned codecs, not a generic compatibility layer. Native records require objects and unit enums require strings; document incidental prior serde admission forms that are intentionally not retained. Confidence must decode as finite f64 and manually constructed nonfinite confidence must hard-fail on native export before it can become null. Preserve the existing infallible string API with actionable panic on invalid export.

Pure external json_packet_* tests use independently constructed typed packets and literal JSON only. Root owns Cargo.lock, formatting, builds, qualification and delivery. No corpus/ontology assembly, production, model or host work from this lane. Full JSON campaign and actual consumer acceptance remain open.
