//! ratatui rendering: bordered list, reverse-video cursor row, and a footer
//! hint line built from the *actual* keymap so it never lies about bindings.

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::herdr_client::AgentStatus;
use crate::keymap::{Action, Keymap};
use crate::model::Item;

/// Footer entries as `(key label, action label)` pairs, e.g. `("↑/↓", "move")`.
pub struct FooterHints {
    pub entries: Vec<(String, String)>,
}

impl FooterHints {
    /// Reads the first bound key per action out of the keymap, so the hint
    /// line reflects the user's config, not our defaults.
    pub fn from_keymap(keymap: &Keymap) -> FooterHints {
        let mut entries = Vec::new();
        let up = keymap.first_binding_label(Action::Up);
        let down = keymap.first_binding_label(Action::Down);
        match (up, down) {
            (Some(up), Some(down)) => entries.push((format!("{up}/{down}"), "move".to_string())),
            (Some(key), None) | (None, Some(key)) => entries.push((key, "move".to_string())),
            (None, None) => {}
        }
        for (action, label) in [(Action::Accept, "accept"), (Action::Cancel, "cancel")] {
            if let Some(key) = keymap.first_binding_label(action) {
                entries.push((key, label.to_string()));
            }
        }
        FooterHints { entries }
    }

    fn line(&self) -> String {
        self.entries
            .iter()
            .map(|(key, action)| format!("{key} {action}"))
            .collect::<Vec<_>>()
            .join("   ")
    }
}

