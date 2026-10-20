#!/usr/bin/env python3
"""Run every Rust unit/integration harness and doctest with bounded concurrency."""
import argparse
import concurrent.futures
import datetime
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
RESULT = re.compile(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;')


def inventory(messages):
    """Cargo is the authority for executable paths and package working directories."""
    harnesses = {}
    for line in messages.splitlines():
        message = json.loads(line)
        if message.get('reason') != 'compiler-artifact' or not message.get('profile', {}).get('test'):
            continue
        executable = message.get('executable')
        if executable and set(message['target']['kind']) & {'lib', 'bin', 'test'}:
            harnesses[executable] = {
                'executable': executable, 'package': message['package_id'],
                'name': message['target']['name'],
                'cwd': str(Path(message['manifest_path']).parent),
            }
    return list(harnesses.values())


def totals(output, expected):
    matches = RESULT.findall(output)
    if len(matches) != 1:
        raise ValueError('missing or ambiguous libtest result')
    passed, failed, ignored, measured, filtered = map(int, matches[0])
    if filtered or measured or passed + failed + ignored != expected:
        raise ValueError('test inventory differs from executed inventory')
    return {'passed': passed, 'failed': failed, 'ignored': ignored}


def execute(harness, directory, threads):
    executable = Path(harness['executable'])
    log = directory / (executable.name + '.log')
    started = time.perf_counter()
    with log.open('w') as stream:
        result = subprocess.run([str(executable), f'--test-threads={threads}'],
                                cwd=harness['cwd'], stdout=stream, stderr=subprocess.STDOUT)
    row = dict(harness, seconds=time.perf_counter() - started, log=str(log), exit_code=result.returncode)
    try:
        row.update(totals(log.read_text(), harness['tests']))
    except ValueError as error:
        row['error'] = str(error)
    row['ok'] = result.returncode == 0 and 'error' not in row and row.get('failed') == 0
    return row


def positive(value):
    value = int(value)
    if value < 1:
        raise argparse.ArgumentTypeError('must be positive')
    return value


def run(args):
    started = time.perf_counter()
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    scope = [arg for package in args.package for arg in ['-p', package]] if args.package else ['--workspace']
    common = ['cargo', 'test', *scope, '--locked', '--jobs', str(args.build_jobs)]
    with (directory / 'build.log').open('w') as errors:
        build = subprocess.run([*common, '--lib', '--tests', '--no-run', '--message-format=json'],
                               cwd=ROOT, stdout=subprocess.PIPE, stderr=errors, text=True)
    (directory / 'artifacts.jsonl').write_text(build.stdout)
    if build.returncode:
        raise RuntimeError(f'build failed: {directory / "build.log"}')
    report = {'build_seconds': time.perf_counter() - started, 'build_jobs': args.build_jobs, 'jobs': args.jobs,
              'threads_per_harness': args.test_threads, 'harnesses': [], 'empty_harnesses': 0}
    harnesses = inventory(build.stdout)
    for harness in harnesses:
        listed = subprocess.run([harness['executable'], '--list', '--format=terse'],
                                cwd=harness['cwd'], capture_output=True, text=True, check=True)
        harness['tests'] = sum(line.endswith(': test') for line in listed.stdout.splitlines())
        if not harness['tests']:
            report['empty_harnesses'] += 1
    harnesses = [h for h in harnesses if h['tests']]
    report['listed_tests'] = sum(h['tests'] for h in harnesses)
    # Reuse timings only to choose order; no cache can exclude a harness or test.
    previous = ROOT / 'target/test-runs/durations.json'
    durations = json.loads(previous.read_text()) if previous.exists() else {}
    harnesses.sort(key=lambda h: durations.get(h['package'] + ':' + h['name'], h['tests']), reverse=True)
    testing = time.perf_counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = [pool.submit(execute, h, directory, args.test_threads) for h in harnesses]
        for future in concurrent.futures.as_completed(futures):
            row = future.result()
            report['harnesses'].append(row)
            print(f'{"PASS" if row["ok"] else "FAIL"} {row["name"]}: {row["tests"]} tests, {row["seconds"]:.2f}s', flush=True)
            (directory / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    report['test_seconds'] = time.perf_counter() - testing
    docs = time.perf_counter()
    with (directory / 'docs.log').open('w') as stream:
        result = subprocess.run([*common, '--doc', '--', f'--test-threads={args.test_threads}'],
                                cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT)
    report['docs_seconds'] = time.perf_counter() - docs
    report['docs_exit_code'] = result.returncode
    report['totals'] = {k: sum(r.get(k, 0) for r in report['harnesses']) for k in ['passed', 'failed', 'ignored']}
    report['ok'] = result.returncode == 0 and all(r['ok'] for r in report['harnesses'])
    report['total_seconds'] = time.perf_counter() - started
    (directory / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    previous.parent.mkdir(parents=True, exist_ok=True)
    durations.update({r['package'] + ':' + r['name']: r['seconds'] for r in report['harnesses']})
    temporary = previous.with_name(f'durations-{os.getpid()}.tmp')
    temporary.write_text(json.dumps(durations, indent=2) + '\n')
    temporary.replace(previous)
    print(json.dumps({k: report[k] for k in ['ok', 'listed_tests', 'totals', 'test_seconds', 'total_seconds']}, indent=2))
    return report['ok']


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('-p', '--package', action='append', default=[])
    parser.add_argument('--build-jobs', type=positive, default=1)
    parser.add_argument('--jobs', type=positive, default=1)
    parser.add_argument('--test-threads', type=positive, default=1)
    parser.add_argument('--output', type=Path, default=ROOT / 'target/test-runs' /
                        (datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%d-%H%M%S') + f'-{os.getpid()}'))
    try:
        raise SystemExit(0 if run(parser.parse_args()) else 1)
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f'{error}\n')
