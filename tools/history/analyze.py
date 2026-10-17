#!/usr/bin/env python3
"""Audit ancestry and summarize a completed, possibly resumed history.

Usage: python3 tools/history/analyze.py manifest.json output-directory
The manifest contains `final_directory` and chronological `segments`, each with
`directory`, optional exclusive `after`, and optional inclusive `through` notice
sequence bounds. Paths are relative to the manifest. Never include an interrupted
prefix beyond its saved boundary. This reads developer evidence, not actor memory.
"""

import collections
import csv
import gzip
import json
from pathlib import Path
import statistics
import sys

DAY = 86_400_000
YEAR = 365 * DAY


def event_stream(segment, base):
    if "journal" in segment:
        path=base / segment["journal"]
    else:
        directory=base / segment["directory"]
        path=next((directory/name for name in ["events.jsonl","events.jsonl.gz","demographics.jsonl.gz"] if (directory/name).exists()),directory/"events.jsonl")
    return gzip.open(path,"rb") if path.suffix==".gz" else path.open("rb")



def reproductive_end(body, model):
    if 'fixed_age_years' in model:
        return body['born'] + int(model['fixed_age_years']) * YEAR
    numerator, denominator = model['lifespan_fraction']
    if not 0 < numerator <= denominator:
        raise ValueError('invalid reproductive lifespan fraction')
    limit = body.get('life_limit')
    return body['born'] if limit is None else body['born'] + max(0, limit-body['born']) // denominator * numerator


