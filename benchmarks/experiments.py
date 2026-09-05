#!/usr/bin/env python3
"""Isolated release-build sweeps and diagnostic reranking ablations."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import time

import run

VARIANTS = {
    'default': {},
    'legacy': {'legacy': True},
    'overlap-only': {'diversity': False},
    'chunk-16-2': {'lines': 16, 'overlap': 2},
    'chunk-32-0': {'overlap': 0},
    'chunk-64-8': {'lines': 64, 'overlap': 8, 'bytes': 4096},
    'path-weight-1': {'weight': 1.0},
    'path-weight-6': {'weight': 6.0},
    'candidates-2': {'candidates': 2},
    'candidates-50': {'candidates': 50},
}
FILLER = set('where can i find the function that please show me how to a an is does code'.split())


def terms(text):
    result = []
    for token in re.findall(r'\w+', text):
        result.append(token.lower())
        for word in token.split('_'):
            parts = re.sub(r'([a-z0-9])([A-Z])', r'\1 \2', word)
            parts = re.sub(r'([A-Z])([A-Z][a-z])', r'\1 \2', parts).lower().split()
            result.extend(part for part in parts if part != token.lower())
    return sorted(set(result))


def rerank(results, text, mode):
    query_terms = set(terms(text))
    if mode == 'filler':
        query_terms -= FILLER
    query_terms = query_terms or set(terms(text))
    identifiers = {t.lower() for t in re.findall(r'\w+', text)
                   if '_' in t or any(c.isupper() for c in t[1:])}

    def key(result):
        raw = result['path'] + ' ' + result['snippet']
        coverage = len(query_terms & set(terms(raw))) / len(query_terms)
        exact = len(identifiers & set(re.findall(r'\w+', raw.lower())))
        phrase = int(' '.join(text.lower().split()) in ' '.join(raw.lower().split()))
        return {'coverage': (coverage,), 'identifier': (exact,),
                'phrase': (phrase,), 'filler': (coverage,),
                'combined': (exact, phrase, coverage)}[mode]

    return sorted(results, key=key, reverse=True)


def select(results, limit=10):
    chosen = []
    for result in results:
        if any(r['path'] == result['path'] and r['start_byte'] < result['end_byte']
               and result['start_byte'] < r['end_byte'] for r in chosen):
            continue
        chosen.append(result)
        if len(chosen) == limit:
            break
    return chosen


def ranking_experiments(database, queries, repetitions):
    modes = ('bm25', 'coverage', 'identifier', 'phrase', 'filler', 'combined')
    entries = []
    with sqlite3.connect(f'{database.as_uri()}?mode=ro', uri=True) as db:
        db.row_factory = sqlite3.Row
        for cap in (20, 100, 500):
            for mode in modes:
                cases, samples = [], []
                for query in queries:
                    query_terms = terms(query['query'])
                    if mode == 'filler':
                        query_terms = [t for t in query_terms if t not in FILLER] or query_terms
                    expression = ' OR '.join('"' + t + '"' for t in query_terms)
                    sql = '''SELECT f.path, c.start_line, c.end_line, c.start_byte,
                             c.end_byte, c.text AS snippet FROM chunk_search
                             JOIN chunks c ON c.id=chunk_search.rowid
                             JOIN files f ON f.id=c.file_id
                             WHERE chunk_search MATCH ?
                             ORDER BY bm25(chunk_search,3.0,1.0), c.id LIMIT ?'''
                    db.execute(sql, (expression, cap)).fetchall()
                    for _ in range(repetitions):
                        start = time.perf_counter()
                        candidates = [dict(row) for row in db.execute(sql, (expression, cap))]
                        results = select(candidates if mode == 'bm25' else rerank(candidates, query['query'], mode))
                        samples.append({'seconds': time.perf_counter() - start, 'peak_rss_kib': 0})
                    cases.append({'query': query['query'], 'category': query['category'],
                                  'top5': run.score(results, query['expected'], 5),
                                  'top10': run.score(results, query['expected'], 10),
                                  'candidates_returned': len(candidates), 'results': results})
                entries.append({'mode': mode, 'candidate_cap': cap, 'cases': cases,
                                'aggregate': {f'{metric}@{k}': sum(c[f'top{k}'][metric] for c in cases) / len(cases)
                                              for k in (5, 10) for metric in ('hit', 'recall')},
                                'diagnostic_python_sqlite_latency': run.summary(samples)})
    return entries


def configure(source, variant):
    chunk = source / 'src/chunk.rs'
    text = chunk.read_text()
    lines, overlap, size = (variant.get(k, v) for k, v in [('lines', 32), ('overlap', 4), ('bytes', 2048)])
    text = text.replace('WINDOW_LINES: usize = 32', f'WINDOW_LINES: usize = {lines}')
    text = text.replace('OVERLAP_LINES: usize = 4', f'OVERLAP_LINES: usize = {overlap}')
    text = text.replace('MAX_CHUNK_BYTES: usize = 2048', f'MAX_CHUNK_BYTES: usize = {size}')
    chunk.write_text(text.replace('lines=32;overlap=4;max_bytes=2048', f'lines={lines};overlap={overlap};max_bytes={size}'))
    backend = source / 'src/backend/sqlite.rs'
    text = backend.read_text().replace('bm25(chunk_search, 3.0, 1.0)', f"bm25(chunk_search, {variant.get('weight', 3.0)}, 1.0)")
    text = text.replace('query.limit * 10', f"query.limit * {variant.get('candidates', 10)}")
    if variant.get('legacy'):
        text = text.replace(' LIMIT ?3', '').replace(', query.limit * 10', '')
        text = text.replace('    let candidates = candidates.collect::<rusqlite::Result<Vec<_>>>()?;\n    Ok(query::select(candidates, query.limit))', '''    let mut results = Vec::new();
    for candidate in candidates {
        let candidate = candidate?;
        if !query::overlaps(&candidate, &results) {
            results.push(candidate);
            if results.len() == query.limit {
                break;
            }
        }
    }
    Ok(results)''')
    backend.write_text(text)
    if variant.get('diversity') is False:
        query = source / 'src/query.rs'
        text = query.read_text().replace('if repeated_file', 'if false && repeated_file').replace('|| results', '|| false && results')
        query.write_text(text)


def write_summary(output):
    experiments = json.loads((output / 'experiments.json').read_text())
    baseline = json.loads((output / 'default/report.json').read_text())
    def compact(timing):
        return {key: value for key, value in timing.items() if key != 'samples'}
    summary = {'schema_version': 1, 'scope': 'Synthetic, explicitly warmed, sequential variants; not a cold-cache claim',
               'environment': baseline['environment'], 'corpus': baseline['corpus'],
               'variants': {}, 'ranking': [], 'relevance': {},
               'info_startup': compact(baseline['info_startup']),
               'first_query': baseline['first_query']}
    for name, report in experiments['variants'].items():
        summary['variants'][name] = {
            'configuration': report['variant'], 'build': report['build'],
            'source_sha256': report['source_sha256'],
            'binary_sha256': report['environment']['binary_sha256'],
            'report_sha256': hashlib.sha256((output / name / 'report.json').read_bytes()).hexdigest(),
            'aggregate': report['aggregate'], 'index': compact(report['index']),
            'index_repetitions': len(report['index']['samples']),
            'query_repetitions': len(next(iter(report['query'].values()))['samples']),
            'database_bytes': report['database_bytes'],
            'query': {'pooled_suite': compact(run.summary([sample for timing in report['query'].values() for sample in timing['samples']])),
                      'pub const': compact(report['query']['pub const']),
                      'siftnomatchsentinelzz': compact(report['query']['siftnomatchsentinelzz'])}}
        if name in ('default', 'legacy'):
            full = json.loads((output / name / 'report.json').read_text())
            summary['relevance'][name] = [
                {'query': case['query'], 'category': case['category'],
                 'sift': {k: v for k, v in case['engines']['sift'].items() if k != 'results'},
                 'sift_text_bytes': case['sift_text_bytes'], 'sift_json_bytes': case['sift_json_bytes'],
                 'rg_json_bytes_all_matches': case['rg_json_bytes_all_matches']}
                for case in full['relevance']]
    for entry in experiments['ranking']:
        summary['ranking'].append({
            'mode': entry['mode'], 'candidate_cap': entry['candidate_cap'],
            'aggregate': entry['aggregate'],
            'diagnostic_python_sqlite_latency': compact(entry['diagnostic_python_sqlite_latency'])})
    (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repetitions', type=int, default=20)
    parser.add_argument('--lines', type=int, default=100000)
    parser.add_argument('--reference', type=Path)
    args = parser.parse_args()
    if args.repetitions < 1 or args.lines < 0:
        parser.error('repetitions must be positive and lines nonnegative')
    if args.reference:
        try:
            run.reference_notes(args.reference)
        except (OSError, ValueError) as error:
            parser.error(str(error))
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    reports = {}
    for name, variant in VARIANTS.items():
        source = output / 'sources' / name
        source.mkdir(parents=True)
        for directory in ('src', '.cargo'):
            shutil.copytree(run.ROOT / directory, source / directory)
        for file in ('Cargo.toml', 'Cargo.lock'):
            shutil.copy2(run.ROOT / file, source / file)
        configure(source, variant)
        env = {**os.environ, 'CARGO_PROFILE_RELEASE_DEBUG': '1'}
        run.run(['cargo', 'rustc', '--release', '--locked', '--bin', 'sift',
                 '--target-dir', str(output / 'build'), '--', '-C', 'link-arg=-Wl,--no-rosegment'], cwd=source, env=env)
        binary = output / 'binaries' / name
        binary.parent.mkdir(exist_ok=True)
        shutil.copy2(output / 'build/release/sift', binary)
        command = [os.sys.executable, str(run.ROOT / 'benchmarks/run.py'), '--output', str(output / name),
                   '--binary', str(binary), '--repetitions', str(args.repetitions), '--lines', str(args.lines)]
        if args.reference:
            command += ['--reference', str(args.reference.resolve())]
        run.run(command)
        report = json.loads((output / name / 'report.json').read_text())
        reports[name] = {key: report[key] for key in ('corpus', 'aggregate', 'index', 'database_bytes', 'query', 'environment')}
        reports[name]['variant'] = variant
        reports[name]['build'] = {'profile': 'release', 'debug': 1, 'link_args': ['-Wl,--no-rosegment']}
        reports[name]['binary'] = str(binary.relative_to(output))
        reports[name]['source_sha256'] = hashlib.sha256(b''.join(
            p.relative_to(source).as_posix().encode() + b'\0' + p.read_bytes()
            for p in sorted(source.rglob('*')) if p.is_file())).hexdigest()
        print(name, report['aggregate']['sift'], flush=True)
    databases = sorted((output / 'default/data').rglob('db.sqlite'))
    queries = json.loads((run.ROOT / 'benchmarks/queries.json').read_text())
    ranking = ranking_experiments(databases[-1], queries, args.repetitions)
    (output / 'experiments.json').write_text(json.dumps({'variants': reports, 'ranking': ranking}, indent=2) + '\n')
    write_summary(output)


if __name__ == '__main__':
    main()
