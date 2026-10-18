//! The automorphism group of the model, and the column orbits it induces.
//!
//! Two columns are interchangeable when some relabelling of the whole model carries
//! one onto the other and leaves the model itself unchanged. The cheap version of that
//! -- two columns with identical matrix entries and objective -- reaches nothing on the
//! models this exists for: `cod105` and `graph20-20-1rand` have exactly zero duplicate
//! columns between them and are still solved by symmetry handling and by nothing else.
//! What they have is a genuine automorphism group of the coefficient matrix, so that is
//! what this computes.
//!
//! # The graph
//!
//! Rows and columns become vertices and nonzeros become edges, which turns "relabelling
//! that leaves the model unchanged" into "automorphism of a vertex-coloured graph". The
//! colouring carries everything an automorphism is *not* allowed to exchange:
//!
//! - a row's two bounds, so a `<= 3` row cannot become a `<= 4` row;
//! - a column's objective coefficient, bounds and integrality;
//! - the value of a coefficient, which is an edge label rather than a vertex one.
//!
//! Edge labels are handled by subdividing: a nonzero whose value is not the model's
//! commonest gets a vertex of its own, coloured by that value, sitting between its row
//! and its column. Nonzeros at the commonest value stay as plain edges. That keeps the
//! graph near `nnz` vertices rather than `2 * nnz` on the usual all-ones matrix, and it
//! is sound because the three kinds of vertex are in disjoint colour classes: no
//! automorphism can exchange a subdivided edge for a plain one, or a row for a column.
//!
//! Getting this colouring wrong in the permissive direction makes every reduction built
//! on it unsound, so coefficient values are compared by their exact bit pattern rather
//! than within a tolerance. Two coefficients a hair apart are then held to be different,
//! which costs symmetries and cannot invent one.
//!
//! # Finding the generators
//!
//! Individualization and refinement, which is the nauty family of algorithm. Colour
//! refinement splits cells until the partition is equitable; where it stalls, a vertex
//! is individualized and the search branches. Two leaves of that search that agree on
//! every invariant along the way differ by a candidate automorphism.
//!
//! This is deliberately *not* a complete implementation of nauty. It does not compute a
//! canonical form, and it prunes in ways that can lose generators. That is the right
//! trade here because of an asymmetry in what the answer is used for: **every generator
//! is checked against the graph before it is returned**, so anything reported is a true
//! automorphism and everything built on it stays sound. Missing a generator costs
//! reduction and nothing else. So the search can be cut off by a budget, mid-way, and
//! what it has found so far is still usable.

use crate::model::Problem;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

/// What the automorphism search is allowed to spend.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Total work -- see [`Budget::stall_work`] for what a unit of it is -- after which
    /// the search stops wherever it has got to.
    ///
    /// The cap that decides what detection costs on a model that has a group and keeps
    /// finding more of it. `cod105` is answered completely inside 8 million.
    pub max_work: usize,
    /// Wall clock, checked every few hundred nodes.
    pub time: Option<Duration>,
    /// Vertices above which the graph is not built at all.
    ///
    /// Refinement is roughly linear in the edge count per splitter, so a model with
    /// millions of nonzeros can spend a long time proving it has no symmetry. Models
    /// that large are not where this pays.
    pub max_vertices: usize,
    /// Work to keep spending after the largest column orbit last grew.
    ///
    /// A unit of work is an edge walked by the refinement or a vertex copied when the
    /// search branches, and both are counted because on this set either can dominate:
    /// `ex10` spends 185 million walking edges before the search starts, and
    /// `neos-787933` spends almost none but branches ten thousand times over a partition
    /// of 240 thousand vertices. A budget in nodes prices neither.
    ///
    /// This is the budget that matters and the three above are backstops behind it. Two
    /// things are wrong with counting nodes instead, and they are the two mistakes this
    /// project keeps making about budgets.
    ///
    /// A node is not a unit of work. Refinement walks the edges of whatever cells it
    /// splits by, so a node on `nw04` at 636k nonzeros costs what a hundred nodes cost
    /// on `cod105`, and a node budget hands the large model a hundred times the work for
    /// the same answer.
    ///
    /// And a generator is not a unit of progress. `irp` yields 257 generators and its
    /// largest orbit is three columns wide; `ex9` yields 79 and its largest is two.
    /// Neither is worth branching on, and a budget that resets on every generator
    /// searches both of them to the end of the clock. What the search is *for* is an
    /// orbit wide enough to collapse a tree, so that is what has to be growing for it to
    /// be worth continuing.
    pub stall_work: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_work: 400_000_000,
            time: Some(Duration::from_secs(5)),
            max_vertices: 500_000,
            stall_work: 150_000_000,
        }
    }
}

