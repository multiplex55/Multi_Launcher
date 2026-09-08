//! Revision-owned analysis. Environment I/O happens only on explicit refresh.
use crate::mkmacro::{
    MkDiagnostic, MkMacroDocument, StructureAnalysis, analyze_structure, validate_document,
};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub(super) struct AnalysisCache {
    revision: Option<u64>,
    diagnostics: Arc<Vec<MkDiagnostic>>,
    structures: HashMap<u64, Arc<StructureAnalysis>>,
    graph: crate::mkmacro::call_graph::CallGraph,
    root_failures: HashMap<u64, Option<String>>,
    environment: Vec<MkDiagnostic>,
    environment_document: Option<MkMacroDocument>,
    pub generation: u64,
    pub environment_pending: bool,
}

impl AnalysisCache {
    pub fn refresh_environment(&mut self, document: &MkMacroDocument, root: &std::path::Path) {
        let monitors = crate::mkmacro::monitor_descriptors();
        let diagnostics = crate::mkmacro::validate_document_with_context(
            document,
            crate::mkmacro::ValidationContext {
                asset_root: Some(root),
                monitors: match &monitors {
                    Ok(monitors) => crate::mkmacro::MonitorValidation::Available(monitors),
                    Err(_) => crate::mkmacro::MonitorValidation::EnumerationFailed,
                },
            },
        );
        let semantic = validate_document(document, None);
        self.environment = diagnostics
            .into_iter()
            .filter(|d| !semantic.contains(d))
            .collect();
        self.environment_document = Some(document.clone());
        self.revision = None;
    }

    fn ensure(&mut self, document: &MkMacroDocument, revision: u64) {
        if self.revision == Some(revision) {
            return;
        }
        self.environment_pending = self.environment_document.as_ref() != Some(document);
        let analysis = crate::mkmacro::analyze_document(document);
        let mut diagnostics = analysis.diagnostics;
        self.graph = analysis.graph;
        self.root_failures.clear();
        // Do not display an environment result for a removed or edited action.
        // Explicit Refresh checks new image/monitor references against the OS.
        diagnostics.extend(
            self.environment
                .iter()
                .filter(|d| {
                    let old = self
                        .environment_document
                        .as_ref()
                        .and_then(|doc| doc.macros.iter().find(|m| m.id == d.macro_id));
                    let new = document.macros.iter().find(|m| m.id == d.macro_id);
                    match (old, new, d.step_id) {
                        (Some(old), Some(new), Some(id)) => old
                            .steps
                            .iter()
                            .find(|s| s.id == id)
                            .zip(new.steps.iter().find(|s| s.id == id))
                            .is_some_and(|(a, b)| a.action == b.action),
                        (Some(old), Some(new), None) => old == new,
                        _ => false,
                    }
                })
                .cloned(),
        );
        self.diagnostics = Arc::new(diagnostics);
        self.structures = document
            .macros
            .iter()
            .map(|m| (m.id, Arc::new(analyze_structure(&m.steps))))
            .collect();
        self.revision = Some(revision);
        self.generation += 1;
    }

    pub fn diagnostics(
        &mut self,
        document: &MkMacroDocument,
        revision: u64,
    ) -> Arc<Vec<MkDiagnostic>> {
        self.ensure(document, revision);
        self.diagnostics.clone()
    }

    pub fn structure(
        &mut self,
        document: &MkMacroDocument,
        revision: u64,
        macro_id: u64,
    ) -> Option<Arc<StructureAnalysis>> {
        self.ensure(document, revision);
        self.structures.get(&macro_id).cloned()
    }

    pub fn root_failure(
        &mut self,
        document: &MkMacroDocument,
        revision: u64,
        root: u64,
    ) -> Option<String> {
        self.ensure(document, revision);
        self.root_failures
            .entry(root)
            .or_insert_with(|| {
                self.graph
                    .root_diagnostics(root, &self.diagnostics)
                    .into_iter()
                    .find(|d| d.severity == crate::mkmacro::DiagnosticSeverity::Fatal)
                    .map(|d| d.message.clone())
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_revision_reuses_analysis() {
        let document = MkMacroDocument::default();
        let mut cache = AnalysisCache::default();
        let first = cache.diagnostics(&document, 1);
        let next = cache.diagnostics(&document, 1);
        assert!(Arc::ptr_eq(&first, &next));
        assert_eq!(cache.generation, 1);
        assert!(!Arc::ptr_eq(&first, &cache.diagnostics(&document, 2)));
        assert_eq!(cache.generation, 2);
    }
    #[test]
    fn root_admission_is_scoped_and_cached_until_a_dependency_revision_changes() {
        use crate::mkmacro::*;
        let mut document: MkMacroDocument = serde_json::from_value(serde_json::json!({"macros":[
            {"id":1,"name":"root"}, {"id":2,"name":"unrelated","playback":{"speed_percent":0}}
        ]}))
        .unwrap();
        let mut cache = AnalysisCache::default();
        assert!(cache.root_failure(&document, 1, 1).is_none());
        assert!(cache.root_failure(&document, 1, 2).is_some());
        assert_eq!(cache.generation, 1);
        assert_eq!(cache.root_failures.len(), 2);
        for _ in 0..20 {
            assert!(cache.root_failure(&document, 1, 1).is_none());
        }
        assert_eq!(cache.generation, 1);
        document.macros[0].steps.push(
            serde_json::from_value(
                serde_json::json!({"id":1,"action":{"type":"call_macro","data":{"macro_id":2}}}),
            )
            .unwrap(),
        );
        assert!(cache.root_failure(&document, 2, 1).is_some());
        assert_eq!(cache.generation, 2);
        // An edited callee changes the cached document diagnostics on revision.
        let old = cache.diagnostics(&document, 2);
        document.macros[1].playback.speed_percent = 100;
        let new = cache.diagnostics(&document, 3);
        assert!(!Arc::ptr_eq(&old, &new));
        assert!(!new.iter().any(|d| d.code == "invalid_speed"));
        document.macros[1].id = 0;
        assert!(cache.root_failure(&document, 4, 1).is_some());
        assert!(
            cache
                .diagnostics(&document, 4)
                .iter()
                .any(|d| d.scope == DiagnosticScope::Document)
        );
    }
}
