/* @ts-self-types="./gmeow_rdf_wasm.d.ts" */

/**
 * An RDF/JS `DataFactory`. Stateless except for the auto-generated blank-node
 * counter (`blankNode()` with no argument mints a fresh label).
 */
export class DataFactory {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        DataFactoryFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_datafactory_free(ptr, 0);
    }
    /**
     * `blankNode(value?)` → a `BlankNode` term; a fresh label is minted when omitted.
     * @param {string | null} [value]
     * @returns {Term}
     */
    blankNode(value) {
        var ptr0 = isLikeNone(value) ? 0 : passStringToWasm0(value, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len0 = WASM_VECTOR_LEN;
        const ret = wasm.datafactory_blankNode(this.__wbg_ptr, ptr0, len0);
        return Term.__wrap(ret);
    }
    /**
     * `defaultGraph()` → the `DefaultGraph` term.
     * @returns {Term}
     */
    defaultGraph() {
        const ret = wasm.datafactory_defaultGraph(this.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * `directionalLiteral(value, language, direction)` → an RDF-1.2 base-direction
     * literal (`direction` is `"ltr"` or `"rtl"`). The deliberate extension to stock
     * RDF/JS — no incumbent library carries base direction.
     * @param {string} value
     * @param {string} language
     * @param {string} direction
     * @returns {Term}
     */
    directionalLiteral(value, language, direction) {
        const ptr0 = passStringToWasm0(value, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(direction, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.datafactory_directionalLiteral(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Term.__wrap(ret[0]);
    }
    /**
     * `fromQuad(original)` → a copy of `original`.
     * @param {Quad} original
     * @returns {Quad}
     */
    fromQuad(original) {
        _assertClass(original, Quad);
        const ret = wasm.datafactory_fromQuad(this.__wbg_ptr, original.__wbg_ptr);
        return Quad.__wrap(ret);
    }
    /**
     * `fromTerm(original)` → a copy of `original` (RDF/JS structural clone).
     * @param {Term} original
     * @returns {Term}
     */
    fromTerm(original) {
        _assertClass(original, Term);
        const ret = wasm.datafactory_fromTerm(this.__wbg_ptr, original.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * `literal(value, language?)` → a plain (`xsd:string`) or language-tagged literal.
     *
     * The RDF/JS spec's unified `literal(value, languageOrDatatype)` — where the second
     * argument may be a string *or* a `NamedNode` — is presented by the TypeScript
     * wrapper, which dispatches the `NamedNode` case to [`DataFactory::typed_literal`].
     * (A `#[wasm_bindgen]`-exported type cannot be recovered from an untyped `JsValue`
     * in Rust, so the polymorphism lives one layer out, in JS.) For base-direction
     * literals (RDF 1.2) use [`DataFactory::directional_literal`].
     * @param {string} value
     * @param {string | null} [language]
     * @returns {Term}
     */
    literal(value, language) {
        const ptr0 = passStringToWasm0(value, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(language) ? 0 : passStringToWasm0(language, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.datafactory_literal(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        return Term.__wrap(ret);
    }
    /**
     * `namedNode(value)` → a `NamedNode` term.
     * @param {string} value
     * @returns {Term}
     */
    namedNode(value) {
        const ptr0 = passStringToWasm0(value, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.datafactory_namedNode(this.__wbg_ptr, ptr0, len0);
        return Term.__wrap(ret);
    }
    constructor() {
        const ret = wasm.datafactory_new();
        this.__wbg_ptr = ret;
        DataFactoryFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * `quad(subject, predicate, object, graph?)` → a `Quad`. The graph defaults to the
     * default graph. A quoted-triple term (from [`DataFactory::quoted_triple`]) may be
     * passed as `subject` or `object` (the RDF-1.2 wedge).
     * @param {Term} subject
     * @param {Term} predicate
     * @param {Term} object
     * @param {Term | null} [graph]
     * @returns {Quad}
     */
    quad(subject, predicate, object, graph) {
        _assertClass(subject, Term);
        _assertClass(predicate, Term);
        _assertClass(object, Term);
        let ptr0 = 0;
        if (!isLikeNone(graph)) {
            _assertClass(graph, Term);
            ptr0 = graph.__destroy_into_raw();
        }
        const ret = wasm.datafactory_quad(this.__wbg_ptr, subject.__wbg_ptr, predicate.__wbg_ptr, object.__wbg_ptr, ptr0);
        return Quad.__wrap(ret);
    }
    /**
     * `quotedTriple(subject, predicate, object)` → a quoted-triple `Term`
     * (`termType: "Quad"`) — the RDF-1.2 wedge. Embed it by passing it as the
     * `subject`/`object` of another quad.
     * @param {Term} subject
     * @param {Term} predicate
     * @param {Term} object
     * @returns {Term}
     */
    quotedTriple(subject, predicate, object) {
        _assertClass(subject, Term);
        _assertClass(predicate, Term);
        _assertClass(object, Term);
        const ret = wasm.datafactory_quotedTriple(this.__wbg_ptr, subject.__wbg_ptr, predicate.__wbg_ptr, object.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Term.__wrap(ret[0]);
    }
    /**
     * `typedLiteral(value, datatype)` → a datatyped literal. `datatype` must be a
     * `NamedNode`. (The RDF/JS `literal(value, datatype)` form, surfaced by the TS
     * wrapper.)
     * @param {string} value
     * @param {Term} datatype
     * @returns {Term}
     */
    typedLiteral(value, datatype) {
        const ptr0 = passStringToWasm0(value, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(datatype, Term);
        const ret = wasm.datafactory_typedLiteral(this.__wbg_ptr, ptr0, len0, datatype.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Term.__wrap(ret[0]);
    }
    /**
     * `variable(value)` → a `Variable` term.
     * @param {string} value
     * @returns {Term}
     */
    variable(value) {
        const ptr0 = passStringToWasm0(value, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.datafactory_variable(this.__wbg_ptr, ptr0, len0);
        return Term.__wrap(ret);
    }
}
if (Symbol.dispose) DataFactory.prototype[Symbol.dispose] = DataFactory.prototype.free;

/**
 * An RDF/JS `DatasetCore` backed by the engine's COW mutable dataset.
 */
export class Dataset {
    static __wrap(ptr) {
        const obj = Object.create(Dataset.prototype);
        obj.__wbg_ptr = ptr;
        DatasetFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        DatasetFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_dataset_free(ptr, 0);
    }
    /**
     * `add(quad)` → insert a quad. Returns `true` if the effective set changed.
     * @param {Quad} quad
     * @returns {boolean}
     */
    add(quad) {
        _assertClass(quad, Quad);
        const ret = wasm.dataset_add(this.__wbg_ptr, quad.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] !== 0;
    }
    /**
     * `delete(quad)` → remove a quad. Returns `true` if the effective set changed.
     * @param {Quad} quad
     * @returns {boolean}
     */
    delete(quad) {
        _assertClass(quad, Quad);
        const ret = wasm.dataset_delete(this.__wbg_ptr, quad.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] !== 0;
    }
    /**
     * `has(quad)` → whether the quad is in the dataset.
     * @param {Quad} quad
     * @returns {boolean}
     */
    has(quad) {
        _assertClass(quad, Quad);
        const ret = wasm.dataset_has(this.__wbg_ptr, quad.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] !== 0;
    }
    /**
     * `match(subject?, predicate?, object?, graph?)` → a new dataset of the matching
     * quads. An omitted (`undefined`) position is a wildcard; `defaultGraph()` matches
     * only the default graph, a named node matches that graph.
     * @param {Term | null} [subject]
     * @param {Term | null} [predicate]
     * @param {Term | null} [object]
     * @param {Term | null} [graph]
     * @returns {Dataset}
     */
    match(subject, predicate, object, graph) {
        let ptr0 = 0;
        if (!isLikeNone(subject)) {
            _assertClass(subject, Term);
            ptr0 = subject.__destroy_into_raw();
        }
        let ptr1 = 0;
        if (!isLikeNone(predicate)) {
            _assertClass(predicate, Term);
            ptr1 = predicate.__destroy_into_raw();
        }
        let ptr2 = 0;
        if (!isLikeNone(object)) {
            _assertClass(object, Term);
            ptr2 = object.__destroy_into_raw();
        }
        let ptr3 = 0;
        if (!isLikeNone(graph)) {
            _assertClass(graph, Term);
            ptr3 = graph.__destroy_into_raw();
        }
        const ret = wasm.dataset_match(this.__wbg_ptr, ptr0, ptr1, ptr2, ptr3);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Dataset.__wrap(ret[0]);
    }
    /**
     * An empty dataset.
     */
    constructor() {
        const ret = wasm.dataset_new();
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        DatasetFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * `parse(input, format, base?)` → a dataset of the parsed quads.
     *
     * `format` is a media type or short name (turtle/ntriples/nquads/trig/rdfxml).
     * Ill-typed literals are preserved verbatim (RDFLib parity), not rejected.
     * @param {string} input
     * @param {string} format
     * @param {string | null} [base]
     * @returns {Dataset}
     */
    static parse(input, format, base) {
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(format, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len2 = WASM_VECTOR_LEN;
        const ret = wasm.dataset_parse(ptr0, len0, ptr1, len1, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Dataset.__wrap(ret[0]);
    }
    /**
     * `quads()` → every effective quad, as a JS array.
     * @returns {Quad[]}
     */
    quads() {
        const ret = wasm.dataset_quads(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * `query(sparql, base?)` → run a SPARQL query against this dataset, offline.
     *
     * Returns **SPARQL Results JSON** for SELECT / ASK and **Turtle** for
     * CONSTRUCT / DESCRIBE. A parse error, an evaluation error, or a `SERVICE` /
     * `LOAD` clause (unresolvable in-browser) throws a JsError — never a silent
     * empty result.
     * @param {string} sparql
     * @param {string | null} [base]
     * @returns {string}
     */
    query(sparql, base) {
        let deferred4_0;
        let deferred4_1;
        try {
            const ptr0 = passStringToWasm0(sparql, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(base) ? 0 : passStringToWasm0(base, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            const ret = wasm.dataset_query(this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var ptr3 = ret[0];
            var len3 = ret[1];
            if (ret[3]) {
                ptr3 = 0; len3 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred4_0 = ptr3;
            deferred4_1 = len3;
            return getStringFromWasm0(ptr3, len3);
        } finally {
            wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
        }
    }
    /**
     * `serialize(format)` → the dataset rendered in `format` (a UTF-8 string).
     *
     * Formats: `turtle` / `ntriples` / `nquads` / `trig` / `rdfxml` (their media types
     * too) plus `jsonld` (JSON-LD-star). Note: a quoted-triple term appearing as a quad
     * object currently round-trips only through N-Quads (a serializer limitation for
     * the other text formats).
     * @param {string} format
     * @returns {string}
     */
    serialize(format) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(format, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.dataset_serialize(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * `size` — the number of effective quads.
     * @returns {number}
     */
    get size() {
        const ret = wasm.dataset_size(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) Dataset.prototype[Symbol.dispose] = Dataset.prototype.free;

/**
 * An RDF/JS [Quad](https://rdf.js.org/data-model-spec/#quad-interface) — a statement
 * `(subject, predicate, object, graph)` with `termType: "Quad"`.
 */
export class Quad {
    static __wrap(ptr) {
        const obj = Object.create(Quad.prototype);
        obj.__wbg_ptr = ptr;
        QuadFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        QuadFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_quad_free(ptr, 0);
    }
    /**
     * A quoted-triple [`Term`] (`termType: "Quad"`) viewing this quad's `(s, p, o)` —
     * the RDF-1.2 wedge: pass the result as a subject/object to embed it.
     * @returns {Term}
     */
    asTerm() {
        const ret = wasm.quad_asTerm(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Term.__wrap(ret[0]);
    }
    /**
     * Structural RDF/JS quad equality.
     * @param {Quad} other
     * @returns {boolean}
     */
    equals(other) {
        _assertClass(other, Quad);
        const ret = wasm.quad_equals(this.__wbg_ptr, other.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @returns {Term}
     */
    get graph() {
        const ret = wasm.quad_graph(this.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * @returns {Term}
     */
    get object() {
        const ret = wasm.quad_object(this.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * @returns {Term}
     */
    get predicate() {
        const ret = wasm.quad_predicate(this.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * @returns {Term}
     */
    get subject() {
        const ret = wasm.quad_subject(this.__wbg_ptr);
        return Term.__wrap(ret);
    }
    /**
     * Always `"Quad"` (a Quad is itself an RDF/JS term).
     * @returns {string}
     */
    get termType() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.quad_term_type(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Empty for a Quad (per RDF/JS).
     * @returns {string}
     */
    get value() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.quad_value(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Quad.prototype[Symbol.dispose] = Quad.prototype.free;

/**
 * An RDF/JS `Sink` — a streaming consumer that interns pushed quads through the
 * `gmeow-rdf-events` protocol and freezes them at `finish()`.
 */
export class Sink {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SinkFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_sink_free(ptr, 0);
    }
    /**
     * `finish()` — run the protocol's forward-reference resolution and return the
     * resulting dataset. The sink is consumed; further `push`/`finish` is an error.
     * @returns {Dataset}
     */
    finish() {
        const ret = wasm.sink_finish(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return Dataset.__wrap(ret[0]);
    }
    constructor() {
        const ret = wasm.sink_new();
        this.__wbg_ptr = ret;
        SinkFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * `push(quad)` — stream one quad into the sink (interned via the event protocol).
     * @param {Quad} quad
     */
    push(quad) {
        _assertClass(quad, Quad);
        const ret = wasm.sink_push(this.__wbg_ptr, quad.__wbg_ptr);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
}
if (Symbol.dispose) Sink.prototype[Symbol.dispose] = Sink.prototype.free;

/**
 * An RDF/JS [Term](https://rdf.js.org/data-model-spec/#term-interface).
 */
export class Term {
    static __wrap(ptr) {
        const obj = Object.create(Term.prototype);
        obj.__wbg_ptr = ptr;
        TermFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TermFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_term_free(ptr, 0);
    }
    /**
     * `datatype` — the literal's datatype as a `NamedNode`, or `undefined` for a
     * non-literal.
     * @returns {Term | undefined}
     */
    get datatype() {
        const ret = wasm.term_datatype(this.__wbg_ptr);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * `direction` — the RDF-1.2 base direction (`"ltr"`/`"rtl"`), or `""` when absent.
     * The deliberate extension to stock RDF/JS (`.goals`: overcome, don't inherit).
     * @returns {string}
     */
    get direction() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.term_direction(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Structural RDF/JS term equality.
     * @param {Term} other
     * @returns {boolean}
     */
    equals(other) {
        _assertClass(other, Term);
        const ret = wasm.term_equals(this.__wbg_ptr, other.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * `graph` of a quoted-triple term — always the default graph (a quoted triple has
     * no graph slot), else `undefined`.
     * @returns {Term | undefined}
     */
    get graph() {
        const ret = wasm.term_graph(this.__wbg_ptr);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * `language` — the literal's language tag, or `""` for a non-language-tagged term
     * (RDF/JS uses the empty string, not `undefined`).
     * @returns {string}
     */
    get language() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.term_language(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * `object` of a quoted-triple term, else `undefined`.
     * @returns {Term | undefined}
     */
    get object() {
        const ret = wasm.term_object(this.__wbg_ptr);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * `predicate` of a quoted-triple term as a `NamedNode`, else `undefined`.
     * @returns {Term | undefined}
     */
    get predicate() {
        const ret = wasm.term_predicate(this.__wbg_ptr);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * `subject` of a quoted-triple term (`termType: "Quad"`), else `undefined`.
     * @returns {Term | undefined}
     */
    get subject() {
        const ret = wasm.term_subject(this.__wbg_ptr);
        return ret === 0 ? undefined : Term.__wrap(ret);
    }
    /**
     * `termType` — the RDF/JS discriminator.
     * @returns {string}
     */
    get termType() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.term_term_type(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * `value` — the IRI, blank label, lexical form, or variable name. Empty for a
     * quoted triple and the default graph (per RDF/JS).
     * @returns {string}
     */
    get value() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.term_value(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) Term.prototype[Symbol.dispose] = Term.prototype.free;

/**
 * The purrdf engine version (the crate's SemVer), exposed to JS as `version()`.
 *
 * A liveness probe for the wasm build + the npm package: importing `purrdf` and
 * calling `version()` proves the module instantiated and the engine linked.
 * @returns {string}
 */
export function version() {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.version();
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_fdd633d4bb5dd76a: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_throw_ea4887a5f8f9a9db: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_quad_new: function(arg0) {
            const ret = Quad.__wrap(arg0);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./gmeow_rdf_wasm_bg.js": import0,
    };
}

const DataFactoryFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_datafactory_free(ptr, 1));
const DatasetFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_dataset_free(ptr, 1));
const QuadFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_quad_free(ptr, 1));
const SinkFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_sink_free(ptr, 1));
const TermFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_term_free(ptr, 1));

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('gmeow_rdf_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