/// The column symmetry of a model.
#[derive(Clone, Debug, Default)]
pub struct Symmetry {
    /// Generators, each a permutation of the columns and each verified to be a genuine
    /// automorphism of the model. `generators[k][j]` is the image of column `j`.
    pub generators: Vec<Vec<u32>>,
    /// The columns each generator moves, sorted. Held alongside the generators because
    /// every node of the search asks two questions of it -- does this generator fix
    /// everything branched on so far, and which columns does it join up -- and both are
    /// answered from the support in time proportional to the support rather than to the
    /// model.
    pub supports: Vec<Vec<u32>>,
    /// Orbit representative of every column under the group the generators make: the
    /// lowest-numbered column reachable from it. A column alone in its orbit is its own
    /// representative.
    pub orbits: Vec<u32>,
    /// Columns moved by at least one generator.
    pub moved: usize,
    /// Individualizations performed. Reported so the cost can be read off.
    pub nodes: usize,
    /// Edges the refinement walked, which is what the search actually spent.
    pub work: usize,
    /// Vertices in the graph that was searched, or zero if it was never built.
    pub vertices: usize,
    /// Seconds spent building the graph, before any searching.
    pub build_seconds: f64,
    /// Whether the search ran out of budget. A search that stopped early may have found
    /// fewer generators than the model has; what it did find is still sound.
    pub exhausted_budget: bool,
}

impl Symmetry {
    /// Whether any column moves at all.
    pub fn is_trivial(&self) -> bool {
        self.generators.is_empty()
    }

    /// The orbits with more than one column in them, each as a sorted column list.
    pub fn nontrivial_orbits(&self) -> Vec<Vec<u32>> {
        let mut by_rep: std::collections::BTreeMap<u32, Vec<u32>> = Default::default();
        for (j, &rep) in self.orbits.iter().enumerate() {
            by_rep.entry(rep).or_default().push(j as u32);
        }
        by_rep.into_values().filter(|o| o.len() > 1).collect()
    }
}

/// Find generators of the model's column symmetry group.
///
/// Returns an empty [`Symmetry`] when the model is asymmetric, when it is larger than
/// the budget's vertex ceiling, or when the search found nothing before its budget ran
/// out. Callers cannot tell those apart from the generators alone and should not need
/// to: no symmetry and no symmetry found act the same way downstream.
pub fn detect(problem: &Problem, budget: Budget) -> Symmetry {
    let n_cols = problem.n_cols();
    let identity: Vec<u32> = (0..n_cols as u32).collect();
    let building = Instant::now();
    let Some(graph) = Graph::of_model(problem, budget.max_vertices) else {
        return Symmetry {
            orbits: identity,
            ..Default::default()
        };
    };

    let build_seconds = building.elapsed().as_secs_f64();
    let found = find_generators(&graph, budget);

    // A generator is only interesting for the columns it moves. One that permutes rows
    // and nothing else says something true about the model and nothing usable about its
    // solutions, so it is dropped rather than carried.
    let mut generators = Vec::new();
    for perm in &found.generators {
        let column: Vec<u32> = (0..n_cols)
            .map(|j| perm[graph.first_col + j] - graph.first_col as u32)
            .collect();
        if column
            .iter()
            .enumerate()
            .any(|(j, &image)| image != j as u32)
        {
            generators.push(column);
        }
    }

    let orbits = orbits_of(n_cols, &generators);
    let moved = (0..n_cols)
        .filter(|&j| generators.iter().any(|g| g[j] != j as u32))
        .count();
    let supports = generators
        .iter()
        .map(|g| (0..n_cols as u32).filter(|&j| g[j as usize] != j).collect())
        .collect();

    Symmetry {
        generators,
        supports,
        orbits,
        moved,
        nodes: found.nodes,
        work: found.work,
        vertices: graph.n,
        build_seconds,
        exhausted_budget: found.exhausted_budget,
    }
}

/// Orbit representatives under the group generated by `generators`, by union-find.
fn orbits_of(n: usize, generators: &[Vec<u32>]) -> Vec<u32> {
    let mut parent: Vec<u32> = (0..n as u32).collect();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    for g in generators {
        for (j, &image) in g.iter().enumerate() {
            let (a, b) = (find(&mut parent, j as u32), find(&mut parent, image));
            if a != b {
                // Point the higher representative at the lower, so an orbit's
                // representative is its lowest-numbered column.
                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                parent[hi as usize] = lo;
            }
        }
    }
    (0..n as u32).map(|j| find(&mut parent, j)).collect()
}

// ---------------------------------------------------------------------------
// The coloured graph
// ---------------------------------------------------------------------------

/// An undirected vertex-coloured graph in adjacency-list form.
///
/// Vertices are laid out rows first, then columns, then the subdivision vertices for
/// nonzeros whose coefficient is not the commonest, so a column index is recovered by
/// subtracting `first_col`.
struct Graph {
    n: usize,
    start: Vec<u32>,
    adj: Vec<u32>,
    colour: Vec<u32>,
    first_col: usize,
}

impl Graph {
    fn neighbours(&self, v: u32) -> &[u32] {
        let (s, e) = (
            self.start[v as usize] as usize,
            self.start[v as usize + 1] as usize,
        );
        &self.adj[s..e]
    }

