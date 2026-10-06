//! Adding the AppImage to the desktop's app launcher, which an AppImage doesn't do by itself.
//! Done the first time it runs, unless a tool like AppImageLauncher or Gear Lever already has.
//! An entry removed after that stays removed.

use std::io;
use std::path::{Path, PathBuf};

const DESKTOP_ID: &str = "uk.co.jamedjo.seesongs";
/// The desktop file the AppImage ships, with `Exec=seesongs` to be pointed at the AppImage.
const PACKAGED_ENTRY: &str = include_str!("../packaging/linux/uk.co.jamedjo.seesongs.desktop");
const ICON: &str = include_str!("../packaging/linux/uk.co.jamedjo.seesongs.svg");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Added,
    NotAdded,
    /// Another tool's desktop file already starts this AppImage.
    AddedElsewhere,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Launcher {
    appimage: PathBuf,
    /// Where desktop files and icons go: `~/.local/share` unless `XDG_DATA_HOME` says otherwise.
    data: PathBuf,
    /// Left once the first run has added the entry, so a removed entry isn't added back.
    added_once: PathBuf,
}

impl Launcher {
    /// `None` unless running from an AppImage, whose runtime sets `APPIMAGE` to its path.
    pub fn from_env() -> Option<Self> {
        let appimage = PathBuf::from(std::env::var_os("APPIMAGE")?);
        appimage.is_file().then_some(())?;
        Some(Self {
            appimage,
            data: dirs::data_dir()?,
            added_once: dirs::config_dir()?
                .join("seesongs")
                .join("added-to-launcher"),
        })
    }

    fn entry(&self) -> PathBuf {
        self.data
            .join("applications")
            .join(format!("{DESKTOP_ID}.desktop"))
    }

    fn icon(&self) -> PathBuf {
        self.data
            .join("icons/hicolor/scalable/apps")
            .join(format!("{DESKTOP_ID}.svg"))
    }

    fn state(&self) -> State {
        if self.entry().exists() {
            State::Added
        } else if self.added_elsewhere() {
            State::AddedElsewhere
        } else {
            State::NotAdded
        }
    }

    fn added_elsewhere(&self) -> bool {
        let Ok(entries) = std::fs::read_dir(self.data.join("applications")) else {
            return false;
        };
        let path = self.appimage.to_string_lossy();
        entries.flatten().any(|entry| {
            entry.path().extension().is_some_and(|e| e == "desktop")
                && std::fs::read_to_string(entry.path()).is_ok_and(|text| {
                    text.lines()
                        .filter(|line| line.starts_with("Exec=") || line.starts_with("TryExec="))
                        .any(|line| line.contains(path.as_ref()))
                })
        })
    }

    fn add(&self) -> io::Result<()> {
        let entry = desktop_entry(&self.appimage)?;
        write_creating_dirs(&self.icon(), ICON)?;
        write_creating_dirs(&self.entry(), &entry)
    }

    /// Add the entry on the first run, and keep it pointing at this AppImage after that.
    pub fn start(&self) {
        let first_run = !self.added_once.exists();
        let mut done = self.set_up(first_run);
        if first_run && done.is_ok() {
            done = write_creating_dirs(&self.added_once, "");
        }
        if let Err(error) = done {
            eprintln!("couldn't update the app launcher entry: {error}");
        }
    }

    fn set_up(&self, first_run: bool) -> io::Result<()> {
        if first_run && self.state() == State::NotAdded {
            self.add()
        } else {
            self.repair()
        }
    }

    /// Point an entry added earlier at this AppImage, if the one it starts has gone, as it does
    /// when the AppImage is moved or replaced by an update with another name.
    fn repair(&self) -> io::Result<()> {
        let Ok(text) = std::fs::read_to_string(self.entry()) else {
            return Ok(());
        };
        let starts = text.lines().find_map(|line| line.strip_prefix("TryExec="));
        if starts.is_some_and(|path| Path::new(&unescape(path)).exists()) {
            return Ok(());
        }
        self.add()
    }
}

fn write_creating_dirs(path: &Path, contents: &str) -> io::Result<()> {
    std::fs::create_dir_all(path.parent().expect("a file in a directory"))?;
    std::fs::write(path, contents)
}

/// The packaged desktop file, starting `appimage`, and hidden by launchers once it's gone.
///
/// A path with `%` is refused: Exec would need it doubled, but GLib looks for the program
/// without undoubling it, so the launcher would hide the entry.
fn desktop_entry(appimage: &Path) -> io::Result<String> {
    let path = appimage
        .to_str()
        .filter(|path| !path.contains(['\n', '\r', '%']))
        .ok_or_else(|| io::Error::other("rename the AppImage without % or line breaks first"))?;
    let mut entry = String::new();
    for line in PACKAGED_ENTRY.lines() {
        if line.starts_with("Exec=") {
            entry += &format!("TryExec={}\nExec={}\n", escape(path), escape(&quote(path)));
        } else {
            entry += line;
            entry += "\n";
        }
    }
    Ok(entry)
}

