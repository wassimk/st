# st

Set your status across Slack and GitHub with a single command, with reminders for Linear.

![st demo](demo/st.gif)

## 🛠️ Install

```
brew install wassimk/tap/st
```

Or build from source:

```
cargo install --path .
```

## ⚙️ Setup

### Environment Variables

Set these in your shell profile:

- `SLACK_PAT` — Slack User OAuth Token (`xoxp-...`) with `users.profile:write` and `dnd:write` scopes
- `GITHUB_PAT` — GitHub classic Personal Access Token with `user` and `read:org` scopes

### Config File

Create `~/.config/st/config.toml`:

```toml
github_org_id = "YOUR_ORG_GRAPHQL_NODE_ID"  # limits GitHub busy status to this org
github_org_name = "Your Org"                 # optional, shown in output
```

To find your GitHub org's GraphQL node ID, run:

```
gh api graphql -f query='{ organization(login: "your-org") { id } }'
```

## 💻 Usage

```
st <keyword> [back_date] [back_time]
```

### Keywords

| Keyword | Slack | GitHub | Linear |
|---|---|---|---|
| `lunch` | :fork_and_knife: only + DND | — | — |
| `meeting` | In a meeting, optionally until a time | — | — |
| `pairing` | In a pairing session, optionally until a time | — | — |
| `eod` | Outside work hours + DND until next work day 7am | — | — |
| `vacation` | Off recharging + DND | Busy (org-scoped) | Reminds to add Google Calendar OOO |
| `sick` | Out sick + DND | Busy (org-scoped) | Reminds to add Google Calendar OOO |
| `away` | Away for a bit until 5pm + DND | Busy (org-scoped) | Reminds to add Google Calendar OOO |
| `back` | Catching up for 15 min, clears DND | Clears busy | Reminds to end OOO early |
| `clear` | Clears everything | Clears status | — |

Linear shows you as Away from your Google Calendar out-of-office events, and it has no way to set that from the API, so `st` reminds you instead.

`away` with no date or time ends at 5pm, or at 5pm the next work day if you run it after hours. Work days are Monday through Thursday, so `eod` on a Thursday lasts until Monday at 7am. A `vacation`, `sick`, or `away` return date on a Friday or weekend moves to Monday at the same time.

### Examples

```
st lunch              # DND for ~1 hour (next quarter-hour + 1hr)
st lunch 1:30pm       # DND until 1:30pm
st vacation friday    # Off until Monday 7am, since Fridays are off
st vacation 3/10 9am  # Vacation until March 10 at 9am
st vacation 3/10 At the beach  # Custom message in place of "Off recharging"
st sick tomorrow      # Out sick until tomorrow 7am
st eod                # Outside work hours, DND until 7am next work day
st back               # Clear everything, set "Catching up" for 15 min
st clear              # Clear everything
```

### Custom Messages

Every keyword except `clear` takes an optional message that replaces the default status text in Slack and GitHub. Put it after the date and time, or skip the date entirely:

```
st meeting Standup
st meeting 3pm Standup
st pairing with Jeremy
st lunch 1pm Tacos
st vacation 3/10 At the beach
```

For `vacation` and `sick`, which have no default end, the first word must be a date if you give anything, so a typo like `fridy` is an error instead of a status that never ends. Quote a message that starts with a number, like `st vacation friday "3 day weekend"`, so the number isn't read as a time.

### Date Formats

Day names (`friday`, `mon`), `tomorrow`, `3/10`, `3-10-2026`, `3/10/26`, `2026-03-10`

### Time Formats

`9am`, `1:30pm`, `15:00`, `3p.m.` — defaults to 7am if not specified.

## 🔗 Related Projects

- [raycast-st](https://github.com/wassimk/raycast-st) - A [Raycast](https://www.raycast.com) extension for setting your status with `st` on macOS.
