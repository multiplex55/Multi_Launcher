use super::catalog::normalize_name;
use super::error::ClipboardModifyError;
use super::model::{ClipboardModifierCatalog, ClipboardTemplate, SavedPipeline, StageSpec};

pub fn find_template<'a>(
    catalog: &'a ClipboardModifierCatalog,
    name: &str,
) -> Option<&'a ClipboardTemplate> {
    let needle = normalize_name(name);
    catalog.templates.iter().find(|t| {
        normalize_name(&t.id) == needle || t.aliases.iter().any(|a| normalize_name(a) == needle)
    })
}

pub fn find_pipeline<'a>(
    catalog: &'a ClipboardModifierCatalog,
    name: &str,
) -> Option<&'a SavedPipeline> {
    let needle = normalize_name(name);
    catalog.pipelines.iter().find(|p| {
        normalize_name(&p.id) == needle || p.aliases.iter().any(|a| normalize_name(a) == needle)
    })
}

pub fn validate_executable_stages(
    stages: &[StageSpec],
    catalog: &ClipboardModifierCatalog,
) -> Result<(), ClipboardModifyError> {
    for stage in stages {
        stage.validate()?;
        match stage.operation {
            super::model::OperationId::Template | super::model::OperationId::NamedWrap => {
                let name = stage.arguments.name.as_deref().unwrap_or_default();
                if find_pipeline(catalog, name).is_some() {
                    return Err(ClipboardModifyError::NestedPipeline { name: name.into() });
                }
                if find_template(catalog, name).is_none() {
                    return Err(ClipboardModifyError::UnknownTemplate { name: name.into() });
                }
            }
            _ => {}
        }
    }
    Ok(())
}
