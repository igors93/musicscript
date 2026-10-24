//! The script being edited.
//!
//! Text is kept with `\n` line endings internally; the file's original
//! convention is remembered and restored on save. Files are written as
//! UTF-8. A file in another encoding is only opened when the user asks for
//! Windows-1252 explicitly, and saving it converts it to UTF-8.

use std::fs::File;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Script shown when no file is open.
pub const DEFAULT_SCRIPT: &str = "import musicscript as ms\n\n# Adjust these parameters in real time from the Parameters panel!\nbpm = 132\nswing = 0.08\ncutoff = 2400\n\nbeat = ms.Beat(bpm=bpm, bars=2, swing=swing)\n\n# Drums\nbeat.kick(\"x... ..x. x... ..x. | x... ..x. x..x ..x.\", punch=0.85, drive=0.2)\nbeat.snare(\".... x... .... x... | .... x... .... x.x.\", snap=0.65)\nbeat.hihat(\"x.x. x.x. xxxx x.x. | x.x. xxxx x.xx xxxx\", pan=-0.15, volume=0.72)\n\n# 808 Sub Bass\nbeat.bass808(\n    notes=\"C2 . . C2 . . Eb2 . | F2 . . F2 . . G2->C2 .\",\n    glide=0.06,\n    drive=0.30,\n    sub=0.75,\n)\n\n# Synth Lead & Pluck Arpeggio\nbeat.pluck(\n    notes=\"C4 Eb4 G4 Bb4 C5 G4 Eb4 G4 | F4 Ab4 C5 Eb5 D5 Bb4 G4 D4\",\n    cutoff=cutoff,\n).delay(time=0.25, feedback=0.35, mix=0.28).reverb(room=0.55, mix=0.25)\n\nshow_object(beat)\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineEnding {
    Lf,
    CrLf,
    Cr,
}

/// How the bytes of a file on disk map to text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Encoding {
    #[default]
    Utf8,
    /// Western European legacy encoding (a superset of printable Latin-1),
    /// common for scripts written with older Windows editors.
    Windows1252,
}

impl Encoding {
    pub fn label(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Windows1252 => "Windows-1252",
        }
    }
}

/// The error of reading a file that is not valid UTF-8; see [`is_not_utf8`].
#[derive(Debug)]
struct NotUtf8 {
    path: PathBuf,
    offset: usize,
}

impl std::fmt::Display for NotUtf8 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is not valid UTF-8 (invalid data near byte {}); the file was not opened or modified",
            self.path.display(),
            self.offset
        )
    }
}

impl std::error::Error for NotUtf8 {}

/// Whether opening a file failed only because it is not UTF-8, so it can be
/// offered again as [`Encoding::Windows1252`].
pub fn is_not_utf8(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|inner| inner.is::<NotUtf8>())
}

/// A normalized snapshot read from the document's current path without
/// modifying the editor buffer.
#[derive(Clone, Debug)]
pub struct DiskVersion {
    pub text: String,
    path: PathBuf,
    line_ending: LineEnding,
    utf8_bom: bool,
    encoding: Encoding,
}

/// Relationship between an on-disk snapshot and the editor's local state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalUpdate {
    /// Only metadata changed; disk contents still match the saved baseline.
    Unchanged,
    /// Disk and buffer now agree, so accepting the snapshot loses nothing.
    MatchesBuffer,
    /// The buffer is clean and can be reloaded automatically.
    Reload,
    /// Disk and buffer diverged from the same saved baseline.
    Conflict,
}

impl LineEnding {
    #[cfg(windows)]
    pub const NATIVE: LineEnding = LineEnding::CrLf;
    #[cfg(not(windows))]
    pub const NATIVE: LineEnding = LineEnding::Lf;

