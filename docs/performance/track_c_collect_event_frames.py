"""Strict C3-C companion collector; separate from original G0 queue controls.

Draft until the source/schema and method review are accepted. No personal payloads
are accepted: bounded static receipt vocabulary and numeric/hex identities only.
"""
import argparse
from collections import Counter
import json
import math
from pathlib import Path
import re

MARKER = 'TRACK_C3C_EVENT_RESULT '
CASES = ('quiet_settled_frame', 'finite_burst_frame', 'bounded_replenishing_frame')
EVENTS = dict(zip(CASES, (0, 10, 18)))
HANDLERS = ['clipboard_modify', 'gui_actions', 'gui_notes', 'json_dashboard']
KINDS = {'actions': ('FileReload', 'FileWatcher'),
         'index_ready': ('IndexCompletion', 'IndexCoordinator'),
         'recycle': ('Clipboard', 'Registry'),
         'screen_draw_emergency': ('Recovery', 'Registry'),
         'radial_dispatch': ('Radial', 'Registry')}


TOP_KEYS = """schema stage owner measurement_scope sequence_elapsed_scope setup_root_frame_scope completion_scope source profile mode telemetry_enabled fixture fixture_signature case count warmups samples exclusions recorder_capacity max_frames age_scope frame_scope registry_scope depth_scope finite_finalization_scope original_setup_scope owner_elapsed_p50_ns owner_elapsed_p95_ns owner_elapsed_max_ns owner_elapsed_mean_ns frame_elapsed_ns_sum frame_elapsed_ns_max event_age_distribution sample_records""".split()
SAMPLE_KEYS = """sample owner_elapsed_ns setup_accepted setup_dequeued setup_root_frames trigger_accepted producer_accepted accepted_in_owner dequeued_in_owner queue_depth_before_owner queue_depth_after_triggers queue_depth_after_owner final_queue_depth registry_queued_before_owner registry_queued_after_triggers registry_queued_after_owner send_failures overflow abandoned event_ages age_p50_ns age_p95_ns age_max_ns age_mean_ns age_scope frames frame_elapsed_ns_sum frame_elapsed_ns_max event_drain_calls event_drain_ns event_drain_work_units drain_send_ack_windows handler_receipts setup_viewport_wake_requests setup_egui_repaint_requests viewport_wake_requests_in_owner egui_repaint_requests_in_owner producer_ack_elapsed_ns producer_sends_total producer_quiescent coordinator_B_pending_during_owner action_output_id projection_output_id full_state_id viewport_id revision_receipts completion_receipt urgent_receipts finalization handler_lifecycle""".split()
FRAME_KEYS = """frame owner_start_ns owner_complete_ns elapsed_ns queue_depth_before queue_depth_after registry_queued_before registry_queued_after dequeued dispatch_ordinal_start dispatch_ordinal_end producer_successful_sends_before producer_successful_sends_after viewport_wake_requests egui_repaint_requests full_output_dropped""".split()
AGE_KEYS = "ordinal kind frame class origin age_origin age_ns depth_after_dequeue".split()
HANDLER_KEYS = "class origin enqueued dequeued send_failed abandoned age_ns_total age_ns_max handler_calls handler_ns_total handler_ns_max".split()
URGENT_KEYS = """screen_draw_intent_id screen_draw_activity_epoch screen_draw_bridge_active_after_admission screen_draw_stale_intent_receipts radial_invocation_id radial_lease_registered radial_rejected_before_resolution radial_consumed query_unchanged last_query_unchanged history_unchanged visibility_unchanged restore_unchanged result_generation_delta expected_action_refresh_generation_delta provider_revision_unchanged catalog_versions_unchanged""".split()
FINAL_KEYS = "scope accepted dequeued kind class origin age_ns queue_depth_after final_successful_sends final_dequeued overflow abandoned".split()


def exact_keys(value, keys, scope):
    require(type(value) is dict and set(value) == set(keys), 'exact keys: '+scope)


