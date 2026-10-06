//! Shared physical-unit definitions and conversion behavior.
//!
//! Plugins and UI surfaces should use this module instead of maintaining their
//! own unit aliases, dimensions, or conversion factors.

use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Length,
    Mass,
    Temperature,
    Volume,
    Area,
    Speed,
    Pressure,
    Energy,
    Power,
    Data,
    Duration,
    FuelEconomy,
    Angle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    Meter,
    Kilometer,
    Mile,
    Foot,
    Inch,
    Centimeter,
    Millimeter,
    NauticalMile,
    Kilogram,
    Gram,
    Pound,
    Ounce,
    Celsius,
    Fahrenheit,
    Kelvin,
    Liter,
    Milliliter,
    Gallon,
    SquareMeter,
    SquareFoot,
    Hectare,
    Acre,
    KilometerPerHour,
    MilePerHour,
    MeterPerSecond,
    FootPerSecond,
    Atmosphere,
    Pascal,
    Bar,
    Psi,
    Joule,
    Kilojoule,
    Calorie,
    Kilocalorie,
    WattHour,
    KilowattHour,
    Btu,
    FootPound,
    Electronvolt,
    Watt,
    Kilowatt,
    Megawatt,
    Milliwatt,
    Horsepower,
    Bit,
    Byte,
    Kilobyte,
    Kibibyte,
    Kilobit,
    Kibibit,
    Megabyte,
    Mebibyte,
    Megabit,
    Mebibit,
    Gigabyte,
    Gibibyte,
    Gigabit,
    Gibibit,
    Terabyte,
    Tebibyte,
    Terabit,
    Tebibit,
    Nanosecond,
    Microsecond,
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
    KilometerPerLiter,
    LiterPer100Kilometers,
    MilesPerUsGallon,
    MilesPerImperialGallon,
    Degree,
    Radian,
    Gradian,
    Arcminute,
    Arcsecond,
    Revolution,
}

#[derive(Debug, Clone, Copy)]
pub struct UnitDefinition {
    pub unit: Unit,
    pub category: Category,
    /// Stable short identifier used for compact display and persisted queries.
    pub symbol: &'static str,
    /// Case-insensitive names recognized by the current simple parser.
    pub aliases: &'static [&'static str],
    strategy: Strategy,
}

#[derive(Debug, Clone, Copy)]
enum Strategy {
    Linear(f64),
    Temperature(TemperatureScale),
    FuelEconomy(FuelEconomyScale),
}

#[derive(Debug, Clone, Copy)]
enum TemperatureScale {
    Celsius,
    Fahrenheit,
    Kelvin,
}

#[derive(Debug, Clone, Copy)]
enum FuelEconomyScale {
    KilometerPerLiter,
    LiterPer100Kilometers,
    MilesPerUsGallon,
    MilesPerImperialGallon,
}

macro_rules! unit {
    ($unit:ident, $category:ident, $symbol:literal, $strategy:expr, [$($alias:literal),+ $(,)?]) => {
        UnitDefinition {
            unit: Unit::$unit,
            category: Category::$category,
            symbol: $symbol,
            aliases: &[$($alias),+],
            strategy: $strategy,
        }
    };
}

