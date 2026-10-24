import json
import copy
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from state_hashes import artifact_digest, WORLD_CANONICAL, EVIDENCE_CANONICAL


class ReplayStateHashes(unittest.TestCase):
    def test_evidence_layout_can_change_but_every_payload_stamp_and_world_field_is_checked(self):
        original = {'time': 7, 'jobs': [2, 1], 'resources': {'knowledge.facts': {
            'archive': {'version': 1, 'keys': ['measured:x', 'memory:empirical:1'],
                        'values': [{'value': 2}, {'value': 2}],
                        'people': {'9': [[0, 0, 9, 7, 3, 80], [1, 1, 9, 7, 3, 80]]}},
            'cognition': {'9': {'due': 8}}}}}
        compact = copy.deepcopy(original)
        archive = compact['resources']['knowledge.facts']['archive']
        archive['keys'].reverse()
        archive['values'].pop()
        archive['people']['9'] = [[1, 0, 9, 7, 3, 80], [0, 0, 9, 7, 3, 80]]
        with tempfile.TemporaryDirectory() as root:
            path = Path(root)/'checkpoint.json'
            def hashed(value, mode=EVIDENCE_CANONICAL):
                path.write_text(json.dumps(value))
                return artifact_digest(path, mode)
            expected = hashed(original)
            self.assertEqual(hashed(compact), expected)
            self.assertNotEqual(hashed(original, WORLD_CANONICAL), hashed(compact, WORLD_CANONICAL))
            for slot in range(6):
                changed = copy.deepcopy(compact)
                row = changed['resources']['knowledge.facts']['archive']['people']['9'][0]
                row[slot] = 1 - row[slot] if slot == 0 else row[slot] + 1
                if slot == 1:
                    with self.assertRaises(ValueError): hashed(changed)
                else:
                    self.assertNotEqual(hashed(changed), expected)
            for change in ['payload', 'unused', 'trace', 'time', 'jobs', 'row-order']:
                changed = copy.deepcopy(compact)
                archive = changed['resources']['knowledge.facts']['archive']
                if change == 'payload': archive['values'][0]['value'] = 3
                if change == 'unused': archive['values'].append({'unused': True})
                if change == 'trace': changed['resources']['knowledge.facts']['cognition']['9']['due'] += 1
                if change == 'time': changed['time'] += 1
                if change == 'jobs': changed['jobs'].reverse()
                if change == 'row-order': archive['people']['9'].reverse()
                self.assertNotEqual(hashed(changed), expected, change)
            for invalid in [-1, True, 2]:
                changed = copy.deepcopy(compact)
                changed['resources']['knowledge.facts']['archive']['people']['9'][0][0] = invalid
                with self.assertRaises(ValueError): hashed(changed)

    def test_world_reordering_is_equivalent_but_values_and_array_order_are_not(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root)/'checkpoint.json'
            path.write_text('{"jobs":[{"key":"work","payload":{"z":1,"a":2}}],"time":7}')
            expected = artifact_digest(path, WORLD_CANONICAL)
            path.write_text('{"time":7,"jobs":[{"payload":{"a":2,"z":1},"key":"work"}]}')
            self.assertEqual(artifact_digest(path, WORLD_CANONICAL), expected)
            value = json.loads(path.read_text())
            value['jobs'][0]['payload']['a'] = 3
            path.write_text(json.dumps(value))
            self.assertNotEqual(artifact_digest(path, WORLD_CANONICAL), expected)
            path.write_text('{"jobs":[1,2]}')
            expected = artifact_digest(path, WORLD_CANONICAL)
            path.write_text('{"jobs":[2,1]}')
            self.assertNotEqual(artifact_digest(path, WORLD_CANONICAL), expected)

    def test_journal_bytes_remain_exact_and_unknown_modes_are_rejected(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root)/'events.jsonl'
            path.write_text('{"a":1,"b":2}\n')
            expected = artifact_digest(path, WORLD_CANONICAL)
            path.write_text('{"b":2,"a":1}\n')
            self.assertNotEqual(artifact_digest(path, WORLD_CANONICAL), expected)
            with self.assertRaises(ValueError):
                artifact_digest(path, 'skip-state')
