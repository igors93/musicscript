//! The ground grid on the XY plane and the scale bar (FEAT-C7). Both adapt
//! to the zoom: grid lines are 10^k units apart, at least
//! [`MIN_GRID_SPACING`] points on screen, and the scale bar shows a round
//! length (1, 2 or 5 × 10^k).

use glam::{Vec2, Vec3};

use super::camera::Camera;
use crate::scene::Mesh;
use crate::settings::Units;

/// Closest that minor grid lines get on screen, in points.
pub const MIN_GRID_SPACING: f32 = 12.0;
/// Major lines are every this many minor lines.
const MAJOR_EVERY: i32 = 10;
/// Lines from the centre to each side, at most: enough to cover any view.
const MAX_LINES_PER_SIDE: i32 = 300;
/// The scale bar is at least this long on screen, in points.
const MIN_SCALE_BAR: f32 = 60.0;

/// Model units per screen point at the point the camera looks at.
pub fn units_per_point(camera: &Camera, view_height: f32) -> f32 {
    let half_height = if camera.is_perspective() {
        camera.distance() * (camera.fov_y / 2.0).tan()
    } else {
        camera.extent
    };
    2.0 * half_height / view_height.max(1.0)
}

/// The smallest of 1, 2 and 5 × 10^k that is at least `length`.
pub fn round_length(length: f32) -> f32 {
    let length = if length.is_finite() && length > 0.0 {
        length
    } else {
        1.0
    };
    let exponent = length.log10().floor() as i32;
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|mantissa| decimal(mantissa, exponent))
        .find(|&step| step >= length * (1.0 - 1e-5))
        .unwrap_or_else(|| decimal(1.0, exponent + 1))
}

/// `mantissa` × 10^`exponent`, as close to the decimal as an f32 gets
/// (`5 × 10⁻²` is the literal `0.05`, which f32 arithmetic would miss).
fn decimal(mantissa: f64, exponent: i32) -> f32 {
    (mantissa * 10f64.powi(exponent)) as f32
}

/// A length with its unit, without needless decimals: `2.5 mm`, `100 in`.
pub fn format_length(length: f32, units: Units) -> String {
    let text = format!("{:.6}", length);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    match units.suffix() {
        "" => text.to_owned(),
        suffix => format!("{text} {suffix}"),
    }
}

/// The grid lines for a view: spacing, where they are centred and how far
/// they reach. Two grids are drawn the same when they are equal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundGrid {
    /// Distance between minor lines, a power of ten.
    pub minor: f32,
    /// On a major line, near the point the camera looks at.
    pub center: Vec2,
    /// Minor lines on each side of the centre.
    pub lines: i32,
    /// How visible minor lines are (0 to 1): they fade in as they spread.
    pub minor_opacity: f32,
}

impl GroundGrid {
    /// The grid for `camera` in a view `view` points wide and high.
    pub fn for_view(camera: &Camera, view: Vec2) -> Self {
        let units_per_point = units_per_point(camera, view.y);
        let minor = decimal(
            1.0,
            (MIN_GRID_SPACING * units_per_point).log10().ceil() as i32,
        );
        let major = minor * MAJOR_EVERY as f32;
        let target = camera.target.truncate();
        let center = (target / major).round() * major;
        // Three times the view's half-diagonal, for oblique and perspective
        // views, in whole major cells.
        let reach = view.length() * units_per_point * 1.5 + (target - center).length();
        let needed = (reach / minor).ceil().max(1.0) as i32;
        let lines = ((needed + MAJOR_EVERY - 1) / MAJOR_EVERY * MAJOR_EVERY)
            .clamp(MAJOR_EVERY, MAX_LINES_PER_SIDE);
        // Minor lines fade in from MIN_GRID_SPACING to four times that.
        let spacing = minor / units_per_point;
        let fade = ((spacing - MIN_GRID_SPACING) / (3.0 * MIN_GRID_SPACING)).clamp(0.0, 1.0);
        Self {
            minor,
            center,
            lines,
            minor_opacity: 0.25 + 0.75 * fade,
        }
    }

    /// How far the grid reaches from its centre.
    pub fn half_size(&self) -> f32 {
        self.minor * self.lines as f32
    }

    /// The minor and the major lines, on the plane z = 0.
    pub fn meshes(&self) -> (Mesh, Mesh) {
        let (mut minor, mut major) = (Vec::new(), Vec::new());
        let reach = self.half_size();
        for line in -self.lines..=self.lines {
            let offset = line as f32 * self.minor;
            let lines = if line % MAJOR_EVERY == 0 {
                &mut major
            } else {
                &mut minor
            };
            for (start, end) in [
                (Vec2::new(offset, -reach), Vec2::new(offset, reach)),
                (Vec2::new(-reach, offset), Vec2::new(reach, offset)),
            ] {
                lines.push((self.center + start).extend(0.0).to_array());
                lines.push((self.center + end).extend(0.0).to_array());
            }
        }
        (Mesh::lines(minor), Mesh::lines(major))
    }

    /// Bounds of the grid, for the clipping planes.
    pub fn corners(&self) -> [Vec3; 2] {
        let reach = Vec2::splat(self.half_size());
        [
            (self.center - reach).extend(0.0),
            (self.center + reach).extend(0.0),
        ]
    }
}

/// A scale bar: a round length and how long it is on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct ScaleBar {
    pub length: f32,
    /// In points.
    pub width: f32,
    pub label: String,
}

impl ScaleBar {
    /// The bar for a view with `units_per_point`, at least
    /// [`MIN_SCALE_BAR`] points long.
    pub fn new(units_per_point: f32, units: Units) -> Self {
        let length = round_length(MIN_SCALE_BAR * units_per_point);
        Self {
            length,
            width: length / units_per_point,
            label: format_length(length, units),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/rust/viewer/test_ground.rs"]
mod tests;
