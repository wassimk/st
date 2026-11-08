mod focus;

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveTime, Timelike, Weekday};
use clap::Parser;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

const DEFAULT_BACK_HOUR: u32 = 7;
const END_OF_DAY_HOUR: u32 = 17;
const CATCHING_UP_MINUTES: i64 = 15;
const LINEAR_OOO_REMINDER: &str =
    "Add an out-of-office event in Google Calendar (Linear shows you as Away)";

// --- Config ---

#[derive(Deserialize, Default)]
struct Config {
    github_org_id: Option<String>,
    github_org_name: Option<String>,
    #[serde(default)]
    keywords: BTreeMap<String, KeywordConfig>,
}

// A [keywords.NAME] table. For a built-in keyword, emoji and text replace its defaults. Any other
// name adds a keyword, which needs a base: the built-in keyword whose behavior it copies.
#[derive(Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)]
struct KeywordConfig {
    base: Option<String>,
    emoji: Option<String>,
    text: Option<String>,
}

fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("st")
        .join("config.toml")
}

// A config that doesn't parse stops st, since falling back to the defaults would also drop
// github_org_id and set GitHub busy for every org
fn load_config() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_else(|e| {
            eprintln!("Error in {}: {e}", path.display());
            std::process::exit(1);
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

    NaiveTime::from_hms_opt(hour, minute, 0).ok_or_else(|| anyhow::anyhow!("Invalid time: {input}"))
}

fn to_local_datetime(date: NaiveDate, time: NaiveTime) -> DateTime<Local> {
    date.and_time(time).and_local_timezone(Local).unwrap()
}

fn format_time(dt: DateTime<Local>) -> String {
    let hour = dt
        .format("%I")
        .to_string()
        .trim_start_matches('0')
        .to_string();
    let minute = dt.minute();
    let ampm = dt.format("%p").to_string().to_lowercase();

    if minute == 0 {
        format!("{}{}", hour, ampm)
    } else {
        format!("{}:{:02}{}", hour, minute, ampm)
    }
}

fn format_until(dt: DateTime<Local>, today: NaiveDate) -> String {
    let date = dt.date_naive();
    let days_away = (date - today).num_days();

    if days_away == 0 {
        format_time(dt)
    } else if days_away < 7 {
        format!("{} {}", dt.format("%A"), format_time(dt))
    } else {
        format!(
            "{} {}/{} {}",
            dt.format("%a"),
            date.month(),
            date.day(),
            format_time(dt)
        )
    }
}

// Four-day work week: Monday through Thursday
fn is_workday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Fri | Weekday::Sat | Weekday::Sun)
}

// Being back on a day off means being back the next work day, at the same time
fn roll_to_workday(dt: DateTime<Local>) -> DateTime<Local> {
    let mut date = dt.date_naive();
    while !is_workday(date) {
        date += chrono::Duration::days(1);
    }
    to_local_datetime(date, dt.time())
}

// Away with no date lasts the rest of the work day, or all of the next one after hours
fn away_back_date(today: NaiveDate, now: NaiveTime) -> NaiveDate {
    if is_workday(today) && now.hour() < END_OF_DAY_HOUR {
        today
    } else {
        next_workday(today)
    }
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

// Without am/pm, "1" means whichever 1 o'clock comes next, so "lunch 1" is 1pm
fn parse_today_time(input: &str, now: NaiveTime) -> Result<NaiveTime> {
    let time = parse_time(Some(input))?;
    let has_meridiem = input.chars().any(|c| c.is_ascii_alphabetic());
    if !has_meridiem && time.hour() < 12 && time <= now {
        return Ok(time + chrono::Duration::hours(12));
    }
    Ok(time)
}

fn parse_today_back_time(input: Option<&str>) -> Result<DateTime<Local>> {
    let today = Local::now().date_naive();
    let time = match input {
        Some(s) => parse_today_time(s, Local::now().time())?,
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

#[derive(Debug, PartialEq)]
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
        slack_emoji: ":pair:",
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
        slack_text: "Off recharging",
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
        github_busy: true,
        out_of_office: true,
    },
    Status {
        keyword: "away",
        slack_text: "Stepped away for a bit",
        slack_emoji: ":car:",
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

// A keyword as it's used at runtime. The emoji and text are what gets set, and everything else
// (DND, GitHub busy, default end, date rules) comes from the built-in status it's based on.
#[derive(Debug, Clone, PartialEq)]
struct Keyword {
    name: String,
    base: &'static Status,
    emoji: String,
    text: String,
}

impl Keyword {
    fn from_status(status: &'static Status) -> Self {
        Keyword {
            name: status.keyword.to_string(),
            base: status,
            emoji: status.slack_emoji.to_string(),
            text: status.slack_text.to_string(),
        }
    }

    // The built-in keyword whose behavior this one follows, which is itself for a built-in
    fn behavior(&self) -> &'static str {
        self.base.keyword
    }

    fn is_custom(&self) -> bool {
        self.name != self.base.keyword
    }
}

fn builtin_keywords() -> Vec<Keyword> {
    STATUSES.iter().map(Keyword::from_status).collect()
}

// Accepts "car" or ":car:" and returns ":car:". Slack emoji names are letters, digits, and
// - _ + ' only, which also keeps a config value from breaking the GitHub query.
fn normalize_emoji(input: &str) -> Result<String, String> {
    let name = input.trim().trim_matches(':');
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '\''));

    if valid {
        Ok(format!(":{name}:"))
    } else {
        Err(format!(
            "\"{input}\" isn't a Slack emoji name, like :car: or car"
        ))
    }
}

// Words st reads itself, which a config keyword can't take over
const RESERVED_NAMES: &[&str] = &["clear", "keywords", "setup-focus"];

fn is_builtin(name: &str) -> bool {
    STATUSES.iter().any(|s| s.keyword == name)
}

fn builtin_names() -> String {
    STATUSES
        .iter()
        .map(|s| s.keyword)
        .collect::<Vec<_>>()
        .join(", ")
}

fn apply_overrides(keyword: &mut Keyword, entry: &KeywordConfig) -> Result<(), String> {
    if let Some(emoji) = &entry.emoji {
        keyword.emoji = normalize_emoji(emoji).map_err(|e| format!("emoji: {e}"))?;
    }
    if let Some(text) = &entry.text {
        keyword.text = text.clone();
    }
    Ok(())
}

