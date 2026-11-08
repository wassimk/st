//! macOS Do Not Disturb, driven through the bundled "st Focus" shortcut, since macOS has no
//! public API for Focus.

use chrono::{DateTime, Local, TimeDelta};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

pub const SHORTCUT_NAME: &str = "st Focus";

/// Shared by `shortcuts list` and `shortcuts run`, kept well under zoom-autostatus's limit
pub const BUDGET: Duration = Duration::from_secs(10);

// Set Focus rejects an end time that has passed by the time it runs
const MIN_UNTIL: TimeDelta = TimeDelta::seconds(60);

#[derive(Debug, Clone, PartialEq)]
pub enum FocusRequest {
    On,
    Until(DateTime<Local>),
    Off,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FocusOutcome {
    On,
    OnUntil(DateTime<Local>),
    Off,
    Skipped(String),
    Failed(String),
}

pub trait Shortcuts {
    fn list(&self, deadline: Instant) -> Result<Vec<String>, String>;
    fn run(&self, name: &str, input: &str, deadline: Instant) -> Result<(), String>;
}

fn usable_until(request: &FocusRequest, now: DateTime<Local>) -> Option<DateTime<Local>> {
    match request {
        FocusRequest::Until(end) if *end - now >= MIN_UNTIL => Some(*end),
        _ => None,
    }
}

pub fn shortcut_input(request: &FocusRequest, now: DateTime<Local>) -> String {
    match (request, usable_until(request, now)) {
        (FocusRequest::Off, _) => "off".to_string(),
        (_, Some(end)) => end.format("%Y-%m-%dT%H:%M:%S%z").to_string(),
        _ => "on".to_string(),
    }
}

pub fn is_installed(names: &[String]) -> bool {
    names.iter().any(|name| name == SHORTCUT_NAME)
}

pub fn apply(
    shortcuts: &impl Shortcuts,
    request: &FocusRequest,
    now: DateTime<Local>,
) -> FocusOutcome {
    let deadline = Instant::now() + BUDGET;

    match shortcuts.list(deadline) {
        Ok(names) if is_installed(&names) => {}
        Ok(_) => {
            return FocusOutcome::Failed(format!(
                "The {SHORTCUT_NAME} shortcut isn't installed. Run: st setup-focus"
            ));
        }
        Err(e) => return FocusOutcome::Failed(e),
    }

    if let Err(e) = shortcuts.run(SHORTCUT_NAME, &shortcut_input(request, now), deadline) {
        return FocusOutcome::Failed(e);
    }

    match (request, usable_until(request, now)) {
        (FocusRequest::Off, _) => FocusOutcome::Off,
        (_, Some(end)) => FocusOutcome::OnUntil(end),
        _ => FocusOutcome::On,
    }
}

pub struct ShellShortcuts {
    pub program: PathBuf,
}

impl Default for ShellShortcuts {
    fn default() -> Self {
        ShellShortcuts {
            program: PathBuf::from("/usr/bin/shortcuts"),
        }
    }
}

impl ShellShortcuts {
    fn output(&self, args: &[&str], deadline: Instant) -> Result<Output, String> {
        let mut child = Command::new(&self.program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Couldn't run {}: {e}", self.program.display()))?;

        // shortcuts prints a few lines at most, so reading after exit can't fill the pipes
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("timed out after {}s", BUDGET.as_secs()));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(format!("Couldn't wait for shortcuts: {e}")),
            }
        };

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        if let Some(mut out) = child.stdout.take() {
            let _ = out.read_to_end(&mut stdout);
        }
        if let Some(mut err) = child.stderr.take() {
            let _ = err.read_to_end(&mut stderr);
        }
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
}

impl Shortcuts for ShellShortcuts {
    fn list(&self, deadline: Instant) -> Result<Vec<String>, String> {
        let output = self.output(&["list"], deadline)?;
        if !output.status.success() {
            return Err(failure_message(&output));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect())
    }

