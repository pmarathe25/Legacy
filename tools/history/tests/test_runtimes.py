import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import runtimes
from study import digest


class RuntimeUpgrade(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root/'checkpoint.json').write_text('verified world')
        (self.root/'checkpoint-ledger.json').write_text('{"sequence":7}')
        self.state = {'binary_sha256':'old', 'runs':[{'name':'population','state':'censored','checkpoint':str(self.root),
                      'days':7300,'cursor':7,'depth':2,'segments':[{'journal':'original','through':7}]}]}
        hashes = {}
        for name in ['checkpoint.json','checkpoint-ledger.json','events.jsonl','people.json']:
            for build in ['control','candidate']:
                folder=self.root/f'03-{build}';folder.mkdir(exist_ok=True)
                (folder/name).write_text('checked '+name)
            hashes[name]=digest(folder/name)
        self.report = {'equivalent':True,'pairs':3,'days':7330,'checkpoint_sha256':digest(self.root/'checkpoint.json'),
                       'binaries':{'control':{'sha256':'old'},'candidate':{'sha256':'new'}},
                       'runs':[{'pair':p,'build':b,'hashes':hashes.copy()} for p in range(1,4) for b in ['control','candidate']]}
        self.plan = {'binary_sha256':'new','runtime_upgrade':{'from_sha256':'old','comparisons':{'population':'comparison.json'}}}
        self.write_report()

    def write_report(self):
        (self.root/'comparison.json').write_text(json.dumps(self.report))

    def test_verified_upgrade_preserves_the_population_cursor_and_journal_prefix(self):
        previous = copy.deepcopy(self.state['runs'])
        self.assertTrue(runtimes.accept_upgrade(self.plan,self.state,self.root))
        self.assertEqual(self.state['runs'],previous)
        self.assertEqual(self.state['binary_sha256'],'new')
        self.assertEqual(len(self.state['runtime_upgrades']),1)
        self.assertFalse(runtimes.accept_upgrade(self.plan,self.state,self.root))

    def test_different_journal_rejects_upgrade_without_mutating_study(self):
        self.report['runs'][-1]['hashes']['events.jsonl']='different';self.write_report()
        previous=copy.deepcopy(self.state)
        with self.assertRaises(ValueError):runtimes.accept_upgrade(self.plan,self.state,self.root)
        self.assertEqual(self.state,previous)

    def test_changed_retained_outputs_reject_upgrade(self):
        (self.root/'03-candidate/events.jsonl').write_text('tampered')
        previous=copy.deepcopy(self.state)
        with self.assertRaises(ValueError):runtimes.accept_upgrade(self.plan,self.state,self.root)
        self.assertEqual(self.state,previous)

    def test_other_checkpoint_or_live_writer_cannot_be_upgraded(self):
        previous=copy.deepcopy(self.state)
        self.report['checkpoint_sha256']='another world';self.write_report()
        with self.assertRaises(ValueError):runtimes.accept_upgrade(self.plan,self.state,self.root)
        self.assertEqual(self.state,previous)
        self.state['runs'][0]['state']='running'
        with self.assertRaises(ValueError):runtimes.accept_upgrade(self.plan,self.state,self.root)


if __name__=='__main__':unittest.main()
