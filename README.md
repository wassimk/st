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

- `SLACK_PAT` — Slack User OAuth Token (`xoxp-...`) with `users.profile:write`, `dnd:write`, and `users:write` scopes, plus `users.profile:read` for `st clear --if`
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

| Keyword | Emoji | Slack | GitHub | Linear |
|---|---|---|---|---|
| `lunch` | `:fork_and_knife:` 🍴 | Emoji only + DND | — | — |
| `meeting` | `:virtual-meeting:` | In a meeting, optionally until a time | — | — |
| `pairing` | `:pair:` | In a pairing session, optionally until a time | — | — |
| `eod` | `:mantelpiece_clock:` 🕰️ | Outside work hours + DND until next work day 7am | — | — |
| `vacation` | `:desert_island:` 🏝️ | Off recharging + DND | Busy (org-scoped) | Reminds to add Google Calendar OOO |
| `sick` | `:face_with_thermometer:` 🤒 | Out sick + DND | Busy (org-scoped) | Reminds to add Google Calendar OOO |
| `away` | `:car:` 🚗 | Stepped away for a bit until 5pm + DND | Busy (org-scoped) | Reminds to add Google Calendar OOO |
| `back` | `:inbox_tray:` 📥 | Catching up for 15 min, clears DND | Clears busy | Reminds to end OOO early |
| `clear` | | Clears everything, sets you active | Clears status | — |
| `clear --if <keyword>` | | Clears the status, ends DND, and sets you active, only if its emoji is still that keyword's | — | — |
| `keywords` | | Lists every keyword with its emoji and text, `--json` for JSON | — | — |

`:virtual-meeting:` and `:pair:` are custom Slack emoji. If your workspace doesn't have them, change them in the config (see [Your Own Keywords](#your-own-keywords)).

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
st clear --if meeting # Clear the meeting status, but leave lunch or anything else alone
```

### Focus

Add `--focus` to also turn on macOS Do Not Disturb, which is handy for meetings and pairing. If the status has an end time, Do Not Disturb turns itself off then too. `clear` and `back` turn it off.

```
st meeting --focus             # In a meeting, Do Not Disturb on until you turn it off
st pairing 3pm --focus         # Pairing until 3pm, Do Not Disturb off at 3pm
st clear --focus               # Clear everything and turn Do Not Disturb off
st clear --if meeting --focus  # Same, but if your status changed (to lunch, say), both stay
```

macOS has no public way for apps to change Focus, so `st` uses a shortcut in the Shortcuts app. Install it once:

```
st setup-focus
```

It opens an Add Shortcut window. After you click Add Shortcut, it turns Do Not Disturb on and off once to check that it works. Allow any prompt macOS shows then. Keep the shortcut named `st Focus`, since `st` runs it by name.

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

### Your Own Keywords

You can change the emoji and text of any keyword, or add keywords of your own, in `~/.config/st/config.toml`:

```toml
# Change a built-in keyword. Leave out anything you want to keep.
[keywords.meeting]
emoji = ":calendar:"
text = "In a call"

# Add a keyword. It acts like its base: DND, GitHub busy, the Linear
# reminder, and how dates work all come from the built-in keyword.
[keywords.sabbatical]
base = "vacation"
emoji = ":ooo:"
text = "Sabbatical"
```

```
st sabbatical 11/11 10am     # vacation, with your emoji and text
st clear --if sabbatical     # clears it only if the status is still :ooo:
st keywords                  # shows every keyword after your changes
```

- `emoji` takes a Slack emoji name, with or without the colons, including your workspace's custom emoji. GitHub only has standard emoji, so a custom one like `:ooo:` is left off the GitHub status.
- `text` is the default message. A message you type still replaces it.
- A custom keyword starts with its base's emoji and text, so give it its own emoji.
- `st clear --if` tells keywords apart by emoji, so `st` warns when two share one. If you change an emoji while that status is set, `st clear --if` skips it once. Use `st clear` instead.
- `clear` and `keywords` can't be changed, and a built-in keyword can't have a `base`.

### Date Formats

Day names (`friday`, `mon`), `tomorrow`, `3/10`, `3-10-2026`, `3/10/26`, `2026-03-10`

### Time Formats

`9am`, `1:30pm`, `15:00`, `3p.m.` — defaults to 7am if not specified.

## 🔗 Related Projects

- [raycast-st](https://github.com/wassimk/raycast-st) - A [Raycast](https://www.raycast.com) extension for setting your status with `st` on macOS.
