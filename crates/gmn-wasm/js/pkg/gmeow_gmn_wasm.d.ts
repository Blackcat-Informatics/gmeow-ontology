/* tslint:disable */
/* eslint-disable */

/**
 * wasm export: read a GMN-1 surface back to canonical N-Quads. Thin marshal over
 * [`transcode_from_gmn1`].
 *
 * # Errors
 *
 * Throws if the GMN-1 text cannot be read back.
 */
export function from_gmn1(gmn1_text: string): string;

/**
 * wasm export: the GMN-1 glyph legend as JSON. Thin marshal over
 * [`glyph_legend_json`].
 *
 * # Errors
 *
 * Throws if the embedded codebook cannot be read.
 */
export function glyph_legend(): string;

/**
 * wasm export: transcode RDF text to the GMN-1 surface. Thin marshal over
 * [`transcode_to_gmn1`].
 *
 * # Errors
 *
 * Throws if the RDF cannot be parsed or the GMN-1 write fails.
 */
export function to_gmn1(data: string, format: string): string;

/**
 * The codec version (the crate's SemVer), exposed to JS as `version()`.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly from_gmn1: (a: number, b: number, c: number) => void;
    readonly glyph_legend: (a: number) => void;
    readonly to_gmn1: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly version: (a: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
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
