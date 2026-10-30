use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveTime, Timelike, Weekday};
use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

const DEFAULT_BACK_HOUR: u32 = 7;
const CATCHING_UP_MINUTES: i64 = 5;
const LINEAR_OOO_REMINDER: &str =
    "Add an out-of-office event in Google Calendar (Linear shows you as Away)";

// --- Config ---

#[derive(Deserialize, Default)]
struct Config {
    github_org_id: Option<String>,
    github_org_name: Option<String>,
}

fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("st")
        .join("config.toml")
}

fn load_config() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_else(|e| {
            eprintln!("Warning: failed to parse {}: {e}", path.display());
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

// --- Date/time parsing ---

fn parse_back_date(date_str: &str, time_str: Option<&str>) -> Result<DateTime<Local>> {
    let today = Local::now().date_naive();
    let lower = date_str.to_lowercase();

    // Day names: "monday", "tuesday", etc. — next occurrence
    let weekday = match lower.as_str() {
        "monday" | "mon" => Some(Weekday::Mon),
        "tuesday" | "tue" | "tues" => Some(Weekday::Tue),
        "wednesday" | "wed" => Some(Weekday::Wed),
        "thursday" | "thu" | "thurs" => Some(Weekday::Thu),
        "friday" | "fri" => Some(Weekday::Fri),
        "saturday" | "sat" => Some(Weekday::Sat),
        "sunday" | "sun" => Some(Weekday::Sun),
        "tomorrow" => {
            let date = today + chrono::Duration::days(1);
            return Ok(to_local_datetime(date, parse_time(time_str)?));
        }
        _ => None,
    };

    let date = if let Some(day) = weekday {
        let today_weekday = today.weekday().num_days_from_monday();
        let target = day.num_days_from_monday();
        let delta = if target > today_weekday {
            target - today_weekday
        } else {
            7 - today_weekday + target
        };
        today + chrono::Duration::days(delta as i64)
    } else if let Some(date) = parse_date_with_separators(date_str, today) {
        date
    } else {
        anyhow::bail!(
            "Could not parse date: {date_str}\nExamples: friday, 3/10, 3-10-2026, tomorrow"
        );
    };

    Ok(to_local_datetime(date, parse_time(time_str)?))
}

fn parse_date_with_separators(input: &str, today: NaiveDate) -> Option<NaiveDate> {
    // Split on / or -
    let parts: Vec<&str> = input.split(&['/', '-'][..]).collect();

    match parts.len() {
        // M/D or M-D
        2 => {
            let month = parts[0].parse::<u32>().ok()?;
            let day = parts[1].parse::<u32>().ok()?;
            let mut year = today.year();
            let date = NaiveDate::from_ymd_opt(year, month, day)?;
            if date < today {
                year += 1;
            }
            NaiveDate::from_ymd_opt(year, month, day)
        }
        // Y-M-D or Y/M/D (4-digit year first)
        3 if parts[0].len() == 4 => {
            let year = parts[0].parse::<i32>().ok()?;
            let month = parts[1].parse::<u32>().ok()?;
            let day = parts[2].parse::<u32>().ok()?;
            NaiveDate::from_ymd_opt(year, month, day)
        }
        // M/D/Y or M-D-Y (2-digit or 4-digit year)
        3 => {
            let month = parts[0].parse::<u32>().ok()?;
            let day = parts[1].parse::<u32>().ok()?;
            let mut year = parts[2].parse::<i32>().ok()?;
            if year < 100 {
                year += 2000;
            }
            NaiveDate::from_ymd_opt(year, month, day)
        }
        _ => None,
    }
}

fn parse_time(input: Option<&str>) -> Result<NaiveTime> {
    let input = match input {
        Some(s) => s,
        None => return Ok(NaiveTime::from_hms_opt(DEFAULT_BACK_HOUR, 0, 0).unwrap()),
    };

    let s = input.to_lowercase();
    let s = s.trim();

    // Strip am/pm suffix and track it
    let (num_part, is_pm) = if let Some(rest) = s.strip_suffix("pm") {
        (rest.trim(), Some(true))
    } else if let Some(rest) = s.strip_suffix("p.m.") {
        (rest.trim(), Some(true))
    } else if let Some(rest) = s.strip_suffix("am") {
        (rest.trim(), Some(false))
    } else if let Some(rest) = s.strip_suffix("a.m.") {
        (rest.trim(), Some(false))
    } else {
        (s, None)
    };

    // Parse hour and optional minutes
    let (hour, minute): (u32, u32) = if let Some((h, m)) = num_part.split_once(':') {
        (h.parse()?, m.parse()?)
    } else {
        (num_part.parse()?, 0)
    };

    // Apply AM/PM
    let hour = match is_pm {
        Some(true) if hour < 12 => hour + 12,
        Some(false) if hour == 12 => 0,
        _ => hour,
    };

    NaiveTime::from_hms_opt(hour, minute, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid time: {input}"))
}

fn to_local_datetime(date: NaiveDate, time: NaiveTime) -> DateTime<Local> {
    date.and_time(time)
        .and_local_timezone(Local)
        .unwrap()
}

fn format_back_date(dt: DateTime<Local>) -> String {
    let today = Local::now().date_naive();
    let date = dt.date_naive();
    let days_away = (date - today).num_days();

    if days_away <= 7 {
        format!("Back {}.", date.format("%A"))
    } else {
        format!("Back {}/{}.", date.month(), date.day())
    }
}

fn format_back_date_with_time(dt: DateTime<Local>) -> String {
    let today = Local::now().date_naive();
    let date = dt.date_naive();
    let days_away = (date - today).num_days();
    let time = format_time(dt);

    if days_away <= 7 {
        format!("Back {} {}.", date.format("%A"), time)
    } else {
        format!("Back {}/{} {}.", date.month(), date.day(), time)
    }
}

fn format_time(dt: DateTime<Local>) -> String {
    let hour = dt.format("%I").to_string().trim_start_matches('0').to_string();
    let minute = dt.minute();
    let ampm = dt.format("%p").to_string().to_lowercase();

    if minute == 0 {
        format!("{}{}", hour, ampm)
    } else {
        format!("{}:{:02}{}", hour, minute, ampm)
    }
}

fn format_until(dt: DateTime<Local>, today: NaiveDate) -> String {
    if dt.date_naive() == today {
        format_time(dt)
    } else {
        format!("{} {}", dt.format("%A"), format_time(dt))
    }
}

// Four-day work week: Monday through Thursday
fn is_workday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Fri | Weekday::Sat | Weekday::Sun)
}

// Before the back hour on a workday you're still finishing the night before,
// so eod ends this morning rather than skipping a whole day
fn eod_back_date(today: NaiveDate, now: NaiveTime) -> NaiveDate {
    if is_workday(today) && now.hour() < DEFAULT_BACK_HOUR {
        today
    } else {
        next_workday(today)
    }
}

fn next_workday(today: NaiveDate) -> NaiveDate {
    let mut date = today + chrono::Duration::days(1);
    while !is_workday(date) {
        date += chrono::Duration::days(1);
    }
    date
}

// Without am/pm, "1" means whichever 1 o'clock comes next, so lunch at 1 is 1pm
fn parse_lunch_time(input: &str, now: NaiveTime) -> Result<NaiveTime> {
    let time = parse_time(Some(input))?;
    let has_meridiem = input.chars().any(|c| c.is_ascii_alphabetic());
    if !has_meridiem && time.hour() < 12 && time <= now {
        return Ok(time + chrono::Duration::hours(12));
    }
    Ok(time)
}

fn parse_lunch_back_time(input: Option<&str>) -> Result<DateTime<Local>> {
    let today = Local::now().date_naive();
    let time = match input {
        Some(s) => parse_lunch_time(s, Local::now().time())?,
        None => {
            // Next quarter hour + 1 hour
            let now = Local::now();
            let min = now.minute();
            let next_quarter = ((min / 15) + 1) * 15;
            let round_up = (next_quarter - min) as i64;
            let back = now + chrono::Duration::minutes(round_up + 60);
            return Ok(back);
        }
    };
    Ok(to_local_datetime(today, time))
}

// --- Status definitions ---

struct Status {
    keyword: &'static str,
    slack_text: &'static str,
    slack_emoji: &'static str,
    slack_dnd: bool,
    github_busy: bool,
    out_of_office: bool,
}

const STATUSES: &[Status] = &[
    Status {
        keyword: "lunch",
        slack_text: "",
        slack_emoji: ":fork_and_knife:",
        slack_dnd: true,
        github_busy: false,
        out_of_office: false,
    },
    Status {
        keyword: "pairing",
        slack_text: "In a pairing session",
        slack_emoji: ":tuple:",
        slack_dnd: false,
        github_busy: false,
        out_of_office: false,
    },
    Status {
        keyword: "meeting",
        slack_text: "In a meeting",
        slack_emoji: ":virtual-meeting:",
        slack_dnd: false,
        github_busy: false,
        out_of_office: false,
    },
    Status {
        keyword: "eod",
        slack_text: "Outside work hours",
        slack_emoji: ":mantelpiece_clock:",
        slack_dnd: true,
        github_busy: false,
        out_of_office: false,
    },
    Status {
        keyword: "vacation",
        slack_text: "Vacation",
        slack_emoji: ":desert_island:",
        slack_dnd: true,
        github_busy: true,
        out_of_office: true,
    },
    Status {
        keyword: "sick",
        slack_text: "Out sick",
        slack_emoji: ":face_with_thermometer:",
        slack_dnd: true,
        github_busy: false,
        out_of_office: true,
    },
    Status {
        keyword: "away",
        slack_text: "Out of office",
        slack_emoji: ":no_entry:",
        slack_dnd: true,
        github_busy: true,
        out_of_office: true,
    },
    Status {
        keyword: "back",
        slack_text: "Catching up",
        slack_emoji: ":inbox_tray:",
        slack_dnd: false,
        github_busy: false,
        out_of_office: false,
    },
];

fn find_status(keyword: &str) -> Option<&'static Status> {
    STATUSES.iter().find(|s| s.keyword == keyword)
}

