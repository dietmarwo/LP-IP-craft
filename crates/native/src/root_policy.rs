//! Work-bounded selection among the existing certified cut families.
use crate::{NativeOptions, WarmStart};
use lp_ip_craft::{Model, Options};

pub(crate) struct Policy {
    pub family: &'static str,
    pub families: [bool; 2],
    pub rounds: usize,
}

impl Policy {
    pub fn choose(model: &Model, settings: &NativeOptions) -> Option<Self> {
        if !settings.adaptive_root
            || !model.is_mip()
            || settings.gmi_cuts
            || settings.mir_cuts
            || settings.dual_presolve
            || settings.root_cuts
            || settings.root_separation
            || settings.root_heuristics
            || settings.packing_root
            || settings.packing_neighborhoods > 0
            || settings.packing_cuts
            || settings.tree_threads != 1
            || settings.mip_cut_rounds == 0
            || settings.cut_exact_work == 0
            || settings.node_limit.is_some_and(|n| n < 4)
            || settings.iteration_limit.is_some_and(|n| n < 64)
            || model.n() > 1024
            || model.m() > 1024
            || model.nnz() > 8192
            || model
                .variables()
                .lower
                .iter()
                .chain(&model.variables().upper)
                .any(|x| !x.is_finite() || x.abs() > 9_007_199_254_740_991.)
        {
            return None;
        }
        let (mut binary, mut general, mut continuous) = (0, 0, 0);
        let bounds = model.variables();
        for (j, kind) in model.kinds().iter().enumerate() {
            let (l, u) = (bounds.lower[j], bounds.upper[j]);
            if l == u {
                continue;
            }
            if !kind.is_integral() {
                continuous += 1;
            } else if l >= 0. && u <= 1. {
                binary += 1;
            } else {
                general += 1;
            }
        }
        if continuous == 0 && binary + general >= 16 {
            if general == 0 && prefer_uncut_binary_root(model, binary) {
                return None;
            }
            Some(Self {
                family: if general > 0 { "gmi" } else { "gmi_mir" },
                families: [true, general == 0],
                rounds: settings.mip_cut_rounds.min(16),
            })
        } else if binary > 0 && general == 0 && continuous > 0 && has_linked_balances(model) {
            Some(Self {
                family: "gmi",
                families: [true, false],
                rounds: settings.mip_cut_rounds.min(16),
            })
        } else if binary > 0 && general > 0 && continuous > 0 {
            // One round limits the cost of generating dense cuts on mixed
            // domains, where cut accumulation can slow node reoptimization.
            Some(Self {
                family: "gmi",
                families: [true, false],
                rounds: 1,
            })
        } else {
            None
        }
    }

    pub fn budget(settings: &NativeOptions) -> NativeOptions {
        let mut root = settings.clone();
        root.node_limit = root.node_limit.map(|n| n / 2);
        root.iteration_limit = root.iteration_limit.map(|n| n / 2);
        root.time_limit_seconds = root.time_limit_seconds.map(|t| t * 0.25);
        root
    }

    pub fn can_improve(
        model: &Model,
        point: &[f64],
        bound: Option<f64>,
        start: Option<&WarmStart>,
        options: &Options,
    ) -> bool {
        if !point
            .iter()
            .zip(model.kinds())
            .any(|(x, k)| k.is_integral() && (x - x.round()).abs() > options.tolerances.integrality)
        {
            return false;
        }
        if let (Some(b), Some(s)) = (bound, start) {
            if let Ok(e) = lp_ip_craft::check::evaluate(model, &s.primal, &options.tolerances) {
                if e.feasible_candidate
                    && crate::oriented(model, b) >= crate::oriented(model, e.objective)
                {
                    return false;
                }
            }
        }
        true
    }
}

