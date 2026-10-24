#!/usr/bin/env python3
"""Compare exact parallel drafts and the explicitly different decision-frame model.

Run without builds, profiling or other simulations. Timings include full histories
and export. Check complete replay artifacts within each model; compare outcomes
between models. A frame speedup cannot approve an ordinary-policy runtime upgrade.
"""
import argparse
import json
import os
from pathlib import Path
import statistics

from decisions import ARTIFACTS, behavior, digest, measure, prune
from state_hashes import WORLD_CANONICAL, artifact_digest

CASES = {
    'opening': ('baseline-profile', 12, 11),
    'older': ('warm-prefix', 330, 315),
    'heldout-19': ('policy-prefix-19', 330, 315),
    'heldout-37': ('policy-prefix-37', 330, 315),
    'scale': ('integration-1000-prefix', 2, None),
}


def run(args):
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve()
    binaries = {'candidate': binary}
    baseline = getattr(args, 'baseline_binary', None)
    if baseline:
        binaries['baseline'] = baseline.resolve()
    if args.control_binary:
        binaries['previous'] = args.control_binary.resolve()
    if args.control_frame_binary:
        binaries['previous-frame'] = args.control_frame_binary.resolve()
    if args.control_recovery_binary:
        binaries['previous-recovery'] = args.control_recovery_binary.resolve()
    if args.control_decisions_binary:
        binaries['previous-decisions'] = args.control_decisions_binary.resolve()
    configs = []
    if 'ordinary' in args.models:
        configs.append(('ordinary-1', 1, False, None, 'ordinary'))
    if 'exact' in args.models:
        configs.append(('exact-parallel-4', 4, True, None, 'ordinary'))
    frame_models = ['frame', 'recovery', 'food', 'recovery-food', 'decisions', 'recovery-decisions']
    for group in frame_models:
        if group in args.models:
            configs += [(f'{group}-{workers}', workers,
                         group in ['decisions', 'recovery-decisions'] and not getattr(args, 'actor_only', False), args.minutes, group)
                        for workers in args.workers]
    env = os.environ.copy()
    for key in ['LEGACY_PROFILE', 'LEGACY_REVIEW_TRACE']:
        env.pop(key, None)
    env['LEGACY_APPRAISAL_CACHE'] = '0'
    report = {'hash_mode': WORLD_CANONICAL, 'minutes': args.minutes, 'milliseconds': args.milliseconds,
              'rounds': args.rounds, 'models': args.models, 'span_days': args.span_days,
              'end_day': getattr(args, 'end_day', None),
              'demand_clock': getattr(args, 'demand_clock', False),
              'actor_only': getattr(args, 'actor_only', False),
              'minimum_cohort': getattr(args, 'minimum_cohort', 0),
              'verify_workers': getattr(args, 'verify_workers', 4),
              'commit_batch': getattr(args, 'commit_batch', 0),
              'discard_verified': args.discard_verified,
              'resident_executor': args.resident_executor, 'active_executor': args.active_executor, 'affinity': sorted(os.sched_getaffinity(0)),
              'cpu_quota': Path('/sys/fs/cgroup/cpu.max').read_text().strip(),
              'driver_sha256': digest(Path(__file__)),
              'binaries': {k: {'path': str(p), 'sha256': digest(p)} for k, p in binaries.items()},
              'cases': {}, 'complete': False}

    def save():
        (root/'comparison.json').write_text(json.dumps(report, indent=2)+'\n')

    save()
    for name in args.cases:
        fixture, days, reload_day = CASES[name]
        source = (args.fixtures/fixture).resolve()
        prefix_counts = json.loads((source/'checkpoint-ledger.json').read_text())['counts']
        inputs = ARTIFACTS[:2].copy()
        end_day = getattr(args, 'end_day', None)
        if args.span_days or end_day is not None:
            inputs += ['summary.json']
            start = json.loads((source/'summary.json').read_text())['days']
            days = end_day if end_day is not None else min(days, start + args.span_days)
            if days <= start:
                raise ValueError('end day must follow the checkpoint')
            reload_day = start + (days-start)//2 if days-start > 1 else None
        result = {'inputs': {p: digest(source/p) for p in inputs},
                  'days': days, 'runs': [], 'verification': []}
        report['cases'][name] = result
        expected = {}

        def replay(label, workers, parallel, minutes, group, executable=binary,
                   reload_at=0, verification=False):
            folder = root/name/label
            folder.mkdir(parents=True)
            command = [str(executable), '--resume', str(source), '--days', str(days),
                       '--workers', str(workers), '--parallel-listeners', str(parallel).lower(),
                       '--prepare-window', '0', '--final-save', 'false',
                       '--continue-after-extinction', 'true', '--journal', 'notable',
                       '--reload-day', str(reload_at), '--output', str(folder)]
            if (args.resident_executor or args.active_executor) and executable == binary:
                command += ['--resident-executor', 'true']
            if args.active_executor and executable == binary:
                command += ['--active-executor', 'true']
            if minutes is not None:
                command += (['--decision-frame-millis', str(args.milliseconds)]
                            if args.milliseconds is not None else ['--decision-frame-minutes', str(minutes)])
                if group in ['recovery', 'recovery-food', 'recovery-decisions']:
                    command += ['--frame-recovery', 'true']
                if group in ['food', 'recovery-food']:
                    command += ['--frame-food', 'true']
                if group in ['decisions', 'recovery-decisions']:
                    command += ['--frame-decisions', 'true']
                if getattr(args, 'demand_clock', False):
                    command += ['--frame-demand', 'true']
                if getattr(args, 'minimum_cohort', 0):
                    command += ['--frame-minimum-cohort', str(args.minimum_cohort)]
                if getattr(args, 'commit_batch', 0):
                    command += ['--frame-commit-batch', str(args.commit_batch)]
            with (folder/'run.log').open('w') as log:
                metrics = measure(command, env, log)
            hashes = {p: artifact_digest(folder/p, WORLD_CANONICAL) for p in ARTIFACTS}
            summary = json.loads((folder/'summary.json').read_text())
            if summary['days'] != days:
                raise ValueError(f'{name}/{label}: did not reach the requested horizon; retained artifacts')
            final = summary['final']
            row = {'label': label, 'workers': workers, 'group': group, 'binary': str(executable),
                   'parallel_listeners': parallel,
                   'metrics': metrics, 'hashes': hashes, 'reload_day': reload_at,
                   'drafts': summary.get('drafts'), 'frames': summary.get('frames'),
                   'outcomes': {k: final.get(k) for k in [
                       'alive', 'births', 'pregnancies', 'hungry', 'injured', 'care',
                       'work_failures', 'actions', 'works', 'goals']},
                   'events': final['ledger']['events']}
            row['behavior'] = behavior(folder, prefix_counts)
            row['effective_cores'] = metrics['cpu_seconds']/metrics['wall_seconds']
            result['verification' if verification else 'runs'].append(row)
            save()
            if group in expected and hashes != expected[group]:
                raise ValueError(f'{name}/{label}: replay mismatch within {group}; retained artifacts')
            expected[group] = hashes
            print(name, label, f'{metrics["wall_seconds"]:.3f}s',
                  f'{row["effective_cores"]:.2f} cores', group+' equal', flush=True)
            if args.discard_verified or verification or not label.startswith(f'{args.rounds:02d}-'):
                prune(folder)

        if 'previous' in binaries:
            replay('previous', 1, False, None, 'ordinary', binaries['previous'], verification=True)
        if 'previous-frame' in binaries:
            replay('previous-frame', 1, False, args.minutes, 'frame', binaries['previous-frame'], verification=True)
        if 'previous-recovery' in binaries:
            replay('previous-recovery', 1, False, args.minutes, 'recovery', binaries['previous-recovery'], verification=True)
        if 'previous-decisions' in binaries:
            replay('previous-decisions', 1, False, args.minutes, 'recovery-decisions', binaries['previous-decisions'], verification=True)
        for n in range(args.rounds):
            for config in configs[n % len(configs):]+configs[:n % len(configs)]:
                label, workers, parallel, minutes, group = config
                executable = binaries.get('baseline', binary) if group == 'ordinary' else binary
                replay(f'{n+1:02d}-{label}', workers, parallel, minutes, group, executable)
        result['median'] = {}
        for label, *_ in configs:
            rows = [r for r in result['runs'] if r['label'].split('-', 1)[1] == label]
            result['median'][label] = {key: statistics.median(r['metrics'][key] for r in rows)
                                      for key in ['wall_seconds', 'cpu_seconds', 'rss_kib']}
        if reload_day is not None and args.verify_reload:
            check_workers = getattr(args, 'verify_workers', 4)
            if 'ordinary' in expected:
                replay('reload-ordinary', check_workers, False, None, 'ordinary', reload_at=reload_day, verification=True)
            for group in frame_models:
                if group in expected:
                    replay('reload-'+group, check_workers, False, args.minutes, group, reload_at=reload_day, verification=True)
        if any(digest(source/p) != value for p, value in result['inputs'].items()):
            raise ValueError('fixture changed during comparison')
        result['equal_within_models'] = True
        save()
    if any(digest(path) != report['binaries'][key]['sha256'] for key, path in binaries.items()):
        raise ValueError('binary changed during comparison')
    if digest(Path(__file__)) != report['driver_sha256']:
        raise ValueError('driver changed during comparison')
    report['complete'] = True
    save()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--baseline-binary', type=Path,
                        help='time ordinary histories with this frozen production runtime')
    parser.add_argument('--control-binary', type=Path)
    parser.add_argument('--control-frame-binary', type=Path)
    parser.add_argument('--control-recovery-binary', type=Path)
    parser.add_argument('--control-decisions-binary', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, default=Path('target/history/optimization'))
    parser.add_argument('--cases', nargs='+', choices=CASES, default=['opening', 'heldout-19', 'scale'])
    parser.add_argument('--workers', type=int, nargs='+', default=[1, 2, 4])
    parser.add_argument('--models', nargs='+', choices=['ordinary', 'exact', 'frame', 'recovery', 'food', 'recovery-food', 'decisions', 'recovery-decisions'], default=['ordinary', 'exact', 'frame'])
    horizons = parser.add_mutually_exclusive_group()
    horizons.add_argument('--span-days', type=int)
    horizons.add_argument('--end-day', type=int, help='override the fixed absolute horizon for selected cases')
    parser.add_argument('--demand-clock', action='store_true', help='request frames only for deadlines and new cues')
    parser.add_argument('--actor-only', action='store_true',
                        help='parallelize whole actors without nested provider fan-out')
    parser.add_argument('--minimum-cohort', type=int, default=0,
                        help='decide small cohorts live; larger cohorts prepare in parallel')
    parser.add_argument('--commit-batch', type=int, default=0,
                        help='resolve actors in atomic groups of 1..128; zero keeps individual jobs')
    parser.add_argument('--discard-verified', action='store_true')
    parser.add_argument('--minutes', type=int, default=1)
    parser.add_argument('--milliseconds', type=int)
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--verify-reload', action='store_true')
    parser.add_argument('--verify-workers', type=int, default=4)
    parser.add_argument('--resident-executor', action='store_true')
    parser.add_argument('--active-executor', action='store_true')
    args = parser.parse_args()
    if not 0 <= args.minimum_cohort <= 100_000:
        parser.error('minimum cohort must be 0..=100000')
    if not 1 <= args.verify_workers <= 64:
        parser.error('verification workers must be 1..=64')
    if not 0 <= args.commit_batch <= 128:
        parser.error('commit batch must be 0..=128')
    if args.rounds < 1 or args.minutes < 1 or (args.span_days is not None and args.span_days < 1) or (args.end_day is not None and args.end_day < 1) or (args.milliseconds is not None and args.milliseconds < 1) or any(w < 1 or w > 64 for w in args.workers):
        parser.error('rounds/minutes must be positive, workers must be 1..64')
    run(args)
