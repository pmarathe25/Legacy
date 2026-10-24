#!/usr/bin/env python3
"""Paired stochastic continuations from one unchanged demographic/history state.

Only the future random seed and declared attention rules differ. These are
conditional continuations, not independent regenerated civilizations.
"""
import argparse
import json
from pathlib import Path
import shutil
from types import SimpleNamespace

import cadence
from decisions import ARTIFACTS, digest
from frames import CASES


def run(args):
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    fixture, end, reload_day = CASES[args.case]
    source = (args.fixtures/fixture).resolve()
    world = json.loads((source/'checkpoint.json').read_text())
    plan = {'complete': False, 'case': args.case, 'end_day': end,
            'future_seeds': args.seeds, 'attention_millis': args.intervals, 'busy_attention': args.busy_attention,
            'driver_sha256': digest(Path(__file__)),
            'inputs': {a: digest(source/a) for a in [*ARTIFACTS[:2], 'people.json', 'summary.json']},
            'scope': __doc__}
    path = root/'ensemble.json'
    path.write_text(json.dumps(plan, indent=2)+'\n')
    fixtures = root/'fixtures'
    fixtures.mkdir()
    cases = {}
    for seed in args.seeds:
        name = f'future-{seed}'
        folder = fixtures/name
        folder.mkdir()
        world['seed'] = seed
        with (folder/'checkpoint.json').open('w') as stream:
            json.dump(world, stream, separators=(',', ':'))
        for artifact in ['checkpoint-ledger.json', 'people.json', 'summary.json']:
            shutil.copyfile(source/artifact, folder/artifact)
        cases[name] = (name, end, reload_day)
    del world
    cadence.CASES = cases
    cadence.study(SimpleNamespace(binary=args.binary, output=root/'study', fixtures=fixtures,
                                 cases=list(cases), intervals=args.intervals, rounds=1, verify=False,
                                 busy_attention=args.busy_attention))
    if digest(Path(__file__)) != plan['driver_sha256']:
        raise ValueError('ensemble driver changed')
    if any(digest(source/a) != expected for a, expected in plan['inputs'].items()):
        raise ValueError('source history changed')
    plan.update(complete=True, report='study/comparison.json',
                report_sha256=digest(root/'study/comparison.json'))
    path.write_text(json.dumps(plan, indent=2)+'\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, default=Path('target/history/optimization'))
    parser.add_argument('--case', choices=CASES, default='heldout-37')
    parser.add_argument('--seeds', nargs='+', type=int, default=[101, 102, 103, 104, 105])
    parser.add_argument('--intervals', type=int, nargs='+', default=[1, 60000])
    parser.add_argument('--busy-attention', action='store_true')
    args = parser.parse_args()
    if (len(args.seeds) != len(set(args.seeds)) or any(not 0 <= n < 2**64 for n in args.seeds)
            or len(args.intervals) != len(set(args.intervals)) or any(not 0 < n <= 86400000 for n in args.intervals)):
        parser.error('unique u64 seeds and an attention window in (0, DAY] required')
    run(args)
