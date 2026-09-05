#!/usr/bin/env python3
"""Linux-only, standard-library evaluation driver. See docs/BENCHMARKS.md."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def run(command, *, cwd=ROOT, env=None, allowed=(0,)):
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        start = time.perf_counter()
        child = subprocess.Popen(command, cwd=cwd, env=env, stdout=out, stderr=err)
        _, status, usage = os.wait4(child.pid, 0)
        child.returncode = os.waitstatus_to_exitcode(status)
        elapsed = time.perf_counter() - start
        out.seek(0)
        err.seek(0)
        stdout, stderr = out.read(), err.read()
    if child.returncode not in allowed:
        raise RuntimeError(f"{command!r}: {stderr.decode(errors='replace')}")
    return stdout, {"seconds": elapsed, "peak_rss_kib": usage.ru_maxrss,
                    "stdout_bytes": len(stdout)}


def summary(samples):
    values = sorted(sample['seconds'] for sample in samples)
    return {"samples": samples, "p50_seconds": values[math.ceil(len(values) * .5) - 1],
            "p95_seconds": values[math.ceil(len(values) * .95) - 1],
            "peak_rss_kib": max(s['peak_rss_kib'] for s in samples)}


def score(results, expected, k):
    found = sum(any(r['path'] == e['path'] and
                    r['start_line'] <= e['line'] <= r['end_line']
                    for r in results[:k]) for e in expected)
    return {"hit": int(found > 0), "recall": found / len(expected)}


def lexical_database(source):
    connection = sqlite3.connect(':memory:')
    connection.execute("CREATE VIRTUAL TABLE lexical USING fts5(path, body, tokenize = 'unicode61 remove_diacritics 0 tokenchars _')")
    with sqlite3.connect(f'{source.as_uri()}?mode=ro', uri=True) as db:
        rows = db.execute('SELECT c.id, f.path, c.start_line, c.end_line, c.text FROM chunks c JOIN files f ON f.id=c.file_id ORDER BY c.id').fetchall()
    locations = {}
    for ident, path, start, end, text in rows:
        connection.execute('INSERT INTO lexical(rowid,path,body) VALUES (?,?,?)', (ident, path, text))
        locations[ident] = {'path': path, 'start_line': start, 'end_line': end, 'snippet': text}
    return connection, locations


def lexical_query(connection, locations, text):
    terms = re.findall(r'\w+', text, re.UNICODE)
    expression = ' OR '.join('"' + term + '"' for term in terms)
    rows = connection.execute('SELECT rowid FROM lexical WHERE lexical MATCH ? ORDER BY bm25(lexical), rowid LIMIT 10', (expression,))
    return [locations[row[0]] for row in rows]


def rg_query(text, corpus):
    output, _ = run(['rg', '--no-config', '--json', '--sort', 'path', '--fixed-strings', '--', text, '.'], cwd=corpus, allowed=(0, 1))
    results = []
    for line in output.splitlines():
        event = json.loads(line)
        if event['type'] == 'match':
            data = event['data']
            results.append({'path': data['path']['text'].removeprefix('./'),
                            'start_line': data['line_number'], 'end_line': data['line_number'],
                            'snippet': data['lines']['text']})
    return results, len(output)


def profile(args, binary, corpus, handle, env, queries):
    base = ['flamegraph', '--output']
    run(base + [str(args.output / 'index.svg'), '--', str(binary), 'index', '--root', str(corpus), '.'], cwd=args.output, env=env)
    workload = args.output / 'profile-queries.json'
    workload.write_text(json.dumps([[str(binary), 'query', str(handle), q['query']] for q in queries]))
    run(base + [str(args.output / 'query.svg'), '--', sys.executable, str(Path(__file__).resolve()),
                '--profile-worker', str(workload), '--repetitions', str(args.profile_repetitions)], cwd=args.output, env=env)


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'target/evaluation')
    parser.add_argument('--repetitions', type=int, default=10)
    parser.add_argument('--lines', type=int, default=100000)
    parser.add_argument('--flamegraphs', action='store_true')
    parser.add_argument('--profile-repetitions', type=int, default=100)
    parser.add_argument('--profile-worker', type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.repetitions < 1 or args.lines < 0 or args.profile_repetitions < 1:
        parser.error('repetitions must be positive and lines nonnegative')
    if not args.profile_worker and args.flamegraphs and not shutil.which('flamegraph'):
        parser.error('install flamegraph with: cargo install flamegraph --locked')
    return args


def profile_worker(args):
    commands = json.loads(args.profile_worker.read_text())
    for _ in range(args.repetitions):
        for command in commands:
            subprocess.run(command, stdout=subprocess.DEVNULL, check=True)


def query_schedule(queries, repetitions):
    entries = enumerate(queries * repetitions)
    ordered = sorted(entries, key=lambda entry: hashlib.sha256(str(entry[0]).encode()).digest())
    return [query for _, query in ordered]


def prepare_corpus(corpus, lines):
    shutil.copytree(ROOT / 'benchmarks/corpus', corpus)
    remaining = lines
    for number in range(math.ceil(remaining / 1000)):
        count = min(remaining, 1000)
        (corpus / f'generated_{number:04}.rs').write_text(''.join(
            f'pub const RECORD_{number}_{i}: usize = {i};\n' for i in range(count)))
        remaining -= count
    files = sorted(p for p in corpus.rglob('*') if p.is_file())
    digest = hashlib.sha256()
    for path in files:
        digest.update(path.relative_to(corpus).as_posix().encode() + b'\0' + path.read_bytes() + b'\0')
    return {'files': len(files), 'bytes': sum(p.stat().st_size for p in files),
            'lines': sum(len(p.read_bytes().splitlines()) for p in files), 'sha256': digest.hexdigest()}


def evaluate_relevance(binary, handle, corpus, queries, connection, locations):
    entries = []
    for query in queries:
        command = [str(binary), 'query', str(handle), query['query'], '--limit', '10']
        stdout, _ = run(command + ['--json'])
        sift = json.loads(stdout)['results']
        lexical = lexical_query(connection, locations, query['query'])
        rg, rg_bytes = rg_query(query['query'], corpus)
        entry = dict(query)
        entry['engines'] = {}
        for name, results in [('sift', sift), ('lexical', lexical), ('rg', rg)]:
            entry['engines'][name] = {'top5': score(results, query['expected'], 5),
                                      'top10': score(results, query['expected'], 10),
                                      'results': results[:10],
                                      'unique_files_at_10': len({r['path'] for r in results[:10]}),
                                      'unique_snippets_at_10': len({r['snippet'].strip() for r in results[:10]}),
                                      'normalized_top10_bytes': len(json.dumps([
                                          {key: r[key] for key in ('path', 'start_line', 'end_line', 'snippet')}
                                          for r in results[:10]], ensure_ascii=False).encode())}
        entry['sift_text_bytes'] = len(run(command)[0])
        entry['sift_json_bytes'] = len(stdout)
        entry['rg_json_bytes_all_matches'] = rg_bytes
        entries.append(entry)
    return entries


def main():
    args = parse_args()
    if args.profile_worker:
        profile_worker(args)
        return
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    build_env = {**os.environ, 'CARGO_PROFILE_RELEASE_DEBUG': '1'}
    run(['cargo', 'rustc', '--bin', 'sift', '--release', '--locked', '--target-dir',
         str(args.output / 'build'), '--', '-C', 'link-arg=-Wl,--no-rosegment'], env=build_env)
    binary = args.output / 'build/release/sift'
    corpus = args.output / 'corpus'
    corpus_info = prepare_corpus(corpus, args.lines)
    env = {**os.environ, 'XDG_DATA_HOME': str(args.output / 'data')}
    queries = json.loads((ROOT / 'benchmarks/queries.json').read_text())
    report = {'schema_version': 1, 'environment': {
        'platform': platform.platform(), 'cpu': Path('/proc/cpuinfo').read_text().split('model name', 1)[-1].split('\n', 1)[0].strip(' :\t'),
        'memory': Path('/proc/meminfo').read_text().splitlines()[0],
        'rustc': run(['rustc', '-Vv'])[0].decode(), 'rg': run(['rg', '--version'])[0].decode(),
        'python': sys.version, 'sqlite': sqlite3.sqlite_version,
        'revision': run(['git', 'rev-parse', 'HEAD'])[0].decode().strip(),
        'git_status': run(['git', 'status', '--porcelain'])[0].decode(),
        'cache': 'OS cache uncontrolled; first query recorded separately; timed queries explicitly warmed',
        'release_debug': 1, 'binary_link_args': ['-Wl,--no-rosegment'],
        'cargo_config': (ROOT / '.cargo/config.toml').read_text(),
        'rustflags': os.environ.get('RUSTFLAGS'),
        'filesystem': run(['stat', '-f', '-c', '%T', str(args.output)])[0].decode().strip()},
        'corpus': corpus_info,
        'relevance': [], 'query': {}}
    index_samples = []
    for _ in range(args.repetitions):
        stdout, sample = run([str(binary), 'index', '--root', str(corpus), '.'], env=env)
        handle = Path(os.fsdecode(stdout).strip())
        index_samples.append(sample)
    report['index'] = summary(index_samples)
    report['database_bytes'] = (handle / 'db.sqlite').stat().st_size
    report['info_startup'] = summary([run([str(binary), 'info', str(handle)])[1] for _ in range(args.repetitions)])
    report['first_query'] = run([str(binary), 'query', str(handle), queries[0]['query']])[1]
    connection, locations = lexical_database(handle / 'db.sqlite')
    report['relevance'] = evaluate_relevance(binary, handle, corpus, queries, connection, locations)
    connection.close()
    report['query'] = {query['query']: [] for query in queries}
    performance_queries = queries + [
        {'query': 'pub const', 'category': 'high-fanout'},
        {'query': 'siftnomatchsentinelzz', 'category': 'no-match'},
    ]
    for query in performance_queries[len(queries):]:
        run([str(binary), 'query', str(handle), query['query'], '--limit', '10'])
        report['query'][query['query']] = []
    report['performance_queries'] = performance_queries
    schedule = query_schedule(performance_queries, args.repetitions)
    for query in schedule:
        _, sample = run([str(binary), 'query', str(handle), query['query'], '--limit', '10'])
        report['query'][query['query']].append(sample)
    report['query'] = {q: summary(samples) for q, samples in report['query'].items()}
    report['aggregate'] = {engine: {f'{metric}@{k}': sum(e['engines'][engine][f'top{k}'][metric] for e in report['relevance']) / len(queries)
                                   for k in (5, 10) for metric in ('hit', 'recall')}
                           for engine in ('sift', 'lexical', 'rg')}
    (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    if args.flamegraphs:
        profile(args, binary, corpus, handle, env, performance_queries)
    print(args.output / 'report.json')


if __name__ == '__main__':
    main()
