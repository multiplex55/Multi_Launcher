"""Strict final-source four-record Notes/Actions join against frozen Track C G0."""
import argparse
import json
from pathlib import Path
import re

KEY = ('owner','fixture','fixture_signature','item_count','viewport','scale_milli','fonts','settings','cold_type')
OWNERS = {'NotePanel::maybe_refresh_heavy_derived', 'process_watch_events_actions_reload'}
ARRAYS = ('elapsed_ns','complete_output_state_ids','structural_output_ids','revision_receipts','viewport_receipts','work_units','work_counters')

def require(ok, message):
    if not ok:
        raise ValueError(message)

def uint(value):
    return type(value) is int and 0 <= value <= 2**64-1

def tokens(value, count):
    require(type(value) is str and re.fullmatch(r'[0-9]+(?::[0-9]+)*',value,flags=re.ASCII), 'decimal receipt')
    values=value.split(':')
    require(len(values)==count and all(uint(int(v)) for v in values), 'bounded receipt tokens')

def strict_json(value):
    def pairs(items):
        result={}
        for k,v in items:
            require(k not in result,'duplicate JSON key')
            result[k]=v
        return result
    def invalid(value):
        raise ValueError('float/nonfinite input')
    return json.loads(value,object_pairs_hook=pairs,parse_constant=invalid,parse_float=invalid)

def validate(data, baseline, source):
    require(type(source) is str and re.fullmatch(r'[0-9a-f]{40}',source),'source')
    require(baseline['status']=='FROZEN_G0' and baseline['source']=='071243db068835617a054a74fb5734a6dd9f128d','immutable G0')
    require(type(data) is dict and set(data)=={'schema','source','profile','mode','status','native_display','records'},'collection fields')
    require(type(data['schema']) is int and data['schema']==1 and data['source']==source and data['profile']=='fast-dev' and data['mode']=='Full' and data['status']=='COLLECTED_PENDING_REVIEW','collection provenance')
    require(data['native_display']=='NOT MEASURED','native scope')
    expected={tuple(r[k] for k in KEY):r for r in baseline['records'] if r['owner'] in OWNERS}
    require(len(expected)==4 and type(data['records']) is list and len(data['records'])==4,'four record inventory')
    seen=set(); rows=[]
    for r in data['records']:
        require(type(r) is dict,'record object')
        key=tuple(r.get(k) for k in KEY)
        require(key in expected and key not in seen,'missing/extra/duplicate recipe');seen.add(key);old=expected[key]
        require(set(r)==set(old),'exact26 record fields')
        require(r['source']==source and r['profile']=='fast-dev' and r['mode']=='Full' and r['telemetry_enabled'] is True,'record provenance')
        for k in ('item_count','scale_milli','warmups','samples','exclusions','p50_ns','p95_ns','max_ns'):
            require(uint(r[k]),'typed uint '+k)
        require((r['warmups'],r['samples'],r['exclusions'])==(5,20,0),'sampling')
        for k in ARRAYS:
            require(type(r[k]) is list and len(r[k])==20,'array '+k)
        require(all(uint(v) for v in r['elapsed_ns']+r['work_units']),'timing/work uint')
        for k in ('complete_output_state_ids','structural_output_ids'):
            require(all(type(v) is str and re.fullmatch(r'[0-9a-f]{16}',v) for v in r[k]),'identity '+k)
        notes=r['owner']=='NotePanel::maybe_refresh_heavy_derived'
        for v in r['revision_receipts']:tokens(v,2 if notes else 5)
        for v in r['work_counters']:tokens(v,4 if notes else 6)
        for k in ('complete_output_state_ids','structural_output_ids','revision_receipts','viewport_receipts'):
            require(r[k]==old[k],'frozen identity/revision '+k)
        require(r['viewport_receipts']==['none']*20,'nonrendered viewport')
        if notes:
            n=r['item_count']+12
            require(old['work_units']==[3*n]*20 and old['work_counters']==[f'1:{3*n}:1:{n}']*20,'G0 triple scan meaning')
            require(r['work_units']==[n]*20 and r['work_counters']==[f'1:{n}:1:{n}']*20,'fused relationship visits')
        else:
            require(r['work_units']==old['work_units'] and r['work_counters']==old['work_counters'],'Action output cardinality receipts')
        ordered=sorted(r['elapsed_ns']);row={k:r[k] for k in KEY}
        for field,index in (('p50_ns',9),('p95_ns',18),('max_ns',19)):
            require(r[field]==ordered[index],'nearest rank '+field)
            require(uint(old[field]) and old[field]>0,'baseline denominator')
            row.update({'g0_'+field:old[field],'current_'+field:r[field],field+'_ratio':r[field]/old[field]})
        row.update(g0_work_units=old['work_units'],current_work_units=r['work_units'],work_semantics='fused_relationship_visits' if notes else 'Action_output_cardinalities_not_clone_receipts')
        rows.append(row)
    require(seen==set(expected),'complete keys')
    return dict(schema=1,source=source,profile='fast-dev',mode='Full',status='PARITY_PASS_INDEPENDENT_REVIEW_PENDING',records=data['records'],comparison=dict(g0_source=baseline['source'],rows=rows,scope='original_Full_Notes_Actions_CPU_owners',limits='No native/display/appwide claim; C1F projection copy metrics remain separate'))

def main():
    a=argparse.ArgumentParser();a.add_argument('--input',type=Path,required=True);a.add_argument('--source',required=True);a.add_argument('--baseline',type=Path,required=True);a.add_argument('--output',type=Path,required=True);args=a.parse_args()
    result=validate(strict_json(args.input.read_text(encoding='utf-8-sig')),json.loads(args.baseline.read_text(encoding='utf-8-sig')),args.source)
    with args.output.open('x',encoding='utf-8') as f:json.dump(result,f,indent=2);f.write('\n')

if __name__=='__main__':main()
