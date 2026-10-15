from typing import Any, Iterable, Literal, Optional

class EvaluationError(RuntimeError):
    """Floating-point evaluation failed, for example a row activity overflowed."""

class Model:
    """A validated LP or mixed-integer LP; immutable and picklable.

    ``A`` is given in canonical zero-based CSR form: ``indptr`` has ``m + 1``
    entries from 0 to ``nnz``; each row's ``indices`` are strictly increasing
    and below ``n``. ``m`` is ``len(row_lower)``. ``kinds`` defaults to all
    ``"continuous"``.
    """

    def __init__(
        self,
        *,
        objective: Iterable[float],
        lower: Iterable[float],
        upper: Iterable[float],
        row_lower: Iterable[float],
        row_upper: Iterable[float],
        indptr: Iterable[int],
        indices: Iterable[int],
        values: Iterable[float],
        kinds: Optional[Iterable[Literal["continuous", "integer", "binary"]]] = None,
        sense: Literal["minimize", "maximize"] = "minimize",
        objective_offset: float = 0.0,
    ) -> None: ...
    @property
    def n(self) -> int: ...
    @property
    def m(self) -> int: ...
    @property
    def nnz(self) -> int: ...
    @property
    def is_mip(self) -> bool: ...
    @property
    def sense(self) -> Literal["minimize", "maximize"]: ...
    @property
    def objective(self) -> list[float]: ...
    @property
    def objective_offset(self) -> float: ...
    @property
    def lower(self) -> list[float]: ...
    @property
    def upper(self) -> list[float]: ...
    @property
    def kinds(self) -> list[Literal["continuous", "integer", "binary"]]: ...
    @property
    def row_lower(self) -> list[float]: ...
    @property
    def row_upper(self) -> list[float]: ...
    @property
    def indptr(self) -> list[int]: ...
    @property
    def indices(self) -> list[int]: ...
    @property
    def values(self) -> list[float]: ...

class Cancellation:
    """Cooperative cancellation flag; checked at the solver's checkpoints."""

    def __init__(self) -> None: ...
    def cancel(self) -> None: ...
    @property
    def cancelled(self) -> bool: ...

def solve_dummy(
    model: Model,
    start: Optional[Iterable[float]],
    feasibility_tolerance: float,
    integrality_tolerance: float,
    threads: int,
    cancellation: Cancellation,
) -> dict[str, Any]: ...
def evaluate(
    model: Model,
    x: Iterable[float],
    feasibility_tolerance: float,
    integrality_tolerance: float,
) -> dict[str, Any]: ...
