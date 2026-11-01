"""One label-integrity correction; reuse all saved predictions without ranking."""
from __future__ import annotations

import base64
import hashlib
import json
import sys

from evaluate import observe, summarize
from reference import Reference
from runtime_io import Budget, EVALUATION, GOAL_ROOT, canonical_bytes, environment, load_frozen, sha, write_json


REASON = 'Q122 data viewer label contradicts already-frozen reserved data head; measurement-only v2 overlay'
OVERLAY = EVALUATION / 'deviations' / '001-q122-data-head'
ORIGINAL_FILES = ('reference.py', 'runtime_io.py', 'evaluate.py', 'smoke.py', 'README.md')


def prediction_hash(result: dict) -> str:
    data = json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(',', ':'), allow_nan=False).encode('utf-8')
    return hashlib.sha256(data).hexdigest()


def load_overlay(inputs: dict) -> tuple[list[dict], dict, dict]:
    manifest = json.loads((OVERLAY / 'amendment_manifest.json').read_text(encoding='utf-8'))
    if manifest['parent_manifest_sha256'] != inputs['manifest_canonical_sha256']:
        raise AssertionError('Wrong original freeze')
    if manifest['inherited_v1_input_hashes'] != inputs['input_hashes']:
        raise AssertionError('Inherited v1 hash mapping differs')
    checked = {}
    for entry in manifest['files']:
        path = (OVERLAY / entry['path']).resolve()
        if not path.is_relative_to(OVERLAY.resolve()):
            raise ValueError('Overlay read outside amendment')
        raw = canonical_bytes(path.read_bytes())
        if hashlib.sha256(raw).hexdigest() != entry['sha256'] or len(raw) != entry['canonical_utf8_bytes']:
            raise AssertionError('Amendment drift: ' + entry['path'])
        checked[entry['path']] = entry['sha256']
    cases = [json.loads(line) for line in (OVERLAY / 'queries-v2.jsonl').read_text(encoding='utf-8').splitlines()]
    counts = json.loads((OVERLAY / 'counts-v2.json').read_text(encoding='utf-8'))
    if len(cases) != 300 or [c['id'] for c in cases] != [c['id'] for c in inputs['cases']]:
        raise AssertionError('Amended identity/count mismatch')
    changed = []
    for old, new in zip(inputs['cases'], cases):
        if old != new:
            changed.append(old['id'])
            for key in ('id', 'query', 'context', 'split', 'split_group', 'family'):
                if old[key] != new[key]:
                    raise AssertionError('Nonlabel field changed: ' + key)
    if changed != ['Q122']:
        raise AssertionError('Correction must change Q122 only')
    target = next(c for c in cases if c['id'] == 'Q122')
    if target['expected']['admission'] != 'ProtectedSyntax' or target['expected']['relevance']:
        raise AssertionError('Invalid corrected label')
    return cases, counts, dict(amendment_manifest_sha256=sha((OVERLAY / 'amendment_manifest.json').read_bytes()),
                               overlay_hashes=checked)


def source_receipt(inputs: dict, budget: Budget) -> dict:
    snapshots = {}
    for name in ORIGINAL_FILES:
        path = GOAL_ROOT / 'prototype' / name
        raw = path.read_bytes()
        snapshots[name] = dict(raw_sha256=hashlib.sha256(raw).hexdigest(), canonical_sha256=sha(raw),
                               raw_bytes=len(raw), raw_base64=base64.b64encode(raw).decode('ascii'))
    initial_receipts = {}
    for attempt in ('smoke-initial', 'P0-initial', 'P1-initial', 'P2-initial'):
        path = EVALUATION / 'results' / attempt / 'receipt.json'
        receipt = json.loads(path.read_text(encoding='utf-8'))
        if receipt['status'] != 'COMPLETED' or not receipt['freeze_verified']:
            raise AssertionError('Initial attempt not completed/frozen')
        if receipt['input_hashes'] != inputs['input_hashes']:
            raise AssertionError('Initial input identity differs')
        recorded = receipt['environment']['prototype_canonical_hashes']
        if any(recorded[name] != snapshots[name]['canonical_sha256'] for name in ORIGINAL_FILES):
            raise AssertionError('Original prototype identity changed')
        initial_receipts[attempt] = sha(path.read_bytes())
    write_json(budget.directory / 'initial-source-snapshot.json', dict(
        identity='raw-byte-base64 plus utf8-lf-text-v1 hashes',
        limitation='Raw bytes captured from unchanged author-owned sources at correction time; initial receipts recorded canonical identity, not raw byte hashes.',
        initial_receipt_hashes=initial_receipts, files=snapshots))
    return dict(initial_receipt_hashes=initial_receipts,
                original_source_raw_hashes={k: v['raw_sha256'] for k, v in snapshots.items()},
                original_source_canonical_hashes={k: v['canonical_sha256'] for k, v in snapshots.items()})


