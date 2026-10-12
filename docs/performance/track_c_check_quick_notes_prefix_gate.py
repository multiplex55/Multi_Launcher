import argparse,json,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--candidate',required=True);p.add_argument('--source',required=True);p.add_argument('--output',required=True);a=p.parse_args()
out=Path(a.output)
if out.exists():raise SystemExit('Refuse existing gate output')
gpath=Path('docs/performance/track_c_quick_notes_prefix_gate.json');g=json.loads(gpath.read_text(encoding='utf-8-sig'));bpath=Path('docs/performance/track_c_quick_notes_characterization_baseline.json');assert hashlib.sha256(bpath.read_bytes()).hexdigest()==g['baseline_sha256'],'Frozen baseline hash';b=json.loads(bpath.read_text());c=json.loads(Path(a.candidate).read_text(encoding='utf-8-sig'));assert c['schema']==2 and c['status']=='C2_F_PREFIX_CANDIDATE_COLLECTED_PENDING_REVIEW' and c['source']==a.source and c['profile']=='fast-dev' and c['mode']=='Full' and len(c['records'])==30
key=lambda r:f"{r['recipe']}:{r['count']}:{r['screen_height_milli']}"
br={key(r):r for r in b['records']};cr={key(r):r for r in c['records']};assert len(cr)==30 and cr.keys()==br.keys()
identities=['fixture_ids','canonical_ids','search_index_ids','metadata_ids','projection_ids','geometry_ids','viewport_ids','complete_output_ids'];state=['settings_receipts','viewport_receipts','width_receipts','visible_original_indices'];work=['snapshot_calls','snapshot_successes','snapshot_errors','snapshot_note_entries','snapshot_estimated_clone_bytes','search_index_build_calls','search_index_entries','search_index_utf8_bytes','metadata_build_calls','metadata_entries','metadata_output_utf8_bytes','metadata_backlink_queries','metadata_checkbox_lines_visited','metadata_preview_builds','projection_calls','projection_entries_scanned','projection_indices_materialized','projection_selected_indices','geometry_ensure_calls','geometry_rebuilds','geometry_identity_entries','painted_rows']
checks=[]
for k,r in cr.items():
 base=br[k]
 for name in identities+state+work:
  assert r[name]==base[name],f'Complete semantic/state/work parity {k} {name}'
 for name in ['capacity_before','capacity_after','geometry_overlap']:
  # Preserve every observed value for review; storage equivalence is not assumed.
  assert len(r[name])==20
 for q in ['p50_ns','p95_ns','max_ns']:
  assert r['statistics']['elapsed_ns'][q]==sorted(r['elapsed_ns'])[{ 'p50_ns':9,'p95_ns':18,'max_ns':19}[q]],'Recomputed owner quantile'
for item in g['gates']:
 r=cr[item['key']];actual=r['statistics']['elapsed_ns'][item['quantile']];baseline=br[item['key']]['statistics']['elapsed_ns'][item['quantile']];assert baseline==item['baseline_ns'];passed=actual*100<=baseline*item['candidate_percent_limit'];checks.append(dict(**item,candidate_ns=actual,passed=passed,change_percent=(actual/baseline-1)*100))
assert len(checks)==32
result=dict(schema=1,status='ALL_NUMERIC_GATES_PASS_PENDING_INDEPENDENT_REVIEW' if all(x['passed'] for x in checks) else 'NUMERIC_GATE_FAILED_REJECT_RUNTIME_CANDIDATE',baseline_source=b['source'],candidate_source=c['source'],profile='fast-dev',records=30,samples=600,numeric_pass=sum(x['passed'] for x in checks),numeric_total=32,semantic_identity_hashes_checked=4800,matched_nontiming_arrays=30*len(identities+state+work),runtime_candidate_accepted=False,native_display='NOT MEASURED',allocator_and_process_ram='NOT MEASURED',storage_review_required=True,gates=checks)
out.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(result['status'],str(result['numeric_pass'])+'/32')
