use std::marker;
use fb::FixedBitSet;

use super::{
    EdgeType,
    Incoming,
};
use super::graph::{
    Graph,
    IndexType,
    NodeIndex,
};

use super::visit::GetAdjacencyMatrix;

#[derive(Debug)]
struct Vf2State<Ty, Ix> {
    /// The current mapping M(s) of nodes from G0 → G1 and G1 → G0,
    /// NodeIndex::end() for no mapping.
    mapping: Vec<NodeIndex<Ix>>,
    /// out[i] is non-zero if i is in either M_0(s) or Tout_0(s)
    /// These are all the next vertices that are not mapped yet, but
    /// have an outgoing edge from the mapping.
    out: Vec<usize>,
    /// ins[i] is non-zero if i is in either M_0(s) or Tin_0(s)
    /// These are all the incoming vertices, those not mapped yet, but
    /// have an edge from them into the mapping.
    /// Unused if graph is undirected -- it's identical with out in that case.
    ins: Vec<usize>,
    out_size: usize,
    ins_size: usize,
    adjacency_matrix: FixedBitSet,
    generation: usize,
    _etype: marker::PhantomData<Ty>,
}

impl<Ty, Ix> Vf2State<Ty, Ix> where
    Ty: EdgeType,
    Ix: IndexType,
{
    pub fn new<N, E>(g: &Graph<N, E, Ty, Ix>) -> Self
    {
        let c0 = g.node_count();
        let mut state = Vf2State {
            mapping: Vec::with_capacity(c0),
            out: Vec::with_capacity(c0),
            ins: Vec::with_capacity(c0 * (g.is_directed() as usize)),
            out_size: 0,
            ins_size: 0,
            adjacency_matrix: g.adjacency_matrix(),
            generation: 0,
            _etype: marker::PhantomData,
        };
        for _ in (0..c0) {
            state.mapping.push(NodeIndex::end());
            state.out.push(0);
            if <Ty as EdgeType>::is_directed() {
                state.ins.push(0);
            }
        }
        state
    }

    /// Return **true** if we have a complete mapping
    pub fn is_complete(&self) -> bool
    {
        self.generation == self.mapping.len()
    }

    /// Add mapping **from** <-> **to** to the state.
    pub fn push_mapping<N, E>(&mut self, from: NodeIndex<Ix>, to: NodeIndex<Ix>,
                              g: &Graph<N, E, Ty, Ix>)
    {
        self.generation += 1;
        let s = self.generation;
        self.mapping[from.index()] = to;
        // update T0 & T1 ins/outs
        // T0out: Node in G0 not in M0 but successor of a node in M0.
        // st.out[0]: Node either in M0 or successor of M0
        for ix in g.neighbors(from) {
            if self.out[ix.index()] == 0 {
                self.out[ix.index()] = s;
            }
            self.out_size += 1;
        }
        if g.is_directed() {
            for ix in g.neighbors_directed(from, Incoming) {
                if self.ins[ix.index()] == 0 {
                    self.ins[ix.index()] = s;
                }
            }
            self.ins_size += 1;
        }
    }

    /// Restore the state to before the last added mapping
    pub fn pop_mapping<N, E>(&mut self, from: NodeIndex<Ix>,
                             g: &Graph<N, E, Ty, Ix>)
    {
        let s = self.generation;
        self.generation -= 1;

        // undo (n, m) mapping
        self.mapping[from.index()] = NodeIndex::end();

        // unmark in ins and outs
        for ix in g.neighbors(from) {
            if self.out[ix.index()] == s {
                self.out[ix.index()] = 0;
            }
            self.out_size -= 1;
        }
        if g.is_directed() {
            for ix in g.neighbors_directed(from, Incoming) {
                if self.ins[ix.index()] == s {
                    self.ins[ix.index()] = 0;
                }
            }
            self.ins_size -= 1;
        }
    }

    /// Find the next (least) node in the Tout set.
    pub fn next_out_index(&self, from_index: usize) -> Option<usize>
    {
        self.out[from_index..].iter()
                    .enumerate()
                    .filter(|&(index, elt)| *elt > 0 && self.mapping[from_index + index] == NodeIndex::end())
                    .next()
                    .map(|(index, _)| index)
    }

    /// Find the next (least) node in the Tin set.
    pub fn next_in_index(&self, from_index: usize) -> Option<usize>
    {
        if !<Ty as EdgeType>::is_directed() {
            return None
        }
        self.ins[from_index..].iter()
                    .enumerate()
                    .filter(|&(index, elt)| *elt > 0 && self.mapping[from_index + index] == NodeIndex::end())
                    .next()
                    .map(|(index, _)| index)
    }

    /// Find the next (least) node in the N - M set.
    pub fn next_rest_index(&self, from_index: usize) -> Option<usize>
    {
        self.mapping[from_index..].iter()
               .enumerate()
               .filter(|&(_, elt)| *elt == NodeIndex::end())
               .next()
               .map(|(index, _)| index)
    }
}


