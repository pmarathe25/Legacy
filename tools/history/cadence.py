#!/usr/bin/env python3
"""Compare attention windows by changing saved rules, without adding startup wakes.

Different models may produce different histories. Complete state must replay
within each model. Run without concurrent builds, tests, profiles or simulations.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import statistics

from decisions import ARTIFACTS, behavior, digest, fork, measure, prune
from frames import CASES
import richness
from state_hashes import EVIDENCE_CANONICAL, artifact_digest


def study(args):
    binary, root = args.binary.resolve(), args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    policies = {'ordinary': {}}
    busy = getattr(args, 'busy_attention', False)
    policies.update({f'{"busy" if busy else "attention"}-{n}ms': {
        'attention': {'ordinary': n, 'evidence': n}, **({'busy_attention': True} if busy else {})}
        for n in args.intervals})
    environment = dict(os.environ)
    for key in ['LEGACY_PROFILE', 'LEGACY_REVIEW_TRACE']:
        environment.pop(key, None)
    environment.update(LEGACY_MEMORY_INDEX='0', LEGACY_APPRAISAL_CACHE='0')
    drivers = [Path(__file__), Path(richness.__file__), Path('tools/history/decisions.py'),
               Path('tools/history/state_hashes.py')]
    report = {'complete': False, 'equivalent': False, 'mode': 'behavioral',
              'hash_mode': EVIDENCE_CANONICAL, 'binary': str(binary), 'binary_sha256': digest(binary),
              'drivers': {str(p): digest(p) for p in drivers}, 'policies': policies,
              'rounds': args.rounds, 'cpu_quota': Path('/sys/fs/cgroup/cpu.max').read_text().strip(),
              'affinity': sorted(os.sched_getaffinity(0)), 'cases': {}}
    reference_path = getattr(args, 'legacy_reference', None)
    reference = json.loads(reference_path.read_text()) if reference_path else None
    if reference:
        if not reference['complete'] or reference['hash_mode'] != EVIDENCE_CANONICAL:
            raise ValueError('legacy reference must be a completed compatible study')
        report['legacy_reference'] = {'path': str(reference_path.resolve()),
                                     'sha256': digest(reference_path), 'scope': 'ordinary only'}

    def save():
        (root/'comparison.json').write_text(json.dumps(report, indent=2)+'\n')

    save()
    for case_name in args.cases:
        fixture, end, reload_day = CASES[case_name]
        if getattr(args, 'end_day', None) is not None:
            end = args.end_day
        source = (args.fixtures/fixture).resolve()
        original = json.loads((source/'checkpoint.json').read_text())
        rules = original['resources']['characters.minds']['rules']
        if rules.get('attention') or rules.get('routine') or rules.get('option_memory'):
            raise ValueError('fixture must have legacy ordinary attention')
        if any(m.get('review_clock') for m in original['resources']['characters.minds']['people'].values()):
            raise ValueError('fixture contains attention-clock history')
        initial = richness.read(source)
        ledger = json.loads((source/'checkpoint-ledger.json').read_text())
        prefix = ledger['counts']
        start = (original['time']-ledger['epoch'])/86400000
        if end <= start:
            raise ValueError('end day must follow the checkpoint')
        if getattr(args, 'end_day', None) is not None:
            reload_day = int(start)+(end-int(start))//2 if end-start > 1 else None
        case_root = root/case_name
        case_root.mkdir()
        inputs = {'ordinary': source}
        for name, patch in policies.items():
            if not patch:
                continue
            folder = case_root/('input-'+name)
            folder.mkdir()
            with (folder/'checkpoint.json').open('w') as stream:
                json.dump(fork(original, patch), stream, separators=(',', ':'))
            shutil.copyfile(source/'checkpoint-ledger.json', folder/'checkpoint-ledger.json')
            inputs[name] = folder
        del original
        case = report['cases'][case_name] = {'end_day': end, 'runs': [], 'verification': [],
                'inputs': {name: {a: digest(folder/a) for a in ARTIFACTS[:2]}
                           for name, folder in inputs.items()}}
        expected = {}
        if reference:
            old = reference['cases'][case_name]
            if old['end_day'] != end or old['inputs']['ordinary'] != case['inputs']['ordinary']:
                raise ValueError('legacy reference horizon or input differs')
            legacy = [row['hashes'] for row in old['runs'] if row['model'] == 'ordinary']
            if not legacy or any(row != legacy[0] for row in legacy):
                raise ValueError('inconsistent legacy reference')
            expected['ordinary'] = legacy[0]

        def replay(name, label, workers=1, reload_at=0, verification=False):
            folder = case_root/label
            folder.mkdir()
            command = [str(binary), '--resume', str(inputs[name]), '--days', str(end),
                       '--workers', str(workers), '--resident-executor', 'true',
                       '--parallel-listeners', 'false', '--prepare-window', '0',
                       '--final-save', 'false', '--continue-after-extinction', 'true',
                       '--journal', 'notable', '--reload-day', str(reload_at), '--output', str(folder)]
            with (folder/'run.log').open('w') as log:
                metrics = measure(command, environment, log)
            outcome = behavior(folder, prefix)
            if outcome['end_day'] != end:
                raise ValueError('fixed horizon not reached')
            hashes = {a: artifact_digest(folder/a, EVIDENCE_CANONICAL) for a in ARTIFACTS}
            if name in expected and hashes != expected[name]:
                raise ValueError(f'{case_name}/{label}: within-model replay mismatch; retained')
            expected[name] = hashes
            row = {'model': name, 'workers': workers, 'reload_day': reload_at, 'command': command,
                   'metrics': metrics, 'hashes': hashes, 'raw_hashes': {a: digest(folder/a) for a in ARTIFACTS},
                   'behavior': outcome}
            if not verification:
                row['richness'] = richness.compare(initial, folder)
            case['verification' if verification else 'runs'].append(row)
            save()
            if not getattr(args, 'retain_state', False):
                prune(folder)
            print(case_name, label, f'{metrics["wall_seconds"]:.3f}s', flush=True)

        for n in range(args.rounds):
            names = list(policies)
            names = names[n % len(names):]+names[:n % len(names)]
            for name in names:
                replay(name, f'{n+1:02d}-{name}')
        if args.verify:
            for name in policies:
                replay(name, 'verify-'+name, 4, reload_day or 0, True)
        case['median_seconds'] = {name: statistics.median(
            row['metrics']['wall_seconds'] for row in case['runs'] if row['model'] == name)
            for name in policies}
        for name, folder in inputs.items():
            if {a: digest(folder/a) for a in ARTIFACTS[:2]} != case['inputs'][name]:
                raise ValueError('input changed')
        case['within_model_replay_equal'] = True
        save()
    if digest(binary) != report['binary_sha256'] or any(digest(p) != report['drivers'][str(p)] for p in drivers):
        raise ValueError('binary or evaluation source changed')
    if reference_path and digest(reference_path) != report['legacy_reference']['sha256']:
        raise ValueError('legacy reference changed')
    report['complete'] = True
    save()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, default=Path('target/history/optimization'))
    parser.add_argument('--cases', nargs='+', choices=CASES, default=['heldout-19', 'heldout-37', 'scale'])
    parser.add_argument('--intervals', type=int, nargs='+', default=[1, 60000])
    parser.add_argument('--rounds', type=int, default=2)
    parser.add_argument('--verify', action='store_true')
    parser.add_argument('--end-day', type=int, help='override the absolute end day')
    parser.add_argument('--retain-state', action='store_true', help='retain complete outputs for qualitative inspection')
    parser.add_argument('--legacy-reference', type=Path, help='verify the unchanged ordinary model against a prior completed report')
    parser.add_argument('--busy-attention', action='store_true', help='idle choices and completed actions bypass the floor')
    args = parser.parse_args()
    if args.rounds < 1 or len(args.intervals) != len(set(args.intervals)) or any(not 0 < n <= 86400000 for n in args.intervals):
        parser.error('positive rounds and unique attention windows in (0, DAY] required')
    study(args)
