//! User preferences, persisted by eframe between sessions.
//!
//! Defaults follow the original CQ-editor where practical. Compatibility-
//! relevant deviations are recorded in `docs/development/compatibility.md`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::worker::protocol::{Release, RunSettings, SceneSettings};

const MAX_RECENT_FILES: usize = 10;
/// Files whose breakpoints are remembered, most recently used first.
const MAX_BREAKPOINT_FILES: usize = 100;
/// Files whose named views are remembered, most recently used first.
const MAX_VIEW_FILES: usize = 100;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub general: General,
    pub viewer: Viewer,
    pub editor: Editor,
    pub objects: Objects,
    pub execution: Execution,
    pub recent_files: Vec<PathBuf>,
    /// File reopened on startup.
    pub last_file: Option<PathBuf>,
    /// Breakpoints of recently edited files, most recent first.
    pub breakpoints: Vec<FileBreakpoints>,
    /// Named views of recently edited files, most recent first.
    pub views: Vec<FileViews>,
}

/// The named views of one file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FileViews {
    pub path: PathBuf,
    pub views: Vec<NamedView>,
}

/// A camera saved under a name: the point it looks at, its orientation,
/// how much it shows and its projection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NamedView {
    pub name: String,
    pub target: [f32; 3],
    /// Unit quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    /// Half the height of the view, in model units.
    pub extent: f32,
    pub projection: Projection,
}

/// The breakpoints of one file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FileBreakpoints {
    pub path: PathBuf,
    pub lines: Vec<SavedBreakpoint>,
}

/// A breakpoint as remembered between sessions: its 1-based line and the
/// trimmed text of that line, to find it again if the file changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SavedBreakpoint {
    pub line: usize,
    pub text: String,
}

impl Settings {
    pub fn add_recent_file(&mut self, path: &Path) {
        self.recent_files.retain(|p| p != path);
        self.recent_files.insert(0, path.to_path_buf());
        self.recent_files.truncate(MAX_RECENT_FILES);
    }

    pub fn breakpoints_of(&self, path: &Path) -> &[SavedBreakpoint] {
        self.breakpoints
            .iter()
            .find(|file| file.path == path)
            .map_or(&[], |file| &file.lines)
    }

    /// Remembers the breakpoints of `path`; none forgets the file.
    pub fn remember_breakpoints(&mut self, path: &Path, lines: Vec<SavedBreakpoint>) {
        if self.breakpoints_of(path) == lines.as_slice()
            && self
                .breakpoints
                .first()
                .is_some_and(|file| file.path == path)
        {
            return;
        }
        self.breakpoints.retain(|file| file.path != path);
        if !lines.is_empty() {
            self.breakpoints.insert(
                0,
                FileBreakpoints {
                    path: path.to_path_buf(),
                    lines,
                },
            );
            self.breakpoints.truncate(MAX_BREAKPOINT_FILES);
        }
    }

    pub fn views_of(&self, path: &Path) -> &[NamedView] {
        self.views
            .iter()
            .find(|file| file.path == path)
            .map_or(&[], |file| &file.views)
    }

    /// Remembers the named views of `path`; none forgets the file.
    pub fn remember_views(&mut self, path: &Path, views: Vec<NamedView>) {
        if self.views_of(path) == views.as_slice()
            && self.views.first().is_some_and(|file| file.path == path)
        {
            return;
        }
        self.views.retain(|file| file.path != path);
        if !views.is_empty() {
            self.views.insert(
                0,
                FileViews {
                    path: path.to_path_buf(),
                    views,
                },
            );
            self.views.truncate(MAX_VIEW_FILES);
        }
    }

    pub fn run_settings(&self) -> RunSettings {
        RunSettings {
            reload_cq: self.execution.reload_cq,
            add_script_dir_to_path: self.execution.add_script_dir_to_path,
            change_working_dir: self.execution.change_working_dir,
            reload_imported_modules: self.execution.reload_imported_modules,
        }
    }

    pub fn scene_settings(&self) -> SceneSettings {
        SceneSettings {
            linear_tolerance: self.viewer.linear_tolerance,
            angular_tolerance: self.viewer.angular_tolerance,
            merge_assemblies: self.objects.merge_assemblies,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    /// Follow the operating system.
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "System",
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub theme: Theme,
    /// Python interpreter with CadQuery; empty means auto-detect.
    pub python: String,
    /// Scale factor on top of the system's.
    pub ui_scale: f32,
    /// Look for a newer CQ-editor once a day, when Python starts.
    pub check_updates: bool,
    /// When it last looked, in seconds since the Unix epoch.
    pub last_update_check: u64,
    /// The newest release it found then (see `updates::available`).
    pub newer_release: Option<Release>,
}

impl Default for General {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            python: String::new(),
            ui_scale: 1.0,
            check_updates: true,
            last_update_check: 0,
            newer_release: None,
        }
    }
}