// --- GitHub integration ---

fn github_graphql(token: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
    let resp: serde_json::Value = ureq::post("https://api.github.com/graphql")
        .header("Authorization", &format!("Bearer {token}"))
        .header("User-Agent", "st-cli")
        .send_json(body)?
        .into_body()
        .read_json()?;

    if let Some(errors) = resp.get("errors") {
        anyhow::bail!("GraphQL error: {errors}");
    }

    Ok(resp)
}

fn set_github_status(
    status: &Status,
    back_date: Option<DateTime<Local>>,
    org_id: Option<&str>,
) -> Result<()> {
    let token = std::env::var("GITHUB_PAT").context("GITHUB_PAT not set")?;

    if !status.github_busy {
        return Ok(());
    }

    let mut input = format!(
        "message: \"{}\", emoji: \"{}\", limitedAvailability: true",
        status.slack_text, status.slack_emoji,
    );

    if let Some(dt) = back_date {
        input.push_str(&format!(", expiresAt: \"{}\"", dt.to_utc().format("%Y-%m-%dT%H:%M:%SZ")));
    }

    if let Some(id) = org_id {
        input.push_str(&format!(", organizationId: \"{}\"", id));
    }

    let query = format!(
        "mutation {{ changeUserStatus(input: {{ {input} }}) {{ status {{ message }} }} }}"
    );

    let body = serde_json::json!({ "query": query });
    github_graphql(&token, &body)?;

    Ok(())
}

