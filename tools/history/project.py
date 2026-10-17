#!/usr/bin/env python3
"""Keep human/world events in a compact companion to a full genetic journal.

Usage: project.py full-events.jsonl.gz compact-events.jsonl.gz metadata.json
The original remains untouched. Counts and SHA-256 hashes identify both artifacts.
"""
import collections
import gzip
import hashlib
import json
from pathlib import Path
import sys

def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda:stream.read(1024*1024),b''): h.update(chunk)
    return h.hexdigest()

if __name__=='__main__':
    source,output,metadata=map(Path,sys.argv[1:])
    excluded=collections.Counter();kept=collections.Counter()
    with gzip.open(source,'rt') as rows, gzip.open(output,'wt',compresslevel=6) as out:
        for line in rows:
            row=json.loads(line);kind=row['kind']
            if kind=='cultivars.created': excluded[kind]+=1
            else: out.write(line);kept[kind]+=1
    metadata.write_text(json.dumps({'source':str(source),'source_sha256':digest(source),'projection':str(output),'projection_sha256':digest(output),'excluded':dict(excluded),'kept':dict(kept),'scope':'Only raw plant family creation is omitted; all recorded human births/deaths, social, bodily, crop, ecological, and object events are unchanged.'},indent=2)+'\n')
    print(json.dumps({'kept':sum(kept.values()),'excluded':sum(excluded.values()),'bytes':output.stat().st_size}))
