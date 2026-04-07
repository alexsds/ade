use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Theme mode preference: Dark, Light, or System (follows OS appearance).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ThemeMode {
    Dark,
    Light,
    System,
}

impl ThemeMode {
    /// Returns all theme mode variants in display order.
    pub fn all() -> &'static [ThemeMode] {
        &[ThemeMode::Dark, ThemeMode::Light, ThemeMode::System]
    }

    /// Human-readable display name for the theme mode.
    pub fn display_name(&self) -> &'static str {
        match self {
            ThemeMode::Dark => "Dark",
            ThemeMode::Light => "Light",
            ThemeMode::System => "System",
        }
    }

    /// Resolve the user's theme mode preference to a concrete ThemeName.
    /// For System mode, defaults to Dark when no window context is available.
    /// Prefer `resolve_with_appearance` when a window is accessible.
    pub fn resolve(&self) -> crate::theme::ThemeName {
        match self {
            ThemeMode::Dark => crate::theme::ThemeName::Dark,
            ThemeMode::Light => crate::theme::ThemeName::Light,
            ThemeMode::System => crate::theme::ThemeName::Dark,
        }
    }

    /// Resolve the user's theme mode preference to a concrete ThemeName,
    /// using the window's appearance to determine System mode mapping.
    pub fn resolve_with_appearance(
        &self,
        appearance: gpui::WindowAppearance,
    ) -> crate::theme::ThemeName {
        match self {
            ThemeMode::Dark => crate::theme::ThemeName::Dark,
            ThemeMode::Light => crate::theme::ThemeName::Light,
            ThemeMode::System => match appearance {
                gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark => {
                    crate::theme::ThemeName::Dark
                }
                _ => crate::theme::ThemeName::Light,
            },
        }
    }
}

fn default_theme_mode() -> ThemeMode {
    ThemeMode::System
}

/// Supported external editors for opening files from code review.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EditorChoice {
    MacOsOpen,
    VsCode,
    SublimeText,
    Zed,
    Cursor,
    IntelliJ,
    Nova,
    Xcode,
}

impl EditorChoice {
    /// Returns all editor variants in display order (MacOsOpen first).
    pub fn all() -> &'static [EditorChoice] {
        &[
            EditorChoice::MacOsOpen,
            EditorChoice::VsCode,
            EditorChoice::SublimeText,
            EditorChoice::Zed,
            EditorChoice::Cursor,
            EditorChoice::IntelliJ,
            EditorChoice::Nova,
            EditorChoice::Xcode,
        ]
    }

    /// Human-readable display name for the editor.
    pub fn display_name(&self) -> &'static str {
        match self {
            EditorChoice::MacOsOpen => "macOS open (Default)",
            EditorChoice::VsCode => "VS Code",
            EditorChoice::SublimeText => "Sublime Text",
            EditorChoice::Zed => "Zed",
            EditorChoice::Cursor => "Cursor",
            EditorChoice::IntelliJ => "IntelliJ IDEA",
            EditorChoice::Nova => "Nova",
            EditorChoice::Xcode => "Xcode",
        }
    }

    /// CLI command used for `which` detection (not necessarily for launch).
    pub fn cli_command(&self) -> &'static str {
        match self {
            EditorChoice::MacOsOpen => "open",
            EditorChoice::VsCode => "code",
            EditorChoice::SublimeText => "subl",
            EditorChoice::Zed => "zed",
            EditorChoice::Cursor => "cursor",
            EditorChoice::IntelliJ => "idea",
            EditorChoice::Nova => "nova",
            EditorChoice::Xcode => "xcode",
        }
    }

    /// macOS application bundle name for `/Applications/<name>.app` detection.
    fn app_name(&self) -> Option<&'static str> {
        match self {
            EditorChoice::MacOsOpen => None,
            EditorChoice::VsCode => Some("Visual Studio Code"),
            EditorChoice::SublimeText => Some("Sublime Text"),
            EditorChoice::Zed => Some("Zed"),
            EditorChoice::Cursor => Some("Cursor"),
            EditorChoice::IntelliJ => Some("IntelliJ IDEA"),
            EditorChoice::Nova => Some("Nova"),
            EditorChoice::Xcode => Some("Xcode"),
        }
    }
}