    /// Build the coloured graph, or `None` if it would exceed `max_vertices`.
    fn of_model(problem: &Problem, max_vertices: usize) -> Option<Graph> {
        let n_rows = problem.n_rows();
        let n_cols = problem.n_cols();
        let matrix = &problem.matrix;

        // The commonest coefficient value becomes a plain edge and every other value
        // gets a subdivision vertex. On an all-ones matrix that is no extra vertices at
        // all, which is the case the two models this exists for are in.
        let mut value_counts: std::collections::HashMap<u64, usize> = Default::default();
        for c in 0..n_cols {
            let (_, values) = matrix.column(c);
            for &v in values {
                *value_counts.entry(bits(v)).or_default() += 1;
            }
        }
        // Ties broken by the bit pattern so the choice does not depend on hash order.
        let plain = value_counts
            .iter()
            .max_by_key(|&(&key, &count)| (count, Reverse(key)))
            .map(|(&key, _)| key);

        let subdivided = value_counts
            .iter()
            .filter(|&(&key, _)| Some(key) != plain)
            .map(|(_, &count)| count)
            .sum::<usize>();
        let n = n_rows + n_cols + subdivided;
        if n > max_vertices {
            return None;
        }

        // Colours. Rows, columns and each distinct subdivided value occupy disjoint
        // ranges of colour numbers, which is what stops an automorphism exchanging a row
        // for a column or a coefficient for either.
        let row_key = |r: usize| (bits(problem.row_lb[r]), bits(problem.row_ub[r]));
        let col_key = |c: usize| {
            (
                bits(problem.obj[c]),
                bits(problem.col_lb[c]),
                bits(problem.col_ub[c]),
                problem.is_integer(c),
            )
        };
        let row_classes = classify(n_rows, row_key);
        let col_classes = classify(n_cols, col_key);
        let mut value_classes: Vec<u64> = value_counts
            .keys()
            .copied()
            .filter(|&key| Some(key) != plain)
            .collect();
        value_classes.sort_unstable();
        let value_class = |key: u64| value_classes.binary_search(&key).unwrap() as u32;

        let row_base = 0u32;
        let col_base = row_base + row_classes.n_classes;
        let value_base = col_base + col_classes.n_classes;

        let mut colour = vec![0u32; n];
        for (r, slot) in colour[..n_rows].iter_mut().enumerate() {
            *slot = row_base + row_classes.of[r];
        }
        for c in 0..n_cols {
            colour[n_rows + c] = col_base + col_classes.of[c];
        }

        // Two passes: degrees, then fill. A subdivision vertex has degree two and its
        // row and column each gain one neighbour from it, exactly as a plain edge would,
        // so the degree count is the same either way.
        let first_sub = n_rows + n_cols;
        let mut degree = vec![0u32; n];
        let mut next_sub = first_sub;
        for c in 0..n_cols {
            let (rows, values) = matrix.column(c);
            for (&r, &v) in rows.iter().zip(values) {
                degree[r] += 1;
                degree[n_rows + c] += 1;
                if Some(bits(v)) != plain {
                    degree[next_sub] = 2;
                    colour[next_sub] = value_base + value_class(bits(v));
                    next_sub += 1;
                }
            }
        }

        let mut start = Vec::with_capacity(n + 1);
        let mut total = 0u32;
        for &d in &degree {
            start.push(total);
            total += d;
        }
        start.push(total);

        let mut fill = start.clone();
        let mut adj = vec![0u32; total as usize];
        let mut next_sub = first_sub as u32;
        for c in 0..n_cols {
            let (rows, values) = matrix.column(c);
            let col_vertex = (n_rows + c) as u32;
            for (&r, &v) in rows.iter().zip(values) {
                let row_vertex = r as u32;
                let (a, b) = if Some(bits(v)) == plain {
                    (col_vertex, row_vertex)
                } else {
                    let mid = next_sub;
                    next_sub += 1;
                    adj[fill[mid as usize] as usize] = row_vertex;
                    fill[mid as usize] += 1;
                    adj[fill[mid as usize] as usize] = col_vertex;
                    fill[mid as usize] += 1;
                    (mid, mid)
                };
                adj[fill[row_vertex as usize] as usize] = a;
                fill[row_vertex as usize] += 1;
                adj[fill[col_vertex as usize] as usize] = b;
                fill[col_vertex as usize] += 1;
            }
        }

        Some(Graph {
            n,
            start,
            adj,
            colour,
            first_col: n_rows,
        })
    }

    /// Whether `perm` really is an automorphism: colour-preserving and adjacency-
    /// preserving. Every generator goes through this before it is returned, which is
    /// what lets the search above it prune however it likes.
    fn is_automorphism(&self, perm: &[u32], mark: &mut [u32], stamp: u32) -> bool {
        for (v, &image) in perm.iter().enumerate() {
            if self.colour[image as usize] != self.colour[v] {
                return false;
            }
        }
        for v in 0..self.n as u32 {
            let image = perm[v as usize];
            let theirs = self.neighbours(image);
            if theirs.len() != self.neighbours(v).len() {
                return false;
            }
            for &u in theirs {
                mark[u as usize] = stamp;
            }
            for &u in self.neighbours(v) {
                if mark[perm[u as usize] as usize] != stamp {
                    return false;
                }
            }
        }
        true
    }
}

/// A float's bit pattern, with `-0.0` folded onto `0.0` so the two compare equal.
///
/// Exact rather than tolerant on purpose: merging two values that differ slightly would
/// claim a symmetry the model does not have, and every reduction downstream would
/// inherit that. Splitting two values that are morally the same only costs symmetry.
fn bits(v: f64) -> u64 {
    if v == 0.0 {
        0.0f64.to_bits()
    } else {
        v.to_bits()
    }
}

struct Classes {
    of: Vec<u32>,
    n_classes: u32,
}

