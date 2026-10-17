"""Accept an optimizer upgrade only with matching replays of every continuing checkpoint."""
import json
from pathlib import Path
from study import digest


def accept_upgrade(plan, state, root):
    old, new = state['binary_sha256'], plan['binary_sha256']
    if old == new:
        return False
    evidence = plan.get('runtime_upgrade', {})
    if evidence.get('from_sha256') != old:
        raise ValueError('cannot resume with another executable without replay evidence')
    verified = []
    for row in state['runs']:
        if row['state'] == 'running':
            raise ValueError('stop the supervisor at a verified checkpoint before upgrading')
        if not row.get('checkpoint') or row['state'] in ['extinct', 'target reached']:
            continue
        checkpoint = Path(row['checkpoint'])
        world = digest(checkpoint/'checkpoint.json')
        ledger = json.loads((checkpoint/'checkpoint-ledger.json').read_text())
        if ledger['sequence'] != row['cursor']:
            raise ValueError('checkpoint does not match the verified journal cursor')
        report_path = evidence.get('comparisons', {}).get(row['name'])
        if not report_path:
            raise ValueError('each continuing population needs replay evidence')
        report_path = root/report_path
        report = json.loads(report_path.read_text())
        if (report.get('equivalent') is not True or report.get('pairs', 0) < 3
                or report.get('checkpoint_sha256') != world or report.get('days', 0) <= row['days']
                or report['binaries']['control']['sha256'] != old
                or report['binaries']['candidate']['sha256'] != new):
            raise ValueError('replay evidence does not verify this optimizer and checkpoint')
        for pair in range(1, report['pairs']+1):
            runs = [r for r in report['runs'] if r['pair'] == pair]
            if (len(runs) != 2 or {r['build'] for r in runs} != {'control', 'candidate'}
                    or set(runs[0]['hashes']) != {'checkpoint.json', 'checkpoint-ledger.json', 'events.jsonl', 'people.json'}
                    or runs[0]['hashes'] != runs[1]['hashes']):
                raise ValueError('replay evidence contains an incomplete or different world or journal')
        # The comparator retains its last pair. Check the actual files, not only metadata.
        for run in [r for r in report['runs'] if r['pair'] == report['pairs']]:
            folder = report_path.parent/f"{report['pairs']:02d}-{run['build']}"
            if any(digest(folder/name) != expected for name, expected in run['hashes'].items()):
                raise ValueError('retained replay outputs differ from their evidence')
        verified.append({'name': row['name'], 'days': row['days'], 'cursor': row['cursor'],
                         'world_sha256': world, 'evidence': str(report_path.resolve()),
                         'evidence_sha256': digest(report_path), 'binaries': report['binaries']})
    if not verified:
        raise ValueError('no continuing checkpoint was verified')
    state.setdefault('runtime_upgrades', []).append({'from_sha256': old, 'to_sha256': new, 'checkpoints': verified})
    state['binary_sha256'] = new
    return True