    /// The convention used by `text`, or the platform's for text without
    /// line breaks.
    pub fn detect(text: &str) -> Self {
        if text.contains("\r\n") {
            LineEnding::CrLf
        } else if text.contains('\r') {
            LineEnding::Cr
        } else if text.contains('\n') {
            LineEnding::Lf
        } else {
            LineEnding::NATIVE
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
            LineEnding::Cr => "\r",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::CrLf => "CRLF",
            LineEnding::Cr => "CR",
        }
    }
}

#[derive(Debug)]
pub struct Document {
    pub text: String,
    path: Option<PathBuf>,
    line_ending: LineEnding,
    utf8_bom: bool,
    /// Encoding of the file when it was opened; saving writes UTF-8.
    encoding: Encoding,
    saved_hash: u64,
    /// Title of the bundled example this buffer was loaded from. Examples
    /// are read-only: saving one asks for a new location.
    example: Option<String>,
}

impl Document {
    pub fn untitled(text: &str) -> Self {
        let text = normalize(text);
        Self {
            saved_hash: hash(&text),
            text,
            path: None,
            line_ending: LineEnding::NATIVE,
            utf8_bom: false,
            encoding: Encoding::Utf8,
            example: None,
        }
    }

    pub fn example(title: &str, source: &str) -> Self {
        Self {
            example: Some(title.to_owned()),
            ..Self::untitled(source)
        }
    }

    pub fn open(path: &Path) -> io::Result<Self> {
        Self::open_with(path, Encoding::Utf8)
    }

    /// Opens `path` decoded as `encoding`. Saving writes UTF-8 whatever the
    /// encoding was.
    pub fn open_with(path: &Path, encoding: Encoding) -> io::Result<Self> {
        let (text, line_ending, utf8_bom) = read(path, encoding)?;
        Ok(Self {
            saved_hash: hash(&text),
            text,
            path: Some(path.to_path_buf()),
            line_ending,
            utf8_bom,
            encoding,
            example: None,
        })
    }

    /// Opens `path` as UTF-8 or, when it is not, as Windows-1252. For
    /// archiving existing contents, where asking the user is not possible.
    pub fn open_any(path: &Path) -> io::Result<Self> {
        match Self::open(path) {
            Err(error) if is_not_utf8(&error) => Self::open_with(path, Encoding::Windows1252),
            result => result,
        }
    }

