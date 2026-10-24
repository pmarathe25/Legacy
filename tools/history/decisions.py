#!/usr/bin/env python3
"""Measure explicitly different decision policies from an unchanged checkpoint.

Linux/POSIX; keep results under target/history. Never use this evidence to
approve a behavior-preserving runtime upgrade. Run without other simulations or
builds. All variants use the same frozen executable and absolute end day.
"""
import argparse
import copy
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

from state_hashes import artifact_digest, WORLD_CANONICAL

ARTIFACTS = ['checkpoint.json', 'checkpoint-ledger.json', 'events.jsonl', 'people.json']
FINAL_FIELDS = ['alive', 'mortals', 'births', 'pregnancies', 'generations', 'hungry',
                'injured', 'actions', 'care', 'goals', 'work_failures', 'trade', 'works']


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def policies(world, options, reviews, limits=(16,), review_millis=(), completion_minutes=()):
    """Every fork differs only in the declared character rules; retain the baseline."""
    rules = world['resources']['characters.minds']['rules']
    if rules.get('routine') is not None or rules.get('option_memory') is not None or rules.get('care_signal_changes_only', False):
        raise ValueError('source must have the ordinary decision policy')
    if any(n <= 0 for n in options + reviews + list(limits) + list(review_millis) + list(completion_minutes)):
        raise ValueError('policy intervals must be positive minutes')
    if any(n > 256 for n in limits):
        raise ValueError('option capacity cannot exceed 256')
    variants = {'ordinary': {}}
    for minutes in options:
        for limit in limits:
            variants[f'options-{minutes}-limit-{limit}'] = {
                'option_memory': minutes * 60000, 'option_limit': limit}
    intervals = [(f'reviews-{n}', n*60000) for n in reviews]
    intervals += [(f'reviews-{n}ms', n) for n in review_millis]
    for name, ticks in intervals:
        variants[name] = {'routine': {
            'review_floor': ticks, 'minimum': 1800000,
            'maximum': max(7200000, ticks),
            'kinds': ['experiment', 'reason', 'inspect-food', 'express-need',
                      'exchange-information']}}
    for minutes in completion_minutes:
        variants[f'completion-routines-{minutes}'] = {'routine': {
            'preserve_on_care_completion': True,
            'minimum': minutes*60000, 'maximum': minutes*60000,
            'kinds': ['experiment', 'reason', 'inspect-food', 'express-need',
                      'exchange-information']}}
    return variants


def declared_policies(path, variants):
    if path is None:
        return variants
    patches = json.loads(path.read_text())
    if not isinstance(patches, dict) or not patches:
        raise ValueError('policy file must contain a nonempty name-to-rules-patch object')
    for name, patch in patches.items():
        if not re.fullmatch(r'[a-z][a-z0-9-]{0,63}', name) or name in variants:
            raise ValueError('invalid or reserved policy name')
        if not isinstance(patch, dict) or not patch:
            raise ValueError('each policy must declare a nonempty rules patch')
    return {**variants, **patches}


def fork(world, patch):
    # Only the changed branch needs copying; the rest is read solely by json.dump.
    result = dict(world)
    result['resources'] = dict(world['resources'])
    minds = result['resources']['characters.minds'] = dict(world['resources']['characters.minds'])
    minds['rules'] = dict(minds['rules'])
    minds['rules'].update(copy.deepcopy(patch))
    return result


def _measure_child(command, environment, log):
    """wait4 gives this child's CPU/RSS, rather than a cumulative child maximum."""
    started = time.perf_counter()
    with subprocess.Popen(command, env=environment, stdout=log, stderr=log) as child:
        try:
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
        except BaseException:
            child.kill()
            child.wait()
            raise
        if child.returncode:
            raise subprocess.CalledProcessError(child.returncode, command)
    return {'wall_seconds': time.perf_counter() - started,
            'cpu_seconds': usage.ru_utime + usage.ru_stime,
            'user_seconds': usage.ru_utime, 'system_seconds': usage.ru_stime,
            'rss_kib': usage.ru_maxrss}


