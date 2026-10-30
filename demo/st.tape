Output "demo/st.gif"

Require python3

Set Shell "bash"
Set FontFamily "JetBrainsMono Nerd Font"
Set FontSize 15
Set Width 1000
Set Height 400
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms

# Point st at the fake APIs and a sandboxed HOME with a demo config, so no real
# Slack or GitHub account is touched
Hide
Type "python3 demo/fake_apis.py & sleep 1"
Enter
Type "export DEMO_HOME=$(mktemp -d) && mkdir -p $DEMO_HOME/.config/st"
Enter
Type `printf 'github_org_id = \"O_demo\"\ngithub_org_name = \"Acme\"\n' > $DEMO_HOME/.config/st/config.toml`
Enter
Type "export PATH=$PWD/target/release:$PATH HOME=$DEMO_HOME SLACK_PAT=xoxp-demo GITHUB_PAT=ghp_demo"
Enter
Type "export ST_SLACK_API_URL=http://127.0.0.1:19200/slack ST_GITHUB_API_URL=http://127.0.0.1:19200/graphql"
Enter
Type "export PS1='$ ' && clear"
Enter
Sleep 500ms
Show

Type "st meeting 3pm Standup"
Sleep 500ms
Enter
Wait+Screen /Linear/
Sleep 2s

Type "st vacation friday"
Sleep 500ms
Enter
Wait+Screen /Google Calendar/
Sleep 3s

Type "st back"
Sleep 500ms
Enter
Wait+Screen /back early/
Sleep 3s

Hide
Type "kill %1"
Enter