fn clear_github_status() -> Result<()> {
    let token = std::env::var("GITHUB_PAT").context("GITHUB_PAT not set")?;

    let body: serde_json::Value = serde_json::from_str(
        r#"{"query":"mutation { changeUserStatus(input: {}) { clientMutationId } }"}"#,
    )?;

    github_graphql(&token, &body)?;

    Ok(())
}

// --- Slack integration ---

fn set_slack_status(
    status: &Status,
    back_date: Option<DateTime<Local>>,
    show_back_in_text: bool,
) -> Result<()> {
    let token = std::env::var("SLACK_PAT").context("SLACK_PAT not set")?;

    let text = match (back_date, show_back_in_text) {
        (Some(dt), true) => format!("{}. {}", status.slack_text, format_back_date(dt)),
        _ => status.slack_text.to_string(),
    };

    let expiration = match back_date {
        Some(dt) => dt.timestamp(),
        None => 0,
    };

    let profile = serde_json::json!({
        "profile": {
            "status_text": text,
            "status_emoji": status.slack_emoji,
            "status_expiration": expiration
        }
    });

    let resp: SlackResponse = ureq::post("https://slack.com/api/users.profile.set")
        .header("Authorization", &format!("Bearer {token}"))
        .send_json(&profile)?
        .into_body()
        .read_json()?;

    if !resp.ok {
        anyhow::bail!("Slack users.profile.set: {}", resp.error.unwrap_or_default());
    }

    if status.slack_dnd {
        let minutes = match back_date {
            Some(dt) => {
                let diff = dt.signed_duration_since(Local::now()).num_minutes();
                if diff > 0 { diff } else { 1440 }
            }
            None => 1440,
        };
        set_slack_dnd(&token, minutes)?;
    }

    Ok(())
}

