use crate::linking::{format_link_id, parse_link_id, EntityKey, LinkRef};
use std::collections::{HashMap, HashSet};
use tracing::warn;

#[derive(Debug, Clone, Default)]
pub struct LinkResolverCatalog {
    existing: HashSet<EntityKey>,
    anchors: HashMap<EntityKey, HashSet<String>>,
}

impl LinkResolverCatalog {
    pub fn add_target(&mut self, target: EntityKey) {
        self.existing.insert(target);
    }

    pub fn add_anchor(&mut self, target: EntityKey, anchor: impl Into<String>) {
        self.existing.insert(target.clone());
        self.anchors
            .entry(target)
            .or_default()
            .insert(anchor.into().to_ascii_lowercase());
    }

    fn has_target(&self, target: &EntityKey) -> bool {
        self.existing.contains(target)
    }

    fn has_anchor(&self, target: &EntityKey, anchor: &str) -> bool {
        self.anchors
            .get(target)
            .map(|anchors| anchors.contains(&anchor.to_ascii_lowercase()))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveLinkError {
    InvalidLinkId(crate::linking::LinkParseError),
    MissingTarget,
    InvalidAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLink {
    pub canonical: LinkRef,
    pub location: String,
}

pub trait LinkTelemetry {
    fn on_resolve_failure(&self, link_id: &str, reason: &ResolveLinkError);
    fn on_broken_anchor(&self, link: &LinkRef, anchor: &str);
}

pub struct TracingLinkTelemetry;

impl LinkTelemetry for TracingLinkTelemetry {
    fn on_resolve_failure(&self, link_id: &str, reason: &ResolveLinkError) {
        warn!(link_id, ?reason, "link resolution failed");
    }

    fn on_broken_anchor(&self, link: &LinkRef, anchor: &str) {
        warn!(
            target_type = link.target_type.as_str(),
            target_id = link.target_id,
            anchor,
            "link anchor is invalid"
        );
    }
}

pub fn resolve_link(
    link_id: &str,
    catalog: &LinkResolverCatalog,
    telemetry: &dyn LinkTelemetry,
) -> Result<ResolvedLink, ResolveLinkError> {
    let parsed = parse_link_id(link_id).map_err(ResolveLinkError::InvalidLinkId)?;
    let key = EntityKey::new(parsed.target_type, parsed.target_id.clone());
    if !catalog.has_target(&key) {
        let err = ResolveLinkError::MissingTarget;
        telemetry.on_resolve_failure(link_id, &err);
        return Err(err);
    }
    if let Some(anchor) = &parsed.anchor
        && !catalog.has_anchor(&key, anchor) {
            telemetry.on_broken_anchor(&parsed, anchor);
            let err = ResolveLinkError::InvalidAnchor;
            telemetry.on_resolve_failure(link_id, &err);
            return Err(err);
        }
    Ok(ResolvedLink {
        canonical: parsed.clone(),
        location: format_link_id(&parsed),
    })
}
