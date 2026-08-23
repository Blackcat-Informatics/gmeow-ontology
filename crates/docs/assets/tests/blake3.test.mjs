// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Proof that `assets/blake3.mjs` is BLAKE3, from two independent directions.
//
// 1. The OFFICIAL BLAKE3 reference test vectors, copied from `test_vectors.json` in the
//    BLAKE3-team/BLAKE3 repository. These are published constants of the algorithm, not
//    goldens blessed from this implementation's own output — nothing here may ever be
//    "re-blessed". Per that file's own preamble, the input of length N is the repeating
//    byte sequence `i % 251`, and each case's `hash` field is an EXTENDED output whose
//    first 32 bytes (64 hex characters) are the ordinary digest.
//
// 2. Cross-implementation agreement with the Rust `blake3` crate over bytes already
//    committed in this repository. `mcp/DIGESTS.blake3` and `mcp-core/DIGESTS.blake3`
//    were written by that crate over the shipped WASM images; re-hashing those files here
//    exercises thousands of chunks and a deep merge tree on real multi-megabyte input.
//    This is the load-bearing assertion, because it is exactly the job a browser does when
//    it verifies a fetched asset against a shipped manifest.

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { blake3Hex, Blake3Hasher } from "../blake3.mjs";

// From BLAKE3-team/BLAKE3 `test_vectors/test_vectors.json`, hash mode, truncated to the
// 32-byte digest prefix. `input_len` → expected digest.
const REFERENCE_VECTORS = [
  [0, "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"],
  [1, "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213"],
  [2, "7b7015bb92cf0b318037702a6cdd81dee41224f734684c2c122cd6359cb1ee63"],
  [3, "e1be4d7a8ab5560aa4199eea339849ba8e293d55ca0a81006726d184519e647f"],
  [4, "f30f5ab28fe047904037f77b6da4fea1e27241c5d132638d8bedce9d40494f32"],
  [5, "b40b44dfd97e7a84a996a91af8b85188c66c126940ba7aad2e7ae6b385402aa2"],
  [6, "06c4e8ffb6872fad96f9aaca5eee1553eb62aed0ad7198cef42e87f6a616c844"],
  [7, "3f8770f387faad08faa9d8414e9f449ac68e6ff0417f673f602a646a891419fe"],
  [8, "2351207d04fc16ade43ccab08600939c7c1fa70a5c0aaca76063d04c3228eaeb"],
  [63, "e9bc37a594daad83be9470df7f7b3798297c3d834ce80ba85d6e207627b7db7b"],
  [64, "4eed7141ea4a5cd4b788606bd23f46e212af9cacebacdc7d1f4c6dc7f2511b98"],
  [65, "de1e5fa0be70df6d2be8fffd0e99ceaa8eb6e8c93a63f2d8d1c30ecb6b263dee"],
  [127, "d81293fda863f008c09e92fc382a81f5a0b4a1251cba1634016a0f86a6bd640d"],
  [128, "f17e570564b26578c33bb7f44643f539624b05df1a76c81f30acd548c44b45ef"],
  [129, "683aaae9f3c5ba37eaaf072aed0f9e30bac0865137bae68b1fde4ca2aebdcb12"],
  [1023, "10108970eeda3eb932baac1428c7a2163b0e924c9a9e25b35bba72b28f70bd11"],
  [1024, "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7"],
  [1025, "d00278ae47eb27b34faecf67b4fe263f82d5412916c1ffd97c8cb7fb814b8444"],
  [2048, "e776b6028c7cd22a4d0ba182a8bf62205d2ef576467e838ed6f2529b85fba24a"],
  [2049, "5f4d72f40d7a5f82b15ca2b2e44b1de3c2ef86c426c95c1af0b6879522563030"],
  [3072, "b98cb0ff3623be03326b373de6b9095218513e64f1ee2edd2525c7ad1e5cffd2"],
  [3073, "7124b49501012f81cc7f11ca069ec9226cecb8a2c850cfe644e327d22d3e1cd3"],
  [4096, "015094013f57a5277b59d8475c0501042c0b642e531b0a1c8f58d2163229e969"],
  [4097, "9b4052b38f1c5fc8b1f9ff7ac7b27cd242487b3d890d15c96a1c25b8aa0fb995"],
  [5120, "9cadc15fed8b5d854562b26a9536d9707cadeda9b143978f319ab34230535833"],
  [5121, "628bd2cb2004694adaab7bbd778a25df25c47b9d4155a55f8fbd79f2fe154cff"],
  [6144, "3e2e5b74e048f3add6d21faab3f83aa44d3b2278afb83b80b3c35164ebeca205"],
  [6145, "f1323a8631446cc50536a9f705ee5cb619424d46887f3c376c695b70e0f0507f"],
  [7168, "61da957ec2499a95d6b8023e2b0e604ec7f6b50e80a9678b89d2628e99ada77a"],
  [7169, "a003fc7a51754a9b3c7fae0367ab3d782dccf28855a03d435f8cfe74605e7817"],
  [8192, "aae792484c8efe4f19e2ca7d371d8c467ffb10748d8a5a1ae579948f718a2a63"],
  [8193, "bab6c09cb8ce8cf459261398d2e7aef35700bf488116ceb94a36d0f5f1b7bc3b"],
  [16384, "f875d6646de28985646f34ee13be9a576fd515f76b5b0a26bb324735041ddde4"],
  [31744, "62b6960e1a44bcc1eb1a611a8d6235b6b4b78f32e7abc4fb4c6cdcce94895c47"],
  [102400, "bc3e3d41a1146b069abffad3c0d44860cf664390afce4d9661f7902e7943e085"],
];

