"""Pure synthetic discovery reference. No I/O or execution authority."""
from __future__ import annotations

from dataclasses import dataclass
import json
import re
from typing import Callable


class ProjectionError(ValueError):
    pass


@dataclass(frozen=True)
class ActionPayload:
    label: str
    desc: str
    action: str
    args: str | None

    def key(self) -> tuple[str, str | None]:
        return self.action, self.args

    def record(self) -> dict:
        return dict(label=self.label, desc=self.desc, action=self.action, args=self.args)


@dataclass(frozen=True)
class Descriptor:
    id: str
    owner: str
    action: ActionPayload
    subset: str
    declared_search: bool
    semantics: tuple[str, str]
    phrases: tuple[tuple[str, tuple[str, ...]], ...]
    tokens: frozenset[str]
    token_fields: tuple[tuple[str, tuple[str, ...]], ...]
    intents: frozenset[str]
    objects: frozenset[str]


def normalized_phrase(text: str, policy: dict) -> tuple[str, ...]:
    """Metadata/natural text normalization only; admission precedes query use."""
    norm = policy['normalization']
    lower = text.lower()
    # Whole space-delimited compound tokens, not arbitrary embedded rewrites.
    lower = ' '.join(norm['compound_rewrites'].get(w, w) for w in lower.split(' '))
    seen = set()
    answer = []
    for token in re.findall(r'[a-z0-9]+', lower):
        token = norm['inflections'].get(token, token)
        if token not in norm['stopwords'] and token not in seen:
            seen.add(token)
            answer.append(token)
    return tuple(answer)


def vocabulary_maps(policy: dict) -> tuple[dict[str, frozenset[str]], dict[str, frozenset[str]]]:
    maps = []
    for field in ('intent_vocabulary', 'object_vocabulary'):
        reverse: dict[str, set[str]] = {}
        for code, words in policy[field].items():
            for word in words:
                for token in normalized_phrase(word, policy):
                    reverse.setdefault(token, set()).add(code)
        maps.append({k: frozenset(v) for k, v in reverse.items()})
    return maps[0], maps[1]


def prepare(catalog: dict, policy: dict, candidate: str) -> tuple[tuple[Descriptor, ...], list[dict]]:
    """Bounded projection. Source evidence/descriptions are not retrieval fields."""
    if candidate not in policy['candidates']:
        raise ProjectionError('Unknown declared candidate')
    bound = policy['bounds']
    if len(catalog['entries']) > bound['max_descriptors']:
        raise ProjectionError('Descriptor limit')
    result = []
    seen = set()
    for entry in catalog['entries']:
        eid = entry['id']
        if eid in seen or not re.fullmatch(bound['descriptor_id_pattern'], eid):
            raise ProjectionError('Invalid/duplicate descriptor ID')
        seen.add(eid)
        aliases = entry['proposed_aliases']
        shortcuts = entry['existing_shortcuts']
        if len(aliases) > bound['max_aliases_per_descriptor'] or len(shortcuts) > bound['max_shortcuts_per_descriptor']:
            raise ProjectionError('Alias/shortcut limit')
        for alias in aliases:
            if not alias.isascii() or len(alias.encode('utf-8')) > bound['max_alias_utf8_bytes'] or len(normalized_phrase(alias, policy)) > bound['max_alias_tokens']:
                raise ProjectionError('Alias size/alphabet')
        action = ActionPayload(**entry['current_action'])
        if action.args is not None:
            raise ProjectionError('Fixed opener research scope requires None args')
        roles = policy['entry_roles'][eid]
        if not set(roles['intents']) <= policy['intent_vocabulary'].keys() or not set(roles['objects']) <= policy['object_vocabulary'].keys():
            raise ProjectionError('Unknown role')
        texts = [('label', entry['label'])]
        texts += [('shortcut:' + item['query'], item['action']['label']) for item in shortcuts]
        if candidate != 'P0':
            texts += [('alias:' + text, text) for text in aliases]
        phrases = tuple((field, normalized_phrase(text, policy)) for field, text in texts)
        fields: dict[str, set[str]] = {}
        for field, phrase in phrases:
            for token in phrase:
                fields.setdefault(token, set()).add(field)
        if candidate == 'P2':
            for kind, codes in (('intent', roles['intents']), ('object', roles['objects'])):
                for code in codes:
                    for word in policy[kind + '_vocabulary'][code]:
                        for token in normalized_phrase(word, policy):
                            fields.setdefault(token, set()).add(kind + ':' + code)
        if len(fields) > bound['max_document_tokens']:
            raise ProjectionError('Document token limit')
        result.append(Descriptor(
            eid, entry['owner']['name'], action, entry['scope_subset'],
            'search' in entry['declared_capabilities'],
            (entry['typed_command'], entry['opener_effect']), phrases,
            frozenset(fields), tuple((t, tuple(sorted(v))) for t, v in sorted(fields.items())),
            frozenset(roles['intents']), frozenset(roles['objects']),
        ))
    conflicts = []
    groups: dict[tuple, list[Descriptor]] = {}
    for desc in result:
        groups.setdefault(desc.action.key(), []).append(desc)
    excluded = set()
    for group in groups.values():
        if len({(d.owner, d.semantics) for d in group}) > 1:
            excluded.update(d.id for d in group)
            conflicts.append({'ids': sorted(d.id for d in group), 'reason': 'ConflictingOwnerOrSemantics'})
    result = [d for d in result if d.id not in excluded]
    portable = [dict(id=d.id, owner=d.owner, action=d.action.record(), phrases=d.phrases,
                     tokens=sorted(d.tokens), token_fields=d.token_fields,
                     intents=sorted(d.intents), objects=sorted(d.objects)) for d in result]
    if len(json.dumps(portable, ensure_ascii=False, separators=(',', ':')).encode('utf-8')) > policy['engineering_targets_unmeasured']['projection_limit_utf8_bytes']:
        raise ProjectionError('Projection serialized-byte limit')
    return tuple(result), conflicts


