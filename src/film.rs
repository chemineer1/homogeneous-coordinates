//! Deterministic film timing. There is no input handling or user interface.
use crate::{
    math::{Affine, eased},
    scene,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

const CAMERA_YAW: f32 = 0.32;
const FLAT_ROTATION_END: f32 = 2.55 / 10.;
const CAMERA_TILT_END: f32 = 7. / 10.;

pub const CHAPTERS: [&str; 14] = [
    "Rotation",
    "Shear",
    "Translation",
    "Axis-aligned scaling",
    "Diagonal scaling",
    "Affine: scaling",
    "Affine: add shear",
    "Affine: add rotation",
    "Affine: add translation",
    "Affine transformation",
    "Vertical shear",
    "Project onto the plane",
    "Perspective",
    "Perspective",
];
pub const DURATIONS: [f32; 14] = [
    10., 3., 4.5, 2., 3., 1.5, 1.5, 1.5, 1.5, 1., 3., 4., 3.5, 2.5,
];
pub fn total_seconds() -> f32 {
    DURATIONS.iter().sum()
}
pub fn total_frames(fps: u32) -> u32 {
    DURATIONS
        .iter()
        .map(|s| (s * fps as f32).round() as u32)
        .sum()
}

#[derive(Resource)]
pub struct Frame {
    pub chapter: usize,
    pub progress: f32,
    pub affine: Affine,
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub ghost: bool,
    pub lattice: bool,
    pub rays: bool,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            chapter: 0,
            progress: 0.,
            affine: Affine {
                scale: Vec2::new(1.45, 0.7),
                // Parallel to the camera screen: motion has no depth or height component.
                translation: Vec2::new(CAMERA_YAW.cos(), -CAMERA_YAW.sin()) * 1.15,
                ..Affine::default()
            },
            yaw: CAMERA_YAW,
            pitch: 0.52,
            zoom: 1.05,
            ghost: true,
            lattice: true,
            rays: true,
        }
    }
}
impl Frame {
    pub fn seek(&mut self, frame: u32, fps: u32) {
        let mut remaining = frame.min(total_frames(fps) - 1);
        for (chapter, seconds) in DURATIONS.iter().enumerate() {
            let span = (seconds * fps as f32).round() as u32;
            if remaining < span {
                self.chapter = chapter;
                self.progress = remaining as f32 / (span - 1) as f32;
                return;
            }
            remaining -= span;
        }
    }
    pub fn plane_focus(&self) -> f32 {
        if self.chapter != 4 {
            return 0.;
        }
        eased(self.progress / 0.12).min(eased((1. - self.progress) / 0.12))
    }
    /// Reveal the embedding without changing the mathematical square.
    pub fn reveal_progress(&self) -> f32 {
        if self.chapter == 0 {
            ((self.progress * DURATIONS[0] - 2.8) / 3.6).clamp(0., 1.)
        } else if self.chapter == 12 {
            1. - cinematic((self.progress * 3.5 - 0.25) / 2.75)
        } else if self.chapter == 13 {
            0.
        } else {
            1.
        }
    }
    pub fn camera_angles(&self) -> (f32, f32) {
        let u = self.reveal_progress();
        let top = std::f32::consts::FRAC_PI_2;
        if u <= 0. {
            return (0., top);
        }
        if u >= 1. {
            return (self.yaw, self.pitch);
        }
        let blend = cinematic((u - 0.12) / 0.88);
        (self.yaw * blend, top + (self.pitch - top) * blend)
    }
    pub fn camera_zoom(&self) -> f32 {
        self.zoom * (1. + 0.75 * (1. - cinematic(self.reveal_progress() / 0.65)))
    }
    /// Plane, origin/rays, then ambient cell: each becomes visible in order.
    pub fn reveal_layers(&self) -> (f32, f32, f32) {
        let u = self.reveal_progress();
        (
            cinematic((u - 0.30) / 0.20),
            cinematic((u - 0.40) / 0.25),
            cinematic((u - 0.62) / 0.28),
        )
    }
    /// Introduce the full ground patch once the camera has room for its edges.
    pub fn ground_visibility(&self) -> f32 {
        cinematic((self.reveal_progress() - 0.8) / 0.15)
    }
    /// The principal scaling direction joins the selected opposite vertices.
    pub fn diagonal_axis(&self) -> Vec2 {
        Vec2::new(1., -1.).normalize()
    }
    pub fn perspective_coefficient(&self) -> f32 {
        -0.35
            * match self.chapter {
                10 => eased(self.progress / 0.8),
                11 | 12 => 1.,
                13 => 1. - eased(self.progress),
                _ => 0.,
            }
    }
    pub fn projection_amount(&self) -> f32 {
        match self.chapter {
            11 => eased(self.progress / 0.8),
            12 | 13 => 1.,
            _ => 0.,
        }
    }
    pub fn projected_color(&self) -> egui::Color32 {
        let teal = if self.chapter == 13 {
            1. - eased(self.progress)
        } else {
            1.
        };
        let blend =
            |gold: u8, blue: u8| (gold as f32 + (blue as f32 - gold as f32) * teal).round() as u8;
        egui::Color32::from_rgb(blend(246, 107), blend(197, 223), blend(106, 203))
    }
    pub fn amount(&self) -> f32 {
        eased(self.progress)
    }
    /// Keep the square's orientation fixed while its center follows this path.
    pub fn translation_path(&self) -> Vec2 {
        let p = self.progress.clamp(0., 1.);
        let radius = 0.95;
        let start = Vec2::new(self.yaw.cos(), -self.yaw.sin()) * radius;
        if p <= 1. / 6. {
            start * eased(p * 6.)
        } else if p < 5. / 6. {
            let angle = std::f32::consts::TAU * eased((p - 1. / 6.) * 1.5);
            bevy::math::Mat2::from_angle(angle) * start
        } else {
            start * eased((1. - p) * 6.)
        }
    }
    pub fn matrix(&self) -> Mat3 {
        if self.chapter >= 10 {
            let p = self.perspective_coefficient() * 0.5;
            // Change height along the screen-separated (1, -1) diagonal.
            // Its opposite vertices move out/in; the other two stay on w=1.
            return Mat3::from_cols(Vec3::new(1., 0., p), Vec3::new(0., 1., -p), Vec3::Z);
        }
        let identity = Affine {
            angle: 0.,
            scale: Vec2::ONE,
            shear: 0.,
            translation: Vec2::ZERO,
        };
        if self.chapter == 0 {
            let phase = if self.progress <= CAMERA_TILT_END {
                (self.progress / FLAT_ROTATION_END).clamp(0., 1.)
            } else {
                (self.progress - CAMERA_TILT_END) / (1. - CAMERA_TILT_END)
            };
            let amount = if phase < 0.5 {
                eased(phase * 2.)
            } else {
                eased((1. - phase) * 2.)
            };
            return Affine {
                angle: 180. * amount,
                ..identity
            }
            .matrix(1., true, false);
        }
        if self.chapter == 1 || self.chapter == 3 {
            // Identity -> positive extreme -> opposite extreme -> identity.
            // The middle leg passes through identity without stopping.
            let amount = bipolar_sweep(self.progress);
            return Affine {
                shear: if self.chapter == 1 {
                    self.affine.shear * amount
                } else {
                    0.
                },
                scale: if self.chapter == 3 {
                    Vec2::new(
                        self.affine.scale.x.powf(amount),
                        self.affine.scale.y.powf(amount),
                    )
                } else {
                    Vec2::ONE
                },
                ..identity
            }
            .matrix(1., true, false);
        }
        if self.chapter == 4 {
            let u = self.diagonal_axis();
            let n = Vec2::new(-u.y, u.x);
            // Two perpendicular eigenvectors, with reciprocal positive eigenvalues.
            let k = 1.6_f32.powf(bipolar_sweep(self.progress));
            let along = k - 1.;
            let across = k.recip() - 1.;
            return Mat3::from_cols(
                (Vec2::X + u * (along * u.x) + n * (across * n.x)).extend(0.),
                (Vec2::Y + u * (along * u.y) + n * (across * n.y)).extend(0.),
                Vec3::Z,
            );
        }
        if self.chapter == 2 {
            return Affine {
                translation: self.translation_path(),
                ..identity
            }
            .matrix(1., false, true);
        }
        let scaled = Affine {
            scale: self.affine.scale,
            ..identity
        };
        let sheared = Affine {
            shear: self.affine.shear,
            ..scaled
        };
        let rotated = Affine {
            angle: self.affine.angle,
            ..sheared
        };
        // Each affine step adds one operation. Restore the square before
        // changing height in the perspective sequence.
        let poses = [
            identity,
            identity,
            identity,
            identity,
            identity,
            identity,
            scaled,
            sheared,
            rotated,
            self.affine,
            identity,
        ];
        let a = poses[self.chapter];
        let b = poses[self.chapter + 1];
        let u = self.amount();
        let mixed = Affine {
            angle: a.angle + (b.angle - a.angle) * u,
            scale: a.scale.lerp(b.scale, u),
            shear: a.shear + (b.shear - a.shear) * u,
            translation: a.translation.lerp(b.translation, u),
        };
        mixed.matrix(1., true, true)
    }
}

