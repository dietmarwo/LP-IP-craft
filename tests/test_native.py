"""Native engine acceptance, independent checks and explicit limits."""
import math
from concurrent.futures import ThreadPoolExecutor
import pytest
from lp_ip_craft import BACKENDS, Model, NativeOptions, Options, Cancellation, Duals, solve, lp_residuals

def make(c=(3.,2.), a=((1.,1.),(2.,1.)), lo=None, up=None, rl=None, ru=(4.,5.), kinds=None, sense="maximize", offset=0.):
    n=len(c); a=list(a); indptr=[0];indices=[];values=[]
    for row in a:
        for j,v in enumerate(row):
            if v!=0:indices.append(j);values.append(v)
        indptr.append(len(indices))
    return Model(objective=c,objective_offset=offset,lower=[0.]*n if lo is None else lo,upper=[math.inf]*n if up is None else up,
                 row_lower=[-math.inf]*len(a) if rl is None else rl,row_upper=ru,indptr=indptr,indices=indices,values=values,kinds=kinds,sense=sense)

def test_backend_inventory():
    assert BACKENDS == ("dummy","native")
    with pytest.raises(ValueError):solve(make(),backend="highs")

@pytest.mark.parametrize("sense,c,expected",[("maximize",(3.,2.),9.),("minimize",(-3.,-2.),-9.)])
def test_lp_duals_and_offsets(sense,c,expected):
    m=make(c=c,sense=sense,offset=7.)
    r=solve(m,backend="native")
    assert r.status=="optimal" and r.evaluation.feasible_candidate
    assert r.evaluation.objective==pytest.approx(expected+7.)
    k=lp_residuals(m,r.candidate,r.duals)
    assert max(k.primal,k.stationarity,k.dual_sign,k.duality_gap)<1e-7

@pytest.mark.parametrize("opts,status",[(NativeOptions(time_limit_seconds=0.),"time_limit"),(NativeOptions(iteration_limit=0),"iteration_limit")])
def test_limits(opts,status):assert solve(make(),backend="native",backend_options=opts).status==status

def test_cancel_and_validation():
    c=Cancellation();c.cancel()
    assert solve(make(),backend="native",cancellation=c).status=="cancelled"
    with pytest.raises(ValueError):solve(make(),backend="native",cancellation=c,start=[])
    with pytest.raises(ValueError):solve(make(),backend="native",backend_options=NativeOptions(pivot_tolerance=math.nan))

def test_mip_start_node_limit():
    m=make(up=[1.,1.],kinds=["binary"]*2)
    r=solve(m,backend="native",start=[1.,0.],backend_options=NativeOptions(node_limit=0))
    assert r.status=="node_limit" and r.candidate==[1.,0.] and r.best_bound is None

def test_independent_instances():
    m=make()
    with ThreadPoolExecutor(4) as pool:results=list(pool.map(lambda _:solve(m,backend="native"),range(12)))
    assert all(r.status=="optimal" and r.evaluation.objective==pytest.approx(9.) for r in results)

def test_infeasible_unbounded_and_general_bounds():
    assert solve(make(c=[1.],a=[[1.],[1.]],lo=[-math.inf],rl=[2.,-math.inf],ru=[math.inf,1.]),backend="native").status=="infeasible"
    assert solve(make(c=[1.],a=[],ru=[]),backend="native").status=="unbounded"
    r=solve(make(c=[1.],a=[],lo=[-math.inf],up=[3.],ru=[]),backend="native")
    assert r.status=="optimal" and r.candidate==[3.]

def test_small_coefficients():
    r=solve(make(c=[1.],a=[[1e-11]],rl=[2e-11],ru=[2e-11],up=[10.],sense="minimize"),backend="native")
    assert r.status=="optimal" and r.candidate==pytest.approx([2.])

def test_binary_exhaustive():
    import random,itertools
    rng=random.Random(8923)
    for _ in range(60):
        c=[rng.randint(-9,13) for _ in range(7)]; a=[[rng.randint(-3,9) for _ in c] for _ in range(3)];b=[rng.randint(3,15) for _ in a]
        feasible=[sum(cj*xj for cj,xj in zip(c,x)) for x in itertools.product([0.,1.],repeat=7) if all(sum(v*t for v,t in zip(row,x))<=rhs for row,rhs in zip(a,b))]
        r=solve(make(c=c,a=a,ru=b,up=[1.]*7,kinds=["binary"]*7),backend="native")
        assert r.status=="optimal" and r.evaluation.objective==pytest.approx(max(feasible))
        if r.best_bound is not None:assert r.best_bound>=max(feasible)-1e-8

