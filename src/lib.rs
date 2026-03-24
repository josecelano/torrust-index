#![forbid(unsafe_code)]

pub(crate) mod contour_range;
pub(crate) mod gnode;
pub(crate) mod handle;
pub(crate) mod pewei;
pub(crate) mod plateau;
pub(crate) mod view;

pub(crate) mod graph;
pub(crate) mod traits;

pub(crate) mod arena;
pub(crate) mod decay;
pub(crate) mod diagnostic;
pub(crate) mod evict;
pub(crate) mod graph_budget;
pub(crate) mod graph_extract;
pub(crate) mod graph_plateau;
pub(crate) mod graph_query;
pub(crate) mod graph_traits;
pub(crate) mod gtree;
pub(crate) mod observe;
pub(crate) mod rebalance;
pub(crate) mod split;
pub(crate) mod vnode;
pub(crate) mod vtree;

#[doc(hidden)]
pub mod invariants;

pub use contour_range::{BasisElement, ContourRange, ContourRangeEnergy};
pub use gnode::GState;
#[doc(hidden)]
pub use graph::GNodeChildren;
pub use graph::{Config, GvGraph};
pub use handle::GNodeId;
pub use pewei::{Layer, Pewei, Terminal, Transition};
pub use plateau::{BasisEdge, Plateau};
pub use traits::{
    Accumulator, Attenuatable, Coordinate, Inspectable, Observation, Proratable, Rng, ScalableObservation, SpatialRead,
    SpatialWrite, TemporalDecay, Weighable, WeightedSampler,
};
pub use view::{Cell, Node, Span};
