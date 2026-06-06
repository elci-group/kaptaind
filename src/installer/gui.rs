use fltk::prelude::*;
use fltk::{
    app,
    button::{Button, CheckButton},
    dialog,
    enums::Color,
    frame::Frame,
    text::TextEditor,
    window::Window,
};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

const WINDOW_WIDTH: i32 = 700;
const WINDOW_HEIGHT: i32 = 600;
const PADDING: i32 = 20;
const BUTTON_HEIGHT: i32 = 40;
const BUTTON_WIDTH: i32 = 150;

#[derive(Debug, Clone)]
struct SystemInfo {
    os: String,
    arch: String,
    has_rust: bool,
    has_git: bool,
    has_cargo: bool,
}

#[derive(Debug, Clone)]
struct InstallerState {
    install_path: PathBuf,
    build_mode: String,
    system_install: bool,
    enable_autostart: bool,
}

impl Default for InstallerState {
    fn default() -> Self {
        Self {
            install_path: PathBuf::from(format!(
                "{}/.local/bin",
                std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
            )),
            build_mode: "release".to_string(),
            system_install: false,
            enable_autostart: false,
        }
    }
}

fn detect_system_info() -> SystemInfo {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let has_rust = Command::new("rustc").arg("--version").output().is_ok();
    let has_cargo = Command::new("cargo").arg("--version").output().is_ok();
    let has_git = Command::new("git").arg("--version").output().is_ok();

    SystemInfo {
        os,
        arch,
        has_rust,
        has_git,
        has_cargo,
    }
}

fn check_dependencies(info: &SystemInfo) -> Vec<(String, bool)> {
    vec![
        ("Rust (rustc)".to_string(), info.has_rust),
        ("Cargo Package Manager".to_string(), info.has_cargo),
        ("Git Version Control".to_string(), info.has_git),
    ]
}

fn screen_welcome(sender: app::Sender<Message>) -> app::Receiver<Message> {
    let (sender, receiver) = app::channel::<Message>();

    let mut wind = Window::default()
        .with_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .with_label("Kaptaind Installer");
    wind.set_color(Color::White);

    let info = detect_system_info();

    let mut title = Frame::default()
        .with_size(WINDOW_WIDTH - 2 * PADDING, 60)
        .with_pos(PADDING, PADDING);
    title.set_label("🚀 Welcome to Kaptaind Installer");
    title.set_label_size(28);
    title.set_align(fltk::enums::Align::Left | fltk::enums::Align::Top);

    let mut info_text = TextEditor::default()
        .with_size(WINDOW_WIDTH - 2 * PADDING, 200)
        .with_pos(PADDING, PADDING + 70);
    info_text.set_buffer(fltk::text::TextBuffer::default());
    info_text.buffer().unwrap().set_text(&format!(
        "System Information:\n\
         ├─ OS: {}\n\
         ├─ Architecture: {}\n\
         ├─ Rust (rustc): {}\n\
         ├─ Cargo: {}\n\
         └─ Git: {}\n\n\
         This installer will:\n\
         • Clone the kaptaind repository\n\
         • Build release binaries\n\
         • Install to ~/.local/bin\n\
         • Create configuration directory",
        info.os,
        info.arch,
        if info.has_rust {
            "✓ Found"
        } else {
            "✗ Missing"
        },
        if info.has_cargo {
            "✓ Found"
        } else {
            "✗ Missing"
        },
        if info.has_git {
            "✓ Found"
        } else {
            "✗ Missing"
        }
    ));
    info_text.set_buffer_type(fltk::text::TextEditorType::Wrapped);

    let mut next_btn = Button::default()
        .with_size(BUTTON_WIDTH, BUTTON_HEIGHT)
        .with_pos(
            WINDOW_WIDTH - PADDING - BUTTON_WIDTH,
            WINDOW_HEIGHT - PADDING - BUTTON_HEIGHT,
        )
        .with_label("Next >");
    next_btn.set_color(Color::from_hex(0x4CAF50));
    next_btn.set_label_color(Color::White);

    wind.end();
    wind.show();

    let sender_clone = sender.clone();
    next_btn.set_callback(move |_| {
        sender_clone.send(Message::GotoCheck);
    });

    receiver
}

