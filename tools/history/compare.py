#!/usr/bin/env python3
"""Compare serial checkpoint replays, checking complete worlds and compact journals.

All output belongs under target/history (or another scratch directory).
"""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
import os
import time
import signal
from study import digest, write_json
from state_hashes import artifact_digest, WORLD_CANONICAL


def cpu_temperature(root=Path('/sys/class/thermal')):
    values = []
    for zone in root.glob('thermal_zone*'):
        try:
            if (zone/'type').read_text().strip() in ['x86_pkg_temp', 'cpu-thermal']:
                values.append(int((zone/'temp').read_text()) / 1000)
        except OSError:
            continue
    if not values:
        raise RuntimeError('CPU temperature sensor unavailable')
    return max(values)


def cool_before_run(limit):
    if limit is None:
        return None, 0
    started = time.monotonic()
    while (temperature := cpu_temperature()) > limit:
        if time.monotonic()-started > 600:
            raise RuntimeError('CPU did not cool within ten minutes; no timing run started')
        time.sleep(2)
    return temperature, time.monotonic()-started


def run_measured(command, environment, log, maximum_temperature):
    if maximum_temperature is None:
        subprocess.run(command, env=environment, stdout=log, stderr=log, check=True)
        return None
    peak = cpu_temperature()
    if peak > maximum_temperature:
        raise RuntimeError(f'CPU reached {peak} C; no replay started and timing discarded')
    process = subprocess.Popen(command, env=environment, stdout=log, stderr=log, start_new_session=True)
    completed = False
    try:
        while process.poll() is None:
            peak = max(peak, cpu_temperature())
            if peak > maximum_temperature:
                raise RuntimeError(f'CPU reached {peak} C; replay aborted and timing discarded')
            try:
                process.wait(timeout=0.05)
            except subprocess.TimeoutExpired:
                pass
        if process.returncode:
            raise subprocess.CalledProcessError(process.returncode, command)
        peak = max(peak, cpu_temperature())
        if peak > maximum_temperature:
            raise RuntimeError(f'CPU reached {peak} C; replay completed but timing discarded')
        completed = True
        return peak
    finally:
        if not completed:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                pass
            # The timing wrapper may exit before its descendants; terminate any remainder.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()


