# Demo

The README GIF is rendered with [VHS](https://github.com/charmbracelet/vhs) from `st.tape`.

## Requirements

- `vhs`
- `python3`
- JetBrainsMono Nerd Font
- A release build of `st`: `cargo build --release`

## Rendering

From the repo root:

```shell
cargo build --release
vhs demo/st.tape
```

This overwrites `demo/st.gif`.

## How it works

No real Slack or GitHub account is touched. `fake_apis.py` serves fake Slack and GitHub GraphQL endpoints on `127.0.0.1:19200` that always succeed. The tape points `ST_SLACK_API_URL` and `ST_GITHUB_API_URL` at it, sets dummy tokens, and points `HOME` at a temp directory with a demo config, so the GitHub line shows a fake "Acme" org instead of the real one.

The return date in `st vacation friday` depends on the day you render, so the date in the GIF changes between renders.