/// Number `n` items by their key, with class numbers following the sorted key order so
/// the result does not depend on hash iteration.
fn classify<K: Ord + Clone>(n: usize, key: impl Fn(usize) -> K) -> Classes {
    let mut keys: Vec<K> = (0..n).map(&key).collect();
    let mut sorted = keys.clone();
    sorted.sort();
    sorted.dedup();
    let of = keys
        .drain(..)
        .map(|k| sorted.binary_search(&k).unwrap() as u32)
        .collect();
    Classes {
        of,
        n_classes: sorted.len() as u32,
    }
}

// ---------------------------------------------------------------------------
// Ordered partitions and colour refinement
// ---------------------------------------------------------------------------

/// An ordered partition of the vertices into cells.
///
/// `lab` holds the vertices in partition order and `pos` inverts it. A cell is a
/// contiguous run of positions: `cell_start[i]` is where the cell containing position
/// `i` begins, and `cell_end[s]` is where the cell beginning at `s` ends. Refinement
/// only ever splits a run into sub-runs, so no vertex ever leaves its cell's range.
#[derive(Clone)]
struct Partition {
    lab: Vec<u32>,
    pos: Vec<u32>,
    cell_start: Vec<u32>,
    cell_end: Vec<u32>,
    n_cells: usize,
}

impl Partition {
    /// The partition into the graph's initial colour classes.
    fn of_colours(colour: &[u32]) -> Partition {
        let n = colour.len();
        let mut lab: Vec<u32> = (0..n as u32).collect();
        lab.sort_unstable_by_key(|&v| colour[v as usize]);

        let mut pos = vec![0u32; n];
        let mut cell_start = vec![0u32; n];
        let mut cell_end = vec![0u32; n];
        let mut n_cells = 0;
        let mut i = 0;
        while i < n {
            let mut j = i;
            while j < n && colour[lab[j] as usize] == colour[lab[i] as usize] {
                j += 1;
            }
            for k in i..j {
                pos[lab[k] as usize] = k as u32;
                cell_start[k] = i as u32;
            }
            cell_end[i] = j as u32;
            n_cells += 1;
            i = j;
        }
        Partition {
            lab,
            pos,
            cell_start,
            cell_end,
            n_cells,
        }
    }

    fn is_discrete(&self) -> bool {
        self.n_cells == self.lab.len()
    }

    /// Split `v` off into a cell of its own at the front of the cell it is in, and
    /// return the two cell starts that creates.
    fn individualize(&mut self, v: u32) -> (u32, u32) {
        let i = self.pos[v as usize] as usize;
        let s = self.cell_start[i] as usize;
        let e = self.cell_end[s] as usize;
        debug_assert!(
            e - s > 1,
            "individualizing a vertex already alone in its cell"
        );

        let displaced = self.lab[s];
        self.lab.swap(s, i);
        self.pos[v as usize] = s as u32;
        self.pos[displaced as usize] = i as u32;

        self.cell_end[s] = (s + 1) as u32;
        for k in s + 1..e {
            self.cell_start[k] = (s + 1) as u32;
        }
        self.cell_end[s + 1] = e as u32;
        self.n_cells += 1;
        (s as u32, (s + 1) as u32)
    }

    /// The cell to branch on: the smallest that still holds more than one vertex, ties
    /// going to the earliest. Both halves of that rule are properties of the partition
    /// rather than of the labelling, which is what makes two isomorphic search nodes
    /// pick corresponding cells.
    fn target_cell(&self) -> Option<(u32, u32)> {
        let n = self.lab.len();
        let mut best: Option<(u32, u32)> = None;
        let mut s = 0usize;
        while s < n {
            let e = self.cell_end[s] as usize;
            if e - s > 1 && best.is_none_or(|(bs, be)| (e - s) < (be - bs) as usize) {
                best = Some((s as u32, e as u32));
            }
            s = e;
        }
        best
    }
}

/// Scratch space the refinement reuses across every node of the search, so a node costs
/// no allocation.
struct Scratch {
    counts: Vec<u32>,
    touched_vertices: Vec<u32>,
    touched_cells: Vec<u32>,
    is_touched: Vec<bool>,
    queued: Vec<bool>,
    heap: BinaryHeap<Reverse<u32>>,
    fragment: Vec<(u32, u32)>,
    mark: Vec<u32>,
    /// Edges walked since the scratch was made. Refinement's cost is this and little
    /// else, so it is what the budget is denominated in.
    work: usize,
}

impl Scratch {
    fn new(n: usize) -> Scratch {
        Scratch {
            counts: vec![0; n],
            touched_vertices: Vec::new(),
            touched_cells: Vec::new(),
            is_touched: vec![false; n],
            queued: vec![false; n],
            heap: BinaryHeap::new(),
            fragment: Vec::new(),
            mark: vec![u32::MAX; n],
            work: 0,
        }
    }
}

