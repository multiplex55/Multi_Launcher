"""Research-input hashing and bounded research-output receipts; no subprocesses."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys
import time


GOAL_ROOT = Path(__file__).resolve().parent.parent
EVALUATION = GOAL_ROOT / 'evaluation'
LEDGER_PATH = EVALUATION / 'EXECUTION_BUDGET.json'


def canonical_bytes(raw: bytes) -> bytes:
    text = raw.decode('utf-8')
    if text.startswith('\ufeff'):
        raise ValueError('BOM forbidden by utf8-lf-text-v1')
    text = text.replace('\r\n', '\n')
    if '\r' in text:
        raise ValueError('Lone physical CR forbidden')
    return text.encode('utf-8')


def sha(raw: bytes) -> str:
    return hashlib.sha256(canonical_bytes(raw)).hexdigest()


def write_json(path: Path, value: dict) -> None:
    destination = path.resolve()
    if destination != LEDGER_PATH.resolve() and not destination.is_relative_to((EVALUATION / 'results').resolve()):
        raise ValueError('Write outside research evaluation outputs')
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + '\n'
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(text, encoding='utf-8', newline='\n')
    temporary.replace(path)


def load_frozen() -> dict:
    manifest_path = EVALUATION / 'freeze_manifest.json'
    manifest_raw = manifest_path.read_bytes()
    manifest = json.loads(canonical_bytes(manifest_raw))
    verified = {}
    contents = {}
    for entry in manifest['files']:
        path = (GOAL_ROOT / entry['path']).resolve()
        if not path.is_relative_to(GOAL_ROOT.resolve()):
            raise ValueError('Frozen read outside goal root')
        data = canonical_bytes(path.read_bytes())
        if (hashlib.sha256(data).hexdigest() != entry['sha256'] or
                len(data) != entry['canonical_utf8_bytes'] or
                data.count(b'\n') != entry['canonical_line_count']):
            raise ValueError('Frozen input drift: ' + entry['path'])
        contents[entry['path']] = data
        verified[entry['path']] = entry['sha256']
    def read_json(relative: str):
        return json.loads(contents[relative])
    catalog = read_json('catalog/catalog.json')
    policy = read_json('evaluation/candidate_policies.json')
    cases = [json.loads(line) for line in contents['evaluation/queries.jsonl'].decode('utf-8').splitlines()]
    splits = read_json('evaluation/splits.json')
    if len(cases) != 300 or {x['id'] for x in cases} != {'Q%03d' % n for n in range(1, 301)}:
        raise ValueError('Frozen case identity/count mismatch')
    if catalog['freeze_state'] != policy['freeze_state'] or policy['freeze_state'] != 'FROZEN_FOR_EVALUATION_V1':
        raise ValueError('Unfrozen policy/catalog')
    for case in cases:
        if case['id'] not in splits['memberships'][case['split']]:
            raise ValueError('Split mismatch')
    return dict(catalog=catalog, policy=policy, cases=cases, splits=splits,
                routes=read_json('evaluation/route_fixtures.json'),
                manifest=manifest, input_hashes=verified,
                manifest_canonical_sha256=sha(manifest_raw))


def environment() -> dict:
    # Deliberately avoid platform helpers that may invoke subprocesses.
    return dict(python_version=sys.version, sys_platform=sys.platform,
                command_argv=sys.argv, interpreter_flags='-X utf8 -B required',
                prototype_canonical_hashes={p.name: sha(p.read_bytes())
                                           for p in sorted(Path(__file__).parent.iterdir())
                                           if p.suffix == '.py' or p.name == 'README.md'},
                timing='perf_counter from guarded main entry through raw/summary output; interpreter startup and final receipt/ledger writes excluded')


class Budget:
    def __init__(self, kind: str, candidates: list[str], phase: str, reason: str = ''):
        self.started = time.perf_counter()
        self.kind, self.phase, self.candidates = kind, phase, candidates
        self.limit = 15.0 if kind == 'smoke' else 120.0
        self.ledger = (json.loads(LEDGER_PATH.read_text(encoding='utf-8')) if LEDGER_PATH.exists()
                       else dict(schema_version=1, aggregate_limit_seconds=600,
                                 nominal_maximum_seconds=510, correction_cycle=None, attempts=[]))
        attempts = self.ledger['attempts']
        if any(a['status'] == 'RUNNING' for a in attempts):
            raise ValueError('Unresolved previous attempt; preserve evidence before continuation')
        elapsed = sum(a.get('elapsed_seconds', a['limit_seconds']) for a in attempts)
        if elapsed + self.limit > self.ledger['aggregate_limit_seconds']:
            raise ValueError('Aggregate budget cannot reserve this invocation')
        if phase == 'initial':
            if kind == 'evaluate' and len(candidates) != 1:
                raise ValueError('One initial invocation per candidate')
            for old in attempts:
                if old['phase'] == 'initial' and old['kind'] == kind:
                    if kind == 'smoke' or set(candidates).intersection(old['candidates']):
                        raise ValueError('Initial attempt already recorded; no silent rerun')
        else:
            if not reason:
                raise ValueError('Concrete correction reason required')
            cycle = self.ledger['correction_cycle']
            if cycle is None:
                self.ledger['correction_cycle'] = dict(reason=reason, candidates=candidates)
            elif cycle['reason'] != reason:
                raise ValueError('Only one concrete corrective cycle')
            if any(a['phase'] == 'corrective' and a['kind'] == kind for a in attempts):
                raise ValueError('Corrective invocation already recorded')
        self.attempt_id = ('smoke-' if kind == 'smoke' else '-'.join(candidates) + '-') + phase
        self.directory = EVALUATION / 'results' / self.attempt_id
        self.directory.mkdir(parents=True, exist_ok=False)
        self.entry = dict(id=self.attempt_id, kind=kind, phase=phase, candidates=candidates,
                          limit_seconds=self.limit, status='RUNNING', correction_reason=reason,
                          output_directory=str(self.directory.relative_to(GOAL_ROOT)).replace('\\', '/'))
        attempts.append(self.entry)
        write_json(LEDGER_PATH, self.ledger)

    def tick(self) -> None:
        if time.perf_counter() - self.started > self.limit:
            raise TimeoutError('Per-invocation wall-clock limit exceeded')

    def finish(self, status: str, receipt: dict) -> None:
        elapsed = time.perf_counter() - self.started
        if elapsed > self.limit:
            status = 'TIME_LIMIT'
        self.entry.update(status=status, elapsed_seconds=elapsed)
        self.ledger['cumulative_actual_seconds'] = sum(a.get('elapsed_seconds', a['limit_seconds'])
                                                      for a in self.ledger['attempts'])
        receipt.update(attempt_id=self.attempt_id, status=status, elapsed_seconds=elapsed,
                       limit_seconds=self.limit, cumulative_actual_seconds=self.ledger['cumulative_actual_seconds'],
                       aggregate_limit_seconds=self.ledger['aggregate_limit_seconds'])
        write_json(self.directory / 'receipt.json', receipt)
        write_json(LEDGER_PATH, self.ledger)
