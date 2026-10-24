#!/usr/bin/env python3
"""Verify a completed behavioral study with workers/reload, outside accepted timing."""
import argparse
import json
import os
from pathlib import Path

from decisions import ARTIFACTS, digest, measure, prune
from state_hashes import artifact_digest


def verify(args):
    source = args.report.resolve()
    original = json.loads(source.read_text())
    if not original['complete']:
        raise ValueError('verification requires a completed study')
    binary = Path(original['binary'])
    if digest(binary) != original['binary_sha256']:
        raise ValueError('frozen binary changed')
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    report = {'complete': False, 'source_report': str(source), 'source_sha256': digest(source),
              'driver_sha256': digest(Path(__file__)), 'binary_sha256': digest(binary), 'runs': [],
              'scope': 'Complete artifacts within the selected model; excludes accepted runtime medians.'}
    environment = dict(os.environ)
    for key in ['LEGACY_PROFILE', 'LEGACY_REVIEW_TRACE']:
        environment.pop(key, None)
    environment.update(LEGACY_MEMORY_INDEX='0', LEGACY_APPRAISAL_CACHE='0')
    for case_name, case in original['cases'].items():
        models = args.models or sorted({r['model'] for r in case['runs']} - {'ordinary'})
        for model in models:
            rows = [r for r in case['runs'] if r['model'] == model]
            if not rows or any(r['hashes'] != rows[0]['hashes'] for r in rows):
                raise ValueError('missing or inconsistent model')
            old = rows[0]
            command = old['command'].copy()
            checkpoint = Path(command[command.index('--resume')+1])
            if any(digest(checkpoint/a) != h for a, h in case['inputs'][model].items()):
                raise ValueError('study input changed')
            world = json.loads((checkpoint/'checkpoint.json').read_text())
            ledger = json.loads((checkpoint/'checkpoint-ledger.json').read_text())
            start = (world['time']-ledger['epoch'])/86400000
            del world
            reload_at = int(start)+(case['end_day']-int(start))//2 if case['end_day']-start > 1 else 0
            folder = root/(case_name+'-'+model)
            folder.mkdir()
            for flag, value in [('--workers', args.workers), ('--reload-day', reload_at), ('--output', folder)]:
                command[command.index(flag)+1] = str(value)
            with (folder/'run.log').open('w') as log:
                metrics = measure(command, environment, log)
            hashes = {a: artifact_digest(folder/a, original['hash_mode']) for a in ARTIFACTS}
            if hashes != old['hashes']:
                raise ValueError(f'{case_name}/{model}: replay differs; retained')
            report['runs'].append({'case': case_name, 'model': model, 'workers': args.workers,
                                   'reload_day': reload_at, 'hashes': hashes, 'metrics': metrics,
                                   'command': command})
            (root/'verification.json').write_text(json.dumps(report, indent=2)+'\n')
            prune(folder)
            print(case_name, model, 'worker/reload artifacts match', flush=True)
    if digest(source) != report['source_sha256'] or digest(binary) != report['binary_sha256'] or digest(Path(__file__)) != report['driver_sha256']:
        raise ValueError('verification source changed')
    report['complete'] = True
    (root/'verification.json').write_text(json.dumps(report, indent=2)+'\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--models', nargs='+')
    parser.add_argument('--workers', type=int, default=4)
    args = parser.parse_args()
    if not 1 <= args.workers <= 64:
        parser.error('workers must be in 1..64')
    verify(args)
