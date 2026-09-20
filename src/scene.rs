//! Orthographic projection of a real 3D mathematical scene into antialiased
//! vector primitives. This keeps labels and fine grid lines crisp at any DPI.
use crate::film::Frame;
use bevy::math::{Mat3, Vec3};
use bevy_egui::egui::{self, Color32, Painter, Pos2, Rect, Shape, Stroke};

pub const INK: Color32 = Color32::from_rgb(232, 236, 244);
pub const MUTED: Color32 = Color32::from_rgb(150, 153, 159);
pub const GOLD: Color32 = Color32::from_rgb(246, 197, 106);
const GROUND_EXTENT: f32 = 2.5;

pub struct View {
    pub rect: Rect,
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
}
impl View {
    pub fn project(&self, v: Vec3) -> Pos2 {
        let v = Vec3::new(v.x, v.y, (v.z - 0.65) * 2.2);
        let right = Vec3::new(self.yaw.cos(), -self.yaw.sin(), 0.);
        let up = Vec3::new(
            -self.yaw.sin() * self.pitch.sin(),
            -self.yaw.cos() * self.pitch.sin(),
            self.pitch.cos(),
        );
        let unit = (self.rect.width() * 0.105).min(self.rect.height() * 0.16) * self.zoom;
        self.rect.center() + egui::vec2(v.dot(right) * unit, -v.dot(up) * unit)
    }
    fn line(&self, p: &Painter, a: Vec3, b: Vec3, color: Color32, width: f32) {
        p.line_segment(
            [self.project(a), self.project(b)],
            Stroke::new(width, color),
        );
    }
    fn arrow(&self, p: &Painter, a: Vec3, b: Vec3, color: Color32, width: f32) {
        arrow(p, self.project(a), self.project(b), color, width);
    }
}

pub fn arrow(p: &Painter, a: Pos2, b: Pos2, color: Color32, width: f32) {
    let d = b - a;
    if d.length() < 1.0 {
        return;
    }
    let dir = d.normalized();
    let side = egui::vec2(-dir.y, dir.x);
    let len = 10.0_f32.min(d.length() * 0.3);
    p.line_segment([a, b], Stroke::new(width, color));
    p.add(Shape::convex_polygon(
        vec![
            b,
            b - dir * len + side * len * 0.42,
            b - dir * len - side * len * 0.42,
        ],
        color,
        Stroke::NONE,
    ));
}
fn dashed(p: &Painter, a: Pos2, b: Pos2, color: Color32) {
    let n = ((b - a).length() / 10.).max(1.) as usize;
    for i in 0..n {
        p.line_segment(
            [
                a.lerp(b, i as f32 / n as f32),
                a.lerp(b, (i as f32 + 0.45) / n as f32),
            ],
            Stroke::new(1., color),
        );
    }
}

/// Stationary world reference: this grid is never multiplied by the film matrix.
fn ground(p: &Painter, view: &View, visibility: f32) {
    let mut ground_p = p.clone();
    ground_p.multiply_opacity(visibility);
    let p = &ground_p;
    let e = GROUND_EXTENT;
    let corners = [
        Vec3::new(-e, -e, 0.),
        Vec3::new(e, -e, 0.),
        Vec3::new(e, e, 0.),
        Vec3::new(-e, e, 0.),
    ];
    polygon(
        p,
        view,
        &corners,
        Color32::from_rgba_unmultiplied(150, 153, 159, 8),
        MUTED.gamma_multiply(0.23),
        0.8,
    );
    for i in -1..=1 {
        let a = i as f32 * e * 0.5;
        let alpha = if i == 0 { 0.48 } else { 0.20 };
        view.line(
            p,
            Vec3::new(a, -e, 0.),
            Vec3::new(a, e, 0.),
            MUTED.gamma_multiply(alpha),
            0.9,
        );
        view.line(
            p,
            Vec3::new(-e, a, 0.),
            Vec3::new(e, a, 0.),
            MUTED.gamma_multiply(alpha),
            0.9,
        );
    }
}

/// Contact rings lie in the origin plane, so their shape follows its perspective.
fn contact(p: &Painter, view: &View, center: Vec3, visibility: f32) {
    let ring: Vec<_> = (0..24)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / 24.;
            center + Vec3::new(angle.cos(), angle.sin(), 0.) * 0.045
        })
        .collect();
    polygon(
        p,
        view,
        &ring,
        Color32::TRANSPARENT,
        INK.gamma_multiply(0.85 * visibility),
        1.2,
    );
}

