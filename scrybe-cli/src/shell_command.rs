// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Idempotent lifecycle management for the user-facing `scrybe` shell link.
//!
//! The desktop app and the development installer both call this code through
//! the `scrybe shell-command` subcommand.  It deliberately manages one
//! symlink only: `~/.local/bin/scrybe` (or `SCRYBE_BIN_DIR/scrybe`). It never
//! edits shell startup files or overwrites an unrelated entry.

use serde::Serialize;
#[cfg(unix)]
use std::collections::BTreeSet;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

#[cfg(unix)]
const COMMAND_NAME: &str = "scrybe";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Install,
    Repair,
    Status,
    Uninstall,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Report {
    pub outcome: String,
    pub command_path: String,
    pub target_path: Option<String>,
    pub path_winner: Option<String>,
    pub other_installations: Vec<String>,
    pub app_bundles: Vec<String>,
    pub warnings: Vec<String>,
    pub message: String,
}

#[cfg(unix)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum LinkState {
    Missing,
    Current(PathBuf),
    Broken(PathBuf),
    ManagedOther(PathBuf),
    ConflictingSymlink(PathBuf),
    ConflictingEntry,
}

pub fn run(operation: Operation, requested_bin_dir: Option<PathBuf>) -> anyhow::Result<Report> {
    #[cfg(not(unix))]
    {
        let _ = operation;
        let _ = requested_bin_dir;
        anyhow::bail!("shell-command installation is currently supported on macOS and Linux")
    }

    #[cfg(unix)]
    {
        let source = std::env::current_exe()
            .map_err(|e| anyhow::anyhow!("cannot locate the running scrybe executable: {e}"))?;
        let home = home_dir()?;
        ensure_stable_install_source(operation, &source, &home)?;
        let path = std::env::var_os("PATH");
        let bin_dir = requested_bin_dir.unwrap_or_else(|| default_bin_dir(&home));
        run_with(operation, &bin_dir, &source, &home, path.as_deref())
    }
}

#[cfg(unix)]
fn home_dir() -> anyhow::Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("cannot determine the current user's home directory"))
}

#[cfg(unix)]
fn default_bin_dir(home: &Path) -> PathBuf {
    std::env::var_os("SCRYBE_BIN_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/bin"))
}

#[cfg(unix)]
fn run_with(
    operation: Operation,
    bin_dir: &Path,
    source: &Path,
    home: &Path,
    path: Option<&std::ffi::OsStr>,
) -> anyhow::Result<Report> {
    let source = absolute_path(source)?;
    if !source.is_file() {
        anyhow::bail!(
            "the scrybe executable does not exist at {}",
            source.display()
        );
    }

    let command_path = bin_dir.join(COMMAND_NAME);
    let before = inspect_link(&command_path, &source, home)?;
    let outcome = match operation {
        Operation::Status => state_name(&before).to_string(),
        Operation::Install | Operation::Repair => match before {
            LinkState::Missing => {
                replace_link_atomically(&command_path, &source)?;
                "installed".to_string()
            }
            LinkState::Current(_) => "already_installed".to_string(),
            LinkState::Broken(_) | LinkState::ManagedOther(_) => {
                replace_link_atomically(&command_path, &source)?;
                "repaired".to_string()
            }
            LinkState::ConflictingSymlink(ref target) => anyhow::bail!(
                "refusing to replace unrelated symlink {} -> {}; remove it explicitly or choose another SCRYBE_BIN_DIR",
                command_path.display(),
                target.display()
            ),
            LinkState::ConflictingEntry => anyhow::bail!(
                "refusing to overwrite existing non-symlink {}; move it or choose another SCRYBE_BIN_DIR",
                command_path.display()
            ),
        },
        Operation::Uninstall => match before {
            LinkState::Missing => "not_installed".to_string(),
            LinkState::Current(_) | LinkState::Broken(_) | LinkState::ManagedOther(_) => {
                std::fs::remove_file(&command_path).map_err(|e| {
                    anyhow::anyhow!("cannot remove {}: {e}", command_path.display())
                })?;
                "removed".to_string()
            }
            LinkState::ConflictingSymlink(ref target) => anyhow::bail!(
                "refusing to remove unrelated symlink {} -> {}",
                command_path.display(),
                target.display()
            ),
            LinkState::ConflictingEntry => anyhow::bail!(
                "refusing to remove existing non-symlink {}",
                command_path.display()
            ),
        },
    };

    let final_state = inspect_link(&command_path, &source, home)?;
    Ok(build_report(
        outcome,
        &command_path,
        &final_state,
        &source,
        home,
        path,
    ))
}

