class Sig:
    def __init__(self, seed, kind, count):
        self.v=0xcbf29ce484222325
        self.raw(kind.encode()); self.num(seed); self.num(count)
    def raw(self,b):
        for x in b: self.v=((self.v^x)*0x100000001b3)&((1<<64)-1)
    def num(self,x): self.raw(x.to_bytes(8,'little'))
    def string(self,s):
        b=s.encode('utf-8'); self.num(len(b)); self.raw(b)
def actions(n):
    out=[]
    for i in range(n):
        long=i%19==18
        label=(f'Synthetic action {i:05} — long wrapped label '+('label-segment-東京-'*12)) if long else f'Synthetic action {i:05} — 東京'
        desc=('Synthetic description for layout wrapping: '+('description-segment-λ-'*24)) if long else f'Synthetic action description group {i%23:02}'
        args=('{"fixture":'+str(i)+'}') if i%8==3 else None
        out.append((label,desc,f'synthetic:action:{i:05}',args))
    return out
def aid(xs,kind='track-c-watch-actions',seed=0):
    s=Sig(seed,kind,len(xs))
    for label,desc,action,args in xs:
        for x in (label,desc,action): s.string(x)
        s.num(args is not None)
        if args is not None:s.string(args)
    return s.v
def projection(n,prefix,suggestions):
    half=n//2; custom=actions(half)
    tail=[]
    for i in range(n-half):
        leaf=i//64; p=f'@owned-index/shard-{leaf//16:03}/group-{leaf:04}/file-{i:05}.txt'
        tail.append((f'file-{i:05}.txt',p,p,None))
    a=custom[0]; custom[0]=(a[0],a[1]+'; accepted external replacement',tail[0][2],a[3]); xs=custom+tail
    s=Sig(0,'track-c-watch-projections',n); s.num(aid(xs));s.num(half);s.num(n)
    for label,desc,action,args in xs:
        for x in (label.lower(),desc.lower(),action.lower()):s.string(x)
    s.num(n)
    for label,desc,action,args in xs:
        s.string(action.lower())
        candidates=sorted(set([desc.strip().lower(),prefix if action.startswith('@owned-index') else action.split(':')[0].strip().lower()]))
        s.num(len(candidates))
        for x in candidates:s.string(x)
    # Sorting raw canonical Windows IDs puts all indexed IDs before synthetic IDs.
    byid=tail+custom[1:]
    s.num(len(byid))
    for a in byid:s.string(a[2]);s.num(aid([a]))
    s.num(aid([xs[9]]));s.string('app Synthetic action 00009');s.string('app Synthetic action 00009')
    for x in (1,0,0,1,0):s.num(x)
    s.num(len(suggestions))
    for x in suggestions:s.string(x)
    return aid(xs),s.v
print('action_fixture_1000_control',f'{aid(actions(1000),"track-c-action-state",0x435f574f524b4c44):016x}')
for n in (200,20000):
    for suggestions in ([],['app synthetic action 00009 — 東京']):
        a,p=projection(n,'//?/c',suggestions)
        print(n, 'ascending_paths_C_verbatim', 'empty_suggestions' if not suggestions else 'rebuilt_one_suggestion',f'{a:016x}',f'{p:016x}')
