use crate::gui::LauncherApp;
use crate::unit_conversion::{self, Category, Unit};
use eframe::egui;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Simple conversion panel with an input box and two combo boxes.
pub struct ConvertPanel {
    pub open: bool,
    input: String,
    result: String,
    filter: String,
    category: PanelCategory,
    from: Option<PanelUnit>,
    to: Option<PanelUnit>,
    focus_input: bool,
}

impl Default for ConvertPanel {
    fn default() -> Self {
        Self {
            open: false,
            input: String::new(),
            result: String::new(),
            filter: String::new(),
            category: PanelCategory::Physical(Category::Length),
            from: None,
            to: None,
            focus_input: false,
        }
    }
}

impl ConvertPanel {
    /// Open the panel.
    pub fn open(&mut self) {
        self.open = true;
        self.focus_input = true;
        self.result.clear();
    }

    /// Draw the panel UI when open.
    pub fn ui(&mut self, ctx: &egui::Context, _app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Convert")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Value");
                let val_edit = ui.text_edit_singleline(&mut self.input);
                if self.focus_input {
                    val_edit.request_focus();
                    self.focus_input = false;
                }

                self.compute_result();
                ui.label("Result");
                ui.add_enabled(false, egui::TextEdit::singleline(&mut self.result));

                ui.label("Type");
                let mut selected_category = self.category;
                egui::ComboBox::from_id_source("convert_category")
                    .selected_text(self.category.label())
                    .show_ui(ui, |ui| {
                        for category in category_options() {
                            ui.selectable_value(&mut selected_category, category, category.label());
                        }
                    });
                if selected_category != self.category {
                    self.select_category(selected_category);
                }

                ui.label("Filter");
                ui.text_edit_singleline(&mut self.filter);

                // Build options after the category and filter controls so a
                // category change is reflected in both combos this frame.
                let filtered = self.filtered_units();
                self.reconcile_selections(&filtered);
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("From")
                        .selected_text(self.from.map_or("Select unit", PanelUnit::label))
                        .show_ui(ui, |ui| {
                            for option in &filtered {
                                ui.selectable_value(&mut self.from, Some(*option), option.label());
                            }
                        });
                    egui::ComboBox::from_label("To")
                        .selected_text(self.to.map_or("Select unit", PanelUnit::label))
                        .show_ui(ui, |ui| {
                            for option in &filtered {
                                ui.selectable_value(&mut self.to, Some(*option), option.label());
                            }
                        });
                });
            });
        self.compute_result();
        self.open = open;
    }

    fn select_category(&mut self, category: PanelCategory) {
        self.category = category;
        self.from = None;
        self.to = None;
        let filtered = self.filtered_units();
        self.reconcile_selections(&filtered);
    }

    fn filtered_units(&self) -> Vec<PanelUnit> {
        let filter = self.filter.trim().to_lowercase();
        category_units(self.category)
            .into_iter()
            .filter(|unit| unit.matches_filter(&filter))
            .collect()
    }

    fn reconcile_selections(&mut self, options: &[PanelUnit]) {
        let valid_options = category_units(self.category);
        let fallback = options
            .first()
            .copied()
            .or_else(|| valid_options.first().copied());
        if !self
            .from
            .is_some_and(|selected| valid_options.contains(&selected))
        {
            self.from = fallback;
        }
        if !self
            .to
            .is_some_and(|selected| valid_options.contains(&selected))
        {
            self.to = fallback;
        }
    }

    fn compute_result(&mut self) {
        self.result.clear();
        if self.input.trim().is_empty() {
            return;
        }

        match (self.category, self.from, self.to) {
            (
                PanelCategory::Physical(category),
                Some(PanelUnit::Physical(from)),
                Some(PanelUnit::Physical(to)),
            ) if from.category() == category && to.category() == category => {
                let expression =
                    format!("{} {} to {}", self.input.trim(), from.symbol(), to.symbol());
                if let Ok(outcome) = unit_conversion::evaluate_conversion(&expression)
                    && let Some(formatted) =
                        crate::common::number_format::format_number(outcome.value)
                {
                    self.result = formatted;
                }
            }
            (PanelCategory::Base, Some(PanelUnit::Base(from)), Some(PanelUnit::Base(to))) => {
                if let Some(result) = convert_base(&self.input, from, to) {
                    self.result = result;
                }
            }
            _ => {}
        }
    }
}

