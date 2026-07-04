//! Pure input state machine: keys go in, an [`Outcome`] comes out.
//! No terminal, no socket — fully unit-testable.

use crate::keymap::{Action, KeyPress, Keymap, Resolution};
use crate::tree::{FocusTarget, NodePath, Row, RowKind, Tree};

/// What the event loop should do after a key press.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Continue,
    Focus(FocusTarget),
    Cancel,
}

/// `[behavior] enter_on_branch` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterOnBranch {
    /// Accept on a workspace/tab jumps straight to it.
    Jump,
    /// Accept on a workspace/tab toggles its subtree instead.
    Expand,
}

impl EnterOnBranch {
    pub fn parse(text: &str) -> Option<EnterOnBranch> {
        match text {
            "jump" => Some(EnterOnBranch::Jump),
            "expand" => Some(EnterOnBranch::Expand),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct App {
    tree: Tree,
    /// Cache of `tree.visible_rows()`, rebuilt after every mutation.
    rows: Vec<Row>,
    pub cursor: usize,
    /// Keys buffered while a chord is in flight.
    pub pending: Vec<KeyPress>,
    /// Rows the list area can show; set by the UI on each draw so that
    /// page movements track the real terminal size.
    pub viewport_height: u16,
    enter_on_branch: EnterOnBranch,
}

impl App {
    pub fn new(tree: Tree, enter_on_branch: EnterOnBranch) -> App {
        let rows = tree.visible_rows();
        let cursor = tree.initial_cursor();
        App {
            tree,
            rows,
            cursor,
            pending: Vec::new(),
            viewport_height: 0,
            enter_on_branch,
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn handle_key(&mut self, keymap: &Keymap, key: KeyPress) -> Outcome {
        if self.rows.is_empty() {
            // SPEC "Empty tree": show the message, close on any key.
            return Outcome::Cancel;
        }
        match keymap.resolve(&self.pending, key) {
            Resolution::Action(action) => {
                self.pending.clear();
                self.apply(action)
            }
            Resolution::Pending => {
                self.pending.push(key);
                Outcome::Continue
            }
            Resolution::NoMatch => {
                // A failed chord swallows the key: firing its standalone
                // binding instead would be a surprising double meaning.
                self.pending.clear();
                Outcome::Continue
            }
        }
    }

    fn apply(&mut self, action: Action) -> Outcome {
        let last = self.rows.len() - 1;
        let page = (self.viewport_height as usize).max(1);
        let row = self.rows[self.cursor].clone();
        match action {
            Action::Down => self.cursor = (self.cursor + 1).min(last),
            Action::Up => self.cursor = self.cursor.saturating_sub(1),
            Action::PageDown => self.cursor = (self.cursor + page).min(last),
            Action::PageUp => self.cursor = self.cursor.saturating_sub(page),
            Action::Top => self.cursor = 0,
            Action::Bottom => self.cursor = last,
            Action::Expand => {
                if self.tree.expand(row.path) {
                    self.refresh_keeping(row.path);
                }
            }
            Action::Collapse => {
                if self.tree.collapse(row.path) {
                    self.refresh_keeping(row.path);
                } else if let Some(parent) = self.tree.parent_path(row.path) {
                    // Collapsing a leaf or an already-collapsed node walks
                    // up instead — the usual file-tree `h` behavior.
                    self.refresh_keeping(parent);
                }
            }
            Action::Toggle => {
                if self.tree.toggle(row.path) {
                    self.refresh_keeping(row.path);
                }
            }
            Action::Accept => {
                let is_branch = row.kind != RowKind::Pane;
                if is_branch && self.enter_on_branch == EnterOnBranch::Expand {
                    self.tree.toggle(row.path);
                    self.refresh_keeping(row.path);
                } else {
                    return Outcome::Focus(row.focus_target);
                }
            }
            Action::Cancel => return Outcome::Cancel,
        }
        Outcome::Continue
    }

    /// Rebuilds the visible rows and parks the cursor on `path` (which is
    /// always still visible after our mutations).
    fn refresh_keeping(&mut self, path: NodePath) {
        self.rows = self.tree.visible_rows();
        self.cursor = self
            .rows
            .iter()
            .position(|row| row.path == path)
            .unwrap_or_else(|| self.cursor.min(self.rows.len().saturating_sub(1)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KeysConfig;
    use crate::herdr_client::{AgentStatus, PaneInfo, TabInfo, WorkspaceInfo};
    use crate::keymap::parse_key_spec;
    use crate::tree::InitialExpansion;

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
            pane_count: 1,
            agent_status: AgentStatus::Idle,
        }
    }

    fn pane(id: &str, tab_id: &str, ws_id: &str, focused: bool) -> PaneInfo {
        PaneInfo {
            pane_id: id.to_string(),
            tab_id: tab_id.to_string(),
            workspace_id: ws_id.to_string(),
            focused,
            agent: None,
            display_agent: None,
            agent_status: AgentStatus::Idle,
            cwd: None,
            label: None,
            title: None,
            terminal_id: format!("term_{id}"),
        }
    }

    /// Rows with All expansion:
    /// 0 alpha / 1 a-one / 2 pane 1(focused) / 3 a-two / 4 pane 2 / 5 beta / 6 b-one / 7 pane 1
    fn tree(initial: InitialExpansion) -> Tree {
        Tree::build(
            vec![
                workspace("w1", 1, "alpha", true),
                workspace("w2", 2, "beta", false),
            ],
            vec![
                tab("w1:t1", "w1", 1, "a-one", true),
                tab("w1:t2", "w1", 2, "a-two", false),
                tab("w2:t1", "w2", 1, "b-one", true),
            ],
            vec![
                pane("w1:p1", "w1:t1", "w1", true),
                pane("w1:p2", "w1:t2", "w1", false),
                pane("w2:p1", "w2:t1", "w2", false),
            ],
            initial,
        )
    }

    fn app() -> App {
        let mut app = App::new(tree(InitialExpansion::All), EnterOnBranch::Jump);
        app.viewport_height = 3;
        app
    }

    fn default_keymap() -> Keymap {
        let (keymap, warnings) = Keymap::from_bindings(&KeysConfig::default().to_bindings());
        assert!(warnings.is_empty(), "default config must be warning-free");
        keymap
    }

    fn press(app: &mut App, keymap: &Keymap, spec: &str) -> Outcome {
        let keys = parse_key_spec(spec).unwrap();
        let mut outcome = Outcome::Continue;
        for key in keys.0 {
            outcome = app.handle_key(keymap, key);
        }
        outcome
    }

    fn cursor_label(app: &App) -> &str {
        &app.rows()[app.cursor].label
    }

    #[test]
    fn cursor_starts_on_the_current_row() {
        let app = app();
        assert_eq!(cursor_label(&app), "pane 1");
        assert_eq!(app.cursor, 2);
    }

    #[test]
    fn movement_moves_over_visible_rows_and_clamps() {
        let keymap = default_keymap();
        let mut app = app();

        press(&mut app, &keymap, "home");
        assert_eq!(app.cursor, 0);
        press(&mut app, &keymap, "up");
        assert_eq!(app.cursor, 0, "up clamps at the top");
        press(&mut app, &keymap, "j");
        press(&mut app, &keymap, "ctrl+n");
        assert_eq!(app.cursor, 2);
        press(&mut app, &keymap, "shift+g");
        assert_eq!(app.cursor, 7, "bottom hits the last visible row");
        press(&mut app, &keymap, "down");
        assert_eq!(app.cursor, 7, "down clamps at the bottom");
        press(&mut app, &keymap, "ctrl+u");
        assert_eq!(app.cursor, 4, "page up moves by viewport height");
    }

    #[test]
    fn collapse_hides_the_subtree_and_keeps_cursor_on_the_branch() {
        let keymap = default_keymap();
        let mut app = app();

        press(&mut app, &keymap, "home");
        assert_eq!(cursor_label(&app), "alpha");
        press(&mut app, &keymap, "h");
        assert_eq!(cursor_label(&app), "alpha", "cursor stays on the branch");
        // alpha, beta, b-one, pane 1 — alpha's own subtree is hidden.
        assert_eq!(app.rows().len(), 4);
    }

    #[test]
    fn collapse_on_a_leaf_walks_up_to_the_parent() {
        let keymap = default_keymap();
        let mut app = app(); // cursor on "pane 1"

        press(&mut app, &keymap, "h");
        assert_eq!(cursor_label(&app), "a-one", "pane -> its tab");
        press(&mut app, &keymap, "h"); // collapses a-one (it is expanded)
        assert_eq!(cursor_label(&app), "a-one");
        press(&mut app, &keymap, "h"); // now collapsed -> walks up
        assert_eq!(cursor_label(&app), "alpha");
    }

    #[test]
    fn expand_opens_a_collapsed_branch_in_place() {
        let keymap = default_keymap();
        let mut app = App::new(tree(InitialExpansion::None), EnterOnBranch::Jump);

        assert_eq!(app.rows().len(), 2);
        press(&mut app, &keymap, "l");
        assert_eq!(cursor_label(&app), "alpha", "cursor stays put");
        assert_eq!(app.rows().len(), 4, "alpha's tabs appeared");
        press(&mut app, &keymap, "l");
        assert_eq!(app.rows().len(), 4, "expanding again is a no-op");
    }

    #[test]
    fn toggle_flips_the_branch() {
        let keymap = default_keymap();
        let mut app = app();

        press(&mut app, &keymap, "home");
        press(&mut app, &keymap, "space");
        assert_eq!(app.rows().len(), 4);
        press(&mut app, &keymap, "space");
        assert_eq!(app.rows().len(), 8);
    }

    #[test]
    fn accept_on_a_pane_focuses_the_pane() {
        let keymap = default_keymap();
        let mut app = app();

        assert_eq!(
            press(&mut app, &keymap, "enter"),
            Outcome::Focus(FocusTarget::Pane("w1:p1".to_string()))
        );
    }

    #[test]
    fn accept_on_branches_jumps_when_configured_to_jump() {
        let keymap = default_keymap();
        let mut app = app();

        press(&mut app, &keymap, "home");
        assert_eq!(
            press(&mut app, &keymap, "enter"),
            Outcome::Focus(FocusTarget::Workspace("w1".to_string()))
        );
        press(&mut app, &keymap, "j");
        assert_eq!(
            press(&mut app, &keymap, "enter"),
            Outcome::Focus(FocusTarget::Tab("w1:t1".to_string()))
        );
    }

    #[test]
    fn accept_on_branches_toggles_when_configured_to_expand() {
        let keymap = default_keymap();
        let mut app = App::new(tree(InitialExpansion::All), EnterOnBranch::Expand);

        press(&mut app, &keymap, "home");
        assert_eq!(press(&mut app, &keymap, "enter"), Outcome::Continue);
        assert_eq!(app.rows().len(), 4, "enter collapsed the workspace");

        // Panes still jump.
        press(&mut app, &keymap, "space"); // reopen
        let mut app2 = App::new(tree(InitialExpansion::All), EnterOnBranch::Expand);
        assert_eq!(
            press(&mut app2, &keymap, "enter"),
            Outcome::Focus(FocusTarget::Pane("w1:p1".to_string()))
        );
    }

    #[test]
    fn cancel_and_empty_tree_behave_like_m1() {
        let keymap = default_keymap();
        let mut app = app();
        assert_eq!(press(&mut app, &keymap, "esc"), Outcome::Cancel);

        let mut empty = App::new(
            Tree::build(vec![], vec![], vec![], InitialExpansion::All),
            EnterOnBranch::Jump,
        );
        assert_eq!(press(&mut empty, &keymap, "x"), Outcome::Cancel);
    }

    #[test]
    fn enter_on_branch_parses_known_values_only() {
        assert_eq!(EnterOnBranch::parse("jump"), Some(EnterOnBranch::Jump));
        assert_eq!(EnterOnBranch::parse("expand"), Some(EnterOnBranch::Expand));
        assert_eq!(EnterOnBranch::parse("teleport"), None);
    }
}
