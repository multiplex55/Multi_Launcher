"""Frozen strict collector for controlled C2-G CPU-root-frame receipts."""
import json
import math
import re
from pathlib import Path

MARKER = 'TRACK_C2G_QUERY_RESULT'
CASES = {
    'typed_suffix_sequence': (4, 4),
    'app_provider_supersession': (4, 4),
    'equivalent_filter_sequence': (2, 2),
    'viewport_controls': (0, 7),
}

class Invalid(ValueError):
    pass

def require(condition, code):
    if not condition:
        raise Invalid(code)

def object_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate-json-key')
        result[key] = value
    return result

def bad_constant(_):
    raise Invalid('nonfinite-json-number')

def read_records(paths):
    records = []
    for path in paths:
        p = Path(path)
        require(p.stat().st_size <= 64 * 1024 * 1024, 'log-size-limit')
        for line in p.read_text(encoding='utf-8-sig').splitlines():
            if MARKER not in line:
                continue
            require(line.count(MARKER) == 1, 'multiple-markers-on-line')
            _, _, body = line.partition(MARKER)
            require(body.startswith(' '), 'marker-separator')
            try:
                value = json.loads(body.strip(), object_pairs_hook=object_pairs,
                                   parse_constant=bad_constant)
            except json.JSONDecodeError as error:
                raise Invalid('invalid-json-record') from error
            require(type(value) is dict, 'record-object-required')
            records.append(value)
    require(bool(records), 'no-query-records')
    return records

def exact_keys(value, expected, code):
    require(type(value) is dict and set(value) == set(expected), code)

def uint(value, code):
    require(type(value) is int and 0 <= value <= 2**64 - 1, code)

def boolean(value, code):
    require(type(value) is bool, code)

def signature(value, code):
    require(type(value) is str and re.fullmatch('[0-9a-f]{16}', value) is not None, code)

def catalog(value, code):
    require(type(value) is list and len(value) == 3, code)
    for item in value:
        uint(item, code)

def distribution(values):
    require(len(values) == 20, 'distribution-sample-count')
    for value in values:
        uint(value, 'distribution-value')
    ordered = sorted(values)
    return dict(p50_ns=ordered[math.ceil(.50 * len(ordered)) - 1],
                p95_ns=ordered[math.ceil(.95 * len(ordered)) - 1],
                max_ns=ordered[-1])


HEADER_KEYS = {
    'schema','stage','owner','owner_scope','provider_scope','fixture','source',
    'profile','mode','telemetry_enabled','observer_enabled','warmups','samples',
    'exclusions','frame_limit','case','count','fixture_signature','sample_records',
    'sequence_elapsed_scope',
}
STATIC_HEADER = {
    'schema': 1,
    'sequence_elapsed_scope': 'whole_controlled_sequence_including_interstep_validation',
    'stage': 'gui_query_to_usable_cpu_viewport',
    'owner': 'egui_Context_run+LauncherApp::render_root_frame',
    'owner_scope': 'accepted_gui_mutation_to_first_usable_cpu_root_frame',
    'provider_scope': 'captured_controlled_snapshot_no_other_provider_calls',
    'fixture': 'synthetic_action_catalog_controlled_provider',
    'profile': 'fast-dev',
    'telemetry_enabled': True,
    'observer_enabled': True,
    'warmups': 5,
    'samples': 20,
    'exclusions': 0,
    'frame_limit': 8,
}