/// Refine `p` until it is equitable with respect to the cells in `worklist`, and fold
/// what the refinement did into `invariant`.
///
/// The invariant is the whole point of returning anything: it records the *shape* of
/// every split -- which cell was the splitter, how many neighbours each fragment had in
/// it, and how large the fragment was -- in a way that depends on the partition and not
/// on the labels. Two search nodes related by an automorphism produce the same
/// invariant, so a difference is a proof that no automorphism relates them, and the
/// subtree can go.
fn refine(
    graph: &Graph,
    p: &mut Partition,
    worklist: &[u32],
    scratch: &mut Scratch,
    invariant: &mut u64,
) {
    scratch.heap.clear();
    for &s in worklist {
        if !scratch.queued[s as usize] {
            scratch.queued[s as usize] = true;
            scratch.heap.push(Reverse(s));
        }
    }

    // Splitters are taken in order of position, which is structural: it does not depend
    // on which vertices happen to sit in which cell, only on where the cells are.
    while let Some(Reverse(w)) = scratch.heap.pop() {
        scratch.queued[w as usize] = false;
        if p.is_discrete() {
            break;
        }

        let (ws, we) = (w as usize, p.cell_end[w as usize] as usize);
        scratch.touched_vertices.clear();
        scratch.touched_cells.clear();
        for i in ws..we {
            let theirs = graph.neighbours(p.lab[i]);
            scratch.work += theirs.len();
            for &u in theirs {
                if scratch.counts[u as usize] == 0 {
                    scratch.touched_vertices.push(u);
                    let cell = p.cell_start[p.pos[u as usize] as usize];
                    if !scratch.is_touched[cell as usize] {
                        scratch.is_touched[cell as usize] = true;
                        scratch.touched_cells.push(cell);
                    }
                }
                scratch.counts[u as usize] += 1;
            }
        }

        scratch.touched_cells.sort_unstable();
        for ci in 0..scratch.touched_cells.len() {
            let cs = scratch.touched_cells[ci] as usize;
            scratch.is_touched[cs] = false;
            let ce = p.cell_end[cs] as usize;
            if ce - cs == 1 {
                continue;
            }

            // Order the cell by neighbour count. Vertices the splitter does not reach
            // have count zero and lead the run, which is as good an order as any so long
            // as it is the same one on both sides of an isomorphism -- and the count is.
            p.lab[cs..ce].sort_unstable_by_key(|&v| scratch.counts[v as usize]);
            if scratch.counts[p.lab[cs] as usize] == scratch.counts[p.lab[ce - 1] as usize] {
                continue;
            }

            scratch.fragment.clear();
            let mut k = cs;
            while k < ce {
                let count = scratch.counts[p.lab[k] as usize];
                let mut j = k;
                while j < ce && scratch.counts[p.lab[j] as usize] == count {
                    j += 1;
                }
                scratch.fragment.push((k as u32, j as u32));
                mix(invariant, w as u64);
                mix(invariant, cs as u64);
                mix(invariant, count as u64);
                mix(invariant, (j - k) as u64);
                k = j;
            }

            for &(fs, fe) in &scratch.fragment {
                for i in fs..fe {
                    p.pos[p.lab[i as usize] as usize] = i;
                    p.cell_start[i as usize] = fs;
                }
                p.cell_end[fs as usize] = fe;
                if !scratch.queued[fs as usize] {
                    scratch.queued[fs as usize] = true;
                    scratch.heap.push(Reverse(fs));
                }
            }
            p.n_cells += scratch.fragment.len() - 1;
        }

        for &u in &scratch.touched_vertices {
            scratch.counts[u as usize] = 0;
        }
    }

    // The heap can still hold cells when the loop broke on discreteness.
    for Reverse(s) in scratch.heap.drain() {
        scratch.queued[s as usize] = false;
    }
}

/// A cheap dependable mix; the invariant only has to be hard to collide by accident,
/// not by design.
fn mix(state: &mut u64, value: u64) {
    let mut h = *state ^ value.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    *state = h.wrapping_mul(0x94d0_49bb_1331_11eb);
}

// ---------------------------------------------------------------------------
// The search
// ---------------------------------------------------------------------------

struct Search<'a> {
    graph: &'a Graph,
    scratch: Scratch,
    /// The labelling at the first leaf reached. Every automorphism found is the map that
    /// carries this labelling onto another leaf's.
    first_leaf: Option<Vec<u32>>,
    /// The refinement invariant at each level of the path to that first leaf. A branch
    /// whose invariant differs from it at any level cannot reach an automorphism of it.
    spine: Vec<u64>,
    generators: Vec<Vec<u32>>,
    /// Orbits of the group found so far, used to skip branches on the spine that a known
    /// automorphism already covers.
    orbit: Vec<u32>,
    /// Size of each orbit, kept at its representative. Rows, columns and coefficient
    /// vertices are in disjoint colour classes and so never share an orbit, which is
    /// what lets the largest *column* orbit be read off this directly.
    orbit_size: Vec<u32>,
    largest_column_orbit: u32,
    /// The work spent when that last grew, against which the stall budget is measured.
    work_at_growth: usize,
    nodes: usize,
    budget: Budget,
    started: Instant,
    out_of_budget: bool,
    stamp: u32,
}

/// What the search came back with.
struct Found {
    generators: Vec<Vec<u32>>,
    nodes: usize,
    work: usize,
    exhausted_budget: bool,
}