    /// Reconstructs a recovered buffer while using the current disk contents,
    /// when readable, as its saved baseline.
    pub fn recovered(
        text: String,
        path: Option<PathBuf>,
        line_ending: LineEnding,
        utf8_bom: bool,
    ) -> Self {
        let text = normalize(&text);
        let disk = path.as_deref().and_then(|path| Self::open_any(path).ok());
        let saved_hash = disk
            .as_ref()
            .map_or_else(|| hash(&text) ^ u64::MAX, |disk| disk.saved_hash);
        Self {
            saved_hash,
            text,
            path,
            line_ending,
            utf8_bom,
            // Keeps reading the file the way it is encoded on disk, so the
            // file watcher can compare it.
            encoding: disk.map_or(Encoding::Utf8, |disk| disk.encoding),
            example: None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn has_utf8_bom(&self) -> bool {
        self.utf8_bom
    }

    /// The encoding the file was opened with; the next save writes UTF-8.
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    pub fn example_title(&self) -> Option<&str> {
        self.example.as_deref()
    }

    /// True when the buffer differs from what was last loaded or saved.
    pub fn is_modified(&self) -> bool {
        hash(&self.text) != self.saved_hash
    }

    /// Name for the window title and tabs.
    pub fn display_name(&self) -> String {
        if let Some(title) = &self.example {
            return format!("{title} (example)");
        }
        self.path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || "untitled".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// Saves to the current path; `false` when a "save as" is needed first
    /// (untitled buffers and examples).
    pub fn save(&mut self) -> io::Result<bool> {
        match (&self.path, &self.example) {
            (Some(path), None) => {
                let path = path.clone();
                self.write(&path)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn save_as(&mut self, path: &Path) -> io::Result<()> {
        self.write(path)?;
        self.path = Some(path.to_path_buf());
        self.example = None;
        Ok(())
    }

    /// Applies an inspected disk version when the editor buffer is clean.
    /// Unsaved changes are never discarded by this convenience method;
    /// callers need an explicit conflict flow to replace a modified buffer.
    pub fn reload(&mut self, version: DiskVersion) -> io::Result<bool> {
        if self.is_modified() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "refusing to reload a document with unsaved changes",
            ));
        }
        self.apply_disk_version(version)
    }

    /// Reads the current file without changing the buffer or saved state.
    pub fn disk_version(&self) -> io::Result<Option<DiskVersion>> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        let (text, line_ending, utf8_bom) = read(path, self.encoding)?;
        Ok(Some(DiskVersion {
            text,
            path: path.clone(),
            line_ending,
            utf8_bom,
            encoding: self.encoding,
        }))
    }

    /// Whether a disk snapshot still has the contents last loaded or saved.
    pub fn disk_matches_saved(&self, version: &DiskVersion) -> bool {
        self.path.as_ref() == Some(&version.path) && hash(&version.text) == self.saved_hash
    }

    pub fn classify_external_update(&self, version: &DiskVersion) -> ExternalUpdate {
        if self.disk_matches_saved(version) {
            ExternalUpdate::Unchanged
        } else if version.text == self.text {
            ExternalUpdate::MatchesBuffer
        } else if self.is_modified() {
            ExternalUpdate::Conflict
        } else {
            ExternalUpdate::Reload
        }
    }

    /// Explicitly replaces the buffer with a previously inspected snapshot.
    pub fn apply_disk_version(&mut self, version: DiskVersion) -> io::Result<bool> {
        if self.path.as_ref() != Some(&version.path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "disk snapshot belongs to another document",
            ));
        }
        let changed = version.text != self.text;
        self.saved_hash = hash(&version.text);
        self.text = version.text;
        self.line_ending = version.line_ending;
        self.utf8_bom = version.utf8_bom;
        self.encoding = version.encoding;
        Ok(changed)
    }

    /// Writes the current buffer elsewhere without changing this document's
    /// path or marking its edits as saved.
    pub fn save_copy(&self, path: &Path) -> io::Result<()> {
        if self.path.as_deref() == Some(path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the copy must use a different path",
            ));
        }
        atomic_write(path, &self.encoded_bytes())
    }

    /// Creates a uniquely named recovery copy of the current editor buffer.
    /// The document remains modified and keeps its original path.
    pub fn create_recovery_backup(&self, directory: &Path) -> io::Result<PathBuf> {
        std::fs::create_dir_all(directory)?;
        let stem = self
            .path
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("script");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        for suffix in 0..1000 {
            let path = directory.join(format!("{stem}-{timestamp}-{suffix}.recovery.py"));
            match atomic_create(&path, &self.encoded_bytes()) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique recovery file name",
        ))
    }

    fn write(&mut self, path: &Path) -> io::Result<()> {
        atomic_write(path, &self.encoded_bytes())?;
        self.saved_hash = hash(&self.text);
        self.encoding = Encoding::Utf8;
        Ok(())
    }

    fn encoded_bytes(&self) -> Vec<u8> {
        let text = if self.line_ending == LineEnding::Lf {
            self.text.clone()
        } else {
            self.text.replace('\n', self.line_ending.as_str())
        };
        let mut encoded = Vec::with_capacity(text.len() + usize::from(self.utf8_bom) * 3);
        if self.utf8_bom {
            encoded.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        }
        encoded.extend_from_slice(text.as_bytes());
        encoded
    }
}

