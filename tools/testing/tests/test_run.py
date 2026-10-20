import importlib.util
import argparse
import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('runner', Path(__file__).parents[1] / 'run.py')
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class RunnerTests(unittest.TestCase):
    def test_inventory_selects_cargo_test_artifacts_and_keeps_package_working_directory(self):
        import json
        row = {'reason': 'compiler-artifact', 'profile': {'test': True}, 'executable': '/tmp/tests',
               'target': {'kind': ['test'], 'name': 'replay'}, 'package_id': 'fixture',
               'manifest_path': '/repo/crates/fixture/Cargo.toml'}
        ignored = dict(row, profile={'test': False}, executable='/tmp/tool')
        rows = runner.inventory('\n'.join(json.dumps(r) for r in [row, ignored, row]))
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]['cwd'], '/repo/crates/fixture')

    def test_counts_reject_missing_filtered_crashed_or_incomplete_results(self):
        result = 'test result: ok. 7 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.02s'
        self.assertEqual(runner.totals(result, 8), {'passed': 7, 'failed': 0, 'ignored': 1})
        for output, count in [('', 8), (result, 9), (result.replace('0 filtered', '1 filtered'), 8), (result * 2, 8)]:
            with self.assertRaises(ValueError):
                runner.totals(output, count)

    def test_failing_and_panicking_harnesses_cannot_be_reported_as_passes(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)
            script = path / 'fixture'
            for body in ['exit 1', 'echo "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out;"; exit 101',
                         'echo "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;"; exit 1']:
                script.write_text('#!/bin/sh\n' + body + '\n')
                script.chmod(0o755)
                row = runner.execute({'executable': str(script), 'cwd': folder, 'tests': 1}, path, 1)
                self.assertFalse(row['ok'])

    def test_concurrency_must_be_bounded_by_positive_values(self):
        for value in ['0', '-1']:
            with self.assertRaises(Exception):
                runner.positive(value)
        self.assertEqual(runner.positive('2'), 2)

    def test_cargo_build_and_doctests_use_the_limit_and_run_all_listed_tests(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            harness = root / 'harness'
            harness.write_text('#!/bin/sh\nif [ "$1" = "--list" ]; then echo "example: test"; '
                               'else echo "test result: ok. 1 passed; 0 failed; 0 ignored; '
                               '0 measured; 0 filtered out;"; fi\n')
            harness.chmod(0o755)
            artifact = {'reason': 'compiler-artifact', 'profile': {'test': True},
                        'executable': str(harness), 'target': {'kind': ['test'], 'name': 'example'},
                        'package_id': 'fixture', 'manifest_path': str(root / 'Cargo.toml')}
            cargo = root / 'cargo'
            cargo.write_text(f'#!{sys.executable}\nimport json,sys\n'
                             f'with open({str(root / "calls.jsonl")!r}, "a") as f: '
                             'f.write(json.dumps(sys.argv[1:])+"\\n")\n'
                             f'if "--no-run" in sys.argv: print({json.dumps(artifact)!r})\n')
            cargo.chmod(0o755)
            args = argparse.Namespace(output=root / 'report', package=[], build_jobs=1,
                                      jobs=1, test_threads=1)
            with patch.object(runner, 'ROOT', root), patch.dict(os.environ, {'PATH': f'{root}:{os.environ["PATH"]}'}), contextlib.redirect_stdout(io.StringIO()):
                self.assertTrue(runner.run(args))
            calls = [json.loads(line) for line in (root / 'calls.jsonl').read_text().splitlines()]
            self.assertEqual(len(calls), 2)
            self.assertIn('--no-run', calls[0])
            self.assertIn('--doc', calls[1])
            self.assertIn('--test-threads=1', calls[1])
            for call in calls:
                self.assertEqual(call[call.index('--jobs') + 1], '1')
            report = json.loads((root / 'report/report.json').read_text())
            self.assertEqual(report['totals'], {'passed': 1, 'failed': 0, 'ignored': 0})
            self.assertEqual(report['listed_tests'], 1)


if __name__ == '__main__':
    unittest.main()