/// One Exec argument, quoted so spaces and other reserved characters are taken literally.
fn quote(argument: &str) -> String {
    let mut quoted = String::from('"');
    for c in argument.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted
}

/// A string value's escaping, which applies on top of an Exec argument's quoting.
fn escape(value: &str) -> String {
    value.replace('\\', "\\\\")
}

fn unescape(value: &str) -> String {
    value.replace("\\\\", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value<'a>(entry: &'a str, key: &str) -> &'a str {
        entry
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
            .unwrap_or_else(|| panic!("no {key} in {entry}"))
    }

    #[test]
    fn entry_starts_the_appimage() {
        let entry = desktop_entry(Path::new("/home/me/Apps/seesongs-x86_64.AppImage")).unwrap();
        assert_eq!(
            value(&entry, "Exec"),
            r#""/home/me/Apps/seesongs-x86_64.AppImage""#
        );
        assert_eq!(
            value(&entry, "TryExec"),
            "/home/me/Apps/seesongs-x86_64.AppImage"
        );
        assert_eq!(value(&entry, "Icon"), DESKTOP_ID);
        assert_eq!(value(&entry, "StartupWMClass"), "seesongs");
        assert_eq!(entry.matches("Exec=").count(), 2, "{entry}");
    }

    #[test]
    fn reserved_characters_in_the_path_are_escaped() {
        let entry = desktop_entry(Path::new(r#"/home/me/My "$apps" `x`\y.AppImage"#)).unwrap();
        assert_eq!(
            value(&entry, "Exec"),
            r#""/home/me/My \\"\\$apps\\" \\`x\\`\\\\y.AppImage""#
        );
        assert_eq!(
            value(&entry, "TryExec"),
            r#"/home/me/My "$apps" `x`\\y.AppImage"#
        );
    }

    #[test]
    fn paths_a_launcher_would_misread_are_refused() {
        assert!(desktop_entry(Path::new("/home/me/a\nb.AppImage")).is_err());
        assert!(desktop_entry(Path::new("/home/me/100%.AppImage")).is_err());
    }

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("seesongs-launcher-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        fn launcher(&self, appimage: &str) -> Launcher {
            let appimage = self.0.join(appimage);
            std::fs::write(&appimage, "").unwrap();
            Launcher {
                appimage,
                data: self.0.join("data"),
                added_once: self.0.join("config/added-to-launcher"),
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn adding() {
        let scratch = Scratch::new("add");
        let launcher = scratch.launcher("seesongs.AppImage");
        assert_eq!(launcher.state(), State::NotAdded);
        launcher.add().unwrap();
        assert_eq!(launcher.state(), State::Added);
        assert!(launcher.icon().exists());
    }

    #[test]
    fn another_tools_entry_for_the_same_appimage_counts() {
        let scratch = Scratch::new("elsewhere");
        let launcher = scratch.launcher("seesongs.AppImage");
        let applications = launcher.data.join("applications");
        std::fs::create_dir_all(&applications).unwrap();
        let other = format!("[Desktop Entry]\nExec={} %U\n", launcher.appimage.display());
        std::fs::write(
            applications.join("appimagekit_0123-seesongs.desktop"),
            other,
        )
        .unwrap();
        assert_eq!(launcher.state(), State::AddedElsewhere);
    }

    #[test]
    fn repair_follows_a_moved_appimage_only() {
        let scratch = Scratch::new("repair");
        let old = scratch.launcher("old.AppImage");
        old.add().unwrap();
        let new = scratch.launcher("new.AppImage");

        new.repair().unwrap();
        let entry = std::fs::read_to_string(new.entry()).unwrap();
        assert!(entry.contains("old.AppImage"), "{entry}");

        std::fs::remove_file(&old.appimage).unwrap();
        new.repair().unwrap();
        let entry = std::fs::read_to_string(new.entry()).unwrap();
        assert!(
            entry.contains("new.AppImage") && !entry.contains("old"),
            "{entry}"
        );
    }

    #[test]
    fn added_on_the_first_run_only() {
        let scratch = Scratch::new("first");
        let launcher = scratch.launcher("seesongs.AppImage");
        launcher.start();
        assert_eq!(launcher.state(), State::Added);
        std::fs::remove_file(launcher.entry()).unwrap();
        launcher.start();
        assert_eq!(launcher.state(), State::NotAdded);
    }

    #[test]
    fn not_added_over_another_tools_entry() {
        let scratch = Scratch::new("first-elsewhere");
        let launcher = scratch.launcher("seesongs.AppImage");
        let applications = launcher.data.join("applications");
        std::fs::create_dir_all(&applications).unwrap();
        let other = format!("[Desktop Entry]\nExec={}\n", launcher.appimage.display());
        std::fs::write(applications.join("gearlever_seesongs.desktop"), other).unwrap();
        launcher.set_up(true).unwrap();
        assert!(!launcher.entry().exists());
    }

    #[test]
    fn repair_leaves_no_entry_alone() {
        let scratch = Scratch::new("none");
        let launcher = scratch.launcher("seesongs.AppImage");
        launcher.repair().unwrap();
        assert_eq!(launcher.state(), State::NotAdded);
    }
}