fn set_slack_dnd(token: &str, minutes: i64) -> Result<()> {
    let resp: SlackResponse = ureq::post("https://slack.com/api/dnd.setSnooze")
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send_form([("num_minutes", &minutes.to_string())])?
        .into_body()
        .read_json()?;

    if !resp.ok {
        anyhow::bail!("Slack dnd.setSnooze: {}", resp.error.unwrap_or_default());
    }

    Ok(())
}

fn end_slack_dnd(token: &str) -> Result<()> {
    let resp: SlackResponse = ureq::post("https://slack.com/api/dnd.endSnooze")
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send_form(std::iter::empty::<(&str, &str)>())?
        .into_body()
        .read_json()?;

    // dnd.endSnooze returns ok=false with "snooze_not_active" if DND isn't on, which is fine
    if !resp.ok && resp.error.as_deref() != Some("snooze_not_active") {
        anyhow::bail!("Slack dnd.endSnooze: {}", resp.error.unwrap_or_default());
    }

    Ok(())
}

fn clear_slack_status() -> Result<()> {
    let token = std::env::var("SLACK_PAT").context("SLACK_PAT not set")?;

    let profile = serde_json::json!({
        "profile": {
            "status_text": "",
            "status_emoji": "",
            "status_expiration": 0
        }
    });

    let resp: SlackResponse = ureq::post("https://slack.com/api/users.profile.set")
        .header("Authorization", &format!("Bearer {token}"))
        .send_json(&profile)?
        .into_body()
        .read_json()?;

    if !resp.ok {
        anyhow::bail!("Slack users.profile.set: {}", resp.error.unwrap_or_default());
    }

    end_slack_dnd(&token)?;

    Ok(())
}

#[derive(Deserialize)]
struct SlackResponse {
    ok: bool,
    error: Option<String>,
}

// --- CLI ---