def owner_reasons(desc: Descriptor, context: dict) -> list[str]:
    state = context.get('owner_overrides', {}).get(desc.owner, {})
    checks = [
        ('Missing', state.get('present', True)),
        ('Ambiguous', state.get('unique', True)),
        ('WrongOrigin', state.get('origin', 'Builtin') == 'Builtin'),
        ('StaleEpoch', state.get('epoch_current', True)),
        ('Disabled', context['all_owners_enabled'] and state.get('enabled', True)),
        ('UndeclaredSearch', desc.declared_search and state.get('declared_search', True)),
    ]
    restriction = state.get('capability_restriction')
    checks.append(('SearchDenied', restriction is None or 'search' in restriction))
    return [reason for reason, passes in checks if not passes]


def admission(raw: str, context: dict, policy: dict, routes: dict) -> tuple[str, tuple[str, ...]]:
    """Frozen proposed diagnostic order; does not emulate Rust query routing."""
    if not context['feature_enabled']:
        return 'Disabled', ()
    if (not context['input_accepting'] or not context['session_current'] or
            context['query_origin'] != 'FreshUserEdit' or
            context['surface'] not in policy['common_admission']['allowed_surfaces']):
        return 'ExcludedSurfaceOrOrigin', ()
    if (context['settings_transition'] or not context['query_generation_current'] or
            not context['stamps_current'] or context['canonical_result_count'] > 0 or
            context['provider_pending'] or context['provider_deferral'] != 'None' or
            not context['result_catalog_stable']):
        return 'BusyOrStale', ()
    if context['reserved_route_coverage'] != 'Complete' or not context['reserved_route_revision_current']:
        return 'CoverageUnknown', ()
    norm = policy['normalization']
    if len(raw.encode('utf-8')) > norm['max_raw_utf8_bytes']:
        return 'InputBound', ()
    stripped = raw.strip(' ')
    lower = stripped.lower()
    head = lower.split(' ', 1)[0]
    if head in routes['heads'] or any(lower.startswith(marker) for marker in routes['raw_starts_with']):
        return 'ProtectedSyntax', ()
    for index, char in enumerate(raw):
        if char == '-':
            if not (index > 0 and index + 1 < len(raw) and
                    raw[index - 1].isascii() and raw[index - 1].isalnum() and
                    raw[index + 1].isascii() and raw[index + 1].isalnum()):
                return 'ProtectedSyntax', ()
        elif char != ' ' and not (char.isascii() and char.isalnum()):
            return 'ProtectedSyntax', ()
    raw_tokens = [word for word in raw.split(' ') if word]
    if (not raw_tokens or len(raw_tokens) > norm['max_raw_tokens'] or
            any(len(word) > norm['max_token_length'] for word in raw_tokens)):
        return 'InputBound', ()
    phrase = normalized_phrase(raw, policy)
    letters = sum(sum('a' <= c <= 'z' for c in word) for word in phrase)
    if (not phrase or len(phrase) > norm['max_unique_content_tokens'] or
            letters < norm['min_total_content_letters'] or
            (len(phrase) == 1 and len(phrase[0]) < norm['single_token_min_length'])):
        return 'InputBound', ()
    words = set(phrase)
    hard = policy['common_admission']
    if words.intersection(hard['dangerous_verb_tokens']):
        return 'UnsupportedIntent', ()
    for rule in hard['unsupported_rules']:
        if all(words.intersection(group) for group in rule['all_groups']):
            return 'UnsupportedIntent', ()
    return 'ADMIT', phrase


