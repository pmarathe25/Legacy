import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
spec = importlib.util.spec_from_file_location('decisions', Path(__file__).parents[1]/'decisions.py')
decisions = importlib.util.module_from_spec(spec)
spec.loader.exec_module(decisions)


class DecisionExperimentTests(unittest.TestCase):
    def test_forks_change_only_the_declared_rules_and_keep_the_source_unchanged(self):
        source = {'resources': {'characters.minds': {'rules': {'review': 100}},
                                'other': {'important': [1, 2]}}, 'time': 42}
        original = copy.deepcopy(source)
        variants = decisions.policies(source, [1, 5], [5, 60])
        self.assertEqual(variants['ordinary'], {})
        for name, patch in variants.items():
            fork = decisions.fork(source, patch)
            self.assertEqual(source, original)
            rules = fork['resources']['characters.minds']['rules']
            self.assertEqual(rules['review'], 100)
            if name.startswith('options-'):
                self.assertNotIn('routine', rules)
            for key in patch:
                del rules[key]
            self.assertEqual(fork, source)

    def test_existing_model_changes_cannot_be_mislabeled_as_an_ordinary_control(self):
        for experiment in [{'option_memory': 300000}, {'routine': {'review_floor': 300000}}, {'care_signal_changes_only': True}]:
            source = {'resources': {'characters.minds': {'rules': experiment}}}
            with self.assertRaisesRegex(ValueError, 'ordinary'):
                decisions.policies(source, [5], [])
        with self.assertRaisesRegex(ValueError, 'positive'):
            decisions.policies({'resources': {'characters.minds': {'rules': {}}}}, [0], [])

    def test_millisecond_screen_uses_integer_ticks_and_declares_its_full_policy(self):
        source = {'resources': {'characters.minds': {'rules': {}}}}
        variants = decisions.policies(source, [], [1], review_millis=[1, 100])
        self.assertEqual(variants['reviews-1']['routine']['review_floor'], 60000)
        self.assertEqual(variants['reviews-1ms']['routine']['review_floor'], 1)
        self.assertEqual(variants['reviews-100ms']['routine']['review_floor'], 100)
        self.assertEqual(variants['reviews-1ms']['routine']['minimum'], 1800000)
        with self.assertRaisesRegex(ValueError, 'positive'):
            decisions.policies(source, [], [], review_millis=[0])

    def test_completion_policy_has_a_fixed_window_and_no_global_review_floor(self):
        source = {'resources': {'characters.minds': {'rules': {}}}}
        variants = decisions.policies(source, [], [], completion_minutes=[2, 5])
        for minutes in [2, 5]:
            policy = variants[f'completion-routines-{minutes}']['routine']
            self.assertTrue(policy['preserve_on_care_completion'])
            self.assertEqual(policy['minimum'], minutes*60000)
            self.assertEqual(policy['maximum'], minutes*60000)
            self.assertNotIn('review_floor', policy)
        with self.assertRaisesRegex(ValueError, 'positive'):
            decisions.policies(source, [], [], completion_minutes=[0])

    def test_declared_policies_retain_control_and_reject_ambiguous_labels(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'policies.json'
            path.write_text(json.dumps({'food-evidence': {'care_signal_changes_only': True}}))
            variants = decisions.declared_policies(path, {'ordinary': {}})
            self.assertEqual(variants['ordinary'], {})
            self.assertTrue(variants['food-evidence']['care_signal_changes_only'])
            for invalid in [{'ordinary': {'routine': {}}}, {'../unsafe': {'review': 1}},
                            {'empty': {}}, {'list': []}, []]:
                path.write_text(json.dumps(invalid))
                with self.assertRaises(ValueError):
                    decisions.declared_policies(path, {'ordinary': {}})

    def test_each_measured_child_has_its_own_usage_and_failure_is_propagated(self):
        import os
        import subprocess
        with tempfile.TemporaryFile('w') as log:
            for _ in range(2):
                metrics = decisions.measure([sys.executable, '-c', 'sum(range(10000))'], dict(os.environ), log)
                self.assertGreater(metrics['wall_seconds'], 0)
                self.assertGreater(metrics['cpu_seconds'], 0)
                self.assertGreater(metrics['rss_kib'], 0)
            with self.assertRaises(subprocess.CalledProcessError):
                decisions.measure([sys.executable, '-c', 'raise SystemExit(7)'], dict(os.environ), log)

    def test_outcomes_subtract_prefix_events_and_label_daily_sampling(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            initial = {'day': 10, 'alive': 8, 'hungry': 2, 'injured': 3, 'actions': {'work': 4}}
            final = dict.fromkeys(decisions.FINAL_FIELDS, 0)
            final.update(day=12, alive=7, hungry=1, injured=2, actions={'work': 3},
                         ledger={'events': {'physiology.fed': 15}})
            (folder/'initial.json').write_text(json.dumps(initial))
            (folder/'daily.jsonl').write_text(json.dumps(final)+'\n')
            (folder/'summary.json').write_text(json.dumps({'final': final, 'extinct': False, 'days': 12}))
            (folder/'checkpoint.json').write_text('{}')
            result = decisions.behavior(folder, {'physiology.fed': 10})
            self.assertEqual(result['events']['physiology.fed'], 5)
            self.assertEqual(result['daily_sampled_person_days'], {'alive': 16, 'hungry': 4, 'injured': 6, 'acting': 8})
            self.assertFalse(result['extinct'])

    def test_large_parent_world_does_not_inflate_child_peak_memory(self):
        import os
        world = bytearray(64*1024*1024)
        for page in range(0, len(world), 4096):
            world[page] = 1
        with tempfile.TemporaryFile('w') as log:
            metrics = decisions.measure([sys.executable, '-c', 'pass'], dict(os.environ), log)
        self.assertLess(metrics['rss_kib'], 48*1024)
