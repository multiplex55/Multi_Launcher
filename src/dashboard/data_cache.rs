use crate::actions::Action;
use crate::plugin::PluginManager;
use crate::plugins::clipboard::{load_history, CLIPBOARD_FILE};
use crate::plugins::fav::{load_favs, FavEntry, FAV_FILE};
use crate::plugins::note::{load_notes, Note};
use crate::plugins::snippets::{load_snippets, SnippetEntry, SNIPPETS_FILE};
use crate::plugins::todo::{load_todos, TodoEntry, TODO_FILE};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct DashboardDataSnapshot {
    pub clipboard_history: Arc<Vec<String>>,
    pub snippets: Arc<Vec<SnippetEntry>>,
    pub notes: Arc<Vec<Note>>,
    pub todos: Arc<Vec<TodoEntry>>,
    pub processes: Arc<Vec<Action>>,
    pub favorites: Arc<Vec<FavEntry>>,
    pub process_error: Option<String>,
}

impl Default for DashboardDataSnapshot {
    fn default() -> Self {
        Self {
            clipboard_history: Arc::new(Vec::new()),
            snippets: Arc::new(Vec::new()),
            notes: Arc::new(Vec::new()),
            todos: Arc::new(Vec::new()),
            processes: Arc::new(Vec::new()),
            favorites: Arc::new(Vec::new()),
            process_error: None,
        }
    }
}

impl DashboardDataSnapshot {
    fn with_clipboard_history(&self, history: Vec<String>) -> Self {
        Self {
            clipboard_history: Arc::new(history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
        }
    }

    fn with_snippets(&self, snippets: Vec<SnippetEntry>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::new(snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
        }
    }

    fn with_notes(&self, notes: Vec<Note>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::new(notes),
            todos: Arc::clone(&self.todos),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
        }
    }

    fn with_todos(&self, todos: Vec<TodoEntry>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::new(todos),
            processes: Arc::clone(&self.processes),
            favorites: Arc::clone(&self.favorites),
            process_error: self.process_error.clone(),
        }
    }

    fn with_favorites(&self, favorites: Vec<FavEntry>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            processes: Arc::clone(&self.processes),
            favorites: Arc::new(favorites),
            process_error: self.process_error.clone(),
        }
    }

    fn with_processes(&self, processes: Vec<Action>, process_error: Option<String>) -> Self {
        Self {
            clipboard_history: Arc::clone(&self.clipboard_history),
            snippets: Arc::clone(&self.snippets),
            notes: Arc::clone(&self.notes),
            todos: Arc::clone(&self.todos),
            processes: Arc::new(processes),
            favorites: Arc::clone(&self.favorites),
            process_error,
        }
    }
}

struct DashboardDataState {
    snapshot: Arc<DashboardDataSnapshot>,
    last_process_refresh: Instant,
}

pub struct DashboardDataCache {
    state: Mutex<DashboardDataState>,
}

impl DashboardDataCache {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(DashboardDataState {
                snapshot: Arc::new(DashboardDataSnapshot::default()),
                last_process_refresh: Instant::now() - Duration::from_secs(60),
            }),
        }
    }

    pub fn snapshot(&self) -> Arc<DashboardDataSnapshot> {
        self.state
            .lock()
            .map(|state| Arc::clone(&state.snapshot))
            .unwrap_or_else(|_| Arc::new(DashboardDataSnapshot::default()))
    }

    pub fn refresh_all(&self, plugins: &PluginManager) {
        self.refresh_clipboard();
        self.refresh_snippets();
        self.refresh_notes();
        self.refresh_todos();
        self.refresh_favorites();
        self.refresh_processes(plugins);
    }

    pub fn refresh_clipboard(&self) {
        let history = load_history(CLIPBOARD_FILE)
            .unwrap_or_default()
            .into_iter()
            .collect();
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_clipboard_history(history));
        }
    }

    pub fn refresh_snippets(&self) {
        let snippets = load_snippets(SNIPPETS_FILE).unwrap_or_default();
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_snippets(snippets));
        }
    }

    pub fn refresh_notes(&self) {
        let notes = load_notes().unwrap_or_default();
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_notes(notes));
        }
    }

    pub fn refresh_todos(&self) {
        let todos = load_todos(TODO_FILE).unwrap_or_default();
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_todos(todos));
        }
    }

    pub fn refresh_favorites(&self) {
        let favorites = load_favs(FAV_FILE).unwrap_or_default();
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_favorites(favorites));
        }
    }

    pub fn maybe_refresh_processes(&self, plugins: &PluginManager, interval: Duration) {
        let should_refresh = self
            .state
            .lock()
            .map(|state| state.last_process_refresh.elapsed() >= interval)
            .unwrap_or(false);
        if should_refresh {
            self.refresh_processes(plugins);
        }
    }

    pub fn refresh_processes(&self, plugins: &PluginManager) {
        let (processes, error) = Self::load_processes(plugins);
        if let Ok(mut state) = self.state.lock() {
            state.snapshot = Arc::new(state.snapshot.with_processes(processes, error));
            state.last_process_refresh = Instant::now();
        }
    }

    fn load_processes(plugins: &PluginManager) -> (Vec<Action>, Option<String>) {
        let plugin = plugins.iter().find(|p| p.name() == "processes");
        if let Some(plugin) = plugin {
            (plugin.search("ps"), None)
        } else {
            (Vec::new(), Some("Processes plugin not available.".into()))
        }
    }
}

impl Default for DashboardDataCache {
    fn default() -> Self {
        Self::new()
    }
}
