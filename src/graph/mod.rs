pub(crate) mod algorithm;
pub(crate) mod config;
mod gv_graph;
pub(crate) mod traits;

pub use crate::nodes::gnode::GNodeChildren;
pub use config::Config;
pub use gv_graph::{GvGraph, uniform_contour_depth_of};