def validate_inventory(records, source, mode, fixture_bindings):
    """Frozen headers with independent exact fixture binding."""
    require(mode in ('small','full'), 'unsupported-mode')
    require(type(source) is str and (source == 'UNCOMMITTED_SOURCE'
            or re.fullmatch('[0-9a-f]{40}',source) is not None),
            'invalid-expected-source')
    require(mode != 'full' or source != 'UNCOMMITTED_SOURCE','full-requires-committed-source')
    sizes = (100,) if mode == 'small' else (100,1000,10000)
    expected = {(case,count) for case in CASES for count in sizes}
    require(len(records) == len(expected), 'record-inventory-count')
    actual = set()
    for record in records:
        exact_keys(record,HEADER_KEYS,'record-keys')
        for key,value in STATIC_HEADER.items():
            require(type(record[key]) is type(value) and record[key] == value,
                    'header-'+key)
        require(type(record['source']) is str and record['source'] == source,
                'source-mismatch')
        require(type(record['mode']) is str and record['mode'] == mode,
                'mode-mismatch')
        require(type(record['case']) is str and record['case'] in CASES,
                'unknown-case')
        uint(record['count'],'catalog-count')
        key = (record['case'],record['count'])
        require(key in expected,'unexpected-case-count')
        require(key not in actual,'duplicate-case-count')
        actual.add(key)
        signature(record['fixture_signature'],'fixture-signature-format')
        require(key in fixture_bindings,'independent-fixture-binding-missing')
        require(record['fixture_signature'] == fixture_bindings[key],
                'fixture-binding-mismatch')
        require(type(record['sample_records']) is list
                and len(record['sample_records']) == 20,'sample-record-count')
    require(actual == expected,'record-inventory-set')


def geometry(value, rebuilds, rows, code):
    require(type(value) is list and len(value) == 5, code+'-causes')
    sums = [0,0]
    for pair in value:
        require(type(pair) is list and len(pair) == 2,code+'-pair')
        for index,item in enumerate(pair):
            uint(item,code+'-value')
            sums[index] += item
        require(pair[0] != 0 or pair[1] == 0,code+'-rows-without-rebuild')
    uint(rebuilds,code+'-rebuilds')
    uint(rows,code+'-rows')
    require(sums == [rebuilds,rows],code+'-sum')

INTENT_KEYS = {
    'sequence','origin','terminal','cause','started_frame','terminal_frame',
    'frame_count','terminal_elapsed_ns','completion_latency_ns','acceptance',
    'painted_frame','painted_generation','painted_sequence','painted_acceptance_frame',
    'geometry_cause_order','geometry_rebuilds','geometry_rows',
    'geometry_by_cause',
}
ACCEPTANCE_KEYS = {
    'kind','frame','result_generation','provider_revision','catalog_versions',
    'catalog_versions_stable','search_pending','capacity_deferred',
    'direct_source_before','direct_source_after','direct_catalog_before','direct_catalog_after',
}

