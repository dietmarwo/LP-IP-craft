import concurrent.futures
import numpy as np
import pytest
import lp_ip_craft as lp


def planted(n=257, salt=0):
    i, j = np.indices((n, n))
    a = ((i * 37 + j * 19 + salt) % 17 + 1) / 16.0
    a[i == j] = 2 * n
    y = 1 + np.arange(n) % 5
    c = y @ a
    m = lp.Model(
        objective=c,
        lower=np.zeros(n),
        upper=np.full(n, 2.0),
        sense="maximize",
        row_lower=np.full(n, -np.inf),
        row_upper=a.sum(axis=1),
        indptr=np.arange(0, n * n + 1, n),
        indices=np.tile(np.arange(n), n),
        values=a.ravel(),
    )
    return m, float(c.sum())


@pytest.mark.parametrize("threads", [1, 2, 4, 8])
def test_numeric_workers_keep_original_lp_certificate(threads):
    m, expected = planted(193)
    r = lp.solve(
        m,
        backend="native",
        options=lp.Options(threads=threads),
        backend_options=lp.NativeOptions(decompose=False, component_threads=1),
    )
    assert r.status == "optimal" and r.evaluation.feasible_candidate
    assert r.evaluation.objective == pytest.approx(expected, abs=1e-6)
    q = lp.lp_residuals(m, r.candidate, r.duals)
    assert q.primal <= 1e-7 and q.duality_gap <= 1e-7
    assert r.backend.effective_options["numeric_threads"] == str(threads)


@pytest.mark.parametrize("workers,inner", [(1, 8), (2, 4), (4, 2), (8, 1), (16, 1)])
def test_mixed_batch_uses_one_explicit_budget(workers, inner):
    m, e = planted(32)
    out = lp.solve_batch([m] * 17, workers=workers, options=lp.Options(threads=inner))
    assert len(out) == 17 and all(r.status == "optimal" for r in out)
    assert all(r.evaluation.objective == pytest.approx(e, abs=1e-6) for r in out)


@pytest.mark.parametrize("workers,inner", [(2, 8), (4, 4), (8, 2), (16, 2)])
def test_mixed_batch_rejects_oversubscription(workers, inner):
    with pytest.raises(ValueError, match="must not exceed 8"):
        lp.solve_batch([], workers=workers, options=lp.Options(threads=inner))


def test_independent_python_callers_recover_after_cancel_and_errors():
    model, expected = planted()

    def work(threads):
        options = lp.Options(threads=threads)
        settings = lp.NativeOptions(decompose=False, component_threads=1)
        token = lp.Cancellation()
        token.cancel()
        assert (
            lp.solve(
                model, backend="native", options=options, cancellation=token
            ).status
            == "cancelled"
        )
        with pytest.raises(ValueError):
            lp.solve(
                model, backend="native", options=options, start=[float("nan")] * 257
            )
        r = lp.solve(model, backend="native", options=options, backend_options=settings)
        assert r.status == "optimal" and r.evaluation.feasible_candidate
        return r.evaluation.objective

    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as ex:
        out = list(ex.map(work, [1, 1, 2, 4]))
    assert all(v == pytest.approx(expected, abs=1e-6) for v in out)
