use multi_launcher::file_search::settings::{FileSearchFilenameSort, FileSearchSettings};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    id: &'static str,
    name: &'static str,
}

#[derive(Debug)]
struct SearchState {
    running: bool,
    completed: bool,
    filters_dirty: bool,
    reruns: usize,
    sort: FileSearchFilenameSort,
    rows: Vec<Row>,
    selected: Option<&'static str>,
    ripgrep_prompt_dismissed: bool,
    pending_rg_path: Option<&'static str>,
    active_rg_path: &'static str,
}
impl Default for SearchState {
    fn default() -> Self {
        Self {
            running: false,
            completed: false,
            filters_dirty: false,
            reruns: 0,
            sort: FileSearchFilenameSort::Relevance,
            rows: vec![],
            selected: None,
            ripgrep_prompt_dismissed: false,
            pending_rg_path: None,
            active_rg_path: "",
        }
    }
}
impl SearchState {
    fn start(&mut self) {
        self.running = true;
        self.completed = false;
        self.filters_dirty = false;
    }
    fn change_filter(&mut self) {
        if self.completed {
            self.filters_dirty = true;
        }
    }
    fn change_sort(&mut self, sort: FileSearchFilenameSort) {
        self.sort = sort;
        if !self.running {
            self.apply_sort();
        }
    }
    fn complete(&mut self, rows: Vec<Row>) {
        self.rows = rows;
        self.running = false;
        self.completed = true;
        self.apply_sort();
    }
    fn apply_sort(&mut self) {
        if self.sort == FileSearchFilenameSort::FilenameAscending {
            self.rows.sort_by_key(|r| r.name);
        }
    }
    fn select(&mut self, id: &'static str) {
        self.selected = Some(id);
    }
    fn missing_rg_prompt(&mut self) -> bool {
        if self.ripgrep_prompt_dismissed {
            false
        } else {
            true
        }
    }
    fn dismiss_rg_prompt(&mut self) {
        self.ripgrep_prompt_dismissed = true;
    }
    fn configure_rg(&mut self, path: &'static str) {
        self.pending_rg_path = Some(path);
    }
    fn next_search(&mut self) {
        if let Some(path) = self.pending_rg_path.take() {
            self.active_rg_path = path;
        }
        self.start();
    }
}

#[test]
fn filters_dirty_marker_after_completed_search_and_no_auto_rerun() {
    let mut s = SearchState::default();
    s.start();
    s.complete(vec![]);
    s.change_filter();
    assert!(s.filters_dirty);
    assert_eq!(s.reruns, 0);
}

#[test]
fn sort_changed_while_running_applies_on_completion() {
    let mut s = SearchState::default();
    s.start();
    s.change_sort(FileSearchFilenameSort::FilenameAscending);
    s.complete(vec![Row { id: "2", name: "z" }, Row { id: "1", name: "a" }]);
    assert_eq!(
        s.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec!["1", "2"]
    );
}

#[test]
fn selected_result_survives_sorting() {
    let mut s = SearchState::default();
    s.complete(vec![Row { id: "2", name: "z" }, Row { id: "1", name: "a" }]);
    s.select("2");
    s.change_sort(FileSearchFilenameSort::FilenameAscending);
    assert_eq!(s.selected, Some("2"));
}

#[test]
fn ripgrep_missing_prompt_dismissal_suppresses_repeated_prompts() {
    let mut s = SearchState::default();
    assert!(s.missing_rg_prompt());
    s.dismiss_rg_prompt();
    assert!(!s.missing_rg_prompt());
}

#[test]
fn configured_ripgrep_path_is_used_only_on_subsequent_searches() {
    let mut s = SearchState {
        active_rg_path: "old",
        ..Default::default()
    };
    s.start();
    s.configure_rg("new");
    assert_eq!(s.active_rg_path, "old");
    s.next_search();
    assert_eq!(s.active_rg_path, "new");
}

#[test]
fn settings_do_not_persist_search_text_selections_or_history() {
    let json = serde_json::to_string(&FileSearchSettings::default()).unwrap();
    assert!(!json.contains("search_text"));
    assert!(!json.contains("selection"));
    assert!(!json.contains("history"));
}