fn convert_base(input: &str, from: BaseUnit, to: BaseUnit) -> Option<String> {
    let trimmed = input.trim();
    let (negative, digits) = if let Some(rest) = trimmed.strip_prefix('-') {
        (true, rest)
    } else {
        (false, trimmed)
    };
    let value = i64::from_str_radix(digits, from.radix()).ok()?;
    let value = if negative { -value } else { value };
    Some(match to {
        BaseUnit::Decimal => value.to_string(),
        BaseUnit::Hexadecimal => format!("{value:x}"),
        BaseUnit::Binary => format!("{value:b}"),
        BaseUnit::Octal => format!("{value:o}"),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        BaseUnit, ConvertPanel, PanelCategory, PanelUnit, category_options, category_units,
    };
    use crate::unit_conversion::{self, Category, Unit};

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
            to: Some(PanelUnit::Physical(Unit::Mile)),
            filter: "oz".to_owned(),
            ..ConvertPanel::default()
        };

        panel.select_category(PanelCategory::Physical(Category::Mass));
        let filtered = panel.filtered_units();
        assert_eq!(filtered, vec![PanelUnit::Physical(Unit::Ounce)]);
        assert_eq!(panel.from, Some(PanelUnit::Physical(Unit::Ounce)));
        assert_eq!(panel.to, Some(PanelUnit::Physical(Unit::Ounce)));
    }

    #[test]
    fn filtering_uses_case_insensitive_symbols_and_catalog_aliases_without_collapsing_units() {
        let mut panel = ConvertPanel {
            category: PanelCategory::Physical(Category::Data),
            filter: "mB".to_owned(),
            ..ConvertPanel::default()
        };
        let matching = panel.filtered_units();
        assert!(matching.contains(&PanelUnit::Physical(Unit::Megabyte)));
        assert!(matching.contains(&PanelUnit::Physical(Unit::Megabit)));
        assert_ne!(Unit::Megabyte, Unit::Megabit);

        panel.filter = "MEGABIT".to_owned();
        assert_eq!(
            panel.filtered_units(),
            vec![PanelUnit::Physical(Unit::Megabit)]
        );

        panel.category = PanelCategory::Physical(Category::Volume);
        panel.filter = "imperial gallon".to_owned();
        assert_eq!(
            panel.filtered_units(),
            vec![PanelUnit::Physical(Unit::ImperialGallon)]
        );
    }

    #[test]
    fn filtering_does_not_discard_valid_category_selections() {
        let mut panel = ConvertPanel {
            from: Some(PanelUnit::Physical(Unit::Kilometer)),
            to: Some(PanelUnit::Physical(Unit::Mile)),
            filter: "mi".to_owned(),
            ..ConvertPanel::default()
        };

        let filtered = panel.filtered_units();
        panel.reconcile_selections(&filtered);
        assert_eq!(panel.from, Some(PanelUnit::Physical(Unit::Kilometer)));
        assert_eq!(panel.to, Some(PanelUnit::Physical(Unit::Mile)));

        panel.filter = "no such unit".to_owned();
        let filtered = panel.filtered_units();
        assert!(filtered.is_empty());
        panel.reconcile_selections(&filtered);
        assert_eq!(panel.from, Some(PanelUnit::Physical(Unit::Kilometer)));
        assert_eq!(panel.to, Some(PanelUnit::Physical(Unit::Mile)));
    }

    #[test]
    fn physical_panel_calculation_matches_inline_domain_and_smart_formatting() {
        let mut panel = ConvertPanel {
            input: "1/2".to_owned(),
            category: PanelCategory::Physical(Category::Length),
            from: Some(PanelUnit::Physical(Unit::Kilometer)),
            to: Some(PanelUnit::Physical(Unit::Mile)),
            ..ConvertPanel::default()
        };
        panel.compute_result();

        let inline = unit_conversion::evaluate_conversion("1/2 km to mi").unwrap();
        let expected = crate::common::number_format::format_number(inline.value).unwrap();
        assert_eq!(panel.result, expected);
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
            let mut panel = ConvertPanel {
                input: input.to_owned(),
                category: PanelCategory::Physical(category),
                from: Some(PanelUnit::Physical(from)),
                to: Some(PanelUnit::Physical(to)),
                ..ConvertPanel::default()
            };
            panel.compute_result();
            assert_eq!(
                panel.result,
                expected,
                "{} to {}",
                from.symbol(),
                to.symbol()
            );
        }
    }

    #[test]
    fn base_category_keeps_its_existing_radix_conversion_behavior() {
        let mut panel = ConvertPanel {
            input: "ff".to_owned(),
            category: PanelCategory::Base,
            from: Some(PanelUnit::Base(BaseUnit::Hexadecimal)),
            to: Some(PanelUnit::Base(BaseUnit::Decimal)),
            ..ConvertPanel::default()
        };
        panel.compute_result();
        assert_eq!(panel.result, "255");

        panel.input = "-10".to_owned();
        panel.from = Some(PanelUnit::Base(BaseUnit::Decimal));
        panel.to = Some(PanelUnit::Base(BaseUnit::Binary));
        panel.compute_result();
        assert_eq!(panel.result, format!("{:b}", -10_i64));
    }
}
