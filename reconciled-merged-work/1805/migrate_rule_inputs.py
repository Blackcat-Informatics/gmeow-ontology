"""One-use, strict source edit for the native rule grammar; never a runtime reader."""
from pathlib import Path
import difflib
import hashlib
import json
import re
import sys

STAGE = Path(__file__).parent
QUOTED = r'"""[\s\S]*?"""|\'\'\'[\s\S]*?\'\'\'|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\''
SUFFIX = r'(?:@[A-Za-z0-9-]+|\^\^(?:<[^>]*>|[^\s\[\]();,]+))?'
LEX = re.compile(
    r'(?P<space>\s+)|(?P<comment>\#[^\n]*)'
    r'|(?P<string>(?:' + QUOTED + r')' + SUFFIX + r')'
    r'|(?P<quote><<\(|\)>>|<<|>>)'
    r'|(?P<iri><(?!<)[^>]*>)|(?P<punct>[\[\]();,])'
    r'|(?P<word>[^\s<>"\'\[\]();,]+)'
)
RUST = re.compile(
    r'(?P<comment>//[^\n]*|/\*[\s\S]*?\*/)'
    r'|(?P<char>b?\'(?:\\.|[^\'\\])\')'
    r'|(?P<raw>b?r(?P<hashes>\#*)"(?:[\s\S]*?)"(?P=hashes))'
    r'|(?P<normal>b?"(?:\\.|[^"\\])*")',
    re.DOTALL,
)
ROLES = {"logic:head", "logic:body", "logic:negatedBody", "logic:distinctBody"}
CLASSIC = {"rdf:subject", "rdf:predicate", "rdf:object"}


def tokens(text):
    result = []
    end = 0
    for match in LEX.finditer(text):
        if text[end:match.start()].strip():
            raise ValueError(f"unrecognized Turtle source at {end}: gap={text[end:match.start()]!r}; source={text!r}")
        if match.lastgroup not in ("space", "comment"):
            result.append((match.group(), match.start(), match.end()))
        end = match.end()
    if text[end:].strip():
        raise ValueError(f"unrecognized Turtle source at {end}")
    return result


def variable(value):
    match = re.fullmatch(r'"\?((?:\\.|[^"\\])+)"(?:\^\^(?:xsd:string|<http://www.w3.org/2001/XMLSchema#string>))?', value)
    return json.loads('"' + match[1] + '"') if match else None


def fields(text, toks):
    parts = []
    depth = 0
    start = 0
    for i, (token, _, _) in enumerate(toks):
        if token in ("[", "(", "<<(", "<<"):
            depth += 1
        elif token in ("]", ")", ")>>", ">>"):
            depth -= 1
        elif token == ";" and depth == 0:
            parts.append(toks[start:i])
            start = i + 1
    parts.append(toks[start:])
    result = {}
    for part in parts:
        if not part:
            continue
        key = part[0][0]
        if key == "a" and len(part) == 2 and part[1][0] == "rdf:Statement":
            continue
        if key not in CLASSIC or key in result or len(part) < 2:
            return None
        value = part[1:]
        if len(value) != 1:
            return None
        result[key] = text[value[0][1]:value[0][2]]
    if set(result) not in ({"rdf:subject", "rdf:object"}, CLASSIC):
        return None
    return result


