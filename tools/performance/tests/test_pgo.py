import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('pgo', Path(__file__).parents[1]/'pgo.py')
pgo = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pgo)


class TrainingTests(unittest.TestCase):
    def test_nonpositive_build_concurrency_is_rejected_before_touching_files(self):
        for jobs in [0, -1]:
            with self.assertRaisesRegex(ValueError, 'build jobs must be positive'):
                pgo.build(Path('/unused'), Path('/unused'), Path('/unused'), build_jobs=jobs)

    def test_checkpoint_and_external_overlay_changes_are_fingerprinted(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ['checkpoint.json', 'checkpoint-ledger.json', 'world.toml']:
                (root/name).write_text('{}')
            arguments = [['--resume', temporary, '--mod', temporary, '--days', '1']]
            original = pgo.input_digests(arguments)
            self.assertEqual(len(original), 3)
            (root/'checkpoint-ledger.json').write_text('{"changed":true}')
            self.assertNotEqual(pgo.input_digests(arguments), original)
            (root/'checkpoint-ledger.json').write_text('{}')
            (root/'new.toml').write_text('value = 1')
            self.assertNotEqual(pgo.input_digests(arguments), original)

    def test_relative_paths_are_resolved_without_changing_the_plan(self):
        plan = {'runs': [{'arguments': ['--resume', '../world', '--mod', 'a',
                                       '--mod', 'b', '--days', '100']}]}
        args = pgo.training_runs(plan, Path('/tmp/plans'))[0]
        self.assertEqual(args, ['--resume', '/tmp/world', '--mod', '/tmp/plans/a',
                                '--mod', '/tmp/plans/b', '--days', '100'])
        self.assertEqual(plan['runs'][0]['arguments'][1], '../world')

    def test_output_overrides_and_unbounded_or_malformed_runs_are_rejected(self):
        for arguments in [[], ['--days'], ['--days', 20], ['--output', 'elsewhere', '--days', '1'],
                          ['--stop-file', 'external', '--days', '1']]:
            with self.subTest(arguments=arguments), self.assertRaises(ValueError):
                pgo.training_runs({'runs': [{'arguments': arguments}]}, Path('/tmp'))
        with self.assertRaises(ValueError):
            pgo.training_runs({'runs': []}, Path('/tmp'))

    def test_restart_reuses_only_interrupted_compilation_before_training(self):
        import json
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)/'build'
            self.assertIsNone(pgo.prepare_output(output, False))
            marker = output/'unrelated.txt'; marker.write_text('keep')
            with self.assertRaises(ValueError): pgo.prepare_output(output, True)
            previous = {'compiler':'rustc', 'target':'host', 'source_sha256':'old',
                        'training_inputs':{}, 'training':[]}
            (output/'metadata.json').write_text(json.dumps(previous))
            (output/'raw').mkdir()
            self.assertEqual(pgo.prepare_output(output, True), previous)
            (output/'raw/profile.profraw').write_bytes(b'training evidence')
            with self.assertRaises(ValueError): pgo.prepare_output(output, True)
            (output/'raw/profile.profraw').unlink()
            (output/'game-history').write_bytes(b'frozen executable')
            with self.assertRaises(ValueError): pgo.prepare_output(output, True)
            self.assertEqual(marker.read_text(), 'keep')

    def test_source_fingerprint_covers_code_and_content_but_excludes_logs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root/'Cargo.toml').write_text('[workspace]\n')
            (root/'Cargo.lock').write_text('version = 4\n')
            (root/'content').mkdir()
            rules = root/'content/rules.toml'
            rules.write_text('value = 1\n')
            before = pgo.source_digest(root)
            (root/'content/run.log').write_text('transient')
            self.assertEqual(pgo.source_digest(root), before)
            rules.write_text('value = 2\n')
            self.assertNotEqual(pgo.source_digest(root), before)
            rules.write_text('value = 1\n')
            (root/'crates/feature/src').mkdir(parents=True)
            (root/'crates/feature/src/lib.rs').write_text('pub fn changed() {}\n')
            self.assertNotEqual(pgo.source_digest(root), before)


if __name__ == '__main__':
    unittest.main()
