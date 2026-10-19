#!/usr/bin/env python3
"""Runnable LP/MIP job tutorial. Build the release binaries first:

    cargo build --release --locked -p optimization-runner -p lp-ip-job-worker
"""
import argparse, json, math, os, subprocess, tempfile
from pathlib import Path
try:
    import jsonschema
except ImportError:
    jsonschema = None

REPO = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
EXAMPLES = ['assignment-mip', 'continuous-lp', 'infeasible', 'assignment-evaluate']
SCHEMAS = REPO / 'docs' / 'job-api' / 'schemas'
if jsonschema is None and os.environ.get('JOB_API_REQUIRE_SCHEMA') == '1':
    raise SystemExit('the jsonschema package is required (JOB_API_REQUIRE_SCHEMA=1)')


def conforms(schema, value):
    """Checks value against a published JSON Schema if jsonschema is installed."""
    if jsonschema is not None:
        jsonschema.validate(value, json.loads((SCHEMAS / schema).read_text()), cls=jsonschema.Draft202012Validator)

parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--root', type=Path, help='node directory (default: a new temporary directory)')
parser.add_argument('--bin-dir', type=Path,
                    default=Path(os.environ.get('CARGO_TARGET_DIR', REPO / 'target')) / 'release',
                    help='directory with optimization-run and lp-ip-job-worker (default: $CARGO_TARGET_DIR/release or target/release)')
args = parser.parse_args()
root = (args.root or Path(tempfile.mkdtemp(prefix='lp-ip-jobs-'))).resolve()
bin_dir = args.bin_dir.resolve()
runner, worker = bin_dir / 'optimization-run', bin_dir / 'lp-ip-job-worker'
for binary in [runner, worker]:
    if not binary.is_file():
        parser.error(f'{binary} not found; build the release binaries first')


def call(*argv):
    run = subprocess.run([str(runner), '--root', str(root), *map(str, argv)], capture_output=True, text=True)
    if run.returncode:
        raise SystemExit(f'optimization-run {" ".join(map(str, argv))} failed:\n{run.stderr}')
    return json.loads(run.stdout)


if not (root / 'node.json').exists():
    call('node', 'init', '--capacity', 8)
call('workers', 'add', '--id', 'lp-ip', '--executable', worker)
report = []
for example in EXAMPLES:
    spec = HERE / (example + '.json')
    conforms('optimization.job-0.1.schema.json', json.loads(spec.read_text()))
    call('validate', '--spec', spec)
    job = call('run', '--spec', spec)
    result = call('result', job['directory'])
    conforms('optimization.result-0.2.schema.json', result)
    # A completed job only says that every task returned normally; the
    # mathematical outcome is in each result.
    assert job['state'] == 'completed', result
    assert result['runner']['accounting_complete'], result
    for record in result['results']:
        outcome = record['outcome']
        evidence = outcome['evidence']
        if example == 'infeasible':
            # The solver's claim: no exported certificate, so not rechecked.
            assert outcome['termination'] == 'infeasible' and outcome['claim'] == 'infeasible', outcome
            assert evidence['level'] == 'claimed', outcome
        elif example == 'assignment-evaluate':
            # x0 + x1 = 1: (1, 0) and (0, 1) are feasible, (1, 1) is not.
            feasible = record['task_id'] < 2
            assert outcome['termination'] == 'evaluated' and outcome['feasible'] == feasible, outcome
            assert outcome['claim'] == ('feasible' if feasible else 'none'), outcome
            assert math.isclose(outcome['objective'], [7., 8., 10.][record['task_id']]), outcome
        else:
            assert outcome['claim'] == 'numerical_optimum', outcome
            assert math.isclose(outcome['objective'], 7., abs_tol=1e-7), outcome
            assert evidence['level'] == 'rechecked', outcome
            # An LP's duals pass the KKT acceptance; a MIP has none.
            assert ('kkt' in evidence['checks']) == (example == 'continuous-lp'), outcome
            bounds = outcome['objective_bounds']
            assert bounds['sense'] == 'minimize' and math.isclose(bounds['primal'], outcome['objective']), outcome
    report.append({'example': example, 'job_id': job['id'], 'directory': job['directory'], 'state': job['state'],
                   'completed_tasks': result['completed_tasks'], 'first_outcome': result['results'][0]['outcome'],
                   'work': result['work']})
output = {'node': str(root), 'worker': 'lp-ip', 'runs': report}
(root / 'tutorial-report.json').write_text(json.dumps(output, indent=2) + '\n')
print(json.dumps(output, indent=2))