@pytest.mark.parametrize("settings",[
    NativeOptions(presolve=False),NativeOptions(steepest_edge=False),
    NativeOptions(refactor_interval=1),NativeOptions(node_reoptimization=False),
    NativeOptions(incumbent_heuristic=False,objective_lattice=False)])
def test_optimization_switches(settings):
    m=make(c=[7.,11.,5.,6.],a=[[3.,5.,2.,4.]],ru=[8.5],up=[1.]*4,kinds=["binary"]*4)
    r=solve(m,backend="native",backend_options=settings)
    assert r.status=="optimal" and r.evaluation.objective==pytest.approx(18.)

@pytest.mark.parametrize("c,kinds,offset,expected_lattice",[
    ([2.,3.],["integer","integer"],0.,"true"),
    ([2.5,3.],["integer","integer"],0.,"false"),
    ([2.,3.],["integer","continuous"],0.,"false"),
    ([2.,0.],["integer","continuous"],0.,"true"),
    ([2.,1e-16],["integer","continuous"],0.,"false"),
    ([2.,3.],["integer","integer"],0.5,"false")])
def test_lattice_scope(c,kinds,offset,expected_lattice):
    m=make(c=c,a=[[1.,1.]],ru=[1.5],up=[2.,2.],kinds=kinds,offset=offset)
    r=solve(m,backend="native")
    assert r.status=="optimal"
    assert r.backend.effective_options["diagnostics.objective_lattice"]==expected_lattice

def test_start_within_the_feasibility_tolerance_is_the_optimum():
    m=make(c=[1.],a=[[1.]],ru=[0.99999995],up=[1.],kinds=["integer"])
    r=solve(m,backend="native",start=[1.])
    # The row limit is within the feasibility tolerance of 1, so integer row
    # rounding keeps x = 1. Up to 0.32.0 the stored row was read exactly, the
    # limit was rounded to 0 and the answer was x = 0.
    assert r.status=="optimal" and r.best_bound>=1.
    assert r.candidate==[1.] and r.evaluation.objective==1.

def test_lp_primal_start_is_explicitly_rejected():
    r=solve(make(),backend="native",start=[0.,0.])
    assert r.status=="optimal" and r.backend.start_status=="rejected"


def test_mixed_bound_against_exact_optimum():
    m=make(c=[7.,-6.,0.,0.,2.],a=[[0.,-4.,5.,-1.,-1.],[-6.,1.,5.,4.,-6.],[1.,3.,6.,-3.,2.],[-4.,-1.,4.,4.,-5.]],
           lo=[0.]*5,up=[3.]*5,rl=[-12.,-13.,9.,-12.],ru=[-9.,-10.,12.,-8.],kinds=["integer","continuous","integer","continuous","integer"],sense="minimize")
    r=solve(m,backend="native")
    assert r.status=="optimal" and r.evaluation.objective==pytest.approx(-8.)
    assert r.best_bound<=-8.+1e-10

@pytest.mark.parametrize("enabled",[False,True])
def test_redundant_rows(enabled):
    m=make(c=[3.,2.],a=[[1.,1.],[1.,-1.],[0.,0.]],up=[1.,1.],rl=[0.,-1.,0.],ru=[2.,1.,0.])
    r=solve(m,backend="native",backend_options=NativeOptions(redundant_rows=enabled))
    assert r.status=="optimal" and r.evaluation.objective==pytest.approx(5.)
    if enabled:assert int(r.backend.effective_options["diagnostics.presolve_removed_rows"])==3


def test_rows_that_agree_within_the_feasibility_tolerance_keep_the_start():
    # x <= 0.99999995 and x >= 1: read exactly the rows contradict each other
    # (numerical_failure with the start up to 0.32.0); within the tolerance
    # x = 1 satisfies both.
    m=make(c=[1.],a=[[1.],[1.]],up=[1.],rl=[-math.inf,1.],ru=[0.99999995,math.inf],kinds=["integer"])
    r=solve(m,backend="native",start=[1.])
    assert r.status=="optimal" and r.candidate==[1.]