// Builds a custom keyword from its base, after the base's own overrides, so a sabbatical
// based on vacation starts with your vacation emoji and text until it sets its own
fn custom_keyword(
    name: &str,
    entry: &KeywordConfig,
    keywords: &[Keyword],
) -> Result<Keyword, String> {
    let valid_name = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'));
    if !valid_name {
        return Err("names can only use letters, digits, - and _".to_string());
    }

    let Some(base_name) = &entry.base else {
        return Err(format!(
            "needs a base, the built-in keyword it acts like ({})",
            builtin_names()
        ));
    };
    let base_name = base_name.to_lowercase();
    let Some(base) = keywords
        .iter()
        .find(|k| k.name == base_name && !k.is_custom())
    else {
        return Err(format!(
            "base \"{base_name}\" isn't a built-in keyword ({})",
            builtin_names()
        ));
    };

    let mut keyword = Keyword {
        name: name.to_string(),
        ..base.clone()
    };
    apply_overrides(&mut keyword, entry)?;
    Ok(keyword)
}

// Applies [keywords.*] from the config to the built-in keywords, then adds the custom ones.
// Returns the keywords and any warnings, or an error for a config entry that can't be used.
fn resolve_keywords(
    config: &BTreeMap<String, KeywordConfig>,
) -> Result<(Vec<Keyword>, Vec<String>), String> {
    let mut keywords = builtin_keywords();
    let entries: Vec<(String, &KeywordConfig)> = config
        .iter()
        .map(|(name, entry)| (name.to_lowercase(), entry))
        .collect();

    for (name, entry) in &entries {
        if RESERVED_NAMES.contains(&name.as_str()) {
            return Err(format!("[keywords.{name}] {name} can't be changed"));
        }
        if !is_builtin(name) {
            continue;
        }
        if entry.base.is_some() {
            return Err(format!(
                "[keywords.{name}] {name} is built in, so it can't have a base"
            ));
        }
        let keyword = keywords.iter_mut().find(|k| &k.name == name).unwrap();
        apply_overrides(keyword, entry).map_err(|e| format!("[keywords.{name}] {e}"))?;
    }

    for (name, entry) in &entries {
        if is_builtin(name) {
            continue;
        }
        let keyword =
            custom_keyword(name, entry, &keywords).map_err(|e| format!("[keywords.{name}] {e}"))?;
        keywords.push(keyword);
    }

    let warnings = duplicate_emoji_warnings(&keywords);
    Ok((keywords, warnings))
}

// st clear --if tells keywords apart by emoji, so two keywords sharing one can't be told apart
fn duplicate_emoji_warnings(keywords: &[Keyword]) -> Vec<String> {
    let mut by_emoji: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for k in keywords {
        by_emoji.entry(&k.emoji).or_default().push(&k.name);
    }

    by_emoji
        .into_iter()
        .filter(|(_, names)| names.len() > 1)
        .map(|(emoji, names)| {
            format!(
                "{} share {emoji}, so st clear --if can't tell them apart",
                names.join(" and ")
            )
        })
        .collect()
}

fn find_keyword<'a>(keywords: &'a [Keyword], name: &str) -> Option<&'a Keyword> {
    keywords.iter().find(|k| k.name == name)
}