def compare(args):
    behavioral = getattr(args, 'behavioral', False)
    destination = args.output.resolve()
    destination.mkdir(parents=True, exist_ok=False)
    checkpoint = args.checkpoint.resolve()
    binaries = {key: getattr(args, key).resolve() for key in ['control', 'candidate']}
    start_limit = getattr(args, 'start_temperature', None)
    maximum_temperature = getattr(args, 'maximum_temperature', None)
    report = {'temperature_limits_celsius': {'start': start_limit, 'maximum': maximum_temperature}, 'mode': 'behavioral' if behavioral else 'equivalent',
              'hash_mode': WORLD_CANONICAL, 'checkpoint': str(checkpoint), 'checkpoint_sha256': digest(checkpoint/'checkpoint.json'),
              'days': args.days, 'pairs': args.pairs, 'workers': args.workers,
              'binaries': {key: {'path': str(path), 'sha256': digest(path)} for key, path in binaries.items()},
              'runs': []}
    write_json(destination/'comparison.json', report)
    environment = dict(os.environ)
    environment.pop('LEGACY_PROFILE', None)
    expected = {}
    cross_build_equal = True
    for pair in range(args.pairs):
        order = ['control', 'candidate'] if pair % 2 == 0 else ['candidate', 'control']
        hashes = {}
        folders = []
        for key in order:
            folder = destination/f'{pair+1:02d}-{key}'
            folder.mkdir(); folders.append(folder)
            command = ['/usr/bin/time', '-f', '{"wall_seconds":%e,"user_seconds":%U,"system_seconds":%S,"rss_kib":%M}',
                       '-o', str(folder/'time.json'), str(binaries[key]), '--resume', str(checkpoint),
                       '--days', str(args.days), '--sample-days', '365000', '--checkpoint-days', '365000',
                       '--workers', str(args.workers), '--final-save', 'false', '--journal', 'notable', '--output', str(folder)]
            starting_temperature, cooling_seconds = cool_before_run(start_limit)
            started = time.perf_counter()
            with (folder/'process.log').open('w') as log:
                peak_temperature = run_measured(command, environment, log, maximum_temperature)
            measured = json.loads((folder/'time.json').read_text())
            measured['wall_seconds'] = time.perf_counter()-started
            measured['cpu_seconds'] = measured['user_seconds']+measured['system_seconds']
            hashes[key] = {name: artifact_digest(folder/name, report['hash_mode']) for name in ['checkpoint.json', 'checkpoint-ledger.json', 'events.jsonl', 'people.json']}
            if key in expected and expected[key] != hashes[key]:
                report['equivalent'] = False; write_json(destination/'comparison.json', report)
                raise ValueError('nondeterministic replay; outputs retained')
            expected[key] = hashes[key]
            row = {'starting_temperature_celsius': starting_temperature, 'peak_temperature_celsius': peak_temperature, 'cooling_seconds_excluded': cooling_seconds, 'pair': pair+1, 'build': key, 'metrics': measured, 'hashes': hashes[key]}
            report['runs'].append(row)
            write_json(destination/'comparison.json', report)
            print(f'{pair+1} {key}: {measured["wall_seconds"]:.3f}s, CPU {measured["cpu_seconds"]:.3f}s', flush=True)
        cross_build_equal &= hashes['control'] == hashes['candidate']
        if not behavioral and not cross_build_equal:
            report['equivalent'] = False; write_json(destination/'comparison.json', report)
            raise ValueError('world, ledger, journal, or individual snapshot changed; outputs retained')
        # Preserve the last pair for inspection, rather than retaining every duplicate world.
        if pair+1 < args.pairs:
            for folder in folders:
                for name in ['checkpoint.json', 'checkpoint-ledger.json', 'events.jsonl', 'people.json', 'initial.json', 'daily.jsonl']:
                    (folder/name).unlink(missing_ok=True)
    report['equivalent'] = not behavioral
    report['cross_build_equal'] = cross_build_equal
    report['median'] = {key: {metric: statistics.median(r['metrics'][metric] for r in report['runs'] if r['build']==key)
                                    for metric in ['wall_seconds', 'cpu_seconds', 'rss_kib']} for key in binaries}
    report['wall_reduction_percent'] = 100*(1-report['median']['candidate']['wall_seconds']/report['median']['control']['wall_seconds'])
    report['rss_reduction_percent'] = 100*(1-report['median']['candidate']['rss_kib']/report['median']['control']['rss_kib'])
    report['cpu_reduction_percent'] = 100*(1-report['median']['candidate']['cpu_seconds']/report['median']['control']['cpu_seconds'])
    write_json(destination/'comparison.json', report)
    print(json.dumps({key: report[key] for key in ['equivalent','median','wall_reduction_percent','cpu_reduction_percent','rss_reduction_percent']}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ['checkpoint', 'control', 'candidate', 'output']:
        parser.add_argument('--'+key, type=Path, required=True)
    parser.add_argument('--days', type=int, required=True, help='absolute simulation end day')
    parser.add_argument('--pairs', type=int, default=3)
    parser.add_argument('--workers', type=int, default=1)
    parser.add_argument('--start-temperature', type=float, help='wait for a CPU package temperature at or below this Celsius value before timing')
    parser.add_argument('--maximum-temperature', type=float, help='abort a replay and discard its timing above this Celsius value')
    parser.add_argument('--behavioral', action='store_true',
                        help='measure intentionally different models; never authorizes an optimizer upgrade')
    arguments = parser.parse_args()
    if arguments.pairs < 1 or arguments.workers < 1: parser.error('pairs and workers must be positive')
    if arguments.start_temperature is not None and arguments.maximum_temperature is not None and arguments.start_temperature >= arguments.maximum_temperature:
        parser.error('starting temperature must be below maximum temperature')
    compare(arguments)
