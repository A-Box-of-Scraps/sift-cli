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


if __name__ == '__main__':
    unittest.main()
