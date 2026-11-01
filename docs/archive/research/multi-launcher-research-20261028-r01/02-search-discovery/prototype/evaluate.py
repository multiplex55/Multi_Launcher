"""One bounded invocation per declared policy; synthetic relevance only."""
from __future__ import annotations

import argparse
from collections import Counter
import json
import math
import sys

from reference import Reference, owner_reasons, signature
from runtime_io import Budget, environment, load_frozen, write_json


def ratio(numerator: int | float, denominator: int) -> dict:
    return dict(numerator=numerator, denominator=denominator,
                value=numerator / denominator if denominator else None)


def observe(case: dict, result: dict, scope_ids: set[str], reference: Reference, checks: dict) -> dict:
    expected = case['expected']
    relevance = expected['relevance']
    acceptable = {eid for eid, grade in relevance.items() if grade >= 2}
    rows = result['rows']
    emitted = [row['id'] for row in rows]
    positive = expected['admission'] == 'ADMIT' and bool(acceptable)
    first = bool(emitted) and emitted[0] in acceptable
    any_hit = bool(acceptable.intersection(emitted))
    rank = next((i for i, eid in enumerate(emitted, 1) if eid in acceptable), None)
    ideal = sorted(relevance, key=lambda eid: (-relevance[eid], eid))[:5]
    def dcg(eids):
        return sum((2 ** relevance.get(eid, 0) - 1) / math.log2(i + 1)
                   for i, eid in enumerate(eids, 1))
    ideal_dcg = dcg(ideal)
    ineligible = {d.id for d in reference.descriptors if owner_reasons(d, case['context'])}
    eligibility_case = (not case['context']['all_owners_enabled'] or
                        any(d.id in ineligible and d.owner in case['context']['owner_overrides']
                            for d in reference.descriptors))
    expected_chooser = expected['behavior'] == 'CHOOSER'
    accepted_keys = {(row['action']['action'], row['action']['args'])
                     for row in rows if row['id'] in acceptable}
    chooser_pass = (result['state'] == 'CHOOSER_EXPLICIT_SELECTION' and len(accepted_keys) >= 2)
    source = {d.id: d for d in reference.descriptors}
    payload_pass = all(row['id'] in source and row['action'] == source[row['id']].action.record()
                       for row in rows)
    key_pass = len({(row['action']['action'], row['action']['args']) for row in rows}) == len(rows)
    score_pass = all(isinstance(row['score'], int) and 0 <= row['score'] <= 100 for row in rows)
    order_pass = emitted == [row['id'] for row in sorted(rows, key=lambda r: (-r['score'], r['id']))]
    o = dict(
        n=1, gate_equal=int(result['gate'] == expected['admission']),
        gated=int(expected['admission'] != 'ADMIT'),
        gated_preserved=int(expected['admission'] != 'ADMIT' and result['gate'] == expected['admission'] and not rows),
        positive=int(positive), top1=int(positive and first), top5=int(positive and any_hit),
        reciprocal_rank=(1.0 / rank if positive and rank else 0.0),
        ndcg=(dcg(emitted[:5]) / ideal_dcg if positive and ideal_dcg else 0.0),
        forbidden_case=int(bool(expected['forbidden_ids'])),
        forbidden_hit=int(bool(set(expected['forbidden_ids']).intersection(emitted))),
        eligibility_case=int(eligibility_case), ineligible_hit=int(bool(ineligible.intersection(emitted))),
        abstention_case=int(expected['behavior'] == 'ABSTAIN'),
        abstention_pass=int(expected['behavior'] == 'ABSTAIN' and not rows),
        chooser_case=int(expected_chooser), chooser_pass=int(expected_chooser and chooser_pass),
        permissive_case=int(expected['behavior'] == 'SUGGEST_OR_CHOOSER'),
        permissive_pass=int(expected['behavior'] == 'SUGGEST_OR_CHOOSER' and first and
                            result['state'] in ('SUGGEST_OPEN_TOOL_NOT_ACTIVATE', 'CHOOSER_EXPLICIT_SELECTION')),
        absent_any=int(positive and bool(acceptable - scope_ids)),
        absent_all=int(positive and not acceptable.intersection(scope_ids)),
        replay_pass=int(checks['identical_replay']), reverse_pass=int(checks['reversed_enumeration']),
        usage_pass=int(checks['zero_vs_heavy_usage']),
        payload_pass=int(payload_pass), unique_payload_pass=int(key_pass),
        integer_score_pass=int(score_pass), stable_order_pass=int(order_pass),
        output_eligibility_pass=int(not ineligible.intersection(emitted)),
        gate_no_rows_pass=int(result['gate'] == 'ADMIT' or not rows),
        topk_pass=int(len(rows) <= 5), binding_case=int(bool(case['context'].get('prior_explicit_selection'))),
        binding_pass=int(bool(case['context'].get('prior_explicit_selection')) and not rows),
        budget_abstention=int(result['state'] == 'ABSTAIN_BUDGET'),
        potential_typo_pairs=result['operations']['potential_typo_pairs'],
        edit_tests=result['operations']['edit_tests'],
    )
    return o


