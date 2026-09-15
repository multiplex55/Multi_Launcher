use super::model::*;
use crate::universal_actions::{PersistableActionTargetRef, PersistedUniversalActionRef};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationErrors(pub Vec<ValidationIssue>);

impl std::fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "radial configuration has {} validation error(s)",
            self.0.len()
        )
    }
}
impl std::error::Error for ValidationErrors {}

pub fn validate(document: &RadialDocument) -> Result<(), ValidationErrors> {
    let mut errors = Vec::new();
    if document.schema_version != CURRENT_SCHEMA_VERSION {
        errors.push(issue(
            "schema_version",
            format!(
                "unsupported schema version {}; expected {CURRENT_SCHEMA_VERSION}",
                document.schema_version
            ),
        ));
    }
    bounded(
        "menus",
        document.menus.len(),
        limits::MAX_MENUS,
        &mut errors,
    );
    bounded(
        "skins",
        document.skins.len(),
        limits::MAX_SKINS,
        &mut errors,
    );
    bounded(
        "context_rules",
        document.context_rules.len(),
        limits::MAX_CONTEXT_RULES,
        &mut errors,
    );
    bounded(
        "custom_triggers",
        document.custom_triggers.len(),
        limits::MAX_CUSTOM_TRIGGERS,
        &mut errors,
    );
    bounded(
        "assets",
        document.assets.len(),
        limits::MAX_ASSETS,
        &mut errors,
    );
    let mut asset_kinds = BTreeMap::new();
    for (index, asset) in document.assets.iter().enumerate() {
        let path = format!("assets[{index}]");
        if asset.id.as_str().trim().is_empty()
            || asset_kinds.insert(asset.id.clone(), asset.kind).is_some()
        {
            errors.push(issue(
                format!("{path}.id"),
                "asset ID is empty or duplicated",
            ));
        }
        validate_managed_path(
            &asset.relative_path,
            &format!("{path}.relative_path"),
            &mut errors,
        );
        if asset.content_sha256.len() != 64
            || !asset
                .content_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            errors.push(issue(
                format!("{path}.content_sha256"),
                "asset digest must contain exactly 64 hexadecimal characters",
            ));
        }
        if asset.id.as_str().starts_with("import-") {
            let kind = match asset.kind {
                MediaKind::Image => "image",
                MediaKind::Sound => "sound",
            };
            let expected = format!(
                "import-{kind}-{}",
                asset.content_sha256.to_ascii_lowercase()
            );
            if asset.id.as_str() != expected {
                errors.push(issue(
                    format!("{path}.id"),
                    "imported asset ID must contain its complete media-kind-qualified SHA-256 digest",
                ));
            }
        }
        if asset.byte_len > limits::MAX_TEXTURE_BYTES {
            errors.push(issue(
                format!("{path}.byte_len"),
                "asset exceeds the configured media byte limit",
            ));
        }
    }
    validate_search_roots(&document.media_search_roots, &mut errors);
    validate_full_style(
        &document.user_style_defaults.values,
        "user_style_defaults",
        &asset_kinds,
        &mut errors,
    );

    let menu_ids = unique_ids(
        document.menus.iter().map(|v| (&v.id, v.id.as_str())),
        "menus",
        &mut errors,
    );
    let skin_ids = unique_ids(
        document.skins.iter().map(|v| (&v.id, v.id.as_str())),
        "skins",
        &mut errors,
    );
    unique_ids(
        document
            .context_rules
            .iter()
            .map(|v| (&v.id, v.id.as_str())),
        "context_rules",
        &mut errors,
    );
    unique_ids(
        document
            .custom_triggers
            .iter()
            .map(|v| (&v.id, v.id.as_str())),
        "custom_triggers",
        &mut errors,
    );
    if !menu_ids.contains(&document.default_menu_id) {
        errors.push(issue(
            "default_menu_id",
            format!("missing menu {}", document.default_menu_id),
        ));
    }

    let mut graph: BTreeMap<MenuId, Vec<MenuId>> = BTreeMap::new();
    let mut total_cells = 0;
    for (mi, menu) in document.menus.iter().enumerate() {
        let path = format!("menus[{mi}]");
        if menu.hover_dwell_ms.is_some_and(|value| value > 60_000) {
            errors.push(issue(
                format!("{path}.hover_dwell_ms"),
                "hover dwell exceeds 60 seconds",
            ));
        }
        if menu.name.trim().is_empty() {
            errors.push(issue(format!("{path}.name"), "name cannot be empty"));
        }
        if !skin_ids.contains(&menu.skin_id) {
            errors.push(issue(
                format!("{path}.skin_id"),
                format!("missing skin {}", menu.skin_id),
            ));
        }
        validate_full_style(
            &menu.style.values,
            &format!("{path}.style"),
            &asset_kinds,
            &mut errors,
        );
        let effective_style = crate::radial::skin::compile_menu_tree(document, menu).ok();
        if let Some(binding) = &menu.center_action {
            validate_binding(binding, &format!("{path}.center_action"), &mut errors);
            validate_keep_open(
                document,
                menu,
                menu.center_primary_after_action,
                binding,
                &format!("{path}.center_action"),
                &mut errors,
            );
        }
        if menu.center_control.is_some() && menu.center_action.is_some() {
            errors.push(issue(
                format!("{path}.center_action"),
                "primary center action is unreachable while a primary center control is configured",
            ));
        }
        if let Some(binding) = &menu.center_secondary_action {
            validate_binding(
                binding,
                &format!("{path}.center_secondary_action"),
                &mut errors,
            );
            validate_keep_open(
                document,
                menu,
                menu.center_secondary_after_action,
                binding,
                &format!("{path}.center_secondary_action"),
                &mut errors,
            );
        }
        if menu.center_secondary_control.is_some() && menu.center_secondary_action.is_some() {
            errors.push(issue(
                format!("{path}.center_secondary_action"),
                "secondary center action is unreachable while a secondary center control is configured",
            ));
        }
        if let Some(binding) = &menu.background_action {
            validate_binding(binding, &format!("{path}.background_action"), &mut errors);
            validate_keep_open(
                document,
                menu,
                menu.background_primary_after_action,
                binding,
                &format!("{path}.background_action"),
                &mut errors,
            );
        }
        if let Some(binding) = &menu.background_secondary_action {
            validate_binding(
                binding,
                &format!("{path}.background_secondary_action"),
                &mut errors,
            );
            validate_keep_open(
                document,
                menu,
                menu.background_secondary_after_action,
                binding,
                &format!("{path}.background_secondary_action"),
                &mut errors,
            );
        }
        if menu.background_control.is_some() && menu.background_action.is_some() {
            errors.push(issue(
                format!("{path}.background_action"),
                "primary background action is unreachable while a primary background control is configured",
            ));
        }
        if menu.background_secondary_control.is_some() && menu.background_secondary_action.is_some()
        {
            errors.push(issue(
                format!("{path}.background_secondary_action"),
                "secondary background action is unreachable while a secondary background control is configured",
            ));
        }
        finite_range(
            format!("{path}.center_radius"),
            menu.center_radius,
            4.0,
            512.0,
            &mut errors,
        );
        bounded(
            format!("{path}.rings"),
            menu.rings.len(),
            limits::MAX_RINGS_PER_MENU,
            &mut errors,
        );
        if menu.rings.is_empty() {
            errors.push(issue(
                format!("{path}.rings"),
                "menu must contain at least one ring",
            ));
        }
        unique_ids(
            menu.rings.iter().map(|v| (&v.id, v.id.as_str())),
            &format!("{path}.rings"),
            &mut errors,
        );
        for (left_index, left) in menu.rings.iter().enumerate() {
            for (right_index, right) in menu.rings.iter().enumerate().skip(left_index + 1) {
                let radial_separation = (left.radius - right.radius).abs();
                let required = left.cell_radius + right.cell_radius + left.gap.max(right.gap);
                if radial_separation + 0.001 < required {
                    errors.push(issue(
                        format!("{path}.rings[{right_index}].radius"),
                        format!(
                            "ring overlaps rings[{left_index}]; radial separation must be at least {required}"
                        ),
                    ));
                }
            }
        }
        if let Some(style) = effective_style.as_ref() {
            let menu_scale =
                crate::radial::skin::resolved_f32(&style.menu.values.geometry.menu_scale);
            let radius_scale =
                crate::radial::skin::resolved_f32(&style.menu.values.geometry.radius_scale);
            let styled_item_radius = (style.menu.source(crate::radial::skin::StyleField::ItemSize)
                != Some(&crate::radial::skin::StyleSource::ApplicationFallback))
            .then(|| {
                crate::radial::skin::resolved_f32(&style.menu.values.geometry.item_size)
                    * menu_scale
                    * 0.5
            });
            for (left_index, left) in menu.rings.iter().enumerate() {
                for (right_index, right) in menu.rings.iter().enumerate().skip(left_index + 1) {
                    let separation = (left.radius - right.radius).abs() * radius_scale * menu_scale;
                    let required = styled_item_radius.unwrap_or(left.cell_radius * menu_scale)
                        + styled_item_radius.unwrap_or(right.cell_radius * menu_scale)
                        + left.gap.max(right.gap) * menu_scale;
                    if separation + 0.001 < required {
                        errors.push(issue(
                            format!("{path}.rings[{right_index}].style"),
                            format!(
                                "effective style overlaps rings[{left_index}]; radial separation must be at least {required}"
                            ),
                        ));
                    }
                }
            }
        }
        let mut cell_ids = BTreeSet::new();
        for (ri, ring) in menu.rings.iter().enumerate() {
            let rp = format!("{path}.rings[{ri}]");
            validate_ring_style(
                &ring.style,
                &format!("{rp}.style"),
                &asset_kinds,
                &mut errors,
            );
            finite_range(
                format!("{rp}.radius"),
                ring.radius,
                1.0,
                8192.0,
                &mut errors,
            );
            finite_range(
                format!("{rp}.cell_radius"),
                ring.cell_radius,
                4.0,
                512.0,
                &mut errors,
            );
            finite_range(
                format!("{rp}.rotation_degrees"),
                ring.rotation_degrees,
                -36000.0,
                36000.0,
                &mut errors,
            );
            finite_range(format!("{rp}.gap"), ring.gap, 0.0, 512.0, &mut errors);
            bounded(
                format!("{rp}.cells"),
                ring.cells.len(),
                limits::MAX_CELLS_PER_RING,
                &mut errors,
            );
            let static_cells = ring
                .cells
                .iter()
                .filter(|cell| !matches!(cell.content, CellContent::Dynamic { .. }))
                .count();
            let effective_capacity = effective_style
                .as_ref()
                .and_then(|style| style.rings.get(&ring.id))
                .map_or_else(
                    || crate::radial::bindings::ring_accessible_capacity(ring),
                    |style| {
                        let menu_scale =
                            crate::radial::skin::resolved_f32(&style.values.geometry.menu_scale);
                        let item_radius = if effective_style.as_ref().is_some_and(|tree| {
                            tree.menu.source(crate::radial::skin::StyleField::ItemSize)
                                != Some(&crate::radial::skin::StyleSource::ApplicationFallback)
                        }) {
                            crate::radial::skin::resolved_f32(&style.values.geometry.item_size)
                                * menu_scale
                                * 0.5
                        } else {
                            ring.cell_radius * menu_scale
                        };
                        crate::radial::bindings::ring_accessible_capacity_for(
                            ring.radius
                                * crate::radial::skin::resolved_f32(
                                    &style.values.geometry.radius_scale,
                                )
                                * menu_scale,
                            item_radius,
                            ring.gap * menu_scale,
                        )
                    },
                );
            if ring
                .cells
                .iter()
                .any(|cell| matches!(cell.content, CellContent::Dynamic { .. }))
                && effective_capacity.saturating_sub(static_cells) < 3
            {
                errors.push(issue(
                    format!("{rp}.cells"),
                    "dynamic ring geometry must leave room for an entry and paging controls",
                ));
            }
            total_cells += ring.cells.len();
            if ring.radius <= menu.center_radius + ring.cell_radius {
                errors.push(issue(
                    format!("{rp}.radius"),
                    "ring overlaps the center control",
                ));
            }
            let n = ring.cells.len();
            if let Some(style) = effective_style
                .as_ref()
                .and_then(|style| style.rings.get(&ring.id))
            {
                let menu_scale =
                    crate::radial::skin::resolved_f32(&style.values.geometry.menu_scale);
                let radius = ring.radius
                    * crate::radial::skin::resolved_f32(&style.values.geometry.radius_scale)
                    * menu_scale;
                let item_radius = if effective_style.as_ref().is_some_and(|tree| {
                    tree.menu.source(crate::radial::skin::StyleField::ItemSize)
                        != Some(&crate::radial::skin::StyleSource::ApplicationFallback)
                }) {
                    crate::radial::skin::resolved_f32(&style.values.geometry.item_size)
                        * menu_scale
                        * 0.5
                } else {
                    ring.cell_radius * menu_scale
                };
                let center_radius = if effective_style.as_ref().is_some_and(|tree| {
                    tree.menu
                        .source(crate::radial::skin::StyleField::CenterSize)
                        != Some(&crate::radial::skin::StyleSource::ApplicationFallback)
                }) {
                    crate::radial::skin::resolved_f32(&style.values.geometry.center_size)
                        * menu_scale
                        * 0.5
                } else {
                    menu.center_radius * menu_scale
                };
                if radius <= center_radius + item_radius {
                    errors.push(issue(
                        format!("{rp}.style"),
                        "effective style causes the ring to overlap the center control",
                    ));
                }
                if menu.layout == LayoutKind::CircularCells && n >= 2 {
                    let spacing = 2.0 * radius * (std::f32::consts::PI / n as f32).sin();
                    if spacing + 0.001 < 2.0 * item_radius + ring.gap * menu_scale {
                        errors.push(issue(
                            format!("{rp}.style"),
                            "effective style causes adjacent circular cells to overlap",
                        ));
                    }
                }
            }
            if menu.layout == LayoutKind::CircularCells && n >= 2 {
                let spacing = 2.0 * ring.radius * (std::f32::consts::PI / n as f32).sin();
                if spacing + 0.001 < 2.0 * ring.cell_radius + ring.gap {
                    errors.push(issue(
                        format!("{rp}.cells"),
                        "adjacent circular cells overlap or violate the requested gap",
                    ));
                }
            }
            for (ci, cell) in ring.cells.iter().enumerate() {
                let cp = format!("{rp}.cells[{ci}]");
                validate_media_override(
                    &cell.icon,
                    MediaKind::Image,
                    &format!("{cp}.icon"),
                    &asset_kinds,
                    &mut errors,
                );
                validate_cell_style(
                    &cell.style,
                    &format!("{cp}.style"),
                    &asset_kinds,
                    &mut errors,
                );
                if let Override::Value(tooltip) = &cell.tooltip
                    && tooltip.len() > 4096
                {
                    errors.push(issue(format!("{cp}.tooltip"), "tooltip is too long"));
                }
                validate_item_inputs(cell, &cp, &mut errors);
                if cell.id.as_str().trim().is_empty() || !cell_ids.insert(cell.id.clone()) {
                    errors.push(issue(
                        format!("{cp}.id"),
                        "cell ID is empty or duplicated within its menu",
                    ));
                }
                let generated_page_control = [
                    ("__radial_page_previous:", Control::PreviousPage),
                    ("__radial_page_next:", Control::NextPage),
                ]
                .into_iter()
                .any(|(prefix, control)| {
                    cell.id.as_str().strip_prefix(prefix) == Some(ring.id.as_str())
                        && matches!(&cell.content, CellContent::Control { control: actual } if *actual == control)
                });
                if (cell.id.as_str() == "__center"
                    || cell.id.as_str() == "__background"
                    || cell.id.as_str().starts_with("__radial_page_previous")
                    || cell.id.as_str().starts_with("__radial_page_next"))
                    && !generated_page_control
                {
                    errors.push(issue(
                        format!("{cp}.id"),
                        "cell ID is reserved by the runtime",
                    ));
                }
                validate_content(
                    &cell.content,
                    &cp,
                    &menu_ids,
                    &mut graph,
                    &menu.id,
                    &mut errors,
                );
                if let CellContent::Action { binding } = &cell.content {
                    validate_keep_open(
                        document,
                        menu,
                        cell.after_action,
                        binding,
                        &cp,
                        &mut errors,
                    );
                }
                let mut gestures = BTreeSet::new();
                for (bi, binding) in cell.alternate_clicks.iter().enumerate() {
                    if !gestures.insert(binding.gesture) {
                        errors.push(issue(
                            format!("{cp}.alternate_clicks[{bi}].gesture"),
                            "duplicate click gesture",
                        ));
                    }
                    validate_binding(
                        &binding.action,
                        &format!("{cp}.alternate_clicks[{bi}]"),
                        &mut errors,
                    );
                    validate_keep_open(
                        document,
                        menu,
                        binding.after_action,
                        &binding.action,
                        &format!("{cp}.alternate_clicks[{bi}]"),
                        &mut errors,
                    );
                }
                for (bi, binding) in cell.alternate_controls.iter().enumerate() {
                    if !gestures.insert(binding.gesture) {
                        errors.push(issue(
                            format!("{cp}.alternate_controls[{bi}].gesture"),
                            "duplicate click gesture",
                        ));
                    }
                }
            }
        }
    }
    bounded(
        "all menu cells",
        total_cells,
        limits::MAX_TOTAL_CELLS,
        &mut errors,
    );
    for (si, skin) in document.skins.iter().enumerate() {
        validate_full_style(
            &skin.style.values,
            &format!("skins[{si}].style"),
            &asset_kinds,
            &mut errors,
        );
    }
    for (ri, rule) in document.context_rules.iter().enumerate() {
        if !menu_ids.contains(&rule.menu_id) {
            errors.push(issue(
                format!("context_rules[{ri}].menu_id"),
                "missing menu",
            ));
        }
        for (field, value) in [
            ("process_name", rule.process_name.as_deref()),
            (
                "window_title_contains",
                rule.window_title_contains.as_deref(),
            ),
            ("monitor_id", rule.monitor_id.as_deref()),
        ] {
            if value.is_some_and(|value| value.len() > 512) {
                errors.push(issue(
                    format!("context_rules[{ri}].{field}"),
                    "context matcher exceeds 512 bytes",
                ));
            }
        }
    }
    let mut chords = BTreeMap::<String, usize>::new();
    for (ti, trigger) in document.custom_triggers.iter().enumerate() {
        match crate::hotkey::parse_hotkey(&trigger.chord) {
            Some(parsed) => {
                let canonical = canonical_hotkey(&parsed);
                if let Some(previous) = chords.insert(canonical, ti) {
                    errors.push(issue(
                        format!("custom_triggers[{ti}].chord"),
                        format!("conflicts with custom_triggers[{previous}]"),
                    ));
                }
            }
            None => errors.push(issue(
                format!("custom_triggers[{ti}].chord"),
                "invalid hotkey chord",
            )),
        }
        if !menu_ids.contains(&trigger.menu_id) {
            errors.push(issue(
                format!("custom_triggers[{ti}].menu_id"),
                "missing menu",
            ));
        }
    }
    validate_item_input_conflicts(document, &mut errors);
    validate_graph(document, &graph, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationErrors(errors))
    }
}

