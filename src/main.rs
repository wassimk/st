use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveTime, Timelike, Weekday};
use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

const DEFAULT_BACK_HOUR: u32 = 7;

// --- Config ---

#[derive(Deserialize)]
struct Config {
    github_org_id: Option<String>,
    asana_user_gid: Option<String>,
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
            Config {
                github_org_id: None,
                asana_user_gid: None,
            }
        }),
        Err(_) => Config {
            github_org_id: None,
            asana_user_gid: None,
        },
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

fn parse_lunch_back_time(input: Option<&str>) -> Result<DateTime<Local>> {
    let today = Local::now().date_naive();
    let time = match input {
        Some(s) => parse_time(Some(s))?,
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
    #[allow(dead_code)]
    asana_dnd: bool, // Asana API doesn't support setting OOO yet
}

const STATUSES: &[Status] = &[
    Status {
        keyword: "lunch",
        slack_text: "Lunchin'",
        slack_emoji: ":fork_and_knife:",
        slack_dnd: true,
        github_busy: false,
        asana_dnd: false,
    },
    Status {
        keyword: "zoom",
        slack_text: "In a meeting (Zoom)",
        slack_emoji: ":video_camera:",
        slack_dnd: false,
        github_busy: false,
        asana_dnd: false,
    },
    Status {
        keyword: "tuple",
        slack_text: "Pairing (Tuple)",
        slack_emoji: ":couple:",
        slack_dnd: false,
        github_busy: false,
        asana_dnd: false,
    },
    Status {
        keyword: "meet",
        slack_text: "In a meeting",
        slack_emoji: ":calendar:",
        slack_dnd: false,
        github_busy: false,
        asana_dnd: false,
    },
    Status {
        keyword: "eod",
        slack_text: "Done for the day",
        slack_emoji: ":wave:",
        slack_dnd: true,
        github_busy: false,
        asana_dnd: true,
    },
    Status {
        keyword: "vacation",
        slack_text: "Vacation",
        slack_emoji: ":desert_island:",
        slack_dnd: true,
        github_busy: true,
        asana_dnd: true,
    },
    Status {
        keyword: "sick",
        slack_text: "Out sick",
        slack_emoji: ":face_with_thermometer:",
        slack_dnd: true,
        github_busy: false,
        asana_dnd: true,
    },
    Status {
        keyword: "away",
        slack_text: "Out of office",
        slack_emoji: ":no_entry:",
        slack_dnd: true,
        github_busy: true,
        asana_dnd: true,
    },
    Status {
        keyword: "back",
        slack_text: "Catching up",
        slack_emoji: ":inbox_tray:",
        slack_dnd: false,
        github_busy: false,
        asana_dnd: false,
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

// --- Asana (no API for setting OOO — can only read vacation_dates) ---

#[derive(Deserialize)]
struct AsanaResponse {
    data: Vec<AsanaWorkspaceMembership>,
}

#[derive(Deserialize)]
struct AsanaWorkspaceMembership {
    vacation_dates: Option<AsanaVacationDates>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct AsanaVacationDates {
    start_on: Option<String>,
    end_on: Option<String>,
}

fn asana_ooo_is_set(config: &Config) -> Result<bool> {
    let token = std::env::var("ASANA_PAT").context("ASANA_PAT not set")?;
    let user_gid = config
        .asana_user_gid
        .as_deref()
        .context("asana_user_gid not set in config")?;

    let url = format!(
        "https://app.asana.com/api/1.0/users/{user_gid}/workspace_memberships?opt_fields=vacation_dates"
    );

    let resp: AsanaResponse = ureq::get(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()?
        .into_body()
        .read_json()?;

    Ok(resp.data.iter().any(|m| m.vacation_dates.is_some()))
}

fn asana_ooo_summary(config: &Config) -> Option<String> {
    match asana_ooo_is_set(config) {
        Ok(true) => Some("OOO is set".into()),
        Ok(false) => None,
        Err(_) => None,
    }
}

// --- CLI ---

#[derive(Parser)]
#[command(name = "st", about = "Set your status across services", version)]
struct Cli {
    /// Status keyword: lunch, zoom, tuple, meet, eod, vacation, sick, away, back, clear
    keyword: String,

    /// Back date: when you'll return (e.g., friday, 2/28, 2026-03-10, tomorrow)
    back_date: Option<String>,

    /// Back time: what time you'll return (e.g., 8am, 9:30am, 15:00). Defaults to 7am.
    back_time: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    let config = load_config();
    let keyword = cli.keyword.to_lowercase();
    let is_clear = keyword == "clear";

    if !is_clear && find_status(&keyword).is_none() {
        eprintln!(
            "Unknown keyword: {keyword}\nAvailable: lunch, zoom, tuple, meet, eod, vacation, sick, away, back, clear"
        );
        std::process::exit(1);
    }

    let back_dt = if keyword == "lunch" {
        let time = cli.back_date.as_deref(); // for lunch, second arg is a time
        Some(parse_lunch_back_time(time).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        }))
    } else {
        cli.back_date.map(|s| {
            parse_back_date(&s, cli.back_time.as_deref()).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1);
            })
        })
    };

    if is_clear {
        run_clear(&config);
    } else {
        let status = find_status(&keyword).unwrap();
        run_set(status, back_dt, &config);
    }
}

