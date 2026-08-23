// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// BLAKE3 in the browser, because a content address must be CHECKABLE by the reader.
//
// Every content address this project ships — the `DIGESTS.blake3` manifests beside the
// WASM engine images, the digest recorded for the `gmeow.gts` bundle — is a BLAKE3 hash
// produced by the Rust `blake3` crate. `crypto.subtle` offers SHA-1/SHA-2 and nothing
// else, so a browser holding freshly fetched bytes has no way to recompute the address it
// was promised. Without this module a client can only TRUST a manifest; with it, a client
// can FALSIFY a fetch. That is the entire reason a hash function lives in JavaScript
// here.
//
// DOM-free and Node-importable: the acceptance lane (`tests/blake3.test.mjs`) hashes the
// committed multi-megabyte WASM images and compares against the digests the Rust crate
// wrote for them, so JS/Rust agreement is a test rather than an assumption.
//
// Hash mode only. Keyed hashing and key derivation are absent by construction — no caller
// here holds a key, and an unused key path is an untested key path.
//
// # The construction, in the terms this file uses
//
// Input splits into 1024-byte CHUNKS; each chunk is up to sixteen 64-byte BLOCKS. Every
// block goes through the same 16-word compression function, threading an 8-word chaining
// value (CV). A chunk's index rides in the compression as a 64-bit counter, so identical
// bytes at different offsets compress differently. Flags mark a chunk's first block
// (CHUNK_START), its last (CHUNK_END), an interior tree node (PARENT), and the single
// final compression of the whole input (ROOT).
//
// Chunk CVs merge pairwise up a binary tree. `_cvStack` holds the CVs of completed left
// subtrees, and the COUNT of finished chunks is itself the merge schedule: a count with k
// trailing zero bits closes k subtrees (the loop in `_pushChunkCv`). That is what makes
// the hasher streaming — at most 54 CVs are ever live, for any input size.
//
// Which compression carries ROOT is not known until the input ends: a one-chunk input
// roots at that chunk's final block, a longer input roots at the topmost parent merge.
// So the update loop NEVER sets ROOT, and it never compresses a chunk's last block —
// that block stays buffered until `digest()` can decide.

const BLOCK_LEN = 64;
const OUT_LEN = 32;

const CHUNK_START = 1 << 0;
const CHUNK_END = 1 << 1;
const PARENT = 1 << 2;
const ROOT = 1 << 3;

/** The SHA-256 initialization vector, reused by BLAKE3 as the unkeyed key words. */
const IV = Uint32Array.of(
  0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
  0x5be0cd19,
);

const MSG_PERMUTATION = Uint8Array.of(2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8);

// The flat 7×16 message schedule. Round 0 is the identity and every later round is the
// previous round with the permutation applied — DERIVED rather than transcribed, because
// a single mistyped index yields a hash that is self-consistent and wrong.
const MSG_SCHEDULE = new Uint8Array(7 * 16);
for (let i = 0; i < 16; i += 1) MSG_SCHEDULE[i] = i;
for (let round = 1; round < 7; round += 1) {
  for (let i = 0; i < 16; i += 1) {
    MSG_SCHEDULE[round * 16 + i] = MSG_SCHEDULE[(round - 1) * 16 + MSG_PERMUTATION[i]];
  }
}

/**
 * The BLAKE3 compression function — first eight output words only.
 *
 * Writes `state[0..8] ^ state[8..16]` into `out[outOff .. outOff + 8]`. The other eight
 * words of the full 16-word output feed only the extended (XOF) output stream, which a
 * 32-byte digest never reads; skipping them is also what makes writing IN PLACE
 * (`out === cv`) safe, since the discarded half is the only part that would still need
 * the original chaining value.
 *
 * `counterLow`/`counterHigh` are the halves of the 64-bit counter. It counts CHUNKS, not
 * bytes or blocks, and is zero for every parent node. `blockLen` is the block's TRUE
 * length; a short final block is zero-padded to 64 bytes and this parameter is what keeps
 * the padding from being ambiguous.
 *
 * All arithmetic stays in signed int32 (`| 0` after each add, `>>>`/`<<` pairs for the
 * rotations). Operands never exceed 2^33 before truncation, which doubles represent
 * exactly, so the wrap-around matches the reference u32 arithmetic.
 */
