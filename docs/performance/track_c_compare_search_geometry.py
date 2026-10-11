"""Validate the bounded C2-H original-owner subset against immutable Track C G0.

Parsing raw owner output remains the original PowerShell collector's responsibility.
This companion enforces exact inventory, complete identities and actual work before
reporting all timing ratios. Small versus frozen Full is context only.
"""
import argparse
import copy
import json
import re
from pathlib import Path

SEARCH = 'LauncherApp::search_read_only_outcome_with_plugin_snapshot'
QUERY = 'query_event+egui_Context_run+LauncherApp::render_root_frame'
ROOT = 'egui_Context_run+LauncherApp::render_root_frame'
NOTES = 'egui_Context_run+NotesDialog::ui'
FAMILIES = {
    (SEARCH, name) for name in (
        'sparse-exact-alias-tie', 'dense-exact-empty-term',
        'dense-fuzzy-usage-weighted', 'empty-query-browse',
        'sparse-exact-pending-provider')
} | {(QUERY, 'root_query_action_fixture'),
     (ROOT, 'root-list-default-layout'), (ROOT, 'root_grid_action_fixture'),
     (NOTES, 'quick_notes_note_fixture')}
KEYS = ('owner', 'fixture', 'fixture_signature', 'item_count', 'viewport',
        'scale_milli', 'fonts', 'settings', 'cold_type')
ARRAYS = ('elapsed_ns', 'work_units', 'complete_output_state_ids',
          'structural_output_ids', 'revision_receipts', 'work_counters',
          'viewport_receipts')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key')
        result[key] = value
    return result


def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'),
                      object_pairs_hook=unique_pairs)


def uint(value):
    return type(value) is int and 0 <= value <= (1 << 64) - 1


def key(record):
    return tuple(record[name] for name in KEYS)


def index(records):
    result = {}
    for record in records:
        k = key(record)
        require(k not in result, 'duplicate scenario')
        result[k] = record
    return result


def validate_record_types(record):
    for name in ('item_count', 'scale_milli', 'warmups', 'samples', 'exclusions',
                 'p50_ns', 'p95_ns', 'max_ns'):
        require(uint(record[name]), 'numeric field type: ' + name)
    for name in KEYS:
        if name not in ('item_count', 'scale_milli'):
            require(type(record[name]) is str, 'scenario key type: ' + name)
    for name in ARRAYS:
        values = record[name]
        require(type(values) is list and len(values) == 20,
                'array inventory: ' + name)
        if name in ('elapsed_ns', 'work_units'):
            require(all(uint(v) for v in values), 'numeric array type: ' + name)
        else:
            require(all(type(v) is str for v in values), 'string array type: ' + name)
        if name in ('revision_receipts', 'work_counters'):
            require(all(all(p.isdecimal() and uint(int(p)) for p in v.split(':'))
                        for v in values), 'receipt uint type: ' + name)
    if 'width_receipt' in record:
        receipt = record['width_receipt']
        require(type(receipt) is dict and set(receipt) ==
                {'scenario', 'transition', 'actual_scroll_area_width_milli'},
                'width receipt shape')
        values = receipt['actual_scroll_area_width_milli']
        require(type(values) is list and len(values) == 20 and
                all(uint(v) for v in values), 'width uint type')


def notes_complete(structural, viewport, revisions, painted):
    value = 0xcbf29ce484222325
    data = b'track-c-quick-notes-output-state'
    for number in (0x435F515549434B4F, painted, int(structural, 16),
                   int(viewport, 16), *revisions):
        data += number.to_bytes(8, 'little')
    for byte in data:
        value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return f'{value:016x}'