fn keyword_names(keywords: &[Keyword]) -> String {
    keywords
        .iter()
        .map(|k| k.name.as_str())
        .chain(["clear"])
        .collect::<Vec<_>>()
        .join(", ")
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

// Returns whether the emoji was set. GitHub only has standard emoji, and for one it doesn't
// know, like a custom Slack :ooo:, it saves nothing and returns a null status with no error.
// The status is then set again without the emoji.
fn set_github_status(
    keyword: &Keyword,
    text: &str,
    back_date: Option<DateTime<Local>>,
    org_id: Option<&str>,
) -> Result<bool> {
    let token = std::env::var("GITHUB_PAT").context("GITHUB_PAT not set")?;

    let with_emoji = github_status_query(text, Some(&keyword.emoji), back_date, org_id)?;
    let resp = github_graphql(&token, &serde_json::json!({ "query": with_emoji }))?;
    if status_saved(&resp) {
        return Ok(true);
    }

    let without_emoji = github_status_query(text, None, back_date, org_id)?;
    let resp = github_graphql(&token, &serde_json::json!({ "query": without_emoji }))?;
    if status_saved(&resp) {
        Ok(false)
    } else {
        anyhow::bail!("GitHub didn't save the status")
    }
}

fn status_saved(resp: &serde_json::Value) -> bool {
    !resp["data"]["changeUserStatus"]["status"].is_null()
}

fn github_status_query(
    text: &str,
    emoji: Option<&str>,
    back_date: Option<DateTime<Local>>,
    org_id: Option<&str>,
) -> Result<String> {
    // A JSON string is also a valid GraphQL string, so quotes in a custom message are escaped
    let mut input = format!(
        "message: {}, limitedAvailability: true",
        serde_json::to_string(text)?
    );

    if let Some(emoji) = emoji {
        input.push_str(&format!(", emoji: {}", serde_json::to_string(emoji)?));
    }

    if let Some(dt) = back_date {
        input.push_str(&format!(
            ", expiresAt: \"{}\"",
            dt.to_utc().format("%Y-%m-%dT%H:%M:%SZ")
        ));
    }

    if let Some(id) = org_id {
        input.push_str(&format!(", organizationId: \"{}\"", id));
    }

    Ok(format!(
        "mutation {{ changeUserStatus(input: {{ {input} }}) {{ status {{ message }} }} }}"
    ))
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

// Slack shows the expiration under the status, so the text doesn't repeat it
fn set_slack_status(
    keyword: &Keyword,
    text: &str,
    back_date: Option<DateTime<Local>>,
) -> Result<()> {
    let token = std::env::var("SLACK_PAT").context("SLACK_PAT not set")?;

    let expiration = match back_date {
        Some(dt) => dt.timestamp(),
        None => 0,
    };

    let profile = serde_json::json!({
        "profile": {
            "status_text": text,
            "status_emoji": keyword.emoji,
            "status_expiration": expiration
        }
    });

    let resp: SlackResponse = ureq::post("https://slack.com/api/users.profile.set")
        .header("Authorization", &format!("Bearer {token}"))
        .send_json(&profile)?
        .into_body()
        .read_json()?;

    if !resp.ok {
        anyhow::bail!(
            "Slack users.profile.set: {}",
            resp.error.unwrap_or_default()
        );
    }

    if keyword.base.slack_dnd {
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

// "auto" hands presence back to Slack, which shows you active while you're using it
fn set_slack_presence_auto() -> Result<()> {
    let token = std::env::var("SLACK_PAT").context("SLACK_PAT not set")?;

    let resp: SlackResponse = ureq::post("https://slack.com/api/users.setPresence")
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send_form([("presence", "auto")])?
        .into_body()
        .read_json()?;

    if !resp.ok {
        anyhow::bail!(
            "Slack users.setPresence: {}",
            resp.error.unwrap_or_default()
        );
    }

    Ok(())
}

fn clear_slack_status() -> Result<()> {
    let token = std::env::var("SLACK_PAT").context("SLACK_PAT not set")?;
    set_slack_profile_status("", "")?;
    end_slack_dnd(&token)?;
    Ok(())
}

fn set_slack_profile_status(text: &str, emoji: &str) -> Result<()> {
    let token = std::env::var("SLACK_PAT").context("SLACK_PAT not set")?;

    let profile = serde_json::json!({
        "profile": {
            "status_text": text,
            "status_emoji": emoji,
            "status_expiration": 0
        }
    });

    let resp: SlackResponse = ureq::post("https://slack.com/api/users.profile.set")
        .header("Authorization", &format!("Bearer {token}"))
        .send_json(&profile)?
        .into_body()
        .read_json()?;

    if !resp.ok {
        anyhow::bail!(
            "Slack users.profile.set: {}",
            resp.error.unwrap_or_default()
        );
    }

    Ok(())
}

// Only the emoji is compared, since custom messages change the status text
fn should_clear(current_emoji: &str, set_by_emoji: &str) -> bool {
    current_emoji == set_by_emoji
}

fn skip_message(current_emoji: &str, set_by_emoji: &str) -> String {
    if current_emoji.is_empty() {
        "Skipped, you have no status".to_string()
    } else {
        format!("Skipped, status is {current_emoji}, not {set_by_emoji}")
    }
}

// --if only makes sense with clear, and names the status that's allowed to be cleared
fn clear_if_target<'a>(
    keywords: &'a [Keyword],
    keyword: &str,
    if_keyword: Option<&str>,
) -> Result<Option<&'a Keyword>, String> {
    let Some(if_keyword) = if_keyword else {
        return Ok(None);
    };

    if keyword != "clear" {
        return Err("--if only works with clear (st clear --if meeting)".to_string());
    }

    find_keyword(keywords, &if_keyword.to_lowercase())
        .map(Some)
        .ok_or_else(|| format!("Unknown keyword for --if: {if_keyword}"))
}

// Needs the users.profile:read scope on top of the scopes for setting status
fn get_slack_status_emoji() -> Result<String> {
    let token = std::env::var("SLACK_PAT").context("SLACK_PAT not set")?;

    let resp: serde_json::Value = ureq::get("https://slack.com/api/users.profile.get")
        .header("Authorization", &format!("Bearer {token}"))
        .call()?
        .into_body()
        .read_json()?;

    if resp["ok"] != true {
        let error = resp["error"].as_str().unwrap_or_default();
        if error == "missing_scope" {
            anyhow::bail!(
                "Slack users.profile.get: missing_scope (add the users.profile:read scope to your Slack app)"
            );
        }
        anyhow::bail!("Slack users.profile.get: {error}");
    }

    Ok(resp["profile"]["status_emoji"]
        .as_str()
        .unwrap_or_default()
        .to_string())
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
  meeting   In a meeting, optionally until a time (st meeting 3pm)
  pairing   In a pairing session, optionally until a time (st pairing 3pm)
  eod       Outside work hours with DND until 7am the next work day (Mon-Thu)
  vacation  Off recharging with DND, GitHub busy, and a Linear out-of-office reminder
  sick      Out sick with DND, GitHub busy, and a Linear out-of-office reminder
  away      Stepped away for a bit until 5pm (or a time you give) with DND, GitHub busy, and a
            Linear out-of-office reminder
  back      Catching up for 15 minutes, ends DND, and clears GitHub busy
  clear     Clears Slack and GitHub status, ends DND, and sets you active
  clear --if KEYWORD
            Clears the Slack status, ends DND, and sets you active, but only if the
            status emoji is still KEYWORD's (needs the users.profile:read scope)
  keywords  Lists every keyword with its emoji and text, including your own (--json for JSON)

Change a keyword's emoji or text, or add your own, in ~/.config/st/config.toml:
  [keywords.meeting]
  emoji = \":calendar:\"
  [keywords.sabbatical]
  base = \"vacation\"
  text = \"Sabbatical\"

Examples:
  st lunch              DND until the next quarter hour plus 1 hour
  st lunch 1:30pm       DND until 1:30pm
  st eod                DND until 7am the next work day
  st meeting Standup    In a meeting, with Standup as the status text
  st meeting 3pm        In a meeting until 3pm
  st vacation friday    Off until Monday at 7am, since Fridays are off
  st vacation 3/10 9am  Vacation until March 10 at 9am
  st vacation 3/10 9am At the beach
                        Same, with a custom message
  st sick tomorrow      Out sick until tomorrow at 7am
  st back               Clear DND and show Catching up for 15 minutes
  st clear              Clear everything
  st clear --if meeting Clear the meeting status, leaving any other status alone

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

    /// When you'll be back (see Dates below). Anything else starts a custom message
    back_date: Option<String>,

    /// What time you'll be back (see Times below), then an optional custom message
    #[arg(value_name = "BACK_TIME] [MESSAGE", trailing_var_arg = true)]
    extra: Vec<String>,

    /// With clear, only clear the Slack status if its emoji matches this keyword's
    #[arg(long = "if", value_name = "KEYWORD")]
    if_keyword: Option<String>,

    /// With keywords, print them as JSON
    #[arg(long)]
    json: bool,

    /// Also turn macOS Do Not Disturb on (with a keyword) or off (with clear)
    #[arg(long)]
    focus: bool,
}

// The word after the date is a time only if it parses as one; the rest is the message
fn split_time_and_message(extra: &[String]) -> (Option<String>, Option<String>) {
    let (time, words) = match extra.first() {
        Some(first) if parse_time(Some(first)).is_ok() => (Some(first.clone()), &extra[1..]),
        _ => (None, extra),
    };
    let message = (!words.is_empty()).then(|| words.join(" "));
    (time, message)
}

#[derive(Debug, PartialEq)]
struct Args {
    date: Option<String>,
    time: Option<String>,
    message: Option<String>,
}

// Words after the keyword are [date] [time] [message], each optional. A first word that
// isn't a date starts the message, except for vacation and sick, which have no default end,
// so a typo in the date should be an error rather than a status that never ends.
fn parse_args(keyword: &str, words: &[String]) -> Args {
    let date_required = matches!(keyword, "vacation" | "sick");

    if keyword == "lunch" {
        let (time, message) = split_time_and_message(words);
        return Args {
            date: None,
            time,
            message,
        };
    }

    let takes_bare_time = matches!(keyword, "meeting" | "pairing" | "away");

    match words.first() {
        Some(first) if parse_back_date(first, None).is_ok() => {
            let (time, message) = split_time_and_message(&words[1..]);
            Args {
                date: Some(first.clone()),
                time,
                message,
            }
        }
        Some(first) if takes_bare_time && parse_time(Some(first)).is_ok() => {
            let (time, message) = split_time_and_message(words);
            Args {
                date: None,
                time,
                message,
            }
        }
        Some(first) if date_required => {
            let (time, message) = split_time_and_message(&words[1..]);
            Args {
                date: Some(first.clone()),
                time,
                message,
            }
        }
        _ => Args {
            date: None,
            time: None,
            message: (!words.is_empty()).then(|| words.join(" ")),
        },
    }
}

fn main() {
    let cli = Cli::parse();
    let config = load_config();
    let (keywords, warnings) = resolve_keywords(&config.keywords).unwrap_or_else(|e| {
        eprintln!("Error in {}: {e}", config_path().display());
        std::process::exit(1);
    });
    for warning in warnings {
        eprintln!("Warning: {warning}");
    }
    let name = cli.keyword.to_lowercase();

    if cli.json && name != "keywords" {
        eprintln!("--json only works with keywords (st keywords --json)");
        std::process::exit(1);
    }
    if cli.focus && name == "keywords" {
        eprintln!("--focus doesn't work with keywords");
        std::process::exit(1);
    }
    if name == "keywords" {
        if cli.json {
            println!("{:#}", keywords_json(&keywords));
        } else {
            print!("{}", keywords_table(&keywords));
        }
        return;
    }
    if name == "setup-focus" {
        let open = |path: &std::path::Path| {
            std::process::Command::new("/usr/bin/open")
                .arg(path)
                .status()
                .map_err(|e| format!("Couldn't open {}: {e}", path.display()))
                .and_then(|status| {
                    status
                        .success()
                        .then_some(())
                        .ok_or_else(|| format!("open {} failed with {status}", path.display()))
                })
        };
        let wait = std::time::Duration::from_secs(120);
        let poll = std::time::Duration::from_secs(1);
        match focus::setup(&focus::ShellShortcuts::default(), open, wait, poll) {
            Ok(message) => println!("{message}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    let is_clear = name == "clear";
    let selected = find_keyword(&keywords, &name);

    if !is_clear && selected.is_none() {
        eprintln!(
            "Unknown keyword: {name}\nAvailable: {}",
            keyword_names(&keywords)
        );
        std::process::exit(1);
    }

    let clear_if =
        clear_if_target(&keywords, &name, cli.if_keyword.as_deref()).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        });

    // Date and time rules follow the built-in keyword, so a custom one acts like its base
    let keyword = selected.map_or("clear", Keyword::behavior);
    let out_of_office = selected.is_some_and(|k| k.base.out_of_office);
    let words: Vec<String> = cli.back_date.into_iter().chain(cli.extra).collect();
    let args = parse_args(keyword, &words);

    // A time with no date means later today, read the same way as a lunch time
    let back_dt = if keyword == "lunch" || (args.date.is_none() && args.time.is_some()) {
        Some(
            parse_today_back_time(args.time.as_deref()).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1);
            }),
        )
    } else if keyword == "back" && args.date.is_none() {
        // "Catching up" is a short heads-up, so it expires on its own
        Some(Local::now() + chrono::Duration::minutes(CATCHING_UP_MINUTES))
    } else if keyword == "eod" && args.date.is_none() {
        // Pause until the start of the next work day, so Thursday's eod lasts the weekend
        let time = NaiveTime::from_hms_opt(DEFAULT_BACK_HOUR, 0, 0).unwrap();
        let now = Local::now();
        Some(to_local_datetime(
            eod_back_date(now.date_naive(), now.time()),
            time,
        ))
    } else if keyword == "away" && args.date.is_none() {
        let time = NaiveTime::from_hms_opt(END_OF_DAY_HOUR, 0, 0).unwrap();
        let now = Local::now();
        Some(to_local_datetime(
            away_back_date(now.date_naive(), now.time()),
            time,
        ))
    } else {
        args.date.as_deref().map(|s| {
            let dt = parse_back_date(s, args.time.as_deref()).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1);
            });
            if out_of_office {
                roll_to_workday(dt)
            } else {
                dt
            }
        })
    };

    let request = if let Some(set_by) = clear_if {
        clear_if_focus(&run_clear_if(set_by))
    } else if is_clear {
        run_clear();
        Some(focus_request(None, None))
    } else if let Some(selected) = selected {
        let text = args.message.as_deref().unwrap_or(&selected.text);
        run_set(selected, text, back_dt, &config);
        Some(focus_request(Some(selected), back_dt))
    } else {
        None
    };

    // Runs last so a slow or failing Focus call never delays Slack, GitHub, or Linear
    if cli.focus {
        let outcome = match request {
            Some(request) => {
                focus::apply(&focus::ShellShortcuts::default(), &request, Local::now())
            }
            None => focus::FocusOutcome::Skipped("Skipped, Slack status wasn't cleared".into()),
        };
        match focus_line(&outcome, Local::now().date_naive()) {
            (true, line) => eprintln!("{line}"),
            (false, line) => println!("{line}"),
        }
    }
}