/** The reference input pattern: byte `i` of an N-byte input is `i % 251`. */
function referenceInput(length) {
  const bytes = new Uint8Array(length);
  for (let i = 0; i < length; i += 1) bytes[i] = i % 251;
  return bytes;
}

/** Parse a `<64 hex>  <relative path>` manifest as the Rust `blake3` crate writes it. */
function readDigestManifest(relativeDir) {
  const manifestUrl = new URL(`../${relativeDir}/DIGESTS.blake3`, import.meta.url);
  const text = readFileSync(fileURLToPath(manifestUrl), "utf8");
  const entries = [];
  for (const line of text.split("\n")) {
    if (line.trim() === "") continue;
    const match = /^([0-9a-f]{64})\s+(\S.*)$/.exec(line);
    assert.ok(match, `unparsable digest line in ${relativeDir}/DIGESTS.blake3: ${line}`);
    entries.push({
      expected: match[1],
      name: match[2],
      path: fileURLToPath(new URL(`../${relativeDir}/${match[2]}`, import.meta.url)),
    });
  }
  assert.ok(entries.length > 0, `${relativeDir}/DIGESTS.blake3 lists no files`);
  return entries;
}

test("official BLAKE3 reference test vectors (hash mode)", () => {
  for (const [length, expected] of REFERENCE_VECTORS) {
    assert.equal(blake3Hex(referenceInput(length)), expected, `input_len ${length}`);
  }
});

test("a digest depends on the bytes, not on how they were fed", () => {
  // Same published vectors, absorbed in ragged pieces. The split sizes deliberately
  // straddle the block (64) and chunk (1024) boundaries so the buffered path, the
  // zero-copy interior path and mid-chunk resumption are all exercised.
  const splits = [1, 63, 64, 65, 127, 1024, 1025, 3000];
  for (const [length, expected] of REFERENCE_VECTORS) {
    const input = referenceInput(length);
    for (const split of splits) {
      const hasher = new Blake3Hasher();
      for (let off = 0; off < length; off += split) {
        hasher.update(input.subarray(off, Math.min(off + split, length)));
      }
      assert.equal(hasher.digestHex(), expected, `input_len ${length}, split ${split}`);
    }
  }
});

test("digest() is repeatable and does not close the hasher", () => {
  const [, expected1024] = REFERENCE_VECTORS.find(([length]) => length === 1024);
  const [, expected1025] = REFERENCE_VECTORS.find(([length]) => length === 1025);
  const input = referenceInput(1025);

  const hasher = new Blake3Hasher().update(input.subarray(0, 1024));
  assert.equal(hasher.digestHex(), expected1024);
  assert.equal(hasher.digestHex(), expected1024, "repeated digest");
  hasher.update(input.subarray(1024));
  assert.equal(hasher.digestHex(), expected1025, "digest after resumed update");
});

for (const dir of ["mcp", "mcp-core"]) {
  test(`agrees with the Rust blake3 crate on ${dir}/DIGESTS.blake3`, (t) => {
    const entries = readDigestManifest(dir);
    let largest = { size: 0 };
    for (const entry of entries) {
      const bytes = readFileSync(entry.path);
      const started = performance.now();
      const actual = blake3Hex(new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.length));
      const elapsedMs = performance.now() - started;
      assert.equal(actual, entry.expected, `${dir}/${entry.name}`);
      t.diagnostic(
        `${dir}/${entry.name}: ${bytes.length} bytes in ${elapsedMs.toFixed(1)} ms ` +
          `(${(bytes.length / 1048576 / (elapsedMs / 1000)).toFixed(1)} MiB/s)`,
      );
      if (bytes.length > largest.size) largest = { size: bytes.length, name: entry.name };
    }
    // Guard the guard: this lane is only meaningful if it hashed a genuinely large file,
    // so a manifest that shrank to trivia fails instead of passing quietly.
    assert.ok(
      largest.size > 1048576,
      `${dir}/DIGESTS.blake3 covers no file over 1 MiB (largest: ${largest.name})`,
    );
  });
}
