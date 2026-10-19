"""Fork a stopped history into an explicitly different model, preserving its people and prefix."""
import argparse
import copy
import fcntl
import hashlib
import json
from pathlib import Path
import shutil

from study import digest, write_json


def fork(source, destination, executable, reason, validation, days=None):
    plan = json.loads(Path(source).read_text())
    archive = Path(plan['root'])/plan['artifacts']
    with (archive/'supervisor.lock').open('a') as lease:
        try:
            fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError('source supervisor is still running') from None
        return _fork(source, destination, executable, reason, validation, days)


def _fork(source, destination, executable, reason, validation, days):
    source, destination = Path(source).resolve(), Path(destination).resolve()
    executable, validation = Path(executable).resolve(), Path(validation).resolve()
    source_bytes = source.read_bytes()
    plan = json.loads(source_bytes)
    root = Path(plan['root'])
    archive = root/plan['artifacts']
    state_bytes = (archive/'study.json').read_bytes()
    validation_bytes = validation.read_bytes()
    state = json.loads(state_bytes)
    if not reason.strip() or json.loads(validation_bytes).get('exit_code') != 0:
        raise ValueError('a model fork needs a reason and successful validation evidence')
    if not (archive/'stop-request').exists() or any(r['state'] == 'running' for r in state['runs']):
        raise ValueError('stop the source study at a verified checkpoint before forking')
    old, new = state['binary_sha256'], digest(executable)
    if old != plan['binary_sha256'] or digest(root/plan['executable']) != old or old == new:
        raise ValueError('model fork binaries must match the source and describe a new model')
    if destination.exists():
        raise ValueError('fork destination already exists')
    source_trees = [archive.resolve()] + [(root/job['output']).resolve() for job in plan['jobs']]
    if any(destination.is_relative_to(tree) for tree in source_trees):
        raise ValueError('fork destination must be outside source archives and runs')
    if days is not None and days <= max(r['days'] for r in state['runs']):
        raise ValueError('fork horizon must extend beyond its checkpoints')
    checkpoints = []
    for row in state['runs']:
        checkpoint = Path(row['checkpoint'])
        ledger = json.loads((checkpoint/'checkpoint-ledger.json').read_text())
        world = json.loads((checkpoint/'checkpoint.json').read_text())
        if ledger['sequence'] != row['cursor'] or world['notices'][-1]['sequence'] != row['cursor']:
            raise ValueError('source checkpoint does not match its verified journal cursor')
        checkpoints.append({'name': row['name'], 'days': row['days'], 'depth': row['depth'],
                            'cursor': row['cursor'], 'world_sha256': digest(checkpoint/'checkpoint.json')})
    destination.mkdir(parents=True)
    new_archive = destination/'archive'
    new_archive.mkdir()
    updated = copy.deepcopy(state)
    updated.pop('finished', None)
    updated['binary_sha256'] = new
    updated.setdefault('model_forks', []).append({
        'source_plan': str(source), 'source_state_sha256': hashlib.sha256(state_bytes).hexdigest(),
        'from_sha256': old, 'to_sha256': new, 'reason': reason,
        'validation_sha256': hashlib.sha256(validation_bytes).hexdigest(), 'checkpoints': checkpoints,
        'interpretation': 'Prefix uses the earlier model; subsequent history uses the new model. '
                          'This is an intentional model change, not an equivalent optimizer upgrade.',
    })
    for row in updated['runs']:
        name = row['name']
        shutil.copytree(archive/name, new_archive/name)
        for segment in row['segments']:
            relative = Path(segment['journal']).relative_to(archive)
            segment['journal'] = str(new_archive/relative)
        manifest = new_archive/name/'segments.json'
        data = json.loads(manifest.read_text())
        data['segments'] = row['segments']
        write_json(manifest, data)
        original = Path(row['checkpoint'])
        # Keep an immutable origin when the ordinary supervisor prunes its bootstrap input.
        origin, bootstrap = destination/'origins'/name, destination/'bootstrap'/name
        for folder in [origin, bootstrap]:
            folder.mkdir(parents=True)
            for filename in ['checkpoint.json', 'checkpoint-ledger.json']:
                shutil.copy2(original/filename, folder/filename)
        if digest(origin/'checkpoint.json') != next(c['world_sha256'] for c in checkpoints if c['name'] == name):
            raise ValueError('source checkpoint changed while copying the fork')
        row['checkpoint'], row['state'] = str(bootstrap), 'forked verified checkpoint'
    result = copy.deepcopy(plan)
    result.update(executable=str(executable), binary_sha256=new, artifacts=str(new_archive))
    result.pop('runtime_upgrade', None)
    if days is not None:
        result['days'] = days
    result['model_note'] = reason
    for job in result['jobs']:
        job['output'] = str(destination/'runs'/job['name'])
    (destination/'validation.json').write_bytes(validation_bytes)
    (destination/'source-plan.json').write_bytes(source_bytes)
    (destination/'source-study.json').write_bytes(state_bytes)
    write_json(new_archive/'study.json', updated)
    write_json(destination/'plan.json', result)
    return destination/'plan.json'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('destination', type=Path)
    parser.add_argument('--executable', type=Path, required=True)
    parser.add_argument('--reason', required=True)
    parser.add_argument('--validation', type=Path, required=True)
    parser.add_argument('--days', type=int)
    args = parser.parse_args()
    print(fork(args.source, args.destination, args.executable, args.reason, args.validation, args.days))
