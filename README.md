# st

> [!CAUTION]
> This project is under active development and is very experimental. It is a personal tool. Use at your own risk.

Set your status across Slack, GitHub, and Asana with a single command.

## Install

```
brew install wassimk/tap/st
```

Or build from source:

```
cargo install --path .
```

## Setup

### Environment Variables

Set these in your shell profile:

- `SLACK_PAT` — Slack User OAuth Token (`xoxp-...`) with `users.profile:write` and `dnd:write` scopes
- `GITHUB_PAT` — GitHub classic Personal Access Token with `user` and `read:org` scopes
- `ASANA_PAT` — Asana Personal Access Token

### Config File

Create `~/.config/st/config.toml`:

```toml
github_org_id = "YOUR_ORG_GRAPHQL_NODE_ID"  # limits GitHub busy status to this org
asana_user_gid = "YOUR_ASANA_USER_GID"       # for reading Asana OOO status
```

To find your GitHub org's GraphQL node ID, run:

```
gh api graphql -f query='{ organization(login: "your-org") { id } }'
```

To find your Asana user GID, run:

```
curl -s -H "Authorization: Bearer $ASANA_PAT" https://app.asana.com/api/1.0/users/me | jq '.data.gid'
```

## Usage

```
st <keyword> [back_date] [back_time]
```

### Keywords

| Keyword | Slack | GitHub | Asana |
|---|---|---|---|
| `lunch` | Lunchin' + DND | — | — |
| `zoom` | In a meeting (Zoom) | — | — |
| `tuple` | Pairing (Tuple) | — | — |
| `meet` | In a meeting | — | — |
| `eod` | Done for the day + DND | — | — |
| `vacation` | Vacation + DND | Busy (org-scoped) | Reminds to set OOO |
| `sick` | Out sick + DND | — | Reminds to set OOO |
| `away` | Out of office + DND | Busy (org-scoped) | Reminds to set OOO |
| `back` | Catching up, clears DND | Clears busy | Reminds to clear OOO |
| `clear` | Clears everything | Clears status | Reminds to clear OOO |

### Examples

```
st lunch              # DND for ~1 hour (next quarter-hour + 1hr)
st lunch 1:30pm       # DND until 1:30pm
st vacation friday    # Vacation until Friday 7am
st vacation 3/10 9am  # Vacation until March 10 at 9am
st sick tomorrow      # Out sick until tomorrow 7am
st eod                # Done for the day, DND on
st back               # Clear everything, set "Catching up" for 5 min
st clear              # Clear everything
```

### Date Formats

Day names (`friday`, `mon`), `tomorrow`, `3/10`, `3-10-2026`, `3/10/26`

### Time Formats

`9am`, `1:30pm`, `15:00`, `3p.m.` — defaults to 7am if not specified.
