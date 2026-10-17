//! Opt-in, bounded observations. Nothing here supplies search decisions.
use lp_ip_craft::{Gap, Model, Sense, SolveResult, Status};
use std::{fmt::Write, time::Instant};

#[derive(Clone, Copy, Default)]
pub(crate) struct PropagationStats {
    pub eligible_rows: usize,
    pub eligible_terms: usize,
    pub rows_dequeued: u64,
    pub terms_examined: u64,
    pub slack_skips: u64,
    pub bound_changes: u64,
    pub work_limit_exits: u64,
    pub infeasibilities: u64,
}

#[derive(Clone, Copy)]
struct Milestone {
    seconds: f64,
    nodes: u64,
    pivots: u64,
    objective: f64,
    proof: Option<f64>,
}

struct Event {
    kind: &'static str,
    source: &'static str,
    at: Milestone,
    tree_nodes: u64,
    open_nodes: usize,
    depth: usize,
    certified: Option<f64>,
    global_bound: Option<f64>,
}

pub(crate) struct SearchDiagnostics {
    started: Instant,
    sense: Sense,
    limit: usize,
    events: Vec<Event>,
    dropped: u64,
    first_candidate: Option<Milestone>,
    first_certified: Option<Milestone>,
    candidate: Option<f64>,
    certified: Option<f64>,
    global_bound: Option<f64>,
    root_status: &'static str,
    root_objective: Option<f64>,
    root_bound: Option<f64>,
    root_fractional: Option<usize>,
    root_pivots: u64,
    root_seconds: f64,
    current_open: usize,
    current_depth: usize,
    pub tree_nodes: u64,
    pub max_depth: usize,
    pub max_open: usize,
    pub node_lp_solves: u64,
    pub node_lp_pivots: u64,
    pub node_lp_seconds: f64,
    pub inherited_prunes: u64,
    pub solved_prunes: u64,
    pub infeasible_lps: u64,
    pub integral_leaves: u64,
    pub heuristic_calls: u64,
    pub rounding_attempts: u64,
    pub accepted_candidates: u64,
    pub candidate_improvements: u64,
    pub certified_improvements: u64,
    pub original_check_rejections: u64,
}

fn number(value: Option<f64>) -> String {
    value
        .filter(|v| v.is_finite())
        .map_or_else(|| "null".into(), |v| v.to_string())
}
fn milestone(value: Option<Milestone>) -> String {
    value.map_or_else(|| "null".into(), |v| format!(
        "{{\"elapsed_seconds\":{},\"nodes\":{},\"pivots\":{},\"objective\":{},\"certified_objective_bound\":{}}}",
        v.seconds, v.nodes, v.pivots, number(Some(v.objective)), number(v.proof)))
}