fn read(path: &Path, encoding: Encoding) -> io::Result<(String, LineEnding, bool)> {
    let mut bytes = std::fs::read(path)?;
    let utf8_bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    if utf8_bom {
        bytes.drain(..3);
    }
    let raw = match encoding {
        Encoding::Utf8 => String::from_utf8(bytes).map_err(|error| {
            let offset = error.utf8_error().valid_up_to() + usize::from(utf8_bom) * 3;
            io::Error::new(
                io::ErrorKind::InvalidData,
                NotUtf8 {
                    path: path.to_path_buf(),
                    offset,
                },
            )
        })?,
        Encoding::Windows1252 => decode_windows_1252(&bytes),
    };
    let line_ending = LineEnding::detect(&raw);
    Ok((normalize(&raw), line_ending, utf8_bom))
}

/// Decodes Windows-1252 as the WHATWG Encoding Standard does: every byte is
/// a character, so decoding never fails. Bytes 0x80..=0x9F map to
/// typographic characters; the five unassigned ones to C1 controls.
pub fn decode_windows_1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}',
        '\u{017D}', '\u{008F}', '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
    ];
    bytes
        .iter()
        .map(|&byte| match byte {
            0x80..=0x9f => HIGH[usize::from(byte - 0x80)],
            _ => char::from(byte),
        })
        .collect()
}

/// Writes and synchronizes a temporary file in the destination directory,
/// then atomically replaces the destination. Existing permissions are kept.
pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    atomic_write_with(path, true, |file| file.write_all(contents))
}

/// Atomic creation variant used for uniquely named recovery versions.
pub(crate) fn atomic_create(path: &Path, contents: &[u8]) -> io::Result<()> {
    atomic_write_with(path, false, |file| file.write_all(contents))
}

fn atomic_write_with(
    path: &Path,
    overwrite: bool,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let target = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => std::fs::canonicalize(path)?,
        _ => path.to_path_buf(),
    };
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    #[cfg(not(windows))]
    let permissions = std::fs::metadata(&target)
        .ok()
        .map(|metadata| metadata.permissions());
    let mut temporary = tempfile::Builder::new()
        .prefix(".cq-editor-")
        .tempfile_in(parent)?;
    write(temporary.as_file_mut())?;
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;

    #[cfg(not(windows))]
    if let Some(permissions) = permissions.clone() {
        temporary.as_file().set_permissions(permissions)?;
    }

    let persisted = persist_temporary(temporary, &target, overwrite)?;
    persisted.sync_all()?;
    sync_parent(parent)
}

#[cfg(not(windows))]
fn persist_temporary(
    temporary: tempfile::NamedTempFile,
    target: &Path,
    overwrite: bool,
) -> io::Result<File> {
    if overwrite {
        temporary.persist(target)
    } else {
        temporary.persist_noclobber(target)
    }
    .map_err(|error| error.error)
}

#[cfg(windows)]
fn persist_temporary(
    temporary: tempfile::NamedTempFile,
    target: &Path,
    overwrite: bool,
) -> io::Result<File> {
    use std::os::windows::ffi::OsStrExt as _;

    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, REPLACEFILE_WRITE_THROUGH,
        ReplaceFileW,
    };

    // Windows replacement calls require the temporary file to be closed.
    // TempPath remains alive so a failed operation still removes the temp.
    let target_exists = target.exists();
    let temporary = temporary.into_temp_path();
    let destination: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let replacement: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();

    if overwrite && target_exists {
        // ReplaceFileW performs a single filesystem replacement while keeping
        // the destination's ACLs, creation time, compression and encryption.
        let replaced_ok = unsafe {
            ReplaceFileW(
                destination.as_ptr(),
                replacement.as_ptr(),
                std::ptr::null(),
                REPLACEFILE_WRITE_THROUGH,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if replaced_ok == 0 {
            return Err(io::Error::last_os_error());
        }
    } else {
        let flags = MOVEFILE_WRITE_THROUGH
            | if overwrite {
                MOVEFILE_REPLACE_EXISTING
            } else {
                0
            };
        let moved_ok = unsafe { MoveFileExW(replacement.as_ptr(), destination.as_ptr(), flags) };
        if moved_ok == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(target)
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> io::Result<()> {
    Ok(())
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
#[path = "../tests/rust/editor/test_document.rs"]
mod tests;
