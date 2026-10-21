# Failure evidence qualification

Source review confirms the existing production parity conditional returns StageFailed before parse_into_graph or StageOutput construction. The new formatter receives only canonical strings already computed by that boundary; it neither invokes producers nor reads or writes corpus. The exact fixture comparison remains unchanged. No fixture bytes were edited.

Two tiny independently authored string tests cover complete Unicode payload/byte/quad framing and deterministic textual differences, plus empty-actual framing. Formatting passed. make nextest-synthetic selected package(gmeow-pipeline) & test(parity_failure_): two tests executed, both passed, 1084 outside-selection skipped. make clippy-prebuild passed with strict workspace/all-target warnings-as-errors; the four embedded consumers remain excluded. Hosted actual ONNX generation, fixture equivalence, authenticated consumers, installed surfaces and full gate remain unrun under the campaign exception.

Review risks: hosted external log retention may truncate despite the local reporter retaining messages. Actual byte/quad framing detects incomplete extraction. Canonical blank-node labels may make textual differences noisy. A later hosted run must be inspected before fixture changes; no parity success is claimed here.

Independent read-only reporter assessment: current generation uses the non-TTY text reporter, not JSONL. StageFailed/Diag display, dev_sync failure lowering and HumanReporter preserve literal newlines without graph indentation or source truncation; severity/code prefixes occur only on the first message line. Make invokes the producer directly without a filtering pipe. Extract exactly the announced UTF-8 payload bytes after BEGIN, verify the END marker and quad count, and remove only any runner timestamp prefix. Preserve RDF escape sequences and Unicode bytes. External hosted retention remains unverified.

After fast-forwarding1907, actual combined helpers-hygiene and source-only producer-recipe both passed. Prior two formatter tests, strict prebuild Clippy and fmt remain applicable under the disjoint path/interface assessment in integration.md.
