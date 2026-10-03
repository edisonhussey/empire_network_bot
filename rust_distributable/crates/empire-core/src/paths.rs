//! Where OpenAuto keeps its data.
//!
//! There is exactly one answer to this question, and every process asks this
//! module for it: the desktop app, the embedded service, `empire-hunt` and the
//! stand-alone daemon.
//!
//! This matters more than it looks. SQLite keeps its write-ahead log next to
//! whichever *path* it was opened with, so two processes that reach the same
//! file by different routes (a symlink, or one using `data/` and the other an
//! application directory) end up with two `-wal` files over one database. That
//! is corruption, not a merge conflict. Sharing one function removes the
//! possibility.
//!
//! `EMPIRE_DATA_DIR` overrides everything, which is what the development
//! scripts use to point a build at a repository-local directory.

use std::path::{Path, PathBuf};

/// Bundle identifier, and therefore the on-disk directory name on every
/// platform. Must stay in step with `identifier` in `tauri.conf.json`.
pub const APP_IDENTIFIER: &str = "com.openauto.desktop";

/// The one database file, inside the data directory.
pub const DATABASE_FILE: &str = "empire.sqlite3";

/// Directory holding the database and any account state.
///
/// Resolution order:
/// 1. `EMPIRE_DATA_DIR`, when set and non-empty.
/// 2. The platform application data directory for [`APP_IDENTIFIER`].
/// 3. `./data`, if the platform gives us no home directory at all.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("EMPIRE_DATA_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    if let Some(dir) = platform_data_dir() {
        return dir;
    }
    PathBuf::from("data")
}

/// The database file inside a data directory.
pub fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join(DATABASE_FILE)
}

/// A `sqlx` URL for the database inside a data directory.
pub fn database_url(data_dir: &Path) -> String {
    format!(
        "sqlite://{}?mode=rwc",
        database_path(data_dir).display()
    )
}

/// Convenience wrapper for the resolved data directory.
pub fn default_database_url() -> String {
    database_url(&data_dir())
}

#[cfg(target_os = "macos")]
fn platform_data_dir() -> Option<PathBuf> {
    // `~/Library/Application Support/<identifier>`, matching what Tauri's
    // `app_data_dir()` produced before this module existed.
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join(APP_IDENTIFIER),
    )
}

#[cfg(target_os = "windows")]
fn platform_data_dir() -> Option<PathBuf> {
    let roaming = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(roaming).join(APP_IDENTIFIER))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_data_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join(APP_IDENTIFIER));
    }
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(APP_IDENTIFIER),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_override_wins() {
        // Set and read in one test so the process-wide variable cannot leak
        // into a parallel case.
        let previous = std::env::var_os("EMPIRE_DATA_DIR");
        unsafe { std::env::set_var("EMPIRE_DATA_DIR", "/tmp/openauto-override") };
        assert_eq!(data_dir(), PathBuf::from("/tmp/openauto-override"));
        match previous {
            Some(value) => unsafe { std::env::set_var("EMPIRE_DATA_DIR", value) },
            None => unsafe { std::env::remove_var("EMPIRE_DATA_DIR") },
        }
    }

    #[test]
    fn the_database_lives_inside_the_data_directory() {
        let dir = PathBuf::from("/tmp/openauto-check");
        assert_eq!(
            database_path(&dir),
            PathBuf::from("/tmp/openauto-check/empire.sqlite3")
        );
        assert_eq!(
            database_url(&dir),
            "sqlite:///tmp/openauto-check/empire.sqlite3?mode=rwc"
        );
    }
}
