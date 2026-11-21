"""Small declared-invariant smoke, not corpus relevance evaluation."""
from __future__ import annotations

import argparse
from copy import deepcopy
import json
import sys

from reference import ProjectionError, Reference, collapse, one_edit, prepare, signature
from runtime_io import Budget, environment, load_frozen, write_json


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--phase', choices=('initial', 'corrective'), default='initial')
    parser.add_argument('--reason', default='')
    args = parser.parse_args()
    budget = Budget('smoke', [], args.phase, args.reason)
    receipt = dict(environment=environment(), freeze_verified=False, assertions=[])
    status, code = 'COMPLETED', 0
    try:
        inputs = load_frozen()
        receipt.update(freeze_verified=True, manifest_sha256=inputs['manifest_canonical_sha256'], input_hashes=inputs['input_hashes'])
        refs = {p: Reference(inputs['catalog'], inputs['policy'], inputs['routes'], p) for p in ('P0', 'P1', 'P2')}
        context = deepcopy(inputs['cases'][0]['context'])
        def check(name, value):
            budget.tick()
            if not value:
                raise AssertionError(name)
            receipt['assertions'].append(name)
        def run(q, policy='P2', ctx=None):
            return refs[policy].query(q, ctx or context, 'full', budget.tick)
        def hash_score(result):
            return next(x['score'] for x in result['scored_candidates'] if x['id'] == 'tool.hash.open')
        check('17 canonical frozen input hashes', len(inputs['input_hashes']) == 17)
        check('disabled first', run('checksum', ctx=dict(context, feature_enabled=False))['gate'] == 'Disabled')
        check('automatic origin excluded', run('checksum', ctx=dict(context, query_origin='Automatic'))['gate'] == 'ExcludedSurfaceOrOrigin')
        check('capacity empty is not no-match', run('checksum', ctx=dict(context, provider_deferral='Capacity'))['gate'] == 'BusyOrStale')
        check('Unknown blocks even a relevant tool', run('checksum', ctx=dict(context, reserved_route_coverage='Unknown'))['gate'] == 'CoverageUnknown')
        check('explicit g remains protected', run('G checksum')['gate'] == 'ProtectedSyntax')
        check('short declared head precedes length bound', run('qr')['gate'] == 'ProtectedSyntax')
        check('quoted text not normalized', run('"checksum"')['gate'] == 'ProtectedSyntax')
        check('finite destructive abstention', run('delete duplicate files')['gate'] == 'UnsupportedIntent')
        check('raw repetition bound', run('checksum ' * 25)['gate'] == 'InputBound')
        check('no eligible owner', run('checksum', ctx=dict(context, all_owners_enabled=False))['gate'] == 'NoEligibleOwner')
        denied = dict(context, owner_overrides={'hash_generator': {'capability_restriction': []}})
        check('denied owner removed before score', all(x['id'] != 'tool.hash.open' for x in run('checksum', ctx=denied)['scored_candidates']))
        check('P1 fixed alias arithmetic100', hash_score(run('open checksum tool', 'P1')) == 100)
        check('P2 contextual typo arithmetic76', hash_score(run('file cheksum')) == 76)
        check('P2 weak typo63 abstains', hash_score(run('cheksum')) == 63 and run('cheksum')['state'] == 'ABSTAIN_LOW_EVIDENCE')
        check('last-token prefix81', hash_score(run('file checks')) == 81)
        check('non-final prefix not extended', hash_score(run('checks file')) == 37)
        check('repetition does not amplify', signature(run('checksum checksum', 'P1')) == signature(run('checksum', 'P1')))
        compare = run('compare', 'P1')
        check('stable ambiguous IDs', compare['state'] == 'CHOOSER_EXPLICIT_SELECTION' and
              [x['id'] for x in compare['rows']] == ['tool.diff.open', 'tool.image_compare.open'])
        check('reverse enumeration stable', signature(compare) == signature(refs['P1'].query('compare', context, 'full', budget.tick, reverse=True)))
        check('heavy synthetic usage ignored', signature(compare) == signature(run('compare', 'P1', dict(context, synthetic_usage_count=1000000))))
        row = run('checksum', 'P1')['rows'][0]
        check('source fixed payload retained', row['action']['action'] == 'hash:open' and row['action']['args'] is None)
        action = dict(label='Synthetic', desc='Smoke identity only', action='fixed:synthetic', args=None)
        same = [dict(id='a', score=80, action=action), dict(id='b', score=80, action=dict(action)),
                dict(id='c', score=80, action=dict(action, args=''))]
        check('None differs from Some empty; stable representative', [x['id'] for x in collapse(same)] == ['a', 'c'])
        check('one-edit operations and two-edit rejection', all(one_edit(a, b) for a, b in
              [('cheksum', 'checksum'), ('abcde', 'abxde'), ('abcde', 'acbde'), ('abcde', 'abcd')]) and
              not one_edit('abcde', 'axcye') and not one_edit('abcde', 'abcde'))
        oversized = dict(inputs['catalog'], entries=inputs['catalog']['entries'] * 5)
        try:
            prepare(oversized, inputs['policy'], 'P0')
            raise AssertionError('Oversized projection was accepted')
        except ProjectionError:
            receipt['assertions'].append('whole projection hard bound')
        # Tiny synthetic cap fixture exercises whole-query refusal; no relevance
        # coefficient/input file is edited and this is not a fourth candidate.
        cap_policy = deepcopy(inputs['policy'])
        cap_policy['bounds']['max_typo_pairs'] = 0
        cap = Reference(inputs['catalog'], cap_policy, inputs['routes'], 'P2').query('file cheksum', context, 'full', budget.tick)
        check('budget refuses whole query', cap['state'] == 'ABSTAIN_BUDGET' and not cap['rows'] and not cap['scored_candidates'])
        budget.tick()
    except Exception as error:
        receipt['error'] = dict(type=type(error).__name__, message=str(error))
        status, code = ('TIME_LIMIT' if isinstance(error, TimeoutError) else 'FAILED'), 1
    finally:
        budget.finish(status, receipt)
    print(json.dumps(dict(attempt=budget.attempt_id, status=receipt['status'],
                          assertions=len(receipt['assertions']), elapsed_seconds=receipt['elapsed_seconds'],
                          cumulative_seconds=receipt['cumulative_actual_seconds'])))
    return code if receipt['status'] != 'TIME_LIMIT' else 1


if __name__ == '__main__':
    sys.exit(main())
