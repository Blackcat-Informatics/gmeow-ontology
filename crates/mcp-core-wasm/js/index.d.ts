// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/** The routing signal a deferred tool raises, read structurally out of a response frame. */
export interface SegmentDeferral {
  /** The tool the caller asked for. */
  tool: string;
  /** The segment identifier that serves it. */
  segment: string;
  /** Every tool that segment serves. */
  segmentTools: string[];
}

/** The progress event `tieredMcp` emits either side of a segment fetch. */
export interface SegmentLoadEvent extends SegmentDeferral {
  phase: "loading" | "loaded";
}

/** The lifecycle a demand-loaded segment module must expose. */
export interface SegmentModule {
  ready(wasmBytesOrUrl?: BufferSource | URL | string): Promise<void>;
  init(snapshot: Uint8Array): void;
  mcp(request_json: string): string;
}

/** The options `tieredMcp` takes. */
export interface TieredOptions {
  /** Resolve a segment name to its module. Called at most once per segment. */
  loadSegment?: (segment: string) => Promise<SegmentModule>;
  /** Optional progress hook — the seam a UI uses to show a segment being fetched. */
  onSegmentLoad?: (event: SegmentLoadEvent) => void;
}

/** The stable diagnostic code the engine raises for a tool whose segment is not resident. */
export const SEGMENT_NOT_LOADED: "mcp.segment-not-loaded";

/** The segment identifier that serves this image's deferred tools. */
export function deferredSegment(): string;

/** The tool names this image defers, read from the engine's own constants. */
export function deferredTools(): string[];

/**
 * Install a `gmeow.gts` snapshot into the LEAN core engine. Must be called once (after
 * `ready()`) before any frame is dispatched. Use `initTiered` instead when frames will be
 * dispatched through `tieredMcp`. Throws if the snapshot cannot be read.
 */
export function init(snapshot: Uint8Array): void;

/**
 * Install a `gmeow.gts` snapshot AND retain it for demand-loaded segments, so a segment
 * loaded later is initialised over the SAME bundle. Resets the segment cache and starts a
 * new SESSION: a `tieredMcp` frame still in flight from the previous snapshot is rejected
 * rather than answered from this one.
 */
export function initTiered(snapshot: Uint8Array): void;

/**
 * Whether a `gmeow.gts` snapshot has been installed. This is the wasm module's OWN
 * `ready()` export, renamed at this wrapper so `ready()` can keep the sibling shims'
 * instantiation meaning.
 */
export function loaded(): boolean;

/**
 * Dispatch ONE JSON-RPC MCP request frame against the LEAN core only. A tool served by a
 * deferred segment answers with the `mcp.segment-not-loaded` signal rather than a result;
 * use `tieredMcp` to make that deferral invisible. Throws if no snapshot is installed.
 */
export function mcp(request_json: string): string;

/**
 * Instantiate the wasm module. Idempotent and single-flighted: concurrent callers share
 * one instantiation. In Node the wasm bytes are read from the colocated file; in a
 * browser pass the bytes/URL, or omit to fetch the colocated `.wasm`. Must be awaited
 * once before any other export is used.
 */
export function ready(wasmBytesOrUrl?: BufferSource | URL | string): Promise<void>;

/**
 * Read the deferral signal out of a response frame, or `null` if the frame is anything
 * else (a real answer, an ordinary tool error, a protocol error, unparsable text).
 *
 * TOTAL over the envelope: the result must be an error envelope (`isError === true`) and
 * the payload must carry a non-empty string `tool`, a non-empty string `segment`, and an
 * array of string `segment_tools`. A payload that carries the code but not the fields a
 * host routes on is `null`, never a partially-populated deferral.
 */
export function segmentDeferral(responseFrame: string): SegmentDeferral | null;

/**
 * Dispatch ONE frame with demand loading: the core answers directly whenever it can, and
 * otherwise the segment serving the named tool is loaded once and the IDENTICAL frame
 * string replayed to it, so the caller observes a slower answer rather than a smaller one.
 *
 * Rejects — never resolves with a degraded answer — when `loadSegment` itself rejects (an
 * unreachable segment is a hard failure), and when `initTiered` started a new session while
 * this frame was in flight (the frame belongs to a bundle this module no longer serves).
 */
export function tieredMcp(requestFrame: string, options?: TieredOptions): Promise<string>;

/** The engine version (the crate's SemVer). */
export function version(): string;
