mod preferences;

use crate::unit_conversion::{self, Category, ConversionError, ConversionOutcome, Unit};
use eframe::egui;
use preferences::{DisplayPreferences, NumberFormat, PreferencesState};

const MAX_TARGETS: usize = 6;
const WIDE_LAYOUT_MIN_WIDTH: f32 = 640.0;

fn converter_root_id() -> egui::Id {
    egui::Id::new("convert-panel-window")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelCategory {
    Physical(Category),
    Base,
}

impl PanelCategory {
    fn label(self) -> &'static str {
        match self {
            Self::Physical(Category::Length) => "Distance",
            Self::Physical(category) => category.display_name(),
            Self::Base => "Base",
        }
    }
}

fn category_options() -> impl Iterator<Item = PanelCategory> {
    Category::ALL
        .iter()
        .copied()
        .map(PanelCategory::Physical)
        .chain(std::iter::once(PanelCategory::Base))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum BaseUnit {
    Decimal,
    Hexadecimal,
    Binary,
    Octal,
}

impl BaseUnit {
    const ALL: &'static [Self] = &[Self::Decimal, Self::Hexadecimal, Self::Binary, Self::Octal];

    fn label(self) -> &'static str {
        match self {
            Self::Decimal => "dec",
            Self::Hexadecimal => "hex",
            Self::Binary => "bin",
            Self::Octal => "oct",
        }
    }

    fn radix(self) -> u32 {
        match self {
            Self::Decimal => 10,
            Self::Hexadecimal => 16,
            Self::Binary => 2,
            Self::Octal => 8,
        }
    }

    fn numeric_base(self) -> crate::common::numeric_base::NumericBase {
        use crate::common::numeric_base::NumericBase;
        match self {
            Self::Decimal => NumericBase::Decimal,
            Self::Hexadecimal => NumericBase::Hexadecimal,
            Self::Binary => NumericBase::Binary,
            Self::Octal => NumericBase::Octal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PanelUnit {
    Physical(Unit),
    Base(BaseUnit),
}

impl PanelUnit {
    fn label(self) -> &'static str {
        match self {
            Self::Physical(unit) => unit.symbol(),
            Self::Base(unit) => unit.label(),
        }
    }

    fn belongs_to(self, category: PanelCategory) -> bool {
        match (category, self) {
            (PanelCategory::Physical(category), Self::Physical(unit)) => {
                unit.category() == category
            }
            (PanelCategory::Base, Self::Base(_)) => true,
            _ => false,
        }
    }

    fn matches_filter(self, filter: &str) -> bool {
        if filter.is_empty() {
            return true;
        }
        match self {
            Self::Physical(unit) => unit_conversion::catalog()
                .iter()
                .find(|definition| definition.unit == unit)
                .is_some_and(|definition| {
                    std::iter::once(definition.symbol)
                        .chain(definition.aliases.iter().copied())
                        .chain(definition.case_sensitive_aliases.iter().copied())
                        .any(|candidate| candidate.to_lowercase().contains(filter))
                }),
            Self::Base(unit) => unit.label().to_lowercase().contains(filter),
        }
    }
}

fn category_units(category: PanelCategory) -> Vec<PanelUnit> {
    match category {
        PanelCategory::Physical(category) => unit_conversion::units_in_category(category)
            .map(|definition| PanelUnit::Physical(definition.unit))
            .collect(),
        PanelCategory::Base => BaseUnit::ALL.iter().copied().map(PanelUnit::Base).collect(),
    }
}

fn filtered_units(category: PanelCategory, filter: &str) -> Vec<PanelUnit> {
    let filter = filter.trim().to_lowercase();
    category_units(category)
        .into_iter()
        .filter(|unit| unit.matches_filter(&filter))
        .collect()
}

fn filtered_favorites(preferences: &DisplayPreferences, category: PanelCategory) -> Vec<PanelUnit> {
    preferences
        .favorites
        .iter()
        .copied()
        .filter(|unit| unit.belongs_to(category))
        .collect()
}

fn reconcile_selection_values(
    category: PanelCategory,
    from_options: &[PanelUnit],
    to_options: &[PanelUnit],
    from: &mut Option<PanelUnit>,
    targets: &mut Vec<Option<PanelUnit>>,
) {
    let valid_options = category_units(category);
    let from_fallback = from_options
        .first()
        .copied()
        .or_else(|| valid_options.first().copied());
    let to_fallback = to_options
        .first()
        .copied()
        .or_else(|| valid_options.first().copied());

    if !from.is_some_and(|selected| valid_options.contains(&selected)) {
        *from = from_fallback;
    }
    if targets.is_empty() {
        targets.push(to_fallback);
    }
    for target in targets {
        if !target.is_some_and(|selected| valid_options.contains(&selected)) {
            *target = to_fallback;
        }
    }
}

fn next_target(category: PanelCategory, targets: &[Option<PanelUnit>]) -> Option<PanelUnit> {
    let options = category_units(category);
    options
        .iter()
        .copied()
        .find(|option| !targets.contains(&Some(*option)))
        .or_else(|| options.first().copied())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConversionFailure {
    Physical(ConversionError),
    InvalidBaseDigits { radix: u32 },
    BaseMagnitudeOutOfRange,
    IncompatibleUnitKinds,
}

impl std::fmt::Display for ConversionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Physical(error) => std::fmt::Display::fmt(error, formatter),
            Self::InvalidBaseDigits { radix } => {
                write!(formatter, "Enter a valid base-{radix} integer")
            }
            Self::BaseMagnitudeOutOfRange => {
                formatter.write_str("Base conversion is limited to signed 64-bit values")
            }
            Self::IncompatibleUnitKinds => {
                write!(formatter, "Select units from the same conversion type")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TargetValue {
    Physical {
        outcome: ConversionOutcome,
        display: String,
    },
    Base {
        display: String,
    },
}

impl TargetValue {
    fn display(&self) -> &str {
        match self {
            Self::Physical { display, .. } | Self::Base { display } => display,
        }
    }

    fn is_approximate(&self) -> bool {
        matches!(self, Self::Physical { outcome, .. } if outcome.is_approximate())
    }
}

#[derive(Debug, Clone, PartialEq)]
struct TargetResult {
    target: PanelUnit,
    value: Result<TargetValue, ConversionFailure>,
}

fn format_number(value: f64, preferences: &DisplayPreferences) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    match preferences.number_format {
        NumberFormat::Smart => crate::common::number_format::format_number(value),
        NumberFormat::Fixed => Some(format!("{:.*}", preferences.decimal_places as usize, value)),
        NumberFormat::Scientific => Some(format!(
            "{:.*e}",
            preferences.decimal_places as usize, value
        )),
    }
}

fn convert_base(input: &str, from: BaseUnit, to: BaseUnit) -> Result<String, ConversionFailure> {
    let trimmed = input.trim();
    let (outer_negative, remaining) = trimmed
        .strip_prefix('-')
        .map_or((false, trimmed), |rest| (true, rest));
    let parsed = match i64::from_str_radix(remaining, from.radix()) {
        Ok(parsed) => parsed,
        Err(error)
            if matches!(
                error.kind(),
                std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow
            ) =>
        {
            return Err(ConversionFailure::BaseMagnitudeOutOfRange);
        }
        Err(_) => {
            return Err(ConversionFailure::InvalidBaseDigits {
                radix: from.radix(),
            });
        }
    };
    let signed = if outer_negative {
        parsed
            .checked_neg()
            .ok_or(ConversionFailure::BaseMagnitudeOutOfRange)?
    } else {
        parsed
    };

    if signed < 0 {
        Ok(match to {
            BaseUnit::Decimal => signed.to_string(),
            BaseUnit::Hexadecimal => format!("{signed:x}"),
            BaseUnit::Binary => format!("{signed:b}"),
            BaseUnit::Octal => format!("{signed:o}"),
        })
    } else {
        Ok(crate::common::numeric_base::format_unsigned(
            signed as u128,
            to.numeric_base(),
        ))
    }
}

fn convert_target(
    input: &str,
    from: Option<PanelUnit>,
    target: PanelUnit,
    preferences: &DisplayPreferences,
) -> Result<TargetValue, ConversionFailure> {
    match (from, target) {
        (Some(PanelUnit::Physical(from)), PanelUnit::Physical(to)) => {
            let expression = format!("{} {} to {}", input.trim(), from.symbol(), to.symbol());
            let outcome = unit_conversion::evaluate_conversion(&expression)
                .map_err(ConversionFailure::Physical)?;
            let formatted = format_number(outcome.value, preferences).ok_or_else(|| {
                ConversionFailure::Physical(ConversionError::OutOfRange {
                    expression: expression.clone(),
                })
            })?;
            let display = if outcome.is_approximate() {
                format!("{formatted} {} (approx.)", to.symbol())
            } else {
                format!("{formatted} {}", to.symbol())
            };
            Ok(TargetValue::Physical { outcome, display })
        }
        (Some(PanelUnit::Base(from)), PanelUnit::Base(to)) => {
            let value = convert_base(input, from, to)?;
            let display = format!("{value} {}", to.label());
            Ok(TargetValue::Base { display })
        }
        _ => Err(ConversionFailure::IncompatibleUnitKinds),
    }
}

fn calculate_targets(
    input: &str,
    from: Option<PanelUnit>,
    targets: &[Option<PanelUnit>],
    preferences: &DisplayPreferences,
) -> Vec<TargetResult> {
    if input.trim().is_empty() {
        return Vec::new();
    }
    targets
        .iter()
        .flatten()
        .copied()
        .map(|target| TargetResult {
            target,
            value: convert_target(input, from, target, preferences),
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ConvertPanelEffect {
    CopyText(String),
    SavePreferences,
}

struct RootEffects {
    frame_nr: u64,
    pass_nr: u64,
    input: String,
    from_filter: String,
    to_filter: String,
    category: PanelCategory,
    from: Option<PanelUnit>,
    targets: Vec<Option<PanelUnit>>,
    preferences: DisplayPreferences,
    focus_requested: bool,
    input_id: Option<egui::Id>,
    focus_add_target: bool,
    add_target_id: Option<egui::Id>,
    favorite_limit_hit: bool,
    window_open: bool,
    effect: Option<ConvertPanelEffect>,
}

/// Unit conversion controls and presentation state. Preference IO is configured
/// only by production construction; `Default` remains filesystem-free.
pub struct ConvertPanel {
    pub open: bool,
    input: String,
    from_filter: String,
    to_filter: String,
    category: PanelCategory,
    from: Option<PanelUnit>,
    targets: Vec<Option<PanelUnit>>,
    focus_input: bool,
    preferences: PreferencesState,
    copy_feedback: Option<Result<(), String>>,
    pending_root_effects: Option<RootEffects>,
    host_close_requested: bool,
    #[cfg(test)]
    test_window_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_input_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_input_id: Option<egui::Id>,
    #[cfg(test)]
    test_content_viewport: Option<egui::Rect>,
    #[cfg(test)]
    test_results_viewport: Option<egui::Rect>,
    #[cfg(test)]
    test_control_rects: Vec<(String, egui::Rect)>,
    #[cfg(test)]
    test_target_row_rects: Vec<(usize, egui::Rect)>,
    #[cfg(test)]
    test_save_preferences_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_save_preferences_id: Option<egui::Id>,
    #[cfg(test)]
    test_copy_rects: Vec<(usize, egui::Rect)>,
    #[cfg(test)]
    test_copy_ids: Vec<(usize, egui::Id)>,
    #[cfg(test)]
    test_swap_rects: Vec<(usize, egui::Rect)>,
    #[cfg(test)]
    test_add_target_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_add_target_id: Option<egui::Id>,
    #[cfg(test)]
    test_remove_rects: Vec<(usize, egui::Rect)>,
}

impl Default for ConvertPanel {
    fn default() -> Self {
        Self {
            open: false,
            input: String::new(),
            from_filter: String::new(),
            to_filter: String::new(),
            category: PanelCategory::Physical(Category::Length),
            from: None,
            targets: vec![None],
            focus_input: false,
            preferences: PreferencesState::default(),
            copy_feedback: None,
            pending_root_effects: None,
            host_close_requested: false,
            #[cfg(test)]
            test_window_rect: None,
            #[cfg(test)]
            test_input_rect: None,
            #[cfg(test)]
            test_input_id: None,
            #[cfg(test)]
            test_content_viewport: None,
            #[cfg(test)]
            test_results_viewport: None,
            #[cfg(test)]
            test_control_rects: Vec::new(),
            #[cfg(test)]
            test_target_row_rects: Vec::new(),
            #[cfg(test)]
            test_save_preferences_rect: None,
            #[cfg(test)]
            test_save_preferences_id: None,
            #[cfg(test)]
            test_copy_rects: Vec::new(),
            #[cfg(test)]
            test_copy_ids: Vec::new(),
            #[cfg(test)]
            test_swap_rects: Vec::new(),
            #[cfg(test)]
            test_add_target_rect: None,
            #[cfg(test)]
            test_add_target_id: None,
            #[cfg(test)]
            test_remove_rects: Vec::new(),
        }
    }
}

impl ConvertPanel {
    /// Creates a panel with the app-owned preference file loaded, without writing it.
    pub fn with_data_root(root: &crate::platform::app_data::AppDataRoot) -> Self {
        let mut panel = Self {
            preferences: PreferencesState::with_data_root(root),
            ..Self::default()
        };
        panel.reconcile_selections();
        panel
    }

    /// Open the panel and focus the value input once.
    pub fn open(&mut self) {
        self.open = true;
        self.host_close_requested = false;
        self.focus_input = true;
    }

    /// Stage a host close request for the next retained ROOT frame.
    pub(crate) fn request_close(&mut self) {
        if self.open {
            self.host_close_requested = true;
        }
    }

    /// Draw the panel and return deferred clipboard or preference effects.
    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
    ) -> Option<ConvertPanelEffect> {
        #[cfg(test)]
        {
            self.test_window_rect = None;
            self.test_input_rect = None;
        }
        if !self.open {
            return None;
        }

        let frame_nr = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT);
        let pass_nr = ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT);

        let mut window_open = self.open;
        let mut input = self.input.clone();
        let mut from_filter = self.from_filter.clone();
        let mut to_filter = self.to_filter.clone();
        let mut category = self.category;
        let mut from = self.from;
        let mut targets = self.targets.clone();
        let mut preferences = self.preferences.current.clone();
        let mut authorized_pass = false;
        let mut focus_requested = false;
        let mut input_id = None;
        let mut add_target_id = None;
        let mut focus_add_target = false;
        let mut favorite_limit_hit = false;
        let mut effect = None;
        let content_rect = ctx.content_rect();
        let min_size = egui::vec2(390.0, 300.0).min(content_rect.size());
        let output = egui::Window::new("Convert")
            .id(converter_root_id())
            .open(&mut window_open)
            .resizable(true)
            .default_size(egui::vec2(760.0, 590.0))
            .min_size(min_size)
            .max_size(content_rect.size())
            .show(ctx, |ui| {
                authorized_pass =
                    first_ui_pass && !ctx.will_discard() && ui.is_visible() && !ui.is_sizing_pass();
                ui.add_enabled_ui(authorized_pass, |ui| {
                    let scroll_output = egui::ScrollArea::vertical()
                        .id_salt("convert-panel-content")
                        .max_height(content_rect.height())
                        .show(ui, |ui| {
                            self.contents_ui(
                                ui,
                                authorized_pass,
                                &mut input,
                                &mut from_filter,
                                &mut to_filter,
                                &mut category,
                                &mut from,
                                &mut targets,
                                &mut preferences,
                                &mut input_id,
                                &mut add_target_id,
                                &mut focus_requested,
                                &mut focus_add_target,
                                &mut favorite_limit_hit,
                                &mut effect,
                            );
                        });
                    #[cfg(test)]
                    {
                        self.test_content_viewport = Some(scroll_output.inner_rect);
                    }
                    #[cfg(not(test))]
                    let _ = scroll_output;
                });
            });

        #[cfg(test)]
        {
            self.test_window_rect = output.as_ref().map(|output| output.response.rect);
        }
        let content_authorized = authorized_pass && !ctx.will_discard();
        let window_lifecycle_authorized = first_ui_pass
            && !ctx.will_discard()
            && output
                .as_ref()
                .is_some_and(|output| output.response.rect.intersects(ctx.content_rect()));
        let state_changed = input != self.input
            || from_filter != self.from_filter
            || to_filter != self.to_filter
            || category != self.category
            || from != self.from
            || targets != self.targets
            || preferences != self.preferences.current;
        let close_requested = (window_lifecycle_authorized && self.host_close_requested)
            || (window_lifecycle_authorized && !window_open);
        if (content_authorized
            && (state_changed
                || focus_requested
                || focus_add_target
                || favorite_limit_hit
                || effect.is_some()))
            || close_requested
        {
            self.pending_root_effects = Some(RootEffects {
                frame_nr,
                pass_nr,
                input,
                from_filter,
                to_filter,
                category,
                from,
                targets,
                preferences,
                focus_requested,
                input_id,
                focus_add_target,
                add_target_id,
                favorite_limit_hit,
                window_open: window_open && !close_requested,
                effect,
            });
        }

        None
    }

    /// Commit staged controls and actions only after the ROOT frame has been
    /// retained. Filesystem and clipboard effects are dispatched by the owner.
    pub(crate) fn on_ui_frame_complete(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<ConvertPanelEffect> {
        let Some(effects) = self.pending_root_effects.take() else {
            return None;
        };
        let retained = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT)
            == effects.frame_nr.saturating_add(1)
            && ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT)
                == effects.pass_nr.saturating_add(1);
        if !retained {
            return None;
        }

        let needs_repaint = effects.input != self.input
            || effects.from_filter != self.from_filter
            || effects.to_filter != self.to_filter
            || effects.category != self.category
            || effects.from != self.from
            || effects.targets != self.targets
            || effects.preferences != self.preferences.current
            || effects.focus_requested
            || effects.focus_add_target
            || effects.effect.is_some()
            || effects.favorite_limit_hit
            || !effects.window_open;
        self.input = effects.input;
        self.from_filter = effects.from_filter;
        self.to_filter = effects.to_filter;
        self.category = effects.category;
        self.from = effects.from;
        self.targets = effects.targets;
        self.preferences.current = effects.preferences;
        if effects.favorite_limit_hit {
            self.preferences.feedback = Some(format!(
                "Converter favorites are limited to {} units",
                preferences::MAX_FAVORITES
            ));
        }
        if effects.focus_requested {
            if let Some(input_id) = effects.input_id {
                ctx.memory_mut(|memory| memory.request_focus(input_id));
                ctx.request_repaint();
            }
            self.focus_input = false;
        }
        if effects.focus_add_target
            && let Some(add_target_id) = effects.add_target_id
        {
            ctx.memory_mut(|memory| memory.request_focus(add_target_id));
            ctx.request_repaint();
        }
        self.open = effects.window_open;
        if !effects.window_open {
            self.host_close_requested = false;
        }
        if needs_repaint {
            ctx.request_repaint();
        }
        effects.effect
    }

    fn contents_ui(
        &mut self,
        ui: &mut egui::Ui,
        authorized_pass: bool,
        input: &mut String,
        from_filter: &mut String,
        to_filter: &mut String,
        category: &mut PanelCategory,
        from: &mut Option<PanelUnit>,
        targets: &mut Vec<Option<PanelUnit>>,
        preferences: &mut DisplayPreferences,
        input_id: &mut Option<egui::Id>,
        add_target_id: &mut Option<egui::Id>,
        focus_requested: &mut bool,
        focus_add_target: &mut bool,
        favorite_limit_hit: &mut bool,
        effect: &mut Option<ConvertPanelEffect>,
    ) {
        #[cfg(test)]
        {
            self.test_control_rects.clear();
            self.test_target_row_rects.clear();
            self.test_copy_rects.clear();
            self.test_copy_ids.clear();
            self.test_swap_rects.clear();
            self.test_remove_rects.clear();
        }

        let wide_layout = ui.available_width() >= WIDE_LAYOUT_MIN_WIDTH;
        let previous_category = *category;
        responsive_row(ui, wide_layout, |ui| {
            ui.label("Category");
            let category_response = category_selector(ui, category, wide_layout);
            if category_response.gained_focus() {
                category_response.scroll_to_me(Some(egui::Align::Center));
            }
            #[cfg(test)]
            self.test_control_rects
                .push(("category".to_owned(), category_response.rect));

            ui.separator();
            ui.label("Number format");
            let format_response = number_format_selector(ui, preferences, wide_layout);
            if format_response.gained_focus() {
                format_response.scroll_to_me(Some(egui::Align::Center));
            }
            #[cfg(test)]
            self.test_control_rects
                .push(("number format".to_owned(), format_response.rect));

            let digits_response = if wide_layout {
                ui.add_enabled(
                    preferences.number_format != NumberFormat::Smart,
                    egui::DragValue::new(&mut preferences.decimal_places)
                        .range(0..=preferences::MAX_DECIMAL_PLACES)
                        .prefix("digits "),
                )
            } else {
                ui.horizontal(|ui| {
                    ui.label("Digits");
                    ui.add_enabled(
                        preferences.number_format != NumberFormat::Smart,
                        egui::DragValue::new(&mut preferences.decimal_places)
                            .range(0..=preferences::MAX_DECIMAL_PLACES),
                    )
                })
                .inner
            };
            if digits_response.gained_focus() {
                digits_response.scroll_to_me(Some(egui::Align::Center));
            }
            #[cfg(test)]
            self.test_control_rects
                .push(("digits".to_owned(), digits_response.rect));
        });
        if *category != previous_category {
            *from = None;
            *targets = vec![None];
        }

        ui.separator();
        responsive_row(ui, wide_layout, |ui| {
            ui.label("Value");
            let id = ui.make_persistent_id("convert-value-input");
            *input_id = Some(id);
            #[cfg(test)]
            {
                self.test_input_id = Some(id);
            }
            let input_response = ui.add(
                egui::TextEdit::singleline(input)
                    .id(id)
                    .desired_width(ui.available_width())
                    .hint_text("Enter a value or supported expression"),
            );
            if input_response.gained_focus() {
                input_response.scroll_to_me(Some(egui::Align::Center));
            }
            #[cfg(test)]
            {
                self.test_input_rect = Some(input_response.rect);
                self.test_control_rects
                    .push(("value".to_owned(), input_response.rect));
            }
            if authorized_pass && self.focus_input {
                *focus_requested = true;
            }
        });

        let from_options = filtered_units(*category, from_filter);
        let to_options = filtered_units(*category, to_filter);
        reconcile_selection_values(*category, &from_options, &to_options, from, targets);

        ui.group(|ui| {
            ui.label("From");
            responsive_row(ui, wide_layout, |ui| {
                ui.label("Search");
                let from_search = ui.add(
                    egui::TextEdit::singleline(from_filter)
                        .desired_width(responsive_control_width(ui, wide_layout, 120.0))
                        .hint_text("unit or alias"),
                );
                if from_search.gained_focus() {
                    from_search.scroll_to_me(Some(egui::Align::Center));
                }
                #[cfg(test)]
                self.test_control_rects
                    .push(("from search".to_owned(), from_search.rect));

                let from_combo = egui::ComboBox::from_id_salt("convert_from_unit")
                    .width(responsive_control_width(ui, wide_layout, 155.0))
                    .selected_text(from.map_or("Select unit", PanelUnit::label))
                    .show_ui(ui, |ui| {
                        for option in &from_options {
                            ui.selectable_value(from, Some(*option), option.label());
                        }
                    })
                    .response;
                if from_combo.gained_focus() {
                    from_combo.scroll_to_me(Some(egui::Align::Center));
                }
                #[cfg(test)]
                self.test_control_rects
                    .push(("from unit".to_owned(), from_combo.rect));

                let favorites = filtered_favorites(preferences, *category);
                let favorites_width = responsive_control_width(ui, wide_layout, 120.0);
                let favorites_response = favorite_selector(
                    ui,
                    "convert_from_favorites",
                    from,
                    &favorites,
                    favorites_width,
                );
                if favorites_response.gained_focus() {
                    favorites_response.scroll_to_me(Some(egui::Align::Center));
                }
                #[cfg(test)]
                self.test_control_rects
                    .push(("from favorites".to_owned(), favorites_response.rect));

                if let Some(selected) = *from {
                    let favorite = preferences.favorites.contains(&selected);
                    let toggle_response = ui
                        .scope_builder(
                            stable_action_scope(converter_root_id().with("from-favorite-toggle")),
                            |ui| ui.small_button(if favorite { "★" } else { "☆" }),
                        )
                        .inner;
                    if toggle_response.gained_focus() {
                        toggle_response.scroll_to_me(Some(egui::Align::Center));
                    }
                    if toggle_response.clicked() && !toggle_favorite(preferences, selected) {
                        *favorite_limit_hit = true;
                    }
                    #[cfg(test)]
                    self.test_control_rects
                        .push(("from favorite toggle".to_owned(), toggle_response.rect));
                }
            });
        });

        ui.group(|ui| {
            responsive_row(ui, wide_layout, |ui| {
                ui.label("To");
                ui.weak(format!("{} of {MAX_TARGETS} targets", targets.len()));
                ui.label("Search");
                let to_search = ui.add(
                    egui::TextEdit::singleline(to_filter)
                        .desired_width(responsive_control_width(ui, wide_layout, 150.0))
                        .hint_text("unit or alias"),
                );
                if to_search.gained_focus() {
                    to_search.scroll_to_me(Some(egui::Align::Center));
                }
                #[cfg(test)]
                self.test_control_rects
                    .push(("to search".to_owned(), to_search.rect));
            });

            let mut remove_target = None;
            for index in 0..targets.len() {
                let mut target = targets[index];
                let row = ui.scope_builder(
                    stable_column_scope(converter_root_id().with(("target-row", index))),
                    |ui| {
                        ui.group(|ui| {
                            responsive_row(ui, wide_layout, |ui| {
                                ui.label(format!("Target {}", index + 1));
                                let target_combo =
                                    egui::ComboBox::from_id_salt(("convert_to_unit", index))
                                        .width(responsive_control_width(ui, wide_layout, 180.0))
                                        .selected_text(
                                            target.map_or("Select unit", PanelUnit::label),
                                        )
                                        .show_ui(ui, |ui| {
                                            for option in &to_options {
                                                ui.selectable_value(
                                                    &mut target,
                                                    Some(*option),
                                                    option.label(),
                                                );
                                            }
                                        })
                                        .response;
                                if target_combo.gained_focus() {
                                    target_combo.scroll_to_me(Some(egui::Align::Center));
                                }
                                #[cfg(test)]
                                self.test_control_rects
                                    .push((format!("target {index} unit"), target_combo.rect));

                                let favorites = filtered_favorites(preferences, *category);
                                let favorites_width =
                                    responsive_control_width(ui, wide_layout, 120.0);
                                let favorites_response = favorite_selector(
                                    ui,
                                    ("convert_to_favorites", index),
                                    &mut target,
                                    &favorites,
                                    favorites_width,
                                );
                                if favorites_response.gained_focus() {
                                    favorites_response.scroll_to_me(Some(egui::Align::Center));
                                }
                                #[cfg(test)]
                                self.test_control_rects.push((
                                    format!("target {index} favorites"),
                                    favorites_response.rect,
                                ));

                                if let Some(selected) = target {
                                    let favorite = preferences.favorites.contains(&selected);
                                    let toggle_response = ui
                                        .scope_builder(
                                            stable_action_scope(
                                                converter_root_id()
                                                    .with(("target-favorite-toggle", index)),
                                            ),
                                            |ui| {
                                                ui.small_button(if favorite {
                                                    "★"
                                                } else {
                                                    "☆"
                                                })
                                            },
                                        )
                                        .inner;
                                    if toggle_response.gained_focus() {
                                        toggle_response.scroll_to_me(Some(egui::Align::Center));
                                    }
                                    if toggle_response.clicked()
                                        && !toggle_favorite(preferences, selected)
                                    {
                                        *favorite_limit_hit = true;
                                    }
                                    #[cfg(test)]
                                    self.test_control_rects.push((
                                        format!("target {index} favorite toggle"),
                                        toggle_response.rect,
                                    ));
                                }

                                let mut show_row_actions = |ui: &mut egui::Ui| {
                                    let swap = ui
                                        .scope_builder(
                                            stable_action_scope(
                                                converter_root_id().with(("target-swap", index)),
                                            ),
                                            |ui| ui.button("Swap"),
                                        )
                                        .inner;
                                    if swap.gained_focus() {
                                        swap.scroll_to_me(Some(egui::Align::Center));
                                    }
                                    #[cfg(test)]
                                    {
                                        self.test_swap_rects.push((index, swap.rect));
                                        self.test_control_rects
                                            .push((format!("target {index} swap"), swap.rect));
                                    }
                                    if swap.clicked() {
                                        std::mem::swap(from, &mut target);
                                    }

                                    if targets.len() > 1 {
                                        let remove = ui
                                            .scope_builder(
                                                stable_action_scope(
                                                    converter_root_id()
                                                        .with(("target-remove", index)),
                                                ),
                                                |ui| ui.small_button("Remove"),
                                            )
                                            .inner;
                                        if remove.gained_focus() {
                                            remove.scroll_to_me(Some(egui::Align::Center));
                                        }
                                        #[cfg(test)]
                                        {
                                            self.test_remove_rects.push((index, remove.rect));
                                            self.test_control_rects.push((
                                                format!("target {index} remove"),
                                                remove.rect,
                                            ));
                                        }
                                        if remove.clicked() {
                                            remove_target = Some(index);
                                        }
                                    }
                                };
                                if wide_layout {
                                    show_row_actions(ui);
                                } else {
                                    ui.horizontal(show_row_actions);
                                }
                            });
                        });
                    },
                );
                #[cfg(test)]
                self.test_target_row_rects.push((index, row.response.rect));
                #[cfg(not(test))]
                let _ = row;
                targets[index] = target;
            }
            if let Some(index) = remove_target {
                targets.remove(index);
                *focus_add_target = true;
            }
            let add_target = ui
                .scope_builder(
                    stable_action_scope(converter_root_id().with("add-target")),
                    |ui| {
                        ui.add_enabled(targets.len() < MAX_TARGETS, egui::Button::new("Add target"))
                    },
                )
                .inner;
            if add_target.gained_focus() {
                add_target.scroll_to_me(Some(egui::Align::Center));
            }
            *add_target_id = Some(add_target.id);
            #[cfg(test)]
            {
                self.test_add_target_rect = Some(add_target.rect);
                self.test_add_target_id = Some(add_target.id);
                self.test_control_rects
                    .push(("add target".to_owned(), add_target.rect));
            }
            if add_target.clicked() && targets.len() < MAX_TARGETS {
                targets.push(next_target(*category, targets));
            }
        });

        if *favorite_limit_hit {
            ui.colored_label(ui.visuals().warn_fg_color, "Favorite limit reached.");
        }

        let results = calculate_targets(input, *from, targets, preferences);
        ui.separator();
        ui.heading("Results");
        if results.is_empty() {
            ui.label("Enter a value to see converted results.");
            #[cfg(test)]
            {
                self.test_results_viewport = None;
            }
        } else {
            let results_height = ui.available_height().min(220.0).max(96.0);
            let results_scroll = egui::ScrollArea::vertical()
                .id_salt("convert-panel-results")
                .max_height(results_height)
                .show(ui, |ui| {
                    let mut outer_scroll_target = None;
                    for (index, result) in results.iter().enumerate() {
                        ui.scope_builder(
                            stable_column_scope(converter_root_id().with(("target-result", index))),
                            |ui| {
                                ui.group(|ui| {
                                    responsive_row(ui, wide_layout, |ui| {
                                        ui.strong(result.target.label());
                                        match &result.value {
                                            Ok(value) => {
                                                ui.label(value.display());
                                                if value.is_approximate() {
                                                    ui.weak("approximate duration");
                                                }
                                                let copy = ui
                                                    .scope_builder(
                                                        stable_action_scope(
                                                            converter_root_id()
                                                                .with(("target-copy", index)),
                                                        ),
                                                        |ui| ui.small_button("Copy"),
                                                    )
                                                    .inner;
                                                if copy.gained_focus() {
                                                    copy.scroll_to_me(Some(egui::Align::Center));
                                                    outer_scroll_target = Some(copy.rect);
                                                }
                                                #[cfg(test)]
                                                {
                                                    self.test_copy_rects.push((index, copy.rect));
                                                    self.test_copy_ids.push((index, copy.id));
                                                    self.test_control_rects.push((
                                                        format!("result {index} copy"),
                                                        copy.rect,
                                                    ));
                                                }
                                                if copy.clicked() {
                                                    *effect = Some(ConvertPanelEffect::CopyText(
                                                        value.display().to_owned(),
                                                    ));
                                                }
                                            }
                                            Err(error) => {
                                                let error_response = ui.colored_label(
                                                    ui.visuals().error_fg_color,
                                                    error.to_string(),
                                                );
                                                #[cfg(test)]
                                                self.test_control_rects.push((
                                                    format!("result {index} error"),
                                                    error_response.rect,
                                                ));
                                                #[cfg(not(test))]
                                                let _ = error_response;
                                            }
                                        }
                                    });
                                });
                            },
                        );
                    }
                    outer_scroll_target
                });
            if let Some(rect) = results_scroll.inner {
                // This runs after the nested results area closes, so the parent
                // content area also moves the newly focused Copy action into view.
                ui.scroll_to_rect(rect, Some(egui::Align::Center));
            }
            #[cfg(test)]
            {
                self.test_results_viewport = Some(results_scroll.inner_rect);
            }
        }

        ui.separator();
        let save = ui.scope_builder(
            stable_column_scope(converter_root_id().with("save-preferences")),
            |ui| {
                let response = ui.button("Save Preferences");
                if response.gained_focus() {
                    response.scroll_to_me(Some(egui::Align::Center));
                }
                if response.clicked() {
                    *effect = Some(ConvertPanelEffect::SavePreferences);
                }
                if self.preferences.has_unsaved_changes() {
                    ui.colored_label(ui.visuals().warn_fg_color, "Unsaved preferences");
                }
                response
            },
        );
        #[cfg(test)]
        {
            self.test_save_preferences_rect = Some(save.inner.rect);
            self.test_save_preferences_id = Some(save.inner.id);
            self.test_control_rects
                .push(("save preferences".to_owned(), save.inner.rect));
        }
        #[cfg(not(test))]
        let _ = save;
        if let Some(error) = self.preferences.load_error.as_deref() {
            ui.colored_label(ui.visuals().error_fg_color, error);
            ui.label("Fix the file and restart the app to load it; saving is blocked until then.");
        }
        if let Some(feedback) = self.preferences.feedback.as_deref() {
            ui.label(feedback);
        }
        if let Some(Ok(())) = self.copy_feedback.as_ref() {
            ui.label("Copied to the clipboard.");
        }
        if let Some(Err(error)) = self.copy_feedback.as_ref() {
            ui.colored_label(ui.visuals().error_fg_color, format!("Copy failed: {error}"));
        }
    }

    fn select_category(&mut self, category: PanelCategory) {
        self.category = category;
        self.from = None;
        self.targets = vec![None];
        self.reconcile_selections();
    }

    fn filtered_units(&self) -> Vec<PanelUnit> {
        filtered_units(self.category, &self.from_filter)
    }

    fn reconcile_selections(&mut self) {
        let from_options = filtered_units(self.category, &self.from_filter);
        let to_options = filtered_units(self.category, &self.to_filter);
        reconcile_selection_values(
            self.category,
            &from_options,
            &to_options,
            &mut self.from,
            &mut self.targets,
        );
    }

    fn compute_results(&self) -> Vec<TargetResult> {
        calculate_targets(
            &self.input,
            self.from,
            &self.targets,
            &self.preferences.current,
        )
    }

    pub(crate) fn save_preferences(&mut self) {
        self.preferences.save_explicitly();
    }

    pub(crate) fn copy_completed(&mut self, result: Result<(), String>) {
        self.copy_feedback = Some(result);
    }

    #[cfg(test)]
    pub(crate) fn test_set_fixed_preferences(&mut self) {
        self.preferences.current.number_format = NumberFormat::Fixed;
        self.preferences.current.decimal_places = 4;
    }

    #[cfg(test)]
    pub(crate) fn test_save_preferences_button_rect(&self) -> Option<egui::Rect> {
        self.test_save_preferences_rect
    }

    #[cfg(test)]
    pub(crate) fn test_preferences_are_dirty(&self) -> bool {
        self.preferences.has_unsaved_changes()
    }

    #[cfg(test)]
    fn add_target(&mut self) {
        if self.targets.len() < MAX_TARGETS {
            self.targets.push(next_target(self.category, &self.targets));
        }
    }
}

fn stable_action_scope(id: egui::Id) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .id(id)
        .layout(egui::Layout::left_to_right(egui::Align::Min))
}

fn stable_column_scope(id: egui::Id) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .id(id)
        .layout(egui::Layout::top_down(egui::Align::Min))
}

