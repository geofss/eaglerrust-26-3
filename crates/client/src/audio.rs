//! Audio subsystem boundary. Browser audio context integration will live behind this interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioState {
    Uninitialized,
    Ready,
    Suspended,
    Unavailable,
}

impl Default for AudioState {
    fn default() -> Self { Self::Uninitialized }
}