def validate_intent(intent, expected_terminal, sequence_span):
    """Frozen controlled TextEdit receipt."""
    exact_keys(intent,INTENT_KEYS,'intent-keys')
    for key in ('sequence','started_frame','terminal_frame','frame_count',
                'terminal_elapsed_ns','geometry_rebuilds','geometry_rows'):
        uint(intent[key],'intent-'+key)
    require(intent['sequence'] > 0,'intent-sequence-positive')
    require(intent['origin'] == 'text_edit','intent-origin')
    require(intent['terminal'] == expected_terminal,'intent-terminal')
    require(intent['started_frame'] <= intent['terminal_frame'],'intent-frame-order')
    require(intent['terminal_elapsed_ns'] <= sequence_span,'intent-span-bound')
    geometry(intent['geometry_by_cause'],intent['geometry_rebuilds'],
             intent['geometry_rows'],'intent-geometry')
    acceptance = intent['acceptance']
    if acceptance is not None:
        exact_keys(acceptance,ACCEPTANCE_KEYS,'acceptance-keys')
        require(acceptance['kind'] == 'search','controlled-acceptance-kind')
        for key in ('frame','result_generation','provider_revision'):
            uint(acceptance[key],'acceptance-'+key)
        catalog(acceptance['catalog_versions'],'acceptance-catalog')
        for key in ('catalog_versions_stable','search_pending','capacity_deferred'):
            boolean(acceptance[key],'acceptance-'+key)
        require(acceptance['direct_source_before'] is None
                and acceptance['direct_source_after'] is None
                and acceptance['direct_catalog_before'] is None
                and acceptance['direct_catalog_after'] is None,'search-direct-fields')
        require(intent['started_frame'] <= acceptance['frame']
                <= intent['terminal_frame'],'acceptance-frame-order')
    if expected_terminal == 'completed':
        require(intent['cause'] == 'first_usable_root_frame','completed-cause')
        require(acceptance is not None,'completed-acceptance-missing')
        require(acceptance['catalog_versions_stable']
                and not acceptance['search_pending']
                and not acceptance['capacity_deferred'],'completed-acceptance-gates')
        uint(intent['completion_latency_ns'],'completed-latency')
        require(0 < intent['completion_latency_ns'] <= intent['terminal_elapsed_ns'],
                'completed-latency-bound')
        require(1 <= intent['frame_count'] <= 8,'completed-frame-bound')
        require(intent['painted_frame'] == intent['terminal_frame'],
                'completed-current-frame-paint')
        uint(intent['painted_frame'],'completed-painted-frame')
        uint(intent['painted_generation'],'completed-painted-generation')
        require(intent['painted_generation'] == acceptance['result_generation'],
                'completed-accepted-painted-generation')
    else:
        require(expected_terminal == 'superseded','unsupported-controlled-terminal')
        require(intent['cause'] == 'superseded_by_query','superseded-cause')
        require(intent['completion_latency_ns'] is None,'abandoned-latency-forbidden')
        for key in ('painted_frame','painted_generation'):
            if intent[key] is not None:
                uint(intent[key],'abandoned-'+key)


FRAME_KEYS = {
    'frame','elapsed_ns','viewport_milli','visible','root_owned',
    'current_result_generation','provider_revision','catalog_versions',
    'catalog_versions_stable','pending_query','background_refresh_pending',
    'search_pending','capacity_deferred','selected_index','popup_open',
    'painted_generation','geometry_rebuilds','geometry_rows','geometry_by_cause',
    'step','kind','provider_calls_before','provider_calls_after','provider_calls_delta',
    'provider_generation_before','provider_generation_after','action_count',
    'action_payload_utf8_bytes','action_output_id','full_state_id','geometry_id','viewport_id',
}

def validate_frame(frame, sequence_span):
    """Frozen actual-emitter frame shape."""
    exact_keys(frame,FRAME_KEYS,'frame-keys')
    for key in ('frame','elapsed_ns','current_result_generation',
                'provider_revision','geometry_rebuilds','geometry_rows'):
        uint(frame[key],'frame-'+key)
    require(frame['frame'] > 0,'frame-positive')
    require(0 < frame['elapsed_ns'] <= sequence_span,'frame-elapsed-bound')
    require(type(frame['viewport_milli']) is list
            and len(frame['viewport_milli']) == 2,'frame-viewport-cardinality')
    for value in frame['viewport_milli']:
        uint(value,'frame-viewport-value')
        require(value > 0,'frame-viewport-positive')
    catalog(frame['catalog_versions'],'frame-catalog')
    for key in ('visible','root_owned','catalog_versions_stable','pending_query',
                'background_refresh_pending','search_pending','capacity_deferred',
                'popup_open'):
        boolean(frame[key],'frame-'+key)
    for key in ('selected_index','painted_generation'):
        if frame[key] is not None:
            uint(frame[key],'frame-'+key)
    geometry(frame['geometry_by_cause'],frame['geometry_rebuilds'],
             frame['geometry_rows'],'frame-geometry')