fn polygon(p: &Painter, view: &View, verts: &[Vec3], fill: Color32, edge: Color32, width: f32) {
    p.add(Shape::convex_polygon(
        verts.iter().map(|v| view.project(*v)).collect(),
        fill,
        Stroke::new(width, edge),
    ));
}
pub fn sample() -> [Vec3; 4] {
    [
        Vec3::new(-1., -1., 1.),
        Vec3::new(1., -1., 1.),
        Vec3::new(1., 1., 1.),
        Vec3::new(-1., 1., 1.),
    ]
}
/// Twelve edges of a 3D cell that encloses the square's original slice.
fn cage_edges() -> Vec<(Vec3, Vec3)> {
    let ring = [(-1.2, -1.2), (1.2, -1.2), (1.2, 1.2), (-1.2, 1.2)];
    let mut edges = Vec::with_capacity(12);
    for i in 0..4 {
        let (x, y) = ring[i];
        let (u, v) = ring[(i + 1) % 4];
        for w in [0., 1.25] {
            edges.push((Vec3::new(x, y, w), Vec3::new(u, v, w)));
        }
        edges.push((Vec3::new(x, y, 0.), Vec3::new(x, y, 1.25)));
    }
    edges
}

pub fn draw(p: &Painter, rect: Rect, s: &Frame) {
    let (yaw, pitch) = s.camera_angles();
    let plane_focus = s.plane_focus();
    let (plane_visibility, rays_visibility, space_visibility) = s.reveal_layers();
    let view = View {
        rect,
        yaw,
        pitch,
        zoom: s.camera_zoom(),
    };
    let m = s.matrix();
    let projection = s.projection_amount();
    let perspective = s.chapter >= 10;
    let normalization_focus = if s.chapter >= 11 {
        (projection / 0.08).min(1.)
    } else {
        0.
    };
    let raw_alpha = if s.chapter >= 12 {
        s.reveal_progress().powi(2)
    } else {
        1.
    };
    let changed = (m - Mat3::IDENTITY)
        .to_cols_array()
        .iter()
        .any(|v| v.abs() > 1e-5);
    let corners = [
        Vec3::new(-2., -2., 1.),
        Vec3::new(2., -2., 1.),
        Vec3::new(2., 2., 1.),
        Vec3::new(-2., 2., 1.),
    ];
    // The stationary ground settles into view as the camera opens.
    let mut ground_p = p.clone();
    ground_p.multiply_opacity(1. - 0.5 * normalization_focus);
    ground(&ground_p, &view, s.ground_visibility());
    // The upper plane stays quiet; the base grid supplies the fixed orientation.
    polygon(
        p,
        &view,
        &corners,
        Color32::from_rgba_unmultiplied(150, 153, 159, (9. * plane_visibility) as u8),
        MUTED.gamma_multiply(0.42 * plane_visibility),
        0.8,
    );
    view.line(
        p,
        Vec3::ZERO,
        Vec3::Z * 1.32,
        MUTED.gamma_multiply(0.45 * rays_visibility),
        1.,
    );
    if s.lattice {
        let mut cage_p = p.clone();
        cage_p.multiply_opacity(1. - 0.85 * normalization_focus);
        let p = &cage_p;
        // The same H acts on every edge of this sparse 3D reference cell.
        // In translation, w=0 stays fixed while the upper face shears by 1.25t.
        let base: Vec<_> = sample()
            .iter()
            .map(|v| m * (v.truncate() * 1.2).extend(0.))
            .collect();
        polygon(
            p,
            &view,
            &base,
            Color32::from_rgba_unmultiplied(150, 153, 159, (13. * space_visibility) as u8),
            INK.gamma_multiply(0.68 * space_visibility),
            1.8,
        );
        for vertex in &base {
            contact(
                p,
                &view,
                *vertex,
                space_visibility * (1. - s.perspective_coefficient().abs() / 0.06).clamp(0., 1.),
            );
        }
        for (a, b) in cage_edges() {
            if a.z == 0. && b.z == 0. {
                continue; // The brighter footprint already draws the bottom edges.
            }
            view.line(
                p,
                m * a,
                m * b,
                MUTED.gamma_multiply(0.55 * space_visibility),
                1.35,
            );
        }
    }
    if s.chapter == 4 || s.chapter == 2 {
        // In-plane guides show the perpendicular scaling directions and orbit.
        let alpha = (s.progress / 0.07)
            .clamp(0., 1.)
            .min(((1. - s.progress) / 0.07).clamp(0., 1.));
        if s.chapter == 4 {
            let mut guide = p.clone();
            guide.multiply_opacity(plane_focus);
            let axis = s.diagonal_axis();
            let a = (-axis * 2.55).extend(1.);
            let b = (axis * 2.55).extend(1.);
            view.line(&guide, a, b, MUTED.gamma_multiply(0.8), 1.2);
            let perpendicular = bevy::math::Vec2::new(-axis.y, axis.x);
            view.line(
                &guide,
                (-perpendicular * 2.55).extend(1.),
                (perpendicular * 2.55).extend(1.),
                MUTED.gamma_multiply(0.45),
                1.,
            );
        } else {
            for i in 0..96 {
                let point = |t: f32| Vec3::new(0.95 * t.cos(), 0.95 * t.sin(), 1.);
                let a = std::f32::consts::TAU * i as f32 / 96.;
                let b = std::f32::consts::TAU * (i as f32 + 0.5) / 96.;
                view.line(p, point(a), point(b), MUTED.gamma_multiply(0.5 * alpha), 1.);
            }
            p.circle_filled(view.project(m * Vec3::Z), 3., GOLD.gamma_multiply(alpha));
        }
    }
    // Preserve the opening mark's screen size while its directions follow the plane.
    let center = view.project(Vec3::Z);
    let cross_half_size = 0.04 * (rect.width() * 0.105).min(rect.height() * 0.16) * s.zoom * 1.75;
    for axis in [Vec3::X, Vec3::Y] {
        let direction = (view.project(Vec3::Z + axis) - center).normalized();
        p.line_segment(
            [
                center - direction * cross_half_size,
                center + direction * cross_half_size,
            ],
            Stroke::new(1., MUTED),
        );
    }
    let original = sample();
    let verts: Vec<_> = original.iter().map(|v| m * *v).collect();
    if s.ghost && changed && !perspective {
        for i in 0..4 {
            dashed(
                p,
                view.project(original[i]),
                view.project(original[(i + 1) % 4]),
                MUTED.gamma_multiply(0.35),
            );
        }
    }
    polygon(
        p,
        &view,
        &verts,
        Color32::from_rgba_unmultiplied(246, 197, 106, (24. * raw_alpha) as u8),
        GOLD.gamma_multiply(raw_alpha * (1. - 0.35 * normalization_focus)),
        2.2,
    );
    let normalized: Vec<_> = verts.iter().map(|v| v.lerp(*v / v.z, projection)).collect();
    let ray_emphasis = normalization_focus;
    if s.rays {
        for (raw, moving) in verts.iter().zip(normalized.iter()) {
            let end = if raw.length_squared() > moving.length_squared() {
                *raw
            } else {
                *moving
            };
            view.line(
                p,
                Vec3::ZERO,
                end,
                MUTED.gamma_multiply((0.6 - 0.4 * normalization_focus) * rays_visibility),
                1.4,
            );
            if ray_emphasis > 0. {
                let alpha = ray_emphasis * rays_visibility;
                // Gold shows the original radial length; teal shows the changing one.
                view.line(p, Vec3::ZERO, *raw, GOLD.gamma_multiply(0.35 * alpha), 1.1);
                view.line(
                    p,
                    Vec3::ZERO,
                    *moving,
                    s.projected_color().gamma_multiply(0.75 * alpha),
                    2.,
                );
            }
        }
    }
    if s.chapter >= 11 {
        let color = s.projected_color();
        polygon(
            p,
            &view,
            &normalized,
            Color32::from_rgba_unmultiplied(
                color.r(),
                color.g(),
                color.b(),
                (24. * ray_emphasis) as u8,
            ),
            color.gamma_multiply(ray_emphasis),
            2.2,
        );
    }
    // Keep the two diagonal displacement arrows above the transparent faces.
    if s.rays && s.chapter >= 11 {
        for (raw, moving) in verts.iter().zip(normalized.iter()) {
            if (*moving - *raw).length_squared() > 1e-8 {
                view.arrow(
                    p,
                    *raw,
                    *moving,
                    s.projected_color()
                        .gamma_multiply(ray_emphasis * rays_visibility),
                    3.,
                );
            }
        }
    }
    let o = m * Vec3::Z;
    if o.truncate().length_squared() > 1e-5 {
        view.arrow(p, Vec3::Z, o, MUTED.gamma_multiply(0.8), 1.2);
    }
    p.circle_filled(
        view.project(Vec3::ZERO),
        3.5,
        INK.gamma_multiply(rays_visibility),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn square_is_centered_at_the_plane_origin_and_has_equal_perpendicular_sides() {
        let q = sample();
        assert_eq!(q.iter().sum::<Vec3>() / 4., Vec3::Z);
        for i in 0..4 {
            let a = q[(i + 1) % 4] - q[i];
            let b = q[(i + 2) % 4] - q[(i + 1) % 4];
            assert_eq!(a.length(), 2.);
            assert_eq!(a.dot(b), 0.);
        }
    }
    #[test]
    fn tighter_framing_keeps_visible_geometry_inside_the_stage() {
        let rect = Rect::from_min_max(egui::pos2(50., 85.), egui::pos2(1870., 1045.));
        for chapter in 0..crate::film::CHAPTERS.len() {
            for sample_index in 0..=120 {
                let s = Frame {
                    chapter,
                    progress: sample_index as f32 / 120.,
                    ..Default::default()
                };
                let (yaw, pitch) = s.camera_angles();
                let (plane_visibility, _, space_visibility) = s.reveal_layers();
                let view = View {
                    rect,
                    yaw,
                    pitch,
                    zoom: s.camera_zoom(),
                };
                let m = s.matrix();
                let mut points: Vec<_> = if s.chapter >= 12 && s.reveal_progress() <= 0.01 {
                    Vec::new()
                } else {
                    sample().iter().map(|v| m * *v).collect()
                };
                if s.chapter >= 11 {
                    points.extend(sample().iter().map(|v| {
                        let raw = m * *v;
                        raw.lerp(raw / raw.z, s.projection_amount())
                    }));
                }
                if s.ground_visibility() > 0.01 {
                    for x in [-GROUND_EXTENT, GROUND_EXTENT] {
                        for y in [-GROUND_EXTENT, GROUND_EXTENT] {
                            points.push(Vec3::new(x, y, 0.));
                        }
                    }
                }
                if space_visibility > 0.01 {
                    for (a, b) in cage_edges() {
                        points.extend([m * a, m * b, a, b]);
                    }
                }
                if plane_visibility > 0.01 {
                    for x in [-2., 2.] {
                        for y in [-2., 2.] {
                            points.push(Vec3::new(x, y, 1.));
                        }
                    }
                }
                for point in points {
                    let screen = view.project(point);
                    assert!(
                        rect.contains(screen),
                        "chapter {chapter}, progress {}, point {point:?} at {screen:?}",
                        s.progress
                    );
                }
            }
        }
    }
    #[test]
    fn every_lower_vertex_stays_on_the_origin_plane_and_inside_the_ground() {
        for chapter in 0..10 {
            for i in 0..=120 {
                let f = Frame {
                    chapter,
                    progress: i as f32 / 120.,
                    ..Default::default()
                };
                for vertex in sample() {
                    let base = f.matrix() * (vertex.truncate() * 1.2).extend(0.);
                    assert_eq!(base.z, 0.);
                    assert!(base.x.abs() + 0.045 < GROUND_EXTENT);
                    assert!(base.y.abs() + 0.045 < GROUND_EXTENT);
                }
            }
        }
    }
    #[test]
    fn ambient_cell_shear_scales_with_height_and_fixes_its_base() {
        let f = Frame {
            chapter: 2,
            progress: 1. / 6.,
            ..Default::default()
        };
        let m = f.matrix();
        for (a, b) in cage_edges() {
            for q in [a, b] {
                let displacement = m * q - q;
                assert!((displacement - f.translation_path().extend(0.) * q.z).length() < 1e-5);
            }
        }
    }
}
