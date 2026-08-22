<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Design: GTS Signature/Trust Verification in `gmeow-validate`

## Decision Summary

| Item | Choice |
|------|--------|
| Integration option | **Option B** — separate `gmeow_gts::verify` phase after parsing |
| GTS bundle parsing | Keep `purrdf::gts::read_all_segments` unchanged |
| Signature verification | `gmeow_gts::verify::verify_file_with_options` |
| Trust policy | `gmeow_gts::policy::TrustPolicy` built from policy TOML |
| Key resolution | `--trusted-key` optional; omitted → embedded `gts:transportKey` |
| Short-circuit | Error-level signature/trust findings abort before ontology validation; warnings continue |
| Consumer relation | Complement to `gmeow verify`; `gmeow-dev validate --gts` adds a policy-aware pre-validation gate |

## Chosen Integration Option: Option B (Separate Verify Phase)

The approved plan considered two integration options:

- **Option A**: Modify the parse path so `purrdf::gts::read_all_segments` accepts a key resolver and verifies signatures during the initial fold.
- **Option B**: Keep the existing parse path unchanged and add a separate verification phase after the GTS bytes have been folded into a `gmeow_gts::model::Graph`.

**Rationale for choosing Option B:**

1. The installed `gmeow-gts` 0.9.2 API does **not** expose signature verification through `gmeow_gts::reader::read` or `read_with_options`. The reader records signature frames but leaves them as `"unverified"` unless an optional *content* key is supplied for decrypting `COSE_Encrypt0` payloads. Cryptographic signature verification is exposed as a separate high-level API in `gmeow_gts::verify`.
2. Using the existing `verify_file_with_options` helper matches the issue intent (“call `gmeow_gts::reader::read` with signature verification enabled”) with the API that actually exists, without hand-rolling COSE/OpenPGP logic inside `gmeow-validate`.
3. Keeping `purrdf` and `gmeow-validate`'s store construction untouched minimizes risk to existing Turtle/GTS validation phases and preserves the content-addressed cache keys derived from `segment_heads`.
4. The performance cost is acceptable for a validation gate: the bundle is read once for folding and again inside `verify_file_with_options`, which is a small, deterministic replay of a CBOR sequence.

## Confirmed `gmeow-gts` API Surface

The crate version used by the workspace is declared in `crates/validate/Cargo.toml` and the `purrdf` pin in the root `Cargo.toml`. The following symbols are confirmed in the installed registry source:

- `gmeow_gts::reader::read(data, allow_segments, expected_head) -> Graph`
- `gmeow_gts::reader::read_with_options(data, ReadOptions) -> Graph`
- `gmeow_gts::reader::ReadOptions { allow_segments, expected_head, content_key }`
- `gmeow_gts::verify::verify_file(data) -> VerificationResult`
- `gmeow_gts::verify::verify_file_with_options(data, &VerifyOptions) -> VerificationResult`
- `gmeow_gts::verify::VerifyOptions { armored_key, require_signatures, trust_policy }`
- `gmeow_gts::verify::VerificationResult { ok, kid, fingerprint, frames, signed, valid, trusted, invalid, unverified, errors, diagnostics, profile_findings, ... }`
- `gmeow_gts::verify::extract_transport_key(graph) -> Option<EmbeddedTransportKey>`
- `gmeow_gts::TrustPolicy { trusted_signers, require_trusted_signer, pseudonymous_kid_pattern }`
- `gmeow_gts::signature_trust(graph, Option<&TrustPolicy>) -> Vec<SignatureTrust>`
- `gmeow_gts::evaluate_profile_policy(graph, Option<&TrustPolicy>, segment_index) -> Vec<ProfileFinding>`
- `gmeow_gts::policy::{ProfileFinding, Severity, SignatureTrust}`

Notably, there is **no public `crypto` module** with `KeyProvider`/`InMemoryKeys` providers. Key resolution is performed internally by `verify_file_with_options` from either an armored OpenPGP key or the bundle's embedded `gts:transportKey`.

## Key-Resolution Rules

