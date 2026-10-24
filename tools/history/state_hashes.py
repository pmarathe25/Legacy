"""Hash complete replay state, allowing JSON object field order to change."""
import hashlib
import json
from study import digest

WORLD_CANONICAL = 'canonical-world-v1'
EVIDENCE_CANONICAL = 'canonical-evidence-v2'


def canonical_evidence(world):
    """Normalize dictionary layout, retaining every unique value and evidence stamp.

    Runtime payload variants can serialize identically yet intern separately.
    Only archive-local dictionary indices are remapped; game IDs, row order,
    pending work, trace metadata and every other world field remain exact.
    """
    memory = world.get('resources', {}).get('knowledge.facts', {})
    if 'archive' not in memory:
        return
    archive = memory['archive']
    if archive['version'] != 1:
        raise ValueError('unsupported evidence archive version')

    def dictionary(values):
        signatures = [json.dumps(v, sort_keys=True, separators=(',', ':'),
                                 ensure_ascii=False) for v in values]
        unique = sorted(set(signatures))
        ids = {value: i for i, value in enumerate(unique)}
        return [json.loads(value) for value in unique], [ids[value] for value in signatures]

    keys, key_ids = dictionary(archive['keys'])
    values, value_ids = dictionary(archive['values'])
    for rows in archive['people'].values():
        for row in rows:
            if len(row) != 6 or any(type(row[i]) is not int or not 0 <= row[i] < len(ids)
                                    for i, ids in enumerate((key_ids, value_ids))):
                raise ValueError('invalid archived evidence row')
            row[0], row[1] = key_ids[row[0]], value_ids[row[1]]
    archive['keys'], archive['values'] = keys, values


def artifact_digest(path, mode=None):
    if mode not in (None, WORLD_CANONICAL, EVIDENCE_CANONICAL):
        raise ValueError('unsupported replay hash mode')
    if mode is None or path.name != 'checkpoint.json':
        return digest(path)
    with path.open() as source:
        world = json.load(source)
    if mode == EVIDENCE_CANONICAL:
        canonical_evidence(world)
    result = hashlib.sha256()
    encoder = json.JSONEncoder(sort_keys=True, separators=(',', ':'), ensure_ascii=False)
    for part in encoder.iterencode(world):
        result.update(part.encode('utf-8'))
    return result.hexdigest()
