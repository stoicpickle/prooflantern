use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::{
    evaluate, load_project,
    model::{CurrentFocus, EvaluatedProject},
    ui,
};

const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefreshStatus {
    Idle,
    Refreshing,
    Updated { needs_recheck: usize },
    Failed { message: String },
}

type RefreshResult = Result<EvaluatedProject, String>;

#[derive(Debug)]
pub struct App {
    project: EvaluatedProject,
    selected: usize,
    inspector_open: bool,
    project_command_hints: bool,
    project_root: Option<PathBuf>,
    refresh_status: RefreshStatus,
    refresh_result: Option<thread::JoinHandle<RefreshResult>>,
    interrupted: Arc<AtomicBool>,
    should_quit: bool,
}

impl App {
    pub fn new(project: EvaluatedProject) -> Self {
        let focused_id = match project.current_focus() {
            CurrentFocus::Capability { capability, .. } => Some(capability.intent.id.clone()),
            CurrentFocus::Complete { .. } => None,
        };
        let selected = focused_id
            .and_then(|id| {
                project
                    .capabilities
                    .iter()
                    .position(|item| item.intent.id == id)
            })
            .unwrap_or(0);
        Self {
            project,
            selected,
            inspector_open: false,
            project_command_hints: false,
            project_root: None,
            refresh_status: RefreshStatus::Idle,
            refresh_result: None,
            interrupted: Arc::new(AtomicBool::new(false)),
            should_quit: false,
        }
    }

    pub fn project(&self) -> &EvaluatedProject {
        &self.project
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected(&self) -> Option<&crate::model::CapabilityAssessment> {
        self.project.capabilities.get(self.selected)
    }

    pub const fn inspector_open(&self) -> bool {
        self.inspector_open
    }

    pub const fn project_command_hints(&self) -> bool {
        self.project_command_hints
    }

    pub const fn with_project_command_hints(mut self) -> Self {
        self.project_command_hints = true;
        self
    }

    /// Bind reloads to a stable absolute path before entering terminal mode.
    /// Rendering an embedded demo alone never enables filesystem access.
    pub fn with_project_root(mut self, root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref().canonicalize()?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "project root is not a directory",
            ));
        }
        self.project_root = Some(root);
        self.project_command_hints = true;
        Ok(self)
    }

    pub fn can_refresh(&self) -> bool {
        self.project_root.is_some()
    }

    pub fn refresh_status(&self) -> &RefreshStatus {
        &self.refresh_status
    }

    /// One read-only worker at a time. Hashing never blocks the terminal loop.
    pub fn request_refresh(&mut self) -> bool {
        let Some(root) = self.project_root.clone() else {
            return false;
        };
        if self.refresh_result.is_some() {
            return false;
        }
        match thread::Builder::new()
            .name("proof-lantern-refresh".into())
            .spawn(move || {
                #[cfg(feature = "terminal-test-hooks")]
                refresh_test_probe();
                load_project(root)
                    .map_err(|error| error.to_string())
                    .and_then(|(spec, observations)| {
                        evaluate(spec, observations).map_err(|error| error.to_string())
                    })
            }) {
            Ok(worker) => {
                self.refresh_result = Some(worker);
                self.refresh_status = RefreshStatus::Refreshing;
            }
            Err(error) => self.finish_refresh(Err(format!("could not start refresh: {error}"))),
        }
        true
    }

    /// Call on each terminal tick, including ticks without keyboard events.
    pub fn poll_refresh(&mut self) -> bool {
        let Some(worker) = self.refresh_result.as_ref() else {
            return false;
        };
        if !worker.is_finished() {
            return false;
        }
        let result = match self
            .refresh_result
            .take()
            .expect("worker is present")
            .join()
        {
            Ok(result) => result,
            // Ratatui's global panic hook already restores the terminal, even
            // for a worker panic. Unwind instead of redrawing in restored mode.
            Err(payload) => std::panic::resume_unwind(payload),
        };
        self.finish_refresh(result);
        true
    }

    fn finish_refresh(&mut self, result: RefreshResult) {
        match result {
            Ok(project) => {
                let selected_id = self
                    .selected()
                    .map(|capability| capability.intent.id.clone());
                let needs_recheck = project
                    .capabilities
                    .iter()
                    .filter(|capability| capability.needs_recheck())
                    .count();
                self.project = project;
                self.selected = selected_id
                    .as_ref()
                    .and_then(|id| {
                        self.project
                            .capabilities
                            .iter()
                            .position(|capability| &capability.intent.id == id)
                    })
                    .unwrap_or_else(|| match self.project.current_focus() {
                        CurrentFocus::Capability { capability, .. } => self
                            .project
                            .capabilities
                            .iter()
                            .position(|item| item.intent.id == capability.intent.id)
                            .unwrap_or(0),
                        CurrentFocus::Complete { .. } => 0,
                    });
                self.refresh_status = RefreshStatus::Updated { needs_recheck };
            }
            Err(message) => {
                // Show the diagnostic, not a parser's multiline source excerpt.
                // Full details remain available via the plain `next` command.
                let message = message
                    .lines()
                    .next()
                    .unwrap_or("refresh failed")
                    .chars()
                    .map(|c| {
                        if c.is_control()
                            || matches!(c, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
                        {
                            ' '
                        } else {
                            c
                        }
                    })
                    .take(2048)
                    .collect::<String>();
                self.refresh_status = RefreshStatus::Failed { message };
            }
        }
    }

    pub fn with_interrupt_flag(mut self, interrupted: Arc<AtomicBool>) -> Self {
        self.interrupted = interrupted;
        self
    }

    pub fn next_node(&mut self) -> bool {
        if self.project.capabilities.is_empty() {
            return false;
        }
        self.selected = (self.selected + 1) % self.project.capabilities.len();
        true
    }

    pub fn previous_node(&mut self) -> bool {
        if self.project.capabilities.is_empty() {
            return false;
        }
        self.selected = self
            .selected
            .checked_sub(1)
            .unwrap_or(self.project.capabilities.len() - 1);
        true
    }

    pub fn select_focus(&mut self) -> bool {
        let focused_id = match self.project.current_focus() {
            CurrentFocus::Capability { capability, .. } => capability.intent.id.clone(),
            CurrentFocus::Complete { .. } => return false,
        };
        let Some(index) = self
            .project
            .capabilities
            .iter()
            .position(|item| item.intent.id == focused_id)
        else {
            return false;
        };
        let changed = self.selected != index;
        self.selected = index;
        changed
    }

    pub fn select_id(&mut self, id: &str) -> bool {
        let Some(index) = self
            .project
            .capabilities
            .iter()
            .position(|item| item.intent.id == id)
        else {
            return false;
        };
        let changed = self.selected != index;
        self.selected = index;
        changed
    }

    pub fn toggle_inspector(&mut self) -> bool {
        self.inspector_open = !self.inspector_open;
        true
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        terminal.draw(|frame| ui::render(frame, &self))?;
        while !self.should_quit && !self.interrupted.load(Ordering::Relaxed) {
            let mut changed = self.poll_refresh();
            if event::poll(EVENT_POLL_INTERVAL)? {
                changed |= self.handle_event(event::read()?);
            }
            if changed && !self.should_quit {
                terminal.draw(|frame| ui::render(frame, &self))?;
            }
        }
        Ok(())
    }

    pub fn handle_event(&mut self, event: Event) -> bool {
        let Event::Key(key) = event else {
            return matches!(event, Event::Resize(_, _));
        };
        if key.kind != KeyEventKind::Press {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return true;
        }
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                true
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('j') => self.next_node(),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('k') => self.previous_node(),
            KeyCode::Enter | KeyCode::Char('e') => self.toggle_inspector(),
            KeyCode::Esc if self.inspector_open => self.toggle_inspector(),
            KeyCode::Char('g') => self.select_focus(),
            KeyCode::Char('r') => self.request_refresh(),
            _ => false,
        }
    }
}