/// The authoritative catalog for the units supported by the existing inline
/// converter. New categories and units should be added here.
pub static UNIT_CATALOG: &[UnitDefinition] = &[
    unit!(
        Meter,
        Length,
        "m",
        Strategy::Linear(1.0),
        ["m", "meter", "meters"]
    ),
    unit!(
        Kilometer,
        Length,
        "km",
        Strategy::Linear(1000.0),
        ["km", "kilometer", "kilometers"]
    ),
    unit!(
        Mile,
        Length,
        "mi",
        Strategy::Linear(1609.34),
        ["mi", "mile", "miles"]
    ),
    unit!(
        Foot,
        Length,
        "ft",
        Strategy::Linear(0.3048),
        ["ft", "foot", "feet"]
    ),
    unit!(
        Inch,
        Length,
        "in",
        Strategy::Linear(0.0254),
        ["in", "inch", "inches"]
    ),
    unit!(
        Centimeter,
        Length,
        "cm",
        Strategy::Linear(0.01),
        ["cm", "centimeter", "centimeters"]
    ),
    unit!(
        Millimeter,
        Length,
        "mm",
        Strategy::Linear(0.001),
        ["mm", "millimeter", "millimeters"]
    ),
    unit!(
        NauticalMile,
        Length,
        "nm",
        Strategy::Linear(1852.0),
        ["nm", "nauticalmile", "nauticalmiles"]
    ),
    unit!(
        Kilogram,
        Mass,
        "kg",
        Strategy::Linear(1.0),
        ["kg", "kilogram", "kilograms"]
    ),
    unit!(
        Gram,
        Mass,
        "g",
        Strategy::Linear(0.001),
        ["g", "gram", "grams"]
    ),
    unit!(
        Pound,
        Mass,
        "lb",
        Strategy::Linear(0.453_592),
        ["lb", "pound", "pounds", "lbs"]
    ),
    unit!(
        Ounce,
        Mass,
        "oz",
        Strategy::Linear(0.028_349_5),
        ["oz", "ounce", "ounces"]
    ),
    unit!(
        Celsius,
        Temperature,
        "c",
        Strategy::Temperature(TemperatureScale::Celsius),
        ["c", "celsius", "centigrade", "°c"]
    ),
    unit!(
        Fahrenheit,
        Temperature,
        "f",
        Strategy::Temperature(TemperatureScale::Fahrenheit),
        ["f", "fahrenheit", "°f"]
    ),
    unit!(
        Kelvin,
        Temperature,
        "k",
        Strategy::Temperature(TemperatureScale::Kelvin),
        ["k", "kelvin", "kelvins"]
    ),
    unit!(
        Liter,
        Volume,
        "l",
        Strategy::Linear(1.0),
        ["l", "liter", "liters", "litre", "litres"]
    ),
    unit!(
        Milliliter,
        Volume,
        "ml",
        Strategy::Linear(0.001),
        [
            "ml",
            "milliliter",
            "milliliters",
            "millilitre",
            "millilitres"
        ]
    ),
    unit!(
        Gallon,
        Volume,
        "gal",
        Strategy::Linear(3.785_41),
        ["gal", "gallon", "gallons"]
    ),
    unit!(
        SquareMeter,
        Area,
        "sq_m",
        Strategy::Linear(1.0),
        ["sq_m", "m2", "squaremeter", "squaremeters"]
    ),
    unit!(
        SquareFoot,
        Area,
        "sq_ft",
        Strategy::Linear(0.092_903),
        ["sq_ft", "ft2", "squarefoot", "squarefeet"]
    ),
    unit!(
        Hectare,
        Area,
        "ha",
        Strategy::Linear(10_000.0),
        ["ha", "hectare", "hectares"]
    ),
    unit!(
        Acre,
        Area,
        "ac",
        Strategy::Linear(4046.86),
        ["ac", "acre", "acres"]
    ),
    unit!(
        KilometerPerHour,
        Speed,
        "kph",
        Strategy::Linear(1000.0 / 3600.0),
        ["kph", "km/h", "kilometerperhour"]
    ),
    unit!(
        MilePerHour,
        Speed,
        "mph",
        Strategy::Linear(1609.34 / 3600.0),
        ["mph", "mileperhour"]
    ),
    unit!(
        MeterPerSecond,
        Speed,
        "mps",
        Strategy::Linear(1.0),
        ["mps", "m/s"]
    ),
    unit!(
        FootPerSecond,
        Speed,
        "fps",
        Strategy::Linear(0.3048),
        ["fps", "ft/s"]
    ),
    unit!(
        Atmosphere,
        Pressure,
        "atm",
        Strategy::Linear(101_325.0),
        ["atm"]
    ),
    unit!(
        Pascal,
        Pressure,
        "pa",
        Strategy::Linear(1.0),
        ["pa", "pascal", "pascals"]
    ),
    unit!(Bar, Pressure, "bar", Strategy::Linear(100_000.0), ["bar"]),
    unit!(Psi, Pressure, "psi", Strategy::Linear(6894.757), ["psi"]),
    unit!(
        Joule,
        Energy,
        "j",
        Strategy::Linear(1.0),
        ["j", "joule", "joules"]
    ),
    unit!(
        Kilojoule,
        Energy,
        "kj",
        Strategy::Linear(1000.0),
        ["kj", "kilojoule", "kilojoules"]
    ),
    unit!(
        Calorie,
        Energy,
        "cal",
        Strategy::Linear(4.184),
        ["cal", "calorie", "calories"]
    ),
    unit!(
        Kilocalorie,
        Energy,
        "kcal",
        Strategy::Linear(4184.0),
        ["kcal", "kilocalorie", "kilocalories"]
    ),
    unit!(
        WattHour,
        Energy,
        "wh",
        Strategy::Linear(3600.0),
        ["wh", "watt-hour", "watt hour"]
    ),
    unit!(
        KilowattHour,
        Energy,
        "kwh",
        Strategy::Linear(3_600_000.0),
        ["kwh", "kilowatt-hour", "kilowatt hour"]
    ),
    unit!(
        Btu,
        Energy,
        "btu",
        Strategy::Linear(1055.06),
        ["btu", "btus"]
    ),
    unit!(
        FootPound,
        Energy,
        "ftlb",
        Strategy::Linear(1.35582),
        ["ftlb", "footpound", "foot-pound", "ft-lb"]
    ),
    unit!(
        Electronvolt,
        Energy,
        "ev",
        Strategy::Linear(1.602_18e-19),
        ["ev", "electronvolt", "electronvolts"]
    ),
    unit!(
        Watt,
        Power,
        "w",
        Strategy::Linear(1.0),
        ["w", "watt", "watts"]
    ),
    unit!(
        Kilowatt,
        Power,
        "kw",
        Strategy::Linear(1000.0),
        ["kw", "kilowatt", "kilowatts"]
    ),
    unit!(
        Megawatt,
        Power,
        "mw",
        Strategy::Linear(1_000_000.0),
        ["mw", "megawatt", "megawatts"]
    ),
    unit!(
        Milliwatt,
        Power,
        "mwatt",
        Strategy::Linear(0.001),
        ["mwatt", "milliwatt", "milliwatts"]
    ),
    unit!(
        Horsepower,
        Power,
        "hp",
        Strategy::Linear(745.7),
        ["hp", "horsepower"]
    ),
    unit!(
        Bit,
        Data,
        "bit",
        Strategy::Linear(0.125),
        ["bit", "bits", "b"]
    ),
    unit!(Byte, Data, "byte", Strategy::Linear(1.0), ["byte", "bytes"]),
    unit!(
        Kilobyte,
        Data,
        "kb",
        Strategy::Linear(1000.0),
        ["kb", "kilobyte", "kilobytes"]
    ),
    unit!(
        Kibibyte,
        Data,
        "kib",
        Strategy::Linear(1024.0),
        ["kib", "kibibyte", "kibibytes"]
    ),
    unit!(
        Kilobit,
        Data,
        "kbit",
        Strategy::Linear(125.0),
        ["kbit", "kilobit", "kilobits"]
    ),
    unit!(
        Kibibit,
        Data,
        "kibit",
        Strategy::Linear(128.0),
        ["kibit", "kibibit", "kibibits"]
    ),
    unit!(
        Megabyte,
        Data,
        "mb",
        Strategy::Linear(1_000_000.0),
        ["mb", "megabyte", "megabytes"]
    ),
    unit!(
        Mebibyte,
        Data,
        "mib",
        Strategy::Linear(1_048_576.0),
        ["mib", "mebibyte", "mebibytes"]
    ),
    unit!(
        Megabit,
        Data,
        "mbit",
        Strategy::Linear(125_000.0),
        ["mbit", "megabit", "megabits"]
    ),
    unit!(
        Mebibit,
        Data,
        "mibit",
        Strategy::Linear(131_072.0),
        ["mibit", "mebibit", "mebibits"]
    ),
    unit!(
        Gigabyte,
        Data,
        "gb",
        Strategy::Linear(1_000_000_000.0),
        ["gb", "gigabyte", "gigabytes"]
    ),
    unit!(
        Gibibyte,
        Data,
        "gib",
        Strategy::Linear(1_073_741_824.0),
        ["gib", "gibibyte", "gibibytes"]
    ),
    unit!(
        Gigabit,
        Data,
        "gbit",
        Strategy::Linear(125_000_000.0),
        ["gbit", "gigabit", "gigabits"]
    ),
    unit!(
        Gibibit,
        Data,
        "gibit",
        Strategy::Linear(134_217_728.0),
        ["gibit", "gibibit", "gibibits"]
    ),
    unit!(
        Terabyte,
        Data,
        "tb",
        Strategy::Linear(1_000_000_000_000.0),
        ["tb", "terabyte", "terabytes"]
    ),
    unit!(
        Tebibyte,
        Data,
        "tib",
        Strategy::Linear(1_099_511_627_776.0),
        ["tib", "tebibyte", "tebibytes"]
    ),
    unit!(
        Terabit,
        Data,
        "tbit",
        Strategy::Linear(125_000_000_000.0),
        ["tbit", "terabit", "terabits"]
    ),
    unit!(
        Tebibit,
        Data,
        "tibit",
        Strategy::Linear(137_438_953_472.0),
        ["tibit", "tebibit", "tebibits"]
    ),
    unit!(
        Nanosecond,
        Duration,
        "ns",
        Strategy::Linear(1e-9),
        ["ns", "nanosecond", "nanoseconds"]
    ),
    unit!(
        Microsecond,
        Duration,
        "us",
        Strategy::Linear(1e-6),
        ["us", "microsecond", "microseconds", "μs"]
    ),
    unit!(
        Millisecond,
        Duration,
        "ms",
        Strategy::Linear(1e-3),
        ["ms", "millisecond", "milliseconds"]
    ),
    unit!(
        Second,
        Duration,
        "s",
        Strategy::Linear(1.0),
        ["s", "sec", "second", "seconds"]
    ),
    unit!(
        Minute,
        Duration,
        "min",
        Strategy::Linear(60.0),
        ["min", "minute", "minutes"]
    ),
    unit!(
        Hour,
        Duration,
        "h",
        Strategy::Linear(3600.0),
        ["h", "hr", "hour", "hours"]
    ),
    unit!(
        Day,
        Duration,
        "day",
        Strategy::Linear(86_400.0),
        ["day", "days", "d"]
    ),
    unit!(
        Week,
        Duration,
        "week",
        Strategy::Linear(604_800.0),
        ["week", "weeks", "wk"]
    ),
    unit!(
        Month,
        Duration,
        "month",
        Strategy::Linear(2_592_000.0),
        ["month", "months", "mo"]
    ),
    unit!(
        Year,
        Duration,
        "year",
        Strategy::Linear(31_536_000.0),
        ["year", "years", "yr"]
    ),
    unit!(
        KilometerPerLiter,
        FuelEconomy,
        "kpl",
        Strategy::FuelEconomy(FuelEconomyScale::KilometerPerLiter),
        ["kpl", "km/l", "kmperliter"]
    ),
    unit!(
        LiterPer100Kilometers,
        FuelEconomy,
        "lp100km",
        Strategy::FuelEconomy(FuelEconomyScale::LiterPer100Kilometers),
        ["l/100km", "lper100km"]
    ),
    unit!(
        MilesPerUsGallon,
        FuelEconomy,
        "mpg",
        Strategy::FuelEconomy(FuelEconomyScale::MilesPerUsGallon),
        ["mpg", "milespergallon", "milepergallon"]
    ),
    unit!(
        MilesPerImperialGallon,
        FuelEconomy,
        "mpgimp",
        Strategy::FuelEconomy(FuelEconomyScale::MilesPerImperialGallon),
        ["mpgimp", "mpg_uk", "mileperimperialgallon"]
    ),
    unit!(
        Degree,
        Angle,
        "deg",
        Strategy::Linear(PI / 180.0),
        ["deg", "degree", "degrees"]
    ),
    unit!(
        Radian,
        Angle,
        "rad",
        Strategy::Linear(1.0),
        ["rad", "radian", "radians"]
    ),
    unit!(
        Gradian,
        Angle,
        "grad",
        Strategy::Linear(PI / 200.0),
        ["grad", "gradian", "gradians", "gon"]
    ),
    unit!(
        Arcminute,
        Angle,
        "arcmin",
        Strategy::Linear(PI / 10_800.0),
        ["arcmin", "arcminute", "arcminutes"]
    ),
    unit!(
        Arcsecond,
        Angle,
        "arcsec",
        Strategy::Linear(PI / 648_000.0),
        ["arcsec", "arcsecond", "arcseconds"]
    ),
    unit!(
        Revolution,
        Angle,
        "rev",
        Strategy::Linear(2.0 * PI),
        ["rev", "revolution", "revolutions", "turn", "turns"]
    ),
];

