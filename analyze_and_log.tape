Output analyze_and_log.gif

Require kaptaind
Require kaptaind-cli

Set Shell "bash"
Set FontSize 15
Set Width 1200
Set Height 750
Set Theme "Catppuccin Mocha"

Hide
Type "pkill -f kaptaind || true"
Enter
Type "rm -rf .kaptaind src/demo.rs"
Enter
Type "cp ./target/release/kaptaind ./target/release/kaptaind-cli ~/.local/bin 2>/dev/null || true"
Enter
Type "export PATH=$PATH:~/.local/bin"
Enter
Type "kaptaind --daemon"
Enter
Type "clear"
Enter
Show

Type 'echo "pub fn new_feature() {}" > src/demo.rs'
Sleep 500ms
Enter
Sleep 1s

Type "kaptaind-cli analyze"
Sleep 500ms
Enter
Sleep 4s

Type "# Waiting for background commit..."
Sleep 500ms
Enter
Sleep 8s

Type "kaptaind-cli log -l 3"
Sleep 500ms
Enter
Sleep 6s

Hide
Type "pkill -f kaptaind"
Enter
Type "rm src/demo.rs"
Enter
