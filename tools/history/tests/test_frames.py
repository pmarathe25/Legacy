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
import frames
from decisions import FINAL_FIELDS


class FrameStudyTests(unittest.TestCase):
    def setup_case(self, root):
        binary = root/'binary'
        binary.write_bytes(b'frozen candidate')
        source = root/'fixture'
        source.mkdir()
        for name in frames.ARTIFACTS[:2]:
            (source/name).write_text('{}')
        (source/'checkpoint-ledger.json').write_text('{"counts":{"physiology.fed":5}}')
        (source/'summary.json').write_text('{"days":10}')
        return SimpleNamespace(
            binary=binary, control_binary=None, control_frame_binary=None, control_recovery_binary=None, control_decisions_binary=None, output=root/'study', fixtures=root,
            cases=['test'], workers=[1, 4], models=['ordinary', 'frame', 'recovery', 'recovery-food'],
            minutes=1, milliseconds=10000, rounds=2, verify_reload=True,
            resident_executor=True, active_executor=False, span_days=1, discard_verified=True)

    def measure(self, command, environment, log, corrupt=False):
        folder = Path(command[command.index('--output')+1])
        recovery = '--frame-recovery' in command
        policy = [recovery, '--frame-food' in command, '--decision-frame-millis' in command, '--frame-decisions' in command]
        if corrupt and recovery and command[command.index('--workers')+1] == '4':
            policy.append('worker-dependent')
        for name in frames.ARTIFACTS + ['initial.json', 'daily.jsonl']:
            (folder/name).write_text(json.dumps({'policy': policy}))
        state = dict.fromkeys(FINAL_FIELDS, 0)
        state.update(alive=1, actions={'rest': 1}, ledger={'events': {'physiology.fed': 7}})
        days = int(command[command.index('--days')+1])
        (folder/'summary.json').write_text(json.dumps({'final': state, 'days': days, 'extinct': False}))
        for name, day in [('initial.json', 10), ('daily.jsonl', days)]:
            (folder/name).write_text(json.dumps({'day': day, 'alive': 1, 'hungry': 0,
                                               'injured': 0, 'actions': {'rest': 1}}))
        return {'wall_seconds': 1.0, 'cpu_seconds': 1.5, 'rss_kib': 10}

    def test_models_are_checked_separately_and_short_runs_record_their_horizon(self):
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
            args = self.setup_case(Path(temp))
            with patch.object(frames, 'CASES', {'test': ('fixture', 12, 11)}), \
                    patch.object(frames, 'measure', side_effect=self.measure) as measure:
                frames.run(args)
            report = json.loads((args.output/'comparison.json').read_text())
            self.assertTrue(report['complete'])
            case = report['cases']['test']
            self.assertEqual(case['days'], 11)
            self.assertIn('summary.json', case['inputs'])
            self.assertEqual(measure.call_count, 14)
            self.assertEqual({r['group'] for r in case['runs']}, {'ordinary', 'frame', 'recovery', 'recovery-food'})
            self.assertFalse(list(args.output.rglob('checkpoint.json')))
            outcome = case['runs'][0]['behavior']
            self.assertEqual(outcome['events']['physiology.fed'], 2)
            self.assertEqual(outcome['daily_sampled_person_days']['acting'], 1)

    def test_worker_divergence_fails_and_retains_the_mismatching_world(self):
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
            args = self.setup_case(Path(temp))
            with patch.object(frames, 'CASES', {'test': ('fixture', 12, 11)}), \
                    patch.object(frames, 'measure', side_effect=lambda *a: self.measure(*a, corrupt=True)):
                with self.assertRaisesRegex(ValueError, 'replay mismatch within recovery'):
                    frames.run(args)
            report = json.loads((args.output/'comparison.json').read_text())
            self.assertFalse(report['complete'])
            self.assertTrue((args.output/'test/01-recovery-4/checkpoint.json').is_file())

    def test_production_baseline_is_timed_only_for_ordinary_histories(self):
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
            args = self.setup_case(Path(temp))
            args.baseline_binary = Path(temp)/'production'
            args.baseline_binary.write_bytes(b'production runtime')
            with patch.object(frames, 'CASES', {'test': ('fixture', 12, 11)}), \
                    patch.object(frames, 'measure', side_effect=self.measure) as measure:
                frames.run(args)
            for call in measure.call_args_list:
                command = call.args[0]
                expected = args.binary if '--decision-frame-millis' in command else args.baseline_binary
                self.assertEqual(Path(command[0]), expected)
            report = json.loads((args.output/'comparison.json').read_text())
            self.assertEqual(report['binaries']['baseline']['sha256'], frames.digest(args.baseline_binary))

    def test_demand_clock_and_extended_horizons_record_outcomes_and_verify_reload(self):
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
            args = self.setup_case(Path(temp))
            args.span_days = None
            args.end_day = 14
            args.demand_clock = True
            args.verify_workers = 1
            with patch.object(frames, 'CASES', {'test': ('fixture', 12, 11)}), \
                    patch.object(frames, 'measure', side_effect=self.measure) as measure:
                frames.run(args)
            report = json.loads((args.output/'comparison.json').read_text())
            self.assertEqual(report['cases']['test']['days'], 14)
            self.assertTrue(report['demand_clock'])
            for call in measure.call_args_list:
                command = call.args[0]
                self.assertEqual('--frame-demand' in command, '--decision-frame-millis' in command)
            reloads = report['cases']['test']['verification']
            self.assertTrue(reloads)
            self.assertTrue(all(r['reload_day'] == 12 for r in reloads))
            self.assertTrue(all(r['workers'] == 1 for r in reloads))

    def test_an_early_stop_cannot_count_as_a_speedup(self):
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
            args = self.setup_case(Path(temp))

            def early(command, environment, log):
                result = self.measure(command, environment, log)
                folder = Path(command[command.index('--output')+1])
                path = folder/'summary.json'
                summary = json.loads(path.read_text())
                summary['days'] -= 1
                path.write_text(json.dumps(summary))
                return result

            with patch.object(frames, 'CASES', {'test': ('fixture', 12, 11)}), \
                    patch.object(frames, 'measure', side_effect=early):
                with self.assertRaisesRegex(ValueError, 'requested horizon'):
                    frames.run(args)
            self.assertTrue((args.output/'test/01-ordinary-1/checkpoint.json').exists())

    def test_actor_only_uses_the_same_decision_model_without_nested_fanout(self):
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
            args = self.setup_case(Path(temp))
            args.models = ['recovery-decisions']
            args.actor_only = True
            args.minimum_cohort = 8
            args.commit_batch = 64
            with patch.object(frames, 'CASES', {'test': ('fixture', 12, 11)}), \
                    patch.object(frames, 'measure', side_effect=self.measure) as measure:
                frames.run(args)
            report = json.loads((args.output/'comparison.json').read_text())
            self.assertTrue(report['actor_only'])
            self.assertEqual(report['minimum_cohort'], 8)
            self.assertEqual(report['commit_batch'], 64)
            self.assertTrue(all('--frame-minimum-cohort' in call.args[0] for call in measure.call_args_list))
            self.assertTrue(all('--frame-commit-batch' in call.args[0] for call in measure.call_args_list))
            self.assertTrue(all(not r['parallel_listeners'] for r in report['cases']['test']['runs']))

    def test_whole_decisions_keep_distinct_models_controls_and_median_groups(self):
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
            args = self.setup_case(Path(temp))
            args.models = ['recovery', 'decisions', 'recovery-decisions']
            args.control_recovery_binary = args.binary
            args.control_decisions_binary = args.binary

            def measured(command, environment, log):
                result = self.measure(command, environment, log)
                if '--frame-recovery' in command:
                    result['wall_seconds'] = 10.0
                return result

            with patch.object(frames, 'CASES', {'test': ('fixture', 12, 11)}), \
                    patch.object(frames, 'measure', side_effect=measured):
                frames.run(args)
            case = json.loads((args.output/'comparison.json').read_text())['cases']['test']
            self.assertEqual(case['median']['decisions-1']['wall_seconds'], 1.0)
            self.assertEqual(case['median']['recovery-decisions-1']['wall_seconds'], 10.0)
            self.assertEqual(case['verification'][0]['label'], 'previous-recovery')
            self.assertEqual(case['verification'][1]['label'], 'previous-decisions')
            self.assertTrue(all(r['parallel_listeners'] for r in case['runs'] if r['group'] == 'recovery-decisions'))
            self.assertNotEqual(case['runs'][2]['hashes'], case['runs'][4]['hashes'])
