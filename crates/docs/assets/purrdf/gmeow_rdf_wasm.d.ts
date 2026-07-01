/* tslint:disable */
/* eslint-disable */

/**
 * An RDF/JS `DataFactory`. Stateless except for the auto-generated blank-node
 * counter (`blankNode()` with no argument mints a fresh label).
 */
export class DataFactory {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * `blankNode(value?)` → a `BlankNode` term; a fresh label is minted when omitted.
     */
    blankNode(value?: string | null): Term;
    /**
     * `defaultGraph()` → the `DefaultGraph` term.
     */
    defaultGraph(): Term;
    /**
     * `directionalLiteral(value, language, direction)` → an RDF-1.2 base-direction
     * literal (`direction` is `"ltr"` or `"rtl"`). The deliberate extension to stock
     * RDF/JS — no incumbent library carries base direction.
     */
    directionalLiteral(value: string, language: string, direction: string): Term;
    /**
     * `fromQuad(original)` → a copy of `original`.
     */
    fromQuad(original: Quad): Quad;
    /**
     * `fromTerm(original)` → a copy of `original` (RDF/JS structural clone).
     */
    fromTerm(original: Term): Term;
    /**
     * `literal(value, language?)` → a plain (`xsd:string`) or language-tagged literal.
     *
     * The RDF/JS spec's unified `literal(value, languageOrDatatype)` — where the second
     * argument may be a string *or* a `NamedNode` — is presented by the TypeScript
     * wrapper, which dispatches the `NamedNode` case to [`DataFactory::typed_literal`].
     * (A `#[wasm_bindgen]`-exported type cannot be recovered from an untyped `JsValue`
     * in Rust, so the polymorphism lives one layer out, in JS.) For base-direction
     * literals (RDF 1.2) use [`DataFactory::directional_literal`].
     */
    literal(value: string, language?: string | null): Term;
    /**
     * `namedNode(value)` → a `NamedNode` term.
     */
    namedNode(value: string): Term;
    constructor();
    /**
     * `quad(subject, predicate, object, graph?)` → a `Quad`. The graph defaults to the
     * default graph. A quoted-triple term (from [`DataFactory::quoted_triple`]) may be
     * passed as `subject` or `object` (the RDF-1.2 wedge).
     */
    quad(subject: Term, predicate: Term, object: Term, graph?: Term | null): Quad;
    /**
     * `quotedTriple(subject, predicate, object)` → a quoted-triple `Term`
     * (`termType: "Quad"`) — the RDF-1.2 wedge. Embed it by passing it as the
     * `subject`/`object` of another quad.
     */
    quotedTriple(subject: Term, predicate: Term, object: Term): Term;
    /**
     * `typedLiteral(value, datatype)` → a datatyped literal. `datatype` must be a
     * `NamedNode`. (The RDF/JS `literal(value, datatype)` form, surfaced by the TS
     * wrapper.)
     */
    typedLiteral(value: string, datatype: Term): Term;
    /**
     * `variable(value)` → a `Variable` term.
     */
    variable(value: string): Term;
}

/**
 * An RDF/JS `DatasetCore` backed by the engine's COW mutable dataset.
 */
export class Dataset {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * `add(quad)` → insert a quad. Returns `true` if the effective set changed.
     */
    add(quad: Quad): boolean;
    /**
     * `delete(quad)` → remove a quad. Returns `true` if the effective set changed.
     */
    delete(quad: Quad): boolean;
    /**
     * `has(quad)` → whether the quad is in the dataset.
     */
    has(quad: Quad): boolean;
    /**
     * `match(subject?, predicate?, object?, graph?)` → a new dataset of the matching
     * quads. An omitted (`undefined`) position is a wildcard; `defaultGraph()` matches
     * only the default graph, a named node matches that graph.
     */
    match(subject?: Term | null, predicate?: Term | null, object?: Term | null, graph?: Term | null): Dataset;
    /**
     * An empty dataset.
     */
    constructor();
    /**
     * `parse(input, format, base?)` → a dataset of the parsed quads.
     *
     * `format` is a media type or short name (turtle/ntriples/nquads/trig/rdfxml).
     * Ill-typed literals are preserved verbatim (RDFLib parity), not rejected.
     */
    static parse(input: string, format: string, base?: string | null): Dataset;
    /**
     * `quads()` → every effective quad, as a JS array.
     */
    quads(): Quad[];
    /**
     * `query(sparql, base?)` → run a SPARQL query against this dataset, offline.
     *
     * Returns **SPARQL Results JSON** for SELECT / ASK and **Turtle** for
     * CONSTRUCT / DESCRIBE. A parse error, an evaluation error, or a `SERVICE` /
     * `LOAD` clause (unresolvable in-browser) throws a JsError — never a silent
     * empty result.
     */
    query(sparql: string, base?: string | null): string;
    /**
     * `serialize(format)` → the dataset rendered in `format` (a UTF-8 string).
     *
     * Note: a quoted-triple term appearing as a quad object currently round-trips
     * only through N-Quads (a gmeow-gts serializer limitation for the other formats).
     */
    serialize(format: string): string;
    /**
     * `size` — the number of effective quads.
     */
    readonly size: number;
}

/**
 * An RDF/JS [Quad](https://rdf.js.org/data-model-spec/#quad-interface) — a statement
 * `(subject, predicate, object, graph)` with `termType: "Quad"`.
 */
