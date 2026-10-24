#!/usr/bin/env python3
"""Summarize optional review-attempt JSONL without treating it as committed history.

Build game-history with --features review-trace, then set LEGACY_REVIEW_TRACE to
an unused output path. This instrumented replay is never an accepted timing run.
Fingerprints cover ordered numeric proposals, not payloads or all decision inputs.
"""
import argparse
from collections import Counter
import json
from pathlib import Path


def chain(row):
    return ' > '.join(f'{kind}:{key}' for kind, key in row.get('stack', [])) or '(agenda/unknown)'


def trigger(row):
    for wanted in ['listener', 'event', 'request']:
        for kind, key in reversed(row.get('stack', [])):
            if kind == wanted:
                return key
    return '(agenda/unknown)'


def summarize(rows):
    counts = Counter()
    wakes, causes, changed = Counter(), Counter(), Counter()
    wake_listeners, cause_listeners = Counter(), Counter()
    accepted_listeners = Counter()
    previous = {}
    for row in rows:
        kind = row.get('type')
        if kind == 'wake':
            counts['wake_attempts'] += 1
            counts['suppressed_wakes' if row['suppressed'] else 'accepted_wakes'] += 1
            if row.get('during_review') is not None:
                counts['wakes_during_review'] += 1
                if not row['suppressed']:
                    counts['accepted_wakes_during_review'] += 1
            wakes[chain(row)] += 1
            wake_listeners[trigger(row)] += 1
            if not row['suppressed']:
                accepted_listeners[trigger(row)] += 1
        elif kind == 'review':
            counts['review_attempts'] += 1
            if row.get('success') is True:
                counts['successful_review_attempts'] += 1
            elif row.get('success') is False:
                counts['failed_review_attempts'] += 1
            else:
                counts['deferred_or_early_exit_attempts'] += 1
            if 'candidates' in row:
                counts['reviews_with_numeric_candidates'] += 1
            for cause in row.get('causes', []):
                causes[chain(cause)] += 1
                cause_listeners[trigger(cause)] += 1
            if not row.get('causes'):
                causes[chain(row)] += 1
                cause_listeners[trigger(row)] += 1
            person = row['person']
            before, after = row.get('before', {}), row.get('after', {})
            if row.get('success') is True and before.get('chosen') == after.get('chosen'):
                counts['successful_attempts_retaining_choice'] += 1
                if 'candidates' in row:
                    counts['numeric_candidate_reviews_retaining_choice'] += 1
            old = previous.get(person)
            if old:
                if old['at'] == row['at']:
                    counts['same_clock_repeated_reviews'] += 1
                candidates = row.get('candidates')
                if candidates is not None and candidates == old.get('candidates'):
                    counts['same_numeric_candidates_as_previous'] += 1
                    if row.get('before') == old.get('before'):
                        counts['same_numeric_candidates_and_observed_state'] += 1
                previous_state = old.get('before', {})
                for field in previous_state.keys() | before.keys():
                    if previous_state.get(field) != before.get(field):
                        changed[field] += 1
            previous[person] = row
    top = lambda counter: [{'chain': key, 'count': count} for key, count in counter.most_common(12)]
    return {'counts': dict(counts), 'people': len(previous), 'wake_chains': top(wakes),
            'review_causes': top(causes), 'changed_observed_fields': dict(changed),
            'wake_listeners': dict(wake_listeners.most_common()),
            'accepted_wake_listeners': dict(accepted_listeners.most_common()),
            'review_cause_listeners': dict(cause_listeners.most_common()),
            'limits': ['Attempts may roll back; success is local, not proof of commit.',
                       'Equal numeric fingerprints do not prove equal payloads or complete inputs.',
                       'Pending causes are thread-local, capped at 8 per person and 4096 people.',
                       'Observed state excludes many owner resources; retention is not redundancy.']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('trace', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    with args.trace.open() as source:
        result = summarize(json.loads(line) for line in source if line.strip())
    encoded = json.dumps(result, indent=2)+'\n'
    if args.output:
        args.output.write_text(encoded)
    else:
        print(encoded, end='')


if __name__ == '__main__':
    main()
