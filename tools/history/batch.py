#!/usr/bin/env python3
"""Replay independent saved histories concurrently; compare batch throughput exactly.

Each child uses one simulation worker. Only the declared children overlap within
a timed group. Run without other benchmarks, builds, tests or profiling.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import json
import os
from pathlib import Path
import re
import statistics
import time

from decisions import ARTIFACTS, digest, measure, prune
from state_hashes import EVIDENCE_CANONICAL, artifact_digest


def parse_plan(path):
    plan = json.loads(path.read_text())
    binary = (path.parent/plan['binary']).resolve()
    runs = []
    names = set()
    for item in plan['runs']:
        name, days = item['name'], item['days']
        if (not isinstance(name, str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name)
                or name in names or type(days) is not int or days <= 0):
            raise ValueError('unique simple names and positive integer end days required')
        names.add(name)
        source = (path.parent/item['checkpoint']).resolve()
        hashes = {a: digest(source/a) for a in ARTIFACTS[:2]}
        runs.append({'name': name, 'days': days, 'checkpoint': str(source), 'inputs': hashes})
    if not runs:
        raise ValueError('at least one saved history is required')
    return binary, runs


def execute_group(commands, environment, concurrency):
    """A failure requests committed-day stops and cancels children not yet started."""
    futures = []
    with ThreadPoolExecutor(max_workers=concurrency) as pool:
        def execute(row):
            with (row['folder']/'run.log').open('w') as log:
                return measure(row['command'], environment, log)
        started = time.monotonic()
        try:
            for row in commands:
                futures.append(pool.submit(execute, row))
            metrics = [None]*len(futures)
            positions = {future: i for i, future in enumerate(futures)}
            for future in as_completed(futures):
                metrics[positions[future]] = future.result()
        except BaseException:
            for row in commands:
                (row['folder']/'stop-request').write_text('batch interrupted; stop after a committed day\n')
            for future in futures:
                future.cancel()
            raise
    return time.monotonic()-started, metrics


def study(args):
    plan = args.plan.resolve()
    binary, runs = parse_plan(plan)
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    drivers = [Path(__file__), Path(__file__).with_name('decisions.py'),
               Path(__file__).with_name('state_hashes.py')]
    report = {'complete': False, 'mode': 'independent-history-throughput',
              'plan_sha256': digest(plan), 'binary': str(binary), 'binary_sha256': digest(binary),
              'drivers': {str(p): digest(p) for p in drivers}, 'inputs': runs,
              'rounds': args.rounds, 'concurrency': args.concurrency, 'runs': [],
              'hash_mode': EVIDENCE_CANONICAL,
              'cpu_quota': Path('/sys/fs/cgroup/cpu.max').read_text().strip(),
              'affinity': sorted(os.sched_getaffinity(0)),
              'scope': 'Batch elapsed excludes post-run hashing. Sum of child peak RSS is an upper '
                       'bound, not simultaneous peak RSS. Throughput gain applies to independent histories.'}
    def save():
        (root/'comparison.json').write_text(json.dumps(report, indent=2)+'\n')
    save()
    environment = dict(os.environ)
    for key in ['LEGACY_PROFILE', 'LEGACY_REVIEW_TRACE']:
        environment.pop(key, None)
    environment.update(LEGACY_MEMORY_INDEX='0', LEGACY_APPRAISAL_CACHE='0')
    expected = {}
    for round_index in range(args.rounds):
        order = args.concurrency[round_index % len(args.concurrency):]+args.concurrency[:round_index % len(args.concurrency)]
        for concurrency in order:
            commands = []
            label = f'{round_index+1:02d}-jobs-{concurrency}'
            for run in runs:
                folder = root/label/run['name']
                folder.mkdir(parents=True)
                command = [str(binary), '--resume', run['checkpoint'], '--days', str(run['days']),
                    '--workers', '1', '--resident-executor', 'true', '--parallel-listeners', 'false',
                    '--prepare-window', '0', '--final-save', 'false', '--continue-after-extinction', 'true',
                    '--journal', 'notable', '--stop-file', str(folder/'stop-request'), '--output', str(folder)]
                commands.append({'folder': folder, 'command': command})
            wall, metrics = execute_group(commands, environment, concurrency)
            group = {'round': round_index+1, 'concurrency': concurrency,
                     'wall_seconds': wall, 'cpu_seconds': sum(m['cpu_seconds'] for m in metrics),
                     'sum_child_peak_rss_kib': sum(m['rss_kib'] for m in metrics), 'histories': []}
            for run, command, timing in zip(runs, commands, metrics):
                folder = command['folder']
                summary = json.loads((folder/'summary.json').read_text())
                if summary['days'] != run['days'] or summary.get('stopped'):
                    raise ValueError('history stopped before its fixed horizon')
                hashes = {a: artifact_digest(folder/a, EVIDENCE_CANONICAL) for a in ARTIFACTS}
                if run['name'] in expected and expected[run['name']] != hashes:
                    raise ValueError(f'{run["name"]}: concurrency changed complete artifacts; retained')
                expected[run['name']] = hashes
                group['histories'].append({'name': run['name'], 'command': command['command'],
                    'metrics': timing, 'hashes': hashes, 'raw_hashes': {a: digest(folder/a) for a in ARTIFACTS}})
                if not args.retain_state:
                    prune(folder)
            report['runs'].append(group)
            save()
            print(label, f'{wall:.3f}s for {len(runs)} histories', flush=True)
    for run in runs:
        source = Path(run['checkpoint'])
        if any(digest(source/a) != h for a, h in run['inputs'].items()):
            raise ValueError('source checkpoint changed')
    if (digest(binary) != report['binary_sha256'] or digest(plan) != report['plan_sha256']
            or any(digest(p) != report['drivers'][str(p)] for p in drivers)):
        raise ValueError('binary, driver or plan changed')
    report['median_seconds'] = {str(n): statistics.median(
        r['wall_seconds'] for r in report['runs'] if r['concurrency'] == n) for n in args.concurrency}
    report['complete'] = True
    save()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--concurrency', type=int, nargs='+', default=[1])
    parser.add_argument('--rounds', type=int, default=1)
    parser.add_argument('--retain-state', action='store_true')
    args = parser.parse_args()
    if (args.rounds < 1 or len(set(args.concurrency)) != len(args.concurrency)
            or any(not 1 <= n <= 64 for n in args.concurrency)):
        parser.error('positive rounds and unique concurrency levels in 1..64 required')
    study(args)
