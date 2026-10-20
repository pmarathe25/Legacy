#!/usr/bin/env python3
"""Run a frozen history in bounded chunks, archive compact records, and prune old saves.

Usage: chunks.py plan.json
Uses the study.py plan schema plus chunk_years (default20). It retains only the
latest checkpoint per job after verifying each new chunk's genealogy and archive.
"""
import gzip
import json
from pathlib import Path
import shutil
import subprocess
import sys
import time
import analyze
import runtimes
from study import digest, memory_mb, publish, write_json


def compress_checked(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary=destination.with_suffix(destination.suffix+'.tmp')
    with source.open('rb') as incoming, temporary.open('wb') as raw:
        with gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, compresslevel=6) as out:
            shutil.copyfileobj(incoming,out)
    import hashlib
    verified=hashlib.sha256()
    with gzip.open(temporary,'rb') as stream:
        for block in iter(lambda:stream.read(1024*1024),b''): verified.update(block)
    if verified.hexdigest()!=digest(source): raise ValueError('compressed archive differs from original')
    temporary.replace(destination)
    return digest(destination)


def archive_chunk(output, archive, index, previous, segments, reproductive_window=None):
    summary=json.loads((output/'summary.json').read_text())
    ledger=json.loads((output/'checkpoint-ledger.json').read_text())
    journal=archive/'chunks'/f'{index:04d}-events.jsonl.gz'
    hashes={'journal':compress_checked(output/'events.jsonl',journal)}
    hashes['annual']=compress_checked(output/'daily.jsonl',archive/'chunks'/f'{index:04d}-annual.jsonl.gz')
    if index==1: compress_checked(output/'initial.json',archive/'initial.json.gz')
    compress_checked(output/'people.json',archive/'people.json.gz')
    shutil.copy2(output/'summary.json',archive/'summary.json')
    segments.append({'journal':str(journal.resolve()),'after':previous,'through':ledger['sequence']})
    manifest={'final_directory':'.','segments':segments}
    if reproductive_window is not None: manifest['reproductive_window']=reproductive_window
    write_json(archive/'segments.json',manifest)
    analyze.analyze(archive/'segments.json',archive)
    write_json(archive/'chunks'/f'{index:04d}-summary.json',{'summary':summary,'hashes':hashes,'through':ledger['sequence']})
    return summary, ledger['sequence']


def prune(output, checkpoint=False):
    for name in ['events.jsonl','daily.jsonl','people.json','initial.json','final-save.json','checkpoint-world.tmp','checkpoint-ledger.tmp']:
        (output/name).unlink(missing_ok=True)
    if checkpoint:
        for name in ['checkpoint.json','checkpoint-ledger.json']:(output/name).unlink(missing_ok=True)