/// Return **true** if the graphs **g0** and **g1** are isomorphic.
///
/// Using the VF2 algorithm.
///
/// Not implemented: Vertex or edge weight matching.
///
/// ## References
/// 
/// * A (Sub)Graph Isomorphism Algorithm for Matching Large Graphs
///   Luigi P. Cordella, Pasquale Foggia, Carlo Sansone,
///   and Mario Vento
pub fn is_isomorphic<N, E, Ty, Ix>(g0: &Graph<N, E, Ty, Ix>,
                                   g1: &Graph<N, E, Ty, Ix>) -> bool where
    Ty: EdgeType,
    Ix: IndexType,
{
    if g0.node_count() != g1.node_count() || g0.edge_count() != g1.edge_count() {
        return false
    }

    /// Return Some(bool) if isomorphism is decided, else None.
    fn try_match<N, E, Ty, Ix>(st: &mut [Vf2State<Ty, Ix>; 2],
                               g0: &Graph<N, E, Ty, Ix>,
                               g1: &Graph<N, E, Ty, Ix>) -> Option<bool> where
        Ty: EdgeType,
        Ix: IndexType,
    {
        let g = [g0, g1];
        let graph_indices = 0..2;
        let end = NodeIndex::end();

        // if all are mapped -- we are done and have an iso
        if st[0].is_complete() {
            return Some(true)
        }

        // A "depth first" search of a valid mapping from graph 1 to graph 2

        // F(s, n, m) -- evaluate state s and add mapping n <-> m

        // Find least T1out node (in st.out[1] but not in M[1])
        #[derive(Copy, Clone, PartialEq, Debug)]
        enum OpenList {
            Out,
            In,
            Other,
        }
        let mut open_list = OpenList::Out;

        let mut to_index;
        let mut from_index = None;
        // Try the out list
        to_index = st[1].next_out_index(0);

        if to_index.is_some() {
            from_index = st[0].next_out_index(0);
            open_list = OpenList::Out;
        }

        // Try the in list
        if to_index.is_none() || from_index.is_none() {
            to_index = st[1].next_in_index(0);

            if to_index.is_some() {
                from_index = st[0].next_in_index(0);
                open_list = OpenList::In;
            }
        }

        // Try the other list -- disconnected graph
        if to_index.is_none() || from_index.is_none() {
            to_index = st[1].next_rest_index(0);
            if to_index.is_some() {
                from_index = st[0].next_rest_index(0);
                open_list = OpenList::Other;
            }
        }

        let (cand0, cand1) = match (from_index, to_index) {
            (Some(n), Some(m)) => (n, m),
            // No more candidates
            _ => return None,
        };

        let mut nx = NodeIndex::new(cand0);
        let mx = NodeIndex::new(cand1);

        let mut first = true;

        'candidates: loop {
            if !first {
                // Find the next node index to try on the `from` side of the mapping
                let start = nx.index() + 1;
                let cand0 = match open_list {
                    OpenList::Out => st[0].next_out_index(start),
                    OpenList::In => st[0].next_in_index(start),
                    OpenList::Other => st[0].next_rest_index(start),
                }.map(|c| c + start); // compensate for start offset.
                nx = match cand0 {
                    None => break, // no more candidates
                    Some(ix) => NodeIndex::new(ix),
                };
                debug_assert!(nx.index() >= start);
            }
            first = false;

            let nodes = [nx, mx];

            /*
            print!("Mapping state: ");
            for (index, nmap) in st.mapping[0].iter().enumerate() {
                if *nmap == end {
                    continue;
                }
                print!("{} => {}, ", index, nmap.index());
            }
            println!("{} => {}", nx.index(), mx.index());
            */
        
            // Check syntactic feasibility of mapping by ensuring adjacencies
            // of nx map to adjacencies of mx.
            //
            // nx == map to => mx
            //
            // R_succ
            //
            // Check that every neighbor of nx is mapped to a neighbor of mx,
            // then check the reverse, from mx to nx. Check that they have the same
            // count of edges.
            //
            // Note: We want to check the lookahead measures here if we can,
            // R_out: Equal for G0, G1: Card(Succ(G, n) ^ Tout); for both Succ and Pred
            // R_in: Same with Tin
            // R_new: Equal for G0, G1: Ñ n Pred(G, n); both Succ and Pred,
            //      Ñ is G0 - M - Tin - Tout
            // last attempt to add these did not speed up any of the testcases
            let mut succ_count = [0, 0];
            for j in graph_indices.clone() {
                for n_neigh in g[j].neighbors(nodes[j]) {
                    succ_count[j] += 1;
                    let m_neigh = st[j].mapping[n_neigh.index()];
                    if m_neigh == end {
                        continue;
                    }
                    let has_edge = g[1-j].is_adjacent(&st[1-j].adjacency_matrix, nodes[1-j], m_neigh);
                    if !has_edge {
                        continue 'candidates;
                    }
                }
            }
            if succ_count[0] != succ_count[1] {
                continue 'candidates;
            }

            // R_pred
            if g[0].is_directed() {
                let mut pred_count = [0, 0];
                for j in graph_indices.clone() {
                    for n_neigh in g[j].neighbors_directed(nodes[j], Incoming) {
                        pred_count[j] += 1;
                        let m_neigh = st[j].mapping[n_neigh.index()];
                        if m_neigh == end {
                            continue;
                        }
                        let has_edge = g[1-j].is_adjacent(&st[1-j].adjacency_matrix, m_neigh, nodes[1-j]);
                        if !has_edge {
                            continue 'candidates;
                        }
                    }
                }
                if pred_count[0] != pred_count[1] {
                    continue 'candidates;
                }
            }

            // Add mapping nx <-> mx to the state
            for j in graph_indices.clone() {
                st[j].push_mapping(nodes[j], nodes[1-j], g[j]);
            }

            // Check counts in Tin/Tout
            if st[0].out_size == st[0].out_size ||
               st[1].ins_size == st[1].ins_size
            {

                // Recurse
                match try_match(st, g0, g1) {
                    None => {}
                    result => return result,
                }
            }

            // Restore state.
            for j in graph_indices.clone() {
                st[j].pop_mapping(nodes[j], g[j]);
            }
        }
        None
    }
    let mut st = [Vf2State::new(g0), Vf2State::new(g1)];
    try_match(&mut st, g0, g1).unwrap_or(false)
}