/// Minimum-jerk camera motion, with zero velocity and acceleration at each end.
fn cinematic(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * t * (10. + t * (-15. + 6. * t))
}

fn bipolar_sweep(progress: f32) -> f32 {
    let p = progress.clamp(0., 1.);
    if p < 0.25 {
        eased(p * 4.)
    } else if p < 0.75 {
        1. - 2. * eased((p - 0.25) * 2.)
    } else {
        -1. + eased((p - 0.75) * 4.)
    }
}

pub fn render(mut contexts: EguiContexts, frame: Res<Frame>) -> Result {
    paint(contexts.ctx_mut()?, &frame);
    Ok(())
}
fn paint(ctx: &egui::Context, frame: &Frame) {
    let rect = ctx.viewport_rect();
    let p = ctx.layer_painter(egui::LayerId::background());
    p.rect_filled(rect, 0., egui::Color32::from_rgb(12, 17, 26));
    let seconds = frame.progress * DURATIONS[frame.chapter];
    let (title, title_time, title_duration) = if frame.chapter == 0 {
        if seconds < 7. {
            ("", seconds, 7.)
        } else {
            ("Rotation", seconds - 7., 3.)
        }
    } else {
        (CHAPTERS[frame.chapter], seconds, DURATIONS[frame.chapter])
    };
    // The same name remains visible throughout both directions of each motion.
    let fade_in = if frame.chapter > 0 && CHAPTERS[frame.chapter - 1] == title {
        1.
    } else {
        (title_time / 0.15).clamp(0., 1.)
    };
    let fade_out = if CHAPTERS.get(frame.chapter + 1) == Some(&title) {
        1.
    } else {
        ((title_duration - title_time) / 0.15).clamp(0., 1.)
    };
    let alpha = fade_in.min(fade_out);
    let mut label_p = p.clone();
    label_p.set_opacity(alpha);
    label_p.text(
        egui::pos2(rect.center().x, 48.),
        egui::Align2::CENTER_TOP,
        title,
        egui::FontId::proportional(30.),
        scene::INK,
    );
    let stage = egui::Rect::from_min_max(
        rect.min + egui::vec2(50., 85.),
        rect.max - egui::vec2(50., 35.),
    );
    scene::draw(&p.with_clip_rect(stage), stage, frame);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timing_covers_each_shot_at_supported_frame_rates() {
        for fps in 12..=60 {
            let mut s = Frame::default();
            let mut start = 0;
            for (chapter, seconds) in DURATIONS.iter().enumerate() {
                let span = (seconds * fps as f32).round() as u32;
                s.seek(start, fps);
                assert_eq!(s.chapter, chapter);
                assert_eq!(s.progress, 0.);
                s.seek(start + span - 1, fps);
                assert_eq!(s.progress, 1.);
                start += span;
            }
            assert_eq!(start, total_frames(fps));
        }
    }
    #[test]
    fn transformations_join_continuously_and_return_exactly_to_identity() {
        for chapter in 0..CHAPTERS.len() {
            let a = Frame {
                chapter,
                progress: 1.,
                ..default()
            };
            let b = Frame {
                chapter: (chapter + 1) % CHAPTERS.len(),
                progress: 0.,
                ..default()
            };
            assert!(
                (a.matrix() - b.matrix())
                    .to_cols_array()
                    .iter()
                    .all(|v| v.abs() < 1e-6)
            );
            for i in 0..121 {
                let f = Frame {
                    chapter,
                    progress: i as f32 / 120.,
                    ..default()
                };
                assert!(f.matrix().determinant() > 0.1);
            }
        }
        assert_eq!(
            Frame {
                chapter: CHAPTERS.len() - 1,
                progress: 1.,
                ..default()
            }
            .matrix(),
            Mat3::IDENTITY
        );
    }
    #[test]
    fn both_2d_and_3d_rotations_go_half_turn_then_retrace() {
        for (start, span) in [
            (0., FLAT_ROTATION_END),
            (CAMERA_TILT_END, 1. - CAMERA_TILT_END),
        ] {
            let mut previous = Vec3::X;
            let mut forward = 0.;
            let mut backward = 0.;
            for i in 1..=240 {
                let f = Frame {
                    chapter: 0,
                    progress: start + span * i as f32 / 240.,
                    ..default()
                };
                if start > 0. {
                    let camera = f.camera_angles();
                    assert!((camera.0 - f.yaw).abs() < 1e-6 && (camera.1 - f.pitch).abs() < 1e-6);
                }
                let m = f.matrix();
                let next = m * Vec3::X;
                let step = previous.cross(next).z.atan2(previous.dot(next));
                if i <= 120 {
                    assert!(step >= -1e-6);
                    forward += step;
                } else {
                    assert!(step <= 1e-6);
                    backward += step;
                }
                if i == 120 {
                    assert!((next + Vec3::X).length() < 1e-6);
                }
                assert!((next.length() - 1.).abs() < 1e-6);
                assert_eq!(m * Vec3::Z, Vec3::Z);
                assert_eq!(m * Vec3::ZERO, Vec3::ZERO);
                previous = next;
            }
            assert!((forward - std::f32::consts::PI).abs() < 1e-5);
            assert!((backward + std::f32::consts::PI).abs() < 1e-5);
            assert_eq!(previous, Vec3::X);
        }
    }
    #[test]
    fn shear_changes_slant_without_adding_rotation() {
        for i in 0..=120 {
            let f = Frame {
                chapter: 1,
                progress: i as f32 / 120.,
                ..default()
            };
            assert_eq!(f.matrix() * Vec3::X, Vec3::X);
            assert_eq!(f.matrix() * Vec3::Z, Vec3::Z);
            assert!((f.matrix().determinant() - 1.).abs() < 1e-6);
        }
    }
    #[test]
    fn shear_returns_to_the_square_before_pure_translation_begins() {
        let start = Frame {
            chapter: 1,
            progress: 0.,
            ..default()
        };
        let peak = Frame {
            chapter: 1,
            progress: 0.25,
            ..default()
        };
        let end = Frame {
            chapter: 1,
            progress: 1.,
            ..default()
        };
        assert_eq!(start.matrix(), Mat3::IDENTITY);
        assert_eq!(end.matrix(), Mat3::IDENTITY);
        assert_eq!((peak.matrix() * Vec3::Y).x, peak.affine.shear);
        for i in 0..=120 {
            let f = Frame {
                chapter: 2,
                progress: i as f32 / 120.,
                ..default()
            };
            // Translation changes the center but preserves both square edges.
            assert_eq!(f.matrix() * Vec3::X, Vec3::X);
            assert_eq!(f.matrix() * Vec3::Y, Vec3::Y);
        }
    }
    #[test]
    fn origin_and_affine_slice_invariants() {
        for chapter in 0..CHAPTERS.len() {
            for progress in [0., 0.4, 0.8, 1.] {
                let s = Frame {
                    chapter,
                    progress,
                    ..default()
                };
                assert_eq!(s.matrix() * Vec3::ZERO, Vec3::ZERO);
                if chapter < 10 {
                    assert_eq!((s.matrix() * Vec3::ONE).z, 1.);
                }
            }
        }
    }
    #[test]
    fn standalone_scaling_returns_to_square_before_anisotropic_scaling() {
        for progress in [0., 1.] {
            assert_eq!(
                Frame {
                    chapter: 3,
                    progress,
                    ..default()
                }
                .matrix(),
                Mat3::IDENTITY
            );
        }
        let peak = Frame {
            chapter: 3,
            progress: 0.25,
            ..default()
        };
        assert_eq!(peak.matrix().x_axis, Vec3::X * peak.affine.scale.x);
        assert_eq!(peak.matrix().y_axis, Vec3::Y * peak.affine.scale.y);
        assert_eq!(peak.matrix().z_axis, Vec3::Z);
    }
    #[test]
    fn scale_and_shear_visit_both_extremes_and_finish_at_identity() {
        for chapter in [1, 3] {
            let state = |progress| {
                Frame {
                    chapter,
                    progress,
                    ..default()
                }
                .matrix()
            };
            let positive = state(0.25);
            let negative = state(0.75);
            assert!(
                (positive * negative - Mat3::IDENTITY)
                    .to_cols_array()
                    .iter()
                    .all(|v| v.abs() < 1e-6)
            );
            for progress in [0., 0.5, 1.] {
                assert_eq!(state(progress), Mat3::IDENTITY);
            }
            if chapter == 1 {
                assert!(positive.y_axis.x > 0. && negative.y_axis.x < 0.);
            } else {
                assert!(positive.x_axis.x > 1. && positive.y_axis.y < 1.);
                assert!(negative.x_axis.x < 1. && negative.y_axis.y > 1.);
            }
        }
    }
    #[test]
    fn translation_traces_a_circle_without_rotating_or_leaving_the_plane() {
        let state = |progress| Frame {
            chapter: 2,
            progress,
            ..default()
        };
        assert_eq!(state(0.).matrix(), Mat3::IDENTITY);
        assert_eq!(state(1.).matrix(), Mat3::IDENTITY);
        let start = state(1. / 6.).translation_path();
        let mut previous = start;
        let mut swept = 0.;
        for i in 1..=240 {
            let f = state(1. / 6. + (2. / 3.) * i as f32 / 240.);
            let t = f.translation_path();
            assert!((t.length() - 0.95).abs() < 1e-6);
            let step = previous.perp_dot(t).atan2(previous.dot(t));
            assert!(step >= -1e-6);
            swept += step;
            previous = t;
        }
        assert!((swept - std::f32::consts::TAU).abs() < 1e-5);
        for i in 0..=240 {
            let f = state(i as f32 / 240.);
            let m = f.matrix();
            assert_eq!(m.x_axis, Vec3::X);
            assert_eq!(m.y_axis, Vec3::Y);
            assert_eq!(m * Vec3::ZERO, Vec3::ZERO);
            for p in scene::sample() {
                let q = m * p;
                assert_eq!(q.z, 1.);
                assert!(q.x.abs() < 2. && q.y.abs() < 2.);
            }
        }
        let f = state(1. / 6.);
        let depth_axis = Vec2::new(f.yaw.sin(), f.yaw.cos());
        assert!(start.dot(depth_axis).abs() < 1e-6);
    }
    #[test]
    fn opening_is_face_on_and_camera_joins_match_including_the_loop() {
        let first = Frame::default();
        assert_eq!(first.camera_angles(), (0., std::f32::consts::FRAC_PI_2));
        for progress in [0., 0.1, FLAT_ROTATION_END * 0.5, FLAT_ROTATION_END] {
            let f = Frame {
                chapter: 0,
                progress,
                ..default()
            };
            assert_eq!(f.camera_angles(), first.camera_angles());
        }
        for chapter in 0..CHAPTERS.len() {
            let a = Frame {
                chapter,
                progress: 1.,
                ..default()
            }
            .camera_angles();
            let b = Frame {
                chapter: (chapter + 1) % CHAPTERS.len(),
                progress: 0.,
                ..default()
            }
            .camera_angles();
            assert!((a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6);
        }
        for progress in [
            FLAT_ROTATION_END,
            (FLAT_ROTATION_END + CAMERA_TILT_END) * 0.5,
            CAMERA_TILT_END,
            1.,
        ] {
            assert_eq!(
                Frame {
                    chapter: 0,
                    progress,
                    ..default()
                }
                .matrix(),
                Mat3::IDENTITY
            );
        }
        {
            let progress = 1.;
            assert_eq!(
                Frame {
                    chapter: CHAPTERS.len() - 1,
                    progress,
                    ..default()
                }
                .matrix(),
                Mat3::IDENTITY
            );
        }
    }
    #[test]
    fn embedding_reveal_keeps_square_still_and_stages_context_without_zoom_snap() {
        let frame_at = |seconds: f32| Frame {
            chapter: 0,
            progress: seconds / DURATIONS[0],
            ..default()
        };
        for i in 0..=180 {
            let f = frame_at(2.55 + 4.45 * i as f32 / 180.);
            assert_eq!(f.matrix(), Mat3::IDENTITY);
        }
        let mut previous = frame_at(2.8);
        for i in 1..=108 {
            let f = frame_at(2.8 + i as f32 / 30.);
            assert!((f.camera_zoom() - previous.camera_zoom()).abs() < 0.025);
            assert!((f.camera_angles().1 - previous.camera_angles().1).abs() < 0.025);
            assert!(f.camera_zoom() <= previous.camera_zoom() + 1e-6);
            previous = f;
        }
        let layers_at = |u: f32| frame_at(2.8 + 3.6 * u).reveal_layers();
        let (plane, rays, space) = layers_at(0.35);
        assert!(plane > 0. && rays == 0. && space == 0.);
        let (_, rays, space) = layers_at(0.55);
        assert!(rays > 0. && space == 0.);
        assert_eq!(layers_at(1.), (1., 1., 1.));
        for chapter in 0..CHAPTERS.len() {
            let a = Frame {
                chapter,
                progress: 1.,
                ..default()
            };
            let b = Frame {
                chapter: (chapter + 1) % CHAPTERS.len(),
                progress: 0.,
                ..default()
            };
            assert!((a.camera_zoom() - b.camera_zoom()).abs() < 1e-6);
            assert_eq!(a.reveal_layers(), b.reveal_layers());
        }
    }
    #[test]
    fn diagonal_scaling_keeps_camera_fixed_and_uses_the_selected_vertex_pair() {
        for i in 0..=120 {
            let f = Frame {
                chapter: 4,
                progress: i as f32 / 120.,
                ..default()
            };
            assert_eq!(f.camera_angles(), (f.yaw, f.pitch));
            let view = scene::View {
                rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920., 1080.)),
                yaw: f.yaw,
                pitch: f.pitch,
                zoom: f.zoom,
            };
            let axis = f.diagonal_axis();
            let d = view.project((axis * 2.).extend(1.)) - view.project(Vec3::Z);
            assert!(d.length() > 150.);
            assert!(d.x > 0. && d.y < 0.);
            for vertex in [scene::sample()[1], scene::sample()[3]] {
                assert!(vertex.truncate().perp_dot(axis).abs() < 1e-6);
                assert!((f.matrix() * vertex).truncate().perp_dot(axis).abs() < 1e-6);
            }
        }
        for progress in [0., 1.] {
            let f = Frame {
                chapter: 4,
                progress,
                ..default()
            };
            assert_eq!(f.matrix(), Mat3::IDENTITY);
            assert_eq!(f.plane_focus(), 0.);
        }
    }
    #[test]
    fn diagonal_scaling_has_reciprocal_eigenvalues_and_preserves_the_plane() {
        for i in 0..=120 {
            let f = Frame {
                chapter: 4,
                progress: i as f32 / 120.,
                ..default()
            };
            let m = f.matrix();
            let u = f.diagonal_axis().extend(0.);
            let n = Vec3::new(-u.y, u.x, 0.);
            let a = m * u;
            let b = m * n;
            assert!(a.cross(u).length() < 1e-6 && b.cross(n).length() < 1e-6);
            assert!(a.dot(u) > 0. && b.dot(n) > 0.);
            assert!((a.length() * b.length() - 1.).abs() < 1e-6);
            assert!((m.determinant() - 1.).abs() < 1e-6);
            assert_eq!(m * Vec3::Z, Vec3::Z);
            assert_eq!(m * Vec3::ZERO, Vec3::ZERO);
            for p in scene::sample() {
                assert_eq!((m * p).z, 1.);
            }
        }
        let f = Frame {
            chapter: 4,
            progress: 0.25,
            ..default()
        };
        let u = f.diagonal_axis().extend(0.);
        let n = Vec3::new(-u.y, u.x, 0.);
        assert!((f.matrix() * u - u * 1.6).length() < 1e-6);
        assert!((f.matrix() * n - n / 1.6).length() < 1e-6);
        let inverse = Frame {
            chapter: 4,
            progress: 0.75,
            ..default()
        }
        .matrix();
        assert!(
            (f.matrix() * inverse - Mat3::IDENTITY)
                .to_cols_array()
                .iter()
                .all(|v| v.abs() < 1e-6)
        );
        for progress in [0., 0.5, 1.] {
            assert_eq!(
                Frame {
                    chapter: 4,
                    progress,
                    ..default()
                }
                .matrix(),
                Mat3::IDENTITY
            );
        }
    }
    #[test]
    fn affine_steps_add_one_operation_at_a_time() {
        let state = |chapter, progress| Frame {
            chapter,
            progress,
            ..default()
        };
        let scale = state(5, 1.).matrix();
        let f = Frame::default();
        assert_eq!(scale * Vec3::X, Vec3::X * f.affine.scale.x);
        assert_eq!(scale * Vec3::Y, Vec3::Y * f.affine.scale.y);
        assert_eq!(scale * Vec3::Z, Vec3::Z);
        let shear = Affine {
            angle: 0.,
            scale: Vec2::ONE,
            shear: f.affine.shear,
            translation: Vec2::ZERO,
        }
        .matrix(1., true, false);
        let rotation = Mat3::from_rotation_z(f.affine.angle.to_radians());
        assert!(
            (state(6, 1.).matrix() - shear * scale)
                .to_cols_array()
                .iter()
                .all(|v| v.abs() < 1e-6)
        );
        assert!(
            (state(7, 1.).matrix() - rotation * shear * scale)
                .to_cols_array()
                .iter()
                .all(|v| v.abs() < 1e-6)
        );
        for i in 0..=120 {
            let m = state(8, i as f32 / 120.).matrix();
            assert!((m.x_axis - state(7, 1.).matrix().x_axis).length() < 1e-6);
            assert!((m.y_axis - state(7, 1.).matrix().y_axis).length() < 1e-6);
        }
    }
    #[test]
    fn perspective_normalizes_on_fixed_rays_and_closes_the_loop() {
        for i in 0..=120 {
            let progress = i as f32 / 120.;
            let shear = Frame {
                chapter: 10,
                progress,
                ..default()
            };
            assert_eq!(shear.projection_amount(), 0.);
            let f = Frame {
                chapter: 11,
                progress,
                ..default()
            };
            assert_eq!(f.perspective_coefficient(), -0.35);
            assert_eq!(f.camera_angles(), (f.yaw, f.pitch));
            for vertex in scene::sample() {
                let raw = f.matrix() * vertex;
                let moving = raw.lerp(raw / raw.z, f.projection_amount());
                if vertex.x == vertex.y {
                    assert_eq!(raw, vertex);
                    assert_eq!(moving, vertex);
                } else if vertex.x > vertex.y {
                    assert!((raw.z - 0.65).abs() < 1e-6);
                } else {
                    assert!((raw.z - 1.35).abs() < 1e-6);
                }
                assert!(moving.cross(raw).length() < 1e-6);
                if raw.z < 1. {
                    assert!(moving.length() >= raw.length() - 1e-6);
                } else {
                    assert!(moving.length() <= raw.length() + 1e-6);
                }
                if progress >= 0.8 {
                    assert_eq!(moving.z, 1.);
                }
            }
        }
        let visible = |f: &Frame, vertex| {
            let raw: Vec3 = f.matrix() * vertex;
            raw.lerp(raw / raw.z, f.projection_amount())
        };
        for chapter in 0..CHAPTERS.len() {
            let a = Frame {
                chapter,
                progress: 1.,
                ..default()
            };
            let b = Frame {
                chapter: (chapter + 1) % CHAPTERS.len(),
                progress: 0.,
                ..default()
            };
            for vertex in scene::sample() {
                assert_eq!(visible(&a, vertex), visible(&b, vertex));
            }
        }
        let last = Frame {
            chapter: 13,
            progress: 1.,
            ..default()
        };
        assert_eq!(last.projected_color(), scene::GOLD);
        assert_eq!(last.ground_visibility(), 0.);
    }
    #[test]
    fn all_film_scenes_tessellate_without_nonfinite_vertices() {
        let ctx = egui::Context::default();
        for chapter in 0..CHAPTERS.len() {
            ctx.begin_pass(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1920., 1080.),
                )),
                ..default()
            });
            paint(
                &ctx,
                &Frame {
                    chapter,
                    progress: 0.8,
                    ..default()
                },
            );
            let mut out = ctx.end_pass();
            out.textures_delta.clear();
            for primitive in ctx.tessellate(out.shapes, out.pixels_per_point) {
                if let egui::epaint::Primitive::Mesh(m) = primitive.primitive {
                    assert!(
                        m.vertices
                            .iter()
                            .all(|v| v.pos.x.is_finite() && v.pos.y.is_finite())
                    );
                }
            }
        }
    }
}