def summarize(items: list[dict]) -> dict:
    s = Counter()
    for item in items:
        s.update(item['observations'])
    n, pos = s['n'], s['positive']
    return dict(
        cases=n,
        gate_agreement=ratio(s['gate_equal'], n),
        synthetic_gated_preservation=ratio(s['gated_preserved'], s['gated']),
        conditional_top1=ratio(s['top1'], pos), conditional_top5=ratio(s['top5'], pos),
        MRR=ratio(s['reciprocal_rank'], pos), nDCG5=ratio(s['ndcg'], pos),
        overall_synthetic_discovery_coverage=ratio(s['top5'], n),
        forbidden_suggestion_case_rate=ratio(s['forbidden_hit'], s['forbidden_case']),
        forbidden_suggestion_all_case_rate=ratio(s['forbidden_hit'], n),
        ineligible_owner_suggestion=ratio(s['ineligible_hit'], s['eligibility_case']),
        desired_abstention=ratio(s['abstention_pass'], s['abstention_case']),
        ambiguity_compliance=ratio(s['chooser_pass'], s['chooser_case']),
        permissive_positive_behavior=ratio(s['permissive_pass'], s['permissive_case']),
        absent_any_acceptable=ratio(s['absent_any'], pos), absent_all_acceptable=ratio(s['absent_all'], pos),
        identical_replay=ratio(s['replay_pass'], n), reversed_enumeration=ratio(s['reverse_pass'], n),
        zero_vs_heavy_usage=ratio(s['usage_pass'], n),
        selection_final_state_fixture=ratio(s['binding_pass'], s['binding_case']),
        hard_invariants={name: ratio(s[name], n) for name in
                         ('payload_pass', 'unique_payload_pass', 'integer_score_pass', 'stable_order_pass',
                          'output_eligibility_pass', 'gate_no_rows_pass', 'topk_pass')},
        operation_counts=dict(potential_typo_pairs=s['potential_typo_pairs'], edit_tests=s['edit_tests'],
                              budget_abstentions=s['budget_abstention']),
        case_ids=[x['case_id'] for x in items],
        failure_ids=dict(
            gate_mismatch=[x['case_id'] for x in items if not x['observations']['gate_equal']],
            forbidden=[x['case_id'] for x in items if x['observations']['forbidden_hit']],
            relevance_top1=[x['case_id'] for x in items if x['observations']['positive'] and not x['observations']['top1']],
            abstention=[x['case_id'] for x in items if x['observations']['abstention_case'] and not x['observations']['abstention_pass']],
            chooser=[x['case_id'] for x in items if x['observations']['chooser_case'] and not x['observations']['chooser_pass']],
        ),
    )