def one_edit(left: str, right: str) -> bool:
    """Exactly one insertion/deletion/substitution/adjacent transposition, O(L)."""
    if abs(len(left) - len(right)) > 1 or left == right:
        return False
    if len(left) == len(right):
        differences = [i for i, (a, b) in enumerate(zip(left, right)) if a != b]
        if len(differences) == 1:
            return True
        return (len(differences) == 2 and differences[1] == differences[0] + 1 and
                left[differences[0]] == right[differences[1]] and
                left[differences[1]] == right[differences[0]])
    short, long = (left, right) if len(left) < len(right) else (right, left)
    i = 0
    while i < len(short) and short[i] == long[i]:
        i += 1
    return short[i:] == long[i + 1:]


def lexical_hits(desc: Descriptor, phrase: tuple[str, ...], policy: dict) -> dict[str, dict]:
    matching = policy['matching']
    hits = {}
    for q in sorted(phrase):
        if q in desc.tokens:
            hits[q] = dict(kind='exact', weight=matching['exact_hit'], target=q)
        elif q == phrase[-1] and len(q) >= matching['prefix_min_query_length']:
            matches = sorted(t for t in desc.tokens if t.startswith(q) and len(t) <= len(q) + matching['prefix_max_extra_characters'])
            hits[q] = (dict(kind='prefix', weight=matching['prefix_hit'], target=matches[0])
                       if matches else dict(kind='none', weight=0, target=None))
        else:
            hits[q] = dict(kind='none', weight=0, target=None)
    return hits


def potential_pairs(desc: Descriptor, hits: dict, policy: dict) -> list[tuple[str, str]]:
    match = policy['matching']
    lo, hi = match['typo_min_lengths'], match['typo_max_lengths']
    # A necessary one-edit condition is length difference <=1. Count all such
    # pairs before choosing a correction, so early matches never bias the cap.
    return [(q, d) for q, hit in sorted(hits.items()) if not hit['weight'] and lo <= len(q) <= hi
            for d in sorted(desc.tokens) if lo <= len(d) <= hi and abs(len(q) - len(d)) <= 1]


def score(desc: Descriptor, phrase: tuple[str, ...], hits: dict, candidate: str,
          policy: dict, role_maps: tuple[dict, dict]) -> dict:
    H = sum(hit['weight'] for hit in hits.values())
    C = H // len(phrase)
    U = sum(not hit['weight'] for hit in hits.values())
    exact_fields = [field for field, field_phrase in desc.phrases if phrase == field_phrase]
    E = int(bool(exact_fields))
    config = policy[candidate]
    base = config['coverage_numerator'] * C // config['coverage_denominator']
    terms = dict(coverage=base, exact_phrase=config['exact_phrase_bonus'] * E,
                 unmatched=-config['unmatched_penalty'] * U)
    qi: set[str] = set()
    qo: set[str] = set()
    I = O = JI = JO = 0
    if candidate == 'P2':
        evidence = set(phrase) | {h['target'] for h in hits.values() if h['kind'] in ('prefix', 'typo')}
        for word in evidence:
            qi.update(role_maps[0].get(word, ()))
            qo.update(role_maps[1].get(word, ()))
        I = 100 * len(qi & desc.intents) // len(qi) if qi else 0
        O = 100 * len(qo & desc.objects) // len(qo) if qo else 0
        JI = int(bool(qi) and not qi.intersection(desc.intents))
        JO = int(bool(qo) and not qo.intersection(desc.objects))
        terms.update(intent=config['matched_intent_bonus'] * I // 100,
                     object=config['matched_object_bonus'] * O // 100,
                     intent_conflict=-config['intent_conflict_penalty'] * JI,
                     object_conflict=-config['object_conflict_penalty'] * JO)
    value = min(100, max(0, sum(terms.values())))
    fields = dict(desc.token_fields)
    explained_hits = {q: dict(hit, fields=list(fields.get(hit['target'], ()))) for q, hit in sorted(hits.items())}
    return dict(id=desc.id, owner=desc.owner, score=value, action=desc.action.record(),
                explanation=dict(H=H, m=len(phrase), C=C, U=U, E=E, exact_fields=exact_fields,
                                 hits=explained_hits, QI=sorted(qi), QO=sorted(qo),
                                 DI=sorted(desc.intents), DO=sorted(desc.objects), I=I, O=O,
                                 JI=JI, JO=JO, terms=terms, unclamped=sum(terms.values())))


