# Actual hosted production boundary failure

Run: https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39585407453

Generation(a) job119190698964 and fixture-prefix job119190699094. Producer-build, rust-prebuild, helpers and lint succeeded. Generation completed compile_logic and then mapping lowering failed. This establishes the carrier integrity repair for this actual production source; it does not establish the later ONNX or financial installed acceptance.

Exact relevant log lines (GitHub job/step/timestamp prefixes retained):

```text
generation (a)	Cold-materialize the generated product	2026-10-21T11:16:47.8670855Z {"event":"stage_start","stage":"stage-compile-logic"}
generation (a)	Cold-materialize the generated product	2026-10-21T11:18:38.5978968Z {"event":"stage_end","stage":"stage-compile-logic","elapsed_ms":110729}
generation (a)	Cold-materialize the generated product	2026-10-21T11:19:09.2388112Z error gmeow-dev.cli.fail: sync pipeline failed: stage stage-mappings failed: correspondence lowering failed: RDF parse error: canonical callable urn:gmeow:finance:return:closing: field https://blackcatinformatics.ca/math/hasMathematicalSymbol has no selected FnO lowering
fixture-prefix	Produce optimized corpus fixtures before any test runner starts	2026-10-21T11:19:24.4680610Z error gmeow-dev.cli.fail: test fixtures: produce authenticated stage DAG: stage stage-mappings failed: correspondence lowering failed: RDF parse error: canonical callable urn:gmeow:finance:return:closing: field https://blackcatinformatics.ca/math/hasMathematicalSymbol has no selected FnO lowering
```

Logs were retrieved read-only through gh run view --log. No local corpus production or full gate. User campaign exception remains active.