fn find_generators(graph: &Graph, budget: Budget) -> Found {
    let mut search = Search {
        graph,
        scratch: Scratch::new(graph.n),
        first_leaf: None,
        spine: Vec::new(),
        generators: Vec::new(),
        orbit: (0..graph.n as u32).collect(),
        orbit_size: vec![1; graph.n],
        largest_column_orbit: 1,
        work_at_growth: 0,
        nodes: 0,
        budget,
        started: Instant::now(),
        out_of_budget: false,
        stamp: 0,
    };

    let mut partition = Partition::of_colours(&graph.colour);
    let all_cells: Vec<u32> = {
        let mut cells = Vec::new();
        let mut s = 0usize;
        while s < graph.n {
            cells.push(s as u32);
            s = partition.cell_end[s] as usize;
        }
        cells
    };
    let mut invariant = 0u64;
    refine(
        graph,
        &mut partition,
        &all_cells,
        &mut search.scratch,
        &mut invariant,
    );
    // The stall clock starts here. What the first refinement costs is the price of
    // asking the question at all, and charging it against a budget for *not making
    // progress* stops the search on the models whose first refinement is the expensive
    // part before it has looked at anything.
    search.work_at_growth = search.scratch.work;

    search.descend(partition, invariant, 0, true);
    Found {
        generators: search.generators,
        nodes: search.nodes,
        work: search.scratch.work,
        exhausted_budget: search.out_of_budget,
    }
}

impl Search<'_> {
    /// Explore the subtree at `partition`, which sits at `level` and reached the given
    /// refinement `invariant`. `on_spine` says whether the path here is the same one
    /// that reached the first leaf.
    ///
    /// Returns whether an automorphism was found in this subtree, which is what lets a
    /// caller off the spine abandon its remaining siblings: it has what the subtree had
    /// to offer and the group it now knows about covers the rest.
    fn descend(
        &mut self,
        partition: Partition,
        invariant: u64,
        level: usize,
        on_spine: bool,
    ) -> bool {
        if on_spine {
            debug_assert_eq!(self.spine.len(), level);
            self.spine.push(invariant);
        } else if self.spine.get(level) != Some(&invariant) {
            return false;
        }

        if partition.is_discrete() {
            return self.at_leaf(&partition);
        }

        let Some((cs, ce)) = partition.target_cell() else {
            return false;
        };

        // Off the spine the cell is taken in its own order; on it, one vertex per orbit
        // is enough, because an automorphism already found carries the branch for every
        // other vertex of that orbit onto the one that was tried.
        let candidates: Vec<u32> = partition.lab[cs as usize..ce as usize].to_vec();
        let mut tried_reps: Vec<u32> = Vec::new();

        for (index, &v) in candidates.iter().enumerate() {
            if self.exhausted() {
                return false;
            }
            if on_spine {
                let rep = self.orbit_of(v);
                if tried_reps.contains(&rep) {
                    continue;
                }
                tried_reps.push(rep);
            }

            self.nodes += 1;
            // The copy is the other half of what a node costs, and on a large partition
            // it is the larger half.
            self.scratch.work += self.graph.n;
            let mut child = partition.clone();
            let (a, b) = child.individualize(v);
            let mut child_invariant = invariant;
            mix(&mut child_invariant, 0xa5a5_a5a5);
            refine(
                self.graph,
                &mut child,
                &[a, b],
                &mut self.scratch,
                &mut child_invariant,
            );

            let found = self.descend(child, child_invariant, level + 1, on_spine && index == 0);
            if found && !on_spine {
                return true;
            }
        }
        false
    }

    /// A discrete partition. The first one seen becomes the reference; every later one
    /// gives a candidate automorphism, which is checked before it is kept.
    fn at_leaf(&mut self, partition: &Partition) -> bool {
        let Some(first) = &self.first_leaf else {
            self.first_leaf = Some(partition.lab.clone());
            return false;
        };

        let mut perm = vec![0u32; self.graph.n];
        for i in 0..self.graph.n {
            perm[first[i] as usize] = partition.lab[i];
        }
        if perm.iter().enumerate().all(|(v, &image)| image == v as u32) {
            return false;
        }
        self.stamp = self.stamp.wrapping_add(1);
        if !self
            .graph
            .is_automorphism(&perm, &mut self.scratch.mark, self.stamp)
        {
            return false;
        }

        for (v, &image) in perm.iter().enumerate() {
            self.union(v as u32, image);
        }
        self.generators.push(perm);
        true
    }

    fn orbit_of(&mut self, mut v: u32) -> u32 {
        while self.orbit[v as usize] != v {
            self.orbit[v as usize] = self.orbit[self.orbit[v as usize] as usize];
            v = self.orbit[v as usize];
        }
        v
    }

    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.orbit_of(a), self.orbit_of(b));
        if ra == rb {
            return;
        }
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        self.orbit[hi as usize] = lo;
        self.orbit_size[lo as usize] += self.orbit_size[hi as usize];
        let joined = self.orbit_size[lo as usize];
        if lo as usize >= self.graph.first_col && joined > self.largest_column_orbit {
            self.largest_column_orbit = joined;
            self.work_at_growth = self.scratch.work;
        }
    }

    fn exhausted(&mut self) -> bool {
        if self.out_of_budget {
            return true;
        }
        if self.scratch.work >= self.budget.max_work
            || self.scratch.work - self.work_at_growth >= self.budget.stall_work
        {
            self.out_of_budget = true;
        } else if let Some(limit) = self.budget.time
            && self.nodes.is_multiple_of(256)
            && self.started.elapsed() >= limit
        {
            self.out_of_budget = true;
        }
        self.out_of_budget
    }
}

// ---------------------------------------------------------------------------
// Using the group inside the search
// ---------------------------------------------------------------------------

/// Reusable working space for a node's stabilizer orbits, so that asking for them costs
/// no allocation.
pub struct OrbitScratch {
    parent: Vec<u32>,
    is_branched: Vec<bool>,
    /// Columns that some surviving generator moves. Only these can be in an orbit of
    /// more than one column, so this is what the caller iterates rather than the model.
    touched: Vec<u32>,
}