fn validate_search_roots(roots: &MediaSearchRoots, errors: &mut Vec<ValidationIssue>) {
    for (kind, values) in [
        ("image_directories", &roots.image_directories),
        ("sound_directories", &roots.sound_directories),
    ] {
        if values.len() > limits::MAX_MEDIA_SEARCH_ROOTS {
            errors.push(issue(kind, "too many configured media search roots"));
        }
        let mut seen = BTreeSet::new();
        for (index, value) in values.iter().enumerate() {
            if value.trim().is_empty() || value.len() > 4096 {
                errors.push(issue(
                    format!("{kind}[{index}]"),
                    "search root is empty or too long",
                ));
            }
            let canonical = value.to_lowercase();
            if !seen.insert(canonical) {
                errors.push(issue(
                    format!("{kind}[{index}]"),
                    "duplicate media search root",
                ));
            }
        }
    }
}

fn validate_managed_path(path: &str, field: &str, errors: &mut Vec<ValidationIssue>) {
    use std::path::{Component, Path};
    if path.trim().is_empty()
        || path.len() > 4096
        || path.contains(':')
        || Path::new(path).is_absolute()
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        errors.push(issue(
            field,
            "managed asset path must be a safe relative path",
        ));
    }
}

fn validate_full_style(
    style: &StyleOverrides,
    path: &str,
    assets: &BTreeMap<AssetId, MediaKind>,
    errors: &mut Vec<ValidationIssue>,
) {
    validate_images(&style.images, &format!("{path}.images"), assets, errors);
    validate_geometry(&style.geometry, &format!("{path}.geometry"), errors);
    validate_text(&style.text, &format!("{path}.text"), errors);
    validate_effects(&style.effects, &format!("{path}.effects"), errors);
    validate_sounds(&style.sounds, &format!("{path}.sounds"), assets, errors);
}

