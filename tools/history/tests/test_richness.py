import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import richness


class RichnessDiagnostics(unittest.TestCase):
    def test_progress_uses_full_skills_and_private_facts_and_retains_cohort_limits(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def snapshot(name, after):
                folder = root/name
                folder.mkdir()
                people = [{'id': 1, 'body': {'alive': True}, 'goals': 5 if after else 3,
                           'conversations': 9 if after else 4, 'skills': []},
                          {'id': 2, 'body': {'alive': not after}, 'goals': 100,
                           'conversations': 100, 'skills': []}]
                skills = {f'skill-{i}': 1 for i in range(9)}
                if after: skills['skill-8'] = 7
                pieces = {'10': {'creator': 1, 'activity': 'play-flute', 'at': 0, 'evidence': []}}
                if after:
                    pieces['11'] = {'creator': 1, 'activity': 'compose-song', 'at': 0,
                                    'evidence': [['theory:x', {}]]}
                world = {'time': 0, 'resources': {
                    'works.creations': {'pieces': pieces},
                    'inquiry.experience': {'people': {'1': {'count': 14 if after else 10}}},
                    'skills.practice': {'levels': {'1': skills}},
                    'knowledge.facts': {'archive': {'keys': ['theory:x', 'roads:1'],
                        'people': {'1': [[0, 0, 1, 0, 0, 80]], '2': [[1, 0, 2, 0, 0, 80]]}}},
                    'relationships.social': {'relationships': {'1': {'2': {'affection': 40}}}}}}
                (folder/'checkpoint.json').write_text(json.dumps(world))
                (folder/'people.json').write_text(json.dumps(people))
                return folder
            before, after = snapshot('before', False), snapshot('after', True)
            result = richness.compare(richness.read(before), after)
            self.assertEqual(result['living_people'], 1)
            self.assertEqual(result['per_person']['1']['goals'], 2)
            self.assertEqual(result['per_person']['1']['conversations'], 5)
            self.assertEqual(result['per_person']['1']['witnessed'], 4)
            self.assertEqual(result['per_person']['1']['skill_growth'], 6)
            self.assertEqual(result['per_person']['1']['skill_breadth'], 9)
            self.assertEqual(result['retained_fact_namespaces'], {'theory': 1})
            self.assertEqual(result['new_work_forms'], {'compose-song': 1})
            self.assertEqual(result['new_work_creators'], 1)
            self.assertEqual(result['new_work_evidence_items'], 1)
            self.assertEqual(result['living_affectionate_edges'], 0)
            self.assertNotIn('2', result['per_person'])
