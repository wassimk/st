Output "demo/st.gif"

Set Shell "bash"
Set FontFamily "JetBrainsMono Nerd Font"
Set FontSize 15
Set Width 1000
Set Height 400
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms

# Put the demo's stand-in st first on PATH, so no real Slack or GitHub account
# is touched
Hide
Type "export PATH=$PWD/demo/bin:$PATH PS1='$ ' && clear"
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

