#![forbid(unsafe_code)]

pub(crate) mod handle;
pub(crate) mod nodes;
pub(crate) mod spatial;

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
pub(crate) mod observe;
pub(crate) mod rebalance;
pub(crate) mod split;
pub(crate) mod tree;

#[doc(hidden)]
pub mod invariants;

#[doc(hidden)]
pub use graph::GNodeChildren;
pub use graph::{Config, GvGraph};
pub use handle::GNodeId;
pub use nodes::gnode::GState;
pub use spatial::contour_range::{BasisElement, ContourRange, ContourRangeEnergy};
pub use spatial::pewei::{Layer, Pewei, Terminal, Transition};
pub use spatial::plateau::{BasisEdge, Plateau};
pub use spatial::view::{Cell, Node, Span};
pub use traits::{
    Accumulator, Attenuatable, Coordinate, Inspectable, Observation, Proratable, Rng,
    ScalableObservation, SpatialRead, SpatialWrite, TemporalDecay, Weighable, WeightedSampler,
};