function compress(cv, cvOff, m, counterLow, counterHigh, blockLen, flags, out, outOff) {
  let v0 = cv[cvOff] | 0;
  let v1 = cv[cvOff + 1] | 0;
  let v2 = cv[cvOff + 2] | 0;
  let v3 = cv[cvOff + 3] | 0;
  let v4 = cv[cvOff + 4] | 0;
  let v5 = cv[cvOff + 5] | 0;
  let v6 = cv[cvOff + 6] | 0;
  let v7 = cv[cvOff + 7] | 0;
  let v8 = 0x6a09e667 | 0;
  let v9 = 0xbb67ae85 | 0;
  let v10 = 0x3c6ef372 | 0;
  let v11 = 0xa54ff53a | 0;
  let v12 = counterLow | 0;
  let v13 = counterHigh | 0;
  let v14 = blockLen | 0;
  let v15 = flags | 0;

  for (let round = 0; round < 7; round += 1) {
    const s = round * 16;

    // Column step: G(0,4,8,12) G(1,5,9,13) G(2,6,10,14) G(3,7,11,15)
    v0 = (v0 + v4 + m[MSG_SCHEDULE[s]]) | 0;
    v12 ^= v0;
    v12 = (v12 >>> 16) | (v12 << 16);
    v8 = (v8 + v12) | 0;
    v4 ^= v8;
    v4 = (v4 >>> 12) | (v4 << 20);
    v0 = (v0 + v4 + m[MSG_SCHEDULE[s + 1]]) | 0;
    v12 ^= v0;
    v12 = (v12 >>> 8) | (v12 << 24);
    v8 = (v8 + v12) | 0;
    v4 ^= v8;
    v4 = (v4 >>> 7) | (v4 << 25);

    v1 = (v1 + v5 + m[MSG_SCHEDULE[s + 2]]) | 0;
    v13 ^= v1;
    v13 = (v13 >>> 16) | (v13 << 16);
    v9 = (v9 + v13) | 0;
    v5 ^= v9;
    v5 = (v5 >>> 12) | (v5 << 20);
    v1 = (v1 + v5 + m[MSG_SCHEDULE[s + 3]]) | 0;
    v13 ^= v1;
    v13 = (v13 >>> 8) | (v13 << 24);
    v9 = (v9 + v13) | 0;
    v5 ^= v9;
    v5 = (v5 >>> 7) | (v5 << 25);

    v2 = (v2 + v6 + m[MSG_SCHEDULE[s + 4]]) | 0;
    v14 ^= v2;
    v14 = (v14 >>> 16) | (v14 << 16);
    v10 = (v10 + v14) | 0;
    v6 ^= v10;
    v6 = (v6 >>> 12) | (v6 << 20);
    v2 = (v2 + v6 + m[MSG_SCHEDULE[s + 5]]) | 0;
    v14 ^= v2;
    v14 = (v14 >>> 8) | (v14 << 24);
    v10 = (v10 + v14) | 0;
    v6 ^= v10;
    v6 = (v6 >>> 7) | (v6 << 25);

    v3 = (v3 + v7 + m[MSG_SCHEDULE[s + 6]]) | 0;
    v15 ^= v3;
    v15 = (v15 >>> 16) | (v15 << 16);
    v11 = (v11 + v15) | 0;
    v7 ^= v11;
    v7 = (v7 >>> 12) | (v7 << 20);
    v3 = (v3 + v7 + m[MSG_SCHEDULE[s + 7]]) | 0;
    v15 ^= v3;
    v15 = (v15 >>> 8) | (v15 << 24);
    v11 = (v11 + v15) | 0;
    v7 ^= v11;
    v7 = (v7 >>> 7) | (v7 << 25);

    // Diagonal step: G(0,5,10,15) G(1,6,11,12) G(2,7,8,13) G(3,4,9,14)
    v0 = (v0 + v5 + m[MSG_SCHEDULE[s + 8]]) | 0;
    v15 ^= v0;
    v15 = (v15 >>> 16) | (v15 << 16);
    v10 = (v10 + v15) | 0;
    v5 ^= v10;
    v5 = (v5 >>> 12) | (v5 << 20);
    v0 = (v0 + v5 + m[MSG_SCHEDULE[s + 9]]) | 0;
    v15 ^= v0;
    v15 = (v15 >>> 8) | (v15 << 24);
    v10 = (v10 + v15) | 0;
    v5 ^= v10;
    v5 = (v5 >>> 7) | (v5 << 25);

    v1 = (v1 + v6 + m[MSG_SCHEDULE[s + 10]]) | 0;
    v12 ^= v1;
    v12 = (v12 >>> 16) | (v12 << 16);
    v11 = (v11 + v12) | 0;
    v6 ^= v11;
    v6 = (v6 >>> 12) | (v6 << 20);
    v1 = (v1 + v6 + m[MSG_SCHEDULE[s + 11]]) | 0;
    v12 ^= v1;
    v12 = (v12 >>> 8) | (v12 << 24);
    v11 = (v11 + v12) | 0;
    v6 ^= v11;
    v6 = (v6 >>> 7) | (v6 << 25);

    v2 = (v2 + v7 + m[MSG_SCHEDULE[s + 12]]) | 0;
    v13 ^= v2;
    v13 = (v13 >>> 16) | (v13 << 16);
    v8 = (v8 + v13) | 0;
    v7 ^= v8;
    v7 = (v7 >>> 12) | (v7 << 20);
    v2 = (v2 + v7 + m[MSG_SCHEDULE[s + 13]]) | 0;
    v13 ^= v2;
    v13 = (v13 >>> 8) | (v13 << 24);
    v8 = (v8 + v13) | 0;
    v7 ^= v8;
    v7 = (v7 >>> 7) | (v7 << 25);

    v3 = (v3 + v4 + m[MSG_SCHEDULE[s + 14]]) | 0;
    v14 ^= v3;
    v14 = (v14 >>> 16) | (v14 << 16);
    v9 = (v9 + v14) | 0;
    v4 ^= v9;
    v4 = (v4 >>> 12) | (v4 << 20);
    v3 = (v3 + v4 + m[MSG_SCHEDULE[s + 15]]) | 0;
    v14 ^= v3;
    v14 = (v14 >>> 8) | (v14 << 24);
    v9 = (v9 + v14) | 0;
    v4 ^= v9;
    v4 = (v4 >>> 7) | (v4 << 25);
  }

  out[outOff] = v0 ^ v8;
  out[outOff + 1] = v1 ^ v9;
  out[outOff + 2] = v2 ^ v10;
  out[outOff + 3] = v3 ^ v11;
  out[outOff + 4] = v4 ^ v12;
  out[outOff + 5] = v5 ^ v13;
  out[outOff + 6] = v6 ^ v14;
  out[outOff + 7] = v7 ^ v15;
}