def strict_json(text):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, 'duplicate JSON key')
            result[key] = value
        return result
    def invalid_constant(value):
        raise ValueError('nonfinite JSON constant: '+value)
    return json.loads(text, object_pairs_hook=pairs, parse_constant=invalid_constant,
                      parse_float=invalid_constant)


def require(ok, message):
    if not ok:
        raise ValueError(message)


def uint(value):
    return type(value) is int and 0 <= value <= 2**64 - 1


def hexid(value):
    return type(value) is str and re.fullmatch('[0-9a-f]{16}', value) is not None


def quantiles(values):
    if not values:
        return dict(p50_ns=None, p95_ns=None, max_ns=None, mean_ns=None)
    ordered = sorted(values)
    return dict(p50_ns=ordered[math.ceil(len(values)*.5)-1],
                p95_ns=ordered[math.ceil(len(values)*.95)-1],
                max_ns=ordered[-1], mean_ns=sum(values)//len(values))


def fixture_signature(count, case):
    # Independently reconstruct watch.rs track_c3c_fixture_signature. The base
    # identities are frozen unchanged Actions fixture identities, not measured
    # queue values. Strings are explicitly length-framed, integers u64 LE.
    value = 0xcbf29ce484222325
    def raw(data):
        nonlocal value
        for byte in data:
            value = ((value ^ byte) * 0x100000001b3) & (2**64-1)
    def number(n):
        raw(n.to_bytes(8, 'little'))
    def token(s):
        encoded = s.encode('utf-8')
        number(len(encoded))
        raw(encoded)
    raw(b'track-c3c-event-frame')
    number(0x4333435f4556454e)
    number(count)
    number({200: 0xb0999378b773fa54, 20000: 0xc75034d300362287}[count])
    for n in (count, count//2, count-count//2):
        number(n)
    token(case)
    for n in ((8 if case == CASES[1] else 1),
              (0 if case == CASES[0] else 2),
              (16 if case == CASES[2] else 0), 32, 8, 960, 640):
        number(n)
    for s in ('query=app Synthetic action 00009',
              'match_exact=true;usage=empty;usage_weight=0;fuzzy_weight=1',
              'plugins=inert;dashboard_initial_refresh_queued=true;toasts=off;hotkeys=off',
              'fixture_origin=owned_actions_owner;paths=owned_root_only'):
        token(s)
    order = {CASES[0]: (), CASES[1]: (
        'setup:actions_file_changed', 'setup:index_ready_candidate_stale_under_B',
        'setup:five_equal_actions', 'setup:registry_recycle_ok_inert',
        'trigger:screen_draw_emergency_stale', 'trigger:radial_dispatch_missing_lease',
        'untimed:release_and_drain_index_B'), CASES[2]: (
        'gate:barrier_start', 'producer:sixteen_actions_file_watcher_send_then_viewport_wake',
        'trigger:screen_draw_emergency_stale', 'trigger:radial_dispatch_missing_lease',
        'stop:producer_ack_and_zero_app_depth')}[case]
    number(len(order))
    for s in order:
        token(s)
    return f'{value:016x}'


def validate_sample(sample, case, index, count):
    exact_keys(sample, SAMPLE_KEYS, 'sample')
    require(uint(sample.get('sample')) and sample['sample'] == index, 'sample order')
    total = EVENTS[case]
    quiet, finite, producer = case == CASES[0], case == CASES[1], case == CASES[2]
    numeric = ('owner_elapsed_ns', 'setup_accepted', 'setup_dequeued', 'setup_root_frames', 'trigger_accepted',
               'producer_accepted', 'accepted_in_owner', 'dequeued_in_owner',
               'queue_depth_before_owner', 'queue_depth_after_triggers',
               'queue_depth_after_owner', 'final_queue_depth', 'abandoned', 'send_failures',
               'frame_elapsed_ns_sum', 'frame_elapsed_ns_max', 'event_drain_calls',
               'event_drain_ns', 'event_drain_work_units', 'producer_sends_total')
    require(all(uint(sample.get(k)) for k in numeric), 'typed sample numeric')
    require(sample['setup_accepted'] == (8 if finite else 1)
            and sample['setup_dequeued'] == (0 if finite else 1), 'setup accounting')
    require(sample['setup_root_frames'] == int(not finite), 'actual root setup frames')
    require(sample['trigger_accepted'] == (0 if quiet else 2)
            and sample['producer_accepted'] == (16 if producer else 0)
            and sample['producer_sends_total'] == (16 if producer else 0), 'trigger/producer accounting')
    require(sample['accepted_in_owner'] == (0 if quiet else 18 if producer else 2)
            and sample['dequeued_in_owner'] == total, 'measured accounting')
    require(sample['queue_depth_before_owner'] == (8 if finite else 0), 'initial measured backlog')
    require(all(sample[k] == 0 for k in ('queue_depth_after_owner', 'final_queue_depth',
                                        'abandoned', 'send_failures'))
            and sample.get('overflow') is False and sample.get('producer_quiescent') is True,
            'quiescence/overflow/failures')
    require(sample.get('coordinator_B_pending_during_owner') is finite, 'B pending scope')
    for k in ('action_output_id', 'projection_output_id', 'full_state_id', 'viewport_id'):
        require(hexid(sample.get(k)), 'identity '+k)
    require(sample['queue_depth_after_triggers'] == (10 if finite else 0) if not producer
            else 2 <= sample['queue_depth_after_triggers'] <= 18, 'post-trigger backlog')
    for k in ('registry_queued_before_owner','registry_queued_after_triggers','registry_queued_after_owner',
              'setup_viewport_wake_requests','setup_egui_repaint_requests',
              'viewport_wake_requests_in_owner','egui_repaint_requests_in_owner'):
        require(uint(sample.get(k)), 'typed wake/registry '+k)
    require(sample['registry_queued_before_owner'] == int(finite)
            and sample['registry_queued_after_triggers'] == (3 if finite else 2 if producer else 0)
            and sample['registry_queued_after_owner'] == 0
            and sample['setup_viewport_wake_requests'] == int(finite)
            and sample['viewport_wake_requests_in_owner'] == (18 if producer else 2 if finite else 0), 'wake/registry accounting')
    expected_age_scope = ('event_weighted_owner_dequeues_including_setup_aged_envelopes' if finite else
                          'NOT_APPLICABLE_zero_events' if quiet else 'event_weighted_owner_dequeues')
    require(sample['age_scope'] == expected_age_scope, 'sample age scope')
    completion = sample.get('completion_receipt')
    exact_keys(completion, ['state','suggestions_count'], 'completion')
    require(completion['state'] in ('pending','rebuilt') and uint(completion['suggestions_count'])
            and completion['suggestions_count'] == int(completion['state'] == 'rebuilt'), 'observed completion state')
    # Independently reconstructed before owner execution; see the companion
    # identity_bindings.json and reconstruct_watch_identities.py, including
    # frozen ordered-fixture and canonical same-host drive qualifications.
    action_id, pending_id, rebuilt_id = {
        200: ('f2e34bcc96dca53e','3f51907ee2bc81c6','db841f1f4f054b11'),
        20000: ('dfcb0be56e94ee20','b85f8321bd00eb1c','1c1962e165ab4ef7')}[count]
    require(sample['action_output_id'] == action_id
            and sample['projection_output_id'] == (rebuilt_id if completion['state'] == 'rebuilt' else pending_id),
            'independent full Action/projection binding')
    revisions = sample.get('revision_receipts')
    require(type(revisions) is list and len(revisions) == 12
            and all(uint(v) for v in revisions) and revisions[1] == revisions[0]+1
            and revisions[2] == revisions[3] and revisions[4:7] == revisions[7:10]
            and revisions[10] == 1, 'revision receipts')
    lifecycle = sample.get('handler_lifecycle', {})
    exact_keys(lifecycle, ['attempted','created','released','complete','overflow'], 'lifecycle')
    require(all(lifecycle.get(k) == HANDLERS for k in ('attempted', 'created', 'released'))
            and lifecycle.get('complete') is True and lifecycle.get('overflow') is False,
            'callback release inventory')
    ages = sample.get('event_ages')
    require(type(ages) is list and len(ages) == total, 'event age inventory')
    expected_counts = Counter({'actions': 6, 'index_ready': 1, 'recycle': 1,
                              'screen_draw_emergency': 1, 'radial_dispatch': 1}) if finite else (
        Counter({'actions': 16, 'screen_draw_emergency': 1, 'radial_dispatch': 1}) if producer else Counter())
    require(Counter(a.get('kind') for a in ages) == expected_counts, 'event kinds')
    if finite:
        require([a['kind'] for a in ages] == ['actions', 'index_ready']+['actions']*5+
                ['recycle', 'screen_draw_emergency', 'radial_dispatch'], 'finite FIFO')
    for ordinal, age in enumerate(ages):
        exact_keys(age, AGE_KEYS, 'event age')
        require(uint(age.get('ordinal')) and age.get('ordinal') == ordinal and uint(age.get('age_ns'))
                and uint(age.get('frame')) and uint(age.get('depth_after_dequeue')), 'typed dispatch')
        require((age.get('class'), age.get('origin')) == KINDS[age['kind']], 'class/origin')
        origin = ('setup_before_owner' if finite and ordinal < 8 else
                  'urgent_trigger_in_owner' if age['kind'] in ('screen_draw_emergency', 'radial_dispatch')
                  else 'bounded_producer_in_owner')
        require(age.get('age_origin') == origin, 'enqueue clock scope')
        if origin != 'setup_before_owner':
            require(age['age_ns'] <= sample['owner_elapsed_ns'], 'in-owner age bound')
    for k, v in quantiles([a['age_ns'] for a in ages]).items():
        actual = sample.get('age_'+k)
        require(actual is None if v is None else uint(actual) and actual == v, 'sample age nearest rank '+k)
    frames = sample.get('frames')
    require(type(frames) is list and 1 <= len(frames) <= (1 if quiet else 8), 'frame inventory')
    position, prior_end = 0, 0
    previous_ack = 0
    for frame_index, frame in enumerate(frames):
        exact_keys(frame, FRAME_KEYS, 'frame')
        require(frame.get('frame') == frame_index+1, 'frame order')
        for k in ('owner_start_ns', 'owner_complete_ns', 'elapsed_ns', 'queue_depth_before',
                  'queue_depth_after', 'registry_queued_before', 'registry_queued_after', 'dequeued',
                  'dispatch_ordinal_start', 'dispatch_ordinal_end', 'viewport_wake_requests', 'egui_repaint_requests'):
            require(uint(frame.get(k)), 'typed frame '+k)
        require(prior_end <= frame['owner_start_ns'] <= frame['owner_complete_ns'] <= sample['owner_elapsed_ns']
                and frame['elapsed_ns'] <= frame['owner_complete_ns']-frame['owner_start_ns'], 'frame nesting')
        prior_end = frame['owner_complete_ns']
        require(frame['full_output_dropped'] is True
                and frame['dispatch_ordinal_start'] == position
                and frame['dispatch_ordinal_end'] == position+frame['dequeued'], 'frame dispatch slice')
        require(all(a['frame'] == frame_index+1 for a in ages[position:position+frame['dequeued']]), 'event frame binding')
        position += frame['dequeued']
        before, after = (frame.get('producer_successful_sends_'+suffix) for suffix in ('before','after'))
        require((uint(before) and uint(after) and previous_ack <= before <= after <= 16) if producer
                else before is None and after is None, 'frame producer acknowledgment')
        if producer:
            previous_ack = after
        require(frame['registry_queued_before'] <= (3 if finite else 2 if producer else 0) and frame['registry_queued_after'] == 0, 'frame registry counts')
    require(position == total and sample['frame_elapsed_ns_sum'] == sum(f['elapsed_ns'] for f in frames)
            and sample['frame_elapsed_ns_max'] == max(f['elapsed_ns'] for f in frames), 'frame aggregates')
    require(sample['event_drain_calls'] == len(frames)
            and sample['event_drain_work_units'] == total
            and sample['event_drain_ns'] <= sample['frame_elapsed_ns_sum'] <= sample['owner_elapsed_ns'], 'drain accounting')
    windows = sample.get('drain_send_ack_windows')
    require(type(windows) is list and len(windows) == len(frames), 'drain window inventory')
    for i, window in enumerate(windows):
        exact_keys(window, ['frame','producer_successful_sends_before','producer_successful_sends_after'], 'drain window')
        require(window.get('frame') == i+1, 'drain frame')
        before, after = (window.get('producer_successful_sends_'+s) for s in ('before', 'after'))
        require((uint(before) and uint(after) and 0 <= before <= after <= 16) if producer
                else before is None and after is None, 'drain producer acknowledgment window')
        if producer:
            require(frames[i]['producer_successful_sends_before'] <= before <= after <= frames[i]['producer_successful_sends_after'], 'drain ack nesting')
    for prefix in ('viewport_wake','egui_repaint'):
        require(sum(f[prefix+'_requests'] for f in frames) <= sample[prefix+'_requests_in_owner'], 'frame wake subset')
    receipts = sample.get('handler_receipts')
    require(type(receipts) is list, 'handler receipts')
    counts = Counter((a['class'], a['origin']) for a in ages)
    require(len(receipts) == len(counts), 'handler class/origin inventory')
    seen = set()
    for receipt in receipts:
        exact_keys(receipt, HANDLER_KEYS, 'handler')
        key = (receipt.get('class'), receipt.get('origin'))
        require(key in counts and key not in seen, 'handler key')
        seen.add(key)
        require(all(uint(receipt.get(k)) for k in ('enqueued','dequeued','send_failed','abandoned',
                    'age_ns_total','age_ns_max','handler_calls','handler_ns_total','handler_ns_max')), 'typed handler')
        require(receipt['dequeued'] == receipt['handler_calls'] == counts[key]
                and receipt['send_failed'] == receipt['abandoned'] == 0, 'handler counts')
        require(receipt['enqueued'] == (0 if finite and key not in (('Recovery','Registry'),('Radial','Registry')) else counts[key]), 'owner-window enqueued')
        matching = [a['age_ns'] for a in ages if (a['class'],a['origin']) == key]
        require(receipt['age_ns_total'] == sum(matching) and receipt['age_ns_max'] == max(matching), 'handler age aggregates')
        require(receipt['handler_ns_max'] <= receipt['handler_ns_total'] <= receipt['handler_calls']*receipt['handler_ns_max']
                and receipt['handler_ns_total'] <= sample['event_drain_ns'], 'handler timing')
    require(sum(r['handler_ns_total'] for r in receipts) <= sample['event_drain_ns'], 'all handler timings nested in drain')
    cleanup = sample.get('finalization')
    if finite:
        exact_keys(cleanup, FINAL_KEYS, 'finalization')
        require(all(uint(cleanup[k]) for k in ('accepted','dequeued','age_ns','queue_depth_after','final_successful_sends','final_dequeued','overflow','abandoned')), 'typed cleanup accounting')
        require(type(cleanup) is dict and cleanup.get('scope') == 'untimed_blocked_index_B_completion'
                and cleanup.get('accepted') == cleanup.get('dequeued') == 1
                and (cleanup.get('kind'), cleanup.get('class'), cleanup.get('origin')) ==
                    ('index_ready', 'IndexCompletion', 'IndexCoordinator')
                and uint(cleanup.get('age_ns')) and cleanup.get('queue_depth_after') == 0
                and cleanup.get('final_successful_sends') == cleanup.get('final_dequeued') == 11
                and cleanup.get('overflow') == cleanup.get('abandoned') == 0, 'untimed B finalization')
    else:
        require(cleanup is None, 'unexpected cleanup')
    urgent = sample.get('urgent_receipts')
    if quiet:
        require(urgent is None and sample.get('producer_ack_elapsed_ns') is None, 'quiet null receipts')
    else:
        exact_keys(urgent, URGENT_KEYS, 'urgent')
        require(all(uint(urgent.get(k)) for k in ('screen_draw_intent_id','screen_draw_activity_epoch','radial_invocation_id')), 'typed urgent identity')
        require(type(urgent) is dict and urgent.get('screen_draw_stale_intent_receipts') == 1
                and urgent.get('screen_draw_bridge_active_after_admission') is False
                and urgent.get('radial_lease_registered') is False and urgent.get('radial_consumed') is False,
                'urgent stale/invalid receipt')
        for k in ('radial_rejected_before_resolution','query_unchanged','last_query_unchanged',
                  'history_unchanged','visibility_unchanged','restore_unchanged',
                  'provider_revision_unchanged','catalog_versions_unchanged'):
            require(urgent.get(k) is True, 'urgent invariant '+k)
        require(uint(urgent.get('result_generation_delta'))
                and uint(urgent.get('expected_action_refresh_generation_delta'))
                and urgent['result_generation_delta'] == urgent['expected_action_refresh_generation_delta'] == 0,
                'recipe Actions refresh generation delta')
    if not producer:
        require(sample.get('producer_ack_elapsed_ns') is None, 'nonproducer null acknowledgment')
    if producer:
        require(uint(sample.get('producer_ack_elapsed_ns'))
                and sample['producer_ack_elapsed_ns'] <= sample['owner_elapsed_ns'], 'producer acknowledgment clock')
    return sample


def validate(records, source, mode):
    require(source == 'UNCOMMITTED_SOURCE' or re.fullmatch('[0-9a-f]{40}', source), 'source')
    require(mode in ('Small','Full') and (mode != 'Full' or source != 'UNCOMMITTED_SOURCE'), 'Full source')
    count = 200 if mode == 'Small' else 20000
    require(len(records) == 3, 'exact three recipe records')
    seen = set()
    for record in records:
        exact_keys(record, TOP_KEYS, 'record')
        require(all(uint(record.get(k)) for k in ('schema','count','warmups','samples','exclusions','recorder_capacity','max_frames')), 'typed record metadata')
        case = record.get('case')
        require(case in CASES and case not in seen, 'recipe key')
        seen.add(case)
        require(record.get('schema') == 1 and record.get('source') == source
                and record.get('profile') == 'fast-dev' and record.get('mode') == mode
                and record.get('telemetry_enabled') is True, 'provenance')
        require(record.get('stage') == 'gui_event_frame_characterization'
                and record.get('owner') == 'LauncherApp::process_watch_events+LauncherApp::render_root_frame'
                and record.get('measurement_scope') == 'trigger_enqueue_or_start_gate+event_drain+egui_Context_run+FullOutput_drop'
                and record.get('fixture') == 'synthetic_owned_actions_watch_queue', 'measurement domain')
        require(record.get('count') == count and record.get('fixture_signature') == fixture_signature(count, case), 'source-derived fixture binding')
        static_scopes = {
            'setup_root_frame_scope': 'quiet_and_replenishing_use_one_actual_root_frame_outside_owner; finite_keeps_original_queue_for_measured_frame',
            'completion_scope': 'post_owner_state_observation; natural_debounce_only; no_forced_rebuild',
            'sequence_elapsed_scope': 'whole_controlled_sequence_including_trigger_setup_gate_frame_and_full_output_drop',
            'age_scope': 'event_weighted_individual_dequeues; finite includes setup-aged original queue; quiet has no age quantiles',
            'frame_scope': 'actual ctx.run LauncherApp::render_root_frame through FullOutput drop; CPU only',
            'registry_scope': 'registry queued is wake bookkeeping, distinct from app channel depth',
            'depth_scope': 'reserved app queue depth sampled at boundaries, not exact concurrent peak',
            'finite_finalization_scope': 'blocked index B IndexReady is measured only as a separate untimed finalization receipt',
            'original_setup_scope': ('original eight event sequence accepted before owner; ages retain original timestamps'
                if case == CASES[1] else 'one fixture Actions event drained outside owner; recorder reset only after zero depth')}
        require(all(record[k] == value for k,value in static_scopes.items()), 'static scope vocabulary')
        require((record.get('warmups'),record.get('samples'),record.get('exclusions')) == (5,20,0)
                and record.get('recorder_capacity') == 32 and record.get('max_frames') == 8, 'sampling/capacity')
        samples = record.get('sample_records')
        require(type(samples) is list and len(samples) == 20, 'sample inventory')
        for i, sample in enumerate(samples):
            validate_sample(sample, case, i, count)
        for identity in ('action_output_id',):
            require(len({s[identity] for s in samples}) == 1, 'per-recipe stable identity '+identity)
        elapsed = [s['owner_elapsed_ns'] for s in samples]
        for key, value in quantiles(elapsed).items():
            actual = record.get('owner_elapsed_'+key)
            require(uint(actual) and actual == value, 'owner nearest rank '+key)
        for key in ('frame_elapsed_ns_sum','frame_elapsed_ns_max'):
            require(record.get(key) == [s[key] for s in samples], 'frame sample arrays')
        pooled = [a['age_ns'] for s in samples for a in s['event_ages']]
        distribution = record.get('event_age_distribution', {})
        exact_keys(distribution, ['scope','weighting','events','p50_ns','p95_ns','max_ns','mean_ns'], 'pooled distribution')
        require(distribution['scope'] == ('NOT_APPLICABLE_zero_events' if not pooled else 'owner_dequeued_events_including_setup_aged_envelopes_when_present')
                and distribution['weighting'] == ('event_weighted' if not pooled else 'event_weighted_pooled_across_retained_samples'), 'age population weighting')
        require(uint(distribution.get('events')) and distribution['events'] == len(pooled), 'pooled event count')
        for key, value in quantiles(pooled).items():
            actual = distribution.get(key)
            require(actual is None if value is None else uint(actual) and actual == value, 'event-weighted nearest rank '+key)
    summaries = []
    for record in records:
        samples = record['sample_records']
        summaries.append(dict(case=record['case'], count=record['count'], distributions={
            key: quantiles([sample[key] for sample in samples]) for key in (
                'owner_elapsed_ns','event_drain_ns','frame_elapsed_ns_sum','frame_elapsed_ns_max')},
            burst_age_mean=quantiles([s['age_mean_ns'] for s in samples]) if EVENTS[record['case']] else quantiles([]),
            burst_age_max=quantiles([s['age_max_ns'] for s in samples]) if EVENTS[record['case']] else quantiles([])))
    return dict(schema=1, summaries=summaries, status='VALIDATED_INDEPENDENT_REVIEW_PENDING', source=source,
                profile='fast-dev', mode=mode, records=records,
                limits='CPU real egui frames; finite offered load; no native input/composition or sustained-starvation proof')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--log', type=Path, required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--mode', choices=('Small','Full'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    require(not args.output.exists(), 'refuse overwrite')
    records = []
    for line in args.log.read_text(encoding='utf-8-sig').splitlines():
        if MARKER in line:
            records.append(strict_json(line.split(MARKER,1)[1]))
    result = validate(records, args.source, args.mode)
    args.output.write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')


if __name__ == '__main__':
    main()