- `--trusted-key <path>` is **optional**.
- When `--trusted-key` is **omitted**, the verification phase calls `verify_file_with_options` with `armored_key: None`. The helper reads the bundle once to extract the embedded `gts:transportKey` metadata and uses that OpenPGP Ed25519 certificate for COSE_Sign1 verification.
- When `--trusted-key` is **provided**, the file contents are read as an ASCII-armored OpenPGP public key and passed to `verify_file_with_options` as `armored_key: Some(...)`. The embedded transport key is ignored for verification. This mirrors the existing `gmeow verify --trusted-key` behavior.
- In both cases, the resolved `kid` and fingerprint are surfaced in the validation report for transparency.

## Short-Circuit Rules

The signature/trust phase runs early in `ValidationRun::run`, immediately after the GTS bytes are folded into a graph and before any ontology validation phases.

Findings are mapped to `gmeow_diagnostics::Finding` with the following severity rules:

| Condition | Severity | Abort? |
|-----------|----------|--------|
| No signatures present and `require_signatures == true` | Error | Yes |
| One or more signatures cryptographically invalid | Error | Yes |
| One or more signatures unresolved (no key) | Error | Yes |
| `require_trusted_signer == true` and no trusted signer | Error | Yes |
| Signatures present but no trust policy supplied | Warning | No |
| Signatures present but signer not in `trusted_signers` (when `require_trusted_signer == false`) | Warning | No |
| Reader diagnostics from the verification fold | Warning/Error based on diagnostic code | Error-level aborts |

When an aborting finding is emitted, `ValidationRun::run` returns early with an empty ontology store/shapes and a report containing only the signature/trust findings. This prevents malformed, unsigned, or untrusted bundles from being passed to SHACL/reasoning phases.

## TOML Policy Schema

`gmeow-dev validate` accepts `--trust-policy path/to/policy.toml`. The file is a flat TOML document. The trust fields (`trusted_signers`, `require_trusted_signer`) map onto `gmeow_gts::policy::TrustPolicy`; `trusted_key` is a separate key-resolution option used to supply an out-of-band ASCII-armored OpenPGP public key:

```toml
# Optional: list of signer KIDs considered trusted by this deployment.
trusted_signers = [
  "0123456789ABCDEF",
  "release@blackcatinformatics.ca",
]

# Optional: require at least one cryptographically valid signature from a
# trusted signer. Defaults to false.
require_trusted_signer = true

# Optional: path to an out-of-band ASCII-armored OpenPGP public key.
# If omitted, the bundle's embedded gts:transportKey is used.
# The path is resolved relative to the policy file's directory.
trusted_key = "keys/gmeow-release-key.asc"
```

`require_signatures` is controlled by the CLI flag `--require-signed` rather than the policy file, so a bundle can be validated with a trust policy but without requiring signatures to be present.

## Relation to `gmeow verify`

- `gmeow verify` is the standalone, consumer-facing verification tool. It checks the bundle's transport key, signatures, and optional trust policy and prints a human-readable result.
- `gmeow-dev validate --gts` is the repository maintenance tool. The new Rust gate adds a *policy-aware* verification step *before* ontology validation, ensuring that CI and local validation runs only ingest signed/trusted release bundles when configured to do so.
- The two commands are complementary: `gmeow verify` answers “is this bundle authentic?”; `gmeow-dev validate --gts` answers “does this bundle satisfy the project's release policy and is the ontology valid?”.

## API Assumptions and Fallback

The design assumes `gmeow-gts` 0.9.2's `verify_file_with_options` continues to:

1. Accept an optional `armored_key` and a `TrustPolicy`.
2. Return a `VerificationResult` with counts and error strings.
3. Treat invalid/unverified signatures and missing signatures (when `require_signatures` is true) as non-`ok` outcomes.
4. Evaluate `require_trusted_signer` through `evaluate_profile_policy` and surface `ProfileFinding` errors.

If a future `gmeow-gts` release changes this surface, the fallback is to drop back to the lower-level building blocks still present in every version: fold with `read`, call `extract_transport_key` to resolve the embedded key (or parse an armored key via `gmeow_gts::openpgp::parse_transport_key` if made public), invoke `gmeow_gts::cose::verify_signatures` directly, and then call `signature_trust`/`evaluate_profile_policy` manually. Because 0.9.2 already exposes the high-level helper, the implementation should use it and avoid this fallback path unless necessary.