impl OrbitScratch {
    pub fn new(n_cols: usize) -> OrbitScratch {
        OrbitScratch {
            parent: (0..n_cols as u32).collect(),
            is_branched: vec![false; n_cols],
            touched: Vec::new(),
        }
    }

    /// The orbit representative of column `j` under the last stabilizer computed.
    pub fn representative(&mut self, mut j: u32) -> u32 {
        while self.parent[j as usize] != j {
            self.parent[j as usize] = self.parent[self.parent[j as usize] as usize];
            j = self.parent[j as usize];
        }
        j
    }

    /// The columns any surviving generator moves, which is where every orbit of more
    /// than one column is.
    pub fn moved(&self) -> &[u32] {
        &self.touched
    }

    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.representative(a), self.representative(b));
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.parent[hi as usize] = lo;
        }
    }
}

impl Symmetry {
    /// Compute the column orbits of the subgroup generated by the generators that fix
    /// every column of `branched` pointwise, leaving them in `scratch`.
    ///
    /// A subgroup of the true stabilizer is all this finds, because a stabilizing
    /// element need not be a product of stabilizing generators. That costs reduction and
    /// nothing else: every orbit it reports is a genuine orbit of a genuine subgroup,
    /// which is the whole of what the branching rule needs.
    pub fn stabilizer_orbits(&self, branched: &[u32], scratch: &mut OrbitScratch) {
        for (j, slot) in scratch.parent.iter_mut().enumerate() {
            *slot = j as u32;
        }
        scratch.touched.clear();
        if self.generators.is_empty() {
            return;
        }
        for &j in branched {
            scratch.is_branched[j as usize] = true;
        }

        for (generator, support) in self.generators.iter().zip(&self.supports) {
            if support.iter().any(|&j| scratch.is_branched[j as usize]) {
                continue;
            }
            for &j in support {
                scratch.union(j, generator[j as usize]);
                scratch.touched.push(j);
            }
        }

        for &j in branched {
            scratch.is_branched[j as usize] = false;
        }
        scratch.touched.sort_unstable();
        scratch.touched.dedup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Builder, RowSense, Sense};
    use std::collections::HashSet;

    /// Every binary point a model admits, as bit patterns. Only for models small enough
    /// to enumerate, which is the point: it turns "is this a symmetry" into a question
    /// about the feasible set itself rather than about the graph the detector built.
    fn feasible_points(problem: &Problem) -> HashSet<(u32, i64)> {
        let n = problem.n_cols();
        assert!(n <= 16);
        let mut found = HashSet::new();
        for mask in 0u32..(1 << n) {
            let x: Vec<f64> = (0..n).map(|j| f64::from((mask >> j) & 1)).collect();
            let within_bounds =
                (0..n).all(|j| x[j] >= problem.col_lb[j] && x[j] <= problem.col_ub[j]);
            if !within_bounds {
                continue;
            }
            let csr = problem.matrix.to_csr();
            let feasible = (0..problem.n_rows()).all(|r| {
                let (cols, values) = csr.column(r);
                let activity: f64 = cols.iter().zip(values).map(|(&c, &v)| v * x[c]).sum();
                activity >= problem.row_lb[r] - 1e-9 && activity <= problem.row_ub[r] + 1e-9
            });
            if feasible {
                let objective: f64 = (0..n).map(|j| problem.obj[j] * x[j]).sum();
                found.insert((mask, (objective * 1e6).round() as i64));
            }
        }
        found
    }

    /// Assert that every generator carries the feasible set onto itself and preserves
    /// the objective. This is the property the whole module exists to provide, checked
    /// against the model rather than against the machinery that produced it.
    fn check_generators_preserve_the_model(problem: &Problem, symmetry: &Symmetry) {
        let points = feasible_points(problem);
        for generator in &symmetry.generators {
            for &(mask, objective) in &points {
                let mut image = 0u32;
                for (j, &to) in generator.iter().enumerate() {
                    if (mask >> j) & 1 == 1 {
                        image |= 1 << to;
                    }
                }
                assert!(
                    points.contains(&(image, objective)),
                    "generator {generator:?} took feasible point {mask:b} to {image:b}, \
                     which is not feasible at the same objective",
                );
            }
        }
    }

    /// The order of the group the generators make, by closing the set under composition.
    /// Only usable when the group is small; it is here to check a known one.
    fn group_order(n: usize, generators: &[Vec<u32>]) -> usize {
        let identity: Vec<u32> = (0..n as u32).collect();
        let mut elements: HashSet<Vec<u32>> = HashSet::from([identity.clone()]);
        let mut frontier = vec![identity];
        while let Some(g) = frontier.pop() {
            for h in generators {
                let product: Vec<u32> = (0..n).map(|j| h[g[j] as usize]).collect();
                if elements.insert(product.clone()) {
                    frontier.push(product);
                }
            }
        }
        elements.len()
    }