// Focus follows the status: on until it ends, and off for back and clear
fn focus_request(
    keyword: Option<&Keyword>,
    back_dt: Option<DateTime<Local>>,
) -> focus::FocusRequest {
    match (keyword, back_dt) {
        (None, _) => focus::FocusRequest::Off,
        (Some(k), _) if k.behavior() == "back" => focus::FocusRequest::Off,
        (Some(_), Some(end)) => focus::FocusRequest::Until(end),
        (Some(_), None) => focus::FocusRequest::On,
    }
}

#[derive(Debug, PartialEq)]
enum ClearIf {
    Cleared,
    NoStatus,
    OtherStatus,
    ReadFailed,
}

// A status the user set afterward, like lunch, keeps Focus on. With no status, nothing wants quiet.
fn clear_if_focus(result: &ClearIf) -> Option<focus::FocusRequest> {
    match result {
        ClearIf::Cleared | ClearIf::NoStatus => Some(focus::FocusRequest::Off),
        ClearIf::OtherStatus | ClearIf::ReadFailed => None,
    }
}

fn focus_line(outcome: &focus::FocusOutcome, today: NaiveDate) -> (bool, String) {
    use focus::FocusOutcome;
    match outcome {
        FocusOutcome::On => (false, "  Focus   \u{2713} Do Not Disturb on".to_string()),
        FocusOutcome::OnUntil(end) => (
            false,
            format!(
                "  Focus   \u{2713} Do Not Disturb on until {}",
                format_until(*end, today)
            ),
        ),
        FocusOutcome::Off => (false, "  Focus   \u{2713} Do Not Disturb off".to_string()),
        FocusOutcome::Skipped(reason) => (false, format!("  Focus   - {reason}")),
        FocusOutcome::Failed(reason) => (true, format!("  Focus   \u{2717} {reason}")),
    }
}

// base is null for a built-in keyword, and names the built-in one a custom keyword acts like
fn keywords_json(keywords: &[Keyword]) -> serde_json::Value {
    keywords
        .iter()
        .map(|k| {
            serde_json::json!({
                "keyword": k.name,
                "base": k.is_custom().then_some(k.behavior()),
                "emoji": k.emoji,
                "text": k.text,
                "dnd": k.base.slack_dnd,
                "github_busy": k.base.github_busy,
                "out_of_office": k.base.out_of_office,
            })
        })
        .collect()
}