def test_strengthened_integer_rows_do_not_block_continuous_certification():
    import json
    from pathlib import Path
    case = json.loads((Path(__file__).parent / "data/mip_integer_row_certification.json").read_text())
    m = Model(**case["model"])
    r = solve(m, backend="native")
    assert r.status == "optimal"
    assert abs(r.evaluation.objective - case["objective"]) < 1e-6
    assert r.nodes < 1000

@pytest.mark.parametrize("method", ["auto", "eta", "forrest_tomlin"])
def test_basis_update_controls(method):
    r = solve(make(), backend="native", backend_options=NativeOptions(basis_update=method))
    assert r.status == "optimal"
    assert r.evaluation.objective == pytest.approx(9.)
    assert dict(r.backend.effective_options)["basis_update"] == method

def test_unknown_basis_update_is_rejected():
    with pytest.raises(ValueError, match="basis_update"):
        solve(make(), backend="native", backend_options=NativeOptions(basis_update="unknown"))

@pytest.mark.parametrize("ordering", ["auto", "natural", "min_degree"])
@pytest.mark.parametrize("adaptive", [False, True])
def test_factorization_controls(ordering, adaptive):
    r = solve(make(), backend="native", backend_options=NativeOptions(
        factor_ordering=ordering, factor_pivot_search=8, adaptive_refactor=adaptive))
    assert r.status == "optimal" and r.evaluation.objective == pytest.approx(9.)
    assert dict(r.backend.effective_options)["factor_ordering"] == ordering

@pytest.mark.parametrize("search", [0, 65])
def test_invalid_factor_pivot_search(search):
    with pytest.raises(ValueError, match="factor_pivot_search"):
        solve(make(), backend="native", backend_options=NativeOptions(factor_pivot_search=search))

def test_invalid_factor_ordering():
    with pytest.raises(ValueError, match="factor_ordering"):
        solve(make(), backend="native", backend_options=NativeOptions(factor_ordering="unknown"))


def test_integer_equality_residue_with_a_checked_start():
    model = make(c=(1., 0., 0.), a=((0.9, 1., -1.),), lo=(-13., -20., -20.),
                 up=(16., 20., 20.), rl=(0.,), ru=(0.,), kinds=["integer"] * 3)
    result = solve(model, backend="native", start=[0., 0., 0.],
                   backend_options=NativeOptions(integer_equality_presolve=True))
    assert result.status == "optimal"
    assert result.candidate[0] == 10.
    assert result.evaluation.feasible_candidate
    assert result.backend.effective_options["integer_equality_presolve"] == "true"


def test_adaptive_root_honors_small_limits_and_explicit_families():
    n = 16
    model = make(c=[1.] * n, a=[[1.] * n], up=[1.] * n, ru=(5.75,), kinds=["binary"] * n)
    baseline = solve(model, backend="native", backend_options=NativeOptions(node_limit=1))
    limited = solve(model, backend="native",
                    backend_options=NativeOptions(adaptive_root=True, node_limit=1))
    assert (limited.status, limited.nodes, limited.candidate) == (baseline.status, baseline.nodes, baseline.candidate)
    explicit = solve(model, backend="native", backend_options=NativeOptions(adaptive_root=True, gmi_cuts=True))
    assert "diagnostics.adaptive_root_family" not in explicit.backend.effective_options
    assert explicit.status == "optimal"
    assert explicit.evaluation.objective == 5.
    stop = Cancellation()
    stop.cancel()
    stopped = solve(model, backend="native", cancellation=stop,
                    backend_options=NativeOptions(adaptive_root=True, integer_equality_presolve=True))
    assert stopped.status == "cancelled" and stopped.nodes == 0

@pytest.mark.parametrize("sense,sign", [("maximize", 1.), ("minimize", -1.)])
def test_mixed_integer_objective_keeps_bound_on_original_scale(sense, sign):
    m = make(c=[sign, 0.], a=[[1., 1.]], lo=[0., 0.25], up=[2., 1.],
             ru=[1.75], kinds=["integer", "continuous"], sense=sense)
    for enabled in (False, True):
        result = solve(m, backend="native", backend_options=NativeOptions(objective_lattice=enabled))
        assert result.status == "optimal"
        assert result.evaluation.feasible_candidate
        assert result.evaluation.objective == pytest.approx(sign)
        assert result.best_bound == pytest.approx(sign)