def analyze(manifest_path, output):
    manifest_path = Path(manifest_path).resolve()
    manifest = json.loads(manifest_path.read_text())
    reproductive_model = manifest.get('reproductive_window', {'fixed_age_years':45})
    base = manifest_path.parent
    final = base / manifest["final_directory"]
    summary = json.loads((final / "summary.json").read_text())
    if (final / "people.json").exists():
        people = json.loads((final / "people.json").read_text())
    else:
        with gzip.open(final / "people.json.gz", "rt") as stream: people = json.load(stream)
    mortals = {p["id"]: p for p in people if p["generation"] is not None
               and p["body"]["kind"] == "Mortal"}
    depths = {}
    children = collections.Counter()

    def depth(pid, visiting=frozenset()):
        if pid in depths:
            return depths[pid]
        if pid in visiting:
            raise ValueError("cyclic ancestry")
        person = mortals[pid]
        family = person["family"]
        if family:
            for parent in [family["mother"], family["father"]]:
                if person["body"]["born"] - mortals[parent]["body"]["born"] < 18 * YEAR:
                    raise ValueError("parent was younger than the campaign's biological adulthood")
        result = 0 if family is None else 1 + min(
            depth(family["mother"], visiting | {pid}),
            depth(family["father"], visiting | {pid}))
        if person["generation"] != result:
            raise ValueError(f"incorrect generation for {pid}")
        depths[pid] = result
        return result

    for pid, person in mortals.items():
        depth(pid)
        if person["family"]:
            children.update([person["family"]["mother"], person["family"]["father"]])
    if max(depths.values(), default=0) != summary["final"]["generations"]:
        raise ValueError("summary ancestry depth differs from actual parentage")

    births, deaths = {}, {}
    important = []
    milestone_depth = 0
    recorded = collections.Counter()
    previous = 0
    for segment in manifest["segments"]:
        lower, upper = segment.get("after", 0), segment.get("through", float("inf"))
        with event_stream(segment, base) as stream:
            for raw in stream:
                try:
                    event = json.loads(raw)
                except (json.JSONDecodeError, UnicodeDecodeError):
                    if upper != float("inf") and not raw.endswith(b"\n"):
                        break  # Interrupted tail is allowed only beyond a frozen boundary.
                    raise
                seq = event["sequence"]
                if seq <= lower:
                    continue
                if seq > upper:
                    break
                if seq <= previous:
                    raise ValueError("overlapping or unordered journal segments")
                previous = seq
                kind, data = event["kind"], event["data"]
                recorded[kind] += 1
                if kind == "lifecycle.born":
                    pid = data["person"]
                    if pid in births:
                        raise ValueError("duplicate birth")
                    births[pid] = event
                    if pid in mortals and depth(pid) > milestone_depth:
                        milestone_depth = depth(pid)
                        important.append({"day": event["day"], "kind": kind,
                                          "person": pid, "generation": depth(pid)})
                elif kind == "physiology.died":
                    pid = data["person"]
                    if pid in deaths:
                        raise ValueError("duplicate death")
                    deaths[pid] = event
                elif kind in {"communities.founded", "institutions.procedure-changed",
                              "institutions.rule-adopted", "pressure.submitted"}:
                    important.append({"day": event["day"], "kind": kind, "data": data})

    expected_births = {pid for pid, p in mortals.items() if p["family"]}
    if set(births) != expected_births:
        raise ValueError("birth journal does not match actual families")
    expected_deaths = {pid for pid, p in mortals.items() if not p["body"]["alive"]}
    if set(deaths) != expected_deaths:
        raise ValueError("death journal does not match mortal bodies")
    for pid, event in births.items():
        if event["data"]["at"] != mortals[pid]["body"]["born"]:
            raise ValueError("birth timestamp differs from body")

    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    traits = sorted(next(iter(mortals.values()))["traits"])
    rows = []
    by_generation = collections.defaultdict(list)
    for pid, person in mortals.items():
        body = person["body"]
        end = deaths[pid]["data"]["at"] if pid in deaths else summary["final"]["time"]
        age = (end - body["born"]) / YEAR
        death_body = deaths.get(pid, {}).get("body") or {}
        reproductive_limit = reproductive_end(body, reproductive_model)
        row = {"id": pid, "name": person["name"], "generation": depth(pid),
               "alive": body["alive"], "age_observed": age,
               "birth_day": (body["born"] - summary["epoch"]) / DAY,
               "death_day": deaths[pid]["day"] if pid in deaths else "",
               "children_born": children[pid],
               "reproductive_end_age": (reproductive_limit-body['born'])/YEAR,
               "reproductive_window_closed": end >= reproductive_limit or not body["alive"],
               "hunger_at_death": death_body.get("hunger", ""),
               **person["traits"]}
        rows.append(row)
        by_generation[depth(pid)].append(row)
    with (output / "people.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)

    cohorts = []
    for generation, group in sorted(by_generation.items()):
        closed = [p for p in group if p["reproductive_window_closed"]]
        adults = [p for p in group if p["age_observed"] >= 18]
        cohort = {"generation": generation, "born": len(group),
                  "alive": sum(p["alive"] for p in group),
                  "reached_adulthood": len(adults), "closed_windows": len(closed),
                  "mean_children_closed_windows": statistics.mean(
                      p["children_born"] for p in closed) if closed else None,
                  "traits": {}}
        for trait in traits:
            values = [p[trait] for p in group]
            offspring = sum(p["children_born"] for p in closed)
            cohort["traits"][trait] = {
                "mean": statistics.mean(values), "sd": statistics.pstdev(values),
                "min": min(values), "max": max(values),
                "parent_mean_closed_windows": sum(p[trait] * p["children_born"] for p in closed)
                / offspring if offspring else None,
                "mean_closed_windows": statistics.mean(p[trait] for p in closed) if closed else None}
            genomes = [(mortals[p["id"]].get("genes") or {}).get(trait) for p in group]
            if all(g is not None for g in genomes):
                alleles = [allele for pair in genomes for allele in pair]
                cohort["traits"][trait]["distinct_alleles"] = len(set(alleles))
                cohort["traits"][trait]["heterozygous_fraction"] = statistics.mean(
                    pair[0] != pair[1] for pair in genomes)
        cohorts.append(cohort)
    result = {"reproductive_window": reproductive_model, "maximum_depth_verified": max(depths.values()), "mortals": len(mortals),
              "births_verified": len(births), "deaths_verified": len(deaths),
              "journal_rows": dict(recorded), "cohorts": cohorts,
              "timeline": important, "summary": summary,
              "interpretation": "Closed-window reproductive associations are descriptive, not causal. "
              "Death and observation censor cohorts; founder adult ages are left-truncated. "
              "Selection, drift, mutation, family rules, and learned culture coexist."}
    (output / "analysis.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({k: result[k] for k in ["maximum_depth_verified", "mortals",
                                           "births_verified", "deaths_verified"]}))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    analyze(sys.argv[1], sys.argv[2])
