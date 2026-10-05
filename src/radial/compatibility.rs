#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatibilityClassification {
    Native,
    Translated,
    Incompatible,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompatibilitySource {
    Radify,
    RadialMenuV4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompatibilityFieldGroup {
    pub source: CompatibilitySource,
    pub fields: &'static [&'static str],
    pub classification: CompatibilityClassification,
    pub runtime: &'static str,
    pub editor: &'static str,
    pub import: &'static str,
    pub rationale: &'static str,
}

pub const FIELD_REGISTRY: &[CompatibilityFieldGroup] = &[
    CompatibilityFieldGroup {
        source: CompatibilitySource::Radify,
        fields: &[
            "Skin",
            "ItemGlowImage",
            "MenuOuterRimImage",
            "MenuBackgroundImage",
            "ItemBackgroundImage",
            "CenterBackgroundImage",
            "CenterImage",
            "SubmenuIndicatorImage",
        ],
        classification: CompatibilityClassification::Native,
        runtime: "typed image references resolved by the M4 asset/style pipeline",
        editor: "image selectors with inherited/value/clear state in the M5 skin inspector",
        import: "recognized file/search-root/resource references become typed MediaReference values",
        rationale: "portable image identities have direct native equivalents",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::Radify,
        fields: &[
            "ItemSize",
            "RadiusScale",
            "CenterSize",
            "CenterImageScale",
            "ItemImageScale",
            "ItemImageYRatio",
            "SubmenuIndicatorSize",
            "SubmenuIndicatorYRatio",
            "OuterRingMargin",
            "OuterRimWidth",
            "ItemBackgroundImageOnCenter",
            "ItemBackgroundImageOnItems",
        ],
        classification: CompatibilityClassification::Native,
        runtime: "typed geometry overrides participate in effective-style layout",
        editor: "bounded numeric/boolean controls expose inheritance and explicit zero/false",
        import: "finite literal values map directly to geometry style fields",
        rationale: "these values describe portable geometry rather than backend objects",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::Radify,
        fields: &[
            "EnableItemText",
            "TextColor",
            "TextFont",
            "TextSize",
            "TextFontOptions",
            "TextShadowColor",
            "TextShadowOffset",
            "TextBoxScale",
            "TextYRatio",
        ],
        classification: CompatibilityClassification::Native,
        runtime: "typed text overrides feed the shared text layout/render path",
        editor: "font, color, option, shadow, and sizing controls are planned for M5",
        import: "recognized literals map to typed text fields; unknown font flags are diagnosed",
        rationale: "font and label presentation are portable with system fallback",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::Radify,
        fields: &[
            "MenuClick",
            "MenuRightClick",
            "CenterClick",
            "CenterRightClick",
            "MirrorClickToRightClick",
            "CloseOnItemClick",
            "CloseOnItemRightClick",
        ],
        classification: CompatibilityClassification::Native,
        runtime: "existing typed bindings and after-action policies own behavior",
        editor: "M5 menu/cell inspectors edit actions and policies independently per button",
        import: "known actions map to typed bindings; executable callbacks are rejected",
        rationale: "native behavior types already replace callback identity",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::Radify,
        fields: &[
            "EnableGlow",
            "AutoTooltip",
            "EnableTooltip",
            "AlwaysOnTop",
            "ActivateOnShow",
            "FillCenterHitZone",
            "FillItemsHitZone",
            "SoundOnShow",
            "SoundOnClose",
            "SoundOnSelect",
            "SoundOnSubShow",
            "SoundOnSubClose",
        ],
        classification: CompatibilityClassification::Native,
        runtime: "typed effect/hit-zone/sound policies and per-menu topmost/activation flags feed the retained native host; no-activate remains the default",
        editor: "M5 exposes explicit booleans, tooltip mode, and sound selectors",
        import: "recognized values map directly while preserving false and clear",
        rationale: "the approved native host has equivalent bounded policies",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::Radify,
        fields: &["TextRendering", "SmoothingMode", "InterpolationMode"],
        classification: CompatibilityClassification::Translated,
        runtime: "legacy backend integers map to named RenderingQuality modes",
        editor: "named quality choices avoid exposing GDI+ implementation integers",
        import: "known integers receive deterministic approximate mappings and diagnostics",
        rationale: "native rendering can preserve intent but not claim identical GDI+ output",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::Radify,
        fields: &[
            "Image",
            "Tooltip",
            "Text",
            "Click",
            "RightClick",
            "CtrlClick",
            "ShiftClick",
            "AltClick",
        ],
        classification: CompatibilityClassification::Native,
        runtime: "cell media/text and exact typed click gestures drive native dispatch",
        editor: "M5 cell inspector owns these fields",
        import: "data values and recognized actions map without evaluating callbacks",
        rationale: "each public item property has a stable native cell field",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::Radify,
        fields: &[
            "HotkeyClick",
            "HotkeyRightClick",
            "HotkeyCtrlClick",
            "HotkeyShiftClick",
            "HotkeyAltClick",
            "HotstringClick",
            "HotstringRightClick",
            "HotstringCtrlClick",
            "HotstringShiftClick",
            "HotstringAltClick",
            "Submenu",
            "SubmenuOptions",
        ],
        classification: CompatibilityClassification::Native,
        runtime: "the existing synchronous keyboard service recognizes bounded memory-only scoped inputs and routes all five gestures through prepared radial dispatch",
        editor: "M5 edits scope, gesture, text/chord, and submenu options explicitly",
        import: "local bindings map directly; global variants require explicit opt-in and conflict checks",
        rationale: "typed scope, explicit global opt-in, reservation checks, and provenance filtering prevent legacy callbacks or global key history from leaking into runtime",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::Radify,
        fields: &[
            "AutoCenterMouse",
            "CloseMenuBlock",
            "GuiOptions",
            "hIcon",
            "hBitmap",
            "pBitmap",
        ],
        classification: CompatibilityClassification::Incompatible,
        runtime: "never enabled or interpreted",
        editor: "shown only as an incompatibility diagnostic with safe alternatives",
        import: "rejected with a field-specific diagnostic; file-based media may be selected instead",
        rationale: "cursor warp, disabled recovery, raw flags, and process-local handles violate product or portability constraints",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::RadialMenuV4,
        fields: &[
            "MenuClick",
            "MenuRightClick",
            "CenterClick",
            "CenterRightClick",
            "Click",
            "RightClick",
            "CtrlClick",
            "ShiftClick",
            "AltClick",
        ],
        classification: CompatibilityClassification::Native,
        runtime: "typed surface/cell bindings and after-action policies own imported behavior",
        editor: "M5 exposes each gesture and surface binding without callback identities",
        import: "only Close, CloseMenu, and Drag literals become typed controls; other callbacks are diagnosed and ignored",
        rationale: "known non-executable controls have exact native intents",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::RadialMenuV4,
        fields: &[
            "SkinName",
            "ItemSize",
            "RadiusSizeFactor",
            "AutoSubmenuMarking",
            "AutoSubmenuMark",
            "ItemGlow",
            "TextBoxShrink",
            "TextFont",
            "TextSize",
            "TextColor",
            "TextTrans",
            "TextShadow",
            "TextShadowColor",
            "TextShadowTrans",
            "TextShadowOffset",
            "IconShrink",
            "ItemBack",
            "ItemBackShrink",
            "ItemBackTrans",
            "ItemFore",
            "ItemForeShrink",
            "ItemForeTrans",
            "ItemShadow",
            "ItemShadowShrink",
            "ItemShadowTrans",
            "MenuBack",
            "MenuBackTrans",
            "MenuBackOuterRim",
            "MenuBackOuterRimWidth",
            "MenuBackOuterRimTrans",
            "MenuFore",
            "MenuForeTrans",
            "MenuShadowWidth",
            "MenuShadowInnerColor",
            "MenuShadowOuterColor",
            "MenuBackCenter",
            "MenuBackCenterShrink",
        ],
        classification: CompatibilityClassification::Native,
        runtime: "typed M4 image/geometry/text/effect fields provide the native equivalent",
        editor: "M5 style inspectors expose supported values and their inheritance source",
        import: "literal assignments are parsed as data into typed overrides",
        rationale: "the inspected fields describe bounded portable presentation",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::RadialMenuV4,
        fields: &["TextRendering", "MenuBackSize", "MenuForeSize"],
        classification: CompatibilityClassification::Translated,
        runtime: "quality names and bounded fac/add size adjustments approximate legacy intent",
        editor: "native named quality and explicit size controls show translated values",
        import: "known integers and fac/add expressions are parsed without evaluation",
        rationale: "backend-specific or expression syntax is converted to bounded native data",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::RadialMenuV4,
        fields: &["IconTrans"],
        classification: CompatibilityClassification::Translated,
        runtime: "scalar opacity maps to a typed image opacity override",
        editor: "the translated opacity is visible as a normal inherited/value/clear control",
        import: "scalar values translate; ARGB and arbitrary matrix forms receive field-local incompatible diagnostics",
        rationale: "the field is portable only for the scalar form found in several supplied skins",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::RadialMenuV4,
        fields: &[
            "MenuBackHatchStyle",
            "MenuBackHatchFrontColor",
            "MenuBackHatchBackColor",
            "MenuBackHatchShrink",
            "MenuBackOuterRimHatchStyle",
            "MenuBackOuterRimHatchFrontColor",
            "MenuBackOuterRimHatchBackColor",
            "MenuBackOuterRimHatchShrink",
            "ItemBackHatchStyle",
            "ItemBackHatchFrontColor",
            "ItemBackHatchBackColor",
            "ItemBackHatchShrink",
        ],
        classification: CompatibilityClassification::Incompatible,
        runtime: "arbitrary color matrices and hatch backends are not evaluated",
        editor: "import diagnostics explain the unsupported effect and retain no hidden setting",
        import: "simple scalar opacity may be translated later; matrices/hatches are rejected now",
        rationale: "the supplied forms include unbounded backend-specific matrices without a safe equivalent",
    },
    CompatibilityFieldGroup {
        source: CompatibilitySource::RadialMenuV4,
        fields: &[
            "RMProcessPriority",
            "AutoCheckForUpdates",
            "RunSoundPlayers",
            "CreateShortcut",
            "FileExtensions",
            "AHKFunction",
            "AHKVariable",
            "PowerScript",
        ],
        classification: CompatibilityClassification::NotApplicable,
        runtime: "owned by the application or intentionally absent from radial configuration",
        editor: "not exposed as a radial style option",
        import: "reported and ignored without executing content",
        rationale: "these settings configure the legacy host/application rather than a portable menu or skin",
    },
];