    fn run(&self, name: &str, input: &str, deadline: Instant) -> Result<(), String> {
        static RUNS: AtomicUsize = AtomicUsize::new(0);
        let input_path = std::env::temp_dir().join(format!(
            "st-focus-{}-{}.txt",
            std::process::id(),
            RUNS.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&input_path, input)
            .map_err(|e| format!("Couldn't write {}: {e}", input_path.display()))?;

        let path = input_path.display().to_string();
        let result = self.output(&["run", name, "--input-path", &path], deadline);
        let _ = std::fs::remove_file(&input_path);

        let output = result?;
        if output.status.success() {
            Ok(())
        } else {
            Err(failure_message(&output))
        }
    }
}

fn failure_message(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("shortcuts exited with {}", output.status)
    } else {
        stderr
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::cell::RefCell;

    pub(super) struct FakeShortcuts {
        names: Result<Vec<String>, String>,
        run_result: Result<(), String>,
        runs: RefCell<Vec<(String, String)>>,
    }

    impl FakeShortcuts {
        pub(super) fn listing(names: &[&str]) -> Self {
            FakeShortcuts {
                names: Ok(names.iter().map(|n| n.to_string()).collect()),
                run_result: Ok(()),
                runs: RefCell::default(),
            }
        }

        pub(super) fn installed() -> Self {
            Self::listing(&[SHORTCUT_NAME])
        }

        fn run_error(message: &str) -> Self {
            FakeShortcuts {
                run_result: Err(message.to_string()),
                ..Self::installed()
            }
        }

        fn list_error(message: &str) -> Self {
            FakeShortcuts {
                names: Err(message.to_string()),
                ..Self::installed()
            }
        }

        pub(super) fn runs(&self) -> Vec<(String, String)> {
            self.runs.borrow().clone()
        }
    }

    impl Shortcuts for FakeShortcuts {
        fn list(&self, _deadline: Instant) -> Result<Vec<String>, String> {
            self.names.clone()
        }

        fn run(&self, name: &str, input: &str, _deadline: Instant) -> Result<(), String> {
            self.runs
                .borrow_mut()
                .push((name.to_string(), input.to_string()));
            self.run_result.clone()
        }
    }

    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 9, 10, 0, 0).unwrap()
    }

    #[test]
    fn input_for_on_and_off() {
        assert_eq!(shortcut_input(&FocusRequest::On, now()), "on");
        assert_eq!(shortcut_input(&FocusRequest::Off, now()), "off");
    }

    #[test]
    fn input_for_until_is_iso_with_offset() {
        let end = now() + TimeDelta::hours(2);
        assert_eq!(
            shortcut_input(&FocusRequest::Until(end), now()),
            end.format("%Y-%m-%dT%H:%M:%S%z").to_string()
        );
    }

    #[test]
    fn until_less_than_a_minute_away_becomes_on() {
        let soon = now() + TimeDelta::seconds(30);
        let past = now() - TimeDelta::hours(1);
        assert_eq!(shortcut_input(&FocusRequest::Until(soon), now()), "on");
        assert_eq!(shortcut_input(&FocusRequest::Until(past), now()), "on");
    }

    #[test]
    fn installed_needs_an_exact_name() {
        assert!(is_installed(&["dnd-on".into(), "st Focus".into()]));
        assert!(!is_installed(&["st Focus 2".into(), "st focus".into()]));
    }

    #[test]
    fn missing_shortcut_fails_without_running() {
        let fake = FakeShortcuts::listing(&["Zoom DND"]);
        let outcome = apply(&fake, &FocusRequest::On, now());
        assert_eq!(
            outcome,
            FocusOutcome::Failed(
                "The st Focus shortcut isn't installed. Run: st setup-focus".into()
            )
        );
        assert!(fake.runs().is_empty());
    }

    #[test]
    fn apply_maps_requests_to_outcomes() {
        let end = now() + TimeDelta::hours(1);
        let installed = FakeShortcuts::installed();
        assert_eq!(
            apply(&installed, &FocusRequest::On, now()),
            FocusOutcome::On
        );
        assert_eq!(
            apply(&installed, &FocusRequest::Until(end), now()),
            FocusOutcome::OnUntil(end)
        );
        assert_eq!(
            apply(&installed, &FocusRequest::Off, now()),
            FocusOutcome::Off
        );
        assert_eq!(
            installed.runs(),
            vec![
                ("st Focus".to_string(), "on".to_string()),
                (
                    "st Focus".to_string(),
                    end.format("%Y-%m-%dT%H:%M:%S%z").to_string()
                ),
                ("st Focus".to_string(), "off".to_string()),
            ]
        );
    }

    #[test]
    fn near_until_reports_plain_on() {
        let soon = now() + TimeDelta::seconds(30);
        assert_eq!(
            apply(
                &FakeShortcuts::installed(),
                &FocusRequest::Until(soon),
                now()
            ),
            FocusOutcome::On
        );
    }

    #[test]
    fn run_and_list_errors_become_failed() {
        assert_eq!(
            apply(&FakeShortcuts::run_error("boom"), &FocusRequest::Off, now()),
            FocusOutcome::Failed("boom".into())
        );
        assert_eq!(
            apply(
                &FakeShortcuts::list_error("nope"),
                &FocusRequest::Off,
                now()
            ),
            FocusOutcome::Failed("nope".into())
        );
    }
}

