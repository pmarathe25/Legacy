"""Outcome diagnostics from complete saved state; not a scalar quality score."""
from collections import Counter
import json


def read(folder):
    world = json.loads((folder/'checkpoint.json').read_text())
    people = json.loads((folder/'people.json').read_text())
    resources = world['resources']
    pieces = resources['works.creations']['pieces']
    inquiry = resources['inquiry.experience']['people']
    result = {'people': {}, 'pieces': pieces, 'time': world['time']}
    for person in people:
        key = str(person['id'])
        result['people'][key] = {
            'alive': person['body']['alive'], 'goals': person['goals'] or 0,
            'conversations': person['conversations'] or 0,
            'skills': resources['skills.practice']['levels'].get(key, {}),
            'witnessed': inquiry.get(key, {}).get('count', 0),
        }
    archive = resources['knowledge.facts']['archive']
    result['facts'] = {p: Counter(archive['keys'][r[0]].split(':')[0] for r in rows)
                       for p, rows in archive['people'].items()}
    result['ties'] = resources['relationships.social']['relationships']
    return result


def compare(prefix, folder):
    state = read(folder)
    per_person = {}
    for key, person in state['people'].items():
        if not person['alive']:
            continue
        old = prefix['people'].get(key, {})
        per_person[key] = {k: person[k]-old.get(k, 0)
                           for k in ['goals', 'conversations', 'witnessed']}
        per_person[key]['skill_growth'] = sum(
            value-old.get('skills', {}).get(skill, 0) for skill, value in person['skills'].items())
        per_person[key]['skill_breadth'] = len(person['skills'])
        per_person[key]['facts'] = dict(state['facts'].get(key, {}))
    created = [v for key, v in state['pieces'].items() if key not in prefix['pieces']]
    activities = Counter(v['activity'] for v in created)
    learned = Counter()
    for person in per_person.values():
        learned.update(person['facts'])
    return {
        'living_people': len(per_person), 'per_person': per_person,
        'living_participants': {k: sum(p[k] > 0 for p in per_person.values())
                                for k in ['goals', 'conversations', 'witnessed', 'skill_growth']},
        'new_retained_works': len(created), 'new_work_forms': dict(activities),
        'new_work_creators': len({v['creator'] for v in created}),
        'new_work_evidence_items': sum(len(v.get('evidence', [])) for v in created),
        'retained_fact_namespaces': dict(learned),
        'living_affectionate_edges': sum(
            relation['affection'] >= 30 for p, ties in state['ties'].items()
            if p in per_person for other, relation in ties.items() if other in per_person),
        'limits': 'Final living cohorts differ after deaths; counters include only their progress. '
                  'Retained facts/works are not total learning/creation. No single metric establishes realism.'}