CAUSE_ORDER=['query','provider_or_catalog','display_or_resize','navigation','other']
KINDS={
 'typed_suffix_sequence':['text_edit_suffix']*4,
 'app_provider_supersession':['append_app_space','capacity_deferred_provider_query','provider_refresh_then_superseding_beta_edit','final_provider_query'],
 'equivalent_filter_sequence':['whole_query_filter_replacement']*2,
 'viewport_controls':['resize_narrow','resize_restore','select_first','select_middle','select_last','open_result_context_menu','close_result_context_menu'],
}
SAMPLE_KEYS={'sequence_elapsed_ns','sequence_frame_count','intents','steps','provider_calls_total','provider_generation','catalog_versions','action_count','action_payload_utf8_bytes','action_output_id','full_state_id','geometry_id','viewport_id'}
STEP_KEYS={'index','kind','provider_calls_delta','frame'}
ID_KEYS=('action_output_id','full_state_id','geometry_id','viewport_id')

def validate_output(value,expected):
    for key in ID_KEYS: signature(value[key],'output-'+key)
    for key in ('action_count','action_payload_utf8_bytes'): uint(value[key],'output-'+key)
    require(value['action_output_id']==expected['full_action_id'],'independent-action-identity')
    require(value['action_count']==expected['action_count'],'independent-action-count')
    require(value['action_payload_utf8_bytes']==expected['payload_utf8_bytes'],'independent-action-payload')

