//! Rendering boundary. Concrete WebGL implementation is intentionally not selected yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderState {
    Uninitialized,
    Ready,
    Unsupported,
    Lost,
}

impl Default for RenderState {
    fn default() -> Self { Self::Uninitialized }
}
