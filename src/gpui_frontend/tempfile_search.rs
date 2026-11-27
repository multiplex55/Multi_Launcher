//! Bounded, generation-bound projection of checked temporary-file rows.
use super::*;
use multi_launcher::actions::tempfiles::ListedTempFile;
use multi_launcher::plugin::Plugin;

struct Request {
    query: String,
    generation: u64,
    revision: u64,
    other: Vec<(String, Action)>,
    enabled: Option<HashSet<String>>,
    caps: Option<HashMap<String, Vec<String>>>,
}

#[derive(Default)]
pub(super) struct Owner {
    active: bool,
    revision: u64,
    cancellation: Arc<AtomicU64>,
    pending: Option<Request>,
    projection: Option<(String, HashMap<String, Action>)>,
    published: Option<(String, u64, Vec<ListedTempFile>, Arc<AtomicBool>)>,
    diagnostic: Option<String>,
}
impl Owner {
    pub fn revision(&self) -> u64 { self.revision }
    pub fn retire(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.cancellation.fetch_add(1, Ordering::AcqRel);
        self.pending = None;
        self.projection = None;
        self.diagnostic = None;
        if let Some((_, _, _, authority)) = self.published.take() { authority.store(false, Ordering::Release); }
    }
    pub fn diagnostic(&self) -> Option<&String> { self.diagnostic.as_ref() }
    pub fn projected(&self, query: &str, action: &Action) -> bool {
        self.projection.as_ref().is_some_and(|(raw, rows)| raw == query && rows.get(&action.action) == Some(action))
    }
    pub fn receipt(&self, generation: u64, query: &str, action: &Action) -> Option<(ListedTempFile, Arc<AtomicBool>)> {
        let (raw, published, rows, authority) = self.published.as_ref()?;
        if raw != query || *published != generation || !authority.load(Ordering::Acquire) { return None; }
        rows.iter().find(|row| row.action == *action).cloned().map(|row| (row, authority.clone()))
    }
}
impl Drop for Owner { fn drop(&mut self) { self.retire(); } }
pub(super) fn enabled(plugins: Option<&HashSet<String>>, caps: Option<&HashMap<String, Vec<String>>>) -> bool {
    plugins.is_none_or(|plugins| plugins.contains("tempfile"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "tempfile")
}
pub(super) fn list_query(query: &str) -> bool {
    let (query, _) = multi_launcher::common::query::split_action_filters(query);
    multi_launcher::plugins::tempfile::list_filter(&query).is_some()
}
struct Rows<'a>(&'a [ListedTempFile]);
impl Plugin for Rows<'_> {
    fn name(&self) -> &str { "tempfile" }
    fn description(&self) -> &str { "Manage temporary files" }
    fn capabilities(&self) -> &[&str] { &["search"] }
    fn query_prefixes(&self) -> &[&str] { &["tmp"] }
    fn search(&self, _: &str) -> Vec<Action> { self.0.iter().map(|row| row.action.clone()).collect() }
}
impl RootWindow {
    pub(super) fn start_tempfile_search(&mut self, query: String, other: Vec<(String, Action)>, scroll: bool, cx: &mut Context<Self>) {
        self.publish_launcher_provider_results(query.clone(), other.clone(), scroll);
        self.tempfile_search.pending = Some(Request { query, generation: self.launcher_results.generation,
            revision: self.tempfile_search.revision, other, enabled: self.enabled_plugins.clone(), caps: self.enabled_capabilities.clone() });
        self.launch_tempfile_search(cx);
    }
    fn launch_tempfile_search(&mut self, cx: &mut Context<Self>) {
        if self.tempfile_search.active { return; }
        if self.restart_admission.is_fenced() { self.tempfile_search.retire(); return; }
        let Some(request) = self.tempfile_search.pending.take() else { return; };
        self.tempfile_search.active = true;
        let query = request.query.clone();
        let plugin_set = request.enabled.clone();
        let caps = request.caps.clone();
        let cancellation = self.tempfile_search.cancellation.clone();
        let expected = cancellation.load(Ordering::Acquire);
        let task = cx.background_executor().spawn(async move {
            let (effective, _) = multi_launcher::common::query::split_action_filters(&query);
            let rows = multi_launcher::actions::tempfiles::list_checked_in(&multi_launcher::plugins::tempfile::storage_dir(), &effective, || cancellation.load(Ordering::Acquire) == expected).map_err(|error| error.to_string())?;
            let routed = search_providers::search_routed([&Rows(&rows) as &dyn Plugin], &query, plugin_set.as_ref(), caps.as_ref());
            let accepted: HashSet<_> = routed.into_iter().filter(|(name, _)| name == "tempfile")
                .map(|(_, action)| (action.label, action.desc, action.action, action.args)).collect();
            Ok::<_, String>(rows.into_iter().filter(|row| accepted.contains(&(row.action.label.clone(), row.action.desc.clone(), row.action.action.clone(), row.action.args.clone()))).collect::<Vec<_>>())
        });
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let rows = task.await;
            let _ = root.update(cx, |root, cx| {
                root.tempfile_search.active = false;
                let current = root.tempfile_search.revision == request.revision
                    && root.launcher_results.generation == request.generation && root.launcher_results.query == request.query
                    && root.query.read(cx).value().as_str() == request.query
                    && root.root_visibility.active.load(Ordering::Acquire) && root.root_visibility.visible.load(Ordering::Acquire)
                    && !root.restart_admission.is_fenced() && enabled(root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref());
                if current { match rows { Ok(rows) => {
                    root.tempfile_search.diagnostic = None;
                    let mut combined = request.other;
                    let names: Vec<_> = root.search_catalog.providers.plugins().map(|plugin| plugin.name().to_owned()).collect();
                    let position = names.iter().position(|name| name == "tempfile").unwrap_or(names.len());
                    let index = combined.iter().position(|(name, _)| names.iter().position(|candidate| candidate == name).is_some_and(|i| i > position)).unwrap_or(combined.len());
                    for (offset, row) in rows.iter().enumerate() { combined.insert(index + offset, ("tempfile".into(), row.action.clone())); }
                    root.tempfile_search.projection = Some((request.query.clone(), rows.iter().map(|row| (row.action.action.clone(), row.action.clone())).collect()));
                    root.publish_launcher_provider_results(request.query.clone(), combined, false);
                    root.tempfile_search.projection = None;
                    root.tempfile_search.published = Some((request.query, root.launcher_results.generation, rows, Arc::new(AtomicBool::new(true))));
                    cx.notify();
                }, Err(error) => {
                    root.tempfile_search.diagnostic = Some(format!("Could not list temporary files: {error}"));
                    cx.notify();
                } } }
                root.launch_tempfile_search(cx);
            });
        }).detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{Owner, enabled, list_query};
    use std::sync::{Arc, atomic::AtomicBool};

    #[test]
    fn exact_stock_composition_and_retired_receipt() {
        let temporary = tempfile::tempdir().unwrap();
        let dir = temporary.path().join("owned");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("Café.txt"), b"owned").unwrap();
        let query = "tmp list CAFÉ";
        let rows = multi_launcher::actions::tempfiles::list_checked_in(&dir, query, || true).unwrap();
        let action = rows[0].action.clone();
        let catalog = super::super::tests::completion_catalog(Vec::new(), true, None);
        let composed = catalog.compose(query, vec![("tempfile".into(), action.clone())], false);
        assert!(composed.iter().any(|row| row.action == action));
        let mut owner = Owner::default();
        owner.diagnostic = Some("Could not list temporary files: denied".into());
        owner.published = Some((query.into(), 7, rows, Arc::new(AtomicBool::new(true))));
        assert!(owner.receipt(7, query, &action).is_some());
        assert!(owner.receipt(8, query, &action).is_none());
        assert!(owner.receipt(7, "tmp list other", &action).is_none());
        owner.retire();
        assert!(owner.diagnostic().is_none());
        assert!(owner.receipt(7, query, &action).is_none());
        assert!(list_query(query));
        assert!(!list_query("tmp open"));
        assert!(!enabled(Some(&std::collections::HashSet::new()), None));
    }
}
