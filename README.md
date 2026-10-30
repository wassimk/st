# st

> [!CAUTION]
> This project is under active development and is very experimental. It is a personal tool. Use at your own risk.

Set your status across Slack and GitHub with a single command, with reminders for Linear.

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
| `meet` or `meeting` | In a meeting | — | — |
| `pairing` | In a pairing session | — | — |
| `eod` | Outside work hours + DND until next work day 7am | — | — |
| `vacation` | Vacation + DND | Busy (org-scoped) | Reminds to add Google Calendar OOO |
| `sick` | Out sick + DND | — | Reminds to add Google Calendar OOO |
| `away` | Out of office + DND | Busy (org-scoped) | Reminds to add Google Calendar OOO |
| `back` | Catching up for 5 min, clears DND | Clears busy | Reminds to end OOO early |
| `clear` | Clears everything | Clears status | — |

Linear shows you as Away from your Google Calendar out-of-office events, and it has no way to set that from the API, so `st` reminds you instead.

Work days are Monday through Thursday, so `eod` on a Thursday lasts until Monday at 7am.

### Examples

```
st lunch              # DND for ~1 hour (next quarter-hour + 1hr)
st lunch 1:30pm       # DND until 1:30pm
st vacation friday    # Vacation until Friday 7am
st vacation 3/10 9am  # Vacation until March 10 at 9am
st sick tomorrow      # Out sick until tomorrow 7am
st eod                # Outside work hours, DND until 7am next work day
st back               # Clear everything, set "Catching up" for 5 min
st clear              # Clear everything
```

### Date Formats

Day names (`friday`, `mon`), `tomorrow`, `3/10`, `3-10-2026`, `3/10/26`, `2026-03-10`

### Time Formats

`9am`, `1:30pm`, `15:00`, `3p.m.` — defaults to 7am if not specified.