def run(plan_path):
    plan=json.loads(Path(plan_path).read_text());root=Path(plan['root']).resolve()
    executable=root/plan['executable'];artifacts=root/plan['artifacts'];artifacts.mkdir(parents=True,exist_ok=True)
    if digest(executable)!=plan['binary_sha256']: raise ValueError('frozen executable differs from plan')
    days=int(plan.get('days',365000));target=int(plan.get('generations',20));chunk=int(plan.get('chunk_days',int(plan.get('chunk_years',20))*365))
    if chunk<=0: raise ValueError('chunk_years must be positive')
    first=int(plan.get('initial_chunk_days',chunk))
    if first<=0 or first>chunk: raise ValueError('initial_chunk_days must be within the normal chunk size')
    limit=plan.get('chunks_per_invocation');completed_chunks=0
    if limit is not None and int(limit)<=0: raise ValueError('chunks_per_invocation must be positive')
    state_path=artifacts/'study.json'
    state=json.loads(state_path.read_text()) if state_path.exists() else {'binary_sha256':plan['binary_sha256'],'generations':target,'chunk_days':chunk,'verified_target':False,'runs':[]}
    state.pop('finished',None)
    if state['generations']!=target or state['chunk_days']!=chunk: raise ValueError('cannot change the target or chunk size of an existing study')
    if state['binary_sha256']!=plan['binary_sha256']:
        previous=json.loads(json.dumps(state))
        if runtimes.accept_upgrade(plan,state,root):
            write_json(artifacts/f"runtime-upgrade-{len(state['runtime_upgrades']):04d}-before.json",previous)
            publish(artifacts,state)
    for job in plan['jobs']:
        if state['verified_target'] and plan.get('stop_after_target',True): break
        row=next((r for r in state['runs'] if r['name']==job['name']),None)
        if row is None:
            row={'name':job['name'],'state':'pending','days':0,'depth':0,'births':0,'mortals':0,'chunks':0,'cursor':0,'segments':[]};state['runs'].append(row)
        if row['state'] in ['extinct','target reached'] or row['days']>=days: continue
        output_root=root/job['output'];archive=artifacts/job['name'];archive.mkdir(exist_ok=True)
        for old in range(1,row['chunks']+1):prune(output_root/f'chunk-{old:04d}',checkpoint=old<row['chunks'])
        while row['days']<days:
            if (artifacts/'stop-request').exists(): row['state']='stopped between chunks';publish(artifacts,state);return
            index=row['chunks']+1;end=min(days,row['days']+(first if row['chunks']==0 else chunk));output=output_root/f'chunk-{index:04d}'
            owner={'job':job['name'],'seed':job['seed'],'binary_sha256':plan['binary_sha256']}
            if output.exists() and (not (output/'owner.json').exists() or json.loads((output/'owner.json').read_text())!=owner):
                raise ValueError(f'refusing to reuse an unrelated output: {output}')
            completed_process=(output/'summary.json').exists() and (output/'checkpoint-ledger.json').exists()
            if output.exists() and not completed_process:
                # An interrupted chunk can be replayed from the last verified checkpoint.
                attempts=row.get('failed_attempts',0)+1;row['failed_attempts']=attempts
                if (output/'process.log').exists():(output/'process.log').rename(output/f'failed-attempt-{attempts}.log')
                prune(output,checkpoint=True)
                (output/'summary.json').unlink(missing_ok=True)
            if shutil.disk_usage(output_root.parent if output_root.parent.exists() else root).free<2*1024**3: raise ValueError('less than 2GiB free before starting a chunk')
            output.mkdir(parents=True,exist_ok=True);write_json(output/'owner.json',owner);stop=output/'stop-request';stop.unlink(missing_ok=True)
            args=[str(executable),'--days',str(end),'--generations',str(target),'--sample-days','365','--checkpoint-days',str(chunk),'--journal','notable','--final-save','false','--stop-file',str(stop),'--output',str(output)]
            if plan.get('content'): args+=['--content',str(root/plan['content'])]
            if plan.get('continue_after_extinction',False): args+=['--continue-after-extinction','true']
            if row.get('checkpoint'): args+=['--resume',row['checkpoint']]
            else:
                args+=['--seed',str(job['seed'])]
                for overlay in job.get('mods',[]): args+=['--mod',str(root/overlay)]
            row['state']='running';row['active_chunk']=index;publish(artifacts,state)
            print(f"{job['name']}: chunk{index}, through year{end/365:g}",flush=True)
            if not completed_process:
                with (output/'process.log').open('w') as log:
                    process=subprocess.Popen(args,cwd=root,stdout=log,stderr=log)
                    while process.poll() is None:
                        row['rss_mb']=round(memory_mb(process.pid),1)
                        if row['rss_mb']>plan.get('memory_mb',2400) or shutil.disk_usage(output).free<2*1024**3 or (artifacts/'stop-request').exists():
                            stop.write_text('finish at a committed day\n')
                        time.sleep(2)
                if process.returncode:
                    row['state']='execution failed; not a biological outcome';row['exit_code']=process.returncode;publish(artifacts,state);return
            previous_checkpoint=row.get('checkpoint')
            try:
                summary,cursor=archive_chunk(output,archive,index,row['cursor'],list(row['segments']),plan.get('reproductive_window'))
                row['segments']=json.loads((archive/'segments.json').read_text())['segments']
            except Exception as error:
                row['state']='archive verification failed; checkpoint retained';row['error']=str(error);publish(artifacts,state)
                raise
            final=summary['final'];row.update(extinct=summary['extinct'],days=summary['days'],depth=final['generations'],births=final['births'],mortals=final['mortals'],chunks=index,cursor=cursor,checkpoint=str(output.resolve()),verified_depth=final['generations'])
            row['state']='extinct' if summary['extinct'] and not plan.get('continue_after_extinction',False) else 'target reached' if summary['reached_generations'] else 'censored' if summary['stopped'] else 'horizon reached' if row['days']>=days else 'chunk verified'
            state['verified_target']|=row['depth']>=target
            publish(artifacts,state)
            prune(output)
            if previous_checkpoint:
                # The new checkpoint has completed a run loaded from its predecessor.
                for name in ['checkpoint.json','checkpoint-ledger.json','final-save.json']:(Path(previous_checkpoint)/name).unlink(missing_ok=True)
            print(f"verified chunk{index}: year{row['days']/365:.2f}, depth{row['depth']}, mortals{row['mortals']}",flush=True)
            completed_chunks+=1
            if limit is not None and completed_chunks>=int(limit): return
            if row['state'] in ['extinct','target reached','censored','horizon reached']: break
        if state['verified_target'] and plan.get('stop_after_target',True): break
    state['finished']=True;publish(artifacts,state)

def locked_run(plan_path):
    import fcntl
    plan=json.loads(Path(plan_path).read_text())
    artifacts=Path(plan['root'])/plan['artifacts'];artifacts.mkdir(parents=True,exist_ok=True)
    with (artifacts/'supervisor.lock').open('a') as lease:
        try: fcntl.flock(lease,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError: raise ValueError('this study already has a running supervisor') from None
        run(plan_path)

if __name__=='__main__':locked_run(sys.argv[1])
