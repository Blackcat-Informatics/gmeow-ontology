# Plan review — self-review

No independent local review dispatcher is exposed by the current tool inventory. This is a local assessment, not independent approval. The plan preserves every original diagnostic requirement, distinguishes existing merged implementation from the new cause-report defect, and keeps consumer runtime qualification explicit.

The smallest coherent repair is to delete diagnostic_snapshot and reuse ReportDiagnostic::from_diag, matching the real reasoning-input adapter. The original live Diag is retained in PreservedDiagnostic, while its wire projection retains all shared report fields. No dependency, profile, corpus source, compatibility path or error substrate changes. The source regression exercises a real native parser refusal through the actual prover adapter and ProveReport.

Concern: current embedded consumer assets are unavailable. Publishing an honestly pending PR is authorized by the user instruction to push and merge work with CI findings handled visibly; it is not proof of issue completion. Current backend tests can qualify unchanged numeric/model paths but cannot stand in for the new consumer regression. No scope cut or full-gate pass is approved by this self-review.