def collapse(rows: list[dict]) -> list[dict]:
    groups: dict[tuple, list[dict]] = {}
    for row in rows:
        action = row['action']
        groups.setdefault((action['action'], action['args']), []).append(row)
    result = []
    for group in groups.values():
        ordered = sorted(group, key=lambda row: (-row['score'], row['id']))
        result.append(dict(ordered[0], contributors=sorted(row['id'] for row in group)))
    return sorted(result, key=lambda row: (-row['score'], row['id']))


class Reference:
    def __init__(self, catalog: dict, policy: dict, routes: dict, candidate: str):
        self.policy, self.routes, self.candidate = policy, routes, candidate
        self.descriptors, self.projection_errors = prepare(catalog, policy, candidate)
        self.role_maps = vocabulary_maps(policy)

    def query(self, raw: str, context: dict, scope: str, tick: Callable[[], None] = lambda: None,
              reverse: bool = False) -> dict:
        tick()
        if scope not in ('full', 'initial'):
            raise ValueError('Unknown declared catalog scope')
        source = [d for d in self.descriptors if scope == 'full' or d.subset == 'SEARCH_F001_INITIAL']
        if reverse:
            source.reverse()
        eligible = []
        filtered = []
        for desc in source:
            reasons = owner_reasons(desc, context)
            if reasons:
                filtered.append(dict(id=desc.id, owner=desc.owner, reasons=reasons))
            else:
                eligible.append(desc)
        gate, phrase = admission(raw, context, self.policy, self.routes)
        if gate == 'ADMIT' and not eligible:
            gate = 'NoEligibleOwner'
        base = dict(gate=gate, state='EXCLUDED' if gate != 'ADMIT' else None,
                    rows=[], scored_candidates=[], filtered_owners=sorted(filtered, key=lambda x: x['id']),
                    projection_errors=self.projection_errors, operations=dict(descriptors=len(source), eligible=len(eligible), potential_typo_pairs=0, edit_tests=0))
        if gate != 'ADMIT':
            return base
        prepared = [(d, lexical_hits(d, phrase, self.policy)) for d in eligible]
        pairs = {}
        if self.candidate == 'P2':
            pairs = {d.id: potential_pairs(d, hits, self.policy) for d, hits in prepared}
            total = sum(len(items) for items in pairs.values())
            base['operations']['potential_typo_pairs'] = total
            if total > self.policy['bounds']['max_typo_pairs']:
                base['state'] = 'ABSTAIN_BUDGET'
                return base
        scored = []
        for desc, hits in prepared:
            tick()
            if self.candidate == 'P2':
                for q, target in pairs[desc.id]:
                    tick()
                    base['operations']['edit_tests'] += 1
                    if one_edit(q, target):
                        hits[q] = dict(kind='typo', weight=self.policy['matching']['typo_hit'], target=target)
                        break
            scored.append(score(desc, phrase, hits, self.candidate, self.policy, self.role_maps))
        ordered = collapse(scored)
        base['scored_candidates'] = sorted(scored, key=lambda row: (-row['score'], row['id']))
        result = self.policy['result_policy']
        qualified = [row for row in ordered if row['score'] >= result['min_score']]
        if not qualified:
            base['state'] = result['zero_candidates']
        elif (qualified[0]['score'] >= result['clear_single_min_score'] and
              (len(qualified) == 1 or qualified[0]['score'] - qualified[1]['score'] >= result['clear_single_margin'])):
            base['state'] = result['clear_single']
            base['rows'] = qualified[:self.policy['bounds']['top_k']]
        elif len(qualified) > 1:
            base['state'] = result['ambiguous']
            base['rows'] = qualified[:self.policy['bounds']['top_k']]
        else:
            base['state'] = result['one_weak_candidate']
        return base


def signature(result: dict) -> tuple:
    return (result['gate'], result['state'], tuple(
        (row['id'], row['score'], tuple(row['action'][k] for k in ('label', 'desc', 'action', 'args')))
        for row in result['rows']))
