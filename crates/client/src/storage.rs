//! Persistence boundary. The concrete browser backend will be chosen after save-format review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageState {
    Uninitialized,
    Ready,
    ReadOnly,
    Unavailable,
}

impl Default for StorageState {
    fn default() -> Self { Self::Uninitialized }
}