const AFTER_HELP: &str = "\
Keywords:
  lunch     Fork and knife emoji with DND for about an hour, or until a time (st lunch 1:30pm)
  meeting   In a meeting
  pairing   In a pairing session
  eod       Outside work hours with DND until 7am the next work day (Mon-Thu)
  vacation  Vacation with DND, GitHub busy, and a Linear out-of-office reminder
  sick      Out sick with DND and a Linear out-of-office reminder
  away      Out of office with DND, GitHub busy, and a Linear out-of-office reminder
  back      Catching up for 5 minutes, ends DND, and clears GitHub busy
  clear     Clears Slack and GitHub status and ends DND

Examples:
  st lunch              DND until the next quarter hour plus 1 hour
  st lunch 1:30pm       DND until 1:30pm
  st eod                DND until 7am the next work day
  st vacation friday    Vacation until Friday at 7am
  st vacation 3/10 9am  Vacation until March 10 at 9am
  st sick tomorrow      Out sick until tomorrow at 7am
  st back               Clear DND and show Catching up for 5 minutes
  st clear              Clear everything

Dates: friday, mon, tomorrow, 3/10, 3-10-2026, 3/10/26, 2026-03-10
Times: 9am, 1:30pm, 15:00, 3p.m. (defaults to 7am)";

#[derive(Parser)]
#[command(
    name = "st",
    about = "Set your status across Slack and GitHub, with reminders for Linear",
    version,
    arg_required_else_help = true,
    after_help = AFTER_HELP
)]
struct Cli {
    /// Status to set (see Keywords below)
    keyword: String,

    /// When you'll be back (see Dates below)
    back_date: Option<String>,

    /// What time you'll be back (see Times below)
    back_time: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    let config = load_config();
    let keyword = cli.keyword.to_lowercase();
    let is_clear = keyword == "clear";

    if !is_clear && find_status(&keyword).is_none() {
        eprintln!(
            "Unknown keyword: {keyword}\nAvailable: lunch, meeting, pairing, eod, vacation, sick, away, back, clear"
        );
        std::process::exit(1);
    }

    let back_dt = if keyword == "lunch" {
        let time = cli.back_date.as_deref(); // for lunch, second arg is a time
        Some(parse_lunch_back_time(time).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        }))
    } else if keyword == "back" && cli.back_date.is_none() {
        // "Catching up" is a short heads-up, so it expires on its own
        Some(Local::now() + chrono::Duration::minutes(CATCHING_UP_MINUTES))
    } else if keyword == "eod" && cli.back_date.is_none() {
        // Pause until the start of the next work day, so Thursday's eod lasts the weekend
        let time = NaiveTime::from_hms_opt(DEFAULT_BACK_HOUR, 0, 0).unwrap();
        let now = Local::now();
        Some(to_local_datetime(eod_back_date(now.date_naive(), now.time()), time))
    } else {
        cli.back_date.map(|s| {
            parse_back_date(&s, cli.back_time.as_deref()).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1);
            })
        })
    };

    if is_clear {
        run_clear();
    } else {
        let status = find_status(&keyword).unwrap();
        run_set(status, back_dt, &config);
    }
}

