# Native iteration 4 — parent-state reuse and dual simplex

Version 0.8.0. Keep a shared immutable snapshot of the parent dictionary in
pending branch nodes. Add each child's branching inequality to that dictionary
and use dual simplex to restore primal feasibility while retaining dual
feasibility. Keep original row multipliers and account for branch bounds
through reduced costs. A child is always checked against its own original-unit
bounds and the unchanged model rows.

Add an opt-out switch and a bounded cold fallback for numerical or
infeasibility outcomes, including independently rejected warm results.
Limits/cancellation must propagate without retry. Track cold solves, warm
nodes and fallbacks. Keep the existing tableau cap, including appended rows.
Compare cold and reoptimized search against exhaustive integer optima and
the 400-case independent oracle. Report both pivots and runtime; node counts
can change with alternative degenerate LP optima.
