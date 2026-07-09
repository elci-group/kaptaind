use kaptaind::trawler::TrawlOptions;
use kaptaind::util::style::*;

pub fn handle_trawl(options: &TrawlOptions, format: &str, dry_run: bool) -> anyhow::Result<()> {
    println!(
        "{} {}",
        "🎣".cyan(),
        "Trawling for codebases...".bold().cyan()
    );
    println!("   Root: {}", options.root.display().to_string().blue());
    if let Some(depth) = options.max_depth {
        println!("   Max depth: {}", depth.to_string().yellow());
    }
    if !options.filter_types.is_empty() {
        let types: Vec<String> = options.filter_types.iter().map(|t| t.to_string()).collect();
        println!("   Filter: {}", types.join(", ").yellow());
    }
    if dry_run {
        println!("   Mode: {}", "dry-run (no changes)".magenta());
    }
    println!();

    let start_time = std::time::Instant::now();
    let result = kaptaind::trawler::trawl(options)?;
    let elapsed = start_time.elapsed();

    if format == "json" {
        let json_output = serde_json::json!({
            "projects": result.projects.iter().map(|p| serde_json::json!({
                "path": p.path.display().to_string(),
                "type": p.project_type.to_string(),
                "is_git": p.is_git_repo,
                "is_initialized": p.is_initialized,
            })).collect::<Vec<_>>(),
            "summary": {
                "discovered": result.projects.len(),
                "initialized": result.initialized_count,
                "registered": result.registered_count,
                "skipped": result.skipped_count,
                "errors": result.errors.len(),
            },
            "elapsed_ms": elapsed.as_millis(),
        });
        println!("{}", serde_json::to_string_pretty(&json_output)?);
    } else {
        // Text format
        println!("{}", "━".repeat(60).bright_black());

        if result.projects.is_empty() {
            println!("{} {}", "ℹ️".blue(), "No projects found.".blue());
        } else {
            println!(
                "{} {}",
                "📁".cyan(),
                format!("Discovered {} project(s):", result.projects.len()).bold()
            );
            println!();

            for project in &result.projects {
                let icon = match project.project_type {
                    kaptaind::trawler::ProjectType::Rust => "🦀",
                    kaptaind::trawler::ProjectType::Node => "📦",
                    kaptaind::trawler::ProjectType::Python => "🐍",
                    kaptaind::trawler::ProjectType::Go => "🐹",
                    kaptaind::trawler::ProjectType::Swift => "🦉",
                    kaptaind::trawler::ProjectType::Kotlin => "🅺",
                    kaptaind::trawler::ProjectType::Java => "☕",
                    kaptaind::trawler::ProjectType::Ruby => "💎",
                    kaptaind::trawler::ProjectType::Elixir => "💧",
                    kaptaind::trawler::ProjectType::Php => "🐘",
                    kaptaind::trawler::ProjectType::Dotnet => "🔷",
                    kaptaind::trawler::ProjectType::Cpp => "⚙️ ",
                    kaptaind::trawler::ProjectType::Lua => "🌙",
                    kaptaind::trawler::ProjectType::Scala => "🎯",
                    kaptaind::trawler::ProjectType::Clojure => "🍃",
                    kaptaind::trawler::ProjectType::Haskell => "λ",
                    kaptaind::trawler::ProjectType::Julia => "🎨",
                    kaptaind::trawler::ProjectType::R => "📊",
                    kaptaind::trawler::ProjectType::Perl => "🐪",
                    kaptaind::trawler::ProjectType::Unknown => "❓",
                };

                let status = if project.is_initialized {
                    "✅ initialized".dimmed()
                } else {
                    "🆕 new".green()
                };

                let git_indicator = if project.is_git_repo { "🌿" } else { "  " };

                println!(
                    "  {} {} {} {} {} {}",
                    icon,
                    project.project_type.to_string().cyan(),
                    project.path.display().to_string().blue(),
                    git_indicator,
                    status,
                    if dry_run && !project.is_initialized {
                        "[would init]".yellow()
                    } else {
                        "".normal()
                    }
                );
            }

            println!();
        }

        println!("{}", "━".repeat(60).bright_black());
        println!("{} {}", "📊".cyan(), "Summary:".bold());
        println!(
            "   Discovered: {}",
            result.projects.len().to_string().yellow()
        );

        if !dry_run {
            println!(
                "   Initialized: {}",
                result.initialized_count.to_string().green()
            );
            println!(
                "   Registered: {}",
                result.registered_count.to_string().green()
            );
            println!("   Skipped: {}", result.skipped_count.to_string().dimmed());
        } else {
            let would_init = result.projects.iter().filter(|p| !p.is_initialized).count();
            println!("   Would initialize: {}", would_init.to_string().green());
        }

        if !result.errors.is_empty() {
            println!("   Errors: {}", result.errors.len().to_string().red());
        }

        println!("   Time: {:.2}s", elapsed.as_secs_f64());
        println!("{}", "━".repeat(60).bright_black());

        if !result.errors.is_empty() {
            println!();
            println!("{} {}", "⚠️".yellow(), "Errors:".yellow().bold());
            for error in &result.errors {
                println!("   - {}", error.red());
            }
        }
    }

    Ok(())
}
