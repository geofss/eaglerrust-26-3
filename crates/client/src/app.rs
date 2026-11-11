//! Client lifecycle state. Keep this independent of browser APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientPhase {
    Booting,
    Ready,
    Running,
    Paused,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientLifecycle {
    phase: ClientPhase,
}

impl Default for ClientLifecycle {
    fn default() -> Self {
        Self { phase: ClientPhase::Booting }
    }
}

impl ClientLifecycle {
    pub fn phase(&self) -> ClientPhase { self.phase }
    pub fn set_phase(&mut self, phase: ClientPhase) { self.phase = phase; }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_starts_booting() {
        assert_eq!(ClientLifecycle::default().phase(), ClientPhase::Booting);
    }
}
