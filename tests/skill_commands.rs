//! Doc test for the shipped agent skill (`skills/kaptaind/SKILL.md`).
//!
//! Every `kaptaind <subcommand>` invocation inside a fenced code block must
//! name a real subcommand: `kaptaind <subcommand> --help` has to parse. This
//! keeps the skill from teaching agents commands that do not exist (the
//! pre-10.4 skill taught `kaptaind-cli` and `aoc stop`).

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// First non-flag token after each standalone `kaptaind` word in fenced code
/// blocks. `kaptaind-cli`, flags (`--force`), and placeholders (`<cmd>`) are
/// skipped.
fn skill_subcommands(markdown: &str) -> BTreeSet<String> {
    let mut subcommands = BTreeSet::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            continue;
        }
        // Drop trailing shell comments.
        let code = line.split(" #").next().unwrap_or("");
        let code = if code.trim_start().starts_with('#') {
            ""
        } else {
            code
        };
        let tokens: Vec<&str> = code.split_whitespace().collect();
        for (i, tok) in tokens.iter().enumerate() {
            if *tok != "kaptaind" {
                continue;
            }
            let Some(next) = tokens.get(i + 1) else {
                continue;
            };
            let is_word = next
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if next.starts_with('-') || !is_word || next.is_empty() {
                continue;
            }
            subcommands.insert((*next).to_string());
        }
    }
    subcommands
}

#[test]
fn extractor_skips_flags_placeholders_and_cli_shim() {
    let md = "```bash\nkaptaind --force\nkaptaind <cmd>\nkaptaind-cli status\n\
              kaptaind ready --for push   # kaptaind bogus\n# kaptaind nope\n```\n\
              kaptaind outside-fence\n";
    let got: Vec<String> = skill_subcommands(md).into_iter().collect();
    assert_eq!(got, vec!["ready".to_string()]);
}

#[test]
fn every_skill_command_parses() {
    let skill = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/kaptaind/SKILL.md");
    let markdown =
        std::fs::read_to_string(&skill).unwrap_or_else(|e| panic!("read {}: {e}", skill.display()));
    let subcommands = skill_subcommands(&markdown);
    assert!(
        subcommands.len() >= 5,
        "expected the skill to document several subcommands, found {subcommands:?}"
    );
    for sub in &subcommands {
        let out = Command::new(env!("CARGO_BIN_EXE_kaptaind"))
            .args([sub.as_str(), "--help"])
            .env("NO_COLOR", "1")
            .output()
            .expect("spawn kaptaind");
        assert!(
            out.status.success(),
            "`kaptaind {sub} --help` failed (skill documents a missing subcommand?)\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
