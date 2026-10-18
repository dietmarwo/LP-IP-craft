"""Audit measured results against original inputs and write reproducible tables."""
import hashlib
import json
import math
from collections import defaultdict
from pathlib import Path

from run import BASE, ROOT, DEV, check

DATA = ROOT / 'docs/ripsolve-027/data'
FILES = ['public', 'public-60s', 'heldout', 'families', 'branch-heldout', 'branch-families', 'ripsolve-serial']


def status(record):
    return record.get('result', record).get('status', '').lower().replace(' ', '_')


def label(record):
    return record['solver'] + '/' + record['policy']


def source(name):
    for folder in [BASE / 'heldout', BASE / 'families']:
        path = folder / (name + '.json')
        if path.exists():
            return path
    return next(ROOT.glob('docs/*/corpus/**/' + name + '.json'))


def root(record):
    metadata = dict(record['result'].get('metadata', []))
    stages = json.loads(metadata.get('diagnostics.root_stages', '[]'))
    if stages:
        return stages[-1].get('objective')
    diagnostic = json.loads(metadata.get('diagnostics.mip_search', '{}'))
    return diagnostic.get('root', {}).get('lp_objective')


def show(value):
    return '—' if value is None else f'{value:,.6g}'


def table(headers, rows):
    return '\n'.join(['| ' + ' | '.join(headers) + ' |',
                      '|' + '|'.join(['---'] * len(headers)) + '|'] +
                     ['| ' + ' | '.join(map(str, row)) + ' |' for row in rows])