@pytest.mark.parametrize("shape,sign,expected", [
    ("cover", 1., None), ("cover", -1., None),
    ("packing", 1., None), ("packing", -1., None),
    ("weighted_cover", 1., "gmi_mir"),
    ("sparse_packing", 1., "gmi_mir"),
    ("balance", 1., "gmi"), ("balance", -1., "gmi"),
    ("unlinked", 1., None),
])
def test_adaptive_cut_selection_uses_row_structure(shape, sign, expected):
    if shape in ("balance", "unlinked"):
        rows = [[-3., 1., 0.], [0., 1., -1.]]
        lo, hi = [-math.inf, 1.], [0., 1.]
        if shape == "unlinked":
            rows[0][0] = 3.
        cost, upper, kinds = [1., 0., 1.], [1., 3., 2.], ["binary", "continuous", "continuous"]
    else:
        cost, upper, kinds = [1.] * 16, [1.] * 16, ["binary"] * 16
        rows = [[1.] * 16]
        if "cover" in shape:
            lo, hi = [2.], [math.inf]
            if shape == "weighted_cover":
                rows[0][0] = 2.
        else:
            lo, hi = [-math.inf], [2.5]
            if shape == "sparse_packing":
                rows[0][7:] = [0.] * 9
    if sign < 0:
        rows = [[-v for v in row] for row in rows]
        lo, hi = [-v for v in hi], [-v for v in lo]
    m = make(c=cost, a=rows, up=upper, rl=lo, ru=hi, kinds=kinds, sense="minimize")
    opts = NativeOptions(adaptive_root=True, mip_presolve=False, coefficient_tightening=False,
                         root_propagation=False, singleton_presolve=False, decompose=False)
    r = solve(m, backend="native", backend_options=opts)
    assert r.backend.effective_options.get("diagnostics.adaptive_root_family") == expected
    assert r.status == ("infeasible" if shape == "unlinked" else "optimal")

@pytest.mark.parametrize("sense,sign", [("maximize", 1.), ("minimize", -1.)])
def test_mixed_lattice_tightens_limited_bound_and_preserves_node_work(sense, sign):
    from dataclasses import replace

    m = make(c=[sign, 0.], a=[[1., 1.]], lo=[0., 0.25], up=[2., 1.],
             ru=[1.75], kinds=["integer", "continuous"], sense=sense)
    opts = NativeOptions(presolve=False, node_limit=1, incumbent_heuristic=False,
                         rational_certification=False)
    base, tight = [solve(m, backend="native", backend_options=replace(opts, objective_lattice=v))
                   for v in (False, True)]
    assert base.status == tight.status == "node_limit"
    assert base.nodes == tight.nodes and base.iterations == tight.iterations
    assert base.candidate == tight.candidate
    assert base.best_bound == pytest.approx(sign * 1.5)
    assert tight.best_bound == pytest.approx(sign)


def test_integer_equality_presolve_matches_enumeration():
    import itertools
    import random
    rng = random.Random(20261127)
    feasible = 0
    for _ in range(300):
        n = rng.randint(2, 5)
        lo = [rng.randint(-6, 2) for _ in range(n)]
        up = [l + rng.randint(0, 6) for l in lo]
        rows, rhs = [], []
        for _ in range(rng.randint(1, 3)):
            row = [0] * n
            for j in rng.sample(range(n), rng.randint(2, min(n, 4))):
                row[j] = rng.choice((1, 2, 3, 4, 6, 10, 12, 15, 21)) * rng.choice((-1, 1))
            point = [rng.randint(l, u) for l, u in zip(lo, up)]
            rows.append(row)
            rhs.append(sum(a * x for a, x in zip(row, point)) + rng.choice((0, 0, 0, 1, -2, 5)))
        c = [rng.randint(-5, 5) for _ in range(n)]
        values = [sum(a * b for a, b in zip(c, x))
                  for x in itertools.product(*[range(l, u + 1) for l, u in zip(lo, up)])
                  if all(sum(a * b for a, b in zip(row, x)) == r for row, r in zip(rows, rhs))]
        m = make(c=[float(v) for v in c], a=[[float(v) for v in row] for row in rows],
                 lo=[float(v) for v in lo], up=[float(v) for v in up],
                 rl=[float(v) for v in rhs], ru=[float(v) for v in rhs],
                 kinds=["integer"] * n, sense="minimize")
        for opts in (NativeOptions(integer_equality_presolve=True),
                     NativeOptions(integer_equality_presolve=True, adaptive_root=True)):
            r = solve(m, backend="native", backend_options=opts)
            if values:
                assert r.status == "optimal" and r.evaluation.feasible_candidate
                assert r.evaluation.objective == min(values)
            else:
                assert r.status == "infeasible"
        feasible += bool(values)
    assert 50 < feasible < 250