fn validate_ring_style(
    style: &RingStyleLayer,
    path: &str,
    assets: &BTreeMap<AssetId, MediaKind>,
    errors: &mut Vec<ValidationIssue>,
) {
    validate_item_images(&style.images, &format!("{path}.images"), assets, errors);
    validate_item_geometry(&style.geometry, &format!("{path}.geometry"), errors);
    validate_text(&style.text, &format!("{path}.text"), errors);
    validate_item_sounds(&style.sounds, &format!("{path}.sounds"), assets, errors);
}

fn validate_cell_style(
    style: &CellStyleLayer,
    path: &str,
    assets: &BTreeMap<AssetId, MediaKind>,
    errors: &mut Vec<ValidationIssue>,
) {
    validate_item_images(&style.images, &format!("{path}.images"), assets, errors);
    validate_item_geometry(&style.geometry, &format!("{path}.geometry"), errors);
    validate_text(&style.text, &format!("{path}.text"), errors);
    validate_item_sounds(&style.sounds, &format!("{path}.sounds"), assets, errors);
}

fn validate_effects(style: &EffectStyleOverrides, path: &str, errors: &mut Vec<ValidationIssue>) {
    if let Override::Value(width) = &style.menu_shadow_width {
        finite_range(
            format!("{path}.menu_shadow_width"),
            *width,
            0.0,
            2048.0,
            errors,
        );
    }
}

fn validate_images(
    style: &ImageStyleOverrides,
    path: &str,
    assets: &BTreeMap<AssetId, MediaKind>,
    errors: &mut Vec<ValidationIssue>,
) {
    for (field, value) in [
        ("item_glow", &style.item_glow),
        ("menu_outer_rim", &style.menu_outer_rim),
        ("menu_background", &style.menu_background),
        ("item_background", &style.item_background),
        ("item_foreground", &style.item_foreground),
        ("item_shadow", &style.item_shadow),
        ("menu_foreground", &style.menu_foreground),
        ("center_background", &style.center_background),
        ("center_image", &style.center_image),
        ("submenu_indicator", &style.submenu_indicator),
    ] {
        validate_media_override(
            value,
            MediaKind::Image,
            &format!("{path}.{field}"),
            assets,
            errors,
        );
    }
    for (field, value) in [
        ("item_glow_opacity", &style.item_glow_opacity),
        ("menu_outer_rim_opacity", &style.menu_outer_rim_opacity),
        ("menu_background_opacity", &style.menu_background_opacity),
        ("item_background_opacity", &style.item_background_opacity),
        ("item_foreground_opacity", &style.item_foreground_opacity),
        ("item_shadow_opacity", &style.item_shadow_opacity),
        ("menu_foreground_opacity", &style.menu_foreground_opacity),
        (
            "center_background_opacity",
            &style.center_background_opacity,
        ),
        ("center_image_opacity", &style.center_image_opacity),
        (
            "submenu_indicator_opacity",
            &style.submenu_indicator_opacity,
        ),
        ("icon_opacity", &style.icon_opacity),
    ] {
        if let Override::Value(value) = value {
            finite_range(format!("{path}.{field}"), *value, 0.0, 1.0, errors);
        }
    }
}