def main():
    failures = []
    observations = []
    sections = []
    references = {}
    for filename in FILES:
        path = DATA / (filename + '.json')
        if path.exists():
            for record in json.loads(path.read_text())['records']:
                if record['solver'] == 'highs' and status(record) in ['optimal', 'infeasible']:
                    references[record['case']] = record
    for filename in FILES:
        path = DATA / (filename + '.json')
        if not path.exists():
            raise RuntimeError(f'missing final evidence: {path}')
        data = json.loads(path.read_text())
        records = data['records']
        groups = defaultdict(list)
        by_case = defaultdict(list)
        for record in records:
            groups[label(record)].append(record)
            by_case[record['case']].append(record)
        for name, same in by_case.items():
            path = source(name)
            payload = json.loads(path.read_text())
            spec = payload['model']
            expected = payload.get('expected')
            highs = references.get(name)
            if highs and status(highs) == 'optimal':
                expected = highs['checked_objective']
            infeasible = highs is not None and status(highs) == 'infeasible'
            for record in same:
                result = record.get('result', {})
                issues = list(record.get('issues', []))
                if record.get('input_sha256') != hashlib.sha256(path.read_bytes()).hexdigest():
                    issues.append('input hash mismatch')
                if len({r.get('model_sha256') for r in same}) != 1:
                    issues.append('different model arrays')
                value, original_issues = check(spec, result)
                issues += original_issues
                bound = result.get('bound')
                tolerance = 1e-6 * (1 + abs(expected or 0))
                if expected is not None:
                    if status(record) == 'infeasible':
                        issues.append('false infeasibility')
                    if status(record) == 'optimal' and (value is None or abs(value - expected) > tolerance):
                        issues.append('wrong optimum against reference')
                    if bound is not None and (bound > expected + tolerance if spec['sense'] == 'minimize' else bound < expected - tolerance):
                        issues.append('unsafe global bound against reference')
                    metadata = dict(result.get('metadata', []))
                    for stage in json.loads(metadata.get('diagnostics.root_stages', '[]')):
                        stage_bound = stage.get('bound')
                        if stage_bound is not None and (stage_bound > expected + tolerance if spec['sense'] == 'minimize' else stage_bound < expected - tolerance):
                            issues.append('unsafe native root-stage bound against reference')
                if infeasible and value is not None:
                    issues.append('feasible point on reference-infeasible model')
                if status(record) == 'optimal' and bound is not None and value is not None and abs(bound - value) > 1e-6 * (1 + abs(value)):
                    issues.append('optimal with open bound gap')
                if issues:
                    failures.append(dict(file=filename, case=name, policy=label(record), issues=sorted(set(issues))))
                if status(record) in ['error', 'process_error', 'watchdog']:
                    observations.append(dict(file=filename, case=name, policy=label(record), status=status(record), error=result.get('error', record.get('error'))))
        cap = data['time_limit']
        rows = []
        for key, group in groups.items():
            solved = sum(status(r) in ['optimal', 'infeasible'] for r in group)
            times = [min(r['seconds'], cap) if status(r) in ['optimal', 'infeasible'] else cap for r in group]
            mean = math.exp(sum(math.log(t + .01) for t in times) / len(times)) - .01
            rows.append([key, f'{solved}/{len(group)}', f'{mean:.5f}'])
        sections += ['## ' + filename, '',
                     table(['Solver/policy', 'Proved optimal/infeasible', 'Shifted geometric mean (s)'], rows), '']
        if filename in ['public', 'public-60s']:
            keys = list(groups)
            rows = []
            for name in DEV:
                row = [name]
                for key in keys:
                    record = next(r for r in groups[key] if r['case'] == name)
                    mark = '' if status(record) == 'optimal' else ' [' + status(record) + ']'
                    row.append(f"{record['seconds']:.4f}" + mark)
                rows.append(row)
            sections += [table(['Case'] + keys, rows), '']
        if filename in ['heldout', 'families']:
            base = {r['case']: r for r in groups['baseline/default']}
            rows = []
            for key in ['native/presolve', 'native/mir', 'native/combined']:
                for record in groups[key]:
                    ratio = record['seconds'] / max(base[record['case']]['seconds'], 1e-9)
                    if ratio > 1.5:
                        rows.append([record['case'], key, f'{ratio:.2f}', f"{record['seconds'] - base[record['case']]['seconds']:.5f}"])
            sections += ['Measured regressions over 1.5× (includes submillisecond cases):', '',
                         table(['Case', 'Policy', 'Ratio', 'Extra seconds'], rows), '']
    public = json.loads((DATA / 'public.json').read_text())['records']
    lp_records = json.loads((DATA / 'plain-lp.json').read_text())['records']
    rows = []
    for name in DEV:
        row = [name, show(next(r for r in lp_records if r['case'] == name)['result']['objective'])]
        for solver, policy in [('baseline', 'default'), ('native', 'presolve'), ('native', 'gmi'), ('native', 'mir'), ('native', 'combined')]:
            row.append(show(root(next(r for r in public if r['case'] == name and r['solver'] == solver and r['policy'] == policy))))
        rows.append(row)
    sections += ['## Root LP objectives', '', 'These are relaxation objectives; checked conservative bounds are separate fields in raw records. — means no root MIP diagnostic (including the LP control avgas).', '',
                 table(['Case', 'Original LP', 'Baseline root', 'W1', 'W1+GMI', 'W1+MIR', 'Combined'], rows), '']
    rows = []
    for name in DEV:
        for record in [r for r in public if r['case'] == name and (r['solver'] == 'baseline' or r['solver'] == 'native' and r['policy'] in ['mir', 'combined', 'heuristics'])]:
            metadata = dict(record['result'].get('metadata', []))
            rows.append([name, label(record), record['result'].get('nodes'), record['result'].get('pivots'),
                         metadata.get('diagnostics.gmi_cuts', '—'), metadata.get('diagnostics.mir_cuts', '—'),
                         metadata.get('diagnostics.root_heuristic_candidates', '—')])
    sections += ['## Work and candidate counters (10 s)', '',
                 table(['Case', 'Policy', 'LP nodes', 'Pivots', 'GMI cuts', 'MIR cuts', 'Root heuristic candidates'], rows), '']
    output = dict(correctness_failures=failures, unsupported_or_process_results=observations,
                  included_files=FILES, note='Reference bug-only ablations are intentionally excluded from correctness acceptance.')
    (DATA / 'comparison-audit.json').write_text(json.dumps(output, indent=2) + '\n')
    (ROOT / 'docs/ripsolve-027/TABLES.md').write_text('# Recomputed benchmark tables\n\nShift 0.01 s. Unproved/error cases are charged the full time cap, including early node limits. Successful times are capped at the time limit. Infeasible proofs count as solved. Single observations; no statistical significance claim.\n\n' + '\n'.join(sections))
    print(json.dumps(output, indent=2))
    if failures:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
