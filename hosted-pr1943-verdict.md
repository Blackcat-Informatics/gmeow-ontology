# Hosted ONNX witness follow-through

Run39706670295, exact PR1943 source57e255c68f52640eb111bf4af5a5290b22f2cd31. Producer-build119588204082, prebuild119588204590, helpers119588204435 and lint119588204324 succeeded. Current forge observation: fixture-prefix119598438279 and generation(a)119598438306 completed failure; generation(b)119598438314 remains in progress. This is not full generation acceptance.

Full raw logs captured without ANSI removal or truncation:

- hosted-pr1943-fixture-prefix.log: 28,559,281 bytes, 73,406 lines. Line73402 at2026-10-22T03:26:08.9422029Z reports runner shutdown; line73404 reports operation canceled. No stage event JSON records in this human-output prefix log.
- hosted-pr1943-generation-a.log: 28,539,075 bytes, 73,174 lines. Line73171 at2026-10-22T03:26:03.0241359Z reports runner shutdown; line73173 reports operation canceled. No semantic parity error or actual-graph failure frame appears.

Generation(a) contains actual completed producer-stage events:

- Line66298: stage-math-producers start2026-10-22T02:15:40.9501897Z.
- Line66311: stage-math-producers end2026-10-22T02:15:53.3258110Z, elapsed_ms12369.
- Research objects also completed: line66306, stage-export-research-objects end2026-10-22T02:15:48.9977801Z, elapsed_ms14633.
- Later stage-validate ended line72988, elapsed_ms29988; docs-render ended72991, verify-attestation ended72992. The last stage event is snapshot start72993 at03:24:17.2005582Z; no snapshot end before shutdown.

Root re-read the real math_producers execute path: producer_turtles is followed unconditionally by enforce_producer_fixture_parity(input.root,&turtles)?; exact canonical fixture-versus-producer inequality returns terminal StageFailed. The successful stage_end therefore establishes the reviewed ONNX producer/fixture parity at this exact hosted input, including the intended metadata/type/domain fixture update from1943. It does not establish full generated products, selector authentication, downstream tests, installed behavior or full model-serving/numerical parent acceptance.

Diagnosis reads raw logs and strips ANSI only in bounded display; original evidence bytes remain unchanged. A broad stage-reason grep initially produced excessive display output, so follow-through used a line scanner selecting exact stage_start/stage_end events and explicit error markers with bounded display. No runner/service/process action, local producer, duplicate workflow dispatch, fixture modification or weakened expectation occurred. Issue1902 bounded-contract completion is being independently assessed against its original requirements; parents1897/1433 and native1803 remain separate.