fn keywords_table(keywords: &[Keyword]) -> String {
    let name_width = keywords.iter().map(|k| k.name.len()).max().unwrap_or(0);
    let emoji_width = keywords.iter().map(|k| k.emoji.len()).max().unwrap_or(0);

    keywords
        .iter()
        .map(|k| {
            let text = if k.text.is_empty() {
                "(emoji only)"
            } else {
                &k.text
            };
            let like = if k.is_custom() {
                format!(" (like {})", k.behavior())
            } else {
                String::new()
            };
            format!(
                "{:name_width$}  {:emoji_width$}  {text}{like}\n",
                k.name, k.emoji
            )
        })
        .collect()
}

fn run_set(keyword: &Keyword, text: &str, back_date: Option<DateTime<Local>>, config: &Config) {
    let status = keyword.base;
    let is_back = keyword.behavior() == "back";

    // Slack (always runs — "back" clears DND then sets catching-up status)
    if is_back
        && let Ok(token) = std::env::var("SLACK_PAT")
        && let Err(e) = end_slack_dnd(&token)
    {
        eprintln!("  Slack   \u{2717} ending DND: {e}");
    }
    match set_slack_status(keyword, text, back_date) {
        Ok(()) => {
            let dnd_detail = match (status.slack_dnd, back_date) {
                (true, Some(dt)) => {
                    format!(
                        " (DND until {})",
                        format_until(dt, Local::now().date_naive())
                    )
                }
                (true, None) => " (DND on)".to_string(),
                _ => String::new(),
            };
            let dnd_cleared = if is_back { " (DND off)" } else { "" };
            let label = if text.is_empty() {
                keyword.emoji.clone()
            } else {
                format!("{text} {}", keyword.emoji)
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
        match set_github_status(keyword, text, back_date, config.github_org_id.as_deref()) {
            Ok(emoji_set) => {
                let org = match (&config.github_org_id, &config.github_org_name) {
                    (Some(_), Some(name)) => format!(" ({name} only)"),
                    (Some(_), None) => " (org only)".to_string(),
                    _ => String::new(),
                };
                let emoji = if emoji_set {
                    String::new()
                } else {
                    format!(", without {} since GitHub doesn't have it", keyword.emoji)
                };
                println!("  GitHub  \u{2713} Limited availability{org}{emoji}");
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

// When the status still matches, clears like `st clear` does on Slack: the status, DND, and
// presence. Anything set after it, like lunch, is skipped along with its DND. GitHub is left alone.
fn run_clear_if(set_by: &Keyword) -> ClearIf {
    let result = match get_slack_status_emoji() {
        Ok(emoji) if should_clear(&emoji, &set_by.emoji) => {
            match clear_slack_status() {
                Ok(()) => println!("  Slack   \u{2713} Cleared {} (DND off)", set_by.emoji),
                Err(e) => eprintln!("  Slack   \u{2717} {e}"),
            }
            match set_slack_presence_auto() {
                Ok(()) => println!("  Slack   \u{2713} Active"),
                Err(e) => eprintln!("  Slack   \u{2717} setting active: {e}"),
            }
            ClearIf::Cleared
        }
        Ok(emoji) => {
            println!("  Slack   - {}", skip_message(&emoji, &set_by.emoji));
            if emoji.is_empty() {
                ClearIf::NoStatus
            } else {
                ClearIf::OtherStatus
            }
        }
        Err(e) => {
            eprintln!("  Slack   \u{2717} {e}");
            ClearIf::ReadFailed
        }
    };

    println!("  GitHub  - No change");
    println!("  Linear  - No change");
    result
}

fn run_clear() {
    match clear_slack_status() {
        Ok(()) => println!("  Slack   \u{2713} Cleared (DND off)"),
        Err(e) => eprintln!("  Slack   \u{2717} {e}"),
    }

    match set_slack_presence_auto() {
        Ok(()) => println!("  Slack   \u{2713} Active"),
        Err(e) => eprintln!("  Slack   \u{2717} setting active: {e}"),
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
    fn clear_if_matches_on_emoji() {
        assert!(should_clear(":virtual-meeting:", ":virtual-meeting:"));
    }

    #[test]
    fn clear_if_skips_a_different_or_empty_emoji() {
        assert!(!should_clear(":fork_and_knife:", ":virtual-meeting:"));
        assert!(!should_clear("", ":virtual-meeting:"));
    }

    #[test]
    fn skip_message_names_the_other_status() {
        assert_eq!(
            skip_message(":fork_and_knife:", ":virtual-meeting:"),
            "Skipped, status is :fork_and_knife:, not :virtual-meeting:"
        );
    }

    #[test]
    fn skip_message_without_a_status() {
        assert_eq!(
            skip_message("", ":virtual-meeting:"),
            "Skipped, you have no status"
        );
    }

    fn keywords_from(toml: &str) -> Result<(Vec<Keyword>, Vec<String>), String> {
        let config: Config = toml::from_str(toml).unwrap();
        resolve_keywords(&config.keywords)
    }

    fn keyword<'a>(keywords: &'a [Keyword], name: &str) -> &'a Keyword {
        find_keyword(keywords, name).unwrap()
    }

    #[test]
    fn no_config_uses_the_built_in_keywords() {
        let (keywords, warnings) = keywords_from("").unwrap();
        assert_eq!(keywords, builtin_keywords());
        assert!(warnings.is_empty());
    }

    #[test]
    fn config_overrides_a_built_in_emoji_and_text() {
        let (keywords, _) = keywords_from(
            r#"
            [keywords.meeting]
            emoji = ":calendar:"
            text = "In a call"
            "#,
        )
        .unwrap();
        let meeting = keyword(&keywords, "meeting");
        assert_eq!(meeting.emoji, ":calendar:");
        assert_eq!(meeting.text, "In a call");
        assert_eq!(meeting.behavior(), "meeting");
    }

    #[test]
    fn config_override_keeps_defaults_it_leaves_out() {
        let (keywords, _) = keywords_from("[keywords.away]\ntext = \"At the doctor\"").unwrap();
        let away = keyword(&keywords, "away");
        assert_eq!(away.emoji, ":car:");
        assert_eq!(away.text, "At the doctor");
        assert_eq!(keyword(&keywords, "lunch"), &builtin_keywords()[0]);
    }

    #[test]
    fn config_keyword_names_are_case_insensitive() {
        let (keywords, _) = keywords_from("[keywords.Lunch]\nemoji = \"taco\"").unwrap();
        assert_eq!(keyword(&keywords, "lunch").emoji, ":taco:");
    }

    #[test]
    fn emoji_accepts_names_with_or_without_colons() {
        assert_eq!(normalize_emoji("car").unwrap(), ":car:");
        assert_eq!(normalize_emoji(":car:").unwrap(), ":car:");
        assert_eq!(normalize_emoji(" +1 ").unwrap(), ":+1:");
        assert_eq!(
            normalize_emoji("virtual-meeting").unwrap(),
            ":virtual-meeting:"
        );
    }

    #[test]
    fn emoji_rejects_anything_that_isnt_a_name() {
        for bad in ["", "::", "two words", "🚗", "car\", \"x", "a:b"] {
            assert!(normalize_emoji(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn config_errors_name_the_bad_entry() {
        let err = keywords_from("[keywords.away]\nemoji = \"two words\"").unwrap_err();
        assert!(err.starts_with("[keywords.away] emoji:"), "{err}");
    }

    #[test]
    fn custom_keyword_copies_its_base() {
        let (keywords, warnings) = keywords_from(
            r#"
            [keywords.sabbatical]
            base = "vacation"
            emoji = "ooo"
            text = "Sabbatical"
            "#,
        )
        .unwrap();
        let sabbatical = keyword(&keywords, "sabbatical");
        assert_eq!(sabbatical.emoji, ":ooo:");
        assert_eq!(sabbatical.text, "Sabbatical");
        assert_eq!(sabbatical.behavior(), "vacation");
        assert!(sabbatical.is_custom());
        assert!(sabbatical.base.slack_dnd && sabbatical.base.github_busy);
        assert!(sabbatical.base.out_of_office);
        assert!(warnings.is_empty());
        assert!(!keyword(&keywords, "vacation").is_custom());
    }

    #[test]
    fn custom_keyword_starts_from_the_base_overrides() {
        let (keywords, warnings) = keywords_from(
            r#"
            [keywords.vacation]
            emoji = "palm_tree"

            [keywords.conference]
            base = "Vacation"
            "#,
        )
        .unwrap();
        let conference = keyword(&keywords, "conference");
        assert_eq!(conference.emoji, ":palm_tree:");
        assert_eq!(conference.text, "Off recharging");
        assert_eq!(
            warnings,
            ["vacation and conference share :palm_tree:, so st clear --if can't tell them apart"]
        );
    }

    #[test]
    fn custom_keywords_follow_the_built_ins() {
        let (keywords, _) = keywords_from(
            "[keywords.zoo]\nbase = \"away\"\nemoji = \"zap\"\n[keywords.appt]\nbase = \"away\"\nemoji = \"tooth\"",
        )
        .unwrap();
        let names: Vec<&str> = keywords.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(&names[STATUSES.len()..], ["appt", "zoo"]);
    }

    #[test]
    fn config_rejects_bad_custom_keywords() {
        let cases = [
            ("[keywords.sabbatical]\nemoji = \"ooo\"", "needs a base"),
            (
                "[keywords.sabbatical]\nbase = \"holiday\"",
                "base \"holiday\" isn't a built-in",
            ),
            (
                "[keywords.a]\nbase = \"away\"\n[keywords.b]\nbase = \"a\"",
                "base \"a\" isn't a built-in",
            ),
            ("[keywords.away]\nbase = \"vacation\"", "away is built in"),
            ("[keywords.clear]\nemoji = \"x\"", "clear can't be changed"),
            (
                "[keywords.keywords]\nbase = \"away\"",
                "keywords can't be changed",
            ),
            (
                "[keywords.\"day off\"]\nbase = \"away\"",
                "names can only use",
            ),
        ];
        for (toml, expected) in cases {
            let err = keywords_from(toml).unwrap_err();
            assert!(err.contains(expected), "{toml:?} gave {err:?}");
        }
    }

    #[test]
    fn keywords_json_marks_custom_keywords_with_their_base() {
        let (keywords, _) = keywords_from(
            "[keywords.sabbatical]\nbase = \"vacation\"\nemoji = \"ooo\"\ntext = \"Sabbatical\"",
        )
        .unwrap();
        let json = keywords_json(&keywords);
        let all = json.as_array().unwrap();
        assert_eq!(all.len(), STATUSES.len() + 1);
        assert_eq!(
            all[0],
            serde_json::json!({
                "keyword": "lunch",
                "base": null,
                "emoji": ":fork_and_knife:",
                "text": "",
                "dnd": true,
                "github_busy": false,
                "out_of_office": false,
            })
        );
        assert_eq!(
            all.last().unwrap(),
            &serde_json::json!({
                "keyword": "sabbatical",
                "base": "vacation",
                "emoji": ":ooo:",
                "text": "Sabbatical",
                "dnd": true,
                "github_busy": true,
                "out_of_office": true,
            })
        );
    }

    #[test]
    fn keywords_table_lines_up_and_names_the_base() {
        let (keywords, _) = keywords_from(
            "[keywords.sabbatical]\nbase = \"vacation\"\nemoji = \"ooo\"\ntext = \"Sabbatical\"",
        )
        .unwrap();
        let table = keywords_table(&keywords);
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(
            lines[0],
            "lunch       :fork_and_knife:         (emoji only)"
        );
        assert_eq!(
            lines.last().unwrap(),
            &"sabbatical  :ooo:                    Sabbatical (like vacation)"
        );
    }

    #[test]
    fn cli_parses_keywords_json() {
        let cli = Cli::try_parse_from(["st", "keywords", "--json"]).unwrap();
        assert_eq!(cli.keyword, "keywords");
        assert!(cli.json);
    }

    #[test]
    fn github_query_escapes_text_and_can_leave_out_the_emoji() {
        let with = github_status_query("Say \"hi\"", Some(":ooo:"), None, Some("O_1")).unwrap();
        assert_eq!(
            with,
            r#"mutation { changeUserStatus(input: { message: "Say \"hi\"", limitedAvailability: true, emoji: ":ooo:", organizationId: "O_1" }) { status { message } } }"#
        );
        let without = github_status_query("Out", None, None, None).unwrap();
        assert!(!without.contains("emoji"));
    }

    // Responses GitHub sent for :desert_island: and for :ooo:, which it doesn't have
    #[test]
    fn a_null_github_status_means_it_wasnt_saved() {
        let saved = serde_json::json!({ "data": { "changeUserStatus": { "status": {
            "message": "Sabbatical", "emoji": ":desert_island:"
        } } } });
        let dropped = serde_json::json!({ "data": { "changeUserStatus": { "status": null } } });
        assert!(status_saved(&saved));
        assert!(!status_saved(&dropped));
    }

    #[test]
    fn config_rejects_unknown_fields() {
        assert!(toml::from_str::<Config>("[keywords.away]\nemjoi = \"car\"").is_err());
    }

    #[test]
    fn shared_emoji_is_a_warning() {
        let (_, warnings) = keywords_from("[keywords.meeting]\nemoji = \"pair\"").unwrap();
        assert_eq!(
            warnings,
            ["pairing and meeting share :pair:, so st clear --if can't tell them apart"]
        );
    }

    #[test]
    fn cli_parses_clear_if() {
        let cli = Cli::try_parse_from(["st", "clear", "--if", "meeting"]).unwrap();
        assert_eq!(cli.keyword, "clear");
        assert_eq!(cli.if_keyword.as_deref(), Some("meeting"));
        assert!(cli.back_date.is_none());
    }

    #[test]
    fn clear_if_target_finds_the_status() {
        let keywords = builtin_keywords();
        let target = clear_if_target(&keywords, "clear", Some("meeting"))
            .unwrap()
            .unwrap();
        assert_eq!(target.name, "meeting");
        assert_eq!(target.emoji, ":virtual-meeting:");
        assert!(clear_if_target(&keywords, "clear", None).unwrap().is_none());
    }

    #[test]
    fn clear_if_target_rejects_bad_input() {
        let keywords = builtin_keywords();
        assert!(clear_if_target(&keywords, "lunch", Some("meeting")).is_err());
        assert!(clear_if_target(&keywords, "clear", Some("bogus")).is_err());
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
        assert_eq!(
            parse_date_with_separators("3/10", today),
            Some(date(2026, 3, 10))
        );
        assert_eq!(
            parse_date_with_separators("3-10", today),
            Some(date(2026, 3, 10))
        );
    }

    #[test]
    fn month_day_rolls_to_next_year_when_past() {
        let today = date(2026, 10, 30);
        assert_eq!(
            parse_date_with_separators("3/10", today),
            Some(date(2027, 3, 10))
        );
    }

    #[test]
    fn month_day_year_accepts_two_and_four_digit_years() {
        let today = date(2026, 1, 1);
        assert_eq!(
            parse_date_with_separators("3/10/26", today),
            Some(date(2026, 3, 10))
        );
        assert_eq!(
            parse_date_with_separators("3-10-2026", today),
            Some(date(2026, 3, 10))
        );
    }

    #[test]
    fn year_month_day_is_iso_order() {
        let today = date(2026, 1, 1);
        assert_eq!(
            parse_date_with_separators("2026-03-10", today),
            Some(date(2026, 3, 10))
        );
        assert_eq!(
            parse_date_with_separators("2026/3/10", today),
            Some(date(2026, 3, 10))
        );
    }

    #[test]
    fn return_dates_on_days_off_move_to_monday() {
        let at = |d: NaiveDate, h: u32| to_local_datetime(d, time(h, 0));
        // Friday 11/20 at 9am becomes Monday 11/23 at 9am
        assert_eq!(
            roll_to_workday(at(date(2026, 11, 20), 9)),
            at(date(2026, 11, 23), 9)
        );
        assert_eq!(
            roll_to_workday(at(date(2026, 11, 22), 7)),
            at(date(2026, 11, 23), 7)
        );
        // Thursday stays Thursday
        assert_eq!(
            roll_to_workday(at(date(2026, 11, 19), 7)),
            at(date(2026, 11, 19), 7)
        );
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
    fn away_ends_at_the_end_of_the_work_day() {
        // Wednesday 1pm ends Wednesday, Wednesday 8pm ends Thursday
        assert_eq!(
            away_back_date(date(2026, 10, 28), time(13, 0)),
            date(2026, 10, 28)
        );
        assert_eq!(
            away_back_date(date(2026, 10, 28), time(20, 0)),
            date(2026, 10, 29)
        );
        // Thursday 8pm and any time Friday end Monday
        assert_eq!(
            away_back_date(date(2026, 10, 29), time(20, 0)),
            date(2026, 11, 2)
        );
        assert_eq!(
            away_back_date(date(2026, 10, 30), time(10, 0)),
            date(2026, 11, 2)
        );
    }

    #[test]
    fn eod_ends_next_work_morning_in_the_evening() {
        // Wednesday 5pm ends Thursday, Thursday 5pm ends Monday
        assert_eq!(
            eod_back_date(date(2026, 10, 28), time(17, 0)),
            date(2026, 10, 29)
        );
        assert_eq!(
            eod_back_date(date(2026, 10, 29), time(17, 0)),
            date(2026, 11, 2)
        );
    }

    #[test]
    fn eod_after_midnight_ends_this_morning_on_a_workday() {
        // Thursday 12:30am still ends Thursday 7am, not Monday
        assert_eq!(
            eod_back_date(date(2026, 10, 29), time(0, 30)),
            date(2026, 10, 29)
        );
        assert_eq!(
            eod_back_date(date(2026, 10, 26), time(6, 59)),
            date(2026, 10, 26)
        );
        // Friday 12:30am is a day off, so it rolls to Monday
        assert_eq!(
            eod_back_date(date(2026, 10, 30), time(0, 30)),
            date(2026, 11, 2)
        );
    }

    #[test]
    fn today_time_without_am_pm_picks_the_next_one() {
        let morning = time(9, 20);
        assert_eq!(parse_today_time("1", morning).unwrap(), time(13, 0));
        assert_eq!(parse_today_time("12:30", morning).unwrap(), time(12, 30));
        assert_eq!(parse_today_time("11", morning).unwrap(), time(11, 0));
        assert_eq!(parse_today_time("9:15", morning).unwrap(), time(21, 15));
    }

    #[test]
    fn today_time_with_am_pm_or_24_hour_is_kept() {
        let morning = time(9, 20);
        assert_eq!(parse_today_time("1am", morning).unwrap(), time(1, 0));
        assert_eq!(parse_today_time("1pm", morning).unwrap(), time(13, 0));
        assert_eq!(parse_today_time("13:00", morning).unwrap(), time(13, 0));
    }

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn extra_args_split_into_time_and_message() {
        assert_eq!(split_time_and_message(&[]), (None, None));
        assert_eq!(
            split_time_and_message(&words("9am")),
            (Some("9am".into()), None)
        );
        assert_eq!(
            split_time_and_message(&words("9am At the beach")),
            (Some("9am".into()), Some("At the beach".into()))
        );
        assert_eq!(
            split_time_and_message(&words("At the beach")),
            (None, Some("At the beach".into()))
        );
        // A quoted message that starts with a number is still a message
        assert_eq!(
            split_time_and_message(&["3 day weekend".to_string()]),
            (None, Some("3 day weekend".into()))
        );
    }

    fn args(date: Option<&str>, time: Option<&str>, message: Option<&str>) -> Args {
        Args {
            date: date.map(String::from),
            time: time.map(String::from),
            message: message.map(String::from),
        }
    }

    #[test]
    fn message_without_a_date() {
        assert_eq!(
            parse_args("meeting", &words("Standup")),
            args(None, None, Some("Standup"))
        );
        assert_eq!(
            parse_args("pairing", &words("with Jeremy")),
            args(None, None, Some("with Jeremy"))
        );
        assert_eq!(
            parse_args("eod", &words("Kid pickup")),
            args(None, None, Some("Kid pickup"))
        );
        assert_eq!(parse_args("meeting", &[]), args(None, None, None));
    }

    #[test]
    fn date_then_time_then_message() {
        assert_eq!(
            parse_args("vacation", &words("friday 9am At the beach")),
            args(Some("friday"), Some("9am"), Some("At the beach"))
        );
        assert_eq!(
            parse_args("eod", &words("monday")),
            args(Some("monday"), None, None)
        );
    }

    #[test]
    fn vacation_and_sick_keep_a_bad_date_as_a_date_so_it_errors() {
        assert_eq!(
            parse_args("vacation", &words("fridy")),
            args(Some("fridy"), None, None)
        );
        assert_eq!(
            parse_args("sick", &words("tomorow")),
            args(Some("tomorow"), None, None)
        );
    }

    #[test]
    fn away_takes_a_message_without_a_date() {
        assert_eq!(
            parse_args("away", &words("Doctor appointment")),
            args(None, None, Some("Doctor appointment"))
        );
    }

    #[test]
    fn meeting_pairing_and_away_take_a_time_without_a_date() {
        assert_eq!(
            parse_args("meeting", &words("3pm")),
            args(None, Some("3pm"), None)
        );
        assert_eq!(
            parse_args("pairing", &words("3pm with Jeremy")),
            args(None, Some("3pm"), Some("with Jeremy"))
        );
        assert_eq!(
            parse_args("away", &words("3pm")),
            args(None, Some("3pm"), None)
        );
        // A date still comes first when given
        assert_eq!(
            parse_args("away", &words("friday")),
            args(Some("friday"), None, None)
        );
        // eod doesn't take a bare time, so it's a message
        assert_eq!(
            parse_args("eod", &words("3pm")),
            args(None, None, Some("3pm"))
        );
    }

    #[test]
    fn lunch_takes_an_optional_time_then_message() {
        assert_eq!(
            parse_args("lunch", &words("1pm Tacos")),
            args(None, Some("1pm"), Some("Tacos"))
        );
        assert_eq!(
            parse_args("lunch", &words("Tacos")),
            args(None, None, Some("Tacos"))
        );
        assert_eq!(
            parse_args("lunch", &words("1")),
            args(None, Some("1"), None)
        );
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
    fn dnd_until_includes_date_when_a_week_or_more_away() {
        let friday = date(2026, 10, 30);
        let next_friday_7am = to_local_datetime(date(2026, 11, 6), time(7, 0));
        let nov_20_7am = to_local_datetime(date(2026, 11, 20), time(7, 0));
        assert_eq!(format_until(next_friday_7am, friday), "Fri 11/6 7am");
        assert_eq!(format_until(nov_20_7am, friday), "Fri 11/20 7am");
    }

    #[test]
    fn invalid_dates_return_none() {
        let today = date(2026, 1, 1);
        assert_eq!(parse_date_with_separators("13/1", today), None);
        assert_eq!(parse_date_with_separators("friday", today), None);
    }
}

#[cfg(test)]
mod focus_flag_tests {
    use super::*;
    use chrono::TimeZone;
    use focus::{FocusOutcome, FocusRequest};

    fn kw(name: &str) -> Keyword {
        find_keyword(&builtin_keywords(), name).unwrap().clone()
    }

    fn custom(name: &str, base: &str) -> Keyword {
        let entry = KeywordConfig {
            base: Some(base.to_string()),
            emoji: Some(":sparkles:".to_string()),
            text: None,
        };
        custom_keyword(name, &entry, &builtin_keywords()).unwrap()
    }

    fn at(h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 9, h, m, 0).unwrap()
    }

    #[test]
    fn focus_on_for_a_status_without_end() {
        assert_eq!(focus_request(Some(&kw("meeting")), None), FocusRequest::On);
    }

    #[test]
    fn focus_until_the_status_end() {
        let end = at(15, 0);
        assert_eq!(
            focus_request(Some(&kw("meeting")), Some(end)),
            FocusRequest::Until(end)
        );
    }

    #[test]
    fn back_and_clear_turn_focus_off() {
        assert_eq!(
            focus_request(Some(&kw("back")), Some(at(9, 15))),
            FocusRequest::Off
        );
        assert_eq!(focus_request(None, None), FocusRequest::Off);
    }

    #[test]
    fn custom_keyword_based_on_back_turns_focus_off() {
        assert_eq!(
            focus_request(Some(&custom("returned", "back")), None),
            FocusRequest::Off
        );
    }

    #[test]
    fn custom_keyword_based_on_meeting_turns_focus_on() {
        assert_eq!(
            focus_request(Some(&custom("standup", "meeting")), None),
            FocusRequest::On
        );
    }

    #[test]
    fn clear_if_focus_rules() {
        assert_eq!(clear_if_focus(&ClearIf::Cleared), Some(FocusRequest::Off));
        assert_eq!(clear_if_focus(&ClearIf::NoStatus), Some(FocusRequest::Off));
        assert_eq!(clear_if_focus(&ClearIf::OtherStatus), None);
        assert_eq!(clear_if_focus(&ClearIf::ReadFailed), None);
    }

    #[test]
    fn focus_lines() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        assert_eq!(
            focus_line(&FocusOutcome::On, today),
            (false, "  Focus   \u{2713} Do Not Disturb on".to_string())
        );
        assert_eq!(
            focus_line(&FocusOutcome::OnUntil(at(15, 0)), today),
            (
                false,
                "  Focus   \u{2713} Do Not Disturb on until 3pm".to_string()
            )
        );
        assert_eq!(
            focus_line(&FocusOutcome::Off, today),
            (false, "  Focus   \u{2713} Do Not Disturb off".to_string())
        );
        assert_eq!(
            focus_line(
                &FocusOutcome::Skipped("Skipped, Slack status wasn't cleared".into()),
                today
            ),
            (
                false,
                "  Focus   - Skipped, Slack status wasn't cleared".to_string()
            )
        );
        assert_eq!(
            focus_line(&FocusOutcome::Failed("boom".into()), today),
            (true, "  Focus   \u{2717} boom".to_string())
        );
    }

    #[test]
    fn focus_flag_parses() {
        let cli = Cli::try_parse_from(["st", "meeting", "--focus"]).unwrap();
        assert!(cli.focus);
        let cli = Cli::try_parse_from(["st", "meeting"]).unwrap();
        assert!(!cli.focus);
    }
}

#[cfg(test)]
mod setup_focus_tests {
    use super::*;

    #[test]
    fn setup_focus_is_reserved() {
        assert!(RESERVED_NAMES.contains(&"setup-focus"));
    }
}
