# Issue #2934: Bind prover output operations to validated filesystem identities

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: bug, cross-cutting, quality, logic, rust, complexity-hard

## Body

Part of #2781. Blocked by: nothing. Blocks: #2781.

## Problem

`gmeow prove` validates output paths and file identities before later reopening raw paths for writes or stale-output removal. A switched output-parent symlink can redirect a later destructive operation onto a selected input. Static aliases, hardlinks and terminal symlinks are already checked; the remaining source-backed gap is between validation and use.

Input discovery and validation also precede a separate directory enumeration and raw-path loading. The protected identities must therefore belong to the actual opened semantic and replay inputs, rather than an earlier path observation. The `PROOF_NOT_RECORDED` failure currently leaves an existing selected proof output untouched.

The parent-redirection gap is verified by source inspection. No deterministic static-alias bypass or live source modification has been demonstrated.

## Required behavior

- Bind output publication and stale-output removal to the validated filesystem objects and parent locations through the actual destructive operation.
- Discover selected semantic inputs once and read their exact bytes through the opened files whose identities are protected. Retain source media/base/digest receipts and the proof-replay input identity.
- Protect all loaded semantic inputs and proof-replay inputs from overwrite, truncation or redirected removal.
- Reuse existing repository filesystem primitives where suitable. Preserve typed, actionable path/alias and I/O failures and the current exit-code distinction.
- Cover JSON/text reports, evidence, proof and FOF sinks, including required-output refusal and stale-output cleanup. Remove stale selected proof bytes when no proof is recorded. Do not weaken the existing static alias checks.
- Pin admitted parent directories through publication and removal. Request paths remain reporting coordinates after admission; re-resolving a switched parent cannot redirect use. The boundary covers parent redirection, without claiming protection against arbitrary adversarial replacement inside the pinned directory.

## Acceptance

- In isolated fixtures, changing the output-parent symlink between validation and a sink/removal operation cannot modify or remove a protected source.
- Existing hardlink, equivalent path, symlink-parent, dangling-terminal-link and conflicting-sink controls remain valid.
- Correct distinct outputs still publish successfully; failures leave the actual loaded semantic and replay input bytes intact and report the actual failure.
- A selected proof output containing old bytes cannot survive a no-proof required-output failure as an apparently current proof.
- Focused filesystem boundary and real CLI checks verify the final shared publication/removal path. The CLI documentation describes its safety contract.

Principles 4 and 7.


## Comments (0)