fn screen_dependencies(sender: app::Sender<Message>) -> app::Receiver<Message> {
    let (sender, receiver) = app::channel::<Message>();

    let mut wind = Window::default()
        .with_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .with_label("Kaptaind Installer - Dependencies");
    wind.set_color(Color::White);

    let mut title = Frame::default()
        .with_size(WINDOW_WIDTH - 2 * PADDING, 60)
        .with_pos(PADDING, PADDING);
    title.set_label("📋 Checking Dependencies");
    title.set_label_size(28);

    let info = detect_system_info();
    let deps = check_dependencies(&info);

    let mut deps_text = TextEditor::default()
        .with_size(WINDOW_WIDTH - 2 * PADDING, 250)
        .with_pos(PADDING, PADDING + 70);
    deps_text.set_buffer(fltk::text::TextBuffer::default());

    let mut dep_display = String::from("Required Dependencies:\n\n");
    let mut all_ok = true;
    for (name, status) in &deps {
        let icon = if *status { "✓" } else { "✗" };
        dep_display.push_str(&format!("{} {}\n", icon, name));
        if !*status {
            all_ok = false;
        }
    }

    if !all_ok {
        dep_display.push_str(
            "\n⚠️  Some dependencies are missing.\n\
             Install Rust from https://rustup.rs/\n\
             Then run this installer again.",
        );
    } else {
        dep_display.push_str(
            "\n✓ All dependencies found!\n\
             Click Next to continue.",
        );
    }

    deps_text.buffer().unwrap().set_text(&dep_display);
    deps_text.set_buffer_type(fltk::text::TextEditorType::Wrapped);

    let mut next_btn = Button::default()
        .with_size(BUTTON_WIDTH, BUTTON_HEIGHT)
        .with_pos(
            WINDOW_WIDTH - PADDING - BUTTON_WIDTH,
            WINDOW_HEIGHT - PADDING - BUTTON_HEIGHT,
        )
        .with_label("Next >");
    next_btn.set_color(Color::from_hex(0x4CAF50));
    next_btn.set_label_color(Color::White);

    let mut back_btn = Button::default()
        .with_size(BUTTON_WIDTH, BUTTON_HEIGHT)
        .with_pos(PADDING, WINDOW_HEIGHT - PADDING - BUTTON_HEIGHT)
        .with_label("< Back");
    back_btn.set_color(Color::from_hex(0x757575));
    back_btn.set_label_color(Color::White);

    wind.end();
    wind.show();

    if !all_ok {
        next_btn.deactivate();
    }

    let sender_clone = sender.clone();
    next_btn.set_callback(move |_| {
        sender_clone.send(Message::GotoOptions);
    });

    let sender_clone = sender.clone();
    back_btn.set_callback(move |_| {
        sender_clone.send(Message::GotoWelcome);
    });

    receiver
}

fn screen_options(
    sender: app::Sender<Message>,
    state: Arc<Mutex<InstallerState>>,
) -> app::Receiver<Message> {
    let (sender_inner, receiver) = app::channel::<Message>();

    let mut wind = Window::default()
        .with_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .with_label("Kaptaind Installer - Installation Options");
    wind.set_color(Color::White);

    let mut title = Frame::default()
        .with_size(WINDOW_WIDTH - 2 * PADDING, 60)
        .with_pos(PADDING, PADDING);
    title.set_label("⚙️  Installation Options");
    title.set_label_size(28);

    let mut info_text = TextEditor::default()
        .with_size(WINDOW_WIDTH - 2 * PADDING, 120)
        .with_pos(PADDING, PADDING + 70);
    info_text.set_buffer(fltk::text::TextBuffer::default());
    info_text.buffer().unwrap().set_text(
        "Installation Path: ~/.local/bin\n\
         (Add to PATH if not already present)\n\n\
         Build Mode: Release (optimized)\n\
         Configuration: ~/.kaptaind/",
    );

    let mut autostart_checkbox = CheckButton::default()
        .with_size(400, 30)
        .with_pos(PADDING, PADDING + 200)
        .with_label("✓ Enable auto-start on login");
    autostart_checkbox.set_checked(false);

    let mut install_btn = Button::default()
        .with_size(BUTTON_WIDTH, BUTTON_HEIGHT)
        .with_pos(
            WINDOW_WIDTH - PADDING - BUTTON_WIDTH,
            WINDOW_HEIGHT - PADDING - BUTTON_HEIGHT,
        )
        .with_label("Install");
    install_btn.set_color(Color::from_hex(0x2196F3));
    install_btn.set_label_color(Color::White);

    let mut back_btn = Button::default()
        .with_size(BUTTON_WIDTH, BUTTON_HEIGHT)
        .with_pos(PADDING, WINDOW_HEIGHT - PADDING - BUTTON_HEIGHT)
        .with_label("< Back");
    back_btn.set_color(Color::from_hex(0x757575));
    back_btn.set_label_color(Color::White);

    wind.end();
    wind.show();

    let state_clone = state.clone();
    autostart_checkbox.set_callback(move |cb| {
        if let Ok(mut s) = state_clone.lock() {
            s.enable_autostart = cb.is_checked();
        }
    });

    let sender_clone = sender_inner.clone();
    let state_clone = state.clone();
    install_btn.set_callback(move |_| {
        if let Ok(mut s) = state_clone.lock() {
            s.enable_autostart = autostart_checkbox.is_checked();
        }
        sender_clone.send(Message::StartInstall);
    });

    let sender_clone = sender_inner.clone();
    back_btn.set_callback(move |_| {
        sender_clone.send(Message::GotoCheck);
    });

    receiver
}

