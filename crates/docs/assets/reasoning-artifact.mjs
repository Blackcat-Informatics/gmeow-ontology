// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/** Decode the shared envelope and expose facets only after native admission. */
export function admitReasoningArtifact(envelope, admit) {
  if (
    envelope?.format !== "gts" ||
    envelope.media_type !== "application/vnd.blackcat.gts+cbor-seq" ||
    envelope.encoding !== "base64" ||
    !Number.isSafeInteger(envelope.bytes) ||
    envelope.bytes <= 0 ||
    typeof envelope.output !== "string"
  ) {
    throw new Error("reasoning response requires the complete binary GTS artifact envelope");
  }
  const decoded = atob(envelope.output);
  if (decoded.length !== envelope.bytes || btoa(decoded) !== envelope.output) {
    throw new Error("reasoning artifact has a noncanonical encoding or incorrect byte length");
  }
  const bytes = Uint8Array.from(decoded, (character) => character.charCodeAt(0));
  const admitted = JSON.parse(admit(bytes));
  if (admitted.artifact?.output !== envelope.output) {
    throw new Error("native admission returned a different reasoning artifact");
  }
  return admitted;
}