pub fn compatibility_for(
    source: CompatibilitySource,
    field: &str,
) -> Option<&'static CompatibilityFieldGroup> {
    FIELD_REGISTRY
        .iter()
        .find(|group| group.source == source && group.fields.contains(&field))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_registry_group_has_runtime_editor_import_and_rationale() {
        assert!(!FIELD_REGISTRY.is_empty());
        let mut registered = BTreeSet::new();
        for group in FIELD_REGISTRY {
            assert!(!group.fields.is_empty());
            assert!(!group.runtime.is_empty());
            assert!(!group.editor.is_empty());
            assert!(!group.import.is_empty());
            assert!(!group.rationale.is_empty());
            for field in group.fields {
                assert!(
                    registered.insert((group.source, *field)),
                    "duplicate compatibility entry for {field}"
                );
            }
        }
    }

    #[test]
    fn registry_covers_the_approved_radify_and_inspected_rm4_fields() {
        let radify = [
            "Skin",
            "ItemGlowImage",
            "MenuOuterRimImage",
            "MenuBackgroundImage",
            "ItemBackgroundImage",
            "CenterBackgroundImage",
            "CenterImage",
            "SubmenuIndicatorImage",
            "ItemSize",
            "RadiusScale",
            "CenterSize",
            "CenterImageScale",
            "ItemImageScale",
            "ItemImageYRatio",
            "SubmenuIndicatorSize",
            "SubmenuIndicatorYRatio",
            "OuterRingMargin",
            "OuterRimWidth",
            "ItemBackgroundImageOnCenter",
            "ItemBackgroundImageOnItems",
            "EnableItemText",
            "TextColor",
            "TextFont",
            "TextSize",
            "TextFontOptions",
            "TextShadowColor",
            "TextShadowOffset",
            "TextBoxScale",
            "TextYRatio",
            "MenuClick",
            "MenuRightClick",
            "CenterClick",
            "CenterRightClick",
            "MirrorClickToRightClick",
            "CloseOnItemClick",
            "CloseOnItemRightClick",
            "EnableGlow",
            "AutoTooltip",
            "EnableTooltip",
            "AlwaysOnTop",
            "ActivateOnShow",
            "FillCenterHitZone",
            "FillItemsHitZone",
            "SoundOnShow",
            "SoundOnClose",
            "SoundOnSelect",
            "SoundOnSubShow",
            "SoundOnSubClose",
            "TextRendering",
            "SmoothingMode",
            "InterpolationMode",
            "Image",
            "Tooltip",
            "Text",
            "Click",
            "RightClick",
            "CtrlClick",
            "ShiftClick",
            "AltClick",
            "HotkeyClick",
            "HotkeyRightClick",
            "HotkeyCtrlClick",
            "HotkeyShiftClick",
            "HotkeyAltClick",
            "HotstringClick",
            "HotstringRightClick",
            "HotstringCtrlClick",
            "HotstringShiftClick",
            "HotstringAltClick",
            "Submenu",
            "SubmenuOptions",
            "AutoCenterMouse",
            "CloseMenuBlock",
            "GuiOptions",
            "hIcon",
            "hBitmap",
            "pBitmap",
        ];
        let rm4 = [
            "SkinName",
            "ItemSize",
            "RadiusSizeFactor",
            "AutoSubmenuMarking",
            "AutoSubmenuMark",
            "ItemGlow",
            "TextBoxShrink",
            "TextFont",
            "TextSize",
            "TextColor",
            "TextTrans",
            "TextRendering",
            "TextShadow",
            "TextShadowColor",
            "TextShadowTrans",
            "TextShadowOffset",
            "IconShrink",
            "IconTrans",
            "ItemBack",
            "ItemBackShrink",
            "ItemBackTrans",
            "ItemFore",
            "ItemForeShrink",
            "ItemForeTrans",
            "ItemShadow",
            "ItemShadowShrink",
            "ItemShadowTrans",
            "MenuBack",
            "MenuBackSize",
            "MenuBackTrans",
            "MenuBackOuterRim",
            "MenuBackOuterRimWidth",
            "MenuBackOuterRimTrans",
            "MenuFore",
            "MenuForeSize",
            "MenuForeTrans",
            "MenuShadowWidth",
            "MenuShadowInnerColor",
            "MenuShadowOuterColor",
            "MenuBackCenter",
            "MenuBackCenterShrink",
            "MenuBackHatchStyle",
            "MenuBackHatchFrontColor",
            "MenuBackHatchBackColor",
            "MenuBackHatchShrink",
            "MenuBackOuterRimHatchStyle",
            "MenuBackOuterRimHatchFrontColor",
            "MenuBackOuterRimHatchBackColor",
            "MenuBackOuterRimHatchShrink",
            "ItemBackHatchStyle",
            "ItemBackHatchFrontColor",
            "ItemBackHatchBackColor",
            "ItemBackHatchShrink",
            "RMProcessPriority",
            "AutoCheckForUpdates",
            "RunSoundPlayers",
            "CreateShortcut",
            "FileExtensions",
            "AHKFunction",
            "AHKVariable",
            "PowerScript",
            "MenuClick",
            "MenuRightClick",
            "CenterClick",
            "CenterRightClick",
            "Click",
            "RightClick",
            "CtrlClick",
            "ShiftClick",
            "AltClick",
        ];
        for (source, expected) in [
            (CompatibilitySource::Radify, radify.as_slice()),
            (CompatibilitySource::RadialMenuV4, rm4.as_slice()),
        ] {
            let registered = FIELD_REGISTRY
                .iter()
                .filter(|group| group.source == source)
                .flat_map(|group| group.fields.iter().copied())
                .collect::<BTreeSet<_>>();
            assert_eq!(registered, expected.iter().copied().collect());
        }
    }

    #[test]
    fn raw_handles_and_backend_quality_have_truthful_classification() {
        assert_eq!(
            compatibility_for(CompatibilitySource::Radify, "hIcon")
                .unwrap()
                .classification,
            CompatibilityClassification::Incompatible
        );
        assert_eq!(
            compatibility_for(CompatibilitySource::Radify, "TextRendering")
                .unwrap()
                .classification,
            CompatibilityClassification::Translated
        );
        assert_eq!(
            compatibility_for(CompatibilitySource::RadialMenuV4, "RMProcessPriority")
                .unwrap()
                .classification,
            CompatibilityClassification::NotApplicable
        );
        let close_block = compatibility_for(CompatibilitySource::Radify, "CloseMenuBlock").unwrap();
        assert_eq!(
            close_block.classification,
            CompatibilityClassification::Incompatible
        );
        assert!(close_block.rationale.contains("recovery"));
    }
}
