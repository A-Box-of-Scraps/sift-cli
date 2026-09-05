import json
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest
from unittest.mock import patch

import run


class EvaluationTests(unittest.TestCase):
    def test_expected_locations_exist(self):
        cases = json.loads((run.ROOT / 'benchmarks/queries.json').read_text())
        self.assertLessEqual({'identifier', 'path', 'error-message', 'keyword',
                              'natural-language', 'filler'}, {c['category'] for c in cases})
        for case in cases:
            self.assertTrue(case['expected'])
            for expected in case['expected']:
                lines = (run.ROOT / 'benchmarks/corpus' / expected['path']).read_text().splitlines()
                self.assertGreaterEqual(expected['line'], 1)
                self.assertLessEqual(expected['line'], len(lines))

    def test_score_counts_locations_not_duplicate_hits(self):
        expected = [{'path': 'a', 'line': 2}, {'path': 'b', 'line': 1}]
        hits = [{'path': 'a', 'start_line': 1, 'end_line': 3}] * 10
        self.assertEqual(run.score(hits, expected, 5), {'hit': 1, 'recall': .5})
        self.assertEqual(run.score([], expected, 10), {'hit': 0, 'recall': 0})

    def test_query_schedule_is_repeatable_and_preserves_samples(self):
        queries = [{'query': 'first'}, {'query': 'second'}]
        schedule = run.query_schedule(queries, 5)
        self.assertEqual(schedule, run.query_schedule(queries, 5))
        self.assertCountEqual(schedule, queries * 5)
        self.assertNotEqual(schedule, queries * 5)
        self.assertEqual(run.query_schedule([], 5), [])

    def test_reference_requires_named_machine_conditions(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'reference.json'
            path.write_text('{}')
            with self.assertRaises(ValueError):
                run.reference_notes(path)
            notes = {key: 'recorded' for key in ('name', 'storage', 'power_mode', 'workloads')}
            path.write_text(json.dumps(notes))
            self.assertEqual(run.reference_notes(path), notes)

    def test_percentiles(self):
        result = run.summary([{'seconds': i, 'peak_rss_kib': i} for i in range(1, 21)])
        self.assertEqual(result['p50_seconds'], 10)
        self.assertEqual(result['p95_seconds'], 19)
        self.assertEqual(result['peak_rss_kib'], 20)

    def test_process_capture_and_errors(self):
        output, sample = run.run([sys.executable, '-c', 'print("hello")'])
        self.assertEqual(output, b'hello\n')
        self.assertEqual(sample['stdout_bytes'], 6)
        self.assertGreater(sample['peak_rss_kib'], 0)
        with self.assertRaises(RuntimeError):
            run.run([sys.executable, '-c', 'raise SystemExit(2)'])

    def test_lexical_baseline_uses_raw_text(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'db.sqlite'
            with sqlite3.connect(path) as db:
                db.executescript("CREATE TABLE files(id, path); CREATE TABLE chunks(id, file_id, start_line, end_line, text); INSERT INTO files VALUES(1, 'a.rs'); INSERT INTO chunks VALUES(1,1,1,1,'rotate_session_token');")
            db, locations = run.lexical_database(path)
            self.assertEqual(len(run.lexical_query(db, locations, 'rotate_session_token')), 1)
            self.assertEqual(run.lexical_query(db, locations, 'rotate'), [])
            db.close()

    def test_flamegraph_commands(self):
        from argparse import Namespace
        with tempfile.TemporaryDirectory() as directory:
            args = Namespace(output=Path(directory), profile_repetitions=12)
            with patch.object(run, 'run') as execute:
                run.profile(args, Path('/bin/sift'), Path('/corpus'), Path('/handle'), {},
                            [{'query': 'a b'}])
            self.assertEqual(execute.call_count, 2)
            first, second = [c.args[0] for c in execute.call_args_list]
            self.assertEqual(first[:2], ['flamegraph', '--output'])
            self.assertIn(str(args.output / 'index.svg'), first)
            self.assertIn(str(args.output / 'query.svg'), second)
            self.assertEqual(second[-2:], ['--repetitions', '12'])
            commands = json.loads((args.output / 'profile-queries.json').read_text())
            self.assertEqual(commands, [['/bin/sift', 'query', '/handle', 'a b']])



class ExperimentTests(unittest.TestCase):
    def test_diagnostic_tokens_and_boosts(self):
        import experiments
        self.assertEqual(experiments.terms('HTTPServer snake_case'),
                         ['case', 'http', 'httpserver', 'server', 'snake', 'snake_case'])
        candidates = [{'path': 'partial', 'snippet': 'rotate session token'},
                      {'path': 'exact', 'snippet': 'rotate_session_token'}]
        self.assertEqual(experiments.rerank(candidates, 'rotate_session_token', 'identifier')[0]['path'], 'exact')
        self.assertEqual(experiments.rerank(candidates, 'rotate session token', 'phrase')[0]['path'], 'partial')
        self.assertEqual(len(experiments.rerank(candidates, 'please show me', 'filler')), 2)

    def test_diversity_metrics(self):
        results = [{'path': 'a', 'snippet': 'a b c'}, {'path': 'b', 'snippet': 'a b c'},
                   {'path': 'b', 'snippet': 'different'}]
        self.assertEqual(run.diversity(results), {'near_duplicate_pairs_at_10': 1, 'max_hits_per_file_at_10': 2})
        self.assertEqual(run.diversity([])['max_hits_per_file_at_10'], 0)

    def test_variants_update_source_and_metadata_together(self):
        import experiments
        import shutil
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            shutil.copytree(run.ROOT / 'src', source / 'src')
            experiments.configure(source, {'lines': 16, 'overlap': 2, 'bytes': 1024,
                                          'candidates': 50, 'weight': 6.0, 'diversity': False})
            chunk = (source / 'src/chunk.rs').read_text()
            self.assertIn('lines=16;overlap=2;max_bytes=1024', chunk)
            self.assertIn('WINDOW_LINES: usize = 16', chunk)
            backend = (source / 'src/backend/sqlite.rs').read_text()
            self.assertIn('query.limit * 50', backend)
            self.assertIn('bm25(chunk_search, 6.0, 1.0)', backend)
            self.assertIn('if false && repeated_file', (source / 'src/query.rs').read_text())

    def test_legacy_variant_restores_streaming_unbounded_query(self):
        import experiments
        import shutil
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            shutil.copytree(run.ROOT / 'src', source / 'src')
            experiments.configure(source, {'legacy': True})
            backend = (source / 'src/backend/sqlite.rs').read_text()
            self.assertNotIn('LIMIT ?3', backend)
            self.assertNotIn('Ok(query::select', backend)
            self.assertIn('for candidate in candidates', backend)


class CacheProtocolTests(unittest.TestCase):
    def test_warmup_is_not_a_sample(self):
        with patch.object(run, 'run', return_value=(b'/snapshot\n', {'seconds': 1})) as execute:
            handle, samples = run.measure_index('/bin/sift', '/corpus', {}, 3, None)
        self.assertEqual(handle, Path('/snapshot'))
        self.assertEqual(len(samples), 3)
        self.assertEqual(execute.call_count, 4)

    def test_eviction_precedes_every_index_sample_without_shell(self):
        with patch.object(run, 'run', return_value=(b'/snapshot\n', {'seconds': 1})) as execute:
            _, samples = run.measure_index('/bin/sift', '/corpus', {}, 2, '/helper "argument with spaces"')
        self.assertEqual(len(samples), 2)
        self.assertEqual([c.args[0][0] for c in execute.call_args_list],
                         ['/helper', '/bin/sift', '/helper', '/bin/sift'])
        self.assertEqual(execute.call_args_list[0].args[0], ['/helper', 'argument with spaces'])

    def test_eviction_failure_aborts(self):
        with patch.object(run, 'run', side_effect=RuntimeError('eviction failed')):
            with self.assertRaises(RuntimeError):
                run.measure_index('/bin/sift', '/corpus', {}, 1, '/helper')


class MeasurementTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.binary = Path(cls.temporary.name) / 'measure'
        run.run(['clang', '-O2', '-Wall', '-Wextra', '-Werror',
                 str(run.ROOT / 'benchmarks/measure.c'), '-o', str(cls.binary)])

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_rss_does_not_include_python_driver_heap(self):
        heap = bytearray(64 * 1024 * 1024)
        with patch.object(run, 'MEASURE', self.binary):
            stdout, sample = run.run([sys.executable, '-c', 'print("measured")'])
        self.assertEqual(stdout, b'measured\n')
        self.assertGreater(sample['seconds'], 0)
        self.assertLess(sample['peak_rss_kib'], len(heap) // 1024)

    def test_status_and_capture_are_preserved(self):
        with patch.object(run, 'MEASURE', self.binary):
            output, _ = run.run([sys.executable, '-c', 'print("miss"); exit(1)'], allowed=(1,))
            self.assertEqual(output, b'miss\n')
            with self.assertRaises(RuntimeError):
                run.run([sys.executable, '-c', 'raise SystemExit(2)'])


if __name__ == '__main__':
    unittest.main()
