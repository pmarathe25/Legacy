#!/usr/bin/env python3
"""Describe a completed world's events without treating correlations as causes.

Usage: patterns.py run-directory journal.jsonl.gz output.json
"""
import collections
import gzip
import json
from pathlib import Path
import statistics
import journals
import sys

YEAR=365*86400000

def summarize(run,journal):
    summary=json.loads((run/'summary.json').read_text())
    def document(name):
        path=run/name
        if path.exists(): return json.loads(path.read_text())
        with gzip.open(path.with_suffix(path.suffix+'.gz'),'rt') as source:return json.load(source)
    people={p['id']:p for p in document('people.json')}
    initial=document('initial.json')
    deaths={};paired=set();reexchange=collections.Counter();goods=collections.Counter()
    received=collections.defaultdict(lambda:collections.defaultdict(set))
    damaged_ingestions=[];marked={};losses=[];counts=collections.Counter()
    source=journals.rows(journal) if journal.is_dir() else (json.loads(line) for line in gzip.open(journal,'rt'))
    for row in source:
        data=row['data'];kind=row['kind'];counts[kind]+=1
        if kind=='physiology.died':
            deaths[data['person']]=row
            if row.get('ingestion_reported_after_death_notice') and data.get('cause')=='Damage':
                damaged_ingestions.append({'person':data['person'],'day':row['day'],'good':row['last_feed']['good']})
        elif kind=='households.formed':paired.update([data['first'],data['second']])
        elif kind=='physiology.fed':
            d=deaths.get(data['patient'])
            if d and d['day']==row['day'] and d['data'].get('cause')=='Damage':
                damaged_ingestions.append({'person':data['patient'],'day':row['day'],'good':data['good']})
        elif kind=='exchange.traded':
            a,b=data['first'],data['second'];give,get=data['offered'],data['requested']
            for actor,peer,good in [(a,b,give),(b,a,get)]:
                goods[good]+=1
                if received[actor][good]-{peer}:reexchange[good]+=1
            received[a][get].add(b);received[b][give].add(a)
        elif kind=='items.marked':marked[data['item']]=data
        elif kind=='sentiment.lost':losses.append(data)
    mortality=collections.Counter()
    for pid,row in deaths.items():
        b=row['body'];age=(row['data']['at']-b['born'])/YEAR
        mortality['deaths']+=1;mortality['before_18']+=age<18
        mortality['before_45']+=age<45
        mortality['at_life_limit']+=b.get('life_limit') is not None and row['data']['at']>=b['life_limit']
        mortality['high_hunger']+=b['hunger']>=120
    mortals=[p for p in people.values() if p['body']['kind']=='Mortal' and p.get('generation') is not None]
    attachments=[]
    for p in mortals:
        for m in (p.get('attachments') or {}).values():
            attachments.append({'person':p['id'],'name':p['name'],'target':m['target'],'value':m['value'],'reason':m['reason']})
    final=summary['final'];plants={p['patch']:p for p in final.get('wild_plants',[])}
    shifts=[]
    for p in initial.get('wild_plants',[]):
        end=plants.get(p['patch'])
        if end:shifts.append({'patch':p['patch'],'initial_cold':p['mean_cold'],'final_cold':end['mean_cold'],'living':end['living'],'generation':end['generation']})
    return {'days':summary['days'],'generation':final['generations'],'births':final['births'],'mortality':dict(mortality),'paired':len(paired),'mortals':len(mortals),'mortals_never_paired':len([p for p in mortals if p['id'] not in paired]),'recorded_damage_deaths_during_ingestion':list({(p['person'],p['day'],p['good']):p for p in damaged_ingestions}.values()),'recorded_trade_goods':dict(goods),'received_then_offered_to_another_partner':dict(reexchange),'strongest_recorded_attachments':sorted(attachments,key=lambda m:abs(m['value']),reverse=True)[:12],'witnessed_sentimental_losses':losses,'plant_shifts':shifts,'recorded_events':dict(counts),'interpretation':'The compact journal samples repeated events. Received-then-offered commodity patterns do not prove the same unit moved or that a monetary system formed. Mortality indicators are not exclusive causes. Generations, habitats, mating policies, exposure, and culture confound selection.'}

if __name__=='__main__':
    run,journal,output=map(Path,sys.argv[1:]);result=summarize(run,journal);output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:result[k] for k in ['days','generation','births','mortality','paired','mortals_never_paired','recorded_damage_deaths_during_ingestion','received_then_offered_to_another_partner']}))
