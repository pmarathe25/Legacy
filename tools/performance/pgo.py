#!/usr/bin/env python3
"""Build a history executable using a caller-supplied set of training runs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def source_digest(root):
    paths = [root/'Cargo.toml', root/'Cargo.lock']
    for directory in ['crates', 'content', 'tools/history']:
        paths.extend(p for p in (root/directory).rglob('*')
                     if p.is_file() and p.suffix in ['.rs', '.toml'])
    checksum = hashlib.sha256()
    for path in sorted(paths):
        checksum.update(str(path.relative_to(root)).encode()+b'\0')
        checksum.update(bytes.fromhex(digest(path)))
    return checksum.hexdigest()


def training_runs(plan, base):
    """Resolve paths relative to the plan; output directories belong to this tool."""
    runs = []
    for row in plan['runs']:
        arguments = list(row['arguments'])
        if len(arguments) % 2 or any(not isinstance(a, str) for a in arguments):
            raise ValueError('training arguments must be string option/value pairs')
        if '--output' in arguments or '--stop-file' in arguments:
            raise ValueError('training outputs and stop files are managed by the build tool')
        if '--days' not in arguments:
            raise ValueError('each training run needs an absolute --days horizon')
        for i in range(0, len(arguments), 2):
            if arguments[i] in ['--resume', '--content', '--mod']:
                arguments[i+1] = str((base/arguments[i+1]).resolve())
        runs.append(arguments)
    if not runs:
        raise ValueError('at least one training run is required')
    return runs


def input_digests(runs):
    paths = set()
    for arguments in runs:
        for option, value in zip(arguments[::2], arguments[1::2]):
            if option == '--resume':
                paths.update(Path(value)/name for name in ['checkpoint.json', 'checkpoint-ledger.json'])
            elif option in ['--content', '--mod']:
                paths.update(Path(value).rglob('*.toml'))
    return {str(path): digest(path) for path in sorted(paths)}


def build(root, plan_path, output):
    plan = json.loads(plan_path.read_text())
    runs = training_runs(plan, plan_path.parent)
    if os.environ.get('RUSTFLAGS') or os.environ.get('CARGO_ENCODED_RUSTFLAGS'):
        raise ValueError('start without RUSTFLAGS or CARGO_ENCODED_RUSTFLAGS')
    compiler = subprocess.check_output(['rustc', '-vV'], text=True)
    target = next(line.removeprefix('host: ') for line in compiler.splitlines()
                  if line.startswith('host: '))
    sysroot = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    profdata = sysroot/'lib/rustlib'/target/'bin/llvm-profdata'
    if not profdata.is_file():
        raise ValueError('install the matching tool with rustup component add llvm-tools-preview')
    original = source_digest(root)
    inputs = input_digests(runs)
    output.mkdir(parents=True, exist_ok=False)
    raw = output/'raw'
    raw.mkdir()
    report = {'compiler': compiler, 'target': target, 'source_sha256': original,
              'training': runs, 'training_inputs': inputs, 'training_plan_sha256': digest(plan_path)}
    (output/'metadata.json').write_text(json.dumps(report, indent=2)+'\n')

    def run(command, log, flags=None, profile=None):
        environment = dict(os.environ)
        environment.pop('LEGACY_PROFILE', None)
        if flags:
            environment['CARGO_ENCODED_RUSTFLAGS'] = flags
        if profile:
            environment['LLVM_PROFILE_FILE'] = str(profile)
        print('Running '+log, flush=True)
        with (output/log).open('w') as stream:
            subprocess.run(command, cwd=root, env=environment, stdout=stream,
                           stderr=stream, check=True)

    def compile_to(directory, flag, log):
        run(['cargo', 'build', '--locked', '--release', '--target', target,
             '--target-dir', str(directory), '-p', 'game-history'], log, flag)
        return directory/target/'release/game-history'

    instrumented = compile_to(output/'generate', '-Cprofile-generate='+str(raw), 'generate.log')
    for i, arguments in enumerate(runs):
        run([str(instrumented), *arguments, '--output', str(output/f'train-{i:02d}')],
            f'train-{i:02d}.log', profile=raw/'%m-%p.profraw')
    profiles = sorted(raw.glob('*.profraw'))
    if not profiles:
        raise ValueError('training produced no profiles')
    merged = output/'merged.profdata'
    run([str(profdata), 'merge', '-o', str(merged), *map(str, profiles)], 'merge.log')
    if source_digest(root) != original or input_digests(runs) != inputs:
        raise ValueError('source or training inputs changed during training; start a new build')
    executable = compile_to(output/'use', '-Cprofile-use='+str(merged), 'use.log')
    if source_digest(root) != original:
        raise ValueError('source changed during compilation; start a new build')
    shutil.copy2(executable, output/'game-history')
    report.update(binary_sha256=digest(output/'game-history'), profile_sha256=digest(merged))
    (output/'metadata.json').write_text(json.dumps(report, indent=2)+'\n')
    print(output/'game-history')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--training', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    build(Path(__file__).resolve().parents[2], args.training.resolve(), args.output.resolve())
