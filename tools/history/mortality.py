#!/usr/bin/env python3
"""Describe recorded mortality and reproduction; indicators are not causes.

Read the verified per-run people.csv and events.jsonl.gz produced by study.py.
Usage: python3 tools/history/mortality.py study-directory output.json
"""
import collections
import csv
import gzip
import json
from pathlib import Path
import statistics
import journals
import sys

YEAR = 365 * 86_400_000


def summarize(folder):
    people = {int(p['id']): p for p in csv.DictReader((folder / 'people.csv').open())}
    deaths = {}
    paired = set()
    for event in journals.rows(folder):
        if event['kind'] == 'physiology.died':
            deaths[event['data']['person']] = event
        elif event['kind'] == 'households.formed':
            paired.update([event['data']['first'], event['data']['second']])
    counts = collections.Counter()
    ages = []
    for pid, event in deaths.items():
        p, b = people[pid], event['body']
        age = (event['data']['at'] - b['born']) / YEAR
        ages.append(age)
        counts['deaths'] += 1
        counts['below_age_6'] += age < 6
        counts['below_age_18'] += age < 18
        counts['at_life_limit'] += event['data']['at'] >= b['life_limit']
        counts['hunger_at_least_120'] += b['hunger'] >= 120
        counts['under_45_low_hunger'] += age < 45 and b['hunger'] < 120
        counts['adult_never_paired'] += age >= 18 and pid not in paired
        counts['adult_no_recorded_children'] += age >= 18 and int(p['children_born']) == 0
    descendants = [p for p in people.values() if int(p['generation']) > 0]
    adults = [p for p in people.values() if float(p['age_observed']) >= 18]
    # Traits versus offspring are descriptive and confounded by the family template.
    traits = {}
    for key in ['intelligence', 'foresight', 'empathy', 'patience']:
        for label, group in [('adult_parents', [p for p in adults if int(p['children_born'])]),
                             ('adult_nonparents', [p for p in adults if not int(p['children_born'])])]:
            traits[f'{key}_{label}'] = statistics.mean(float(p[key]) for p in group) if group else None
    return dict(name=folder.name, **counts, paired=len(paired), adults=len(adults),
                descendants=len(descendants),
                descendant_adults=sum(float(p['age_observed']) >= 18 for p in descendants),
                median_death_age=statistics.median(ages) if ages else None, traits=traits)


if __name__ == '__main__':
    root, output = map(Path, sys.argv[1:])
    rows = [summarize(p) for p in sorted(root.iterdir()) if (p / 'people.csv').exists()]
    output.write_text(json.dumps(rows, indent=2) + '\n')
    for row in rows:
        print({k: v for k, v in row.items() if k != 'traits'}, flush=True)