impl SearchDiagnostics {
    pub fn new(model: &Model, started: Instant, limit: usize) -> Self {
        Self {
            started,
            sense: model.sense(),
            limit,
            events: Vec::new(),
            dropped: 0,
            first_candidate: None,
            first_certified: None,
            candidate: None,
            certified: None,
            global_bound: None,
            root_status: "not_solved",
            root_objective: None,
            root_bound: None,
            root_fractional: None,
            root_pivots: 0,
            root_seconds: 0.,
            current_open: 0,
            current_depth: 0,
            tree_nodes: 0,
            max_depth: 0,
            max_open: 0,
            node_lp_solves: 0,
            node_lp_pivots: 0,
            node_lp_seconds: 0.,
            inherited_prunes: 0,
            solved_prunes: 0,
            infeasible_lps: 0,
            integral_leaves: 0,
            heuristic_calls: 0,
            rounding_attempts: 0,
            accepted_candidates: 0,
            candidate_improvements: 0,
            certified_improvements: 0,
            original_check_rejections: 0,
        }
    }
    fn at(&self, nodes: u64, pivots: u64, objective: Option<f64>, proof: Option<f64>) -> Milestone {
        Milestone {
            seconds: self.started.elapsed().as_secs_f64(),
            nodes,
            pivots,
            objective: objective.unwrap_or(f64::NAN),
            proof,
        }
    }
    fn event(&mut self, kind: &'static str, source: &'static str, at: Milestone) {
        if self.events.len() < self.limit {
            self.events.push(Event {
                kind,
                source,
                at,
                tree_nodes: self.tree_nodes,
                open_nodes: self.current_open,
                depth: self.current_depth,
                certified: self.certified,
                global_bound: self.global_bound,
            });
        } else {
            self.dropped += 1;
        }
    }
    pub fn enter_node(&mut self, depth: usize, open: usize) {
        self.tree_nodes += 1;
        self.current_depth = depth;
        self.current_open = open;
        self.max_depth = self.max_depth.max(depth);
        self.max_open = self.max_open.max(open);
    }
    pub fn checkpoint(&mut self, nodes: u64, pivots: u64, bound: Option<f64>, open: usize) {
        let changed = self.global_bound != bound;
        self.global_bound = bound;
        self.current_open = open;
        self.max_open = self.max_open.max(open);
        if changed || nodes == 0 || nodes.is_power_of_two() {
            self.event(
                "progress",
                "frontier",
                self.at(nodes, pivots, self.candidate, self.certified),
            );
        }
    }
    pub fn root_lp(&mut self, status: Status, pivots: u64, seconds: f64) {
        self.root_status = status.as_str();
        self.root_pivots = pivots;
        self.root_seconds = seconds;
    }
    pub fn root_solution(
        &mut self,
        nodes: u64,
        pivots: u64,
        objective: f64,
        bound: Option<f64>,
        fractional: usize,
    ) {
        self.root_objective = Some(objective);
        self.root_bound = bound;
        self.root_fractional = Some(fractional);
        self.event(
            "root_relaxation",
            "node_lp",
            self.at(nodes, pivots, Some(objective), bound),
        );
    }
    pub fn candidate(
        &mut self,
        objective: f64,
        proof: Option<f64>,
        source: &'static str,
        nodes: u64,
        pivots: u64,
    ) {
        self.accepted_candidates += 1;
        let improves = self.candidate.is_none_or(|old| match self.sense {
            Sense::Minimize => objective < old,
            Sense::Maximize => objective > old,
        });
        if improves {
            self.candidate = Some(objective);
            self.candidate_improvements += 1;
            let at = self.at(nodes, pivots, Some(objective), proof);
            self.first_candidate.get_or_insert(at);
            self.event("accepted_candidate", source, at);
        }
        if let Some(proof) = proof {
            self.certified = Some(proof);
            self.certified_improvements += 1;
            let at = self.at(nodes, pivots, Some(objective), Some(proof));
            self.first_certified.get_or_insert(at);
            self.event("certified_incumbent", source, at);
        }
    }
    pub fn finish(
        mut self,
        result: &mut SolveResult,
        propagation: Option<PropagationStats>,
        original: &Model,
        search_model: &Model,
    ) {
        self.global_bound = result.best_bound;
        let terminal = self.at(
            result.nodes,
            result.iterations,
            result.evaluation.as_ref().map(|e| e.objective),
            self.certified,
        );
        self.event("terminal", result.status.as_str(), terminal);
        let p = propagation.unwrap_or_default();
        let mut json = String::new();
        write!(json, "{{\"schema_version\":1,\"scope\":\"serial_search\",\"time_origin\":\"after_root_preprocessing\",\"sense\":\"{}\",\"original_rows\":{},\"search_rows\":{},\"original_columns\":{},\"search_columns\":{},", if self.sense == Sense::Minimize { "minimize" } else { "maximize" }, original.m(), search_model.m(), original.n(), search_model.n()).unwrap();
        write!(json, "\"root\":{{\"status\":\"{}\",\"lp_objective\":{},\"conservative_bound\":{},\"fractional_integers\":{},\"lp_pivots\":{},\"lp_seconds\":{}}},", self.root_status, number(self.root_objective), number(self.root_bound), self.root_fractional.map_or_else(|| "null".into(), |v| v.to_string()), self.root_pivots, self.root_seconds).unwrap();
        write!(
            json,
            "\"first_accepted_candidate\":{},\"first_certified_incumbent\":{},",
            milestone(self.first_candidate),
            milestone(self.first_certified)
        )
        .unwrap();
        write!(json, "\"totals\":{{\"tree_nodes_visited\":{},\"max_depth\":{},\"max_open_nodes\":{},\"node_lp_solves\":{},\"node_lp_pivots\":{},\"node_lp_seconds\":{},\"other_lp_pivots\":{},\"inherited_bound_prunes\":{},\"solved_bound_prunes\":{},\"infeasible_node_lps\":{},\"integral_leaves\":{},\"heuristic_calls\":{},\"rounding_attempts\":{},\"accepted_candidates\":{},\"candidate_improvements\":{},\"certified_improvements\":{},\"original_check_rejections\":{}}},", self.tree_nodes, self.max_depth, self.max_open, self.node_lp_solves, self.node_lp_pivots, self.node_lp_seconds, result.iterations.saturating_sub(self.node_lp_pivots), self.inherited_prunes, self.solved_prunes, self.infeasible_lps, self.integral_leaves, self.heuristic_calls, self.rounding_attempts, self.accepted_candidates, self.candidate_improvements, self.certified_improvements, self.original_check_rejections).unwrap();
        write!(json, "\"propagation\":{{\"enabled\":{},\"eligible_rows\":{},\"eligible_terms\":{},\"ineligible_rows\":{},\"rows_dequeued\":{},\"terms_examined\":{},\"slack_skips\":{},\"bound_changes\":{},\"work_limit_exits\":{},\"infeasibilities\":{}}},", propagation.is_some(), p.eligible_rows, p.eligible_terms, search_model.m().saturating_sub(p.eligible_rows), p.rows_dequeued, p.terms_examined, p.slack_skips, p.bound_changes, p.work_limit_exits, p.infeasibilities).unwrap();
        write!(json, "\"terminal\":{{\"status\":\"{}\",\"nodes\":{},\"pivots\":{},\"elapsed_seconds\":{},\"returned_objective\":{},\"best_bound\":{},\"certified_incumbent_bound\":{},{} }},", result.status.as_str(), result.nodes, result.iterations, terminal.seconds, number(result.evaluation.as_ref().map(|e| e.objective)), number(result.best_bound), number(self.certified), self.gap(self.certified, result.best_bound)).unwrap();
        write!(
            json,
            "\"event_limit\":{},\"events_dropped\":{},\"truncated\":{},\"events\":[",
            self.limit,
            self.dropped,
            self.dropped > 0
        )
        .unwrap();
        for (i, event) in self.events.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            write!(json, "{{\"kind\":\"{}\",\"source\":\"{}\",\"elapsed_seconds\":{},\"nodes\":{},\"pivots\":{},\"tree_nodes_visited\":{},\"open_nodes\":{},\"depth\":{},\"objective\":{},\"certified_incumbent_bound\":{},\"global_bound\":{},{} }}", event.kind, event.source, event.at.seconds, event.at.nodes, event.at.pivots, event.tree_nodes, event.open_nodes, event.depth, number(Some(event.at.objective)), number(event.certified), number(event.global_bound), self.gap(event.certified, event.global_bound)).unwrap();
        }
        json.push_str("]}");
        result
            .backend
            .effective_options
            .push(("diagnostics.mip_search".into(), json));
    }
    fn gap(&self, incumbent: Option<f64>, bound: Option<f64>) -> String {
        let gap = incumbent
            .zip(bound)
            .map(|(i, b)| Gap::new(self.sense, i, b));
        format!(
            "\"absolute_gap\":{},\"relative_gap\":{}",
            number(gap.map(|g| g.absolute)),
            number(gap.map(|g| g.relative))
        )
    }
}
