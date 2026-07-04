//! ratatui rendering: bordered tree list, reverse-video cursor row, and a
//! footer hint line built from the *actual* keymap so it never lies about
//! bindings.

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::herdr_client::AgentStatus;
use crate::keymap::{Action, Keymap};
use crate::tree::{Row, RowKind};

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
        for (action, label) in [
            (Action::Expand, "expand"),
            (Action::Collapse, "collapse"),
            (Action::Accept, "accept"),
            (Action::Cancel, "cancel"),
        ] {
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
    let popup = popup_rect(frame.area());
    let outer = Block::bordered().title(" goto ");
    let inner = outer.inner(popup);
    frame.render_widget(outer, popup);

    let [list_area, footer_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    app.viewport_height = list_area.height;

    if app.rows().is_empty() {
        frame.render_widget(Paragraph::new("No workspaces found."), list_area);
    } else {
        let width = list_area.width as usize;
        let items: Vec<ListItem> = app
            .rows()
            .iter()
            .map(|row| ListItem::new(Line::from(row_text(row, width))))
            .collect();
        let list = List::new(items).highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        let mut state = ListState::default().with_selected(Some(app.cursor));
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    let footer = Paragraph::new(hints.line()).block(Block::new().borders(Borders::TOP));
    frame.render_widget(footer, footer_area);
}

/// Centered popup with the same margins as herdr's built-in goto
/// (`navigator_popup_rect`: width/16 and height/10, floored at 2 and 1), so
/// the picker keeps the modal geometry users know. A plugin pane cannot
/// float over other panes — herdr composites it as a regular (zoomed) pane —
/// so the surround is our blank canvas rather than see-through.
fn popup_rect(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let margin_x = (area.width / 16).max(2);
    let margin_y = (area.height / 10).max(1);
    let width = area.width.saturating_sub(margin_x * 2).max(4);
    let height = area.height.saturating_sub(margin_y * 2).max(4);
    ratatui::layout::Rect::new(area.x + margin_x, area.y + margin_y, width, height)
        .intersection(area)
}

/// One row: current marker, indentation, expansion glyph, and label on the
/// left; pane count / agent info right-aligned. Drops the right column on
/// narrow terminals rather than wrapping.
fn row_text(row: &Row, width: usize) -> String {
    let marker = if row.is_current { "→" } else { " " };
    let indent = "  ".repeat(row.depth as usize);
    let glyph = if row.expandable {
        if row.expanded {
            "▼ "
        } else {
            "▶ "
        }
    } else if row.kind == RowKind::Pane {
        ""
    } else {
        "· " // childless workspace/tab: nothing to expand
    };
    let left = format!("{marker} {indent}{glyph}{}", row.label);

    let status = status_label(row.agent_status);
    let right = if row.kind == RowKind::Pane {
        let agent = row.agent.as_deref().unwrap_or("shell");
        format!("{agent}  {status}")
    } else {
        let panes = if row.pane_count == 1 { "pane" } else { "panes" };
        format!("{} {panes}  {status}", row.pane_count)
    };

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
    use crate::app::EnterOnBranch;
    use crate::config::KeysConfig;
    use crate::herdr_client::{PaneInfo, TabInfo, WorkspaceInfo};
    use crate::tree::{InitialExpansion, Tree};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn workspace(id: &str, number: usize, label: &str, focused: bool) -> WorkspaceInfo {
        WorkspaceInfo {
            workspace_id: id.to_string(),
            number,
            label: label.to_string(),
            focused,
            pane_count: 0,
            tab_count: 0,
            active_tab_id: String::new(),
            agent_status: AgentStatus::Unknown,
        }
    }

    fn tab(id: &str, ws_id: &str, number: usize, label: &str, focused: bool) -> TabInfo {
        TabInfo {
            tab_id: id.to_string(),
            workspace_id: ws_id.to_string(),
            number,
            label: label.to_string(),
            focused,
            pane_count: 2,
            agent_status: AgentStatus::Working,
        }
    }

    fn pane(id: &str, tab_id: &str, ws_id: &str, focused: bool, agent: Option<&str>) -> PaneInfo {
        PaneInfo {
            pane_id: id.to_string(),
            tab_id: tab_id.to_string(),
            workspace_id: ws_id.to_string(),
            focused,
            agent: agent.map(|a| a.to_string()),
            display_agent: None,
            agent_status: AgentStatus::Idle,
            cwd: None,
            label: None,
            title: None,
            terminal_id: format!("term_{id}"),
        }
    }

    fn sample_app() -> App {
        let tree = Tree::build(
            vec![workspace("w1", 1, "mothership", true)],
            vec![tab("w1:t1", "w1", 1, "main", true)],
            vec![
                pane("w1:p1", "w1:t1", "w1", true, Some("claude")),
                pane("w1:p2", "w1:t1", "w1", false, None),
            ],
            InitialExpansion::All,
        );
        App::new(tree, EnterOnBranch::Jump)
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
    fn tree_rows_show_glyphs_indentation_and_right_columns() {
        let mut app = sample_app();
        let terminal = render(80, 24, &mut app);
        let screen = screen(&terminal);

        assert!(screen.contains("▼ mothership"), "screen:\n{screen}");
        assert!(screen.contains("  ▼ main"), "indented tab:\n{screen}");
        assert!(screen.contains("    pane 1"), "indented pane:\n{screen}");
        assert!(screen.contains("2 panes  working"), "tab column:\n{screen}");
        assert!(
            screen.contains("claude  idle"),
            "agent pane column:\n{screen}"
        );
        assert!(
            screen.contains("shell  idle"),
            "agentless column:\n{screen}"
        );
    }

    #[test]
    fn collapsed_branch_shows_the_collapsed_glyph() {
        let tree = Tree::build(
            vec![workspace("w1", 1, "mothership", true)],
            vec![tab("w1:t1", "w1", 1, "main", true)],
            vec![pane("w1:p1", "w1:t1", "w1", true, None)],
            InitialExpansion::None,
        );
        let mut app = App::new(tree, EnterOnBranch::Jump);
        let terminal = render(80, 24, &mut app);
        assert!(
            screen(&terminal).contains("▶ mothership"),
            "screen:\n{}",
            screen(&terminal)
        );
    }

    #[test]
    fn cursor_row_is_reversed() {
        let mut app = sample_app(); // cursor starts on the focused pane row
        let terminal = render(80, 24, &mut app);

        let buffer = terminal.backend().buffer();
        let lines = buffer_lines(&terminal);
        let cursor_y = lines
            .iter()
            .position(|line| line.contains("pane 1"))
            .expect("cursor row must be on screen") as u16;
        let x = lines[cursor_y as usize].find('p').unwrap() as u16;
        let style = buffer.cell((x, cursor_y)).unwrap().style();
        assert!(
            style.add_modifier.contains(Modifier::REVERSED),
            "cursor row must be reverse video, got {style:?}"
        );
    }

    #[test]
    fn current_row_carries_a_marker() {
        let mut app = sample_app();
        let terminal = render(80, 24, &mut app);
        let lines = buffer_lines(&terminal);

        let current = lines.iter().find(|l| l.contains("pane 1")).unwrap();
        assert!(current.contains("→"), "current row: {current:?}");
        let other = lines.iter().find(|l| l.contains("pane 2")).unwrap();
        assert!(!other.contains("→"), "other row: {other:?}");
    }

    #[test]
    fn footer_reflects_the_actual_keymap_not_the_defaults() {
        let (keymap, warnings) = Keymap::from_bindings(&[
            (Action::Down, vec!["ctrl+j".to_string()]),
            (Action::Up, vec!["ctrl+k".to_string()]),
            (Action::Expand, vec!["tab".to_string()]),
            (Action::Accept, vec!["ctrl+m".to_string()]),
            (Action::Cancel, vec!["q".to_string()]),
        ]);
        assert!(warnings.is_empty());
        let hints = FooterHints::from_keymap(&keymap);

        let mut app = sample_app();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| draw(frame, &mut app, &hints))
            .unwrap();
        let screen = screen(&terminal);

        assert!(screen.contains("C-k/C-j move"), "screen:\n{screen}");
        assert!(screen.contains("tab expand"), "screen:\n{screen}");
        assert!(screen.contains("q cancel"), "screen:\n{screen}");
        assert!(
            !screen.contains("collapse"),
            "unbound actions stay out of the footer:\n{screen}"
        );
    }

    #[test]
    fn empty_tree_shows_placeholder() {
        let tree = Tree::build(vec![], vec![], vec![], InitialExpansion::All);
        let mut app = App::new(tree, EnterOnBranch::Jump);
        let terminal = render(80, 24, &mut app);
        assert!(
            screen(&terminal).contains("No workspaces found."),
            "screen:\n{}",
            screen(&terminal)
        );
    }

    #[test]
    fn narrow_terminal_truncates_without_panicking() {
        let mut app = sample_app();
        let terminal = render(20, 6, &mut app);
        assert!(!screen(&terminal).is_empty());
    }

    #[test]
    fn cursor_far_down_stays_visible_and_viewport_height_is_recorded() {
        let panes: Vec<PaneInfo> = (1..=25)
            .map(|n| pane(&format!("w1:p{n}"), "w1:t1", "w1", n == 20, None))
            .collect();
        let tree = Tree::build(
            vec![workspace("w1", 1, "mothership", true)],
            vec![tab("w1:t1", "w1", 1, "main", true)],
            panes,
            InitialExpansion::All,
        );
        let mut app = App::new(tree, EnterOnBranch::Jump); // cursor on pane 20
        let terminal = render(80, 12, &mut app);
        let screen = screen(&terminal);

        assert!(
            screen.contains("pane 20"),
            "row under cursor must be scrolled into view:\n{screen}"
        );
        // 12 rows, popup margin 1 top+bottom -> 10, minus borders (2) and
        // footer (2) -> 6.
        assert_eq!(app.viewport_height, 6);
    }

    #[test]
    fn popup_is_centered_with_builtin_goto_margins() {
        let mut app = sample_app();
        let terminal = render(80, 24, &mut app);
        let lines = buffer_lines(&terminal);

        // 80x24: margin_x = max(80/16, 2) = 5, margin_y = max(24/10, 1) = 2.
        let char_at = |y: usize, x: usize| lines[y].chars().nth(x).unwrap();
        assert_eq!(char_at(2, 5), '┌', "top-left corner at (5,2)");
        assert_eq!(char_at(21, 5), '└', "bottom-left corner at (5,21)");
        assert_eq!(lines[0].trim(), "", "surround above the popup stays blank");
        assert!(
            lines[2].chars().take(5).all(|c| c == ' '),
            "surround left of the popup stays blank"
        );
    }
}