def convert_turtle(text):
    namespace = hashlib.blake2s(text.encode(), digest_size=4).hexdigest()
    toks = tokens(text)
    named = set()
    for i, (token, _, _) in enumerate(toks[:-1]):
        if token in ROLES and toks[i + 1][0] != "[":
            j = i + 1
            while j < len(toks) and toks[j][0] not in (";", "."):
                if toks[j][0] != ",":
                    named.add(toks[j][0])
                j += 1
    statements = []
    depth = 0
    begin = 0
    for i, (token, _, _) in enumerate(toks):
        if token in ("[", "(", "<<(", "<<"):
            depth += 1
        elif token in ("]", ")", ")>>", ">>"):
            depth -= 1
        elif token == "." and depth == 0:
            statements.append((begin, i))
            begin = i + 1
    edits = []
    count = 0
    for serial, (first, last) in enumerate(statements):
        st = toks[first:last]
        if not st:
            continue
        values = {token[0] for token in st}
        is_rule = "logic:Rule" in values and bool(values & ROLES)
        is_named_atom = st[0][0] in named and "rdf:subject" in values
        if not is_rule and not is_named_atom:
            continue
        declarations = []
        carriers = {}
        owner = st[0][0]
        stem = owner.replace(":", "_") if re.fullmatch(r"(?:logic|gmeow):[A-Za-z][A-Za-z0-9_-]*", owner) else f"rule{namespace}S{serial}"
        def component(value, role):
            name = variable(value)
            if name is not None:
                if name not in carriers:
                    suffix = re.sub(r"[^A-Za-z0-9_-]", "_", name)
                    carrier = f"_:{stem}_{suffix}"
                    if carrier in carriers.values():
                        raise ValueError(f"variable label collision in {owner}: {name}")
                    carriers[name] = carrier
                    declarations.append(
                        f"{carrier} a logic:TermCarrier ; logic:termVariable {json.dumps(name, ensure_ascii=False)} .\n"
                    )
                return carriers[name]
            if role in ("subject", "predicate") and value.startswith('"'):
                return None
            return value
        def atom_replacement(data):
            nonlocal count
            s = component(data["rdf:subject"], "subject")
            o = component(data["rdf:object"], "object")
            p = data.get("rdf:predicate", "logic:termDistinct")
            if s is None or o is None or p.startswith('"'):
                return None
            count += 1
            return f"rdf:reifies <<( {s} {p} {o} )>>"
        if is_named_atom:
            data = fields(text, st[1:])
            replacement = atom_replacement(data) if data else None
            if replacement:
                edits.append((st[0][2], toks[last][1], " " + replacement + " "))
        else:
            stack = []
            pairs = []
            for i in range(first, last):
                if toks[i][0] == "[":
                    stack.append(i)
                elif toks[i][0] == "]":
                    pairs.append((stack.pop(), i))
            for opening, closing in pairs:
                data = fields(text, toks[opening + 1:closing])
                replacement = atom_replacement(data) if data else None
                if replacement:
                    old = text[toks[opening][1]:toks[closing][2]]
                    if "\n" in old:
                        line = text.rfind("\n", 0, toks[opening][1]) + 1
                        indent = re.match(r"[ \t]*", text[line:toks[opening][1]]).group()
                        replacement = "[\n" + indent + "    " + replacement + "\n" + indent + "]"
                    else:
                        replacement = "[ " + replacement + " ]"
                    edits.append((toks[opening][1], toks[closing][2], replacement))
        if declarations:
            edits.append((st[0][1], st[0][1], "".join(declarations)))
    for start, end, replacement in sorted(edits, reverse=True):
        text = text[:start] + replacement + text[end:]
    return text, count


def rust_unescape(value):
    result = []
    i = 0
    simple = {"n": "\n", "r": "\r", "t": "\t", "0": "\0", "\\": "\\", '"': '"', "'": "'"}
    while i < len(value):
        if value[i] != "\\":
            result.append(value[i])
            i += 1
            continue
        i += 1
        char = value[i]
        if char == "\n":
            i += 1
            while i < len(value) and value[i].isspace():
                i += 1
        elif char == "u" and value[i + 1] == "{":
            end = value.index("}", i)
            result.append(chr(int(value[i + 2:end].replace("_", ""), 16)))
            i = end + 1
        elif char == "x":
            result.append(chr(int(value[i + 1:i + 3], 16)))
            i += 3
        elif char in simple:
            result.append(simple[char])
            i += 1
        else:
            raise ValueError(f"unsupported Rust escape {char}")
    return "".join(result)


def convert_rust(text):
    edits = []
    total = 0
    for match in RUST.finditer(text):
        if match.lastgroup in ("comment", "char"):
            continue
        original = match.group()
        if "rdf:subject" not in original or not any(role in original for role in ROLES):
            continue
        raw = match.group("raw")
        byte = original.startswith("b")
        if raw:
            hashes = match.group("hashes")
            first = original.index('"') + 1
            value = original[first:len(original) - len(hashes) - 1]
        else:
            first = original.index('"') + 1
            value = rust_unescape(original[first:-1])
        converted, count = convert_turtle(value)
        if not count:
            continue
        hashes = "#"
        while '"' + hashes in converted:
            hashes += "#"
        replacement = ("b" if byte else "") + "r" + hashes + '"' + converted + '"' + hashes
        edits.append((match.start(), match.end(), replacement))
        total += count
    for start, end, replacement in reversed(edits):
        text = text[:start] + replacement + text[end:]
    return text, total


def main():
    paths = [Path(line) for line in (STAGE / "migration-inputs.txt").read_text().splitlines()]
    paths += [Path("slices/grounding/logic/module.ttl"), Path("slices/grounding/lang/module.ttl")]
    report = []
    patches = []
    for path in paths:
        original = path.read_text()
        try:
            converted, count = (convert_rust(original) if path.suffix == ".rs" else convert_turtle(original))
        except ValueError as error:
            raise ValueError(f"{path}: {error}") from error
        if not count:
            continue
        patches.extend(difflib.unified_diff(original.splitlines(True), converted.splitlines(True), fromfile=str(path), tofile=str(path)))
        report.append({"path": str(path), "atoms": count})
        if "--write" in sys.argv:
            path.write_text(converted)
    (STAGE / "migration-preview.diff").write_text("".join(patches))
    (STAGE / "migration-report.json").write_text(json.dumps(report, indent=2) + "\n")
    for row in report:
        print(row["atoms"], row["path"])


if __name__ == "__main__":
    main()
