Output demo.gif

Require kaptaind
Require kaptaind-cli

Set Shell "bash"
Set FontSize 15
Set Width 1200
Set Height 750
Set Padding 40
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful

Hide
Type "cp target/release/kaptaind target/release/kaptaind-cli ~/.local/bin/ 2>/dev/null || mkdir -p ~/.local/bin && cp target/release/kaptaind target/release/kaptaind-cli ~/.local/bin/"
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
Sleep 3s

Type "kaptaind --lanes"
Sleep 500ms
Enter
Sleep 3s

Type 'echo "pub fn demo_api() {}" > src/demo.rs'
Sleep 500ms
Enter
Sleep 1s

Type "kaptaind-cli analyze"
Sleep 500ms
Enter
Sleep 3s

Type "# Waiting for kaptaind to cluster and commit (5s window)..."
Sleep 500ms
Enter
Sleep 8s

Type "kaptaind-cli log -l 5"
Sleep 500ms
Enter
Sleep 6s

Hide
Type "rm src/demo.rs"
Enter
Type "pkill -f kaptaind"
Enter
