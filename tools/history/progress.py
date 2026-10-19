"""Read bounded live progress without treating it as a verified checkpoint."""
import json
import math
from pathlib import Path


def last_event_day(path, limit=65536):
    """Return the last readable complete event's day; never scan the whole log."""
    if limit <= 0:
        raise ValueError('tail limit must be positive')
    try:
        with Path(path).open('rb') as stream:
            size = stream.seek(0, 2)
            start = max(0, size-limit)
            stream.seek(start)
            data = stream.read(limit)
    except FileNotFoundError:
        return None
    lines = data.split(b'\n')[:-1]  # A writer may still be appending the last row.
    if start and lines:
        lines = lines[1:]  # The bounded read may start inside a row.
    for line in reversed(lines):
        try:
            day = json.loads(line).get('day')
            if (isinstance(day, (int, float)) and not isinstance(day, bool)
                    and math.isfinite(day) and day >= 0):
                return day
        except (ValueError, AttributeError, UnicodeError, OverflowError):
            continue
    return None


def live_progress(plan, state):
    """Keep verified population counts separate from a journal clock lower bound."""
    jobs = {job['name']: job for job in plan['jobs']}
    phases = {c['name']: c for c in (state.get('model_forks') or [{}])[-1].get('checkpoints', [])}
    rows = []
    for row in state['runs']:
        verified = row['days']
        observed = None
        if row['state'] == 'running' and row.get('active_chunk'):
            output = Path(plan['root'])/jobs[row['name']]['output']
            journal = output/f"chunk-{row['active_chunk']:04d}"/'events.jsonl'
            observed = last_event_day(journal)
        lower_bound = max(verified, observed if observed is not None else verified)
        rows.append({
            'name': row['name'], 'state': row['state'],
            'verified_days': verified,
            'live_day_lower_bound': lower_bound,
            'live_year_lower_bound': lower_bound/365,
            'verified_mortals': row['mortals'], 'verified_births': row['births'],
            'verified_depth': row['depth'],
        })
        if row['name'] in phases:
            phase = phases[row['name']]
            rows[-1].update(phase_start_days=phase['days'], phase_start_depth=phase['depth'],
                            verified_days_in_phase=max(0, verified-phase['days']),
                            live_days_in_phase=max(0, lower_bound-phase['days']))
    return rows


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('plan', type=Path)
    args = parser.parse_args()
    plan = json.loads(args.plan.read_text())
    state = json.loads((Path(plan['root'])/plan['artifacts']/'study.json').read_text())
    print(json.dumps(live_progress(plan, state), indent=2))