#[cfg(unix)]
fn absolute_path(path: &Path) -> anyhow::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("cannot resolve current directory: {e}"))?
            .join(path))
    }
}

#[cfg(unix)]
fn ensure_stable_install_source(
    operation: Operation,
    source: &Path,
    home: &Path,
) -> anyhow::Result<()> {
    if !matches!(operation, Operation::Install | Operation::Repair)
        || !looks_like_app_sidecar(source)
    {
        return Ok(());
    }

    let stable_sources = [
        home.join("Applications/Scrybe.app/Contents/MacOS/scrybe"),
        PathBuf::from("/Applications/Scrybe.app/Contents/MacOS/scrybe"),
    ];
    if stable_sources
        .iter()
        .any(|candidate| same_existing_file(source, candidate))
    {
        return Ok(());
    }

    anyhow::bail!(
        "move Scrybe.app to /Applications or ~/Applications and reopen it before installing the shell command"
    )
}

#[cfg(unix)]
fn looks_like_app_sidecar(source: &Path) -> bool {
    source.file_name().is_some_and(|name| name == "scrybe")
        && source
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "MacOS")
        && source
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .is_some_and(|name| name == "Contents")
        && source
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .is_some_and(|name| name == "Scrybe.app")
}

#[cfg(unix)]
fn inspect_link(command_path: &Path, source: &Path, home: &Path) -> anyhow::Result<LinkState> {
    let metadata = match std::fs::symlink_metadata(command_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LinkState::Missing)
        }
        Err(error) => {
            return Err(anyhow::anyhow!(
                "cannot inspect {}: {error}",
                command_path.display()
            ))
        }
    };

    if !metadata.file_type().is_symlink() {
        return Ok(LinkState::ConflictingEntry);
    }

    let raw_target = std::fs::read_link(command_path)
        .map_err(|e| anyhow::anyhow!("cannot read symlink {}: {e}", command_path.display()))?;
    let target = if raw_target.is_absolute() {
        raw_target
    } else {
        command_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(raw_target)
    };

    if same_existing_file(&target, source) {
        return Ok(LinkState::Current(target));
    }
    if !target.exists() {
        return Ok(if is_managed_scrybe_target(&target, home) {
            LinkState::Broken(target)
        } else {
            LinkState::ConflictingSymlink(target)
        });
    }
    if is_managed_scrybe_target(&target, home) {
        return Ok(LinkState::ManagedOther(target));
    }
    Ok(LinkState::ConflictingSymlink(target))
}

#[cfg(unix)]
fn same_existing_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

#[cfg(unix)]
fn is_managed_scrybe_target(target: &Path, home: &Path) -> bool {
    let known = [
        home.join("Applications/Scrybe.app/Contents/MacOS/scrybe"),
        PathBuf::from("/Applications/Scrybe.app/Contents/MacOS/scrybe"),
        home.join("venv/bin/scrybe"),
    ];
    known.iter().any(|candidate| target == candidate)
        || (target.starts_with(home.join(".local/share/scrybe"))
            && target.file_name().is_some_and(|name| name == "scrybe"))
}

