import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from progress import last_event_day, live_progress


class ProgressTests(unittest.TestCase):
    def test_partial_row_does_not_advance_progress(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/'events.jsonl'
            path.write_text('{"day":12.5}\n{"day":99')
            self.assertEqual(last_event_day(path), 12.5)

    def test_tail_is_bounded_and_ignores_a_partial_first_row(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/'events.jsonl'
            path.write_text('x'*100000+'\n{"day":25}\n')
            self.assertEqual(last_event_day(path, 32), 25)
            path.write_text('{"day":25,"detail":"'+'x'*100000+'"}\n')
            self.assertIsNone(last_event_day(path, 32))

    def test_missing_and_invalid_samples_are_not_live_evidence(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/'events.jsonl'
            self.assertIsNone(last_event_day(path))
            path.write_text('{"day":-1}\n{"day":true}\n{"day":NaN}\n[]\n')
            self.assertIsNone(last_event_day(path))

    def test_live_clock_keeps_verified_counts_and_cannot_regress(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/'runs/chunk-0002/events.jsonl'
            path.parent.mkdir(parents=True)
            plan = {'root': folder, 'jobs': [{'name': 'test', 'output': 'runs'}]}
            row = {'name': 'test', 'state': 'running', 'active_chunk': 2,
                   'days': 20, 'mortals': 9, 'births': 2, 'depth': 1}
            state = {'runs': [row]}
            path.write_text(json.dumps({'day': 24.75})+'\n')
            result = live_progress(plan, state)[0]
            self.assertEqual(result['live_day_lower_bound'], 24.75)
            self.assertEqual(result['verified_days'], 20)
            self.assertEqual(result['verified_mortals'], 9)
            path.write_text('{"day":2}\n')
            self.assertEqual(live_progress(plan, state)[0]['live_day_lower_bound'], 20)
            row['state'] = 'censored'
            path.write_text('{"day":99}\n')
            self.assertEqual(live_progress(plan, state)[0]['live_day_lower_bound'], 20)

    def test_model_phase_progress_preserves_absolute_clock_and_ancestry(self):
        plan = {'root': '/', 'jobs': [{'name': 'test', 'output': 'unused'}]}
        state = {'runs': [{'name': 'test', 'state': 'forked verified checkpoint',
                           'days': 1000, 'mortals': 38, 'births': 3, 'depth': 1}],
                 'model_forks': [{'checkpoints': [{'name': 'test', 'days': 900, 'depth': 1}]}]}
        row = live_progress(plan, state)[0]
        self.assertEqual(row['verified_days'], 1000)
        self.assertEqual(row['verified_depth'], 1)
        self.assertEqual(row['phase_start_days'], 900)
        self.assertEqual(row['verified_days_in_phase'], 100)
        self.assertEqual(row['live_days_in_phase'], 100)


if __name__ == '__main__':
    unittest.main()