def validate(data, baseline, source, mode):
    require(re.fullmatch('[0-9a-f]{40}', source) is not None, 'committed source')
    require(baseline['status'] == 'FROZEN_G0' and
            baseline['source'] == '071243db068835617a054a74fb5734a6dd9f128d',
            'frozen baseline provenance')
    require(type(data['schema']) is int and data['schema'] == 1 and data['source'] == source and
            data['profile'] == 'fast-dev' and data['mode'] == mode and
            data['status'] == 'COLLECTED_PENDING_REVIEW', 'collection provenance')
    frozen = [r for r in baseline['records']
              if (r['owner'], r['fixture']) in FAMILIES and
              (mode == 'Full' or r['item_count'] == 100)]
    expected_count = 96 if mode == 'Full' else 32
    require(len(frozen) == expected_count, 'baseline subset count')
    require(type(data['records']) is list, 'record array type')
    for record in data['records']:
        validate_record_types(record)
    old, new = index(frozen), index(data['records'])
    require(old.keys() == new.keys(), 'exact subset inventory')
    rows = []
    width_count = 0
    for k, previous in old.items():
        current = new[k]
        require(current['source'] == source and current['profile'] == 'fast-dev'
                and current['mode'] == mode and current['telemetry_enabled'] is True,
                'record provenance')
        require((current['warmups'], current['samples'], current['exclusions']) ==
                (5, 20, 0), 'sample protocol')
        for name in ARRAYS:
            require(len(current[name]) == 20, 'array inventory: ' + name)
        elapsed = current['elapsed_ns']
        require(all(uint(v) for v in elapsed), 'elapsed values')
        ordered = sorted(elapsed)
        require([current[n] for n in ('p50_ns', 'p95_ns', 'max_ns')] ==
                [ordered[9], ordered[18], ordered[19]], 'nearest-rank quantiles')
        for name in ('structural_output_ids', 'viewport_receipts', 'work_units',
                     'work_counters'):
            require(current[name] == previous[name], 'output/work parity: ' + name)
        if current['owner'] != NOTES:
            for name in ('complete_output_state_ids', 'revision_receipts'):
                require(current[name] == previous[name], 'complete parity: ' + name)
        else:
            revisions = []
            for sample in range(20):
                parts = current['revision_receipts'][sample].split(':')
                require(len(parts) == 4 and all(p.isdecimal() for p in parts),
                        'Notes revision tuple')
                receipt = [int(p) for p in parts]
                require(all(uint(v) for v in receipt) and receipt[1:] == [1, 1, 1],
                        'Notes generation suffix')
                revisions.append(receipt[0])
                painted = int(current['work_counters'][sample].split(':')[2])
                expected = notes_complete(current['structural_output_ids'][sample],
                                          current['viewport_receipts'][sample],
                                          receipt, painted)
                require(current['complete_output_state_ids'][sample] == expected,
                        'Notes complete identity with actual revisions')
            require(all(b - a == 2 for a, b in zip(revisions, revisions[1:])),
                    'Notes retained publication progression')
        if 'width_receipt' in previous:
            width_count += 1
            require(current.get('width_receipt') == previous['width_receipt'],
                    'exact horizontal width receipt')
        else:
            require('width_receipt' not in current, 'unexpected width receipt')
        row = {name: current[name] for name in KEYS}
        for metric in ('p50_ns', 'p95_ns', 'max_ns'):
            require(previous[metric] > 0, 'positive baseline timing')
            row['g0_' + metric] = previous[metric]
            row['g1_' + metric] = current[metric]
            row[metric + '_ratio'] = current[metric] / previous[metric]
        rows.append(row)
    require(width_count == (6 if mode == 'Full' else 2), 'width inventory')
    result = copy.deepcopy(data)
    result['status'] = 'SUBSET_PARITY_PASS_INDEPENDENT_REVIEW_PENDING'
    result['comparison'] = {
        'g0_source': baseline['source'], 'g0_mode': 'Full',
        'scope': 'matched_full_CPU_owners' if mode == 'Full' else
                 'Small_vs_frozenFull_context_only_not_controlled_ratio',
        'fixture_difference': 'owned_root_dashboard_All_suppressed_for_safety',
        'notes_revision_rule': 'actual_publication_revision_rehashed_generations1:1:1',
        'geometry_pairs': 39 if mode == 'Full' else 13,
        'width_receipts': width_count,
        'claims': 'no_native_GPU_allocator_or_applicationwide_benefit',
        'rows': rows,
    }
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', required=True)
    parser.add_argument('--baseline', default='docs/performance/track_c_baseline.json')
    parser.add_argument('--source', required=True)
    parser.add_argument('--mode', choices=('Small', 'Full'), required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    result = validate(load(args.input), load(args.baseline), args.source, args.mode)
    with Path(args.output).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False)
        stream.write('\n')
    print(f'PASS {len(result["records"])} exact matched scenarios; review pending')
