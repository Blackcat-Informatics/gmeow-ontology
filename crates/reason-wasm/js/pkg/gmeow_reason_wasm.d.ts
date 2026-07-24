/* tslint:disable */
/* eslint-disable */

/**
 * Run the structured-DL chase over `data` (RDF text in `format`) and return the
 * **reasoned closure** — the inferred triples — as N-Quads text.
 *
 * - `data` — the RDF document to reason over (UTF-8 text).
 * - `format` — a media type / short id purrdf understands (`turtle`/`ttl`,
 *   `n-triples`/`nt`, `n-quads`/`nq`, `trig`, `rdf+xml`, `json-ld`).
 *
 * # Errors
 *
 * Throws a JS exception if the data cannot be parsed, reasoning fails, or the
 * closure cannot be serialized.
 */
export function reason(data: string, format: string): string;

/**
 * The reasoner version (the crate's SemVer), exposed to JS as `version()` — a
 * liveness probe proving the wasm module instantiated and the reasoner core linked.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly reason: (a: number, b: number, c: number, d: number, e: number) => void;
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