fn validate_sounds(
    style: &SoundStyleOverrides,
    path: &str,
    assets: &BTreeMap<AssetId, MediaKind>,
    errors: &mut Vec<ValidationIssue>,
) {
    for (field, value) in [
        ("on_show", &style.on_show),
        ("on_close", &style.on_close),
        ("on_select", &style.on_select),
        ("on_submenu_show", &style.on_submenu_show),
        ("on_submenu_close", &style.on_submenu_close),
    ] {
        validate_media_override(
            value,
            MediaKind::Sound,
            &format!("{path}.{field}"),
            assets,
            errors,
        );
    }
}

fn validate_item_images(
    style: &ItemImageStyleOverrides,
    path: &str,
    assets: &BTreeMap<AssetId, MediaKind>,
    errors: &mut Vec<ValidationIssue>,
) {
    for (field, value) in [
        ("item_background", &style.item_background),
        ("submenu_indicator", &style.submenu_indicator),
    ] {
        validate_media_override(
            value,
            MediaKind::Image,
            &format!("{path}.{field}"),
            assets,
            errors,
        );
    }
    for (field, value) in [
        ("item_background_opacity", &style.item_background_opacity),
        (
            "submenu_indicator_opacity",
            &style.submenu_indicator_opacity,
        ),
        ("icon_opacity", &style.icon_opacity),
    ] {
        if let Override::Value(value) = value {
            finite_range(format!("{path}.{field}"), *value, 0.0, 1.0, errors);
        }
    }
}

fn validate_item_sounds(
    style: &ItemSoundStyleOverrides,
    path: &str,
    assets: &BTreeMap<AssetId, MediaKind>,
    errors: &mut Vec<ValidationIssue>,
) {
    validate_media_override(
        &style.on_select,
        MediaKind::Sound,
        &format!("{path}.on_select"),
        assets,
        errors,
    );
}

fn validate_media_override(
    value: &Override<MediaReference>,
    expected: MediaKind,
    path: &str,
    assets: &BTreeMap<AssetId, MediaKind>,
    errors: &mut Vec<ValidationIssue>,
) {
    let Override::Value(reference) = value else {
        return;
    };
    match reference {
        MediaReference::Managed { asset_id } => match assets.get(asset_id) {
            Some(actual) if *actual == expected => {}
            Some(_) => errors.push(issue(path, "managed asset has the wrong media kind")),
            None => errors.push(issue(path, format!("missing managed asset {asset_id}"))),
        },
        MediaReference::ExternalFile { path: value } => {
            if invalid_media_text(value) {
                errors.push(issue(
                    path,
                    "external media path is empty, too long, or a raw process handle",
                ));
            }
        }
        MediaReference::SearchPath { file_name } => {
            if invalid_media_text(file_name)
                || file_name
                    .chars()
                    .any(|character| matches!(character, '/' | '\\' | ':'))
            {
                errors.push(issue(
                    path,
                    "search-path media must contain one safe file name",
                ));
            }
        }
        MediaReference::IconResource { path: value, index } => {
            let extension = std::path::Path::new(value)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if expected != MediaKind::Image
                || invalid_media_text(value)
                || !matches!(extension.as_str(), "exe" | "dll" | "cpl")
                || *index == 0
            {
                errors.push(issue(path, "invalid image resource reference"));
            }
        }
    }
}

fn invalid_media_text(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized.is_empty()
        || normalized.len() > 4096
        || ["hicon", "hbitmap", "pbitmap"].iter().any(|prefix| {
            normalized == *prefix
                || normalized.strip_prefix(*prefix).is_some_and(|suffix| {
                    suffix.chars().next().is_some_and(|value| {
                        matches!(value, ':' | '=')
                            || value.is_whitespace()
                            || value.is_ascii_digit()
                    })
                })
        })
}

fn validate_geometry(
    style: &GeometryStyleOverrides,
    path: &str,
    errors: &mut Vec<ValidationIssue>,
) {
    for (field, value, min, max) in [
        ("menu_scale", &style.menu_scale, 0.25, 4.0),
        ("item_size", &style.item_size, 0.0, 2048.0),
        ("radius_scale", &style.radius_scale, 0.0, 16.0),
        ("center_size", &style.center_size, 0.0, 2048.0),
        ("center_image_scale", &style.center_image_scale, 0.0, 16.0),
        ("item_image_scale", &style.item_image_scale, 0.0, 16.0),
        ("item_image_y_ratio", &style.item_image_y_ratio, -4.0, 4.0),
        (
            "item_background_scale",
            &style.item_background_scale,
            0.0,
            16.0,
        ),
        (
            "item_foreground_scale",
            &style.item_foreground_scale,
            0.0,
            16.0,
        ),
        ("item_shadow_scale", &style.item_shadow_scale, 0.0, 16.0),
        (
            "menu_background_scale",
            &style.menu_background_scale,
            0.0,
            16.0,
        ),
        (
            "menu_foreground_scale",
            &style.menu_foreground_scale,
            0.0,
            16.0,
        ),
        (
            "center_background_scale",
            &style.center_background_scale,
            0.0,
            16.0,
        ),
        (
            "submenu_indicator_size",
            &style.submenu_indicator_size,
            0.0,
            2048.0,
        ),
        (
            "submenu_indicator_y_ratio",
            &style.submenu_indicator_y_ratio,
            -4.0,
            4.0,
        ),
        ("outer_ring_margin", &style.outer_ring_margin, 0.0, 2048.0),
        ("outer_rim_width", &style.outer_rim_width, 0.0, 2048.0),
    ] {
        if let Override::Value(value) = value {
            finite_range(format!("{path}.{field}"), *value, min, max, errors);
        }
    }
}

fn validate_item_geometry(
    style: &ItemGeometryStyleOverrides,
    path: &str,
    errors: &mut Vec<ValidationIssue>,
) {
    for (field, value, min, max) in [
        ("item_image_scale", &style.item_image_scale, 0.0, 16.0),
        ("item_image_y_ratio", &style.item_image_y_ratio, -4.0, 4.0),
        (
            "submenu_indicator_size",
            &style.submenu_indicator_size,
            0.0,
            2048.0,
        ),
        (
            "submenu_indicator_y_ratio",
            &style.submenu_indicator_y_ratio,
            -4.0,
            4.0,
        ),
    ] {
        if let Override::Value(value) = value {
            finite_range(format!("{path}.{field}"), *value, min, max, errors);
        }
    }
}

fn validate_text(style: &TextStyleOverrides, path: &str, errors: &mut Vec<ValidationIssue>) {
    if let Override::Value(marker) = &style.submenu_indicator_text
        && (marker.is_empty() || marker.chars().count() > 8)
    {
        errors.push(issue(
            format!("{path}.submenu_indicator_text"),
            "submenu indicator text must contain between one and eight characters",
        ));
    }
    if let Override::Value(family) = &style.font_family
        && (family.trim().is_empty() || family.len() > 256)
    {
        errors.push(issue(
            format!("{path}.font_family"),
            "font family is empty or too long",
        ));
    }
    for (field, value, min, max) in [
        ("font_size", &style.font_size, 1.0, 512.0),
        ("text_box_scale", &style.text_box_scale, 0.0, 16.0),
        ("vertical_ratio", &style.vertical_ratio, -4.0, 4.0),
    ] {
        if let Override::Value(value) = value {
            finite_range(format!("{path}.{field}"), *value, min, max, errors);
        }
    }
    if let Override::Value(offset) = &style.shadow_offset
        && (!offset.x.is_finite()
            || !offset.y.is_finite()
            || offset.x.abs() > 512.0
            || offset.y.abs() > 512.0)
    {
        errors.push(issue(
            format!("{path}.shadow_offset"),
            "invalid text shadow offset",
        ));
    }
}