fn responsive_row(ui: &mut egui::Ui, wide: bool, contents: impl FnOnce(&mut egui::Ui)) {
    if wide {
        ui.horizontal(contents);
    } else {
        ui.vertical(contents);
    }
}

fn responsive_control_width(ui: &egui::Ui, wide: bool, preferred: f32) -> f32 {
    if wide {
        ui.available_width().min(preferred)
    } else {
        ui.available_width()
    }
}

fn category_selector(
    ui: &mut egui::Ui,
    category: &mut PanelCategory,
    wide: bool,
) -> egui::Response {
    egui::ComboBox::from_id_salt("convert_category")
        .width(responsive_control_width(ui, wide, 180.0))
        .selected_text(category.label())
        .show_ui(ui, |ui| {
            for option in category_options() {
                ui.selectable_value(category, option, option.label());
            }
        })
        .response
}

fn number_format_selector(
    ui: &mut egui::Ui,
    preferences: &mut DisplayPreferences,
    wide: bool,
) -> egui::Response {
    egui::ComboBox::from_id_salt("convert_number_format")
        .width(responsive_control_width(ui, wide, 145.0))
        .selected_text(preferences.number_format.label())
        .show_ui(ui, |ui| {
            for format in NumberFormat::ALL {
                ui.selectable_value(&mut preferences.number_format, *format, format.label());
            }
        })
        .response
}

