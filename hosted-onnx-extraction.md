# Hosted ONNX failed-graph recovery

Run39691822332, exactdf283faeac2d0dbb20da8b3d26be1ad36d7994da, generation-a job119552503454 completed FAILURE at stage-math-producers after stage-reason completed. Full raw job output is hosted-pr1937-generation-a.log. The original catalog-carrier failure was passed on this descendant; full production acceptance still failed at ONNX parity.

Exact catalog-consumer evidence: stage-export-research-objects starts at raw log66340 and ends successfully at66344, reported8480ms (2026-10-21T23:54:58.6583252Z). This proves that producer stage completed on this run, not the whole generated/installed contract.

The diagnostic announces expected canonical graph64052bytes/275quads and actual68936bytes/294quads. Root found exactly one complete BEGIN ACTUAL CANONICAL N-QUADS / END frame, stripped only each GitHub runner timestamp prefix, preserved payload bytes and line endings, verified strict UTF-8 and exact announced68936bytes,294 canonical quad lines, each ending in the canonical terminator. Extracted hosted-onnx-actual.nq is exact recovered output, not a locally generated corpus or a blessed fixture.

Textual difference sections announce85expected-only/104actual-only lines; these are not yet a semantic disposition. Source-grounded complete semantic review assigned read-only to audit_gmn. Existing fixture remains untouched. Other hosted fixture-prefix and generation-b jobs were still running at retrieval. No duplicate dispatch or local producer was started.
