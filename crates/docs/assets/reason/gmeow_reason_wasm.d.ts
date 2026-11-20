/* tslint:disable */
/* eslint-disable */

/**
 * Admit a complete artifact before rendering facets or recording a derivation.
 *
 * # Errors
 * Throws when any selected result, attachment or native transport is invalid.
 */
export function admit_artifact(bytes: Uint8Array | number[]): string;

/**
 * Test a candidate `logic:` formula against a KB with the native symmetric engine
 * and return its complete artifact envelope and admitted verdict facets as JSON.
 * The native MCP, CLI and browser share the same publication and admission owner.
 *
 * - `kb` — the knowledge base to test against (RDF text in `kb_format`).
 * - `kb_format` — a media type / short id purrdf understands (`turtle`/`ttl`,
 *   `n-triples`/`nt`, `n-quads`/`nq`, `trig`, `rdf+xml`, `json-ld`).
 * - `formula` — the candidate `logic:` document naming exactly one `logic:Formula` / axiom.
 * - `standpoint` — the reified standpoint IRI the verdict is scoped to (REQUIRED; a
 *   conjecture verdict is always standpoint-scoped, never global — Principle 9).
 *
 * The proof leg `KB ⊨ φ`, counterproof leg `KB ∪ {φ} ⊨ ⊥` and Belnap
 * classification are readable from the admitted RDF facets carried beside the artifact.
 *
 * # Errors
 *
 * Throws a JS exception if the candidate does not name exactly one formula, if the KB
 * cannot be parsed, or if the native conjecture engine fails.
 */
export function conjecture(kb: string, kb_format: string, formula: string, standpoint: string): string;

/**
 * Run the structured-DL chase over `data` (RDF text in `format`) and return the
 * complete reasoning artifact as JSON, retaining its binary native receipt,
 * asserted and inferred closure, and the selected nonempty-domain witness.
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
    readonly admit_artifact: (a: number, b: number, c: number) => void;
    readonly conjecture: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly reason: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly version: (a: number) => void;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export3: (a: number) => void;
    readonly __wbindgen_export4: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_start: () => void;
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
