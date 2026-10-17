"""Small reader fixtures; these are not simulation experiment results."""
import contextlib
import gzip
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("history_analysis", Path(__file__).parents[1] / "analyze.py")
analysis = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analysis)


class AnalysisTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for folder in ["prefix", "suffix"]:
            (self.root / folder).mkdir()
        self.epoch = 40_000 * analysis.DAY
        self.people = []
        for pid in [1, 2, 3]:
            self.people.append({
                "id": pid, "name": str(pid), "generation": int(pid == 3),
                "body": {"kind": "Mortal", "alive": pid != 1,
                         "born": self.epoch - 20 * analysis.YEAR if pid < 3 else self.epoch + analysis.DAY,
                         "last": self.epoch + 10 * analysis.YEAR},
                "family": {"mother": 1, "father": 2} if pid == 3 else None,
                "traits": {"intelligence": 1000}, "genes": {"intelligence": [1000, 2000]}})
        self.summary = {"epoch": self.epoch, "final": {
            "time": self.epoch + 10 * analysis.YEAR, "generations": 1}}
        self.birth = {"sequence": 10, "kind": "lifecycle.born", "day": 1,
                      "data": {"person": 3, "at": self.epoch + analysis.DAY}}
        self.death = {"sequence": 12, "kind": "physiology.died", "day": 365,
                      "data": {"person": 1, "at": self.epoch + analysis.YEAR},
                      "body": {"hunger": 120}}
        # This abandoned tail belongs to a different continuation of the prefix.
        tail = {**self.death, "sequence": 11, "data": {"person": 999}}
        (self.root / "prefix/events.jsonl").write_text(
            json.dumps(self.birth) + "\n" + json.dumps(tail) + "\n")
        (self.root / "suffix/events.jsonl").write_text(json.dumps(self.death) + "\n")
        self.manifest = {"final_directory": "suffix", "segments": [
            {"directory": "prefix", "through": 10}, {"directory": "suffix", "after": 10}]}

    def run_reader(self):
        for filename, value in [("people.json", self.people), ("summary.json", self.summary)]:
            (self.root / "suffix" / filename).write_text(json.dumps(value))
        (self.root / "manifest.json").write_text(json.dumps(self.manifest))
        with contextlib.redirect_stdout(io.StringIO()):
            analysis.analyze(self.root / "manifest.json", self.root / "analysis")
        return json.loads((self.root / "analysis/analysis.json").read_text())

    def test_parentage_boundaries_genotypes_and_death_times(self):
        result = self.run_reader()
        self.assertEqual(result["maximum_depth_verified"], 1)
        self.assertEqual(result["births_verified"], 1)
        self.assertEqual(result["deaths_verified"], 1)
        self.assertEqual(result["cohorts"][1]["traits"]["intelligence"]["heterozygous_fraction"], 1)
        import csv
        with (self.root / "analysis/people.csv").open() as stream:
            people = list(csv.DictReader(stream))
        self.assertEqual(float(people[0]["age_observed"]), 21)
        self.assertEqual(people[0]["children_born"], "1")

    def test_reproductive_censoring_follows_the_declared_inherited_lifespan_window(self):
        self.manifest['reproductive_window']={'lifespan_fraction':[2,3]}
        for person in self.people:person['body']['life_limit']=person['body']['born']+150*analysis.YEAR
        self.summary['final']['time']=self.epoch+50*analysis.YEAR
        self.run_reader()
        import csv
        with (self.root/'analysis/people.csv').open() as stream:rows=list(csv.DictReader(stream))
        self.assertEqual(rows[1]['reproductive_window_closed'],'False')
        self.assertEqual(float(rows[1]['reproductive_end_age']),100)
        self.people[1]['body']['life_limit']=self.people[1]['body']['born']+75*analysis.YEAR
        self.run_reader()
        with (self.root/'analysis/people.csv').open() as stream:rows=list(csv.DictReader(stream))
        self.assertEqual(rows[1]['reproductive_window_closed'],'True')

    def test_missing_death_and_incorrect_depth_are_rejected(self):
        (self.root / "suffix/events.jsonl").write_text("")
        with self.assertRaisesRegex(ValueError, "death journal"):
            self.run_reader()
        (self.root / "suffix/events.jsonl").write_text(json.dumps(self.death) + "\n")
        self.people[2]["generation"] = 20
        with self.assertRaisesRegex(ValueError, "incorrect generation"):
            self.run_reader()

    def test_compacted_segments_preserve_the_same_demographic_evidence(self):
        expected = self.run_reader()
        for segment in self.manifest["segments"]:
            source = self.root / segment.pop("directory") / "events.jsonl"
            archive = source.with_suffix(".jsonl.gz")
            with gzip.open(archive, "wb") as stream:
                stream.write(source.read_bytes())
            source.unlink()
            segment["journal"] = str(archive.relative_to(self.root))
        # The final individual snapshot can also be kept only in compressed form.
        source = self.root / "suffix/people.json"
        with gzip.open(source.with_suffix(".json.gz"), "wb") as stream:
            stream.write(source.read_bytes())
        source.unlink()
        (self.root / "manifest.json").write_text(json.dumps(self.manifest))
        with contextlib.redirect_stdout(io.StringIO()):
            analysis.analyze(self.root / "manifest.json", self.root / "analysis")
        self.assertEqual(json.loads((self.root / "analysis/analysis.json").read_text()), expected)


if __name__ == "__main__":
    unittest.main()
