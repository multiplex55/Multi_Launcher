"""Collect six legacy Track A warm owners with actual Track C runner provenance.

The legacy emitter labels fast-dev as debug-test. Preserve that label verbatim;
this wrapper records the actual profile and never computes historical timing ratios.
Legacy summaries contain quantiles, not raw elapsed samples.
"""
import argparse
import json
import re
from pathlib import Path


def require(condition, message):
    if not condition:
        raise ValueError(message)


def uint(value):
    return type(value) is int and 0 <= value < 1 << 64


def unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key')
        result[key] = value
    return result


def collect(text, historical, source):
    require(re.fullmatch('[0-9a-f]{40}', source) is not None, 'committed source')
    expected = {f'launcher-{n}-{mode}' for n in (100, 1000, 10000)
                for mode in ('list', 'grid-3-column')}
    old = {r['scenario']: r for r in historical['scenarios']
           if r['scenario'] in expected}
    require(old.keys() == expected, 'historical signature inventory')
    records = {}
    for line in text.splitlines():
        if 'TRACK_A_WORKLOAD ' not in line:
            continue
        payload = line.split('TRACK_A_WORKLOAD ', 1)[1]
        record = json.loads(payload, object_pairs_hook=unique_pairs)
        name = record['scenario']
        require(name in expected, 'unexpected scenario')
        require(set(record) == set(old[name]), 'legacy summary field inventory')
        if name in records:
            require(records[name][0] == payload, 'nonidentical duplicate')
            continue
        require(all(uint(record[k]) for k in
                    ('warmups', 'samples', 'fixture_count', 'estimated_fixture_bytes')),
                'numeric field type')
        require(record['warmups'] == 5 and record['samples'] == 20,
                'legacy sample protocol')
        require(record['profile'] == 'debug-test' and record['os'] == 'windows'
                and record['scope'] ==
                'production render_root_frame(None); headless egui debug-test CPU',
                'legacy label changed')
        for field in ('fixture_count', 'fixture_signature', 'estimated_fixture_bytes',
                      'output_signature', 'output_order_signature'):
            require(record[field] == old[name][field], 'signature parity: ' + field)
        times = [record[k] for k in ('p50_nanos', 'p95_nanos', 'max_nanos')]
        require(all(type(v) is int and 0 < v < 1 << 64 for v in times)
                and times == sorted(times), 'timing quantiles')
        grid = name.endswith('grid-3-column')
        geometry_key = 'root_grid_geometry' if grid else 'root_list_geometry'
        unused = 'root_list_geometry' if grid else 'root_grid_geometry'
        geometry = record[geometry_key]
        require(type(geometry) is dict and set(geometry) == set(old[name][geometry_key]),
                'geometry field inventory')
        require(all(uint(v) for v in geometry.values()),
                'geometry uint type')
        require(record[unused] is None and record['notes_geometry'] is None,
                'geometry inventory')
        require(geometry['warm_rebuild_count'] == 0 and
                geometry['warm_cells_measured' if grid else 'warm_rows_measured'] == 0,
                'warm geometry work')
        require(geometry['cold_rebuild_count'] > 0 and
                geometry['cold_cells_measured' if grid else 'cold_rows_measured']
                >= record['fixture_count'], 'cold geometry observation')
        require(len(record['metrics']) == len(old[name]['metrics']) == 1,
                'legacy metric inventory')
        metric = record['metrics'][0]
        require(set(metric) == set(old[name]['metrics'][0]), 'metric field inventory')
        require(all(uint(v) for k, v in metric.items()
                    if k not in ('metric', 'work_unit')), 'metric uint type')
        for prefix in ('elapsed', 'lock_wait'):
            require(metric[prefix + '_nanos_max'] <= metric[prefix + '_nanos_total'],
                    'metric maximum within total')
        for field in ('metric', 'work_unit', 'calls', 'work_units',
                      'errors', 'abandoned', 'completed'):
            require(metric[field] == old[name]['metrics'][0][field],
                    'legacy warm work parity: ' + field)
        records[name] = (payload, record)
    require(records.keys() == expected, 'six-scenario inventory')
    return {
        'schema': 1, 'status': 'PASS_PENDING_INDEPENDENT_REVIEW',
        'source': source, 'actual_profile': 'fast-dev', 'mode': 'Full',
        'legacy_declared_profile': 'debug-test',
        'legacy_scope_label': 'preserved; actual profile supplied by frozen runner',
        'historical_timing_comparison': 'NOT_COMPARABLE_debug_test_vs_fast_dev',
        'quantile_evidence': 'legacy_aggregate_only_no_raw_twenty_elapsed_array',
        'cold_evidence': 'single_last_rebuild_observation_not_cold_quantiles',
        'fixture_difference': 'owned_dashboard_initial_refresh_suppressed',
        'native_display': 'NOT_MEASURED', 'exclusions': 0,
        'records': [records[name][1] for name in sorted(expected)],
    }


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--log', required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    historical = json.loads(Path('docs/performance/track_a_final.json').read_text())
    result = collect(Path(args.log).read_text(encoding='utf-8-sig'),
                     historical, args.source)
    with Path(args.output).open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False)
        stream.write('\n')
    print('PASS six warm summaries; historical timing ratios unavailable')