pub fn draw(frame: &mut Frame, app: &mut App, hints: &FooterHints) {
    let outer = Block::bordered().title(" goto ");
    let inner = outer.inner(frame.area());
    frame.render_widget(outer, frame.area());

    let [list_area, footer_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    app.viewport_height = list_area.height;

    if app.items.is_empty() {
        frame.render_widget(Paragraph::new("No workspaces found."), list_area);
    } else {
        let width = list_area.width as usize;
        let rows: Vec<ListItem> = app
            .items
            .iter()
            .map(|item| ListItem::new(Line::from(row_text(item, width))))
            .collect();
        let list = List::new(rows).highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        let mut state = ListState::default().with_selected(Some(app.cursor));
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    let footer = Paragraph::new(hints.line()).block(Block::new().borders(Borders::TOP));
    frame.render_widget(footer, footer_area);
}

/// One row: current-tab marker + labels on the left, pane count and agent
/// status right-aligned. Truncates the left side on narrow terminals.
fn row_text(item: &Item, width: usize) -> String {
    let marker = if item.is_current { "→" } else { " " };
    let left = format!("{marker} {} › {}", item.workspace_label, item.tab_label);
    let panes = if item.pane_count == 1 {
        "pane"
    } else {
        "panes"
    };
    let right = format!(
        "{} {panes}  {}",
        item.pane_count,
        status_label(item.agent_status)
    );

    let left_cols = left.chars().count();
    let right_cols = right.chars().count();
    if left_cols + right_cols + 2 <= width {
        let padding = width - left_cols - right_cols;
        format!("{left}{}{right}", " ".repeat(padding))
    } else {
        // Not enough room for both: keep the labels, drop the right column.
        left.chars().take(width).collect()
    }
}

fn status_label(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Idle => "idle",
        AgentStatus::Working => "working",
        AgentStatus::Blocked => "blocked",
        AgentStatus::Done => "done",
        AgentStatus::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KeysConfig;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn item(ws: &str, tab: &str, pane_count: usize, current: bool) -> Item {
        Item {
            tab_id: format!("{ws}:{tab}"),
            workspace_label: ws.to_string(),
            tab_label: tab.to_string(),
            pane_count,
            agent_status: AgentStatus::Working,
            is_current: current,
        }
    }

    fn default_hints() -> FooterHints {
        let (keymap, _) = Keymap::from_bindings(&KeysConfig::default().to_bindings());
        FooterHints::from_keymap(&keymap)
    }

    fn render(width: u16, height: u16, app: &mut App) -> Terminal<TestBackend> {
        let hints = default_hints();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, app, &hints)).unwrap();
        terminal
    }

    fn buffer_lines(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buffer = terminal.backend().buffer();
        let area = buffer.area;
        (area.top()..area.bottom())
            .map(|y| {
                (area.left()..area.right())
                    .map(|x| buffer.cell((x, y)).unwrap().symbol())
                    .collect()
            })
            .collect()
    }

    fn screen(terminal: &Terminal<TestBackend>) -> String {
        buffer_lines(terminal).join("\n")
    }

    #[test]
    fn rows_show_labels_pane_count_and_status() {
        let mut app = App::new(
            vec![
                item("mothership", "main", 2, false),
                item("herdr", "dev", 1, false),
            ],
            0,
        );
        let terminal = render(80, 24, &mut app);
        let screen = screen(&terminal);

        assert!(screen.contains("mothership › main"), "screen:\n{screen}");
        assert!(screen.contains("2 panes"), "screen:\n{screen}");
        assert!(screen.contains("1 pane "), "screen:\n{screen}");
        assert!(screen.contains("working"), "screen:\n{screen}");
    }

    #[test]
    fn cursor_row_is_reversed() {
        let mut app = App::new(
            vec![item("a", "one", 1, false), item("b", "two", 1, false)],
            1,
        );
        let terminal = render(80, 24, &mut app);

        let buffer = terminal.backend().buffer();
        let lines = buffer_lines(&terminal);
        let cursor_y = lines
            .iter()
            .position(|line| line.contains("b › two"))
            .expect("cursor row must be on screen") as u16;
        let x = lines[cursor_y as usize].find('b').unwrap() as u16;
        let style = buffer.cell((x, cursor_y)).unwrap().style();
        assert!(
            style.add_modifier.contains(Modifier::REVERSED),
            "cursor row must be reverse video, got {style:?}"
        );

        let other_y = lines
            .iter()
            .position(|line| line.contains("a › one"))
            .unwrap() as u16;
        let x = lines[other_y as usize].find('a').unwrap() as u16;
        let style = buffer.cell((x, other_y)).unwrap().style();
        assert!(
            !style.add_modifier.contains(Modifier::REVERSED),
            "non-cursor rows must not be reversed"
        );
    }

    #[test]
    fn current_tab_carries_a_marker() {
        let mut app = App::new(
            vec![item("a", "one", 1, false), item("b", "two", 1, true)],
            0,
        );
        let terminal = render(80, 24, &mut app);
        let lines = buffer_lines(&terminal);

        let current = lines.iter().find(|l| l.contains("b › two")).unwrap();
        assert!(current.contains("→"), "current row: {current:?}");
        let other = lines.iter().find(|l| l.contains("a › one")).unwrap();
        assert!(!other.contains("→"), "other row: {other:?}");
    }

    #[test]
    fn footer_reflects_the_actual_keymap_not_the_defaults() {
        let (keymap, warnings) = Keymap::from_bindings(&[
            (Action::Down, vec!["ctrl+j".to_string()]),
            (Action::Up, vec!["ctrl+k".to_string()]),
            (Action::Accept, vec!["tab".to_string()]),
            (Action::Cancel, vec!["q".to_string()]),
        ]);
        assert!(warnings.is_empty());
        let hints = FooterHints::from_keymap(&keymap);

        let mut app = App::new(vec![item("a", "one", 1, false)], 0);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| draw(frame, &mut app, &hints))
            .unwrap();
        let screen = screen(&terminal);

        assert!(screen.contains("C-k/C-j move"), "screen:\n{screen}");
        assert!(screen.contains("tab accept"), "screen:\n{screen}");
        assert!(screen.contains("q cancel"), "screen:\n{screen}");
    }

    #[test]
    fn empty_list_shows_placeholder() {
        let mut app = App::new(Vec::new(), 0);
        let terminal = render(80, 24, &mut app);
        assert!(
            screen(&terminal).contains("No workspaces found."),
            "screen:\n{}",
            screen(&terminal)
        );
    }

    #[test]
    fn narrow_terminal_truncates_without_panicking() {
        let mut app = App::new(
            vec![item(
                "a-very-long-workspace-label",
                "long-tab-name",
                12,
                true,
            )],
            0,
        );
        let terminal = render(20, 6, &mut app);
        // Rendering finished without panicking; the row is truncated to fit.
        assert!(!screen(&terminal).is_empty());
    }

    #[test]
    fn cursor_far_down_stays_visible_and_viewport_height_is_recorded() {
        let items: Vec<Item> = (1..=25)
            .map(|n| item("ws", &format!("tab{n:02}"), 1, false))
            .collect();
        let mut app = App::new(items, 20);
        let terminal = render(80, 12, &mut app);
        let screen = screen(&terminal);

        assert!(
            screen.contains("tab21"),
            "row under cursor (index 20) must be scrolled into view:\n{screen}"
        );
        // 12 rows minus top/bottom border minus 2 footer rows.
        assert_eq!(app.viewport_height, 8);
    }
}