fn run_set(status: &Status, back_date: Option<DateTime<Local>>, config: &Config) {
    let is_back = status.keyword == "back";

    // Slack (always runs — "back" clears DND then sets catching-up status)
    if is_back {
        if let Ok(token) = std::env::var("SLACK_PAT") {
            if let Err(e) = end_slack_dnd(&token) {
                eprintln!("  Slack   \u{2717} ending DND: {e}");
            }
        }
    }
    let show_back_in_text = matches!(status.keyword, "vacation" | "sick" | "away");
    match set_slack_status(status, back_date, show_back_in_text) {
        Ok(()) => {
            let text = match (back_date, show_back_in_text) {
                (Some(dt), true) => format!("{}. {}", status.slack_text, format_back_date_with_time(dt)),
                _ => status.slack_text.to_string(),
            };
            let dnd_detail = match (status.slack_dnd, back_date) {
                (true, Some(dt)) => format!(" (DND until {})", format_time(dt)),
                (true, None) => " (DND on)".to_string(),
                _ => String::new(),
            };
            let dnd_cleared = if is_back { " (DND off)" } else { "" };
            println!("  Slack   \u{2713} {} {}{}{}", text, status.slack_emoji, dnd_detail, dnd_cleared);
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
                let org = if config.github_org_id.is_some() {
                    " (Planning Center only)"
                } else {
                    ""
                };
                println!("  GitHub  \u{2713} Limited availability{org}");
            }
            Err(e) => eprintln!("  GitHub  \u{2717} {e}"),
        }
    } else {
        println!("  GitHub  - No change");
    }

    // Asana (no API for setting OOO — remind when relevant)
    if status.keyword == "vacation" || status.keyword == "away" || status.keyword == "sick" {
        if asana_ooo_summary(config).is_none() {
            println!("  Asana   ! Set Out of Office manually: Profile (icon) > Set out of office");
        } else {
            println!("  Asana   \u{2713} Out of Office already set");
        }
    } else if is_back {
        if asana_ooo_summary(config).is_some() {
            println!("  Asana   ! Clear Out of Office manually: Profile (icon) > Set out of office");
        } else {
            println!("  Asana   - No change");
        }
    } else {
        println!("  Asana   - No change");
    }
}

fn run_clear(config: &Config) {
    match clear_slack_status() {
        Ok(()) => println!("  Slack   \u{2713} Cleared (DND off)"),
        Err(e) => eprintln!("  Slack   \u{2717} {e}"),
    }

    match clear_github_status() {
        Ok(()) => println!("  GitHub  \u{2713} Cleared"),
        Err(e) => eprintln!("  GitHub  \u{2717} {e}"),
    }

    if asana_ooo_summary(config).is_some() {
        println!("  Asana   ! Clear Out of Office manually: Profile (icon) > Set out of office");
    } else {
        println!("  Asana   - No change");
    }
}