/// Returns all current unit definitions in catalog order.
pub fn catalog() -> &'static [UnitDefinition] {
    UNIT_CATALOG
}

/// Resolves a current unit name or alias without duplicating catalog knowledge.
pub fn unit_by_alias(alias: &str) -> Option<Unit> {
    let normalized = alias.to_lowercase();
    UNIT_CATALOG
        .iter()
        .find(|definition| {
            definition
                .aliases
                .iter()
                .any(|candidate| *candidate == normalized)
        })
        .map(|definition| definition.unit)
}

/// Iterates over the units in one physical category.
pub fn units_in_category(category: Category) -> impl Iterator<Item = &'static UnitDefinition> {
    UNIT_CATALOG
        .iter()
        .filter(move |definition| definition.category == category)
}

impl Unit {
    /// Returns the stable short identifier used by the current inline results.
    pub fn symbol(self) -> &'static str {
        definition(self).symbol
    }

    /// Returns the physical category that determines conversion compatibility.
    pub fn category(self) -> Category {
        definition(self).category
    }
}

fn definition(unit: Unit) -> &'static UnitDefinition {
    UNIT_CATALOG
        .iter()
        .find(|definition| definition.unit == unit)
        .expect("every Unit variant must have a catalog definition")
}

/// Converts between two catalog units, rejecting incompatible categories.
pub fn convert(value: f64, from: Unit, to: Unit) -> Option<f64> {
    let from_definition = definition(from);
    let to_definition = definition(to);
    if from_definition.category != to_definition.category {
        return None;
    }

    match (from_definition.strategy, to_definition.strategy) {
        (Strategy::Linear(from_factor), Strategy::Linear(to_factor)) => {
            Some(value * from_factor / to_factor)
        }
        (Strategy::Temperature(from_scale), Strategy::Temperature(to_scale)) => {
            Some(to_scale.from_kelvin(from_scale.to_kelvin(value)))
        }
        (Strategy::FuelEconomy(from_scale), Strategy::FuelEconomy(to_scale)) => {
            Some(to_scale.from_kilometers_per_liter(from_scale.to_kilometers_per_liter(value)))
        }
        _ => None,
    }
}