fn run_set(status: &Status, back_date: Option<DateTime<Local>>, config: &Config) {
    let is_back = status.keyword == "back";

    // Slack (always runs — "back" clears DND then sets catching-up status)
    if is_back
        && let Ok(token) = std::env::var("SLACK_PAT")
        && let Err(e) = end_slack_dnd(&token)
    {
        eprintln!("  Slack   \u{2717} ending DND: {e}");
    }
    let show_back_in_text = matches!(status.keyword, "vacation" | "sick" | "away");
    match set_slack_status(status, back_date, show_back_in_text) {
        Ok(()) => {
            let text = match (back_date, show_back_in_text) {
                (Some(dt), true) => format!("{}. {}", status.slack_text, format_back_date_with_time(dt)),
                _ => status.slack_text.to_string(),
            };
            let dnd_detail = match (status.slack_dnd, back_date) {
                (true, Some(dt)) => {
                    format!(" (DND until {})", format_until(dt, Local::now().date_naive()))
                }
                (true, None) => " (DND on)".to_string(),
                _ => String::new(),
            };
            let dnd_cleared = if is_back { " (DND off)" } else { "" };
            let label = if text.is_empty() {
                status.slack_emoji.to_string()
            } else {
                format!("{text} {}", status.slack_emoji)
            };
            println!("  Slack   \u{2713} {label}{dnd_detail}{dnd_cleared}");
        }
        Err(e) => eprintln!("  Slack   \u{2717} {e}"),
    }

    // GitHub — set busy, clear busy (for "back"), or no change
    if is_back {
        match clear_github_status() {
            Ok(()) => println!("  GitHub  \u{2713} Cleared"),
            Err(e) => eprintln!("  GitHub  \u{2717} {e}"),
        }
    } else if status.github_busy {
        match set_github_status(status, back_date, config.github_org_id.as_deref()) {
            Ok(()) => {
                let org = match (&config.github_org_id, &config.github_org_name) {
                    (Some(_), Some(name)) => format!(" ({name} only)"),
                    (Some(_), None) => " (org only)".to_string(),
                    _ => String::new(),
                };
                println!("  GitHub  \u{2713} Limited availability{org}");
            }
            Err(e) => eprintln!("  GitHub  \u{2717} {e}"),
        }
    } else {
        println!("  GitHub  - No change");
    }

    // Linear reads OOO from Google Calendar, so remind instead of setting it
    if status.out_of_office {
        println!("  Linear  ! {LINEAR_OOO_REMINDER}");
    } else if is_back {
        println!("  Linear  ! If you're back early, end your Google Calendar out-of-office event");
    } else {
        println!("  Linear  - No change");
    }
}

