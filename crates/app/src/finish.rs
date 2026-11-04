use std::process::Command;
use std::thread;

use crate::prefs::Power;
use crate::lang::t;

/// F3: macOS's own sounds.
const DONE_SOUND: &str = "/System/Library/Sounds/Glass.aiff";
const FAILED_SOUND: &str = "/System/Library/Sounds/Basso.aiff";

/// F1–F3: every finished item, even while the window is in front.
// ponytail: osascript shows the banner as Script Editor's. Tauri's notification plugin uses
// NSUserNotificationCenter without a presenting delegate, so it stays silent while the app is
// in front (F2); switch to UNUserNotificationCenter once the app is signed for release.
pub fn notify(title: &str, body: &str, is_ok: bool) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let script = ["on run argv", "display notification (item 2 of argv) with title (item 1 of argv)", "end run"];
    let mut banner = Command::new("/usr/bin/osascript");
    for line in script {
        banner.args(["-e", line]);
    }
    banner.args([title, body]);
    // Separate from the banner so the sound plays even with notifications turned off.
    let mut sound = Command::new("/usr/bin/afplay");
    sound.arg(if is_ok { DONE_SOUND } else { FAILED_SOUND });
    for mut command in [banner, sound] {
        // Only the banner or the sound is missed; the result is in the window.
        thread::spawn(move || {
            if let Err(e) = command.status() {
                eprintln!("could not notify: {e}");
            }
        });
    }
}

/// F5: after the window's countdown.
pub fn power(power: Power) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err(t("아직 맥에서만 됨", "Only on the Mac for now").to_string());
    }
    let (program, args): (&str, &[&str]) = match power {
        Power::None => return Ok(()),
        Power::Sleep => ("/usr/bin/pmset", &["sleepnow"]),
        // Asks the apps to quit, like the Apple menu; macOS asks once to let Lapsewright do it.
        Power::Shutdown => ("/usr/bin/osascript", &["-e", r#"tell application "System Events" to shut down"#]),
    };
    let output = Command::new(program).args(args).output().map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    // -1743: the user said no to Lapsewright controlling System Events.
    if stderr.contains("-1743") {
        return Err(t(
            "맥이 허락 안 함 → 시스템 설정 › 개인정보 보호 및 보안 › 자동화 에서 Lapsewright 켜기",
            "macOS didn’t allow it → turn on Lapsewright in System Settings › Privacy & Security › Automation",
        )
        .to_string());
    }
    Err(format!("{}: {}", t("못 함", "Couldn’t do it"), stderr.trim()))
}
