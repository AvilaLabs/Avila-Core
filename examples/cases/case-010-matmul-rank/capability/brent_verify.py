#!/usr/bin/env python3
"""matmul.brent-verify capability for Avila Core (external-checker adapter).

Verifies that a candidate term list is an exact bilinear decomposition of the
<n,m,p> matrix-multiplication tensor over GF(2), by checking all n*m*p^2...
(all (a,b,c) coordinate) cubic Brent parity equations.

Candidate JSON:
{
  "schema": "avila.matmul/scheme/v1",
  "candidate_id": "...",
  "n": 3, "m": 3, "p": 3,
  "convention": "ct",            // w indexes C^T (row j,col i -> j*n+i)
  "terms": [[u_mask,v_mask,w_mask], ...]   // bitmasks, LSB = entry 0
}

argv (fixed by the adapter): check <candidate_path> --output <report_path>
Report JSON: {"valid": "pass"|"fail", "rank": R, "scheme_sha256": "...",
              "fails": K, "tensor": "3x3x3"}
"""
import sys, json, hashlib

def canon(terms):
    return json.dumps(sorted(terms), separators=(',', ':'), sort_keys=True)

def brent_check_f2(terms, n, m, p, conv='ct'):
    du, dv, dw = n*m, m*p, n*p
    fails = []
    want = set()
    for i in range(n):
        for k in range(m):
            for j in range(p):
                a = i*m + k
                b = k*p + j
                c = j*n + i if conv == 'ct' else i*p + j
                want.add((a, b, c))
    acc = {}
    for (u, v, w) in terms:
        abits = [a for a in range(du) if (u >> a) & 1]
        bbits = [b for b in range(dv) if (v >> b) & 1]
        cbits = [c for c in range(dw) if (w >> c) & 1]
        for a in abits:
            for b in bbits:
                for c in cbits:
                    acc[(a, b, c)] = acc.get((a, b, c), 0) ^ 1
    for abc, v in acc.items():
        if v and abc not in want:
            fails.append(abc)
    for abc in want:
        if acc.get(abc, 0) != 1:
            fails.append(abc)
    return len(fails) == 0, fails

def greedy_cse_additions(terms, n, m, p):
    """Greedy pair-CSE upper bound on additions for a GF(2) scheme.
    Atoms are domain-tagged ((dom,bit)) so no illegal cross-domain sharing.
    Each gate (x XOR y) costs 1; repeatedly merge the most frequent pair."""
    from collections import Counter
    forms = []
    for u, v, w in terms:
        for dom, mask, nb in (('u', u, n * m), ('v', v, m * p)):
            f = {(dom, b) for b in range(nb) if (mask >> b) & 1}
            if f: forms.append(set(f))
    for c in range(n * p):
        f = {('w', i) for i, (_, _, w) in enumerate(terms) if (w >> c) & 1}
        if f: forms.append(set(f))
    adds = 0
    while True:
        cnt = Counter()
        for f in forms:
            for a in f:
                for b in f:
                    if a < b: cnt[(a, b)] += 1
        if not cnt or cnt.most_common(1)[0][1] < 2: break
        pair, _ = cnt.most_common(1)[0]
        g = ('g', adds)
        for f in forms:
            if pair[0] in f and pair[1] in f:
                f.discard(pair[0]); f.discard(pair[1]); f.add(g)
        adds += 1
    adds += sum(max(0, len(f) - 1) for f in forms)
    return adds

def main():
    args = sys.argv[1:]
    if not args or args[0] != 'check':
        print('usage: brent_verify.py check <candidate> --output <report>',
              file=sys.stderr)
        sys.exit(2)
    cand_path = args[1]
    out_path = args[args.index('--output') + 1]
    cand = json.load(open(cand_path))
    n, m, p = cand['n'], cand['m'], cand['p']
    conv = cand.get('convention', 'ct')
    terms = [tuple(t) for t in cand['terms']]
    ok, fails = brent_check_f2(terms, n, m, p, conv)
    sha = hashlib.sha256(canon([list(t) for t in terms]).encode()).hexdigest()
    nnz = sum(bin(t[i]).count('1') for t in terms for i in range(3))
    report = {
        'valid': 'pass' if ok else 'fail',
        'rank': len(terms),
        'scheme_sha256': sha,
        'fails': len(fails),
        'nnz': nnz,
        'adds': greedy_cse_additions(terms, n, m, p),
        'tensor': f'{n}x{m}x{p}',
        'field': 'GF(2)',
    }
    with open(out_path, 'w') as f:
        json.dump(report, f)

if __name__ == '__main__':
    main()
