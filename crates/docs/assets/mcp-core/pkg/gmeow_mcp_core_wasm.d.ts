/* tslint:disable */
/* eslint-disable */

/**
 * The identifier of the segment this image defers, as it appears in the
 * `mcp.segment-not-loaded` signal's `segment` field.
 *
 * A host maps this to the module it must load; exporting it means the mapping keys off
 * the engine's own constant rather than a string the JS layer hard-codes.
 */
export function deferred_segment(): string;

/**
 * The tool names this image DEFERS, as a JSON array of strings.
 *
 * The host reads this ONCE, at load, so it can decide to pre-fetch the reasoning segment
 * (say, because the user's first click is a proof) instead of discovering the need
 * mid-frame. It is the same list the deferral signal names, read off the engine's single
 * declaration ([`gmeow_mcp::REASONING_SEGMENT_TOOLS`]) rather than restated here — a
 * second copy in JS would be the exact drift this crate must not introduce.
 */
export function deferred_tools(): string;

/**
 * Load a `gmeow.gts` snapshot and build the LEAN core MCP engine over it.
 *
 * `snapshot` is the raw bundle bytes — the identical artifact the native `gmeow mcp`
 * embeds, the docs site serves, and the full browser segment loads. The bytes are parsed
 * to the carrier dataset, the bundle view is folded, and the builtin tool/resource
 * surface is assembled exactly as [`gmeow_mcp::McpServer::from_snapshot`] does natively;
 * the ONLY difference is [`SegmentSet::core`], which routes the reasoning-segment tools
 * to the deferral signal.
 *
 * Calling this again REPLACES the engine wholesale (a new bundle is a new session).
 * A failed load installs nothing, so [`ready`] stays `false` and [`mcp`] keeps
 * refusing frames rather than answering from a stale or partial bundle.
 *
 * # Errors
 *
 * Throws a JS exception if the snapshot does not read as a GTS bundle, if the startup
 * language is unresolvable, or if the builtin surface does not assemble.
 */
export function init(snapshot: Uint8Array): void;

/**
 * Handle ONE JSON-RPC 2.0 frame and return the response frame.
 *
 * `request_json` is a single MCP request object (`initialize`, `tools/list`,
 * `resources/list`, `tools/call`, `resources/read`, `shutdown`, or a
 * `notifications/*` notification). The return is the serialized response frame — or
 * the EMPTY string for a notification, which by protocol has no response. Protocol
 * and tool errors are reported IN the frame (a JSON-RPC `error` member, or a tool
 * envelope with `isError: true`), exactly as native, and are therefore not JS
 * exceptions: a tool that fails is a successful protocol exchange.
 *
 * A frame naming a deferred tool likewise comes back IN the frame, as the structured
 * `mcp.segment-not-loaded` envelope — the host's cue to load the reasoning segment and
 * send this very string to the full engine.
 *
 * # Errors
 *
 * Throws a JS exception only when no snapshot has been loaded — the one condition
 * that is not expressible as a protocol response, since without a bundle there is no
 * server to answer for. Call [`init`] first; [`ready`] reports whether that happened.
 */
export function mcp(request_json: string): string;

/**
 * Whether a snapshot has been loaded and the engine is ready to take frames.
 *
 * `false` before the first successful [`init`] (and after an [`init`] that failed —
 * a failed load leaves NO engine installed rather than a half-built one).
 */
export function ready(): boolean;

/**
 * The engine version (the crate's SemVer), exposed to JS as `version()`.
 *
 * A liveness probe for the wasm build: importing the module and calling `version()`
 * proves it instantiated and the MCP engine linked. It does NOT require [`init`] —
 * version is a property of the image, not of a loaded bundle.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly deferred_segment: (a: number) => void;
    readonly deferred_tools: (a: number) => void;
    readonly init: (a: number, b: number, c: number) => void;
    readonly mcp: (a: number, b: number, c: number) => void;
    readonly ready: () => number;
    readonly version: (a: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export2: (a: number, b: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
