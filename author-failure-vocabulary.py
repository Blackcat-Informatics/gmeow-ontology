"""Author the coordinated failure vocabulary and its French/Chinese coat."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROPERTIES = [
    ('rdfs:label', 'http://www.w3.org/2000/01/rdf-schema#label'),
    ('skos:definition', 'http://www.w3.org/2004/02/skos/core#definition'),
    ('gmeow:useWhen', 'https://blackcatinformatics.ca/gmeow/useWhen'),
    ('gmeow:avoidWhen', 'https://blackcatinformatics.ca/gmeow/avoidWhen'),
    ('gmeow:howToUse', 'https://blackcatinformatics.ca/gmeow/howToUse'),
    ('skos:scopeNote', 'http://www.w3.org/2004/02/skos/core#scopeNote'),
    ('skos:example', 'http://www.w3.org/2004/02/skos/core#example'),
]
TERMS = [
    ('core/pipeline', 'gmeow:RecordedConsumerFailure', 'logic:Class', None, [
        ('recorded consumer failure', 'échec du consommateur de données enregistrées', '记录数据使用方故障'),
        ('An operational failure while reading, checking, evaluating or reporting explicitly selected recorded inputs, retaining the selected operation and original typed cause.', 'Un échec opérationnel pendant la lecture, la vérification, l’évaluation ou la restitution d’entrées enregistrées explicitement sélectionnées, conservant l’opération sélectionnée et la cause typée d’origine.', '读取、检查、求值或报告显式选定的记录输入时发生的操作故障，保留所选操作和原始类型化原因。'),
        ('Use for failures of an explicit recorded-input consumer operation.', 'À utiliser pour les échecs d’une opération explicite de consommation d’entrées enregistrées.', '用于显式记录输入使用操作的故障。'),
        ('Do not interpret an operational failure as a semantic verdict.', 'Ne pas interpréter un échec opérationnel comme un verdict sémantique.', '不要将操作故障解释为语义判定。'),
        ('Retain the operation, input focus and complete original cause chain.', 'Conserver l’opération, le point d’entrée concerné et toute la chaîne des causes d’origine.', '保留操作、输入焦点和完整的原始原因链。'),
        ('This class concerns availability of the requested consumer result.', 'Cette classe concerne la disponibilité du résultat demandé au consommateur.', '本类描述所请求使用方结果的可用性。'),
        ('A missing selected receipt retains its original file-access error.', 'Un reçu sélectionné absent conserve son erreur d’accès au fichier d’origine.', '缺失的所选回执保留其原始文件访问错误。'),
    ]),
    ('core/pipeline', 'gmeow:RecordedConsumerBoundary', 'logic:NamedIndividual', 'gmeow:RecordedConsumerFailure', [
        ('recorded consumer boundary', 'frontière du consommateur de données enregistrées', '记录数据使用方边界'),
        ('The operational boundary requiring an explicit recorded-input consumer to preserve every selected input, requested output and original failure cause.', 'La frontière opérationnelle imposant à un consommateur explicite d’entrées enregistrées de préserver chaque entrée sélectionnée, chaque sortie demandée et la cause d’origine de tout échec.', '要求显式记录输入使用方保留每项所选输入、所请求输出和原始故障原因的操作边界。'),
        ('Use to identify the boundary enforced by recorded-consumer diagnostics.', 'À utiliser pour identifier la frontière contrôlée par les diagnostics du consommateur de données enregistrées.', '用于标识记录数据使用方诊断所执行的边界。'),
        ('Do not repair missing inputs by executing a corpus producer.', 'Ne pas réparer les entrées absentes en exécutant un producteur de corpus.', '不要通过执行语料生产程序来修复缺失输入。'),
        ('Admit existing products read-only and fail if any selected operation fails.', 'Admettre les produits existants en lecture seule et signaler tout échec d’une opération sélectionnée.', '以只读方式接纳现有产物，任何所选操作失败时均须报错。'),
        ('The boundary does not certify current canonical source completeness.', 'Cette frontière ne certifie pas la complétude des sources canoniques actuelles.', '此边界不证明当前规范源的完整性。'),
        ('A report-writing failure prevents successful consumer completion.', 'Un échec d’écriture du rapport empêche de déclarer le consommateur terminé avec succès.', '报告写入失败时，使用方不得宣告成功完成。'),
    ]),
    ('grounding/logic', 'logic:CompilerSourceRefusal', 'logic:Class', None, [
        ('compiler source refusal', 'refus de source du compilateur', '编译器源输入拒绝'),
        ('An operational refusal to acquire or decode explicitly selected compiler source, retaining the selected path or format and original typed cause.', 'Un refus opérationnel d’acquérir ou de décoder une source du compilateur explicitement sélectionnée, conservant le chemin ou le format sélectionné et la cause typée d’origine.', '获取或解码显式选定的编译器源输入时发生的操作拒绝，保留所选路径或格式以及原始类型化原因。'),
        ('Use when a selected source cannot be read or parsed.', 'À utiliser lorsqu’une source sélectionnée ne peut pas être lue ou analysée.', '用于无法读取或解析所选源输入的情况。'),
        ('Do not classify file-access failure as a false formula.', 'Ne pas classer un échec d’accès au fichier comme une formule fausse.', '不要将文件访问故障归类为公式为假。'),
        ('Preserve the access or codec disposition and original native cause.', 'Préserver la nature du refus d’accès ou du codec et la cause native d’origine.', '保留访问或编解码器拒绝类别以及原始本地原因。'),
        ('The refusal precedes compiler admission and makes no consistency claim.', 'Le refus précède l’admission par le compilateur et ne prétend pas établir la cohérence.', '此拒绝发生在编译器接纳之前，不作一致性断言。'),
        ('A malformed RDF source retains its native parser diagnostic.', 'Une source RDF mal formée conserve le diagnostic de son analyseur natif.', '格式错误的 RDF 源输入保留其原生解析器诊断。'),
    ]),
    ('grounding/logic', 'logic:CompilerSourceBoundary', 'logic:NamedIndividual', 'logic:CompilerSourceRefusal', [
        ('compiler source boundary', 'frontière des sources du compilateur', '编译器源输入边界'),
        ('The operational boundary requiring every selected compiler source to be readable and accepted by its declared codec before compilation.', 'La frontière opérationnelle imposant que chaque source sélectionnée soit lisible et acceptée par son codec déclaré avant la compilation.', '要求每项所选编译器源输入在编译前均可读取并被其声明的编解码器接纳的操作边界。'),
        ('Use to identify the boundary enforced by source-read and source-parse diagnostics.', 'À utiliser pour identifier la frontière contrôlée par les diagnostics de lecture et d’analyse des sources.', '用于标识源读取和源解析诊断所执行的边界。'),
        ('Do not replace a refused source with an empty program.', 'Ne pas remplacer une source refusée par un programme vide.', '不要以空程序替代被拒绝的源输入。'),
        ('Select the source, retain its path or format, and propagate the original failure.', 'Sélectionner la source, conserver son chemin ou son format et propager l’échec d’origine.', '选择源输入，保留其路径或格式，并传播原始故障。'),
        ('Successful source decoding does not establish semantic completeness.', 'Un décodage réussi de la source n’établit pas la complétude sémantique.', '成功解码源输入并不证明语义完整性。'),
        ('An unreadable source file fails before a theory is emitted.', 'Un fichier source illisible provoque un échec avant l’émission d’une théorie.', '无法读取的源文件会在输出理论之前导致失败。'),
    ]),
]

for slice_path, term, kind, failure, coat in TERMS:
    directory = ROOT / 'slices' / slice_path
    module = directory / 'module.ttl'
    source = module.read_text()
    if '\n' + term + '\n' in source:
        raise SystemExit('term already authored: ' + term)
    lines = ['', term, '    a ' + kind + ' ;']
    if failure:
        lines.append('    gmeow:enforcesFailureClass ' + failure + ' ;')
    if term == 'logic:CompilerSourceRefusal':
        lines.append('    logic:subClassOf logic:LogicConformanceFailure ;')
    owner = slice_path.split('/')[-1]
    lines.append('    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/' + owner + '> ;')
    for (predicate, _), strings in zip(PROPERTIES, coat, strict=True):
        lines.append('    ' + predicate + ' ' + json.dumps(strings[0], ensure_ascii=False) + '@x-gmeow-english ;')
    lines.append('    gmeow:graphBoxRole gmeow:' + ('boxTBox' if kind == 'logic:Class' else 'boxABox') + ' .')
    with module.open('a') as output:
        output.write('\n'.join(lines) + '\n')
    namespace, local = term.split(':', 1)
    iri = 'https://blackcatinformatics.ca/' + namespace + '/' + local
    for language, index in [('fr', 1), ('zh', 2)]:
        catalog = directory / 'i18n' / (language + '.po')
        catalog.parent.mkdir(exist_ok=True)
        if not catalog.exists():
            header = '# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>\n# SPDX-License-Identifier: CC-BY-4.0\n\nmsgid ""\nmsgstr ""\n'
            for value in ['Project-Id-Version: gmeow\n', 'Language: ' + language + '\n', 'MIME-Version: 1.0\n', 'Content-Type: text/plain; charset=UTF-8\n', 'Content-Transfer-Encoding: 8bit\n']:
                header += json.dumps(value) + '\n'
            catalog.write_text(header)
        with catalog.open('a') as output:
            for (_, predicate), strings in zip(PROPERTIES, coat, strict=True):
                output.write('\nmsgctxt ' + json.dumps(iri + '|' + predicate) + '\nmsgid ' + json.dumps(strings[0], ensure_ascii=False) + '\nmsgstr ' + json.dumps(strings[index], ensure_ascii=False) + '\n')