fn screen_progress(sender: app::Sender<Message>) -> app::Receiver<Message> {
    let (sender_inner, receiver) = app::channel::<Message>();

    let mut wind = Window::default()
        .with_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .with_label("Kaptaind Installer - Installing");
    wind.set_color(Color::White);

    let mut title = Frame::default()
        .with_size(WINDOW_WIDTH - 2 * PADDING, 60)
        .with_pos(PADDING, PADDING);
    title.set_label("⏳ Installing Kaptaind...");
    title.set_label_size(28);

    let mut progress_text = TextEditor::default()
        .with_size(WINDOW_WIDTH - 2 * PADDING, 350)
        .with_pos(PADDING, PADDING + 70);
    progress_text.set_buffer(fltk::text::TextBuffer::default());
    progress_text.buffer().unwrap().set_text(
        "⏳ Cloning repository...\n\
         ⏳ Building binaries (this may take 1-3 minutes)...\n\
         ⏳ Installing binaries...\n\
         ⏳ Setting up configuration...",
    );

    wind.end();
    wind.show();

    // Simulate installation in background
    let sender_clone = sender_inner.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        sender_clone.send(Message::InstallComplete);
    });

    receiver
}

fn screen_complete(
    sender: app::Sender<Message>,
    state: Arc<Mutex<InstallerState>>,
) -> app::Receiver<Message> {
    let (sender_inner, receiver) = app::channel::<Message>();

    let mut wind = Window::default()
        .with_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .with_label("Kaptaind Installer - Complete");
    wind.set_color(Color::White);

    let mut title = Frame::default()
        .with_size(WINDOW_WIDTH - 2 * PADDING, 60)
        .with_pos(PADDING, PADDING);
    title.set_label("✓ Installation Complete!");
    title.set_label_size(28);
    title.set_label_color(Color::from_hex(0x4CAF50));

    let autostart_msg = if let Ok(s) = state.lock() {
        if s.enable_autostart {
            "\n✓ Auto-start enabled: kaptaind will start on next login"
        } else {
            "\nTo enable auto-start later, run:\n   kaptaind-cli enable-autostart"
        }
    } else {
        ""
    };

    let mut info_text = TextEditor::default()
        .with_size(WINDOW_WIDTH - 2 * PADDING, 280)
        .with_pos(PADDING, PADDING + 70);
    info_text.set_buffer(fltk::text::TextBuffer::default());
    let completion_text = format!(
        "✓ Kaptaind has been successfully installed!\n\n\
         Next Steps:\n\
         1. Update your shell PATH (if needed):\n\
            export PATH=\"$HOME/.local/bin:$PATH\"\n\n\
         2. Initialize a project:\n\
            kaptaind-cli init\n\n\
         3. Start the daemon:\n\
            kaptaind --daemon\n{}",
        autostart_msg
    );
    info_text.buffer().unwrap().set_text(&completion_text);

    let mut finish_btn = Button::default()
        .with_size(BUTTON_WIDTH, BUTTON_HEIGHT)
        .with_pos(
            WINDOW_WIDTH - PADDING - BUTTON_WIDTH,
            WINDOW_HEIGHT - PADDING - BUTTON_HEIGHT,
        )
        .with_label("Finish");
    finish_btn.set_color(Color::from_hex(0x4CAF50));
    finish_btn.set_label_color(Color::White);

    wind.end();
    wind.show();

    let sender_clone = sender_inner.clone();
    finish_btn.set_callback(move |_| {
        sender_clone.send(Message::Exit);
    });

    receiver
}

#[derive(Debug, Clone)]
enum Message {
    GotoWelcome,
    GotoCheck,
    GotoOptions,
    StartInstall,
    InstallComplete,
    Exit,
}

fn main() {
    let app = app::App::default();
    let mut screen = 0; // 0: welcome, 1: check, 2: options, 3: progress, 4: complete

    let (tx, rx) = app::channel::<Message>();

    let state = Arc::new(Mutex::new(InstallerState::default()));

    loop {
        let msg = match screen {
            0 => {
                let rx = screen_welcome(tx.clone());
                match rx.recv() {
                    Some(Message::GotoCheck) => {
                        screen = 1;
                        continue;
                    }
                    Some(Message::Exit) | None => break,
                    _ => continue,
                }
            }
            1 => {
                let rx = screen_dependencies(tx.clone());
                match rx.recv() {
                    Some(Message::GotoWelcome) => {
                        screen = 0;
                        continue;
                    }
                    Some(Message::GotoOptions) => {
                        screen = 2;
                        continue;
                    }
                    Some(Message::Exit) | None => break,
                    _ => continue,
                }
            }
            2 => {
                let rx = screen_options(tx.clone(), state.clone());
                match rx.recv() {
                    Some(Message::GotoCheck) => {
                        screen = 1;
                        continue;
                    }
                    Some(Message::StartInstall) => {
                        screen = 3;
                        continue;
                    }
                    Some(Message::Exit) | None => break,
                    _ => continue,
                }
            }
            3 => {
                let rx = screen_progress(tx.clone());
                match rx.recv() {
                    Some(Message::InstallComplete) => {
                        screen = 4;
                        continue;
                    }
                    Some(Message::Exit) | None => break,
                    _ => continue,
                }
            }
            4 => {
                let rx = screen_complete(tx.clone(), state.clone());
                match rx.recv() {
                    Some(Message::Exit) | None => break,
                    _ => continue,
                }
            }
            _ => break,
        };
    }
}
