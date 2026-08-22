// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The single-threaded contract, read off the SHIPPED WASM BYTES.
//
// Two claims, and both are about the module rather than about the runtime that happens to
// load it:
//
//   1. every declared and every imported memory is `shared=false`;
//   2. the module contains NO atomic instruction (the `0xFE` prefix family).
//
// (2) cannot be answered by scanning for the byte `0xFE`: a WebAssembly code section is a
// dense instruction stream in which LEB128 operands, `f64.const` payloads and index
// immediates take every byte value, and both shipped images contain thousands of `0xFE`
// bytes that are not opcodes. So the code section is DECODED, instruction by instruction,
// and the decode is self-checking: each function body's decoder must land exactly on the
// body's declared end. A desynchronized decoder therefore FAILS rather than reporting a
// comfortable zero.
//
// The decoder is deliberately total in the no-optionality sense: an opcode it does not
// model is a hard error naming the byte and the offset, never a guessed immediate width.
// The shipped images are built with a declared feature set (`wasm-opt -Oz
// --enable-bulk-memory --enable-bulk-memory-opt --enable-nontrapping-float-to-int`, which
// hard-fails on anything outside it), so an opcode outside that set appearing here is news.

/** Read an unsigned LEB128 at `at`. Returns `[value, next]`. */
function u32(bytes, at) {
  let result = 0;
  let shift = 0;
  let index = at;
  for (;;) {
    const byte = bytes[index];
    if (byte === undefined) throw new Error(`truncated LEB128 at ${at}`);
    index += 1;
    result += (byte & 0x7f) * 2 ** shift;
    if ((byte & 0x80) === 0) return [result, index];
    shift += 7;
    if (shift > 63) throw new Error(`LEB128 at ${at} is longer than 64 bits`);
  }
}

/** Skip a signed LEB128 at `at`. Returns the next offset. */
function skipSigned(bytes, at) {
  let index = at;
  for (;;) {
    const byte = bytes[index];
    if (byte === undefined) throw new Error(`truncated signed LEB128 at ${at}`);
    index += 1;
    if ((byte & 0x80) === 0) return index;
  }
}

/** The module's sections, in order, as `{ id, start, end }` byte ranges over the payload. */
export function sections(bytes) {
  if (bytes.length < 8 || bytes[0] !== 0x00 || bytes[1] !== 0x61 || bytes[2] !== 0x73 || bytes[3] !== 0x6d) {
    throw new Error("not a WebAssembly module (bad magic)");
  }
  const out = [];
  let index = 8;
  while (index < bytes.length) {
    const id = bytes[index];
    index += 1;
    const [length, payload] = u32(bytes, index);
    out.push({ id, start: payload, end: payload + length });
    index = payload + length;
  }
  if (index !== bytes.length) throw new Error("the section table overruns the module");
  return out;
}

/** Read one `limits` at `at`. Returns `[{ shared, min, max }, next]`. */
function limits(bytes, at) {
  const flags = bytes[at];
  let index = at + 1;
  const [min, afterMin] = u32(bytes, index);
  index = afterMin;
  let max = null;
  if ((flags & 0x01) !== 0) {
    const [value, afterMax] = u32(bytes, index);
    max = value;
    index = afterMax;
  }
  return [{ shared: (flags & 0x02) !== 0, min, max }, index];
}

/** Skip one `name` (a length-prefixed UTF-8 byte string). */
function skipName(bytes, at) {
  const [length, next] = u32(bytes, at);
  return next + length;
}

/**
 * Every memory the module DECLARES or IMPORTS, with its `shared` flag.
 *
 * Both halves matter: a module can declare no memory of its own and import a shared one,
 * which is exactly the shape a threaded build has.
 */
export function memories(bytes) {
  const found = [];
  for (const section of sections(bytes)) {
    if (section.id === 5) {
      let [count, index] = u32(bytes, section.start);
      for (let n = 0; n < count; n += 1) {
        const [limit, next] = limits(bytes, index);
        found.push({ ...limit, kind: "declared" });
        index = next;
      }
    }
    if (section.id === 2) {
      let [count, index] = u32(bytes, section.start);
      for (let n = 0; n < count; n += 1) {
        index = skipName(bytes, index);
        index = skipName(bytes, index);
        const kind = bytes[index];
        index += 1;
        if (kind === 0x00) {
          [, index] = u32(bytes, index); // typeidx
        } else if (kind === 0x01) {
          index += 1; // reftype
          [, index] = limits(bytes, index);
        } else if (kind === 0x02) {
          const [limit, next] = limits(bytes, index);
          found.push({ ...limit, kind: "imported" });
          index = next;
        } else if (kind === 0x03) {
          index += 2; // valtype + mutability
        } else {
          throw new Error(`unmodelled import kind 0x${kind.toString(16)} at ${index - 1}`);
        }
      }
    }
  }
  return found;
}

/** Skip a `blocktype`: the empty type, one `valtype`, or a signed type index. */
function skipBlockType(bytes, at) {
  const byte = bytes[at];
  if (byte === 0x40) return at + 1;
  if ([0x7f, 0x7e, 0x7d, 0x7c, 0x7b, 0x70, 0x6f].includes(byte)) return at + 1;
  return skipSigned(bytes, at);
}

/** Skip a `memarg` (align + offset). Rejects the multi-memory align bit, which is not enabled. */
function skipMemarg(bytes, at) {
  const [align, afterAlign] = u32(bytes, at);
  if ((align & 0x40) !== 0) {
    throw new Error(`memarg at ${at} carries the multi-memory bit, which this build does not enable`);
  }
  const [, afterOffset] = u32(bytes, afterAlign);
  return afterOffset;
}