fn favorite_selector(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    selected: &mut Option<PanelUnit>,
    favorites: &[PanelUnit],
    width: f32,
) -> egui::Response {
    ui.add_enabled_ui(!favorites.is_empty(), |ui| {
        egui::ComboBox::from_id_salt(id)
            .width(width)
            .selected_text("Favorites")
            .show_ui(ui, |ui| {
                for favorite in favorites {
                    ui.selectable_value(selected, Some(*favorite), favorite.label());
                }
            })
            .response
    })
    .inner
}

fn toggle_favorite(preferences: &mut DisplayPreferences, unit: PanelUnit) -> bool {
    if let Some(index) = preferences
        .favorites
        .iter()
        .position(|favorite| *favorite == unit)
    {
        preferences.favorites.remove(index);
        true
    } else if preferences.favorites.len() >= preferences::MAX_FAVORITES {
        false
    } else {
        preferences.favorites.push(unit);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BaseUnit, ConvertPanel, ConvertPanelEffect, MAX_TARGETS, NumberFormat, PanelCategory,
        PanelUnit, TargetValue, category_options, category_units,
    };
    use crate::platform::app_data::AppDataRoot;
    use crate::unit_conversion::{self, Approximation, Category, Unit};
    use eframe::egui;

    fn input(screen_size: egui::Vec2, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size)),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn frame(
        panel: &mut ConvertPanel,
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Option<ConvertPanelEffect> {
        let output = ctx.run_ui(input(size, events), |ui| {
            let _ = panel.ui(ui.ctx(), ui.ctx().current_pass_index() == 0);
        });
        let effect = panel.on_ui_frame_complete(ctx);
        output.drop_without_applying_deltas();
        effect
    }

    fn frame_requesting_discard(
        panel: &mut ConvertPanel,
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> (Option<ConvertPanelEffect>, bool) {
        let mut first_ui_pass_seen = false;
        let output = ctx.run_ui(input(size, events), |ui| {
            let first_ui_pass = ui.ctx().current_pass_index() == 0;
            if first_ui_pass {
                first_ui_pass_seen = true;
                ui.ctx()
                    .request_discard("Converter discarded-pass regression");
            }
            let _ = panel.ui(ui.ctx(), first_ui_pass);
        });
        let effect = panel.on_ui_frame_complete(ctx);
        output.drop_without_applying_deltas();
        (effect, first_ui_pass_seen)
    }

    fn frame_requesting_late_discard(
        panel: &mut ConvertPanel,
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Option<ConvertPanelEffect> {
        let output = ctx.run_ui(input(size, events), |ui| {
            let first_ui_pass = ui.ctx().current_pass_index() == 0;
            let _ = panel.ui(ui.ctx(), first_ui_pass);
            if first_ui_pass {
                ui.ctx()
                    .request_discard("Converter late ROOT discard regression");
            }
        });
        let effect = panel.on_ui_frame_complete(ctx);
        output.drop_without_applying_deltas();
        effect
    }

    fn key(key: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn click_events(point: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn click(
        panel: &mut ConvertPanel,
        ctx: &egui::Context,
        size: egui::Vec2,
        point: egui::Pos2,
    ) -> Option<ConvertPanelEffect> {
        frame(panel, ctx, size, click_events(point, true));
        frame(panel, ctx, size, click_events(point, false))
    }

    fn tab_until_focused(
        panel: &mut ConvertPanel,
        ctx: &egui::Context,
        size: egui::Vec2,
        target: egui::Id,
    ) -> bool {
        for _ in 0..100 {
            frame(panel, ctx, size, vec![key(egui::Key::Tab, true)]);
            frame(panel, ctx, size, vec![key(egui::Key::Tab, false)]);
            if ctx.memory(|memory| memory.has_focus(target)) {
                return true;
            }
        }
        false
    }

    fn physical_result(
        panel: &ConvertPanel,
        index: usize,
    ) -> (unit_conversion::ConversionOutcome, String) {
        let results = panel.compute_results();
        let row = results.get(index).expect("target result exists");
        match row.value.as_ref().expect("conversion succeeds") {
            TargetValue::Physical { outcome, display } => (outcome.clone(), display.clone()),
            TargetValue::Base { .. } => panic!("expected a physical conversion"),
        }
    }

    #[test]
    fn exposes_every_shared_physical_category_and_unit() {
        let categories: Vec<_> = category_options().collect();
        assert_eq!(categories.len(), Category::ALL.len() + 1);
        assert!(categories.contains(&PanelCategory::Base));

        for category in Category::ALL {
            let panel_category = PanelCategory::Physical(*category);
            let choices = category_units(panel_category);
            let expected: Vec<_> = unit_conversion::units_in_category(*category)
                .map(|definition| PanelUnit::Physical(definition.unit))
                .collect();
            assert_eq!(choices, expected, "category {}", category.display_name());
            assert!(!choices.is_empty(), "category {}", category.display_name());
        }
    }

    #[test]
    fn category_changes_reset_choices_from_the_new_filtered_category() {
        let mut panel = ConvertPanel {
            from: Some(PanelUnit::Physical(Unit::Kilometer)),
            targets: vec![Some(PanelUnit::Physical(Unit::Mile))],
            from_filter: "oz".to_owned(),
            to_filter: "oz".to_owned(),
            ..ConvertPanel::default()
        };

        panel.select_category(PanelCategory::Physical(Category::Mass));
        let filtered = panel.filtered_units();
        assert_eq!(filtered, vec![PanelUnit::Physical(Unit::Ounce)]);
        assert_eq!(panel.from, Some(PanelUnit::Physical(Unit::Ounce)));
        assert_eq!(panel.targets, vec![Some(PanelUnit::Physical(Unit::Ounce))]);
    }

    #[test]
    fn filtering_uses_case_insensitive_symbols_and_catalog_aliases_without_collapsing_units() {
        let mut panel = ConvertPanel {
            category: PanelCategory::Physical(Category::Data),
            from_filter: "mB".to_owned(),
            ..ConvertPanel::default()
        };
        let matching = panel.filtered_units();
        assert!(matching.contains(&PanelUnit::Physical(Unit::Megabyte)));
        assert!(matching.contains(&PanelUnit::Physical(Unit::Megabit)));
        assert_ne!(Unit::Megabyte, Unit::Megabit);

        panel.from_filter = "MEGABIT".to_owned();
        assert_eq!(
            panel.filtered_units(),
            vec![PanelUnit::Physical(Unit::Megabit)]
        );

        panel.category = PanelCategory::Physical(Category::Volume);
        panel.from_filter = "imperial gallon".to_owned();
        assert_eq!(
            panel.filtered_units(),
            vec![PanelUnit::Physical(Unit::ImperialGallon)]
        );
    }

    #[test]
    fn filtering_does_not_discard_valid_category_selections() {
        let mut panel = ConvertPanel {
            from: Some(PanelUnit::Physical(Unit::Kilometer)),
            targets: vec![Some(PanelUnit::Physical(Unit::Mile))],
            from_filter: "mi".to_owned(),
            to_filter: "mi".to_owned(),
            ..ConvertPanel::default()
        };

        panel.reconcile_selections();
        assert_eq!(panel.from, Some(PanelUnit::Physical(Unit::Kilometer)));
        assert_eq!(panel.targets, vec![Some(PanelUnit::Physical(Unit::Mile))]);

        panel.from_filter = "no such unit".to_owned();
        panel.to_filter = "no such unit".to_owned();
        panel.reconcile_selections();
        assert_eq!(panel.from, Some(PanelUnit::Physical(Unit::Kilometer)));
        assert_eq!(panel.targets, vec![Some(PanelUnit::Physical(Unit::Mile))]);
    }

    #[test]
    fn physical_panel_calculation_matches_inline_domain_and_smart_formatting() {
        let mut panel = ConvertPanel {
            input: "1/2".to_owned(),
            category: PanelCategory::Physical(Category::Length),
            from: Some(PanelUnit::Physical(Unit::Kilometer)),
            targets: vec![Some(PanelUnit::Physical(Unit::Mile))],
            ..ConvertPanel::default()
        };

        let (outcome, display) = physical_result(&panel, 0);
        let inline = unit_conversion::evaluate_conversion("1/2 km to mi").unwrap();
        let expected = crate::common::number_format::format_number(inline.value).unwrap();
        assert_eq!(outcome.value, inline.value);
        assert_eq!(display, format!("{expected} mi"));
        panel.input.clear();
        assert!(panel.compute_results().is_empty());
    }

    #[test]
    fn physical_panel_canonical_symbols_resolve_through_the_domain_catalog() {
        let cases = [
            ("1", Category::Mass, Unit::UsShortTon, Unit::Pound, "2000"),
            (
                "1",
                Category::Volume,
                Unit::UsFluidOunce,
                Unit::Milliliter,
                "29.5735",
            ),
            (
                "10",
                Category::FuelEconomy,
                Unit::LiterPer100Kilometers,
                Unit::KilometerPerLiter,
                "10",
            ),
        ];

        for (input, category, from, to, expected) in cases {
            let panel = ConvertPanel {
                input: input.to_owned(),
                category: PanelCategory::Physical(category),
                from: Some(PanelUnit::Physical(from)),
                targets: vec![Some(PanelUnit::Physical(to))],
                ..ConvertPanel::default()
            };
            let (outcome, display) = physical_result(&panel, 0);
            assert_eq!(display, format!("{expected} {}", to.symbol()));
            assert_eq!(
                outcome.request.destination,
                unit_conversion::unit_by_alias(to.symbol()).unwrap()
            );
        }
    }

    #[test]
    fn base_category_keeps_its_existing_radix_conversion_behavior() {
        let mut panel = ConvertPanel {
            input: "ff".to_owned(),
            category: PanelCategory::Base,
            from: Some(PanelUnit::Base(BaseUnit::Hexadecimal)),
            targets: vec![Some(PanelUnit::Base(BaseUnit::Decimal))],
            ..ConvertPanel::default()
        };
        let result = panel.compute_results();
        assert!(matches!(
            &result[0].value,
            Ok(TargetValue::Base { display }) if display == "255 dec"
        ));

        panel.input = "-10".to_owned();
        panel.from = Some(PanelUnit::Base(BaseUnit::Decimal));
        panel.targets = vec![Some(PanelUnit::Base(BaseUnit::Binary))];
        let result = panel.compute_results();
        let expected = format!("{} bin", format!("{:b}", -10_i64));
        assert!(matches!(
            &result[0].value,
            Ok(TargetValue::Base { display }) if display.as_str() == expected.as_str()
        ));

        panel.input = (-i64::MAX).to_string();
        panel.targets = vec![Some(PanelUnit::Base(BaseUnit::Hexadecimal))];
        let result = panel.compute_results();
        let expected = format!("{} hex", format!("{:x}", -i64::MAX));
        assert!(matches!(
            &result[0].value,
            Ok(TargetValue::Base { display }) if display.as_str() == expected.as_str()
        ));

        panel.input = i64::MAX.to_string();
        let result = panel.compute_results();
        let expected = format!("{} hex", format!("{:x}", i64::MAX));
        assert!(matches!(
            &result[0].value,
            Ok(TargetValue::Base { display }) if display.as_str() == expected.as_str()
        ));

        panel.input = i64::MIN.to_string();
        let result = panel.compute_results();
        assert!(matches!(
            &result[0].value,
            Err(super::ConversionFailure::BaseMagnitudeOutOfRange)
        ));

        panel.input = (i64::MAX as u128 + 1).to_string();
        let result = panel.compute_results();
        assert!(matches!(
            &result[0].value,
            Err(super::ConversionFailure::BaseMagnitudeOutOfRange)
        ));

        panel.input = "--9223372036854775808".into();
        let result = panel.compute_results();
        assert!(matches!(
            &result[0].value,
            Err(super::ConversionFailure::BaseMagnitudeOutOfRange)
        ));

        panel.input = "-+1".into();
        panel.targets = vec![Some(PanelUnit::Base(BaseUnit::Decimal))];
        let result = panel.compute_results();
        assert!(matches!(
            &result[0].value,
            Ok(TargetValue::Base { display }) if display.as_str() == "-1 dec"
        ));

        panel.input = "--1".into();
        let result = panel.compute_results();
        assert!(matches!(
            &result[0].value,
            Ok(TargetValue::Base { display }) if display.as_str() == "1 dec"
        ));

        panel.input = "++1".into();
        let result = panel.compute_results();
        assert!(matches!(
            &result[0].value,
            Err(super::ConversionFailure::InvalidBaseDigits { radix: 10 })
        ));
    }

    #[test]
    fn swapping_and_multiple_targets_keep_the_input_and_respect_the_target_cap() {
        let mut panel = ConvertPanel {
            input: "1/2".into(),
            from: Some(PanelUnit::Physical(Unit::Kilometer)),
            targets: vec![
                Some(PanelUnit::Physical(Unit::Mile)),
                Some(PanelUnit::Physical(Unit::Foot)),
            ],
            ..ConvertPanel::default()
        };
        panel.open();
        let ctx = egui::Context::default();
        let size = egui::vec2(760.0, 590.0);
        for _ in 0..3 {
            frame(&mut panel, &ctx, size, Vec::new());
        }
        let input = panel.input.clone();
        let old_from = panel.from.unwrap();
        let old_target = panel.targets[0].unwrap();
        let swap_rect = panel
            .test_swap_rects
            .iter()
            .rev()
            .find(|(index, _)| *index == 0)
            .map(|(_, rect)| *rect)
            .expect("each target row exposes a Swap control");
        click(&mut panel, &ctx, size, swap_rect.center());
        assert_eq!(panel.input, input);
        assert_eq!(panel.from, Some(old_target));
        assert_eq!(panel.targets[0], Some(old_from));

        let add_target_rect = panel
            .test_add_target_rect
            .expect("the target cap has an Add target control");
        click(&mut panel, &ctx, size, add_target_rect.center());
        assert_eq!(panel.targets.len(), 3);
        assert_eq!(panel.input, input);

        let results = panel.compute_results();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].target, PanelUnit::Physical(Unit::Kilometer));
        assert_eq!(results[1].target, PanelUnit::Physical(Unit::Foot));
        let expected = unit_conversion::evaluate_conversion("1/2 mi to ft").unwrap();
        assert!(matches!(
            &results[1].value,
            Ok(TargetValue::Physical { outcome, .. }) if outcome.value == expected.value
        ));

        for _ in panel.targets.len()..MAX_TARGETS {
            panel.add_target();
        }
        assert_eq!(panel.targets.len(), MAX_TARGETS);
        panel.add_target();
        assert_eq!(panel.targets.len(), MAX_TARGETS);
    }

    #[test]
    fn errors_replace_success_and_physical_parity_preserves_approximation_metadata() {
        let mut panel = ConvertPanel {
            input: "1".into(),
            category: PanelCategory::Physical(Category::Duration),
            from: Some(PanelUnit::Physical(Unit::Month)),
            targets: vec![Some(PanelUnit::Physical(Unit::Day))],
            ..ConvertPanel::default()
        };
        let (outcome, display) = physical_result(&panel, 0);
        assert_eq!(outcome.value, 30.0);
        assert_eq!(outcome.approximations, vec![Approximation::ThirtyDayMonth]);
        assert!(display.contains("approx."));

        panel.input = "not a number".into();
        let failed = panel.compute_results();
        assert!(matches!(
            &failed[0].value,
            Err(super::ConversionFailure::Physical(_))
        ));
        assert!(
            failed[0].value.is_err(),
            "the previous success is not retained"
        );

        for (input, category, from, to) in [
            ("32", Category::Temperature, Unit::Fahrenheit, Unit::Celsius),
            ("1 1/2", Category::Volume, Unit::Cup, Unit::Milliliter),
            ("1", Category::Data, Unit::Megabyte, Unit::Bit),
            ("1", Category::Data, Unit::Megabit, Unit::Bit),
        ] {
            let panel = ConvertPanel {
                input: input.to_owned(),
                category: PanelCategory::Physical(category),
                from: Some(PanelUnit::Physical(from)),
                targets: vec![Some(PanelUnit::Physical(to))],
                ..ConvertPanel::default()
            };
            let (outcome, _) = physical_result(&panel, 0);
            let source = format!("{input} {} to {}", from.symbol(), to.symbol());
            let expected = unit_conversion::evaluate_conversion(&source).unwrap();
            assert_eq!(outcome.value, expected.value, "{source}");
            assert_eq!(outcome.approximations, expected.approximations, "{source}");
        }
    }

    #[test]
    fn fixed_and_scientific_precision_change_display_without_changing_value() {
        let mut panel = ConvertPanel {
            input: "1".into(),
            from: Some(PanelUnit::Physical(Unit::Kilometer)),
            targets: vec![Some(PanelUnit::Physical(Unit::Mile))],
            ..ConvertPanel::default()
        };
        let (smart, smart_display) = physical_result(&panel, 0);
        panel.preferences.current.number_format = NumberFormat::Fixed;
        panel.preferences.current.decimal_places = 2;
        let (fixed, fixed_display) = physical_result(&panel, 0);
        assert_eq!(smart.value, fixed.value);
        assert!(fixed_display.starts_with("0.62 "));

        panel.preferences.current.number_format = NumberFormat::Scientific;
        let (scientific, scientific_display) = physical_result(&panel, 0);
        assert_eq!(smart.value, scientific.value);
        assert!(scientific_display.starts_with("6.21e-1 "));
        assert!(smart_display.starts_with("0.621371"));
    }

    #[test]
    fn explicit_save_preferences_round_trip_without_persisting_input_or_results() {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path().join("app-data"));
        let path = root.path().join("converter_preferences.json");
        let mut panel = ConvertPanel::with_data_root(&root);
        assert!(!path.exists());
        panel.input = "secret calculation input".into();
        panel.preferences.current.favorites = vec![
            PanelUnit::Physical(Unit::Kilometer),
            PanelUnit::Base(BaseUnit::Binary),
        ];
        panel.preferences.current.number_format = NumberFormat::Fixed;
        panel.preferences.current.decimal_places = 4;
        assert!(panel.preferences.has_unsaved_changes());
        assert!(!path.exists(), "changing preferences does not write them");

        panel.save_preferences();
        assert!(!panel.preferences.has_unsaved_changes());
        let bytes = std::fs::read_to_string(&path).unwrap();
        assert!(!bytes.contains("secret calculation input"));
        assert!(!bytes.contains("results"));

        let reopened = ConvertPanel::with_data_root(&root);
        assert_eq!(
            reopened.preferences.current.number_format,
            NumberFormat::Fixed
        );
        assert_eq!(reopened.preferences.current.decimal_places, 4);
        assert_eq!(
            reopened.preferences.current.favorites,
            vec![
                PanelUnit::Physical(Unit::Kilometer),
                PanelUnit::Base(BaseUnit::Binary)
            ]
        );
        assert!(reopened.input.is_empty());
    }

    #[test]
    fn constrained_window_keeps_value_copy_and_preference_controls_reachable() {
        let mut panel = ConvertPanel {
            input: "1".into(),
            from: Some(PanelUnit::Physical(Unit::Meter)),
            targets: vec![
                Some(PanelUnit::Physical(Unit::Kilometer)),
                Some(PanelUnit::Physical(Unit::Foot)),
            ],
            ..ConvertPanel::default()
        };
        panel.open();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| {
            style.scroll_animation = egui::style::ScrollAnimation::none();
        });
        let size = egui::vec2(520.0, 380.0);
        for _ in 0..4 {
            frame(&mut panel, &ctx, size, Vec::new());
        }
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let window = panel.test_window_rect.expect("real resizable window");
        let input = panel.test_input_rect.expect("real value input");
        let save = panel
            .test_save_preferences_rect
            .expect("explicit preference save control is present");
        let save_id = panel
            .test_save_preferences_id
            .expect("Save Preferences has a stable response id");
        let copy = panel
            .test_copy_rects
            .first()
            .map(|(_, rect)| *rect)
            .expect("a target result exposes Copy");
        assert!(screen.contains_rect(window));
        assert!(window.intersects(input));
        assert!(input.is_positive());
        assert!(save.is_positive());
        assert!(copy.is_positive());
        assert!(
            !panel
                .test_content_viewport
                .expect("content has a bounded viewport")
                .contains_rect(save),
            "the Save control starts below the constrained viewport"
        );

        let copy_id = panel
            .test_copy_ids
            .first()
            .map(|(_, id)| *id)
            .expect("first result has a stable Copy response id");
        assert!(
            tab_until_focused(&mut panel, &ctx, size, copy_id),
            "Tab can reach Copy from the value field"
        );
        for _ in 0..3 {
            frame(&mut panel, &ctx, size, Vec::new());
        }
        let scrolled_copy = panel
            .test_copy_rects
            .iter()
            .find(|(index, _)| *index == 0)
            .map(|(_, rect)| *rect)
            .expect("first result Copy rect is refreshed after scrolling");
        let viewport = panel
            .test_content_viewport
            .expect("the current outer viewport is available");
        let results_viewport = panel
            .test_results_viewport
            .expect("the results area has its own scroll viewport");
        assert!(ctx.memory(|memory| memory.has_focus(copy_id)));
        assert!(viewport.contains_rect(scrolled_copy));
        assert!(results_viewport.contains_rect(scrolled_copy));
        assert!(screen.contains_rect(scrolled_copy));
        assert_eq!(
            click(&mut panel, &ctx, size, scrolled_copy.center()),
            Some(ConvertPanelEffect::CopyText("0.001 km".into())),
            "the revealed Copy control returns the displayed conversion"
        );

        assert!(
            tab_until_focused(&mut panel, &ctx, size, save_id),
            "Tab can reach Save Preferences from the value field"
        );
        for _ in 0..3 {
            frame(&mut panel, &ctx, size, Vec::new());
        }
        let scrolled_save = panel
            .test_save_preferences_rect
            .expect("Save Preferences remains in the scrollable content");
        let viewport = panel
            .test_content_viewport
            .expect("the current outer viewport is available");
        assert!(ctx.memory(|memory| memory.has_focus(save_id)));
        assert!(viewport.contains_rect(scrolled_save));
        assert!(screen.contains_rect(scrolled_save));
        assert_eq!(
            click(&mut panel, &ctx, size, scrolled_save.center()),
            Some(ConvertPanelEffect::SavePreferences),
            "the explicit save action remains reachable at the constrained size"
        );
    }

    #[test]
    fn narrow_layout_contains_controls_and_tab_reveals_nested_copy_and_save() {
        let mut panel = ConvertPanel {
            input: "1".into(),
            from: Some(PanelUnit::Physical(Unit::Meter)),
            targets: vec![
                Some(PanelUnit::Physical(Unit::Kilometer)),
                Some(PanelUnit::Physical(Unit::Centimeter)),
                Some(PanelUnit::Physical(Unit::Millimeter)),
                Some(PanelUnit::Physical(Unit::Foot)),
                Some(PanelUnit::Physical(Unit::Inch)),
                Some(PanelUnit::Physical(Unit::Mile)),
            ],
            ..ConvertPanel::default()
        };
        panel.open();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| {
            style.scroll_animation = egui::style::ScrollAnimation::none();
        });
        let size = egui::vec2(320.0, 650.0);
        for _ in 0..4 {
            frame(&mut panel, &ctx, size, Vec::new());
        }

        let window = panel
            .test_window_rect
            .expect("Converter window is rendered");
        let viewport = panel
            .test_content_viewport
            .expect("Converter content has a bounded viewport");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        assert!(screen.contains_rect(window));
        assert!(!panel.test_control_rects.is_empty());
        for (label, rect) in &panel.test_control_rects {
            assert!(
                viewport.left() <= rect.left() && rect.right() <= viewport.right(),
                "{label} extends outside the narrow content width: {rect:?} vs {viewport:?}"
            );
        }
        assert_eq!(panel.test_target_row_rects.len(), 6);
        for (index, row) in &panel.test_target_row_rects {
            assert!(
                row.height() < 180.0,
                "target row {index} expanded beyond its compact content: {row:?}"
            );
        }

        let copy_id = panel
            .test_copy_ids
            .iter()
            .find(|(index, _)| *index == 5)
            .map(|(_, id)| *id)
            .expect("last result has a stable Copy response id");
        let save_id = panel
            .test_save_preferences_id
            .expect("Save Preferences has a response id");
        let last_copy_rect = panel
            .test_copy_rects
            .iter()
            .find(|(index, _)| *index == 5)
            .map(|(_, rect)| *rect)
            .expect("last result has a Copy control");
        assert!(
            !viewport.contains_rect(last_copy_rect),
            "the nested Copy control starts below the constrained viewport"
        );

        assert!(
            tab_until_focused(&mut panel, &ctx, size, copy_id),
            "Tab reaches the last Copy action"
        );
        for _ in 0..3 {
            frame(&mut panel, &ctx, size, Vec::new());
        }
        let copy_rect = panel
            .test_copy_rects
            .iter()
            .find(|(index, _)| *index == 5)
            .map(|(_, rect)| *rect)
            .unwrap();
        let outer_viewport = panel.test_content_viewport.unwrap();
        let results_viewport = panel.test_results_viewport.unwrap();
        assert!(ctx.memory(|memory| memory.has_focus(copy_id)));
        assert!(outer_viewport.contains_rect(copy_rect));
        assert!(results_viewport.contains_rect(copy_rect));
        let Some(ConvertPanelEffect::CopyText(copied)) =
            click(&mut panel, &ctx, size, copy_rect.center())
        else {
            panic!("the revealed Copy action is clickable");
        };
        assert!(copied.ends_with("mi"));

        assert!(
            tab_until_focused(&mut panel, &ctx, size, save_id),
            "Tab reaches Save Preferences"
        );
        for _ in 0..3 {
            frame(&mut panel, &ctx, size, Vec::new());
        }
        let save_rect = panel.test_save_preferences_rect.unwrap();
        let outer_viewport = panel.test_content_viewport.unwrap();
        assert!(ctx.memory(|memory| memory.has_focus(save_id)));
        assert!(outer_viewport.contains_rect(save_rect));
        assert_eq!(
            click(&mut panel, &ctx, size, save_rect.center()),
            Some(ConvertPanelEffect::SavePreferences),
            "the revealed Save Preferences action is clickable"
        );
    }

    #[test]
    fn explicit_open_focuses_the_value_field() {
        let mut panel = ConvertPanel::default();
        panel.open();
        let ctx = egui::Context::default();
        let size = egui::vec2(760.0, 590.0);
        for _ in 0..3 {
            frame(&mut panel, &ctx, size, Vec::new());
        }
        let input_id = panel.test_input_id.expect("value field has a stable id");
        assert!(ctx.memory(|memory| memory.has_focus(input_id)));
    }

    #[test]
    fn add_target_enter_keeps_focus_on_the_same_control_after_layout_changes() {
        let mut panel = ConvertPanel {
            input: "1".into(),
            from: Some(PanelUnit::Physical(Unit::Meter)),
            targets: vec![Some(PanelUnit::Physical(Unit::Kilometer))],
            ..ConvertPanel::default()
        };
        panel.open();
        let ctx = egui::Context::default();
        let size = egui::vec2(760.0, 590.0);
        for _ in 0..3 {
            frame(&mut panel, &ctx, size, Vec::new());
        }

        let add_target_id = panel
            .test_add_target_id
            .expect("Add target is registered with a stable response ID");
        ctx.memory_mut(|memory| memory.request_focus(add_target_id));
        assert!(ctx.memory(|memory| memory.has_focus(add_target_id)));
        let previous_target_count = panel.targets.len();

        frame(&mut panel, &ctx, size, vec![key(egui::Key::Enter, true)]);
        frame(&mut panel, &ctx, size, vec![key(egui::Key::Enter, false)]);

        assert_eq!(panel.targets.len(), previous_target_count + 1);
        assert_eq!(panel.test_add_target_id, Some(add_target_id));
        assert!(ctx.memory(|memory| memory.has_focus(add_target_id)));
    }

    #[test]
    fn removing_a_target_transfers_focus_to_the_surviving_add_control() {
        let mut panel = ConvertPanel {
            input: "1".into(),
            from: Some(PanelUnit::Physical(Unit::Meter)),
            targets: vec![
                Some(PanelUnit::Physical(Unit::Kilometer)),
                Some(PanelUnit::Physical(Unit::Foot)),
            ],
            ..ConvertPanel::default()
        };
        panel.open();
        let ctx = egui::Context::default();
        let size = egui::vec2(760.0, 590.0);
        for _ in 0..3 {
            frame(&mut panel, &ctx, size, Vec::new());
        }
        let add_target_id = panel.test_add_target_id.unwrap();
        let remove = panel
            .test_remove_rects
            .iter()
            .rev()
            .find(|(index, _)| *index == 1)
            .map(|(_, rect)| *rect)
            .expect("the second target row exposes Remove");

        click(&mut panel, &ctx, size, remove.center());

        assert_eq!(panel.targets.len(), 1);
        assert_eq!(panel.test_add_target_id, Some(add_target_id));
        assert!(ctx.memory(|memory| memory.has_focus(add_target_id)));
    }

    #[test]
    fn copy_effect_contains_the_displayed_target_unit_and_discarded_save_has_no_file_effect() {
        let mut panel = ConvertPanel {
            input: "1".into(),
            from: Some(PanelUnit::Physical(Unit::Meter)),
            targets: vec![Some(PanelUnit::Physical(Unit::Kilometer))],
            ..ConvertPanel::default()
        };
        panel.open();
        let ctx = egui::Context::default();
        let size = egui::vec2(900.0, 680.0);
        for _ in 0..3 {
            frame(&mut panel, &ctx, size, Vec::new());
        }
        let copy = panel.test_copy_rects[0].1;
        let Some(ConvertPanelEffect::CopyText(text)) = click(&mut panel, &ctx, size, copy.center())
        else {
            panic!("Copy returns an explicit effect after a real click");
        };
        assert_eq!(text, "0.001 km");

        let save_directory = tempfile::tempdir().unwrap();
        let save_root = AppDataRoot::from_path(save_directory.path().join("app-data"));
        let save_path = save_root.path().join("converter_preferences.json");
        let mut save_panel = ConvertPanel::with_data_root(&save_root);
        save_panel.open();
        save_panel.preferences.current.number_format = NumberFormat::Fixed;
        let save_ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut save_panel, &save_ctx, size, Vec::new());
        }
        let save = save_panel.test_save_preferences_rect.unwrap();
        assert_eq!(
            click(&mut save_panel, &save_ctx, size, save.center()),
            Some(ConvertPanelEffect::SavePreferences)
        );
        assert!(!save_path.exists(), "the UI stages an explicit effect");
        save_panel.save_preferences();
        assert!(save_path.exists());
        assert!(!save_panel.preferences.has_unsaved_changes());

        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path().join("app-data"));
        let preferences_path = root.path().join("converter_preferences.json");
        let mut stored_panel = ConvertPanel::with_data_root(&root);
        stored_panel.open();
        stored_panel.preferences.current.number_format = NumberFormat::Fixed;
        let stored_ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&mut stored_panel, &stored_ctx, size, Vec::new());
        }
        let save = stored_panel.test_save_preferences_rect.unwrap();
        assert_eq!(
            frame(
                &mut stored_panel,
                &stored_ctx,
                size,
                click_events(save.center(), true)
            ),
            None,
            "a real pointer-down primes the preference button without activating it"
        );
        let (effect, saw_first_ui_pass) = frame_requesting_discard(
            &mut stored_panel,
            &stored_ctx,
            size,
            click_events(save.center(), false),
        );
        assert!(saw_first_ui_pass);
        assert_eq!(effect, None);
        assert!(!preferences_path.exists());
        assert!(stored_panel.preferences.has_unsaved_changes());
        frame(&mut stored_panel, &stored_ctx, size, Vec::new());
        let refreshed_save = stored_panel
            .test_save_preferences_rect
            .expect("the enabled Save control is republished after discard");
        assert_eq!(
            click(
                &mut stored_panel,
                &stored_ctx,
                size,
                refreshed_save.center()
            ),
            Some(ConvertPanelEffect::SavePreferences),
            "the same preference control remains eligible after the discarded pass"
        );
        assert!(!preferences_path.exists());
        assert!(stored_panel.preferences.has_unsaved_changes());
    }

    #[test]
    fn converter_late_root_discard_does_not_publish_preferences_save() {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path().join("app-data"));
        let path = root.path().join("converter_preferences.json");
        let mut panel = ConvertPanel::with_data_root(&root);
        panel.open();
        panel.test_set_fixed_preferences();
        let ctx = egui::Context::default();
        let size = egui::vec2(900.0, 680.0);
        for _ in 0..3 {
            frame(&mut panel, &ctx, size, Vec::new());
        }
        let save = panel
            .test_save_preferences_button_rect()
            .expect("the actual Save Preferences button is visible");
        frame(&mut panel, &ctx, size, click_events(save.center(), true));

        assert_eq!(
            frame_requesting_late_discard(
                &mut panel,
                &ctx,
                size,
                click_events(save.center(), false),
            ),
            None,
            "a late ROOT discard must not publish the preference-save effect"
        );
        assert!(
            !path.exists(),
            "discarded preference saves do not write files"
        );
        assert!(panel.test_preferences_are_dirty());

        frame(&mut panel, &ctx, size, Vec::new());
        let save = panel
            .test_save_preferences_button_rect()
            .expect("an enabled Save control is republished after the discarded pass");
        assert_eq!(
            click(&mut panel, &ctx, size, save.center()),
            Some(ConvertPanelEffect::SavePreferences)
        );
        panel.save_preferences();
        assert!(path.exists(), "the retained explicit save remains usable");
        assert!(!panel.test_preferences_are_dirty());
    }

    #[test]
    fn collapsed_converter_host_close_is_applied_without_contents_ui() {
        let mut panel = ConvertPanel::default();
        panel.open();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut panel, &ctx, size, Vec::new());
        }

        let mut collapsing = egui::collapsing_header::CollapsingState::load_with_default_open(
            &ctx,
            egui::Id::new("convert-panel-window").with("collapsing"),
            true,
        );
        collapsing.set_open(false);
        collapsing.store(&ctx);
        frame(&mut panel, &ctx, size, Vec::new());
        assert!(
            panel.test_input_rect.is_none(),
            "collapsed Converter skips its contents"
        );

        panel.request_close();
        frame(&mut panel, &ctx, size, Vec::new());
        assert!(!panel.open);
        assert!(panel.test_window_rect.is_some());
    }
}