fn validate_item_inputs(cell: &CellDefinition, path: &str, errors: &mut Vec<ValidationIssue>) {
    bounded(
        format!("{path}.shortcuts"),
        cell.shortcuts.len(),
        limits::MAX_ITEM_SHORTCUTS_PER_CELL,
        errors,
    );
    bounded(
        format!("{path}.hotstrings"),
        cell.hotstrings.len(),
        limits::MAX_ITEM_HOTSTRINGS_PER_CELL,
        errors,
    );
    let mut shortcut_ids = BTreeSet::new();
    let mut shortcut_chords = BTreeSet::new();
    for (index, shortcut) in cell.shortcuts.iter().enumerate() {
        let field = format!("{path}.shortcuts[{index}]");
        if shortcut.id.as_str().trim().is_empty() || !shortcut_ids.insert(shortcut.id.clone()) {
            errors.push(issue(
                format!("{field}.id"),
                "shortcut ID is empty or duplicated",
            ));
        }
        match crate::hotkey::parse_hotkey(&shortcut.chord) {
            Some(parsed) => {
                let identity = canonical_hotkey(&parsed);
                if !shortcut_chords.insert(identity) {
                    errors.push(issue(format!("{field}.chord"), "duplicate shortcut chord"));
                }
            }
            None => errors.push(issue(format!("{field}.chord"), "invalid shortcut chord")),
        }
        if !cell_has_gesture_binding(cell, shortcut.gesture) {
            errors.push(issue(
                format!("{field}.gesture"),
                "shortcut gesture has no prepared action binding on this cell",
            ));
        }
    }
    let mut hotstring_ids = BTreeSet::new();
    let mut hotstrings = BTreeSet::new();
    for (index, hotstring) in cell.hotstrings.iter().enumerate() {
        let field = format!("{path}.hotstrings[{index}]");
        if hotstring.id.as_str().trim().is_empty() || !hotstring_ids.insert(hotstring.id.clone()) {
            errors.push(issue(
                format!("{field}.id"),
                "hotstring ID is empty or duplicated",
            ));
        }
        if hotstring.text.is_empty() || hotstring.text.len() > 128 {
            errors.push(issue(
                format!("{field}.text"),
                "hotstring is empty or too long",
            ));
        }
        if !hotstring
            .text
            .chars()
            .all(|character| character.is_ascii() && !character.is_ascii_control())
        {
            errors.push(issue(
                format!("{field}.text"),
                "hotstring must use printable ASCII characters supported by the synchronous listener",
            ));
        }
        let identity = if hotstring.case_sensitive {
            hotstring.text.clone()
        } else {
            hotstring.text.to_lowercase()
        };
        if !hotstrings.insert((hotstring.scope, identity)) {
            errors.push(issue(format!("{field}.text"), "duplicate hotstring"));
        }
        if !cell_has_gesture_binding(cell, hotstring.gesture) {
            errors.push(issue(
                format!("{field}.gesture"),
                "hotstring gesture has no prepared action binding on this cell",
            ));
        }
    }
}

fn cell_has_gesture_binding(cell: &CellDefinition, gesture: ClickGesture) -> bool {
    (gesture == ClickGesture::Primary
        && matches!(
            &cell.content,
            CellContent::Action { .. }
                | CellContent::Dynamic { .. }
                | CellContent::Submenu { .. }
                | CellContent::Control { .. }
        ))
        || cell
            .alternate_clicks
            .iter()
            .any(|binding| binding.gesture == gesture)
        || cell
            .alternate_controls
            .iter()
            .any(|binding| binding.gesture == gesture)
}

fn validate_item_input_conflicts(document: &RadialDocument, errors: &mut Vec<ValidationIssue>) {
    let mut shortcut_ids = BTreeMap::<ShortcutId, String>::new();
    let mut global_shortcuts = BTreeMap::<String, String>::new();
    let mut local_shortcuts = BTreeMap::<(MenuId, String), String>::new();
    let mut any_local_shortcuts = BTreeMap::<String, String>::new();
    let mut hotstring_ids = BTreeMap::<HotstringId, String>::new();
    let mut global_hotstrings = BTreeMap::<String, String>::new();
    let mut local_hotstrings = BTreeMap::<(MenuId, String), String>::new();
    let mut any_local_hotstrings = BTreeMap::<String, String>::new();

    for (index, trigger) in document.custom_triggers.iter().enumerate() {
        let Some(parsed) = crate::hotkey::parse_hotkey(&trigger.chord) else {
            continue;
        };
        let identity = canonical_hotkey(&parsed);
        let path = format!("custom_triggers[{index}].chord");
        match trigger.scope {
            TriggerScope::Global => {
                global_shortcuts.entry(identity).or_insert(path);
            }
            TriggerScope::MenuLocal => {
                any_local_shortcuts
                    .entry(identity.clone())
                    .or_insert_with(|| path.clone());
                local_shortcuts
                    .entry((trigger.menu_id.clone(), identity))
                    .or_insert(path);
            }
        }
    }

    for (menu_index, menu) in document.menus.iter().enumerate() {
        for (ring_index, ring) in menu.rings.iter().enumerate() {
            for (cell_index, cell) in ring.cells.iter().enumerate() {
                let cell_path =
                    format!("menus[{menu_index}].rings[{ring_index}].cells[{cell_index}]");
                for (index, shortcut) in cell.shortcuts.iter().enumerate() {
                    let path = format!("{cell_path}.shortcuts[{index}]");
                    if let Some(previous) = shortcut_ids.insert(shortcut.id.clone(), path.clone()) {
                        errors.push(issue(
                            format!("{path}.id"),
                            format!("shortcut ID conflicts with {previous}"),
                        ));
                    }
                    let Some(parsed) = crate::hotkey::parse_hotkey(&shortcut.chord) else {
                        continue;
                    };
                    let identity = canonical_hotkey(&parsed);
                    let conflict = match shortcut.scope {
                        TriggerScope::Global => global_shortcuts
                            .get(&identity)
                            .or_else(|| any_local_shortcuts.get(&identity)),
                        TriggerScope::MenuLocal => global_shortcuts
                            .get(&identity)
                            .or_else(|| local_shortcuts.get(&(menu.id.clone(), identity.clone()))),
                    };
                    if let Some(previous) = conflict {
                        errors.push(issue(
                            format!("{path}.chord"),
                            format!("shortcut chord conflicts with {previous}"),
                        ));
                    }
                    match shortcut.scope {
                        TriggerScope::Global => {
                            global_shortcuts.entry(identity).or_insert(path);
                        }
                        TriggerScope::MenuLocal => {
                            any_local_shortcuts
                                .entry(identity.clone())
                                .or_insert_with(|| path.clone());
                            local_shortcuts
                                .entry((menu.id.clone(), identity))
                                .or_insert(path);
                        }
                    }
                }
                for (index, hotstring) in cell.hotstrings.iter().enumerate() {
                    let path = format!("{cell_path}.hotstrings[{index}]");
                    if let Some(previous) = hotstring_ids.insert(hotstring.id.clone(), path.clone())
                    {
                        errors.push(issue(
                            format!("{path}.id"),
                            format!("hotstring ID conflicts with {previous}"),
                        ));
                    }
                    let identity = hotstring.text.to_lowercase();
                    let conflict = match hotstring.scope {
                        TriggerScope::Global => global_hotstrings
                            .get(&identity)
                            .or_else(|| any_local_hotstrings.get(&identity)),
                        TriggerScope::MenuLocal => global_hotstrings
                            .get(&identity)
                            .or_else(|| local_hotstrings.get(&(menu.id.clone(), identity.clone()))),
                    };
                    if let Some(previous) = conflict {
                        errors.push(issue(
                            format!("{path}.text"),
                            format!("hotstring conflicts with {previous}"),
                        ));
                    }
                    match hotstring.scope {
                        TriggerScope::Global => {
                            global_hotstrings.entry(identity).or_insert(path);
                        }
                        TriggerScope::MenuLocal => {
                            any_local_hotstrings
                                .entry(identity.clone())
                                .or_insert_with(|| path.clone());
                            local_hotstrings
                                .entry((menu.id.clone(), identity))
                                .or_insert(path);
                        }
                    }
                }
            }
        }
    }
}

