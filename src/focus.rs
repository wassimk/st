//! macOS Do Not Disturb, driven through the bundled "st Focus" shortcut, since macOS has no
//! public API for Focus.

use chrono::{DateTime, Local, TimeDelta};
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
