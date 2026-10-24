import json
from pathlib import Path
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import batch


class IndependentHistories(unittest.TestCase):
    def test_overlap_is_bounded_and_results_retain_input_order(self):
        active = peak = 0
        lock = threading.Lock()
        def measure(command, environment, log):
            nonlocal active, peak
            with lock:
                active += 1
                peak = max(peak, active)
            time.sleep(.02)
            with lock:
                active -= 1
            return command[0]
        with tempfile.TemporaryDirectory() as temporary:
            rows = []
            for i in range(6):
                folder = Path(temporary)/str(i); folder.mkdir()
                rows.append({'folder': folder, 'command': [i]})
            with patch.object(batch, 'measure', measure):
                _, results = batch.execute_group(rows, {}, 2)
            self.assertEqual(results, list(range(6)))
            self.assertEqual(peak, 2)

    def test_failure_requests_stops_for_all_siblings(self):
        with tempfile.TemporaryDirectory() as temporary:
            rows = []
            for i in range(3):
                folder = Path(temporary)/str(i); folder.mkdir()
                rows.append({'folder': folder, 'command': [i]})
            with patch.object(batch, 'measure', side_effect=RuntimeError('child failed')):
                with self.assertRaisesRegex(RuntimeError, 'child failed'):
                    batch.execute_group(rows, {}, 2)
            self.assertTrue(all((r['folder']/'stop-request').exists() for r in rows))

    def test_duplicate_or_unsafe_job_names_are_rejected_before_launch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root/'checkpoint'; source.mkdir()
            for artifact in batch.ARTIFACTS[:2]: (source/artifact).write_text('{}')
            plan = root/'plan.json'
            for names in [('same', 'same'), ('../escape',), ('/absolute',), ('..',)]:
                plan.write_text(json.dumps({'binary': 'engine', 'runs': [
                    {'name': n, 'checkpoint': 'checkpoint', 'days': 330} for n in names]}))
                with self.assertRaises(ValueError): batch.parse_plan(plan)

    def test_later_child_failure_stops_an_earlier_running_child_promptly(self):
        with tempfile.TemporaryDirectory() as temporary:
            rows = []
            for i in range(2):
                folder = Path(temporary)/str(i); folder.mkdir()
                rows.append({'folder': folder, 'command': [i]})
            first_started = threading.Event()
            def measure(command, environment, log):
                if command[0] == 1:
                    self.assertTrue(first_started.wait(timeout=2))
                    raise RuntimeError('second child failed')
                first_started.set()
                deadline = time.monotonic()+2
                while not (rows[0]['folder']/'stop-request').exists():
                    if time.monotonic() > deadline:
                        raise AssertionError('failure waited for the earlier child instead of requesting its stop')
                    time.sleep(.005)
                return 0
            with patch.object(batch, 'measure', measure):
                with self.assertRaisesRegex(RuntimeError, 'second child failed'):
                    batch.execute_group(rows, {}, 2)
