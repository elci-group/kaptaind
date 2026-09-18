//! Custom top-level `kaptaind --help` screen.
//!
//! Grouping is data-driven off clap's own `Command`/`Subcommand` metadata
//! (`Cli::command()`), not hand-duplicated text, so a subcommand's `about`
//! stays in sync automatically. Per-subcommand help (`kaptaind push --help`)
//! is untouched — this module only ever renders the bare top-level screen.

use clap::CommandFactory;
use form3::anim::Aura;
use form3::ansi::{self, Color};
use form3::compat::Colorize;
use form3::table::{Attribute, Cell, Table, TableStyle};
use form3::term::{ColorSupport, TermInfo};
use std::io::{IsTerminal, Write};
use std::time::Duration;

use crate::Cli;

const BANNER: &str = "+-------------------+\n\
|    .-=====-.      |\n\
|   /  .---.  \\     |\n\
|  |--< </> >--|    |\n\
|   \\  '---'  /     |\n\
|    '---|---'      |\n\
|    ___/ \\___      |\n\
|   /_KAPTAIND_\\    |\n\
+-------------------+";

/// (group heading, subcommand names in that group, in display order).
const GROUPS: &[(&str, &[&str])] = &[
    (
        "Lifecycle & Release",
        &[
            "branch", "release", "checkout", "ship", "rollback", "migrate", "schema", "aoc",
        ],
    ),
    ("Sync & Remote", &["pull", "push", "integrate"]),
    (
        "Observability",
        &[
            "status", "dashboard", "log", "logs", "history", "trace", "audit", "evidence",
            "report", "monitor", "probe",
        ],
    ),
    (
        "Analysis & Safety",
        &[
            "analyze", "explain", "ci-hint", "doctor", "validate", "stress", "trawl",
        ],
    ),
    (
        "System & Daemon",
        &[
            "init",
            "autostart",
            "enable-autostart",
            "disable-autostart",
            "service",
            "suspend",
            "resume",
            "shark",
            "vacs",
            "storage",
            "governance",
            "integrations",
            "environment",
        ],
    ),
];

/// Daemon-mode top-level flags vs. plain global flags, for the flags table.
const DAEMON_FLAGS: &[&str] = &[
    "daemon",
    "dock",
    "radar",
    "lanes",
    "shark-mode",
    "shark-arbiter",
    "health-port",
    "web",
    "web-port",
    "dry-run",
    "config",
    "force",
];
const GLOBAL_FLAGS: &[&str] = &["repo"];

pub fn print_top_level_help() {
    let term = TermInfo::detect();
    let is_tty = std::io::stdout().is_terminal();
    let colorize = term.supports_color();

    if is_tty && colorize {
        animate_banner(term.color_support);
    } else {
        println!("{}", Aura::mystic().paint(BANNER, 1.0, term.color_support));
    }
    println!();

    let cmd = Cli::command();
    if let Some(about) = cmd.get_about() {
        println!("{}", about);
    }
    println!();

    let mut placed = std::collections::HashSet::new();
    for (group_name, names) in GROUPS {
        let mut table = Table::new();
        table.set_style(TableStyle::Rounded);
        table.set_header(vec![
            header_cell("Command", colorize),
            header_cell("Description", colorize),
        ]);
        let mut any = false;
        for name in *names {
            if let Some(sub) = cmd.find_subcommand(name) {
                placed.insert((*name).to_string());
                let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
                table.add_row(vec![command_cell(name, colorize), Cell::new(about)]);
                any = true;
            }
        }
        if any {
            println!("{}", heading(group_name));
            print!("{table}");
            println!();
        }
    }

    // Safety net: any subcommand not covered by the lookup table above still
    // shows up here rather than being silently dropped.
    let others: Vec<&clap::Command> = cmd
        .get_subcommands()
        .filter(|s| s.get_name() != "help" && !placed.contains(s.get_name()))
        .collect();
    if !others.is_empty() {
        let mut table = Table::new();
        table.set_style(TableStyle::Rounded);
        table.set_header(vec![
            header_cell("Command", colorize),
            header_cell("Description", colorize),
        ]);
        for sub in &others {
            let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
            table.add_row(vec![command_cell(sub.get_name(), colorize), Cell::new(about)]);
        }
        println!("{}", heading("Other"));
        print!("{table}");
        println!();
    }

    print_flags_table("Daemon mode", DAEMON_FLAGS, &cmd, colorize);
    print_flags_table("Global", GLOBAL_FLAGS, &cmd, colorize);

    println!(
        "Run `kaptaind <command> --help` for details on a specific command, or `kaptaind --dock`/`--radar`/`--lanes` for daemon-mode views."
    );
}

fn print_flags_table(title: &str, names: &[&str], cmd: &clap::Command, colorize: bool) {
    let mut table = Table::new();
    table.set_style(TableStyle::Rounded);
    table.set_header(vec![
        header_cell("Flag", colorize),
        header_cell("Description", colorize),
    ]);
    let mut any = false;
    for name in names {
        if let Some(arg) = cmd.get_arguments().find(|a| a.get_long() == Some(*name)) {
            let flag = match arg.get_short() {
                Some(short) => format!("-{short}, --{name}"),
                None => format!("--{name}"),
            };
            let help = arg
                .get_help()
                .map(|h| h.to_string())
                .unwrap_or_default();
            table.add_row(vec![command_cell(&flag, colorize), Cell::new(help)]);
            any = true;
        }
    }
    if any {
        println!("{}", heading(title));
        print!("{table}");
        println!();
    }
}

fn header_cell(text: &str, colorize: bool) -> Cell {
    if colorize {
        Cell::new(text).add_attribute(Attribute::Bold).fg(Color::Cyan)
    } else {
        Cell::new(text)
    }
}

fn command_cell(text: &str, colorize: bool) -> Cell {
    if colorize {
        Cell::new(text).add_attribute(Attribute::Bold).fg(Color::Magenta)
    } else {
        Cell::new(text)
    }
}

fn heading(text: &str) -> String {
    // `Colorize`/`StyledText` self-detect NO_COLOR/TTY support, so no manual
    // gating is needed here (unlike `form3::table::Cell`, which always emits
    // its configured SGR codes regardless of terminal support).
    text.bold().cyan().to_string()
}

/// A short (<400ms), skippable color-sweep reveal of the banner, only ever
/// called when stdout is a real, color-capable TTY. Always settles into the
/// same static final frame that the non-animated path prints directly, so
/// redirected/non-interactive output is never truncated or delayed.
fn animate_banner(support: ColorSupport) {
    let aura = Aura::mystic();
    let lines = BANNER.lines().count() as u16;
    let frames = 6;
    let mut stdout = std::io::stdout();
    for step in 0..=frames {
        let progress = step as f32 / frames as f32;
        print!("{}", aura.paint(BANNER, progress, support));
        let _ = stdout.flush();
        if step < frames {
            std::thread::sleep(Duration::from_millis(60));
            // `lines - 1`: the cursor sits on the banner's last row (no
            // trailing newline was printed), so only `lines - 1` rows of
            // upward movement are needed to get back to the first row.
            print!("{}\r", ansi::move_up(lines - 1));
        } else {
            println!();
        }
    }
}
