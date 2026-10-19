"""Hash complete replay state, allowing JSON object field order to change."""
import hashlib
import json
from study import digest

WORLD_CANONICAL = 'canonical-world-v1'


def artifact_digest(path, mode=None):
    if mode not in (None, WORLD_CANONICAL):
        raise ValueError('unsupported replay hash mode')
    if mode != WORLD_CANONICAL or path.name != 'checkpoint.json':
        return digest(path)
    with path.open() as source:
        world = json.load(source)
    result = hashlib.sha256()
    encoder = json.JSONEncoder(sort_keys=True, separators=(',', ':'), ensure_ascii=False)
    for part in encoder.iterencode(world):
        result.update(part.encode('utf-8'))
    return result.hexdigest()
