use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::{
    model::{CurrentFocus, EvaluatedProject},
    ui,
};

const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub struct App {
    project: EvaluatedProject,
    selected: usize,
    inspector_open: bool,
    project_command_hints: bool,
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
            if event::poll(EVENT_POLL_INTERVAL)? {
                let changed = self.handle_event(event::read()?);
                if changed && !self.should_quit {
                    terminal.draw(|frame| ui::render(frame, &self))?;
                }
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
            _ => false,
        }
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
}
