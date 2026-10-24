#!/usr/bin/env python3
"""Compare explicit attention policies on identical, fixed-horizon checkpoints.

Run after builds/tests finish. Timings include load, advance and export. Complete
replay equality is required within each policy; histories across policies differ.
"""
import argparse
import json
import os
from pathlib import Path
import statistics

from decisions import ARTIFACTS, behavior, digest, measure, prune
from frames import CASES
from state_hashes import WORLD_CANONICAL, EVIDENCE_CANONICAL, artifact_digest

RATES = {'one-minute': (1, 1, False), 'two-minute': (2, 1, False),
         'priority': (5, 5, False), 'continuation': (5, 5, True),
         'evidence-1': (5, 1, True), 'evidence-3': (5, 3, True),
         'planning-15': (15, 5, False), 'owner-15': (15, 5, True), 'owner-15-responsive': (15, 1, True)}
MODELS = ['ordinary', 'ordinary-reference', *RATES, 'frames']


def settings(model):
    if model in ('ordinary', 'ordinary-reference'):
        return []
    if model == 'frames':
        return ['--decision-frame-minutes', '5', '--frame-demand', 'true',
                '--frame-decisions', 'true', '--frame-recovery', 'true',
                '--frame-minimum-cohort', '12', '--frame-food', 'false',
                '--frame-commit-batch', '0']
    ordinary, evidence, execution = RATES[model]
    return ['--attention-minutes', str(ordinary), '--evidence-minutes', str(evidence),
            '--goal-execution', str(execution).lower()]