def remeasure(candidate: str, inputs: dict, cases: list[dict], counts: dict, budget: Budget) -> dict:
    # Constructor validates/builds descriptor metadata only. No Reference.query/score call.
    ref = Reference(inputs['catalog'], inputs['policy'], inputs['routes'], candidate)
    destination = budget.directory / candidate
    destination.mkdir(exist_ok=False)
    report = dict(candidate=candidate, label_version='g02-label-v2-q122', ranking_calls=0,
                  label='Synthetic author-visible labels; P0 surrogate; original v1 predictions unchanged', scopes={})
    originals = {}
    unchanged = 0
    for scope in ('full', 'initial'):
        budget.tick()
        source = EVALUATION / 'results' / (candidate + '-initial') / candidate / ('cases-' + scope + '.jsonl')
        raw = source.read_bytes()
        old_records = [json.loads(line) for line in canonical_bytes(raw).decode('utf-8').splitlines()]
        if len(old_records) != 300:
            raise AssertionError('Missing original records')
        originals[scope] = dict(path=str(source.relative_to(GOAL_ROOT)).replace('\\', '/'), sha256=sha(raw))
        scope_ids = {d.id for d in ref.descriptors if scope == 'full' or d.subset == 'SEARCH_F001_INITIAL'}
        records = []
        with (destination / ('cases-' + scope + '.jsonl')).open('w', encoding='utf-8', newline='\n') as output:
            for case, old in zip(cases, old_records):
                budget.tick()
                if old['case_id'] != case['id']:
                    raise AssertionError('Original case order mismatch')
                result = old['result']
                original_hash = prediction_hash(result)
                observations = observe(case, result, scope_ids, ref, old['repeat_checks'])
                if original_hash != prediction_hash(result):
                    raise AssertionError('Measurement mutated original prediction')
                record = dict(case_id=case['id'], split=case['split'], family=case['family'],
                              split_group=case['split_group'], expected=case['expected'], result=result,
                              repeat_checks=old['repeat_checks'], observations=observations,
                              original_prediction_sha256=original_hash,
                              prediction_identity='canonical-json-result-v1', prediction_unchanged=True)
                output.write(json.dumps(record, ensure_ascii=False, separators=(',', ':'), allow_nan=False) + '\n')
                records.append(record)
                unchanged += 1
        groups = dict(overall=summarize(records),
                      by_split={name: summarize([x for x in records if x['split'] == name])
                                for name in ('development', 'validation', 'holdout')},
                      by_family={name: summarize([x for x in records if x['family'] == name])
                                 for name in sorted({x['family'] for x in records})},
                      by_split_family={split: {family: summarize([x for x in records if x['split'] == split and x['family'] == family])
                                                for family in sorted({x['family'] for x in records})}
                                       for split in ('development', 'validation', 'holdout')})
        overall = groups['overall']
        if overall['cases'] != counts['cases'] or overall['conditional_top1']['denominator'] != counts['conditional_positives']:
            raise AssertionError('Amended denominator mismatch')
        for split, denominator in counts['positive_by_split'].items():
            if groups['by_split'][split]['conditional_top1']['denominator'] != denominator:
                raise AssertionError('Amended split denominator mismatch')
        if (overall['forbidden_suggestion_case_rate']['denominator'] != counts['forbidden_case_denominator'] or
                overall['desired_abstention']['denominator'] != counts['abstention_denominator'] or
                overall['ambiguity_compliance']['denominator'] != counts['chooser_denominator']):
            raise AssertionError('Amended guard/chooser denominator mismatch')
        if any(v['numerator'] != v['denominator'] for v in overall['hard_invariants'].values()):
            raise AssertionError('Previously passing mechanical invariant changed')
        report['scopes'][scope] = groups
    report.update(original_raw_files=originals, unchanged_predictions=unchanged)
    write_json(destination / 'summary.json', report)
    return report


def main() -> int:
    budget = Budget('evaluate', ['P0', 'P1', 'P2'], 'corrective', REASON)
    receipt = dict(environment=environment(), freeze_verified=False, ranking_calls=0,
                   measurement_only=True, completed_candidates=[], unchanged_predictions=0,
                   correction_cycle_consumed=True)
    status, code = 'COMPLETED', 0
    try:
        inputs = load_frozen()
        cases, counts, overlay = load_overlay(inputs)
        receipt.update(freeze_verified=True, manifest_sha256=inputs['manifest_canonical_sha256'],
                       input_hashes=inputs['input_hashes'], source_sha=inputs['catalog']['source_sha'],
                       policy_revision=inputs['policy']['policy_revision'], **overlay)
        receipt.update(source_receipt(inputs, budget))
        for candidate in ('P0', 'P1', 'P2'):
            report = remeasure(candidate, inputs, cases, counts, budget)
            receipt['unchanged_predictions'] += report['unchanged_predictions']
            receipt['completed_candidates'].append(candidate)
        if receipt['unchanged_predictions'] != 1800:
            raise AssertionError('Incomplete saved-prediction measurement')
        budget.tick()
    except Exception as error:
        receipt['error'] = dict(type=type(error).__name__, message=str(error))
        status, code = ('TIME_LIMIT' if isinstance(error, TimeoutError) else 'FAILED'), 1
    finally:
        budget.finish(status, receipt)
    print(json.dumps(dict(attempt=budget.attempt_id, status=receipt['status'],
                          elapsed_seconds=receipt['elapsed_seconds'], cumulative_seconds=receipt['cumulative_actual_seconds'],
                          ranking_calls=receipt['ranking_calls'], unchanged_predictions=receipt['unchanged_predictions'])))
    return code if receipt['status'] != 'TIME_LIMIT' else 1


if __name__ == '__main__':
    sys.exit(main())
