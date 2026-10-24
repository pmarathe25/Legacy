import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import appraisals


class AppraisalStudyTests(unittest.TestCase):
    def setup_case(self, root):
        binary = root/'binary'
        binary.write_bytes(b'fixed executable')
        source = root/'fixture'
        source.mkdir()
        for key in appraisals.ARTIFACTS[:2]:
            (source/key).write_text('{}')
        return SimpleNamespace(binary=binary, control_binary=None, output=root/'study',
                               fixtures=root, pairs=1, resume=False)

    def measure(self, command, environment, log):
        folder = Path(command[command.index('--output')+1])
        for key in appraisals.ARTIFACTS+['initial.json', 'daily.jsonl']:
            (folder/key).write_text('{}')
        return {'wall_seconds': 1.0, 'cpu_seconds': 0.9, 'rss_kib': 10}

    def test_recovery_keeps_completed_samples_and_discards_the_incomplete_attempt(self):
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
            args = self.setup_case(Path(temp))
            calls = 0
            def interrupted(command, environment, log):
                nonlocal calls
                calls += 1
                if calls == 3:
                    raise RuntimeError('interrupted verification')
                return self.measure(command, environment, log)
            with patch.object(appraisals, 'CASES', [('test', 'fixture', 2, 1)]):
                with patch.object(appraisals, 'measure', interrupted):
                    with self.assertRaisesRegex(RuntimeError, 'interrupted'):
                        appraisals.run(args)
                args.resume = True
                with patch.object(appraisals, 'measure', wraps=self.measure) as replay:
                    appraisals.run(args)
                    self.assertEqual(replay.call_count, 2)
                report = json.loads((args.output/'comparison.json').read_text())
                self.assertTrue(report['all_equal'])
                self.assertEqual(len(report['cases']['test']['runs']), 2)
                self.assertEqual(len(report['cases']['test']['discarded_partials']), 1)
                args.binary.write_bytes(b'changed executable')
                with self.assertRaisesRegex(ValueError, 'provenance'):
                    appraisals.run(args)

    def test_a_changed_world_never_becomes_equivalence_evidence(self):
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
            args = self.setup_case(Path(temp))
            def changed(command, environment, log):
                metrics = self.measure(command, environment, log)
                if environment['LEGACY_APPRAISAL_CACHE'] == '1':
                    folder = Path(command[command.index('--output')+1])
                    (folder/'checkpoint.json').write_text('{"changed":true}')
                return metrics
            with patch.object(appraisals, 'CASES', [('test', 'fixture', 2, 1)]), \
                    patch.object(appraisals, 'measure', changed):
                with self.assertRaisesRegex(ValueError, 'replay mismatch'):
                    appraisals.run(args)
            report = json.loads((args.output/'comparison.json').read_text())
            self.assertNotIn('all_equal', report)
