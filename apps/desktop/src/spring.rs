//! Springs and easing curves, ported from `app/src/lib/spring.ts`.
//!
//! A spring has a target and no duration. New input is a new target, so the
//! motion stays continuous when the user interrupts it. The caller owns the
//! frame loop: call [`Spring::step`] with the elapsed time on each frame, and
//! ask for another frame while [`Spring::is_moving`] is true.

/// The two parameters that Apple's fluid interface work uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringConfig {
    /// 1.0 settles with no overshoot. Less than 1.0 bounces.
    pub damping: f32,
    /// About how long the spring takes to arrive, in seconds.
    pub response: f32,
}

/// A row that moves out of the way. It was not thrown, so it does not overshoot.
pub const SETTLE: SpringConfig = SpringConfig {
    damping: 1.0,
    response: 0.32,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Spring {
    pub x: f32,
    pub v: f32,
    pub target: f32,
    config: SpringConfig,
}

impl Spring {
    pub fn new(config: SpringConfig) -> Self {
        Self {
            x: 0.0,
            v: 0.0,
            target: 0.0,
            config,
        }
    }

    /// Jump to a value with no animation and no velocity.
    pub fn set(&mut self, x: f32) {
        self.x = x;
        self.target = x;
        self.v = 0.0;
    }

    /// Aim at a new target. The velocity carries through.
    pub fn to(&mut self, target: f32) {
        self.target = target;
    }

    pub fn is_moving(&self) -> bool {
        self.x != self.target || self.v != 0.0
    }

    /// Advance by `dt` seconds. Returns true while the spring still moves.
    pub fn step(&mut self, dt: f32) -> bool {
        // A dropped frame resumes. It does not integrate one huge step.
        let dt = dt.clamp(0.0, 1.0 / 30.0);
        let k = std::f32::consts::TAU / self.config.response;
        let stiffness = k * k;
        let damper = 2.0 * self.config.damping * k;
        // Fixed sub-steps: the integrator is stable only when the step is small
        // against the period.
        let steps = ((dt * 240.0).ceil() as usize).max(1);
        let h = dt / steps as f32;
        for _ in 0..steps {
            let a = -stiffness * (self.x - self.target) - damper * self.v;
            self.v += a * h;
            self.x += self.v * h;
        }
        // Under a twentieth of a pixel and slowing: nobody can see the rest.
        if (self.x - self.target).abs() < 0.05 && self.v.abs() < 0.05 {
            self.x = self.target;
            self.v = 0.0;
        }
        self.is_moving()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mut s: Spring, frames: usize) -> Spring {
        for _ in 0..frames {
            s.step(1.0 / 60.0);
        }
        s
    }

    #[test]
    fn a_critically_damped_spring_arrives_without_overshoot() {
        let mut s = Spring::new(SETTLE);
        s.set(-35.0);
        s.to(0.0);
        let mut worst = f32::MIN;
        for _ in 0..120 {
            s.step(1.0 / 60.0);
            worst = worst.max(s.x);
        }
        assert!(worst <= 0.001, "overshot to {worst}");
        assert!(!s.is_moving());
    }

    #[test]
    fn a_bouncy_spring_overshoots_and_still_settles() {
        let mut s = Spring::new(SpringConfig {
            damping: 0.5,
            response: 0.35,
        });
        s.set(-20.0);
        s.to(0.0);
        let mut crossed = false;
        for _ in 0..240 {
            s.step(1.0 / 60.0);
            crossed |= s.x > 0.5;
        }
        assert!(crossed && !s.is_moving());
    }

    #[test]
    fn a_dropped_frame_does_not_fling_the_value() {
        let s = run(
            {
                let mut s = Spring::new(SETTLE);
                s.set(10.0);
                s.to(0.0);
                s
            },
            1,
        );
        let mut slow = Spring::new(SETTLE);
        slow.set(10.0);
        slow.to(0.0);
        slow.step(5.0);
        assert!(slow.x.abs() <= 10.0 && (slow.x - s.x).abs() < 3.0);
    }
}