const HEX = [];
for (let i = 0; i < 256; i += 1) HEX.push(i.toString(16).padStart(2, "0"));

/**
 * An incremental BLAKE3 hasher in hash mode.
 *
 * `update()` may be called with any number of arbitrarily sized slices — a digest depends
 * only on the concatenated bytes, never on how they were cut — which is what lets a caller
 * hash a streamed response without buffering the whole body. `digest()` is repeatable and
 * does not close the hasher: more input may follow it.
 */
export class Blake3Hasher {
  constructor() {
    // State of the chunk currently being absorbed.
    this._cv = new Uint32Array(8);
    this._cv.set(IV);
    this._chunkCounter = 0;
    this._block = new Uint8Array(BLOCK_LEN);
    this._blockView = new DataView(this._block.buffer);
    this._blockLen = 0;
    this._blocksCompressed = 0;

    // Left-subtree chaining values. 54 is the exact bound: one entry per bit of a chunk
    // counter that a 2^64-byte input could reach.
    this._cvStack = new Uint32Array(54 * 8);
    this._stackLen = 0;

    // Reused scratch, so a multi-megabyte update allocates nothing per block.
    this._m = new Uint32Array(16);
    this._scratchCv = new Uint32Array(8);
  }

  /**
   * Absorb bytes.
   *
   * @param {Uint8Array} bytes
   * @returns {Blake3Hasher} this, for chaining
   */
  update(bytes) {
    if (!(bytes instanceof Uint8Array)) {
      throw new TypeError("blake3: update() expects a Uint8Array");
    }
    const n = bytes.length;
    if (n === 0) return this;

    const view = new DataView(bytes.buffer, bytes.byteOffset, n);
    const m = this._m;
    let off = 0;

    while (off < n) {
      // The chunk holds a full 1024 bytes and more input follows, so it cannot be the
      // root: finalize it, fold it into the tree, and open the next chunk.
      if (this._blocksCompressed === 15 && this._blockLen === BLOCK_LEN) {
        this._closeChunk();
      }
      // The buffered block is full and more input follows, so it is not this chunk's last
      // block. Compressing it here is what keeps the final block buffered for digest().
      if (this._blockLen === BLOCK_LEN) {
        const flags = this._blocksCompressed === 0 ? CHUNK_START : 0;
        for (let i = 0; i < 16; i += 1) m[i] = this._blockView.getUint32(i * 4, true);
        compress(
          this._cv,
          0,
          m,
          this._chunkCounter >>> 0,
          (this._chunkCounter / 4294967296) >>> 0,
          BLOCK_LEN,
          flags,
          this._cv,
          0,
        );
        this._blocksCompressed += 1;
        this._blockLen = 0;
      }
      // Interior blocks stream straight out of the caller's buffer, with no per-block
      // copy. `> BLOCK_LEN` is strictly greater on purpose: a block that exactly exhausts
      // the input may still turn out to be the final block, and must be buffered instead.
      while (this._blockLen === 0 && this._blocksCompressed < 15 && n - off > BLOCK_LEN) {
        for (let i = 0; i < 16; i += 1) m[i] = view.getUint32(off + i * 4, true);
        compress(
          this._cv,
          0,
          m,
          this._chunkCounter >>> 0,
          (this._chunkCounter / 4294967296) >>> 0,
          BLOCK_LEN,
          this._blocksCompressed === 0 ? CHUNK_START : 0,
          this._cv,
          0,
        );
        this._blocksCompressed += 1;
        off += BLOCK_LEN;
      }
      if (off >= n) break;

      const take = Math.min(BLOCK_LEN - this._blockLen, n - off);
      this._block.set(bytes.subarray(off, off + take), this._blockLen);
      this._blockLen += take;
      off += take;
    }
    return this;
  }