/// The original editor's projection types, under the same names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Projection {
    #[default]
    Orthographic,
    Perspective,
    /// Perspective seen by two eyes, composed as [`Viewer::stereo_mode`]
    /// says.
    Stereo,
    /// Perspective from the left eye of the stereo pair alone.
    MonoLeftEye,
    MonoRightEye,
}

impl Projection {
    pub const ALL: [Projection; 5] = [
        Projection::Orthographic,
        Projection::Perspective,
        Projection::Stereo,
        Projection::MonoLeftEye,
        Projection::MonoRightEye,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Projection::Orthographic => "Orthographic",
            Projection::Perspective => "Perspective",
            Projection::Stereo => "Stereo",
            Projection::MonoLeftEye => "Mono Left Eye",
            Projection::MonoRightEye => "Mono Right Eye",
        }
    }

    /// All but orthographic projection have a vanishing point.
    pub fn is_perspective(self) -> bool {
        self != Projection::Orthographic
    }
}

/// How the two images of the stereo projection are combined, under the
/// original editor's names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StereoMode {
    /// Hardware stereo (one buffer per eye), which this renderer cannot
    /// present: shown as [`StereoMode::Anaglyph`] instead.
    QuadBuffer,
    /// Red-cyan glasses.
    #[default]
    Anaglyph,
    /// Even rows for the left eye, odd ones for the right (passive 3D
    /// displays).
    RowInterlaced,
    ColumnInterlaced,
    ChessBoard,
    /// The left eye's image on the left half, the right eye's on the right.
    SideBySide,
    /// The left eye's image on the top half.
    OverUnder,
}

impl StereoMode {
    pub const ALL: [StereoMode; 7] = [
        StereoMode::QuadBuffer,
        StereoMode::Anaglyph,
        StereoMode::RowInterlaced,
        StereoMode::ColumnInterlaced,
        StereoMode::ChessBoard,
        StereoMode::SideBySide,
        StereoMode::OverUnder,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StereoMode::QuadBuffer => "Quad Buffer",
            StereoMode::Anaglyph => "Anaglyph (red-cyan)",
            StereoMode::RowInterlaced => "Row Interlaced",
            StereoMode::ColumnInterlaced => "Column Interlaced",
            StereoMode::ChessBoard => "Chess Board",
            StereoMode::SideBySide => "Side by Side",
            StereoMode::OverUnder => "Over Under",
        }
    }

    /// The mode actually shown: quad-buffered stereo needs a stereo
    /// swap chain, which wgpu does not offer on any platform.
    pub fn shown(self) -> StereoMode {
        match self {
            StereoMode::QuadBuffer => StereoMode::Anaglyph,
            mode => mode,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OrbitMethod {
    /// Rotation about the world Z axis and the screen's horizontal axis;
    /// Z stays up.
    #[default]
    Turntable,
    /// Free rotation, like rolling a ball under the mouse.
    Trackball,
}

impl OrbitMethod {
    pub const ALL: [OrbitMethod; 2] = [OrbitMethod::Turntable, OrbitMethod::Trackball];

    pub fn label(self) -> &'static str {
        match self {
            OrbitMethod::Turntable => "Turntable",
            OrbitMethod::Trackball => "Trackball",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Viewer {
    pub fit_automatically: bool,
    pub show_navigation_cube: bool,
    pub show_axes_indicator: bool,
    pub use_gradient: bool,
    /// sRGB.
    pub background: [u8; 3],
    /// Bottom color of the gradient, sRGB.
    pub background_aux: [u8; 3],
    pub default_color: [u8; 3],
    pub edge_color: [u8; 3],
    pub show_edges: bool,
    /// Hidden edges as faint dashes in the hidden-line display mode.
    pub hidden_edges: bool,
    pub edge_width: f32,
    /// Chordal deviation of the mesh relative to the object size.
    pub linear_tolerance: f64,
    /// Radians.
    pub angular_tolerance: f64,
    pub projection: Projection,
    pub orbit_method: OrbitMethod,
    /// Animate view changes (standard views, navigation cube, fit).
    pub animate: bool,
    /// When parts drawn small are simplified.
    pub large_models: LargeModels,
    /// The grid on the XY plane.
    pub ground_grid: bool,
    pub scale_bar: bool,
    /// What a model unit is called in the scale bar.
    pub units: Units,
    /// How the stereo projection shows its two images.
    pub stereo_mode: StereoMode,
}

/// What a model unit is: CadQuery has no units, but STEP and STL exports
/// are read as millimeters by most tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Units {
    #[default]
    Millimeters,
    Centimeters,
    Meters,
    Inches,
    /// Plain numbers.
    None,
}

impl Units {
    pub const ALL: [Units; 5] = [
        Units::Millimeters,
        Units::Centimeters,
        Units::Meters,
        Units::Inches,
        Units::None,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Units::Millimeters => "Millimeters",
            Units::Centimeters => "Centimeters",
            Units::Meters => "Meters",
            Units::Inches => "Inches",
            Units::None => "None",
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Units::Millimeters => "mm",
            Units::Centimeters => "cm",
            Units::Meters => "m",
            Units::Inches => "in",
            Units::None => "",
        }
    }
}

/// When the view simplifies the parts it draws small (large-model mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LargeModels {
    /// From a couple of million triangles shown.
    #[default]
    Automatic,
    Always,
    Off,
}

impl LargeModels {
    pub const ALL: [LargeModels; 3] = [
        LargeModels::Automatic,
        LargeModels::Always,
        LargeModels::Off,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LargeModels::Automatic => "Automatic",
            LargeModels::Always => "Always",
            LargeModels::Off => "Off",
        }
    }
}

