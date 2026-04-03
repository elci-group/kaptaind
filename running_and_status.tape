Output running_and_status.gif

Require kaptaind
Require kaptaind-cli

Set Shell "bash"
Set FontSize 15
Set Width 1000
Set Height 600
Set Theme "Catppuccin Mocha"

Hide
Type "pkill -f kaptaind || true"
Enter
Type "rm -rf .kaptaind"
Enter
Type "cp ./target/release/kaptaind ./target/release/kaptaind-cli ~/.local/bin 2>/dev/null || true"
Enter
Type "export PATH=$PATH:~/.local/bin"
Enter
Type "clear"
Enter
Show

Type "kaptaind-cli status"
Sleep 500ms
Enter
Sleep 2s

Type "kaptaind --daemon"
Sleep 500ms
Enter
Sleep 2s

Type "kaptaind-cli status"
Sleep 500ms
Enter
Sleep 4s

Hide
Type "pkill -f kaptaind"
Enter