@pytest.mark.parametrize("sense,sign", [("maximize", 1.), ("minimize", -1.)])
def test_limited_mixed_run_reports_the_rounded_bound_and_keeps_its_limit_status(sense, sign):
    # The tree stops at its node limit with the incumbent 1 and an open bound
    # of 1.5. Rounding the returned bound closes the gap; the status still
    # names the limit, as docs/api.md states.
    m = make(c=[sign, 0.], a=[[1., 1.]], lo=[0., 0.25], up=[2., 1.],
             ru=[1.75], kinds=["integer", "continuous"], sense=sense)
    r = solve(m, backend="native", backend_options=NativeOptions(presolve=False, node_limit=1))
    assert r.status == "node_limit" and r.candidate == [1., 0.25]
    assert r.evaluation.objective == sign == r.best_bound
    loose = solve(m, backend="native",
                  backend_options=NativeOptions(presolve=False, node_limit=1, objective_lattice=False))
    assert loose.status == "node_limit" and loose.best_bound == pytest.approx(1.5 * sign)
# Each model is feasible only up to rounding of its decimal data or of a
# computed right-hand side; the result check accepts the point by eight orders
# of magnitude. 0.32.0 answered infeasible on the first, fourth and fifth, a
# worse objective on the second and third and numerical_failure on the sixth.
@pytest.mark.parametrize("c,a,lo,up,rl,ru,best", [
    ((4., 0.), ((15., 0.1),), (-4., -2.), (2., 0.), (29.8,), (29.8,), 8.),
    ((3., 1.), ((-5., -0.1),), (-2., -1.), (-2., 3.), (-math.inf,), (10.1,), -7.),
    ((-1., 1.), ((-0.1, -7.),), (0., -6.), (4., -6.), (41.6,), (math.inf,), -10.),
    ((2., -5., 3., -1.), ((5., 0., 0., 1.), (-0.9, 0., 0.3, 0.)), (0., 2., -1., 0.), (1., 4., 5., 1.),
     (0., 0.3 * 3 - 0.9), (0., 0.3 * 3 - 0.9), -20.),
    ((4., 0.), ((3., -2.),), (1., -1.), (7., 5.), (2.0000000000001,), (2.0000000000001,), 8.),
    ((3., -1.), ((-0.2, -2.),), (2., -2.), (2., 1.), (-0.39999999999990005,), (-0.39999999999990005,), 6.),
])
def test_integer_points_within_the_feasibility_tolerance_survive_presolve(c, a, lo, up, rl, ru, best):
    m = make(c=c, a=a, lo=lo, up=up, rl=rl, ru=ru, kinds=["integer"] * len(c), sense="minimize")
    for opts in (NativeOptions(), NativeOptions(presolve=False),
                 NativeOptions(adaptive_root=True, integer_equality_presolve=True)):
        r = solve(m, backend="native", backend_options=opts)
        assert r.status == "optimal"
        assert r.evaluation.feasible_candidate
        assert r.evaluation.objective == pytest.approx(best, abs=1e-9)
        assert r.best_bound <= best + 1e-6


@pytest.mark.parametrize(
    "sense, sign, cost, offset, expected",
    [("minimize", 1., [1., -3.], 7., 6.),
     ("maximize", -1., [1., 3.], 7., 8.)],
)
def test_certified_start_for_missing_mip_bounds(sense, sign, cost, offset, expected):
    lower = [0., 0.] if sign > 0 else [-math.inf, 0.]
    upper = [math.inf, 1.] if sign > 0 else [0., 1.]
    model = make(c=cost, a=[[sign, -1.]], lo=lower, up=upper,
                 rl=[1.], ru=[math.inf], kinds=["continuous", "binary"],
                 sense=sense, offset=offset)
    start = [2. * sign, 1.]
    with pytest.raises(NotImplementedError):
        solve(model, backend="native", start=start)
    settings = NativeOptions(objective_cutoff_bounds=True)
    result = solve(model, backend="native", backend_options=settings, start=start)
    assert result.status == "optimal"
    assert result.candidate == start
    assert result.evaluation.feasible_candidate
    assert result.evaluation.objective == expected
    assert result.best_bound == pytest.approx(expected)
    assert result.backend.effective_options["diagnostics.objective_cutoff_sides"] == "1"


