use super::*;
use crate::scene::Aabb;
use crate::settings::Projection;
use crate::viewer::camera::{Pose, StandardView};

fn camera(projection: Projection, target: Vec3, extent: f32, view: StandardView) -> Camera {
    let mut camera = Camera {
        projection,
        ..Camera::default()
    };
    camera.set_pose(Pose {
        target,
        rotation: view.rotation(),
        extent,
    });
    camera
}

#[test]
fn lengths_are_rounded_to_one_two_or_five_and_printed_plainly() {
    let rounded: Vec<f32> = [0.03, 0.7, 1.0, 1.2, 3.0, 7.0, 120.0]
        .into_iter()
        .map(round_length)
        .collect();
    assert_eq!(rounded, [0.05, 1.0, 1.0, 2.0, 5.0, 10.0, 200.0]);
    assert_eq!(round_length(0.0), 1.0);
    assert_eq!(round_length(f32::NAN), 1.0);

    assert_eq!(format_length(2.5, Units::Millimeters), "2.5 mm");
    assert_eq!(format_length(100.0, Units::Inches), "100 in");
    assert_eq!(format_length(0.05, Units::None), "0.05");
    assert_eq!(format_length(1000.0, Units::Meters), "1000 m");
}

#[test]
fn units_per_point_follow_the_zoom_in_both_projections() {
    let orthographic = camera(
        Projection::Orthographic,
        Vec3::ZERO,
        10.0,
        StandardView::Top,
    );
    assert!((units_per_point(&orthographic, 500.0) - 0.04).abs() < 1e-6);
    let perspective = camera(Projection::Perspective, Vec3::ZERO, 10.0, StandardView::Top);
    let expected = 2.0 * perspective.distance() * (perspective.fov_y / 2.0).tan() / 500.0;
    assert!((units_per_point(&perspective, 500.0) - expected).abs() < 1e-6);
}

#[test]
fn the_grid_spacing_is_a_power_of_ten_readable_at_any_zoom() {
    let view = Vec2::new(800.0, 600.0);
    let mut previous: Option<f32> = None;
    for extent in [0.05, 0.3, 2.0, 15.0, 120.0, 900.0] {
        let camera = camera(
            Projection::Orthographic,
            Vec3::new(3.3, -7.1, 2.0),
            extent,
            StandardView::Iso,
        );
        let grid = GroundGrid::for_view(&camera, view);
        let spacing = grid.minor / units_per_point(&camera, view.y);
        assert!(
            (MIN_GRID_SPACING..MIN_GRID_SPACING * 10.0 + 1e-3).contains(&spacing),
            "{spacing} points between lines at extent {extent}"
        );
        let exponent = grid.minor.log10();
        assert!((exponent - exponent.round()).abs() < 1e-4, "{}", grid.minor);
        assert!((0.25..=1.0).contains(&grid.minor_opacity));
        if let Some(previous) = previous {
            assert!(grid.minor >= previous, "zooming out never densifies");
        }
        previous = Some(grid.minor);

        // Centred on a major line near the target, covering the view.
        let major = grid.minor * 10.0;
        assert!(((grid.center / major).round() * major - grid.center).length() < 1e-3);
        assert!((grid.center - camera.target.truncate()).abs().max_element() <= major);
        let view_reach = view.length() / 2.0 * units_per_point(&camera, view.y);
        assert!(grid.half_size() >= view_reach.min(grid.minor * 300.0));
        assert!(grid.lines % 10 == 0 && grid.lines <= 300);
    }
}

#[test]
fn grid_lines_lie_on_the_ground_with_every_tenth_one_major() {
    let camera = camera(
        Projection::Orthographic,
        Vec3::new(5.0, 5.0, 0.0),
        10.0,
        StandardView::Top,
    );
    let grid = GroundGrid::for_view(&camera, Vec2::new(400.0, 400.0));
    let (minor, major) = grid.meshes();
    let lines = grid.lines as usize;
    // Lines both ways, from -lines to +lines.
    assert_eq!(major.edges.len() / 2, (2 * lines / 10 + 1) * 2);
    assert_eq!(
        (minor.edges.len() + major.edges.len()) / 2,
        (2 * lines + 1) * 2
    );
    assert!(
        minor
            .edges
            .iter()
            .chain(&major.edges)
            .all(|point| point[2] == 0.0)
    );
    let [low, high] = grid.corners();
    assert!(
        Aabb::from_points(major.edges.iter().map(|p| Vec3::from_array(*p)))
            .max
            .abs_diff_eq(high, 1e-3)
    );
    assert!((high - low).x > 0.0 && (high - low).z == 0.0);
}

#[test]
fn the_scale_bar_shows_a_round_length_at_least_sixty_points_long() {
    for units_per_point in [0.0003, 0.04, 0.7, 13.0] {
        let bar = ScaleBar::new(units_per_point, Units::Millimeters);
        assert!(
            (60.0..150.0).contains(&bar.width),
            "{} points for {units_per_point}",
            bar.width
        );
        assert_eq!(round_length(bar.length), bar.length);
        assert!((bar.width * units_per_point - bar.length).abs() < bar.length * 1e-4);
        assert_eq!(bar.label, format_length(bar.length, Units::Millimeters));
    }
    assert_eq!(ScaleBar::new(0.1, Units::Inches).label, "10 in");
}
