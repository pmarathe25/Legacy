import fcntl
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from fork import fork
from study import digest


class ForkTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.archive = self.root/'old/archive'
        self.checkpoint = self.root/'old/runs/chunk-0001'
        self.checkpoint.mkdir(parents=True)
        self.old, self.new = self.root/'old-binary', self.root/'new-binary'
        self.old.write_bytes(b'old-model')
        self.new.write_bytes(b'new-model')
        self.validation = self.root/'validation.json'
        self.validation.write_text('{"exit_code":0}')
        self.world = {'notices': [{'sequence': 7}],
                      'resources': {'family': {'name': 'Role', 'genes': [12, 99], 'born': 4}}}
        (self.checkpoint/'checkpoint.json').write_text(json.dumps(self.world))
        (self.checkpoint/'checkpoint-ledger.json').write_text('{"sequence":7}')
        prefix = self.archive/'test/chunks/0001-events.jsonl.gz'
        prefix.parent.mkdir(parents=True)
        prefix.write_bytes(b'immutable-compressed-prefix')
        segments = [{'journal': str(prefix), 'after': 0, 'through': 7}]
        (self.archive/'test/segments.json').write_text(json.dumps({'final_directory': '.', 'segments': segments}))
        (self.archive/'stop-request').touch()
        self.state = {'binary_sha256': digest(self.old), 'finished': True, 'generations': 20,
                      'runs': [{'name': 'test', 'state': 'censored', 'days': 10, 'depth': 1,
                                'cursor': 7, 'checkpoint': str(self.checkpoint), 'segments': segments}]}
        self.write_state()
        self.plan = self.root/'old/plan.json'
        self.plan.write_text(json.dumps({'root': str(self.root), 'artifacts': str(self.archive),
                                        'binary_sha256': digest(self.old), 'executable': str(self.old),
                                        'days': 73000, 'jobs': [{'name': 'test', 'output': 'old/runs'}]}))
        self.destination = self.root/'variant'

    def write_state(self):
        (self.archive/'study.json').write_text(json.dumps(self.state))

    def run_fork(self, **kwargs):
        return fork(self.plan, self.destination, self.new, 'Correct actual decision feedback', self.validation, **kwargs)

    def test_fork_preserves_people_prefix_and_immutable_origin(self):
        before = (self.archive/'study.json').read_bytes()
        path = self.run_fork(days=146000)
        plan = json.loads(path.read_text())
        state = json.loads((self.destination/'archive/study.json').read_text())
        row = state['runs'][0]
        self.assertEqual((self.archive/'study.json').read_bytes(), before)
        self.assertEqual((self.destination/'source-study.json').read_bytes(), before)
        self.assertEqual(json.loads((Path(row['checkpoint'])/'checkpoint.json').read_text()), self.world)
        self.assertEqual(state['binary_sha256'], digest(self.new))
        self.assertEqual(state['model_forks'][0]['from_sha256'], digest(self.old))
        self.assertEqual(state['model_forks'][0]['checkpoints'][0]['depth'], 1)
        self.assertEqual(plan['days'], 146000)
        copied = Path(row['segments'][0]['journal'])
        self.assertTrue(copied.is_relative_to(self.destination))
        self.assertEqual(copied.read_bytes(), b'immutable-compressed-prefix')
        manifest = json.loads((self.destination/'archive/test/segments.json').read_text())
        self.assertEqual(manifest['segments'], row['segments'])
        (Path(row['checkpoint'])/'checkpoint.json').unlink()
        self.assertEqual(json.loads((self.destination/'origins/test/checkpoint.json').read_text()), self.world)

    def test_active_source_cannot_be_forked(self):
        self.state['runs'][0]['state'] = 'running'
        self.write_state()
        with self.assertRaisesRegex(ValueError, 'stop the source'):
            self.run_fork()
        self.assertFalse(self.destination.exists())

    def test_mismatched_ledger_cannot_be_presented_as_a_verified_origin(self):
        (self.checkpoint/'checkpoint-ledger.json').write_text('{"sequence":6}')
        with self.assertRaisesRegex(ValueError, 'journal cursor'):
            self.run_fork()
        self.assertFalse(self.destination.exists())

    def test_failed_validation_and_unchanged_model_are_rejected(self):
        self.validation.write_text('{"exit_code":1}')
        with self.assertRaisesRegex(ValueError, 'validation'):
            self.run_fork()
        self.validation.write_text('{"exit_code":0}')
        self.new.write_bytes(self.old.read_bytes())
        with self.assertRaisesRegex(ValueError, 'new model'):
            self.run_fork()
        self.assertFalse(self.destination.exists())

    def test_existing_destination_is_preserved(self):
        self.destination.mkdir()
        sentinel = self.destination/'keep'
        sentinel.write_text('existing results')
        with self.assertRaisesRegex(ValueError, 'already exists'):
            self.run_fork()
        self.assertEqual(sentinel.read_text(), 'existing results')

    def test_supervisor_lease_prevents_copying_a_checkpoint_in_use(self):
        with (self.archive/'supervisor.lock').open('a') as lease:
            fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, 'supervisor'):
                self.run_fork()
        self.assertFalse(self.destination.exists())

    def test_destination_inside_source_runs_is_rejected(self):
        self.destination = self.checkpoint/'nested-variant'
        with self.assertRaisesRegex(ValueError, 'outside source'):
            self.run_fork()
        self.assertFalse(self.destination.exists())


if __name__ == '__main__':
    unittest.main()