/// Unit covering rows and dense packing rows commonly become expensive
/// tableau cuts. Classify only active rows, allowing either row orientation.
fn prefer_uncut_binary_root(model: &Model, binary: usize) -> bool {
    let matrix = model.matrix();
    let bounds = model.variables();
    let (mut cover, mut packing) = (true, true);
    let mut active = false;
    for i in 0..model.m() {
        let (lo, hi) = (model.rows().lower[i], model.rows().upper[i]);
        if !lo.is_finite() && !hi.is_finite() {
            continue;
        }
        if lo.is_finite() && hi.is_finite() {
            return false;
        }
        let mut unit = None;
        let mut count = 0;
        // In this orientation the row has a lower bound. Positive, equal
        // coefficients give a covering row, negative ones a packing row.
        let sign = if lo.is_finite() { 1. } else { -1. };
        for k in matrix.pattern.indptr[i]..matrix.pattern.indptr[i + 1] {
            let j = matrix.pattern.indices[k];
            let a = sign * matrix.values[k];
            if a == 0. || bounds.lower[j] == bounds.upper[j] {
                continue;
            }
            count += 1;
            cover &= a > 0. && *unit.get_or_insert(a) == a;
            packing &= a < 0.;
            if !cover && !packing {
                return false;
            }
        }
        if count > 0 {
            active = true;
            packing &= 2 * count >= binary;
        }
    }
    active && (cover || packing)
}

/// Short continuous flow balances coupled by binary on/off capacity rows.
/// A longer balance, an unrelated row or a general integer leaves the
/// ordinary mixed-model policy in control.
fn has_linked_balances(model: &Model) -> bool {
    let matrix = model.matrix();
    let (mut balances, mut links) = (0, 0);
    for i in 0..model.m() {
        let (lo, hi) = (model.rows().lower[i], model.rows().upper[i]);
        if !lo.is_finite() && !hi.is_finite() {
            continue;
        }
        let range = matrix.pattern.indptr[i]..matrix.pattern.indptr[i + 1];
        if lo == hi && (2..=3).contains(&range.len()) {
            let scale = matrix.values[range.start].abs();
            if scale > 0.
                && range.clone().all(|k| {
                    !model.kinds()[matrix.pattern.indices[k]].is_integral()
                        && matrix.values[k].abs() == scale
                })
            {
                balances += 1;
                continue;
            }
        }
        if range.len() == 2 && ((hi == 0. && !lo.is_finite()) || (lo == 0. && !hi.is_finite())) {
            let sign = if hi == 0. { 1. } else { -1. };
            let a = range.start;
            let b = a + 1;
            if [(a, b), (b, a)].into_iter().any(|(x, y)| {
                !model.kinds()[matrix.pattern.indices[x]].is_integral()
                    && model.kinds()[matrix.pattern.indices[y]].is_integral()
                    && sign * matrix.values[x] > 0.
                    && sign * matrix.values[y] < 0.
            }) {
                links += 1;
                continue;
            }
        }
        return false;
    }
    balances > 0 && links > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auxiliary_search_does_not_inherit_the_root_cut_policy() {
        let settings = NativeOptions {
            adaptive_root: true,
            ..Default::default()
        };
        assert!(
            !crate::packing::child_settings(&settings, std::time::Instant::now()).adaptive_root
        );
    }

    #[cfg(feature = "local-mip-bench")]
    #[test]
    fn cancellation_during_optional_root_processing_returns_to_the_parent() {
        use crate::local_mip_bench;
        use crate::{CancellationToken, NativeSolver, Status};
        use lp_ip_craft::{Bounds, CsrMatrix, ModelData, Sense, VariableKind};

        let n = 16;
        let mut weights = vec![2.; n];
        weights[0] = -2.;
        let model = Model::new(ModelData {
            sense: Sense::Maximize,
            objective: vec![1.; n],
            objective_offset: 0.,
            variables: Bounds::new(vec![0.; n], vec![1.; n]),
            kinds: vec![VariableKind::Binary; n],
            matrix: CsrMatrix::from_dense(1, n, &weights).unwrap(),
            rows: Bounds::new(vec![f64::NEG_INFINITY], vec![13.]),
        })
        .unwrap();
        let settings = NativeOptions {
            adaptive_root: true,
            decompose: false,
            node_limit: Some(8),
            ..Default::default()
        };
        let cancel = CancellationToken::default();
        let mut stopped_at_root = false;
        let result = local_mip_bench::solve(
            &mut NativeSolver::new(settings),
            &model,
            &Options::default(),
            &cancel,
            local_mip_bench::Controls::default(),
            |event| {
                if event.source == "presolve_root_lp" {
                    stopped_at_root = true;
                    cancel.cancel();
                }
            },
        )
        .unwrap();
        assert!(stopped_at_root);
        assert_eq!(result.status, Status::Cancelled);
        assert_eq!(result.nodes, 1);
    }
}