def test_missing_mip_bounds_require_certification_and_keep_limited_incumbent():
    model = make(c=[1., 1.], a=[[1., -1.]], up=[math.inf, 1.],
                 rl=[1.], ru=[math.inf], kinds=["continuous", "binary"],
                 sense="minimize")
    settings = NativeOptions(objective_cutoff_bounds=True, node_limit=1)
    # The point is within feasibility tolerance of the original lower row.
    # The exact stored row rules out using it to cap the improving search, so
    # the fixed-setup LP supplies the cap, as it does without a start. The
    # point lies in the capped model and remains the start of the search.
    near = [2. - 2**-30, 1.]
    result = solve(model, backend="native", backend_options=settings, start=near)
    assert result.status == "node_limit"
    assert result.candidate == near
    assert result.backend.start_status == "evaluated"
    assert result.backend.effective_options["diagnostics.objective_cutoff_sides"] == "1"
    assert "diagnostics.objective_cutoff_start_pivots" in result.backend.effective_options
    # Without the fixed-setup LP nothing certifies a cap. A negative cost
    # makes the model ineligible for it.
    rewarded = make(c=[1., -1.], a=[[1., -1.]], up=[math.inf, 1.],
                    rl=[1.], ru=[math.inf], kinds=["continuous", "binary"],
                    sense="minimize")
    for start in (near, None):
        with pytest.raises(NotImplementedError):
            solve(rewarded, backend="native", backend_options=settings, start=start)
    assert solve(rewarded, backend="native", backend_options=settings,
                 start=[2., 1.]).status == "optimal"
    result = solve(model, backend="native", backend_options=settings)
    assert result.status == "node_limit"
    assert result.nodes == 1
    assert result.candidate == [2., 1.]
    assert result.evaluation.objective == 3.
    assert result.backend.start_status == "not_provided"
    assert result.iterations == int(result.backend.effective_options["diagnostics.objective_cutoff_start_pivots"])
    result = solve(model, backend="native",
                   backend_options=NativeOptions(objective_cutoff_bounds=True))
    assert result.status == "optimal"
    assert result.candidate == [1., 0.]


def test_start_outside_the_objective_caps_is_replaced_by_the_certified_point():
    model = make(c=[1., 1.], a=[[1., -1.]], up=[math.inf, 1.],
                 rl=[1.], ru=[math.inf], kinds=["continuous", "binary"],
                 sense="minimize")
    for limit, status, point in ((1, "node_limit", [2., 1.]), (None, "optimal", [1., 0.])):
        settings = NativeOptions(objective_cutoff_bounds=True, node_limit=limit)
        result = solve(model, backend="native", backend_options=settings, start=[0., 1.])
        assert (result.status, result.candidate) == (status, point)
        assert result.evaluation.feasible_candidate
        assert result.backend.start_status == "rejected"
        assert result.backend.effective_options["diagnostics.objective_cutoff_start_pivots"] == "1"
    # A certified start supplies the cap itself; no LP runs.
    result = solve(model, backend="native",
                   backend_options=NativeOptions(objective_cutoff_bounds=True), start=[9., 1.])
    assert result.status == "optimal" and result.candidate == [1., 0.]
    assert result.backend.start_status == "evaluated"
    assert "diagnostics.objective_cutoff_start_pivots" not in result.backend.effective_options


def test_missing_zero_cost_mip_domain_cannot_be_capped():
    model = make(c=[0., 1.], a=[[1., -1.]], up=[math.inf, 1.],
                 rl=[1.], ru=[math.inf], kinds=["continuous", "binary"],
                 sense="minimize")
    with pytest.raises(NotImplementedError):
        solve(model, backend="native", backend_options=NativeOptions(objective_cutoff_bounds=True),
              start=[1., 0.])