fn canonical_hotkey(parsed: &crate::hotkey::Hotkey) -> String {
    format!(
        "{:?}:{}:{}:{}:{}:{}",
        parsed.key, parsed.ctrl, parsed.shift, parsed.alt, parsed.alt_gr, parsed.win
    )
}

fn validate_keep_open(
    document: &RadialDocument,
    menu: &MenuDefinition,
    policy: AfterActionPolicy,
    binding: &ActionBinding,
    path: &str,
    errors: &mut Vec<ValidationIssue>,
) {
    if effective_after_action(document, menu, policy) != AfterActionPolicy::KeepOpen {
        return;
    }
    let ActionBinding::Persisted { action } = binding else {
        return;
    };
    let requirement = match action.target.as_ref() {
        Some(
            PersistableActionTargetRef::LegacyAction { action }
            | PersistableActionTargetRef::CustomAction { action },
        ) => crate::commands::parse_action(action)
            .map(|command| crate::radial::handoff::command_requirement(&command))
            .unwrap_or(crate::radial::handoff::InteractionRequirement::ExternalInput),
        _ => match crate::radial::handoff::action_id_requirement(&action.action_id) {
            Some(requirement) => requirement,
            None => return,
        },
    };
    if requirement != crate::radial::handoff::InteractionRequirement::None {
        errors.push(issue(
            format!("{path}.after_action"),
            format!("KeepOpen is incompatible with {requirement:?}"),
        ));
    }
}

fn validate_content(
    content: &CellContent,
    path: &str,
    menu_ids: &BTreeSet<MenuId>,
    graph: &mut BTreeMap<MenuId, Vec<MenuId>>,
    owner: &MenuId,
    errors: &mut Vec<ValidationIssue>,
) {
    match content {
        CellContent::Action { binding } => validate_binding(binding, path, errors),
        CellContent::Submenu { menu_id } => {
            if !menu_ids.contains(menu_id) {
                errors.push(issue(
                    format!("{path}.content.menu_id"),
                    format!("missing menu {menu_id}"),
                ));
            }
            graph
                .entry(owner.clone())
                .or_default()
                .push(menu_id.clone());
        }
        CellContent::Dynamic {
            source:
                DynamicSource::LauncherResults { max_items }
                | DynamicSource::LauncherQuery { max_items, .. },
        } if *max_items == 0 || *max_items > limits::MAX_CELLS_PER_RING => errors.push(issue(
            format!("{path}.content.max_items"),
            "dynamic result limit is out of range",
        )),
        CellContent::Dynamic {
            source: DynamicSource::LauncherQuery { query, .. },
        } if query.len() > 512 => errors.push(issue(
            format!("{path}.content.query"),
            "dynamic launcher query is too long",
        )),
        _ => {}
    }
}

fn validate_binding(binding: &ActionBinding, path: &str, errors: &mut Vec<ValidationIssue>) {
    let (action_id, target) = match binding {
        ActionBinding::Persisted {
            action: PersistedUniversalActionRef { action_id, target },
        } => (action_id.as_str(), target.as_ref()),
        ActionBinding::Contextual { action_id, .. } => (action_id.as_str(), None),
    };
    if action_id.trim().is_empty() {
        errors.push(issue(
            format!("{path}.action_id"),
            "action ID cannot be empty",
        ));
    }
    if let Some(target) = target {
        let valid = match target {
            PersistableActionTargetRef::LegacyAction { action }
            | PersistableActionTargetRef::CustomAction { action } => {
                !action.action.trim().is_empty()
            }
            PersistableActionTargetRef::Folder { path }
            | PersistableActionTargetRef::Tempfile { path } => !path.trim().is_empty(),
            PersistableActionTargetRef::Bookmark { url } => !url.trim().is_empty(),
            PersistableActionTargetRef::Snippet { alias } => !alias.trim().is_empty(),
            PersistableActionTargetRef::Note { slug } => !slug.trim().is_empty(),
            PersistableActionTargetRef::MkMacro { id } => *id != 0,
        };
        if !valid {
            errors.push(issue(
                format!("{path}.target"),
                "persistent target identity is empty or invalid",
            ));
        }
    }
}

fn validate_graph(
    document: &RadialDocument,
    graph: &BTreeMap<MenuId, Vec<MenuId>>,
    errors: &mut Vec<ValidationIssue>,
) {
    fn depth(
        node: &MenuId,
        graph: &BTreeMap<MenuId, Vec<MenuId>>,
        path: &mut Vec<MenuId>,
        memo: &mut BTreeMap<MenuId, usize>,
        errors: &mut Vec<ValidationIssue>,
    ) -> usize {
        if let Some(depth) = memo.get(node) {
            return *depth;
        }
        if let Some(index) = path.iter().position(|id| id == node) {
            let mut cycle = path[index..]
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            cycle.push(node.to_string());
            errors.push(issue(
                "menus",
                format!("submenu cycle: {}", cycle.join(" -> ")),
            ));
            return 0;
        }
        path.push(node.clone());
        let mut child_depth = 0;
        if let Some(children) = graph.get(node) {
            for child in children {
                child_depth = child_depth.max(depth(child, graph, path, memo, errors));
            }
        }
        path.pop();
        let result = child_depth
            .saturating_add(1)
            .min(limits::MAX_SUBMENU_DEPTH + 1);
        memo.insert(node.clone(), result);
        result
    }
    let mut memo = BTreeMap::new();
    for menu in &document.menus {
        let depth = depth(&menu.id, graph, &mut Vec::new(), &mut memo, errors);
        if depth > limits::MAX_SUBMENU_DEPTH {
            errors.push(issue(
                "menus",
                format!(
                    "submenu depth exceeds {} at {}",
                    limits::MAX_SUBMENU_DEPTH,
                    menu.id
                ),
            ));
        }
    }
}

