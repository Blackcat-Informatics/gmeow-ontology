# Required behavior

Repair the failed exact-source producer build without deleting its live renderer
or weakening the compiler-read seal. Every selected workspace implementation must
still be read, all actual reads must have declared owners, generated inputs must
retain their correct package, and external immutable source roots must resolve
through the same metadata identity as selection. The raw/explicit ID discrepancy
is visible in production callers; the remote raw compiler-message transcript was
not uploaded and is not claimed as observed.

The synthetic acceptance invokes message collection, real dep-info matching and
verification; it does not produce a corpus or run CPU/environment fingerprinting.
The final proof is exact-source hosted optimized compilation plus downstream
gates. Local tests and normal hooks are necessary evidence, not a hosted pass.
This analysis is local; no independent reviewer was available.