fn default_editor() -> EditorChoice {
    EditorChoice::MacOsOpen
}

/// Application settings persisted to `~/.config/ade/settings.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_editor")]
    pub external_editor: EditorChoice,
    #[serde(default = "default_theme_mode")]
    pub theme_mode: ThemeMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            external_editor: default_editor(),
            theme_mode: default_theme_mode(),
        }
    }
}

impl Settings {
    /// Load settings from the default path, falling back to defaults on any error.
    pub fn load() -> Self {
        Self::load_from(&settings_path())
    }

    /// Save settings to the default path with atomic write.
    pub fn save(&self) -> Result<(), std::io::Error> {
        self.save_to(&settings_path())
    }

    /// Load settings from a specific path. Returns defaults on any error.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save settings to a specific path with atomic write (write temp + rename).
    pub fn save_to(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Atomic write: write to temp file in the same directory, then rename.
        let temp_path = path.with_extension("json.tmp");
        std::fs::write(&temp_path, &json)?;
        std::fs::rename(&temp_path, path)?;
        Ok(())
    }
}

/// Returns the default settings file path: `$HOME/.config/ade/settings.json`.
pub fn settings_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("ade")
        .join("settings.json")
}

/// Check if an editor is installed on the system.
///
/// MacOsOpen always returns true. For others, checks `which <cli_command>` or
/// `/Applications/<AppName>.app` existence.
pub fn is_editor_installed(editor: &EditorChoice) -> bool {
    if *editor == EditorChoice::MacOsOpen {
        return true;
    }

    // Check if CLI command exists in PATH
    if Command::new("which")
        .arg(editor.cli_command())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return true;
    }

    // Check if macOS application bundle exists
    if let Some(app_name) = editor.app_name() {
        let app_path = format!("/Applications/{}.app", app_name);
        if Path::new(&app_path).exists() {
            return true;
        }
    }

    false
}

