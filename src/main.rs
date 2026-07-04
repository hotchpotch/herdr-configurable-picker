//! Entry point: wires env, config, socket client, and the TUI event loop.
//! All logic lives in the tested modules; this file only glues them.

mod app;
mod config;
mod herdr_client;
mod keymap;
mod model;
mod ui;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crossterm::event::{self, Event};

use app::{App, Outcome};
use herdr_client::{HerdrApi, SocketClient};
use keymap::KeyPress;

fn main() -> ExitCode {
    let Some(socket_path) = std::env::var_os("HERDR_SOCKET_PATH") else {
        eprintln!(
            "herdr-configurable-picker must run inside a herdr session \
             (HERDR_SOCKET_PATH is not set).\n\
             Install the plugin and open it via its \"open\" action; \
             see README.md."
        );
        return ExitCode::from(2);
    };

    let mut warnings = Vec::new();
    let config = match std::env::var_os("HERDR_PLUGIN_CONFIG_DIR") {
        Some(dir) => {
            let (config, mut config_warnings) = config::load_or_seed(Path::new(&dir));
            warnings.append(&mut config_warnings);
            config
        }
        None => {
            warnings.push("HERDR_PLUGIN_CONFIG_DIR is not set; using default config".to_string());
            config::Config::default()
        }
    };
    let (keymap, mut keymap_warnings) = keymap::Keymap::from_bindings(&config.keys.to_bindings());
    warnings.append(&mut keymap_warnings);
    report_warnings(&warnings);

    let mut client = match SocketClient::connect(Path::new(&socket_path)) {
        Ok(client) => client,
        Err(e) => return fail_visibly(&format!("{e:#}")),
    };
    let snapshot = client
        .list_workspaces()
        .and_then(|workspaces| Ok((workspaces, client.list_tabs()?)));
    let (workspaces, tabs) = match snapshot {
        Ok(snapshot) => snapshot,
        Err(e) => return fail_visibly(&format!("{e:#}")),
    };

    let items = model::build_flat_list(&workspaces, &tabs);
    let cursor = model::initial_cursor(&items, context_tab_id().as_deref());
    let mut app = App::new(items, cursor);
    let hints = ui::FooterHints::from_keymap(&keymap);

    let mut terminal = ratatui::init();
    let selection = loop {
        if let Err(e) = terminal.draw(|frame| ui::draw(frame, &mut app, &hints)) {
            ratatui::restore();
            return fail_visibly(&format!("failed to draw: {e}"));
        }
        match event::read() {
            Ok(Event::Key(key)) => {
                if let Some(press) = KeyPress::from_crossterm(&key) {
                    match app.handle_key(&keymap, press) {
                        Outcome::Continue => {}
                        Outcome::Cancel => break None,
                        Outcome::FocusTab(tab_id) => break Some(tab_id),
                    }
                }
            }
            // Resize just needs the next draw; other events are ignored.
            Ok(_) => {}
            Err(_) => break None,
        }
    };
    ratatui::restore();

    if let Some(tab_id) = selection {
        if let Err(e) = client.focus_tab(&tab_id) {
            eprintln!("herdr-configurable-picker: {e:#}");
        }
    }
    // Exit 0 even on cancel: the overlay closing is the normal outcome, and
    // herdr raises a toast for non-zero exits.
    ExitCode::SUCCESS
}

/// The tab the picker was invoked from, out of HERDR_PLUGIN_CONTEXT_JSON.
/// Best effort: any missing piece just means the default cursor position.
fn context_tab_id() -> Option<String> {
    let context = std::env::var("HERDR_PLUGIN_CONTEXT_JSON").ok()?;
    let context: serde_json::Value = serde_json::from_str(&context).ok()?;
    Some(context.get("tab_id")?.as_str()?.to_string())
}

/// Stderr flashes and vanishes with the overlay pane, so warnings also go
/// to $HERDR_PLUGIN_STATE_DIR/picker.log where they can be read later.
fn report_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    for warning in warnings {
        eprintln!("herdr-configurable-picker: {warning}");
    }
    if let Some(state_dir) = std::env::var_os("HERDR_PLUGIN_STATE_DIR") {
        let path = PathBuf::from(state_dir).join("picker.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            for warning in warnings {
                let _ = writeln!(file, "{warning}");
            }
        }
    }
}

/// Startup failure inside the overlay: the pane closes as soon as we exit,
/// so hold the message on screen briefly. Exit 0 to avoid a duplicate toast.
fn fail_visibly(message: &str) -> ExitCode {
    report_warnings(&[message.to_string()]);
    std::thread::sleep(std::time::Duration::from_secs(3));
    ExitCode::SUCCESS
}
