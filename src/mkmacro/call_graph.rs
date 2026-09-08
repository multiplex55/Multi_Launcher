//! Document identity and dependency discovery. Traversals are iterative and use
//! source order; hash maps are only used for lookup, never diagnostic ordering.
use super::{model::*, validation::*};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyPolicy {
    EnabledCalls,
    AllAuthoredCalls,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallEdge {
    pub caller_id: u64,
    pub step_id: u64,
    pub target_id: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CallGraph {
    order: Vec<u64>,
    indices: HashMap<u64, usize>,
    names: HashMap<u64, String>,
    enabled: HashMap<u64, bool>,
    edges: HashMap<u64, Vec<CallEdge>>,
    identity_diagnostics: Vec<MkDiagnostic>,
}

impl CallGraph {
    pub fn build(document: &MkMacroDocument) -> Self {
        let mut graph = Self::default();
        for (index, owner) in document.macros.iter().enumerate() {
            if owner.id == 0 || graph.indices.contains_key(&owner.id) {
                let mut diagnostic = MkDiagnostic::fatal(
                    owner.id,
                    None,
                    "invalid_macro_id",
                    "Macro IDs must be non-zero and unique",
                );
                diagnostic.scope = DiagnosticScope::Document;
                graph.identity_diagnostics.push(diagnostic);
            }
            if graph.indices.contains_key(&owner.id) {
                continue;
            }
            graph.order.push(owner.id);
            graph.indices.insert(owner.id, index);
            graph.names.insert(owner.id, owner.name.clone());
            graph.enabled.insert(owner.id, owner.enabled);
            graph.edges.insert(
                owner.id,
                owner
                    .steps
                    .iter()
                    .filter_map(|step| match &step.action {
                        MkAction::CallMacro(call) => Some(CallEdge {
                            caller_id: owner.id,
                            step_id: step.id,
                            target_id: call.macro_id,
                            enabled: step.enabled,
                        }),
                        _ => None,
                    })
                    .collect(),
            );
        }
        graph
    }

    pub fn macro_index(&self, id: u64) -> Option<usize> {
        self.indices.get(&id).copied()
    }
    pub fn edges(&self, id: u64) -> &[CallEdge] {
        self.edges.get(&id).map(Vec::as_slice).unwrap_or_default()
    }
    pub fn identity_diagnostics(&self) -> &[MkDiagnostic] {
        &self.identity_diagnostics
    }

    /// Includes missing targets so caller-owned edge findings cannot disappear
    /// during admission. Callers must validate before resolving those identities.
    pub fn closure(&self, root: u64, policy: DependencyPolicy) -> Vec<u64> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            result.push(id);
            for edge in self
                .edges(id)
                .iter()
                .rev()
                .filter(|edge| edge.enabled || policy == DependencyPolicy::AllAuthoredCalls)
            {
                pending.push(edge.target_id);
            }
        }
        result
    }

    pub fn root_diagnostics<'a>(
        &self,
        root: u64,
        diagnostics: &'a [MkDiagnostic],
    ) -> Vec<&'a MkDiagnostic> {
        let closure: HashSet<_> = self
            .closure(root, DependencyPolicy::EnabledCalls)
            .into_iter()
            .collect();
        diagnostics
            .iter()
            .filter(|d| d.scope == DiagnosticScope::Document || closure.contains(&d.macro_id))
            .collect()
    }

    /// Cycles include a readable path and the exact closing Call site. The
    /// explicit DFS stack bounds Rust stack usage even for corrupt deep graphs.
    pub fn cycle_diagnostics(&self, policy: DependencyPolicy) -> Vec<MkDiagnostic> {
        let mut result = Vec::new();
        let mut colors = HashMap::<u64, u8>::new();
        let mut positions = HashMap::<u64, usize>::new();
        for root in &self.order {
            if colors.contains_key(root) {
                continue;
            }
            let mut stack = vec![(*root, 0usize)];
            colors.insert(*root, 1);
            positions.insert(*root, 0);
            while let Some((owner, next)) = stack.last_mut() {
                let edges = self.edges(*owner);
                if *next >= edges.len() {
                    colors.insert(*owner, 2);
                    positions.remove(owner);
                    stack.pop();
                    continue;
                }
                let edge = &edges[*next];
                *next += 1;
                if (!edge.enabled && policy == DependencyPolicy::EnabledCalls)
                    || !self.indices.contains_key(&edge.target_id)
                {
                    continue;
                }
                match colors.get(&edge.target_id).copied().unwrap_or(0) {
                    0 => {
                        colors.insert(edge.target_id, 1);
                        positions.insert(edge.target_id, stack.len());
                        stack.push((edge.target_id, 0));
                    }
                    1 => {
                        let start = positions[&edge.target_id];
                        let mut path: Vec<_> = stack[start..].iter().map(|(id, _)| *id).collect();
                        path.push(edge.target_id);
                        let readable = path
                            .iter()
                            .map(|id| format!("{} (#{id})", self.names[id]))
                            .collect::<Vec<_>>()
                            .join(" -> ");
                        let mut diagnostic = MkDiagnostic::fatal(
                            edge.caller_id,
                            Some(edge.step_id),
                            "call_cycle",
                            format!("Recursive macro calls are prohibited: {readable}"),
                        );
                        diagnostic.target_macro_id = Some(edge.target_id);
                        diagnostic.cycle_path = path;
                        result.push(diagnostic);
                    }
                    _ => {}
                }
            }
        }
        result
    }

    pub(crate) fn target_diagnostic(
        &self,
        owner: u64,
        step: u64,
        target: u64,
    ) -> Option<MkDiagnostic> {
        let (code, message) = match self.enabled.get(&target) {
            None => (
                "missing_call_target",
                format!("Call target #{target} does not exist"),
            ),
            Some(false) => (
                "disabled_call_target",
                format!(
                    "Call target {} (#{target}) is disabled",
                    self.names[&target]
                ),
            ),
            Some(true) => return None,
        };
        let mut diagnostic = MkDiagnostic::fatal(owner, Some(step), code, message);
        diagnostic.target_macro_id = Some(target);
        Some(diagnostic)
    }
}