impl Default for Viewer {
    fn default() -> Self {
        Self {
            fit_automatically: true,
            show_navigation_cube: true,
            show_axes_indicator: true,
            use_gradient: false,
            background: [95, 95, 95],
            background_aux: [30, 30, 30],
            default_color: [255, 215, 0],
            edge_color: [0, 0, 0],
            show_edges: true,
            hidden_edges: true,
            edge_width: 1.5,
            linear_tolerance: 1e-3,
            angular_tolerance: 0.2,
            projection: Projection::Orthographic,
            orbit_method: OrbitMethod::Turntable,
            animate: true,
            large_models: LargeModels::Automatic,
            ground_grid: true,
            scale_bar: true,
            units: Units::Millimeters,
            stereo_mode: StereoMode::Anaglyph,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Editor {
    pub font_size: f32,
    /// Re-run the script whenever its file changes on disk.
    pub autoreload: bool,
    /// Wait this long after a change, so the file is completely written.
    pub autoreload_delay_ms: u64,
    pub watch_imported_modules: bool,
    pub line_wrap: bool,
    /// Column of the vertical ruler; 0 hides it.
    pub max_line_length: usize,
    pub highlight_current_line: bool,
    pub auto_indent: bool,
    /// Offer completions after this many characters of a name; 0 offers
    /// them only after `.` and on Ctrl+Space.
    pub completion_min_chars: usize,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            font_size: 13.0,
            autoreload: false,
            autoreload_delay_ms: 50,
            watch_imported_modules: false,
            line_wrap: false,
            max_line_length: 79,
            highlight_current_line: true,
            auto_indent: true,
            completion_min_chars: 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Objects {
    /// Keep visibility and colors of objects across runs (matched by name).
    pub preserve_properties: bool,
    pub clear_before_run: bool,
    /// Show assemblies as one object instead of one per part.
    pub merge_assemblies: bool,
    /// STL export chordal tolerance, model units.
    pub stl_tolerance: f64,
    /// STL export angular tolerance, radians.
    pub stl_angular_tolerance: f64,
}

impl Default for Objects {
    fn default() -> Self {
        Self {
            preserve_properties: false,
            clear_before_run: true,
            merge_assemblies: false,
            stl_tolerance: 0.1,
            stl_angular_tolerance: 0.1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Execution {
    pub reload_cq: bool,
    pub add_script_dir_to_path: bool,
    pub change_working_dir: bool,
    pub reload_imported_modules: bool,
}

impl Default for Execution {
    fn default() -> Self {
        Self {
            reload_cq: false,
            add_script_dir_to_path: true,
            change_working_dir: true,
            reload_imported_modules: true,
        }
    }
}

#[cfg(test)]
#[path = "../tests/rust/app/test_settings.rs"]
mod tests;