def validate_sample(sample,case,count,expected_steps):
    exact_keys(sample,SAMPLE_KEYS,'sample-keys')
    for key in ('sequence_elapsed_ns','sequence_frame_count','provider_calls_total','provider_generation'):
        uint(sample[key],'sample-'+key)
    span=sample['sequence_elapsed_ns']; require(span>0,'sequence-positive')
    catalog(sample['catalog_versions'],'sample-catalog')
    require(type(sample['steps']) is list and len(sample['steps'])==CASES[case][1],'step-inventory')
    require(type(sample['intents']) is list and len(sample['intents'])==CASES[case][0],'intent-inventory')
    require(sample['sequence_frame_count']==len(sample['steps']),'sequence-frame-accounting')
    frames=[];total_calls=0
    for index,(step,expected) in enumerate(zip(sample['steps'],expected_steps)):
        exact_keys(step,STEP_KEYS,'step-keys')
        uint(step['index'],'step-index-type');require(step['index']==index,'step-index')
        require(step['kind']==KINDS[case][index],'step-kind')
        uint(step['provider_calls_delta'],'step-provider-calls')
        frame=step['frame']; validate_frame(frame,span)
        uint(frame['step'],'frame-step-type');require(frame['step']==index,'frame-step')
        require(frame['kind']==step['kind'],'frame-step-kind')
        require(frame['frame']==index+1,'frame-sequence')
        for key in ('provider_calls_before','provider_calls_after','provider_calls_delta','provider_generation_before','provider_generation_after'):
            uint(frame[key],'frame-'+key)
        require(frame['provider_calls_before']==total_calls,'provider-call-continuity')
        require(frame['provider_calls_after']>=frame['provider_calls_before'],'provider-call-order')
        require(frame['provider_calls_after']-frame['provider_calls_before']==frame['provider_calls_delta']==step['provider_calls_delta'],'provider-call-accounting')
        total_calls=frame['provider_calls_after']
        require(frame['provider_generation_before']==frame['provider_generation_after']==frame['provider_revision'],'frame-provider-stability')
        require(frame['catalog_versions']==sample['catalog_versions'],'frame-catalog-match')
        require(frame['visible'] and frame['root_owned'],'frame-root-gates')
        require(not frame['pending_query'] and not frame['background_refresh_pending'],'frame-refresh-gates')
        deferred=case=='app_provider_supersession' and index==1
        require(frame['catalog_versions_stable']==(not deferred),'frame-source-stability')
        require(frame['capacity_deferred']==deferred,'frame-capacity-gate')
        require(frame['search_pending']==deferred,'frame-search-pending-gate')
        require(frame['painted_generation']==frame['current_result_generation'],'frame-painted-generation')
        selected=([None,None,0,count//2,count-1,count-1,count-1][index] if case=='viewport_controls' else None)
        require(frame['selected_index']==selected,'frame-selection')
        require(frame['popup_open']==(case=='viewport_controls' and index==5),'frame-popup')
        validate_output(frame,expected)
        expected_geometry=[[0,0] for _ in range(5)]
        if case != 'viewport_controls':expected_geometry[0]=[1,expected['action_count']]
        elif index < 2:expected_geometry[2]=[1,count]
        require(frame['geometry_by_cause']==expected_geometry,'independent-recipe-geometry')
        frames.append(frame)
    require(sum(f['elapsed_ns'] for f in frames)<=span,'frame-time-sum-bound')
    require(total_calls==sample['provider_calls_total']==(4 if case=='app_provider_supersession' else 0),'provider-total')
    require(frames[-1]['provider_revision']==sample['provider_generation'],'final-provider-revision')
    if case=='app_provider_supersession':
        revisions=[f['provider_revision'] for f in frames]
        require(revisions[0]==revisions[1] and revisions[2]==revisions[3]==revisions[0]+1,'provider-one-notification')
        require([f['provider_calls_delta'] for f in frames]==[1,0,2,1],'provider-step-calls')
    else:
        require(len({f['provider_revision'] for f in frames})==1,'stable-provider-control')
    for key in (*ID_KEYS,'action_count','action_payload_utf8_bytes'):
        require(sample[key]==frames[-1][key],'sample-final-'+key)
    validate_output(sample,expected_steps[-1])
    terminals=['completed']*CASES[case][0]
    if case=='app_provider_supersession':terminals[1]='superseded'
    for index,(intent,terminal) in enumerate(zip(sample['intents'],terminals)):
        validate_intent(intent,terminal,span)
        require(intent['sequence']==index+1,'intent-sequence-order')
        require(intent['geometry_cause_order']==CAUSE_ORDER,'intent-cause-order')
        require(intent['started_frame']==index+1,'intent-start-frame')
        require(intent['terminal_frame']==(3 if terminal=='superseded' else index+1),'intent-terminal-frame')
        require(intent['frame_count']==intent['terminal_frame']-intent['started_frame']+1,'intent-frame-accounting')
        for key in ('painted_sequence','painted_acceptance_frame'):
            if intent[key] is not None:uint(intent[key],'intent-'+key)
        if terminal=='completed':
            accepted=intent['acceptance'];frame=frames[intent['terminal_frame']-1]
            require(intent['terminal_elapsed_ns']<=frame['elapsed_ns'],'completed-terminal-frame-time-bound')
            require(intent['painted_sequence']==intent['sequence'],'paint-sequence-match')
            require(intent['painted_acceptance_frame']==accepted['frame'],'paint-acceptance-frame-match')
            require(accepted['frame']==frame['frame'],'acceptance-current-frame')
            require(accepted['provider_revision']==frame['provider_revision'],'acceptance-current-provider')
            require(accepted['catalog_versions']==frame['catalog_versions'],'acceptance-current-catalog')
            require(accepted['result_generation']==frame['current_result_generation'],'acceptance-current-generation')
            require(intent['geometry_by_cause']==frame['geometry_by_cause'],'completed-geometry-frame-match')
        else:
            accepted=intent['acceptance'];frame=frames[2]
            require(intent['geometry_by_cause']==frames[1]['geometry_by_cause'],'superseded-geometry-frame-match')
            require(accepted is not None,'superseded-acceptance-required')
            require(accepted['frame']==frame['frame'],'superseded-refresh-frame')
            require(accepted['catalog_versions_stable'] and not accepted['search_pending'] and not accepted['capacity_deferred'],'superseded-refresh-gates')
            require(accepted['provider_revision']==frame['provider_revision'] and accepted['catalog_versions']==frame['catalog_versions'],'superseded-refresh-sources')
            require(accepted['result_generation']+1==frame['current_result_generation']==frames[1]['current_result_generation']+2,'superseded-refresh-generation')
            require(intent['painted_sequence'] is None and intent['painted_acceptance_frame'] is None
                    and intent['painted_frame'] is None and intent['painted_generation'] is None,'superseded-unpainted')
    return sample

def load_bindings():
    root=Path(__file__).resolve().parent
    fixtures=json.loads((root/'independent-fixture-bindings-v1.json').read_text(encoding='utf-8-sig'))
    steps=json.loads((root/'independent-step-bindings-v1.json').read_text(encoding='utf-8-sig'))
    return ({(r['case'],r['count']):r['fixture_signature'] for r in fixtures['records']},
            {(r['case'],r['count']):r['step_outputs'] for r in steps['records']})

def validate_all(records,source,mode):
    fixtures,steps=load_bindings();validate_inventory(records,source,mode,fixtures)
    for record in records:
        for sample in record['sample_records']:
            validate_sample(sample,record['case'],record['count'],steps[(record['case'],record['count'])])
    return records


def summarize(records,source,mode):
    validate_all(records,source,mode)
    summaries=[]
    for record in records:
        samples=record['sample_records']
        intent_summaries=[]
        for index in range(CASES[record['case']][0]):
            intents=[sample['intents'][index] for sample in samples]
            completed=intents[0]['terminal']=='completed'
            intent_summaries.append(dict(index=index,terminal=intents[0]['terminal'],
                completion_latency=distribution([i['completion_latency_ns'] for i in intents]) if completed else None,
                terminal_elapsed_diagnostic=distribution([i['terminal_elapsed_ns'] for i in intents]),
                geometry_rebuilds=[i['geometry_rebuilds'] for i in intents],
                geometry_rows=[i['geometry_rows'] for i in intents],
                geometry_by_cause=[i['geometry_by_cause'] for i in intents]))
        summaries.append(dict(case=record['case'],count=record['count'],fixture_signature=record['fixture_signature'],
            sequence_elapsed=distribution([s['sequence_elapsed_ns'] for s in samples]),
            sequence_frame_counts=[s['sequence_frame_count'] for s in samples],intents=intent_summaries,
            step_frame_elapsed=[dict(index=index,elapsed=distribution([s['steps'][index]['frame']['elapsed_ns'] for s in samples])) for index in range(CASES[record['case']][1])],
            sample_records=samples))
    return dict(schema=1,status='PASS',source=source,profile='fast-dev',mode=mode,
                record_count=len(records),sequence_count=sum(len(r['sample_records']) for r in records),
                warmups_per_record=5,retained_samples_per_record=20,exclusions=0,
                quantiles='nearest-rank20:p50=index9,p95=index18,max=index19',
                scope='controlled_headless_CPU_root_frame_not_native_input_or_GPU_display',
                sequence_elapsed_scope=STATIC_HEADER['sequence_elapsed_scope'],
                abandoned_elapsed_scope='diagnostic_only_not_completed_latency',
                records=summaries)

def main():
    import argparse
    parser=argparse.ArgumentParser(description='Strict C2-G controlled CPU query receipt collector')
    parser.add_argument('--source',required=True)
    parser.add_argument('--mode',required=True,choices=('small','full'))
    parser.add_argument('--output',required=True)
    parser.add_argument('logs',nargs='+')
    args=parser.parse_args()
    result=summarize(read_records(args.logs),args.source,args.mode)
    with Path(args.output).open('x',encoding='utf-8',newline='\n') as destination:
        json.dump(result,destination,indent=2);destination.write('\n')
    print(f"PASS {result['record_count']} records/{result['sequence_count']} sequences; controlled CPU root frame scope")

if __name__=='__main__':
    try:main()
    except Invalid as error:
        raise SystemExit('REJECT '+str(error))
