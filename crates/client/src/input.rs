//! Platform-neutral input snapshot. Browser event wiring belongs in the host layer.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InputFrame {
    pub forward: f32,
    pub strafe: f32,
    pub vertical: f32,
    pub look_delta_x: f32,
    pub look_delta_y: f32,
    pub jump_pressed: bool,
    pub sprint_pressed: bool,
}

impl InputFrame {
    pub fn clear_transient(&mut self) {
        self.look_delta_x = 0.0;
        self.look_delta_y = 0.0;
        self.jump_pressed = false;
        self.sprint_pressed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transient_input_can_be_cleared() {
        let mut frame = InputFrame { look_delta_x: 2.0, jump_pressed: true, ..Default::default() };
        frame.clear_transient();
        assert_eq!(frame.look_delta_x, 0.0);
        assert!(!frame.jump_pressed);
    }
}