def run(args):
    binary, root = args.binary.resolve(), args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    for key in ['LEGACY_PROFILE', 'LEGACY_REVIEW_TRACE']:
        env.pop(key, None)
    env['LEGACY_APPRAISAL_CACHE'] = '0'
    report = {'complete': False, 'equivalent': False, 'hash_mode': args.hash_mode,
              'binary': str(binary), 'binary_sha256': digest(binary),
              'driver_sha256': digest(Path(__file__)), 'rounds': args.rounds,
              'policies': {m: settings(m) for m in args.models},
              'affinity': sorted(os.sched_getaffinity(0)),
              'cpu_quota': Path('/sys/fs/cgroup/cpu.max').read_text().strip(),
              'cases': {}}
    if args.richness:
        import richness
        report['richness_source_sha256'] = digest(Path(richness.__file__))
    reference_binary = args.reference_binary.resolve() if args.reference_binary else binary
    report['reference_binary'] = str(reference_binary)
    report['reference_binary_sha256'] = digest(reference_binary)

    def save():
        (root/'comparison.json').write_text(json.dumps(report, indent=2)+'\n')

    reference = json.loads(args.reference_report.read_text()) if args.reference_report else None
    if reference:
        if not reference['complete'] or reference['hash_mode'] != args.hash_mode:
            raise ValueError('reference must identify a completed compatible study/subset')
        report['reference'] = {'path': str(args.reference_report.resolve()),
                               'sha256': digest(args.reference_report),
                               'binary_sha256': reference['binary_sha256'],
                               'scope': reference.get('scope', 'complete study')}
    save()
    for name in args.cases:
        fixture, end, reload_day = CASES[name]
        source = (args.fixtures/fixture).resolve()
        initial_richness = richness.read(source) if args.richness else None
        prefix = json.loads((source/'checkpoint-ledger.json').read_text())['counts']
        if args.span_days:
            start = json.loads((source/'summary.json').read_text())['days']
            end = min(end, start+args.span_days)
            reload_day = start+(end-start)//2 if end-start > 1 else None
        case = report['cases'][name] = {
            'end_day': end, 'runs': [], 'verification': [],
            'inputs': {a: digest(source/a) for a in [*ARTIFACTS[:2], 'summary.json']}}
        expected = {}
        if reference:
            previous = reference['cases'][name]
            if previous['end_day'] != end or previous['inputs'] != case['inputs']:
                raise ValueError('reference horizon or input mismatch')
            for row in previous['runs']:
                key = row['model'].removesuffix('-reference')
                if key in expected and expected[key] != row['hashes']:
                    raise ValueError('reference contains unequal artifacts within policy')
                expected[key] = row['hashes']

        def replay(model, label, workers, reload_at=0, verification=False):
            folder = root/name/label
            folder.mkdir(parents=True)
            executable = reference_binary if model == 'ordinary-reference' else binary
            command = [str(executable), '--resume', str(source), '--days', str(end),
                       '--output', str(folder), '--workers', str(workers),
                       '--parallel-listeners', str(model == 'frames').lower(),
                       '--resident-executor', 'true', '--prepare-window', '0',
                       '--final-save', 'false', '--continue-after-extinction', 'true',
                       '--journal', 'notable', '--reload-day', str(reload_at),
                       *settings(model)]
            with (folder/'run.log').open('w') as log:
                child_env = {**env, 'LEGACY_MEMORY_INDEX': '0' if model.endswith('-reference') else str(args.memory_index)}
                metrics = measure(command, child_env, log)
            outcome = behavior(folder, prefix)
            if outcome['end_day'] != end:
                raise ValueError(f'{name}/{label}: requested horizon not reached')
            hashes = {a: artifact_digest(folder/a, args.hash_mode) for a in ARTIFACTS}
            raw_hashes = {a: digest(folder/a) for a in ARTIFACTS}
            canonical = model.removesuffix('-reference')
            if canonical in expected and hashes != expected[canonical]:
                raise ValueError(f'{name}/{label}: replay mismatch; artifacts retained')
            expected[canonical] = hashes
            row = {
                'model': model, 'workers': workers, 'reload_day': reload_at,
                'metrics': metrics, 'hashes': hashes, 'raw_hashes': raw_hashes, 'behavior': outcome,
                'command': command, 'memory_index': child_env['LEGACY_MEMORY_INDEX']}
            if args.richness and not verification:
                row['richness'] = richness.compare(initial_richness, folder)
            case['verification' if verification else 'runs'].append(row)
            save()
            prune(folder)
            print(name, label, f'{metrics["wall_seconds"]:.3f}s', flush=True)

        for n in range(args.rounds):
            models = args.models[n % len(args.models):]+args.models[:n % len(args.models)]
            for model in models:
                replay(model, f'{n+1:02d}-{model}', args.frame_workers if model == 'frames' else args.workers)
        if args.verify:
            for model in args.models:
                if args.reference_binary and model == 'ordinary-reference':
                    continue
                original = args.frame_workers if model == 'frames' else args.workers
                replay(model, 'verify-'+model, 1 if original > 1 else 4,
                       reload_day or 0, True)
        case['median_seconds'] = {model: statistics.median(
            r['metrics']['wall_seconds'] for r in case['runs'] if r['model'] == model)
            for model in args.models}
        case['equal_within_policy'] = True
        if any(digest(source/a) != h for a, h in case['inputs'].items()):
            raise ValueError('input changed during study')
        save()
    if digest(binary) != report['binary_sha256'] or digest(Path(__file__)) != report['driver_sha256']:
        raise ValueError('binary or driver changed during study')
    if digest(reference_binary) != report['reference_binary_sha256']:
        raise ValueError('reference binary changed during study')
    if reference and digest(args.reference_report) != report['reference']['sha256']:
        raise ValueError('reference changed during study')
    if args.richness and digest(Path(richness.__file__)) != report['richness_source_sha256']:
        raise ValueError('richness diagnostics changed during study')
    report['equivalent'] = set(args.models) == {'ordinary', 'ordinary-reference'}
    report['complete'] = True
    save()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--reference-binary', type=Path,
                        help='optional frozen engine for ordinary-reference comparisons')
    parser.add_argument('--hash-mode', choices=[WORLD_CANONICAL, EVIDENCE_CANONICAL],
                        default=WORLD_CANONICAL)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, default=Path('target/history/optimization'))
    parser.add_argument('--models', nargs='+', choices=MODELS, default=['ordinary', 'priority', 'continuation', 'evidence-1', 'owner-15', 'frames'])
    parser.add_argument('--cases', nargs='+', choices=CASES, default=['heldout-19', 'scale'])
    parser.add_argument('--rounds', type=int, default=2)
    parser.add_argument('--workers', type=int, default=1)
    parser.add_argument('--frame-workers', type=int, default=4)
    parser.add_argument('--span-days', type=int)
    parser.add_argument('--reference-report', type=Path)
    parser.add_argument('--memory-index', type=int, choices=[0, 1], default=0)
    parser.add_argument('--verify', action='store_true')
    parser.add_argument('--richness', action='store_true', help='record complete-state activity/learning diagnostics')
    args = parser.parse_args()
    if args.rounds < 1 or not 1 <= args.workers <= 64 or not 1 <= args.frame_workers <= 64 or (args.span_days is not None and args.span_days < 1):
        parser.error('positive rounds/span and 1..64 workers required')
    run(args)