#[cfg(unix)]
fn replace_link_atomically(command_path: &Path, source: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::symlink;

    let bin_dir = command_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("shell command path has no parent"))?;
    let created_bin_dir = !bin_dir.exists();
    if created_bin_dir {
        std::fs::create_dir_all(bin_dir)
            .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", bin_dir.display()))?;
    }

    let mut temporary = None;
    for attempt in 0..100_u32 {
        let candidate = bin_dir.join(format!(".scrybe-install-{}-{attempt}", std::process::id()));
        match symlink(source, &candidate) {
            Ok(()) => {
                temporary = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                if created_bin_dir {
                    let _ = std::fs::remove_dir(bin_dir);
                }
                return Err(anyhow::anyhow!(
                    "cannot create temporary shell link in {}: {error}",
                    bin_dir.display()
                ));
            }
        }
    }

    let temporary = temporary.ok_or_else(|| {
        anyhow::anyhow!(
            "cannot reserve a temporary shell link in {}",
            bin_dir.display()
        )
    })?;
    if let Err(error) = std::fs::rename(&temporary, command_path) {
        let _ = std::fs::remove_file(&temporary);
        if created_bin_dir {
            let _ = std::fs::remove_dir(bin_dir);
        }
        return Err(anyhow::anyhow!(
            "cannot install shell link at {}: {error}",
            command_path.display()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn state_name(state: &LinkState) -> &'static str {
    match state {
        LinkState::Missing => "not_installed",
        LinkState::Current(_) => "installed",
        LinkState::Broken(_) => "broken",
        LinkState::ManagedOther(_) => "stale",
        LinkState::ConflictingSymlink(_) | LinkState::ConflictingEntry => "conflict",
    }
}

#[cfg(unix)]
fn build_report(
    outcome: String,
    command_path: &Path,
    state: &LinkState,
    source: &Path,
    home: &Path,
    path: Option<&std::ffi::OsStr>,
) -> Report {
    let target_path = match state {
        LinkState::Current(target)
        | LinkState::Broken(target)
        | LinkState::ManagedOther(target)
        | LinkState::ConflictingSymlink(target) => Some(target.display().to_string()),
        LinkState::Missing | LinkState::ConflictingEntry => None,
    };
    let path_winner = find_path_winner(path).map(|winner| winner.display().to_string());
    let other_installations = find_other_installations(path, command_path, source, home)
        .into_iter()
        .map(|entry| entry.display().to_string())
        .collect::<Vec<_>>();
    let app_bundles = find_app_bundles(home)
        .into_iter()
        .map(|entry| entry.display().to_string())
        .collect::<Vec<_>>();

    let mut warnings = Vec::new();
    let bin_dir = command_path.parent().unwrap_or_else(|| Path::new("."));
    let bin_dir_on_path = path
        .map(std::env::split_paths)
        .is_some_and(|mut entries| entries.any(|entry| entry == bin_dir));
    if matches!(
        outcome.as_str(),
        "installed" | "already_installed" | "repaired"
    ) && !bin_dir_on_path
    {
        warnings.push(format!(
            "{} is not currently on PATH; add it to your shell configuration",
            bin_dir.display()
        ));
    }
    if let Some(winner) = &path_winner {
        if !same_existing_file(Path::new(winner), command_path)
            && !same_existing_file(Path::new(winner), source)
        {
            warnings.push(format!(
                "another scrybe command currently wins PATH resolution: {winner}"
            ));
        }
    }
    if !other_installations.is_empty() {
        warnings.push(format!(
            "other Scrybe command installations were left untouched: {}",
            other_installations.join(", ")
        ));
    }
    if app_bundles.len() > 1 {
        warnings.push(format!(
            "multiple Scrybe app bundles exist; remove the one you do not use: {}",
            app_bundles.join(", ")
        ));
    }

    let message = match outcome.as_str() {
        "installed" => format!("Installed the scrybe command at {}", command_path.display()),
        "already_installed" => format!(
            "The scrybe command is already installed correctly at {}",
            command_path.display()
        ),
        "repaired" => format!("Repaired the scrybe command at {}", command_path.display()),
        "removed" => format!("Removed the scrybe command at {}", command_path.display()),
        "not_installed" => format!(
            "No managed scrybe command exists at {}",
            command_path.display()
        ),
        "broken" => format!(
            "The scrybe command link is broken at {}",
            command_path.display()
        ),
        "stale" => format!(
            "The scrybe command points to another managed installation at {}",
            command_path.display()
        ),
        "conflict" => format!(
            "The scrybe command path is occupied at {}",
            command_path.display()
        ),
        _ => format!("Scrybe shell command state: {outcome}"),
    };

    Report {
        outcome,
        command_path: command_path.display().to_string(),
        target_path,
        path_winner,
        other_installations,
        app_bundles,
        warnings,
        message,
    }
}

#[cfg(unix)]
fn find_path_winner(path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    for directory in path.map(std::env::split_paths).into_iter().flatten() {
        let candidate = directory.join(COMMAND_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn find_other_installations(
    path: Option<&std::ffi::OsStr>,
    command_path: &Path,
    source: &Path,
    home: &Path,
) -> Vec<PathBuf> {
    let mut found = BTreeSet::new();
    let mut directories = path
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect::<BTreeSet<_>>();
    directories.extend([
        home.join(".local/bin"),
        home.join("bin"),
        home.join(".cargo/bin"),
        home.join("venv/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
    ]);
    for directory in directories {
        let candidate = directory.join(COMMAND_NAME);
        if candidate == command_path || same_existing_file(&candidate, source) {
            continue;
        }
        if candidate.is_file() {
            found.insert(candidate);
        }
    }
    found.into_iter().collect()
}

#[cfg(unix)]
fn find_app_bundles(home: &Path) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        [
            home.join("Applications").join("Scrybe.app"),
            PathBuf::from("/Applications/Scrybe.app"),
        ]
        .into_iter()
        .filter(|candidate| candidate.is_dir())
        .collect()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = home;
        Vec::new()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    struct Fixture {
        temp: TempDir,
        source: PathBuf,
        bin_dir: PathBuf,
        home: PathBuf,
        path: OsString,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = TempDir::new().unwrap();
            let source = temp.path().join("current").join("scrybe");
            std::fs::create_dir_all(source.parent().unwrap()).unwrap();
            std::fs::write(&source, b"current").unwrap();
            let bin_dir = temp.path().join("home").join(".local").join("bin");
            let home = temp.path().join("home");
            let path = std::env::join_paths([bin_dir.clone()]).unwrap();
            Self {
                temp,
                source,
                bin_dir,
                home,
                path,
            }
        }

        fn run(&self, operation: Operation) -> anyhow::Result<Report> {
            run_with(
                operation,
                &self.bin_dir,
                &self.source,
                &self.home,
                Some(&self.path),
            )
        }

        fn command(&self) -> PathBuf {
            self.bin_dir.join("scrybe")
        }
    }

    #[test]
    fn install_creates_link_and_second_install_is_idempotent() {
        let fixture = Fixture::new();
        let installed = fixture.run(Operation::Install).unwrap();
        assert_eq!(installed.outcome, "installed");
        assert_eq!(
            std::fs::read_link(fixture.command()).unwrap(),
            fixture.source
        );

        let again = fixture.run(Operation::Install).unwrap();
        assert_eq!(again.outcome, "already_installed");
        assert_eq!(
            std::fs::read_link(fixture.command()).unwrap(),
            fixture.source
        );
    }

    #[test]
    fn install_repairs_a_broken_link_without_leaving_temporary_files() {
        let fixture = Fixture::new();
        std::fs::create_dir_all(&fixture.bin_dir).unwrap();
        let missing = fixture.home.join("venv/bin/scrybe");
        symlink(&missing, fixture.command()).unwrap();

        let report = fixture.run(Operation::Install).unwrap();
        assert_eq!(report.outcome, "repaired");
        assert_eq!(
            std::fs::read_link(fixture.command()).unwrap(),
            fixture.source
        );
        let leftovers = std::fs::read_dir(&fixture.bin_dir)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".scrybe-install-")
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn install_repairs_a_previous_managed_app_link() {
        let fixture = Fixture::new();
        let old = fixture
            .home
            .join("Applications")
            .join("Scrybe.app")
            .join("Contents")
            .join("MacOS")
            .join("scrybe");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, b"old").unwrap();
        std::fs::create_dir_all(&fixture.bin_dir).unwrap();
        symlink(&old, fixture.command()).unwrap();

        let report = fixture.run(Operation::Repair).unwrap();
        assert_eq!(report.outcome, "repaired");
        assert_eq!(
            std::fs::read_link(fixture.command()).unwrap(),
            fixture.source
        );
    }

    #[test]
    fn install_refuses_to_overwrite_a_regular_file() {
        let fixture = Fixture::new();
        std::fs::create_dir_all(&fixture.bin_dir).unwrap();
        std::fs::write(fixture.command(), b"not ours").unwrap();

        let error = fixture.run(Operation::Install).unwrap_err().to_string();
        assert!(error.contains("refusing to overwrite"));
        assert_eq!(std::fs::read(fixture.command()).unwrap(), b"not ours");
    }

    #[test]
    fn install_refuses_to_overwrite_an_unrelated_healthy_symlink() {
        let fixture = Fixture::new();
        let unrelated = fixture.temp.path().join("other-program");
        std::fs::write(&unrelated, b"other").unwrap();
        std::fs::create_dir_all(&fixture.bin_dir).unwrap();
        symlink(&unrelated, fixture.command()).unwrap();

        let error = fixture.run(Operation::Install).unwrap_err().to_string();
        assert!(error.contains("unrelated symlink"));
        assert_eq!(std::fs::read_link(fixture.command()).unwrap(), unrelated);
    }

    #[test]
    fn install_and_uninstall_refuse_an_unrelated_broken_symlink() {
        let fixture = Fixture::new();
        let unrelated = fixture.temp.path().join("project/venv/bin/scrybe");
        std::fs::create_dir_all(&fixture.bin_dir).unwrap();
        symlink(&unrelated, fixture.command()).unwrap();

        let install_error = fixture.run(Operation::Install).unwrap_err().to_string();
        assert!(install_error.contains("unrelated symlink"));
        assert_eq!(std::fs::read_link(fixture.command()).unwrap(), unrelated);

        let uninstall_error = fixture.run(Operation::Uninstall).unwrap_err().to_string();
        assert!(uninstall_error.contains("unrelated symlink"));
        assert_eq!(std::fs::read_link(fixture.command()).unwrap(), unrelated);
    }

    #[test]
    fn uninstall_removes_managed_and_broken_links_but_not_conflicts() {
        let fixture = Fixture::new();
        fixture.run(Operation::Install).unwrap();
        let removed = fixture.run(Operation::Uninstall).unwrap();
        assert_eq!(removed.outcome, "removed");
        assert!(!fixture.command().exists());

        symlink(fixture.home.join("venv/bin/scrybe"), fixture.command()).unwrap();
        let removed_broken = fixture.run(Operation::Uninstall).unwrap();
        assert_eq!(removed_broken.outcome, "removed");

        let unrelated = fixture.temp.path().join("other");
        std::fs::write(&unrelated, b"other").unwrap();
        symlink(&unrelated, fixture.command()).unwrap();
        let error = fixture.run(Operation::Uninstall).unwrap_err().to_string();
        assert!(error.contains("refusing to remove unrelated"));
        assert_eq!(std::fs::read_link(fixture.command()).unwrap(), unrelated);
    }

    #[test]
    fn status_reports_broken_without_mutating_it() {
        let fixture = Fixture::new();
        let missing = fixture.home.join("venv/bin/scrybe");
        std::fs::create_dir_all(&fixture.bin_dir).unwrap();
        symlink(&missing, fixture.command()).unwrap();

        let report = fixture.run(Operation::Status).unwrap();
        assert_eq!(report.outcome, "broken");
        assert_eq!(std::fs::read_link(fixture.command()).unwrap(), missing);
    }

    #[test]
    fn report_warns_when_bin_directory_is_not_on_path() {
        let fixture = Fixture::new();
        let elsewhere = fixture.temp.path().join("elsewhere");
        let path = std::env::join_paths([elsewhere]).unwrap();
        let report = run_with(
            Operation::Install,
            &fixture.bin_dir,
            &fixture.source,
            &fixture.home,
            Some(&path),
        )
        .unwrap();
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("not currently on PATH")));
    }

    #[test]
    fn a_path_alias_to_the_same_executable_is_not_a_competing_install() {
        let fixture = Fixture::new();
        let alias_dir = fixture.temp.path().join("alias-bin");
        std::fs::create_dir_all(&alias_dir).unwrap();
        symlink(&fixture.source, alias_dir.join("scrybe")).unwrap();
        let path = std::env::join_paths([alias_dir, fixture.bin_dir.clone()]).unwrap();

        let report = run_with(
            Operation::Install,
            &fixture.bin_dir,
            &fixture.source,
            &fixture.home,
            Some(&path),
        )
        .unwrap();
        assert!(!report
            .warnings
            .iter()
            .any(|warning| warning.contains("wins PATH resolution")));
    }

    #[test]
    fn report_finds_the_legacy_venv_command_even_when_it_is_off_path() {
        let fixture = Fixture::new();
        let legacy = fixture.home.join("venv/bin/scrybe");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, b"legacy").unwrap();

        let report = fixture.run(Operation::Status).unwrap();
        assert!(report
            .other_installations
            .contains(&legacy.display().to_string()));
    }

    #[test]
    fn install_refuses_an_app_sidecar_outside_applications() {
        let fixture = Fixture::new();
        let source = fixture
            .temp
            .path()
            .join("AppTranslocation/d/Scrybe.app/Contents/MacOS/scrybe");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"transient").unwrap();

        let error = ensure_stable_install_source(Operation::Install, &source, &fixture.home)
            .unwrap_err()
            .to_string();
        assert!(error.contains("move Scrybe.app"));
        assert!(ensure_stable_install_source(Operation::Status, &source, &fixture.home).is_ok());
    }

    #[test]
    fn install_accepts_the_user_applications_sidecar() {
        let fixture = Fixture::new();
        let source = fixture
            .home
            .join("Applications/Scrybe.app/Contents/MacOS/scrybe");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"stable").unwrap();

        assert!(ensure_stable_install_source(Operation::Install, &source, &fixture.home).is_ok());
    }
}