#[cfg(test)]
mod shell_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn script(body: &str) -> (ShellShortcuts, PathBuf) {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "st-focus-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shortcuts");
        let log = dir.join("log");
        let body = body.replace("$LOG", &log.display().to_string());
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (ShellShortcuts { program: path }, log)
    }

    fn deadline() -> Instant {
        Instant::now() + BUDGET
    }

    #[test]
    fn list_returns_one_name_per_line() {
        let (sh, _) = script("printf 'dnd-on\\nst Focus\\n'");
        assert_eq!(
            sh.list(deadline()).unwrap(),
            vec!["dnd-on".to_string(), "st Focus".to_string()]
        );
    }

    #[test]
    fn run_passes_name_and_input_file() {
        let (sh, log) = script("printf '%s\\n' \"$*\" > \"$LOG\"; cat \"$4\" >> \"$LOG\"");
        sh.run("st Focus", "on", deadline()).unwrap();
        let logged = std::fs::read_to_string(log).unwrap();
        let (args, input) = logged.split_once('\n').unwrap();
        assert!(args.starts_with("run st Focus --input-path "), "{args}");
        assert_eq!(input, "on");
    }

    #[test]
    fn nonzero_exit_reports_stderr() {
        let (sh, _) = script("echo 'Error: nope' >&2; exit 1");
        assert_eq!(
            sh.run("st Focus", "on", deadline()).unwrap_err(),
            "Error: nope"
        );
    }

    #[test]
    fn nonzero_exit_without_stderr_names_the_status() {
        let (sh, _) = script("exit 3");
        let err = sh.run("st Focus", "on", deadline()).unwrap_err();
        assert!(err.starts_with("shortcuts exited with"), "{err}");
    }

    #[test]
    fn hung_command_is_killed_at_the_deadline() {
        let (sh, _) = script("sleep 30");
        let start = Instant::now();
        let err = sh
            .run(
                "st Focus",
                "on",
                Instant::now() + Duration::from_millis(300),
            )
            .unwrap_err();
        assert_eq!(err, "timed out after 10s");
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn input_file_is_removed_afterward() {
        let (sh, log) = script("printf '%s' \"$4\" > \"$LOG\"");
        sh.run("st Focus", "off", deadline()).unwrap();
        let input_path = std::fs::read_to_string(log).unwrap();
        assert!(!std::path::Path::new(&input_path).exists(), "{input_path}");
    }

    #[test]
    fn default_uses_the_system_shortcuts() {
        assert_eq!(
            ShellShortcuts::default().program,
            PathBuf::from("/usr/bin/shortcuts")
        );
    }
}