/// Open a file in the specified editor. Non-blocking, silently logs errors.
///
/// Per research: IntelliJ, Nova, and Xcode use `open -a "<AppName>"` since they
/// don't reliably have CLI tools in PATH. VS Code, Cursor, Zed, Sublime Text use
/// their direct CLI commands. MacOsOpen uses `open <file>`.
pub fn open_in_editor(editor: &EditorChoice, file_path: &Path) {
    let result = match editor {
        EditorChoice::MacOsOpen => Command::new("open").arg(file_path).spawn(),
        EditorChoice::VsCode => Command::new("open")
            .arg("-a")
            .arg("Visual Studio Code")
            .arg(file_path)
            .spawn(),
        EditorChoice::SublimeText => Command::new("open")
            .arg("-a")
            .arg("Sublime Text")
            .arg(file_path)
            .spawn(),
        EditorChoice::Zed => Command::new("open")
            .arg("-a")
            .arg("Zed")
            .arg(file_path)
            .spawn(),
        EditorChoice::Cursor => Command::new("open")
            .arg("-a")
            .arg("Cursor")
            .arg(file_path)
            .spawn(),
        EditorChoice::IntelliJ => Command::new("open")
            .arg("-a")
            .arg("IntelliJ IDEA")
            .arg(file_path)
            .spawn(),
        EditorChoice::Nova => Command::new("open")
            .arg("-a")
            .arg("Nova")
            .arg(file_path)
            .spawn(),
        EditorChoice::Xcode => Command::new("open")
            .arg("-a")
            .arg("Xcode")
            .arg(file_path)
            .spawn(),
    };

    match result {
        Ok(_child) => {
            // Intentionally ignore the child process -- don't collect its status
        }
        Err(e) => {
            tracing::warn!(
                "Failed to open {:?} with {}: {}",
                file_path,
                editor.display_name(),
                e
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default_returns_macos_open() {
        let settings = Settings::default();
        assert_eq!(settings.external_editor, EditorChoice::MacOsOpen);
    }

    #[test]
    fn test_settings_round_trip_preserves_editor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        let mut settings = Settings::default();
        settings.external_editor = EditorChoice::VsCode;
        settings.save_to(&path).unwrap();

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.external_editor, EditorChoice::VsCode);
    }

    #[test]
    fn test_settings_load_returns_default_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent").join("settings.json");

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.external_editor, EditorChoice::MacOsOpen);
    }

    #[test]
    fn test_settings_load_returns_default_on_corrupt_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "this is not valid json {{{").unwrap();

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.external_editor, EditorChoice::MacOsOpen);
    }

    #[test]
    fn test_editor_choice_serializes_correctly() {
        let json = serde_json::to_string(&EditorChoice::VsCode).unwrap();
        assert!(
            json.contains("VsCode"),
            "Expected VsCode in JSON, got: {}",
            json
        );
    }

    #[test]
    fn test_editor_choice_deserializes_correctly() {
        let editor: EditorChoice = serde_json::from_str("\"VsCode\"").unwrap();
        assert_eq!(editor, EditorChoice::VsCode);
    }

    #[test]
    fn test_editor_choice_macos_open_serializes() {
        let json = serde_json::to_string(&EditorChoice::MacOsOpen).unwrap();
        assert!(json.contains("MacOsOpen"));
    }

    #[test]
    fn test_is_editor_installed_macos_open_always_true() {
        assert!(is_editor_installed(&EditorChoice::MacOsOpen));
    }

    #[test]
    fn test_settings_path_ends_correctly() {
        let path = settings_path();
        let path_str = path.to_string_lossy();
        assert!(
            path_str.ends_with(".config/ade/settings.json"),
            "Expected path ending with .config/ade/settings.json, got: {}",
            path_str
        );
    }

    #[test]
    fn test_open_in_editor_does_not_panic_on_nonexistent_file() {
        // This should not panic even with a nonexistent file path
        open_in_editor(
            &EditorChoice::MacOsOpen,
            Path::new("/tmp/ade_test_nonexistent_file_12345.txt"),
        );
    }

    #[test]
    fn test_all_editor_choices_have_display_name() {
        for editor in EditorChoice::all() {
            let name = editor.display_name();
            assert!(!name.is_empty(), "Display name should not be empty");
        }
    }

    #[test]
    fn test_all_editor_choices_represented_in_all() {
        let all = EditorChoice::all();
        assert_eq!(all.len(), 8, "Expected 8 editor variants");

        // Verify specific variants are included
        assert!(all.contains(&EditorChoice::MacOsOpen));
        assert!(all.contains(&EditorChoice::VsCode));
        assert!(all.contains(&EditorChoice::SublimeText));
        assert!(all.contains(&EditorChoice::Zed));
        assert!(all.contains(&EditorChoice::Cursor));
        assert!(all.contains(&EditorChoice::IntelliJ));
        assert!(all.contains(&EditorChoice::Nova));
        assert!(all.contains(&EditorChoice::Xcode));
    }

    #[test]
    fn test_settings_save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("settings.json");

        let settings = Settings::default();
        settings.save_to(&path).unwrap();

        assert!(path.exists(), "Settings file should exist after save");
    }

    #[test]
    fn test_editor_choice_all_cli_commands_non_empty() {
        for editor in EditorChoice::all() {
            let cmd = editor.cli_command();
            assert!(!cmd.is_empty(), "CLI command should not be empty");
        }
    }

    // -- ThemeMode tests --

    #[test]
    fn test_theme_mode_serializes_dark() {
        let json = serde_json::to_string(&ThemeMode::Dark).unwrap();
        assert!(
            json.contains("Dark"),
            "Expected Dark in JSON, got: {}",
            json
        );
    }

    #[test]
    fn test_theme_mode_serializes_light() {
        let json = serde_json::to_string(&ThemeMode::Light).unwrap();
        assert!(
            json.contains("Light"),
            "Expected Light in JSON, got: {}",
            json
        );
    }

    #[test]
    fn test_theme_mode_serializes_system() {
        let json = serde_json::to_string(&ThemeMode::System).unwrap();
        assert!(
            json.contains("System"),
            "Expected System in JSON, got: {}",
            json
        );
    }

    #[test]
    fn test_theme_mode_deserializes() {
        let mode: ThemeMode = serde_json::from_str("\"System\"").unwrap();
        assert_eq!(mode, ThemeMode::System);
    }

    #[test]
    fn test_settings_default_has_system_theme() {
        let settings = Settings::default();
        assert_eq!(settings.theme_mode, ThemeMode::System);
    }

    #[test]
    fn test_settings_round_trip_preserves_theme_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        let mut settings = Settings::default();
        settings.theme_mode = ThemeMode::Light;
        settings.save_to(&path).unwrap();

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.theme_mode, ThemeMode::Light);
    }

    #[test]
    fn test_settings_backward_compat_missing_theme_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        // Write JSON with only external_editor (no theme_mode field)
        std::fs::write(&path, r#"{"external_editor":"VsCode"}"#).unwrap();

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.external_editor, EditorChoice::VsCode);
        assert_eq!(loaded.theme_mode, ThemeMode::System);
    }

    #[test]
    fn test_theme_mode_resolve_dark() {
        assert_eq!(ThemeMode::Dark.resolve(), crate::theme::ThemeName::Dark);
    }

    #[test]
    fn test_theme_mode_resolve_light() {
        assert_eq!(ThemeMode::Light.resolve(), crate::theme::ThemeName::Light);
    }

    #[test]
    fn test_theme_mode_resolve_system() {
        // System currently defaults to Dark (placeholder until Phase 55)
        assert_eq!(ThemeMode::System.resolve(), crate::theme::ThemeName::Dark);
    }

    #[test]
    fn test_theme_mode_all_returns_three_variants() {
        let all = ThemeMode::all();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&ThemeMode::Dark));
        assert!(all.contains(&ThemeMode::Light));
        assert!(all.contains(&ThemeMode::System));
    }

    #[test]
    fn test_theme_mode_display_names() {
        assert_eq!(ThemeMode::Dark.display_name(), "Dark");
        assert_eq!(ThemeMode::Light.display_name(), "Light");
        assert_eq!(ThemeMode::System.display_name(), "System");
    }

    #[test]
    fn test_theme_mode_resolve_with_appearance() {
        use gpui::WindowAppearance;

        // System mode follows OS appearance
        assert_eq!(
            ThemeMode::System.resolve_with_appearance(WindowAppearance::Dark),
            crate::theme::ThemeName::Dark
        );
        assert_eq!(
            ThemeMode::System.resolve_with_appearance(WindowAppearance::VibrantDark),
            crate::theme::ThemeName::Dark
        );
        assert_eq!(
            ThemeMode::System.resolve_with_appearance(WindowAppearance::Light),
            crate::theme::ThemeName::Light
        );
        assert_eq!(
            ThemeMode::System.resolve_with_appearance(WindowAppearance::VibrantLight),
            crate::theme::ThemeName::Light
        );

        // Explicit modes ignore system appearance
        assert_eq!(
            ThemeMode::Dark.resolve_with_appearance(WindowAppearance::Light),
            crate::theme::ThemeName::Dark
        );
        assert_eq!(
            ThemeMode::Light.resolve_with_appearance(WindowAppearance::Dark),
            crate::theme::ThemeName::Light
        );
    }
}