#[cfg(feature = "terminal-test-hooks")]
fn refresh_test_probe() {
    match std::env::var("PROOF_LANTERN_TEST_REFRESH").as_deref() {
        Ok("pause") => thread::sleep(Duration::from_secs(2)),
        Ok("panic") => panic!("injected refresh worker panic"),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crossterm::event::{KeyEvent, KeyEventState};

    use super::*;
    use crate::{evaluate, load_project};

    fn app() -> App {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box");
        let (spec, observations) = load_project(root).unwrap();
        App::new(evaluate(spec, observations).unwrap())
    }

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn navigation_inspector_and_focus_selection_are_deterministic() {
        let mut app = app();
        assert_eq!(app.selected().unwrap().intent.id, "reopen");
        assert!(app.handle_event(press(KeyCode::Right)));
        assert_eq!(app.selected().unwrap().intent.id, "find");
        assert!(app.handle_event(press(KeyCode::Char('e'))));
        assert!(app.inspector_open());
        assert!(app.handle_event(press(KeyCode::Char('g'))));
        assert_eq!(app.selected().unwrap().intent.id, "reopen");
        assert!(app.handle_event(press(KeyCode::Esc)));
        assert!(!app.inspector_open());
    }

    #[test]
    fn pending_refresh_does_not_block_navigation_resize_or_quitting() {
        for exit in [KeyCode::Char('q'), KeyCode::Char('c')] {
            let mut app = app();
            let (_sender, receiver) = std::sync::mpsc::sync_channel::<()>(1);
            app.refresh_result = Some(thread::spawn(move || {
                let _ = receiver.recv();
                Err("test worker ended".into())
            }));
            app.refresh_status = RefreshStatus::Refreshing;
            assert!(!app.poll_refresh());
            assert!(app.handle_event(press(KeyCode::Right)));
            assert_eq!(app.selected().unwrap().intent.id, "find");
            assert!(app.handle_event(Event::Resize(100, 30)));
            let mut event = press(exit);
            if exit == KeyCode::Char('c')
                && let Event::Key(key) = &mut event
            {
                key.modifiers = KeyModifiers::CONTROL;
            }
            assert!(app.handle_event(event));
            assert!(app.should_quit);
        }
    }

    #[test]
    fn error_notice_is_terminal_safe_and_visible_in_every_layout() {
        use ratatui::{Terminal, backend::TestBackend};
        let mut app = app();
        app.finish_refresh(Err(
            "invalid project YAML: unsafe\x1b[31m\u{202e}source\nsource excerpt".into(),
        ));
        let RefreshStatus::Failed { message } = app.refresh_status() else {
            panic!("expected failure notice")
        };
        assert!(!message.chars().any(char::is_control));
        assert!(!message.contains('\u{202e}'));
        assert!(!message.contains("source excerpt"));
        for (width, height) in [(80, 24), (100, 30), (140, 40)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| ui::render(frame, &app)).unwrap();
            let screen: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(
                screen.contains("REFRESH FAILED / OUTDATED SNAPSHOT"),
                "{screen}"
            );
            assert!(screen.contains("invalid project YAML"), "{screen}");
            assert!(screen.contains("R retry"), "{screen}");
        }
    }
}
