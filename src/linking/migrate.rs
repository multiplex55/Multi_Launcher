use crate::linking::{LinkRef, LinkTarget, index::dedupe_links};

pub fn migrate_legacy_links(metadata: &serde_json::Value) -> Vec<LinkRef> {
    let mut links = Vec::new();
    if let Some(items) = metadata.get("links").and_then(|v| v.as_array()) {
        for item in items {
            let ty = item
                .get("type")
                .and_then(|v| v.as_str())
                .and_then(LinkTarget::parse);
            let id = item.get("id").and_then(|v| v.as_str());
            if let (Some(target_type), Some(target_id)) = (ty, id) {
                links.push(LinkRef {
                    target_type,
                    target_id: target_id.to_string(),
                    anchor: item
                        .get("anchor")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    display_text: item
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
    }
    if let Some(raw_refs) = metadata
        .get("metadata")
        .and_then(|v| v.get("refs"))
        .and_then(|v| v.as_array())
    {
        for entry in raw_refs.iter().filter_map(|v| v.as_str()) {
            if let Some((kind, id)) = entry.trim_start_matches('@').split_once(':')
                && let Some(target_type) = LinkTarget::parse(kind)
                && !id.trim().is_empty()
            {
                links.push(LinkRef {
                    target_type,
                    target_id: id.trim().to_string(),
                    anchor: None,
                    display_text: None,
                });
            }
        }
    }
    dedupe_links(links)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linking::LinkRef;

    #[test]
    fn migrates_legacy_metadata_shapes() {
        let blob = serde_json::json!({
            "links": [
                {"type": "note", "id": "alpha"},
                {"type": "todo", "id": "todo-1", "text": "Todo One"}
            ],
            "metadata": {"refs": ["@note:beta", "@layout:daily"]}
        });
        let links = migrate_legacy_links(&blob);
        assert_eq!(
            links,
            vec![
                LinkRef {
                    target_type: LinkTarget::Layout,
                    target_id: "daily".into(),
                    anchor: None,
                    display_text: None
                },
                LinkRef {
                    target_type: LinkTarget::Note,
                    target_id: "alpha".into(),
                    anchor: None,
                    display_text: None
                },
                LinkRef {
                    target_type: LinkTarget::Note,
                    target_id: "beta".into(),
                    anchor: None,
                    display_text: None
                },
                LinkRef {
                    target_type: LinkTarget::Todo,
                    target_id: "todo-1".into(),
                    anchor: None,
                    display_text: Some("Todo One".into())
                },
            ]
        );
    }
}
