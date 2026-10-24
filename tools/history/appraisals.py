#!/usr/bin/env python3
"""Paired exact-replay evaluation of the incremental appraisal views.

Run with no other builds, simulations, or profiling. Both cache settings use the
same frozen binary and unchanged inputs. Verification and traces are untimed.
"""
import argparse
import json
import os
from pathlib import Path
import statistics

from decisions import ARTIFACTS, digest, measure, prune
from state_hashes import WORLD_CANONICAL, artifact_digest

CASES = [('opening', 'baseline-profile', 12, 11),
         ('older', 'warm-prefix', 330, 315),
         ('scale', 'integration-1000-prefix', 2, None)]


def run(args):
    binary = args.binary.resolve()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=args.resume)
    fixtures = args.fixtures.resolve()
    binaries = {'candidate': binary}
    if args.control_binary:
        binaries['previous'] = args.control_binary.resolve()
    report = {'hash_mode': WORLD_CANONICAL, 'pairs': args.pairs,
              'binaries': {key: {'path': str(path), 'sha256': digest(path)} for key, path in binaries.items()},
              'driver_sha256': digest(Path(__file__)), 'cases': {}}
    if args.resume:
        retained = json.loads((root/'comparison.json').read_text())
        for key in ['pairs', 'binaries', 'driver_sha256']:
            if retained[key] != report[key]:
                raise ValueError(f'resume provenance differs: {key}')
        report = retained
    def save():
        (root/'comparison.json').write_text(json.dumps(report, indent=2)+'\n')
    for name, fixture, days, reload_day in CASES:
        source = fixtures/fixture
        result = report['cases'].setdefault(name, {
            'checkpoint': str(source), 'days': days, 'runs': [], 'verification': [],
            'inputs': {key: digest(source/key) for key in ARTIFACTS[:2]}})
        if result['checkpoint'] != str(source) or result['days'] != days:
            raise ValueError('resume workload differs')
        if result['inputs'] != {key: digest(source/key) for key in ARTIFACTS[:2]}:
            raise ValueError('resume input differs')
        expected = None
        def replay(label, cache, workers=1, reload_at=None, executable=binary, timed=False):
            nonlocal expected
            folder = root/f'{name}-{label}'
            completed = next((row for row in result['runs']+result['verification'] if row['label'] == label), None)
            if completed:
                if expected is not None and completed['hashes'] != expected:
                    raise ValueError('resume contains differing replay hashes')
                expected = completed['hashes']
                if completed['retained_artifacts'] and any(
                        artifact_digest(folder/key, WORLD_CANONICAL) != value
                        for key, value in expected.items()):
                    raise ValueError('retained replay artifacts changed')
                print(name, label, 'retained completed sample', flush=True)
                return
            if folder.exists():
                interrupted = folder.with_name(folder.name+f'-interrupted-{len(result.get("discarded_partials", []))+1}')
                folder.rename(interrupted)
                result.setdefault('discarded_partials', []).append(str(interrupted))
                save()
            folder.mkdir()
            command = [str(executable), '--resume', str(source), '--days', str(days),
                       '--workers', str(workers), '--sample-days', '1', '--checkpoint-days', '365000',
                       '--final-save', 'false', '--continue-after-extinction', 'true',
                       '--journal', 'notable', '--prepare-window', '0', '--parallel-listeners', 'false',
                       '--output', str(folder)]
            if reload_at is not None:
                command += ['--reload-day', str(reload_at)]
            env = dict(os.environ)
            env.pop('LEGACY_PROFILE', None)
            env.pop('LEGACY_REVIEW_TRACE', None)
            env['LEGACY_APPRAISAL_CACHE'] = str(cache)
            with (folder/'process.log').open('w') as log:
                metrics = measure(command, env, log)
            hashes = {key: artifact_digest(folder/key, WORLD_CANONICAL) for key in ARTIFACTS}
            row = {'label': label, 'cache': cache, 'workers': workers, 'reload_day': reload_at,
                   'hashes': hashes, 'retained_artifacts': timed and label.startswith(f'{args.pairs:02d}-')}
            if timed:
                row['metrics'] = metrics
            (result['runs'] if timed else result['verification']).append(row)
            save()
            if expected is not None and hashes != expected:
                raise ValueError(f'{name}/{label}: replay mismatch; artifacts retained')
            expected = hashes
            if not row['retained_artifacts']:
                prune(folder)
            print(name, label, f'{metrics["wall_seconds"]:.3f}s', 'equal', flush=True)
        if 'previous' in binaries:
            replay('previous', 0, executable=binaries['previous'])
        for pair in range(args.pairs):
            for cache in ([0, 1] if pair%2 == 0 else [1, 0]):
                replay(f'{pair+1:02d}-cache-{cache}', cache, timed=True)
        result['median'] = {str(cache): {key: statistics.median(row['metrics'][key]
            for row in result['runs'] if row['cache'] == cache)
            for key in ['wall_seconds', 'cpu_seconds', 'rss_kib']} for cache in [0, 1]}
        result['wall_reduction_percent'] = 100*(1-result['median']['1']['wall_seconds']/result['median']['0']['wall_seconds'])
        for cache in [0, 1]:
            replay(f'verify-cache-{cache}', cache, workers=4, reload_at=reload_day)
        for key, value in result['inputs'].items():
            if digest(source/key) != value:
                raise ValueError('fixture changed')
        result['all_equal'] = True
        save()
    if any(digest(path) != report['binaries'][key]['sha256'] for key, path in binaries.items()):
        raise ValueError('binary changed')
    if digest(Path(__file__)) != report['driver_sha256']:
        raise ValueError('driver changed')
    report['all_equal'] = True
    save()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--control-binary', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, default=Path('target/history/optimization'))
    parser.add_argument('--pairs', type=int, default=3)
    parser.add_argument('--resume', action='store_true', help='retain completed samples after an interrupted run')
    args = parser.parse_args()
    if args.pairs < 1:
        parser.error('--pairs must be positive')
    run(args)