def measure(command, environment, log):
    # A fresh helper prevents the parent's decoded world from inflating pre-exec RSS.
    with tempfile.TemporaryDirectory(prefix='legacy-measure-') as directory:
        result = Path(directory)/'time.json'
        started = time.perf_counter()
        subprocess.run([sys.executable, str(Path(__file__).resolve()), '--measure',
                        str(result), *command], env=environment, stdout=log, stderr=log, check=True)
        metrics = json.loads(result.read_text())
        metrics['wall_seconds'] = time.perf_counter()-started
        return metrics


def behavior(folder, prefix_counts):
    summary = json.loads((folder/'summary.json').read_text())
    final = summary['final']
    daily = [json.loads((folder/'initial.json').read_text())]
    daily += [json.loads(line) for line in (folder/'daily.jsonl').read_text().splitlines()]
    # Left endpoints approximate occupancy; they are daily samples, not action durations.
    totals = dict.fromkeys(['alive', 'hungry', 'injured', 'acting'], 0)
    for previous, current in zip(daily, daily[1:]):
        span = current['day'] - previous['day']
        for key in ['alive', 'hungry', 'injured']:
            totals[key] += previous[key] * span
        totals['acting'] += sum(previous['actions'].values()) * span
    counts = final['ledger']['events']
    return {'final': {key: final[key] for key in FINAL_FIELDS},
            'events': {key: counts.get(key, 0)-prefix_counts.get(key, 0)
                       for key in sorted(counts.keys() | prefix_counts.keys())},
            'daily_sampled_person_days': totals,
            'extinct': summary['extinct'], 'end_day': summary['days'],
            'checkpoint_bytes': (folder/'checkpoint.json').stat().st_size}


def prune(folder):
    for artifact in ARTIFACTS + ['initial.json', 'daily.jsonl']:
        (folder/artifact).unlink()