impl TemperatureScale {
    fn to_kelvin(self, value: f64) -> f64 {
        match self {
            Self::Celsius => value + 273.15,
            Self::Fahrenheit => (value - 32.0) * 5.0 / 9.0 + 273.15,
            Self::Kelvin => value,
        }
    }

    fn from_kelvin(self, value: f64) -> f64 {
        match self {
            Self::Celsius => value - 273.15,
            Self::Fahrenheit => (value - 273.15) * 9.0 / 5.0 + 32.0,
            Self::Kelvin => value,
        }
    }
}

impl FuelEconomyScale {
    fn to_kilometers_per_liter(self, value: f64) -> f64 {
        match self {
            Self::KilometerPerLiter => value,
            Self::LiterPer100Kilometers => 100.0 / value,
            Self::MilesPerUsGallon => value * 0.425_144,
            Self::MilesPerImperialGallon => value * 0.354_006,
        }
    }

    fn from_kilometers_per_liter(self, value: f64) -> f64 {
        match self {
            Self::KilometerPerLiter => value,
            Self::LiterPer100Kilometers => 100.0 / value,
            Self::MilesPerUsGallon => value / 0.425_144,
            Self::MilesPerImperialGallon => value / 0.354_006,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Category, Unit, catalog, convert, unit_by_alias};

    fn assert_close(actual: f64, expected: f64) {
        let tolerance = 1e-8 * expected.abs().max(1.0);
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual} (tolerance {tolerance})"
        );
    }

    #[test]
    fn catalog_covers_each_existing_category() {
        let categories = [
            Category::Length,
            Category::Mass,
            Category::Temperature,
            Category::Volume,
            Category::Area,
            Category::Speed,
            Category::Pressure,
            Category::Energy,
            Category::Power,
            Category::Data,
            Category::Duration,
            Category::FuelEconomy,
            Category::Angle,
        ];

        for category in categories {
            assert!(catalog().iter().any(|unit| unit.category == category));
        }
    }

    #[test]
    fn every_catalog_alias_resolves_to_its_unit() {
        for definition in catalog() {
            for alias in definition.aliases {
                assert_eq!(
                    unit_by_alias(alias),
                    Some(definition.unit),
                    "alias {alias:?} should resolve to {:?}",
                    definition.unit
                );
            }
        }
    }

    #[test]
    fn converts_kilometers_to_miles() {
        assert_close(
            convert(1.0, Unit::Kilometer, Unit::Mile).unwrap(),
            1000.0 / 1609.34,
        );
    }

    #[test]
    fn converts_fahrenheit_to_celsius() {
        assert_close(convert(32.0, Unit::Fahrenheit, Unit::Celsius).unwrap(), 0.0);
    }

    #[test]
    fn converts_centimeters_to_inches() {
        assert_close(
            convert(100.0, Unit::Centimeter, Unit::Inch).unwrap(),
            100.0 / 2.54,
        );
    }

    #[test]
    fn converts_liters_to_gallons() {
        assert_close(
            convert(1.0, Unit::Liter, Unit::Gallon).unwrap(),
            1.0 / 3.78541,
        );
    }

    #[test]
    fn converts_kilowatt_hours_to_joules() {
        assert_close(
            convert(1.0, Unit::KilowattHour, Unit::Joule).unwrap(),
            3_600_000.0,
        );
    }

    #[test]
    fn converts_kilowatts_to_watts() {
        assert_close(convert(2.0, Unit::Kilowatt, Unit::Watt).unwrap(), 2000.0);
    }

    #[test]
    fn converts_bits_to_bytes() {
        assert_close(convert(8.0, Unit::Bit, Unit::Byte).unwrap(), 1.0);
    }

    #[test]
    fn converts_hours_to_minutes() {
        assert_close(convert(2.0, Unit::Hour, Unit::Minute).unwrap(), 120.0);
    }

    #[test]
    fn converts_us_miles_per_gallon_to_kilometers_per_liter() {
        assert_close(
            convert(30.0, Unit::MilesPerUsGallon, Unit::KilometerPerLiter).unwrap(),
            12.75432,
        );
    }

    #[test]
    fn converts_degrees_to_radians() {
        assert_close(
            convert(180.0, Unit::Degree, Unit::Radian).unwrap(),
            PI_FOR_TEST,
        );
    }

    #[test]
    fn converts_kilograms_to_pounds() {
        assert_close(
            convert(1.0, Unit::Kilogram, Unit::Pound).unwrap(),
            1.0 / 0.453592,
        );
    }

    #[test]
    fn converts_square_meters_to_square_feet() {
        assert_close(
            convert(1.0, Unit::SquareMeter, Unit::SquareFoot).unwrap(),
            1.0 / 0.092903,
        );
    }

    #[test]
    fn converts_kilometers_per_hour_to_miles_per_hour() {
        assert_close(
            convert(100.0, Unit::KilometerPerHour, Unit::MilePerHour).unwrap(),
            100.0 * 1000.0 / 1609.34,
        );
    }

    #[test]
    fn converts_bar_to_psi() {
        assert_close(
            convert(1.0, Unit::Bar, Unit::Psi).unwrap(),
            100_000.0 / 6894.757,
        );
    }

    #[test]
    fn incompatible_categories_are_rejected_and_ounce_remains_mass() {
        assert_eq!(unit_by_alias("OZ"), Some(Unit::Ounce));
        assert_eq!(unit_by_alias("ounce").unwrap().category(), Category::Mass);
        assert_eq!(convert(1.0, Unit::Ounce, Unit::Liter), None);
        assert_eq!(convert(1.0, Unit::Kilometer, Unit::Second), None);
    }

    #[test]
    fn preserves_approximate_month_and_year_durations() {
        assert_close(convert(1.0, Unit::Month, Unit::Day).unwrap(), 30.0);
        assert_close(convert(1.0, Unit::Year, Unit::Day).unwrap(), 365.0);
    }

    const PI_FOR_TEST: f64 = 3.141592653589793;
}