/**
 * Decode one instruction at `at`.
 *
 * @returns `{ next, atomic }` — `atomic` is true iff the instruction is `0xFE`-prefixed.
 * @throws on any opcode this decoder does not model, naming the byte and the offset.
 */
function instruction(bytes, at) {
  const op = bytes[at];
  let index = at + 1;
  // Instructions with no immediates: the control terminators, the parametric ops, and the
  // whole numeric/comparison/conversion block including the sign-extension opcodes.
  if (
    op === 0x00 ||
    op === 0x01 ||
    op === 0x05 ||
    op === 0x0b ||
    op === 0x0f ||
    op === 0x1a ||
    op === 0x1b ||
    op === 0xd1 ||
    (op >= 0x45 && op <= 0xc4)
  ) {
    return { next: index, atomic: false };
  }
  // Every plain load and store carries one `memarg`.
  if (op >= 0x28 && op <= 0x3e) {
    return { next: skipMemarg(bytes, index), atomic: false };
  }
  switch (op) {
    case 0x02: // block
    case 0x03: // loop
    case 0x04: // if
      return { next: skipBlockType(bytes, index), atomic: false };
    case 0x0c: // br
    case 0x0d: // br_if
    case 0x10: // call
    case 0x12: // return_call
    case 0x20: // local.get
    case 0x21: // local.set
    case 0x22: // local.tee
    case 0x23: // global.get
    case 0x24: // global.set
    case 0x25: // table.get
    case 0x26: // table.set
    case 0xd2: // ref.func
      [, index] = u32(bytes, index);
      return { next: index, atomic: false };
    case 0x0e: {
      // br_table: a vector of label indices plus the default.
      let count;
      [count, index] = u32(bytes, index);
      for (let n = 0; n <= count; n += 1) [, index] = u32(bytes, index);
      return { next: index, atomic: false };
    }
    case 0x11: // call_indirect
    case 0x13: // return_call_indirect
      [, index] = u32(bytes, index);
      [, index] = u32(bytes, index);
      return { next: index, atomic: false };
    case 0x1c: {
      // select with an explicit result-type vector.
      let count;
      [count, index] = u32(bytes, index);
      return { next: index + count, atomic: false };
    }
    case 0x3f: // memory.size
    case 0x40: // memory.grow
      return { next: index + 1, atomic: false };
    case 0x41: // i32.const
    case 0x42: // i64.const
      return { next: skipSigned(bytes, index), atomic: false };
    case 0x43: // f32.const
      return { next: index + 4, atomic: false };
    case 0x44: // f64.const
      return { next: index + 8, atomic: false };
    case 0xd0: // ref.null
      return { next: index + 1, atomic: false };
    case 0xfc: {
      let sub;
      [sub, index] = u32(bytes, index);
      if (sub <= 7) return { next: index, atomic: false }; // trunc_sat family
      switch (sub) {
        case 8: // memory.init
          [, index] = u32(bytes, index);
          return { next: index + 1, atomic: false };
        case 9: // data.drop
        case 13: // elem.drop
        case 15: // table.grow
        case 16: // table.size
        case 17: // table.fill
          [, index] = u32(bytes, index);
          return { next: index, atomic: false };
        case 10: // memory.copy
          return { next: index + 2, atomic: false };
        case 11: // memory.fill
          return { next: index + 1, atomic: false };
        case 12: // table.init
        case 14: // table.copy
          [, index] = u32(bytes, index);
          [, index] = u32(bytes, index);
          return { next: index, atomic: false };
        default:
          throw new Error(`unmodelled 0xFC sub-opcode ${sub} at ${at}`);
      }
    }
    case 0xfe: {
      // The atomics family. The immediate width does not matter: finding one at all is the
      // failure, so the decode stops here and the caller reports it.
      const [sub] = u32(bytes, index);
      return { next: index, atomic: true, sub };
    }
    default:
      throw new Error(
        `unmodelled opcode 0x${op.toString(16).padStart(2, "0")} at ${at} — this decoder refuses ` +
          "to guess an immediate width; the shipped images are built against a declared feature set",
      );
  }
}

/**
 * Every atomic instruction in the module's code section, as `{ function, offset, sub }`.
 *
 * The empty array is a PROVEN absence: every function body is decoded to its declared end,
 * and a body whose decode does not land exactly there raises rather than returning.
 */
export function atomicInstructions(bytes) {
  const code = sections(bytes).find((section) => section.id === 10);
  if (code === undefined) throw new Error("the module carries no code section");
  const found = [];
  let [count, index] = u32(bytes, code.start);
  for (let fn = 0; fn < count; fn += 1) {
    let size;
    [size, index] = u32(bytes, index);
    const bodyEnd = index + size;
    let localGroups;
    [localGroups, index] = u32(bytes, index);
    for (let group = 0; group < localGroups; group += 1) {
      [, index] = u32(bytes, index);
      index += 1; // valtype
    }
    while (index < bodyEnd) {
      const step = instruction(bytes, index);
      if (step.atomic) found.push({ function: fn, offset: index, sub: step.sub });
      if (step.next <= index) throw new Error(`the decoder made no progress at ${index}`);
      index = step.next;
    }
    if (index !== bodyEnd) {
      throw new Error(
        `function body ${fn} decoded to ${index} but its declared end is ${bodyEnd} — the ` +
          "instruction decoder desynchronized, so its atomic-opcode verdict is not trustworthy",
      );
    }
  }
  if (index !== code.end) {
    throw new Error(`the code section decoded to ${index} but ends at ${code.end}`);
  }
  return found;
}
