//! The app as Velopack packages it, as the Linux AppImage, which an update replaces where it is.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateManager, VelopackApp};

/// Releases, with the packages updates are made from.
const REPOSITORY: &str = "https://github.com/Jamedjo/seesongs";
/// Soon enough for a fix to arrive the same day, and far under GitHub's limit for anonymous use.
const CHECK_EVERY: Duration = Duration::from_secs(4 * 60 * 60);

static UPDATES: OnceLock<UpdateManager> = OnceLock::new();

/// Returns straight away, unless an update downloaded last time wasn't installed on closing, in
/// which case it's installed and the app restarted.
pub fn run_installer_step() {
    VelopackApp::build().run();
}

/// Check for a new release now and every few hours, downloading it quietly for the next close.
/// Does nothing for a copy Velopack didn't package, as when it's built from source.
pub fn keep_up_to_date() {
    let source = GithubSource::new(REPOSITORY, None, false);
    let Ok(manager) = UpdateManager::new(source, None, None) else {
        return;
    };
    let manager = UPDATES.get_or_init(|| manager);
    let checker = std::thread::Builder::new()
        .name("updates".into())
        .spawn(move || loop {
            if let Err(error) = download_update(manager) {
                eprintln!("couldn't check for an update: {error}");
            }
            std::thread::sleep(CHECK_EVERY);
        });
    if let Err(error) = checker {
        eprintln!("couldn't start checking for updates: {error}");
    }
}

fn download_update(manager: &UpdateManager) -> Result<(), velopack::Error> {
    if let UpdateCheck::UpdateAvailable(update) = manager.check_for_updates()? {
        manager.download_updates(&update, None)?;
    }
    Ok(())
}

/// Install a downloaded update once the window has closed, without starting it again.
pub fn update_on_close() {
    // Closing can be asked for twice before the app is gone, and only one installer can run.
    static HANDED_OVER: AtomicBool = AtomicBool::new(false);
    if HANDED_OVER.swap(true, Ordering::Relaxed) {
        return;
    }
    let Some(manager) = UPDATES.get() else {
        return;
    };
    let Some(update) = manager.get_update_pending_restart() else {
        return;
    };
    if let Err(error) =
        manager.wait_exit_then_apply_updates(update, true, false, Vec::<String>::new())
    {
        eprintln!("couldn't install the update: {error}");
    }
}
