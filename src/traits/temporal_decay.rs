use super::SpatialRead;

pub trait TemporalDecay: SpatialRead {
    fn decay(&mut self, root: crate::handle::GNodeId, attenuation: f64, q: f64);
}