  /**
   * The 32-byte digest of everything absorbed so far.
   *
   * @returns {Uint8Array} 32 bytes
   */
  digest() {
    const out = this._scratchCv;
    if (this._stackLen === 0) {
      // A single-chunk input has no tree: the root is this chunk's own final block.
      this._chunkOutput(ROOT, out, 0);
    } else {
      this._chunkOutput(0, out, 0);
      // Fold the right-hand spine down through the stack. Every merge but the last
      // produces a plain chaining value; the last one — the tree's actual root — is the
      // single compression in the whole hash that carries ROOT.
      for (let i = this._stackLen - 1; i > 0; i -= 1) {
        this._parent(this._cvStack, i * 8, out, 0, 0, out, 0);
      }
      this._parent(this._cvStack, 0, out, 0, ROOT, out, 0);
    }

    const bytes = new Uint8Array(OUT_LEN);
    const dv = new DataView(bytes.buffer);
    for (let i = 0; i < 8; i += 1) dv.setUint32(i * 4, out[i], true);
    return bytes;
  }

  /** The digest as 64 lowercase hex characters. */
  digestHex() {
    const bytes = this.digest();
    let hex = "";
    for (let i = 0; i < OUT_LEN; i += 1) hex += HEX[bytes[i]];
    return hex;
  }

  /**
   * Compress the buffered (possibly short) final block of the current chunk.
   *
   * `extraFlags` is ROOT when this chunk is the entire input, otherwise 0. The result is
   * the chunk's chaining value.
   */
  _chunkOutput(extraFlags, out, outOff) {
    // Zero the tail. Bytes past `_blockLen` are stale from an earlier block, and BLAKE3
    // defines the final block as zero-padded with the true length passed separately. The
    // fill touches only stale bytes, so digest() stays repeatable and non-destructive.
    this._block.fill(0, this._blockLen);
    const m = this._m;
    for (let i = 0; i < 16; i += 1) m[i] = this._blockView.getUint32(i * 4, true);
    const flags =
      (this._blocksCompressed === 0 ? CHUNK_START : 0) | CHUNK_END | extraFlags;
    compress(
      this._cv,
      0,
      m,
      this._chunkCounter >>> 0,
      (this._chunkCounter / 4294967296) >>> 0,
      this._blockLen,
      flags,
      out,
      outOff,
    );
  }

  /** Merge two child chaining values into a parent node. */
  _parent(left, leftOff, right, rightOff, extraFlags, out, outOff) {
    const m = this._m;
    for (let i = 0; i < 8; i += 1) m[i] = left[leftOff + i];
    for (let i = 0; i < 8; i += 1) m[i + 8] = right[rightOff + i];
    // A parent's message is the two CVs as words — no byte reordering, because they were
    // never serialized. Its counter is always zero and its length always a full block.
    compress(IV, 0, m, 0, 0, BLOCK_LEN, PARENT | extraFlags, out, outOff);
  }

  /** Finalize the current chunk, fold it into the tree, and reset for the next one. */
  _closeChunk() {
    const cv = this._scratchCv;
    this._chunkOutput(0, cv, 0);
    const total = this._chunkCounter + 1;

    // A completed chunk closes as many subtrees as the count of completed chunks has
    // trailing zero bits: the count IS the shape of the left spine. (`& 1` reads the low
    // bit correctly even past 2^32, where `&` truncates the high bits it does not use.)
    let remaining = total;
    while ((remaining & 1) === 0) {
      this._stackLen -= 1;
      this._parent(this._cvStack, this._stackLen * 8, cv, 0, 0, cv, 0);
      remaining = Math.floor(remaining / 2);
    }
    this._cvStack.set(cv, this._stackLen * 8);
    this._stackLen += 1;

    this._cv.set(IV);
    this._chunkCounter = total;
    this._blockLen = 0;
    this._blocksCompressed = 0;
  }
}

/**
 * The BLAKE3 hash of `bytes`, as 64 lowercase hex characters.
 *
 * This is the exact digest the Rust `blake3` crate writes into a `DIGESTS.blake3`
 * manifest, so the two are directly comparable.
 *
 * @param {Uint8Array} bytes
 * @returns {string}
 */
export function blake3Hex(bytes) {
  return new Blake3Hasher().update(bytes).digestHex();
}