def study(args):
    if args.pairs < 1 or args.workers < 1 or args.verify_workers < 1:
        raise ValueError('pairs and worker counts must be positive')
    source, binary, root = args.checkpoint.resolve(), args.binary.resolve(), args.output.resolve()
    world = json.loads((source/'checkpoint.json').read_text())
    variants = policies(world, args.option_minutes, args.review_minutes, args.option_limits,
                        args.review_millis, getattr(args, 'completion_minutes', ()))
    policy_file = getattr(args, 'policies', None)
    variants = declared_policies(policy_file, variants)
    prefix = json.loads((source/'checkpoint-ledger.json').read_text())
    start_day = (world['time']-prefix['epoch']) / 86400000
    if args.days <= start_day:
        raise ValueError('absolute end day must follow the checkpoint')
    root.mkdir(parents=True, exist_ok=False)
    inputs = {'ordinary': source}
    for name, patch in variants.items():
        if name == 'ordinary':
            continue
        folder = root/('input-'+name)
        folder.mkdir()
        with (folder/'checkpoint.json').open('w') as output:
            json.dump(fork(world, patch), output, separators=(',', ':'))
        shutil.copyfile(source/'checkpoint-ledger.json', folder/'checkpoint-ledger.json')
        inputs[name] = folder
    seed = world['seed']
    del world
    report = {'mode': 'behavioral', 'equivalent': False, 'hash_mode': WORLD_CANONICAL,
              'binary': str(binary), 'binary_sha256': digest(binary), 'seed': seed,
              'driver_sha256': digest(Path(__file__)),
              'checkpoint': str(source), 'start_day': start_day, 'end_day': args.days,
              'pairs': args.pairs, 'workers': args.workers,
              'policies': variants, 'inputs': {key: {name: digest(path/name)
                    for name in ARTIFACTS[:2]} for key, path in inputs.items()},
              'runs': [], 'verification': []}
    if policy_file is not None:
        report['policy_file'] = {'path': str(policy_file.resolve()), 'sha256': digest(policy_file)}
    report_path = root/'comparison.json'
    def save():
        report_path.write_text(json.dumps(report, indent=2)+'\n')
    save()
    environment = dict(os.environ)
    environment.pop('LEGACY_PROFILE', None)
    expected = {}

    def run(name, folder, workers, reload_day=None, profiled=False):
        folder.mkdir()
        command = [str(binary), '--resume', str(inputs[name]), '--days', str(args.days),
                   '--workers', str(workers), '--sample-days', '1',
                   '--checkpoint-days', '365000', '--final-save', 'false',
                   '--continue-after-extinction', 'true', '--journal', 'notable',
                   '--prepare-window', '0', '--parallel-listeners', 'false',
                   '--output', str(folder)]
        if reload_day is not None:
            command += ['--reload-day', str(reload_day)]
        env = dict(environment)
        if profiled:
            env['LEGACY_PROFILE'] = '1'
        with (folder/'process.log').open('w') as log:
            metrics = measure(command, env, log)
        hashes = {name: artifact_digest(folder/name, WORLD_CANONICAL) for name in ARTIFACTS}
        if name in expected and hashes != expected[name]:
            raise ValueError(f'nondeterministic {name}; outputs retained')
        expected[name] = hashes
        outcomes = behavior(folder, prefix['counts'])
        if outcomes['end_day'] != args.days:
            raise ValueError('run did not reach the fixed horizon; outputs retained')
        return {'policy': name, 'metrics': metrics, 'hashes': hashes, **outcomes}

    for pair in range(args.pairs):
        names = list(variants)
        if pair % 2:
            names.reverse()
        for name in names:
            row = run(name, root/f'{pair+1:02d}-{name}', args.workers)
            report['runs'].append({'pair': pair+1, **row})
            save()
            print(pair+1, name, f'{row["metrics"]["wall_seconds"]:.3f}s',
                  {k: row['final'][k] for k in ['alive', 'hungry', 'injured']}, flush=True)
            if pair+1 < args.pairs:
                prune(root/f'{pair+1:02d}-{name}')
    report['median'] = {name: {key: statistics.median(row['metrics'][key]
        for row in report['runs'] if row['policy'] == name)
        for key in ['wall_seconds', 'cpu_seconds', 'rss_kib']} for name in variants}
    base = report['median']['ordinary']['wall_seconds']
    report['wall_reduction_percent'] = {name: 100*(1-row['wall_seconds']/base)
                                        for name, row in report['median'].items()}
    save()
    # Worker/reload and instrumented runs are separate from accepted timing rounds.
    for name in variants:
        row = run(name, root/f'verify-{name}', args.verify_workers, args.reload_day)
        report['verification'].append({'policy': name, 'workers': args.verify_workers,
                                      'reload_day': args.reload_day, 'hashes': row['hashes']})
        prune(root/f'verify-{name}')
        save()
        if args.profile:
            folder = root/f'profile-{name}'
            run(name, folder, args.workers, profiled=True)
            report.setdefault('profiles', {})[name] = json.loads((folder/'summary.json').read_text())['profile']
            prune(folder)
            save()
    if policy_file is not None and digest(policy_file) != report['policy_file']['sha256']:
        raise ValueError('declared policies changed during the experiment')
    if digest(binary) != report['binary_sha256']:
        raise ValueError('frozen executable changed during the experiment')
    if digest(Path(__file__)) != report['driver_sha256']:
        raise ValueError('experiment driver changed during the experiment')
    for key, path in inputs.items():
        if {name: digest(path/name) for name in ARTIFACTS[:2]} != report['inputs'][key]:
            raise ValueError('input checkpoint changed during the experiment')
    report['within_policy_replay_equal'] = True
    save()
    print(json.dumps(report['wall_reduction_percent'], indent=2), flush=True)


if __name__ == '__main__':
    if sys.argv[1:2] == ['--measure']:
        Path(sys.argv[2]).write_text(json.dumps(_measure_child(sys.argv[3:], dict(os.environ), sys.stdout)))
        raise SystemExit(0)
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['checkpoint', 'binary', 'output']:
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--days', type=int, required=True)
    parser.add_argument('--pairs', type=int, default=3)
    parser.add_argument('--workers', type=int, default=1)
    parser.add_argument('--option-minutes', type=int, nargs='*', default=[])
    parser.add_argument('--option-limits', type=int, nargs='+', default=[16])
    parser.add_argument('--review-minutes', type=int, nargs='*', default=[])
    parser.add_argument('--review-millis', type=int, nargs='*', default=[])
    parser.add_argument('--completion-minutes', type=int, nargs='*', default=[],
                        help='fixed routine window; preserve through self-completion care prompts')
    parser.add_argument('--verify-workers', type=int, default=4)
    parser.add_argument('--reload-day', type=int)
    parser.add_argument('--policies', type=Path, help='JSON object of named character-rule patches; ordinary is reserved')
    parser.add_argument('--profile', action='store_true', help='separate untimed instrumented replays')
    study(parser.parse_args())
