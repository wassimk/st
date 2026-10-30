# Demo

The README GIF is rendered with [VHS](https://github.com/charmbracelet/vhs) from `st.tape`.

## Requirements

- `vhs`
- JetBrainsMono Nerd Font

## Rendering

From the repo root:

```shell
vhs demo/st.tape
```

This overwrites `demo/st.gif`.

## How it works

No real Slack or GitHub account is touched. The tape puts `demo/bin` first on `PATH`, so `st` runs the stand-in script `demo/bin/st` instead of the real CLI. It prints the output the real `st` gives for the three commands in the tape, copied from a real run with a demo config whose GitHub org is named "Acme".

If `st`'s output changes, update the matching lines in `demo/bin/st` and re-render. To add a command to the demo, add a case for it in `demo/bin/st` first.