def candidate_run(candidate: str, inputs: dict, budget: Budget) -> dict:
    ref = Reference(inputs['catalog'], inputs['policy'], inputs['routes'], candidate)
    report = dict(candidate=candidate, label='Synthetic author-visible relevance; P0 is a documented-policy surrogate', scopes={})
    destination = budget.directory / candidate
    destination.mkdir(exist_ok=False)
    violations = []
    for scope in ('full', 'initial'):
        budget.tick()
        scope_ids = {d.id for d in ref.descriptors if scope == 'full' or d.subset == 'SEARCH_F001_INITIAL'}
        records = []
        with (destination / ('cases-' + scope + '.jsonl')).open('w', encoding='utf-8', newline='\n') as output:
            for case in inputs['cases']:
                budget.tick()
                context = case['context']
                result = ref.query(case['query'], context, scope, budget.tick)
                replay = ref.query(case['query'], context, scope, budget.tick)
                reverse = ref.query(case['query'], context, scope, budget.tick, reverse=True)
                heavy = ref.query(case['query'], dict(context, synthetic_usage_count=1000000), scope, budget.tick)
                zero = ref.query(case['query'], dict(context, synthetic_usage_count=0), scope, budget.tick)
                checks = dict(identical_replay=signature(result) == signature(replay),
                              reversed_enumeration=signature(result) == signature(reverse),
                              zero_vs_heavy_usage=signature(zero) == signature(heavy) == signature(result))
                observations = observe(case, result, scope_ids, ref, checks)
                record = dict(case_id=case['id'], split=case['split'], family=case['family'],
                              split_group=case['split_group'], expected=case['expected'], result=result,
                              repeat_checks=checks, observations=observations)
                output.write(json.dumps(record, ensure_ascii=False, separators=(',', ':'), allow_nan=False) + '\n')
                records.append(record)
        groups = dict(overall=summarize(records),
                      by_split={name: summarize([x for x in records if x['split'] == name])
                                for name in ('development', 'validation', 'holdout')},
                      by_family={name: summarize([x for x in records if x['family'] == name])
                                 for name in sorted({x['family'] for x in records})},
                      by_split_family={split: {family: summarize([x for x in records if x['split'] == split and x['family'] == family])
                                                for family in sorted({x['family'] for x in records})}
                                       for split in ('development', 'validation', 'holdout')})
        if groups['overall']['cases'] != 300 or groups['overall']['conditional_top1']['denominator'] != 120:
            raise AssertionError('Frozen denominator contract')
        for split, denominator in (('development', 68), ('validation', 34), ('holdout', 18)):
            if groups['by_split'][split]['conditional_top1']['denominator'] != denominator:
                raise AssertionError('Frozen split positive denominator')
        for name, measure in groups['overall']['hard_invariants'].items():
            if measure['numerator'] != measure['denominator']:
                violations.append(scope + ':' + name)
        for name in ('identical_replay', 'reversed_enumeration', 'zero_vs_heavy_usage'):
            measure = groups['overall'][name]
            if measure['numerator'] != measure['denominator']:
                violations.append(scope + ':' + name)
        report['scopes'][scope] = groups
        budget.tick()
    report['implementation_invariant_failures'] = violations
    write_json(destination / 'summary.json', report)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', choices=('P0', 'P1', 'P2'), nargs='+', required=True)
    parser.add_argument('--phase', choices=('initial', 'corrective'), default='initial')
    parser.add_argument('--reason', default='')
    args = parser.parse_args()
    if len(set(args.candidate)) != len(args.candidate):
        parser.error('Duplicate candidates')
    budget = Budget('evaluate', args.candidate, args.phase, args.reason)
    receipt = dict(environment=environment(), freeze_verified=False, completed_candidates=[],
                   evidence_limits=['synthetic labels', 'P0 surrogate', 'Complete hypothetical', 'no native/Rust/human proof'])
    status, code = 'COMPLETED', 0
    try:
        inputs = load_frozen()
        receipt.update(freeze_verified=True, manifest_sha256=inputs['manifest_canonical_sha256'],
                       input_hashes=inputs['input_hashes'], source_sha=inputs['catalog']['source_sha'],
                       policy_revision=inputs['policy']['policy_revision'])
        for candidate in args.candidate:
            report = candidate_run(candidate, inputs, budget)
            receipt['completed_candidates'].append(candidate)
            if report['implementation_invariant_failures']:
                status, code = 'COMPLETED_WITH_INVARIANT_FAILURES', 1
        budget.tick()
    except Exception as error:
        receipt['error'] = dict(type=type(error).__name__, message=str(error))
        status, code = ('TIME_LIMIT' if isinstance(error, TimeoutError) else 'FAILED'), 1
    finally:
        budget.finish(status, receipt)
    print(json.dumps(dict(attempt=budget.attempt_id, status=receipt['status'],
                          elapsed_seconds=receipt['elapsed_seconds'], cumulative_seconds=receipt['cumulative_actual_seconds'],
                          completed=receipt['completed_candidates']), ensure_ascii=False))
    return code if receipt['status'] != 'TIME_LIMIT' else 1


if __name__ == '__main__':
    sys.exit(main())
