//! Mathematical coordinates are (x, y, w), with w drawn vertically.
use bevy::math::{Mat2, Mat3, Vec2};

#[derive(Clone, Copy, Debug)]
pub struct Affine {
    pub angle: f32,
    pub scale: Vec2,
    pub shear: f32,
    pub translation: Vec2,
}

impl Default for Affine {
    fn default() -> Self {
        Self {
            angle: 40.0,
            scale: Vec2::new(1.15, 0.85),
            shear: 0.75,
            translation: Vec2::new(1.7, 0.8),
        }
    }
}

impl Affine {
    /// Animate parameters, rather than interpolating matrix entries: a rotation
    /// stays a rotation and positive scales remain invertible throughout.
    pub fn matrix(self, amount: f32, linear: bool, translate: bool) -> Mat3 {
        let u = amount.clamp(0.0, 1.0);
        let a = if linear {
            Mat2::from_angle(self.angle.to_radians() * u)
                * Mat2::from_cols(Vec2::X, Vec2::new(self.shear * u, 1.0))
                * Mat2::from_diagonal(Vec2::ONE.lerp(self.scale, u))
        } else {
            Mat2::IDENTITY
        };
        let t = if translate {
            self.translation * u
        } else {
            Vec2::ZERO
        };
        Mat3::from_cols(a.x_axis.extend(0.0), a.y_axis.extend(0.0), t.extend(1.0))
    }
}

/// Short cosine acceleration/deceleration ramps surrounding steady travel.
/// Position, velocity and acceleration remain continuous at the ramp boundaries.
pub fn eased(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    const RAMP: f32 = 0.18;
    let integral = |x: f32| {
        let u = x / RAMP;
        0.5 * RAMP * (u - (std::f32::consts::PI * u).sin() / std::f32::consts::PI)
    };
    if t <= 0. {
        0.
    } else if t >= 1. {
        1.
    } else if t < RAMP {
        integral(t) / (1. - RAMP)
    } else if t > 1. - RAMP {
        1. - integral(1. - t) / (1. - RAMP)
    } else {
        (t - RAMP * 0.5) / (1. - RAMP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec3;
    fn close(a: Vec3, b: Vec3) {
        assert!((a - b).length() < 1e-5, "{a:?} != {b:?}");
    }
    #[test]
    fn translation_is_a_linear_shear_in_three_dimensions() {
        let m = Affine::default().matrix(1.0, false, true);
        let p = Vec3::new(0.4, -0.8, 1.0);
        close(m * p, p + Vec3::new(1.7, 0.8, 0.0));
        close(m * Vec3::ZERO, Vec3::ZERO);
        close(m * Vec3::new(0.4, -0.8, 0.0), Vec3::new(0.4, -0.8, 0.0));
        close(m * Vec3::new(0.4, -0.8, 2.0), Vec3::new(3.8, 0.8, 2.0));
        let q = Vec3::new(-2.0, 0.3, 1.4);
        close(m * (2.0 * p + q), 2.0 * (m * p) + m * q);
    }
    #[test]
    fn affine_slice_and_direction_difference_agree() {
        let a = Affine::default();
        for u in [0.0, 0.2, 0.7, 1.0] {
            let m = a.matrix(u, true, true);
            let p = Vec3::new(0.4, -0.8, 1.0);
            let q = Vec3::new(-0.6, 0.2, 1.0);
            close(m * (p - q), m * p - m * q);
            assert_eq!((m * p).z, 1.0);
            assert!(m.determinant() > 0.0);
        }
        close(a.matrix(0.0, true, true) * Vec3::ONE, Vec3::ONE);
    }
    #[test]
    fn combined_map_applies_linear_part_before_translation() {
        let a = Affine::default();
        let p = Vec3::new(0.4, -0.8, 1.0);
        close(
            a.matrix(1.0, true, true) * p,
            a.matrix(1.0, false, true) * a.matrix(1.0, true, false) * p,
        );
        assert!(
            (a.matrix(1.0, true, true) * p
                - a.matrix(1.0, true, false) * a.matrix(1.0, false, true) * p)
                .length()
                > 0.1
        );
    }
}
