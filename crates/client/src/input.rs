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
    /// Return forward/strafe axes clamped to the unit circle so diagonal
    /// movement is not faster than movement along a single axis.
    pub fn normalized_movement(&self) -> (f32, f32) {
        let forward = self.forward.clamp(-1.0, 1.0);
        let strafe = self.strafe.clamp(-1.0, 1.0);
        let magnitude = (forward * forward + strafe * strafe).sqrt();

        if magnitude > 1.0 {
            (forward / magnitude, strafe / magnitude)
        } else {
            (forward, strafe)
        }
    }

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

    #[test]
    fn diagonal_movement_is_normalized() {
        let frame = InputFrame { forward: 1.0, strafe: 1.0, ..Default::default() };
        let (forward, strafe) = frame.normalized_movement();
        let length = (forward * forward + strafe * strafe).sqrt();
        assert!((length - 1.0).abs() < 0.0001);
    }

    #[test]
    fn movement_axes_are_clamped() {
        let frame = InputFrame { forward: 3.0, strafe: 0.0, ..Default::default() };
        assert_eq!(frame.normalized_movement(), (1.0, 0.0));
    }
}
