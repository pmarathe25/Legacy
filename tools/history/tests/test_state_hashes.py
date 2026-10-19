import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from state_hashes import artifact_digest, WORLD_CANONICAL


class ReplayStateHashes(unittest.TestCase):
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
