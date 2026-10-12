"""Inventory-only guard for all105 original final-source Track C records.

Family comparators remain responsible for output/work/receipt semantics.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import re

KEY=('owner','fixture','fixture_signature','item_count','viewport','scale_milli','fonts','settings','cold_type')
ARRAYS=('elapsed_ns','complete_output_state_ids','structural_output_ids','revision_receipts','viewport_receipts','work_units','work_counters')

def require(ok,message):
    if not ok:raise ValueError(message)

def uint(value):return type(value) is int and 0<=value<=2**64-1

def strict_json(value):
    def pairs(items):
        result={}
        for k,v in items:
            require(k not in result,'duplicate JSON key');result[k]=v
        return result
    def invalid(value):raise ValueError('float/nonfinite input')
    return json.loads(value,object_pairs_hook=pairs,parse_float=invalid,parse_constant=invalid)

def validate(data,baseline,source):
    require(type(source) is str and re.fullmatch(r'[0-9a-f]{40}',source),'committed source')
    require(baseline['status']=='FROZEN_G0' and baseline['source']=='071243db068835617a054a74fb5734a6dd9f128d','immutable G0 identity')
    require(type(data) is dict and set(data)=={'schema','source','profile','mode','status','native_display','records'},'collection fields')
    require(type(data['schema']) is int and data['schema']==1 and data['source']==source and data['profile']=='fast-dev' and data['mode']=='Full' and data['status']=='COLLECTED_PENDING_REVIEW' and data['native_display']=='NOT MEASURED','collection provenance')
    expected={tuple(r[k] for k in KEY):r for r in baseline['records']}
    require(len(expected)==105 and len(baseline['records'])==105,'frozen inventory')
    require(type(data['records']) is list and len(data['records'])==105,'105 records')
    seen=set();widths=0;age_rows=0;ages=0
    for r in data['records']:
        require(type(r) is dict,'record')
        k=tuple(r.get(f) for f in KEY);require(k in expected and k not in seen,'exact recipe');seen.add(k);old=expected[k]
        require(set(r)==set(old),'exact recipe fieldset')
        require(r['source']==source and r['profile']=='fast-dev' and r['mode']=='Full' and r['telemetry_enabled'] is True,'record provenance')
        for f in ('item_count','scale_milli','warmups','samples','exclusions','p50_ns','p95_ns','max_ns'):require(uint(r[f]),'uint '+f)
        require((r['warmups'],r['samples'],r['exclusions'])==(5,20,0),'sample protocol')
        for f in ARRAYS:require(type(r[f]) is list and len(r[f])==20,'array '+f)
        require(all(uint(v) for v in r['elapsed_ns']+r['work_units']),'timing/work uint')
        for f in ('complete_output_state_ids','structural_output_ids'):require(all(type(v) is str and re.fullmatch(r'[0-9a-f]{16}',v) for v in r[f]),'hex identity')
        for f in ('revision_receipts','viewport_receipts','work_counters'):require(all(type(v) is str for v in r[f]),'typed receipt array')
        ordered=sorted(r['elapsed_ns']);require([r[f] for f in ('p50_ns','p95_ns','max_ns')]==[ordered[i] for i in (9,18,19)],'nearest ranks')
        if 'width_receipt' in r:
            w=r['width_receipt'];ow=old['width_receipt'];require(type(w) is dict and set(w)==set(ow),'width fields')
            require(w['scenario']==ow['scenario'] and w['transition']==ow['transition'],'width scenario')
            values=w['actual_scroll_area_width_milli'];require(type(values) is list and len(values)==20 and all(uint(v) for v in values),'width20array');widths+=1
        if 'event_age_samples' in r:
            rows=r['event_age_samples'];require(type(rows) is list and len(rows)==20,'age20rows');age_rows+=len(rows)
            for index,row in enumerate(rows):
                require(type(row) is dict and set(row)==set(old['event_age_samples'][index]),'age row fields')
                require(type(row) is dict and type(row.get('age_ns')) is list and all(uint(v) for v in row['age_ns']),'age array')
                ages+=len(row['age_ns'])
    require(seen==set(expected) and widths==6 and age_rows==40 and ages==440,'complete inventory/receipt cardinalities')
    return dict(schema=1,status='INVENTORY_PASS_FAMILY_COMPARATORS_REQUIRED',source=source,profile='fast-dev',mode='Full',records=105,retained_samples=2100,width_arrays=6,width_samples=120,event_age_rows=40,individual_ages=440,owner_counts=dict(Counter(r['owner'] for r in data['records'])),limits='Inventory and basic scalar/array schema only; all four family comparators and independent review remain required')

def main():
    a=argparse.ArgumentParser();a.add_argument('--input',type=Path,required=True);a.add_argument('--baseline',type=Path,required=True);a.add_argument('--source',required=True);a.add_argument('--output',type=Path,required=True);args=a.parse_args()
    result=validate(strict_json(args.input.read_text(encoding='utf-8-sig')),json.loads(args.baseline.read_text(encoding='utf-8-sig')),args.source)
    with args.output.open('x',encoding='utf-8') as f:json.dump(result,f,indent=2);f.write('\n')

if __name__=='__main__':main()
