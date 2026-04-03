Output views.gif

Require kaptaind

Set Shell "bash"
Set FontSize 15
Set Width 1000
Set Height 600
Set Theme "Catppuccin Mocha"

Hide
Type "cp ./target/release/kaptaind ~/.local/bin 2>/dev/null || true"
Enter
Type "export PATH=$PATH:~/.local/bin"
Enter
Type "clear"
Enter
Show

Type "kaptaind --dock"
Sleep 500ms
Enter
Sleep 3s

Type "kaptaind --radar"
Sleep 500ms
Enter
Sleep 3s

Type "kaptaind --lanes"
Sleep 500ms
Enter
Sleep 4s
