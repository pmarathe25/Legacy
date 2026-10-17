"""Read compact, demographic-only, or chunked archives with their sequence bounds."""
import json
from pathlib import Path
import analyze


def rows(folder):
    folder=Path(folder)
    for name in ['events.jsonl.gz','notable-events.jsonl.gz','demographics.jsonl.gz']:
        if (folder/name).exists():
            segments=[{'journal':name}];break
    else:
        segments=json.loads((folder/'segments.json').read_text())['segments']
    for segment in segments:
        with analyze.event_stream(segment,folder) as stream:
            for raw in stream:
                row=json.loads(raw);seq=row['sequence']
                if seq>segment.get('through',float('inf')): break
                if seq>segment.get('after',0):yield row