    /// `x_i + x_{i+1} <= 1` around a cycle of `n` columns, maximizing their sum. The
    /// symmetry group of a cycle is the dihedral group of order `2n`.
    fn conflict_cycle(n: usize) -> Problem {
        let mut model = Builder::new(Sense::Maximize).named("cycle");
        let columns: Vec<usize> = (0..n).map(|i| model.binary(&format!("x{i}"))).collect();
        model.objective(&columns.iter().map(|&c| (c, 1.0)).collect::<Vec<_>>());
        for i in 0..n {
            model.row(
                &[(columns[i], 1.0), (columns[(i + 1) % n], 1.0)],
                RowSense::Le,
                1.0,
            );
        }
        model.build()
    }

    #[test]
    fn a_cycle_of_conflicts_has_its_dihedral_group() {
        for n in [5usize, 6, 7] {
            let problem = conflict_cycle(n);
            let symmetry = detect(&problem, Budget::default());
            check_generators_preserve_the_model(&problem, &symmetry);
            assert_eq!(
                group_order(n, &symmetry.generators),
                2 * n,
                "cycle of {n} should have the dihedral group of order {}",
                2 * n
            );
            // One orbit: every column of a cycle reaches every other.
            assert_eq!(symmetry.nontrivial_orbits().len(), 1);
            assert_eq!(symmetry.moved, n);
        }
    }

    #[test]
    fn duplicate_columns_are_found_as_an_orbit() {
        let mut model = Builder::new(Sense::Maximize).named("duplicates");
        let a = model.binary("a");
        let b = model.binary("b");
        let c = model.binary("c");
        model.objective(&[(a, 1.0), (b, 1.0), (c, 2.0)]);
        model.row(&[(a, 1.0), (b, 1.0), (c, 1.0)], RowSense::Le, 2.0);
        let problem = model.build();

        let symmetry = detect(&problem, Budget::default());
        check_generators_preserve_the_model(&problem, &symmetry);
        // a and b are interchangeable; c has a different objective and is not.
        assert_eq!(symmetry.orbits[0], symmetry.orbits[1]);
        assert_ne!(symmetry.orbits[0], symmetry.orbits[2]);
    }

    #[test]
    fn an_objective_that_tells_the_columns_apart_breaks_the_symmetry() {
        let mut model = Builder::new(Sense::Maximize).named("distinct");
        let columns: Vec<usize> = (0..5).map(|i| model.binary(&format!("x{i}"))).collect();
        let terms: Vec<(usize, f64)> = columns
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, 1.0 + i as f64))
            .collect();
        model.objective(&terms);
        for i in 0..5 {
            model.row(
                &[(columns[i], 1.0), (columns[(i + 1) % 5], 1.0)],
                RowSense::Le,
                1.0,
            );
        }
        let problem = model.build();

        let symmetry = detect(&problem, Budget::default());
        assert!(
            symmetry.is_trivial(),
            "distinct objectives leave no symmetry"
        );
        assert_eq!(symmetry.orbits, (0..5).collect::<Vec<u32>>());
    }

    #[test]
    fn coefficient_values_are_not_exchangeable_with_each_other() {
        // Same sparsity pattern as the cycle, but one row weights its two columns
        // differently. Nothing may map the 1 onto the 2.
        let mut model = Builder::new(Sense::Maximize).named("weighted");
        let columns: Vec<usize> = (0..4).map(|i| model.binary(&format!("x{i}"))).collect();
        model.objective(&columns.iter().map(|&c| (c, 1.0)).collect::<Vec<_>>());
        for i in 0..4 {
            let w = if i == 0 { 2.0 } else { 1.0 };
            model.row(
                &[(columns[i], w), (columns[(i + 1) % 4], 1.0)],
                RowSense::Le,
                2.0,
            );
        }
        let problem = model.build();

        let symmetry = detect(&problem, Budget::default());
        check_generators_preserve_the_model(&problem, &symmetry);
    }

    #[test]
    fn row_bounds_are_not_exchangeable_with_each_other() {
        let mut model = Builder::new(Sense::Maximize).named("bounds");
        let columns: Vec<usize> = (0..4).map(|i| model.binary(&format!("x{i}"))).collect();
        model.objective(&columns.iter().map(|&c| (c, 1.0)).collect::<Vec<_>>());
        for i in 0..4 {
            let rhs = if i == 0 { 2.0 } else { 1.0 };
            model.row(
                &[(columns[i], 1.0), (columns[(i + 1) % 4], 1.0)],
                RowSense::Le,
                rhs,
            );
        }
        let problem = model.build();

        let symmetry = detect(&problem, Budget::default());
        check_generators_preserve_the_model(&problem, &symmetry);
        // The loosened row breaks the rotations and leaves the reflection through it.
        assert_eq!(group_order(4, &symmetry.generators), 2);
    }

    #[test]
    fn a_model_with_no_rows_at_all_does_not_trip_the_refinement() {
        let mut model = Builder::new(Sense::Minimize).named("empty");
        model.binary("a");
        model.binary("b");
        let problem = model.build();
        let symmetry = detect(&problem, Budget::default());
        assert_eq!(symmetry.orbits[0], symmetry.orbits[1]);
    }

    #[test]
    fn a_vertex_ceiling_below_the_model_returns_no_symmetry_rather_than_a_wrong_one() {
        let problem = conflict_cycle(6);
        let symmetry = detect(
            &problem,
            Budget {
                max_vertices: 4,
                ..Budget::default()
            },
        );
        assert!(symmetry.is_trivial());
        assert_eq!(symmetry.orbits, (0..6).collect::<Vec<u32>>());
    }
}