fn unique_ids<'a, T: Clone + Ord + 'a>(
    items: impl Iterator<Item = (&'a T, &'a str)>,
    path: &str,
    errors: &mut Vec<ValidationIssue>,
) -> BTreeSet<T> {
    let mut ids = BTreeSet::new();
    for (index, (id, text)) in items.enumerate() {
        if text.trim().is_empty() || !ids.insert(id.clone()) {
            errors.push(issue(
                format!("{path}[{index}].id"),
                "ID is empty or duplicated",
            ));
        }
    }
    ids
}
fn finite_range(path: String, value: f32, min: f32, max: f32, errors: &mut Vec<ValidationIssue>) {
    if !value.is_finite() || value < min || value > max {
        errors.push(issue(path, format!("must be finite and in {min}..={max}")));
    }
}
fn bounded(path: impl Into<String>, actual: usize, max: usize, errors: &mut Vec<ValidationIssue>) {
    if actual > max {
        errors.push(issue(path, format!("contains {actual}; limit is {max}")));
    }
}
fn issue(path: impl Into<String>, message: impl Into<String>) -> ValidationIssue {
    ValidationIssue {
        path: path.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> RadialDocument {
        RadialDocument::starter()
    }

    #[test]
    fn starter_validates() {
        validate(&valid()).unwrap();
    }

    #[test]
    fn rejects_dynamic_ring_geometry_that_cannot_fit_paging_controls() {
        let mut document = valid();
        document.menus[0].rings[0].radius = 70.0;
        document.menus[0].rings[0].cell_radius = 28.0;
        document.menus[0].rings[0].gap = 4.0;
        document.menus[0].rings[0].cells[0].content = CellContent::Dynamic {
            source: DynamicSource::Favorites,
        };
        for cell in &mut document.menus[0].rings[0].cells[1..] {
            cell.content = CellContent::Spacer;
        }
        for index in 0..3 {
            let mut spacer = document.menus[0].rings[0].cells[1].clone();
            spacer.id = CellId::new(format!("spacer-{index}"));
            document.menus[0].rings[0].cells.push(spacer);
        }
        let errors = validate(&document).unwrap_err();
        assert!(errors.0.iter().any(|issue| {
            issue.path.ends_with("rings[0].cells") && issue.message.contains("paging controls")
        }));
    }

    #[test]
    fn drag_center_rejects_an_unreachable_primary_action_but_starter_is_valid() {
        let mut document = valid();
        document.menus[0].center_action = Some(ActionBinding::Contextual {
            selector: TargetSelector::CapturedForeground,
            action_id: crate::universal_actions::action_ids::WINDOW_ACTIVATE,
        });
        let errors = validate(&document).unwrap_err();
        assert!(errors.0.iter().any(|issue| {
            issue.path.ends_with("center_action") && issue.message.contains("unreachable")
        }));
        document.menus[0].center_action = None;
        validate(&document).unwrap();
    }

    #[test]
    fn managed_media_requires_stable_existing_kind_and_safe_record() {
        let mut document = valid();
        document.assets.push(AssetRecord {
            id: AssetId::new("glow"),
            kind: MediaKind::Image,
            relative_path: "images/glow.png".into(),
            content_sha256: "a".repeat(64),
            byte_len: 123,
        });
        document.menus[0].style.values.images.item_glow =
            Override::Value(MediaReference::Managed {
                asset_id: AssetId::new("glow"),
            });
        validate(&document).unwrap();
        document.menus[0].style.values.sounds.on_show = Override::Value(MediaReference::Managed {
            asset_id: AssetId::new("glow"),
        });
        assert!(validate(&document).unwrap_err().0.iter().any(|issue| {
            issue.path.contains("sounds.on_show") && issue.message.contains("wrong media kind")
        }));
        document.assets[0].relative_path = "../escape.png".into();
        assert!(validate(&document).unwrap_err().0.iter().any(|issue| {
            issue.path.contains("relative_path") && issue.message.contains("safe relative")
        }));
    }

    #[test]
    fn effective_style_geometry_is_validated_before_layout_or_paging() {
        let mut document = valid();
        document.menus[0].style.values.geometry.item_size = Override::Value(180.0);
        document.menus[0].style.values.geometry.center_size = Override::Value(120.0);
        let errors = validate(&document).unwrap_err();
        assert!(errors.0.iter().any(|issue| {
            issue.path.contains("rings[0]")
                && (issue.message.contains("effective style")
                    || issue.message.contains("paging controls"))
        }));
    }

    #[test]
    fn icon_opacity_is_validated_at_skin_and_cell_scopes() {
        let mut document = RadialDocument::starter();
        document.skins[0].style.values.images.icon_opacity = Override::Value(1.1);
        document.menus[0].rings[0].cells[0]
            .style
            .images
            .icon_opacity = Override::Value(-0.1);
        let errors = validate(&document).unwrap_err();
        assert!(
            errors
                .0
                .iter()
                .any(|issue| issue.path.ends_with("images.icon_opacity"))
        );
        assert!(
            errors
                .0
                .iter()
                .filter(|issue| issue.path.ends_with("images.icon_opacity"))
                .count()
                >= 2
        );
    }

    #[test]
    fn raw_process_media_handles_are_rejected_in_every_typed_external_slot() {
        for raw in ["hIcon:123", "hBitmap 99", "pBitmap=42"] {
            let mut document = valid();
            document.menus[0].rings[0].cells[0].icon =
                Override::Value(MediaReference::ExternalFile { path: raw.into() });
            assert!(validate(&document).unwrap_err().0.iter().any(|issue| {
                issue.path.ends_with(".icon") && issue.message.contains("raw process handle")
            }));
        }
        let mut document = valid();
        document.menus[0].rings[0].cells[0].icon = Override::Value(MediaReference::SearchPath {
            file_name: "hIcon.png".into(),
        });
        validate(&document).unwrap();
    }

    #[test]
    fn item_shortcuts_and_hotstrings_have_typed_scopes_and_conflict_checks() {
        let mut document = valid();
        let cell = &mut document.menus[0].rings[0].cells[0];
        cell.shortcuts = vec![
            ItemShortcut {
                id: ShortcutId::new("one"),
                chord: "Ctrl+Shift+F2".into(),
                gesture: ClickGesture::Primary,
                scope: TriggerScope::MenuLocal,
            },
            ItemShortcut {
                id: ShortcutId::new("two"),
                chord: "shift + ctrl + f2".into(),
                gesture: ClickGesture::Secondary,
                scope: TriggerScope::MenuLocal,
            },
        ];
        cell.hotstrings = vec![
            ItemHotstring {
                id: HotstringId::new("one"),
                text: "Open".into(),
                gesture: ClickGesture::Primary,
                case_sensitive: false,
                scope: TriggerScope::MenuLocal,
            },
            ItemHotstring {
                id: HotstringId::new("two"),
                text: "open".into(),
                gesture: ClickGesture::Secondary,
                case_sensitive: false,
                scope: TriggerScope::MenuLocal,
            },
        ];
        let errors = validate(&document).unwrap_err();
        assert!(
            errors
                .0
                .iter()
                .any(|issue| issue.message.contains("duplicate shortcut"))
        );
        assert!(
            errors
                .0
                .iter()
                .any(|issue| issue.message.contains("duplicate hotstring"))
        );
    }

    #[test]
    fn item_input_gesture_must_resolve_to_a_prepared_cell_binding() {
        let mut document = valid();
        let cell = &mut document.menus[0].rings[0].cells[0];
        cell.shortcuts.push(ItemShortcut {
            id: ShortcutId::new("missing-secondary"),
            chord: "Ctrl+K".into(),
            gesture: ClickGesture::Secondary,
            scope: TriggerScope::MenuLocal,
        });
        assert!(validate(&document).unwrap_err().0.iter().any(|issue| {
            issue.path.ends_with("gesture") && issue.message.contains("prepared action")
        }));
    }

    #[test]
    fn global_item_inputs_conflict_across_cells_and_use_canonical_hotkeys() {
        let mut document = valid();
        document.menus[0].rings[0].cells[0].shortcuts = vec![ItemShortcut {
            id: ShortcutId::new("global-shortcut"),
            chord: "Win+Alt+F3".into(),
            gesture: ClickGesture::Primary,
            scope: TriggerScope::Global,
        }];
        document.menus[0].rings[0].cells[1].shortcuts = vec![ItemShortcut {
            id: ShortcutId::new("local-shortcut"),
            chord: "alt + super + f3".into(),
            gesture: ClickGesture::Secondary,
            scope: TriggerScope::MenuLocal,
        }];
        document.menus[0].rings[0].cells[0].hotstrings = vec![ItemHotstring {
            id: HotstringId::new("global-hotstring"),
            text: "Launch".into(),
            gesture: ClickGesture::Primary,
            case_sensitive: false,
            scope: TriggerScope::Global,
        }];
        document.menus[0].rings[0].cells[1].hotstrings = vec![ItemHotstring {
            id: HotstringId::new("local-hotstring"),
            text: "launch".into(),
            gesture: ClickGesture::Secondary,
            case_sensitive: true,
            scope: TriggerScope::MenuLocal,
        }];
        let errors = validate(&document).unwrap_err();
        assert!(errors.0.iter().any(|issue| {
            issue.path.contains("shortcuts") && issue.message.contains("conflicts with")
        }));
        assert!(errors.0.iter().any(|issue| {
            issue.path.contains("hotstrings") && issue.message.contains("conflicts with")
        }));
    }

    #[test]
    fn item_shortcuts_share_conflict_ownership_with_menu_triggers() {
        let mut document = valid();
        document.custom_triggers.push(TriggerDefinition {
            id: TriggerId::new("direct"),
            chord: "Ctrl+F4".into(),
            menu_id: document.default_menu_id.clone(),
            scope: TriggerScope::Global,
        });
        document.menus[0].rings[0].cells[0].shortcuts = vec![ItemShortcut {
            id: ShortcutId::new("item"),
            chord: "control + f4".into(),
            gesture: ClickGesture::Primary,
            scope: TriggerScope::MenuLocal,
        }];
        let errors = validate(&document).unwrap_err();
        assert!(errors.0.iter().any(|issue| {
            issue.path.contains("shortcuts") && issue.message.contains("custom_triggers[0]")
        }));
    }

    #[test]
    fn rejects_duplicates_missing_refs_and_bad_numbers() {
        let mut d = valid();
        d.menus[0].rings[0].id = RingId::new("");
        d.menus[0].skin_id = SkinId::new("missing");
        d.menus[0].center_radius = f32::NAN;
        let e = validate(&d).unwrap_err();
        assert!(e.0.iter().any(|e| e.path.ends_with("skin_id")));
        assert!(e.0.iter().any(|e| e.message.contains("finite")));
        assert!(e.0.iter().any(|e| e.message.contains("empty")));
    }
    #[test]
    fn reports_cycle_path_and_excessive_depth() {
        let mut d = valid();
        let original = d.menus[0].clone();
        d.menus[0].rings[0].cells[0].content = CellContent::Submenu {
            menu_id: MenuId::new("child"),
        };
        let mut child = original;
        child.id = MenuId::new("child");
        child.rings[0].cells[0].content = CellContent::Submenu {
            menu_id: MenuId::new("starter"),
        };
        d.menus.push(child);
        let e = validate(&d).unwrap_err();
        assert!(
            e.0.iter()
                .any(|e| e.message.contains("starter -> child -> starter"))
        );
    }

    #[test]
    fn rejects_excessive_depth_and_resource_count() {
        let template = valid().menus[0].clone();
        let mut d = valid();
        d.menus.clear();
        for index in 0..=limits::MAX_SUBMENU_DEPTH {
            let mut menu = template.clone();
            menu.id = MenuId::new(format!("menu-{index}"));
            if index < limits::MAX_SUBMENU_DEPTH {
                menu.rings[0].cells[0].content = CellContent::Submenu {
                    menu_id: MenuId::new(format!("menu-{}", index + 1)),
                };
            }
            d.menus.push(menu);
        }
        d.default_menu_id = MenuId::new("menu-0");
        assert!(
            validate(&d)
                .unwrap_err()
                .0
                .iter()
                .any(|error| error.message.contains("depth exceeds"))
        );

        d.menus = (0..=limits::MAX_MENUS)
            .map(|index| {
                let mut menu = template.clone();
                menu.id = MenuId::new(format!("flat-{index}"));
                menu
            })
            .collect();
        d.default_menu_id = MenuId::new("flat-0");
        assert!(
            validate(&d)
                .unwrap_err()
                .0
                .iter()
                .any(|error| error.message.contains("limit is 256"))
        );
    }
    #[test]
    fn catches_trigger_conflicts_and_invalid_targets() {
        let mut d = valid();
        d.custom_triggers = vec![
            TriggerDefinition {
                id: TriggerId::new("a"),
                chord: "Shift+Control+F2".into(),
                menu_id: MenuId::new("starter"),
                scope: TriggerScope::MenuLocal,
            },
            TriggerDefinition {
                id: TriggerId::new("b"),
                chord: "f2 + ctrl + shift".into(),
                menu_id: MenuId::new("starter"),
                scope: TriggerScope::MenuLocal,
            },
        ];
        let e = validate(&d).unwrap_err();
        assert!(e.0.iter().any(|e| e.message.contains("conflicts")));

        d.custom_triggers.clear();
        d.menus[0].rings[0].cells[0].content = CellContent::Action {
            binding: ActionBinding::Persisted {
                action: PersistedUniversalActionRef {
                    target: Some(PersistableActionTargetRef::Note {
                        slug: String::new(),
                    }),
                    action_id: crate::universal_actions::ActionId::new("note.open"),
                },
            },
        };
        assert!(
            validate(&d)
                .unwrap_err()
                .0
                .iter()
                .any(|error| error.message.contains("target identity"))
        );
    }

    #[test]
    fn keep_open_incompatibility_is_reported_for_secondary_special_surfaces() {
        let mut document = valid();
        let action = crate::actions::Action {
            label: "Draw".into(),
            desc: String::new(),
            action: "screen_draw:start".into(),
            args: None,
        };
        document.menus[0].center_secondary_action = Some(ActionBinding::Persisted {
            action: PersistedUniversalActionRef {
                target: Some(PersistableActionTargetRef::CustomAction { action }),
                action_id: crate::universal_actions::ActionId::new("result.execute"),
            },
        });
        document.menus[0].center_secondary_after_action = AfterActionPolicy::KeepOpen;
        let errors = validate(&document).unwrap_err();
        assert!(errors.0.iter().any(|issue| {
            issue.path.contains("center_secondary_action")
                && issue.message.contains("ExclusiveCapture")
        }));
    }

    #[test]
    fn primary_special_surface_policy_is_validated_independently() {
        let mut document = valid();
        document.menus[0].center_control = None;
        let binding = ActionBinding::Persisted {
            action: PersistedUniversalActionRef {
                target: Some(PersistableActionTargetRef::Note {
                    slug: "daily".into(),
                }),
                action_id: crate::universal_actions::action_ids::NOTE_OPEN_NOTEPAD,
            },
        };
        document.menus[0].center_action = Some(binding.clone());
        document.menus[0].center_primary_after_action = AfterActionPolicy::CloseTree;
        document.menus[0].background_action = Some(binding);
        document.menus[0].background_primary_after_action = AfterActionPolicy::KeepOpen;
        let errors = validate(&document).unwrap_err();
        assert!(errors.0.iter().any(|issue| {
            issue.path.contains("background_action.after_action")
                && issue.message.contains("ExternalInput")
        }));
        assert!(
            !errors
                .0
                .iter()
                .any(|issue| issue.path.contains("center_action.after_action"))
        );
    }

    #[test]
    fn shared_submenu_dag_is_memoized_and_bounded_by_edges() {
        let mut d = valid();
        let mut template = d.menus.remove(0);
        d.menus.clear();
        for cell in &mut template.rings[0].cells {
            cell.content = CellContent::Spacer;
        }
        for level in 0..limits::MAX_SUBMENU_DEPTH {
            let mut menu = template.clone();
            menu.id = MenuId::new(format!("shared-{level}"));
            menu.layout = LayoutKind::Wedges;
            if level + 1 < limits::MAX_SUBMENU_DEPTH {
                menu.rings[0].cells = (0..limits::MAX_CELLS_PER_RING)
                    .map(|cell| {
                        let mut definition = template.rings[0].cells[0].clone();
                        definition.id = CellId::new(format!("cell-{level}-{cell}"));
                        definition.content = CellContent::Submenu {
                            menu_id: MenuId::new(format!("shared-{}", level + 1)),
                        };
                        definition
                    })
                    .collect();
            }
            d.menus.push(menu);
        }
        d.default_menu_id = MenuId::new("shared-0");
        validate(&d).unwrap();
    }
}