export class Quad {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * A quoted-triple [`Term`] (`termType: "Quad"`) viewing this quad's `(s, p, o)` —
     * the RDF-1.2 wedge: pass the result as a subject/object to embed it.
     */
    asTerm(): Term;
    /**
     * Structural RDF/JS quad equality.
     */
    equals(other: Quad): boolean;
    readonly graph: Term;
    readonly object: Term;
    readonly predicate: Term;
    readonly subject: Term;
    /**
     * Always `"Quad"` (a Quad is itself an RDF/JS term).
     */
    readonly termType: string;
    /**
     * Empty for a Quad (per RDF/JS).
     */
    readonly value: string;
}

/**
 * An RDF/JS `Sink` — a streaming consumer that interns pushed quads through the
 * `gmeow-rdf-events` protocol and freezes them at `finish()`.
 */
export class Sink {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * `finish()` — run the protocol's forward-reference resolution and return the
     * resulting dataset. The sink is consumed; further `push`/`finish` is an error.
     */
    finish(): Dataset;
    constructor();
    /**
     * `push(quad)` — stream one quad into the sink (interned via the event protocol).
     */
    push(quad: Quad): void;
}

/**
 * An RDF/JS [Term](https://rdf.js.org/data-model-spec/#term-interface).
 */
export class Term {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Structural RDF/JS term equality.
     */
    equals(other: Term): boolean;
    /**
     * `datatype` — the literal's datatype as a `NamedNode`, or `undefined` for a
     * non-literal.
     */
    readonly datatype: Term | undefined;
    /**
     * `direction` — the RDF-1.2 base direction (`"ltr"`/`"rtl"`), or `""` when absent.
     * The deliberate extension to stock RDF/JS (`.goals`: overcome, don't inherit).
     */
    readonly direction: string;
    /**
     * `graph` of a quoted-triple term — always the default graph (a quoted triple has
     * no graph slot), else `undefined`.
     */
    readonly graph: Term | undefined;
    /**
     * `language` — the literal's language tag, or `""` for a non-language-tagged term
     * (RDF/JS uses the empty string, not `undefined`).
     */
    readonly language: string;
    /**
     * `object` of a quoted-triple term, else `undefined`.
     */
    readonly object: Term | undefined;
    /**
     * `predicate` of a quoted-triple term as a `NamedNode`, else `undefined`.
     */
    readonly predicate: Term | undefined;
    /**
     * `subject` of a quoted-triple term (`termType: "Quad"`), else `undefined`.
     */
    readonly subject: Term | undefined;
    /**
     * `termType` — the RDF/JS discriminator.
     */
    readonly termType: string;
    /**
     * `value` — the IRI, blank label, lexical form, or variable name. Empty for a
     * quoted triple and the default graph (per RDF/JS).
     */
    readonly value: string;
}

/**
 * The purrdf engine version (the crate's SemVer), exposed to JS as `version()`.
 *
 * A liveness probe for the wasm build + the npm package: importing `purrdf` and
 * calling `version()` proves the module instantiated and the engine linked.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_datafactory_free: (a: number, b: number) => void;
    readonly __wbg_dataset_free: (a: number, b: number) => void;
    readonly __wbg_quad_free: (a: number, b: number) => void;
    readonly __wbg_sink_free: (a: number, b: number) => void;
    readonly __wbg_term_free: (a: number, b: number) => void;
    readonly datafactory_blankNode: (a: number, b: number, c: number) => number;
    readonly datafactory_defaultGraph: (a: number) => number;
    readonly datafactory_directionalLiteral: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number, number];
    readonly datafactory_fromQuad: (a: number, b: number) => number;
    readonly datafactory_fromTerm: (a: number, b: number) => number;
    readonly datafactory_literal: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly datafactory_namedNode: (a: number, b: number, c: number) => number;
    readonly datafactory_new: () => number;
    readonly datafactory_quad: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly datafactory_quotedTriple: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly datafactory_typedLiteral: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly datafactory_variable: (a: number, b: number, c: number) => number;
    readonly dataset_add: (a: number, b: number) => [number, number, number];
    readonly dataset_delete: (a: number, b: number) => [number, number, number];
    readonly dataset_has: (a: number, b: number) => [number, number, number];
    readonly dataset_match: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly dataset_new: () => [number, number, number];
    readonly dataset_parse: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly dataset_quads: (a: number) => [number, number, number, number];
    readonly dataset_query: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly dataset_serialize: (a: number, b: number, c: number) => [number, number, number, number];
    readonly dataset_size: (a: number) => number;
    readonly quad_asTerm: (a: number) => [number, number, number];
    readonly quad_equals: (a: number, b: number) => number;
    readonly quad_graph: (a: number) => number;
    readonly quad_object: (a: number) => number;
    readonly quad_predicate: (a: number) => number;
    readonly quad_subject: (a: number) => number;
    readonly quad_term_type: (a: number) => [number, number];
    readonly quad_value: (a: number) => [number, number];
    readonly sink_finish: (a: number) => [number, number, number];
    readonly sink_new: () => number;
    readonly sink_push: (a: number, b: number) => [number, number];
    readonly term_datatype: (a: number) => number;
    readonly term_direction: (a: number) => [number, number];
    readonly term_equals: (a: number, b: number) => number;
    readonly term_graph: (a: number) => number;
    readonly term_language: (a: number) => [number, number];
    readonly term_object: (a: number) => number;
    readonly term_predicate: (a: number) => number;
    readonly term_subject: (a: number) => number;
    readonly term_term_type: (a: number) => [number, number];
    readonly term_value: (a: number) => [number, number];
    readonly version: () => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
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