fn run_clear() {
    match clear_slack_status() {
        Ok(()) => println!("  Slack   \u{2713} Cleared (DND off)"),
        Err(e) => eprintln!("  Slack   \u{2717} {e}"),
    }

    match clear_github_status() {
        Ok(()) => println!("  GitHub  \u{2713} Cleared"),
        Err(e) => eprintln!("  GitHub  \u{2717} {e}"),
    }

    println!("  Linear  - No change");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn time(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn parse_time_defaults_to_back_hour() {
        assert_eq!(parse_time(None).unwrap(), time(DEFAULT_BACK_HOUR, 0));
    }

    #[test]
    fn parse_time_handles_am_pm_formats() {
        assert_eq!(parse_time(Some("9am")).unwrap(), time(9, 0));
        assert_eq!(parse_time(Some("1:30pm")).unwrap(), time(13, 30));
        assert_eq!(parse_time(Some("3p.m.")).unwrap(), time(15, 0));
        assert_eq!(parse_time(Some("12am")).unwrap(), time(0, 0));
        assert_eq!(parse_time(Some("12pm")).unwrap(), time(12, 0));
        assert_eq!(parse_time(Some("9 AM")).unwrap(), time(9, 0));
    }

    #[test]
    fn parse_time_handles_24_hour_format() {
        assert_eq!(parse_time(Some("15:00")).unwrap(), time(15, 0));
    }

    #[test]
    fn parse_time_rejects_invalid_input() {
        assert!(parse_time(Some("25:00")).is_err());
        assert!(parse_time(Some("noon")).is_err());
    }

    #[test]
    fn month_day_is_this_year_when_upcoming() {
        let today = date(2026, 3, 1);
        assert_eq!(parse_date_with_separators("3/10", today), Some(date(2026, 3, 10)));
        assert_eq!(parse_date_with_separators("3-10", today), Some(date(2026, 3, 10)));
    }

    #[test]
    fn month_day_rolls_to_next_year_when_past() {
        let today = date(2026, 10, 30);
        assert_eq!(parse_date_with_separators("3/10", today), Some(date(2027, 3, 10)));
    }

    #[test]
    fn month_day_year_accepts_two_and_four_digit_years() {
        let today = date(2026, 1, 1);
        assert_eq!(parse_date_with_separators("3/10/26", today), Some(date(2026, 3, 10)));
        assert_eq!(parse_date_with_separators("3-10-2026", today), Some(date(2026, 3, 10)));
    }

    #[test]
    fn year_month_day_is_iso_order() {
        let today = date(2026, 1, 1);
        assert_eq!(parse_date_with_separators("2026-03-10", today), Some(date(2026, 3, 10)));
        assert_eq!(parse_date_with_separators("2026/3/10", today), Some(date(2026, 3, 10)));
    }

    #[test]
    fn next_workday_skips_fridays_and_weekends() {
        // 2026-10-26 is a Monday
        assert_eq!(next_workday(date(2026, 10, 26)), date(2026, 10, 27));
        assert_eq!(next_workday(date(2026, 10, 28)), date(2026, 10, 29));
        assert_eq!(next_workday(date(2026, 10, 29)), date(2026, 11, 2));
        assert_eq!(next_workday(date(2026, 10, 30)), date(2026, 11, 2));
        assert_eq!(next_workday(date(2026, 10, 31)), date(2026, 11, 2));
        assert_eq!(next_workday(date(2026, 11, 1)), date(2026, 11, 2));
    }

    #[test]
    fn eod_ends_next_work_morning_in_the_evening() {
        // Wednesday 5pm ends Thursday, Thursday 5pm ends Monday
        assert_eq!(eod_back_date(date(2026, 10, 28), time(17, 0)), date(2026, 10, 29));
        assert_eq!(eod_back_date(date(2026, 10, 29), time(17, 0)), date(2026, 11, 2));
    }

    #[test]
    fn eod_after_midnight_ends_this_morning_on_a_workday() {
        // Thursday 12:30am still ends Thursday 7am, not Monday
        assert_eq!(eod_back_date(date(2026, 10, 29), time(0, 30)), date(2026, 10, 29));
        assert_eq!(eod_back_date(date(2026, 10, 26), time(6, 59)), date(2026, 10, 26));
        // Friday 12:30am is a day off, so it rolls to Monday
        assert_eq!(eod_back_date(date(2026, 10, 30), time(0, 30)), date(2026, 11, 2));
    }

    #[test]
    fn lunch_time_without_am_pm_picks_the_next_one() {
        let morning = time(9, 20);
        assert_eq!(parse_lunch_time("1", morning).unwrap(), time(13, 0));
        assert_eq!(parse_lunch_time("12:30", morning).unwrap(), time(12, 30));
        assert_eq!(parse_lunch_time("11", morning).unwrap(), time(11, 0));
        assert_eq!(parse_lunch_time("9:15", morning).unwrap(), time(21, 15));
    }

    #[test]
    fn lunch_time_with_am_pm_or_24_hour_is_kept() {
        let morning = time(9, 20);
        assert_eq!(parse_lunch_time("1am", morning).unwrap(), time(1, 0));
        assert_eq!(parse_lunch_time("1pm", morning).unwrap(), time(13, 0));
        assert_eq!(parse_lunch_time("13:00", morning).unwrap(), time(13, 0));
    }

    #[test]
    fn dnd_until_includes_weekday_when_not_today() {
        let friday = date(2026, 10, 30);
        let today_1pm = to_local_datetime(friday, time(13, 0));
        let monday_7am = to_local_datetime(date(2026, 11, 2), time(7, 0));
        assert_eq!(format_until(today_1pm, friday), "1pm");
        assert_eq!(format_until(monday_7am, friday), "Monday 7am");
    }

    #[test]
    fn invalid_dates_return_none() {
        let today = date(2026, 1, 1);
        assert_eq!(parse_date_with_separators("13/1", today), None);
        assert_eq!(parse_date_with_separators("friday", today), None);
    }
}
