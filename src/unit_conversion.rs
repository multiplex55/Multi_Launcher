//! Shared physical-unit definitions and conversion behavior.
//!
//! Plugins and UI surfaces should use this module instead of maintaining their
//! own unit aliases, dimensions, or conversion factors.

use std::f64::consts::PI;

const MILE_METERS: f64 = 1609.344;
const US_GALLON_LITERS: f64 = 3.785_411_784;
const IMPERIAL_GALLON_LITERS: f64 = 4.546_09;
const US_MPG_TO_KPL: f64 = (MILE_METERS / 1000.0) / US_GALLON_LITERS;
const IMPERIAL_MPG_TO_KPL: f64 = (MILE_METERS / 1000.0) / IMPERIAL_GALLON_LITERS;

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
    DataRate,
    Duration,
    FuelEconomy,
    Angle,
    Force,
    Torque,
    Frequency,
}

impl Category {
    /// All categories in stable presentation order.
    pub const ALL: &'static [Self] = &[
        Self::Length,
        Self::Area,
        Self::Mass,
        Self::Volume,
        Self::Temperature,
        Self::Speed,
        Self::Pressure,
        Self::Energy,
        Self::Power,
        Self::Duration,
        Self::Angle,
        Self::FuelEconomy,
        Self::Data,
        Self::DataRate,
        Self::Force,
        Self::Torque,
        Self::Frequency,
    ];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Length => "Length",
            Self::Mass => "Mass",
            Self::Temperature => "Temperature",
            Self::Volume => "Volume",
            Self::Area => "Area",
            Self::Speed => "Speed",
            Self::Pressure => "Pressure",
            Self::Energy => "Energy",
            Self::Power => "Power",
            Self::Data => "Data",
            Self::DataRate => "Data rate",
            Self::Duration => "Time / duration",
            Self::FuelEconomy => "Fuel economy",
            Self::Angle => "Angle",
            Self::Force => "Force",
            Self::Torque => "Torque",
            Self::Frequency => "Frequency",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeasurementSystem {
    UsCustomary,
    Imperial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Approximation {
    ThirtyDayMonth,
    ThreeHundredSixtyFiveDayYear,
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
    Yard,
    NauticalMile,
    Kilogram,
    Gram,
    Milligram,
    Pound,
    Ounce,
    Stone,
    Tonne,
    UsShortTon,
    Celsius,
    Fahrenheit,
    Kelvin,
    Liter,
    Milliliter,
    Centiliter,
    Gallon,
    Teaspoon,
    Tablespoon,
    Cup,
    UsFluidOunce,
    UsPint,
    UsQuart,
    ImperialFluidOunce,
    ImperialPint,
    ImperialQuart,
    ImperialGallon,
    CubicCentimeter,
    CubicMeter,
    SquareMeter,
    SquareFoot,
    SquareMillimeter,
    SquareCentimeter,
    SquareKilometer,
    SquareInch,
    SquareYard,
    SquareMile,
    Hectare,
    Acre,
    KilometerPerHour,
    MilePerHour,
    MeterPerSecond,
    FootPerSecond,
    Knot,
    Atmosphere,
    Pascal,
    Bar,
    Psi,
    Kilopascal,
    Megapascal,
    Torr,
    MillimeterOfMercury,
    Joule,
    Kilojoule,
    Megajoule,
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
    BitPerSecond,
    BytePerSecond,
    KilobitPerSecond,
    MegabitPerSecond,
    GigabitPerSecond,
    KilobytePerSecond,
    MegabytePerSecond,
    GigabytePerSecond,
    KibibitPerSecond,
    MebibitPerSecond,
    GibibitPerSecond,
    KibibytePerSecond,
    MebibytePerSecond,
    GibibytePerSecond,
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
    Newton,
    Kilonewton,
    PoundForce,
    NewtonMeter,
    PoundFootTorque,
    PoundInchTorque,
    Hertz,
    Kilohertz,
    Megahertz,
    Gigahertz,
}

#[derive(Debug, Clone, Copy)]
pub struct UnitDefinition {
    pub unit: Unit,
    pub category: Category,
    /// Stable short identifier used for compact display and persisted queries.
    pub symbol: &'static str,
    /// Case-insensitive names recognized by the current simple parser.
    pub aliases: &'static [&'static str],
    /// Conventional spellings whose case carries meaning, checked before the
    /// legacy case-insensitive aliases.
    pub case_sensitive_aliases: &'static [&'static str],
    /// System metadata for US customary and Imperial volume units.
    pub measurement_system: Option<MeasurementSystem>,
    /// Marks durations that intentionally use approximate calendar lengths.
    pub approximation: Option<Approximation>,
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
            case_sensitive_aliases: &[],
            measurement_system: None,
            approximation: None,
            strategy: $strategy,
        }
    };
}

impl UnitDefinition {
    const fn with_case_sensitive_aliases(mut self, aliases: &'static [&'static str]) -> Self {
        self.case_sensitive_aliases = aliases;
        self
    }

    const fn with_measurement_system(mut self, system: MeasurementSystem) -> Self {
        self.measurement_system = Some(system);
        self
    }

    const fn with_approximation(mut self, approximation: Approximation) -> Self {
        self.approximation = Some(approximation);
        self
    }
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
        Strategy::Linear(MILE_METERS),
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
        Yard,
        Length,
        "yd",
        Strategy::Linear(0.9144),
        ["yd", "yard", "yards"]
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
        Milligram,
        Mass,
        "mg",
        Strategy::Linear(0.000_001),
        ["mg", "milligram", "milligrams"]
    ),
    unit!(
        Pound,
        Mass,
        "lb",
        Strategy::Linear(0.453_592_37),
        ["lb", "pound", "pounds", "lbs"]
    ),
    unit!(
        Ounce,
        Mass,
        "oz",
        Strategy::Linear(0.028_349_523_125),
        ["oz", "ounce", "ounces"]
    ),
    unit!(
        Stone,
        Mass,
        "st",
        Strategy::Linear(6.350_293_18),
        ["st", "stone", "stones"]
    ),
    unit!(
        Tonne,
        Mass,
        "tonne",
        Strategy::Linear(1000.0),
        ["t", "tonne", "tonnes", "metric ton", "metric tons"]
    ),
    unit!(
        UsShortTon,
        Mass,
        "ton_us",
        Strategy::Linear(907.18474),
        [
            "ton",
            "tons",
            "us ton",
            "us tons",
            "short ton",
            "short tons",
            "us short ton",
            "us short tons"
        ]
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
    (unit!(
        Gallon,
        Volume,
        "gal",
        Strategy::Linear(US_GALLON_LITERS),
        [
            "gal",
            "gallon",
            "gallons",
            "us gal",
            "us gallon",
            "us gallons"
        ]
    ))
    .with_measurement_system(MeasurementSystem::UsCustomary),
    (unit!(
        Teaspoon,
        Volume,
        "tsp",
        Strategy::Linear(0.004_928_921_593_75),
        ["tsp", "teaspoon", "teaspoons", "us tsp"]
    ))
    .with_measurement_system(MeasurementSystem::UsCustomary),
    (unit!(
        Tablespoon,
        Volume,
        "tbsp",
        Strategy::Linear(0.014_786_764_781_25),
        ["tbsp", "tablespoon", "tablespoons", "us tbsp"]
    ))
    .with_measurement_system(MeasurementSystem::UsCustomary),
    (unit!(
        Cup,
        Volume,
        "cup",
        Strategy::Linear(0.236_588_236_5),
        ["cup", "cups", "us cup", "us cups"]
    ))
    .with_measurement_system(MeasurementSystem::UsCustomary),
    (unit!(
        UsFluidOunce,
        Volume,
        "fl_oz",
        Strategy::Linear(0.029_573_529_562_5),
        [
            "fl oz",
            "floz",
            "fluid ounce",
            "fluid ounces",
            "us fl oz",
            "us fluid ounce",
            "us fluid ounces"
        ]
    ))
    .with_measurement_system(MeasurementSystem::UsCustomary),
    (unit!(
        UsPint,
        Volume,
        "pt",
        Strategy::Linear(0.473_176_473),
        ["pt", "pint", "pints", "us pt", "us pint", "us pints"]
    ))
    .with_measurement_system(MeasurementSystem::UsCustomary),
    (unit!(
        UsQuart,
        Volume,
        "qt",
        Strategy::Linear(0.946_352_946),
        ["qt", "quart", "quarts", "us qt", "us quart", "us quarts"]
    ))
    .with_measurement_system(MeasurementSystem::UsCustomary),
    (unit!(
        ImperialFluidOunce,
        Volume,
        "imp_fl_oz",
        Strategy::Linear(0.028_413_062_5),
        [
            "imperial fl oz",
            "imp fl oz",
            "imperial fluid ounce",
            "imperial fluid ounces",
            "imp_fl_oz",
            "impfloz"
        ]
    ))
    .with_measurement_system(MeasurementSystem::Imperial),
    (unit!(
        ImperialPint,
        Volume,
        "imp_pt",
        Strategy::Linear(0.568_261_25),
        [
            "imperial pint",
            "imperial pints",
            "imp pint",
            "imp pints",
            "imp pt",
            "imp_pt"
        ]
    ))
    .with_measurement_system(MeasurementSystem::Imperial),
    (unit!(
        ImperialQuart,
        Volume,
        "imp_qt",
        Strategy::Linear(1.136_522_5),
        [
            "imperial quart",
            "imperial quarts",
            "imp quart",
            "imp quarts",
            "imp qt",
            "imp_qt"
        ]
    ))
    .with_measurement_system(MeasurementSystem::Imperial),
    (unit!(
        ImperialGallon,
        Volume,
        "imp_gal",
        Strategy::Linear(IMPERIAL_GALLON_LITERS),
        [
            "imperial gallon",
            "imperial gallons",
            "imp gal",
            "imp gallon",
            "imp gallons",
            "imp_gal"
        ]
    ))
    .with_measurement_system(MeasurementSystem::Imperial),
    unit!(
        Centiliter,
        Volume,
        "cl",
        Strategy::Linear(0.01),
        [
            "cl",
            "centiliter",
            "centiliters",
            "centilitre",
            "centilitres"
        ]
    ),
    unit!(
        CubicCentimeter,
        Volume,
        "cm³",
        Strategy::Linear(0.001),
        [
            "cm3",
            "cm^3",
            "cm³",
            "cc",
            "cubic centimeter",
            "cubic centimeters",
            "cubic centimetre",
            "cubic centimetres",
            "cubiccentimeter",
            "cubiccentimeters"
        ]
    ),
    unit!(
        CubicMeter,
        Volume,
        "m³",
        Strategy::Linear(1000.0),
        [
            "m3",
            "m^3",
            "m³",
            "cubic meter",
            "cubic meters",
            "cubic metre",
            "cubic metres",
            "cubicmeter",
            "cubicmeters"
        ]
    ),
    unit!(
        SquareMeter,
        Area,
        "sq_m",
        Strategy::Linear(1.0),
        [
            "sq_m",
            "m2",
            "m^2",
            "m²",
            "square meter",
            "square meters",
            "squaremeter",
            "squaremeters",
            "sqm"
        ]
    ),
    unit!(
        SquareFoot,
        Area,
        "sq_ft",
        Strategy::Linear(0.092_903_04),
        [
            "sq_ft",
            "ft2",
            "ft^2",
            "ft²",
            "square foot",
            "square feet",
            "squarefoot",
            "squarefeet",
            "sqft"
        ]
    ),
    unit!(
        SquareMillimeter,
        Area,
        "mm²",
        Strategy::Linear(0.000_001),
        [
            "mm2",
            "mm^2",
            "mm²",
            "square millimeter",
            "square millimeters",
            "squaremillimeter",
            "squaremillimeters",
            "sq_mm",
            "sqmm"
        ]
    ),
    unit!(
        SquareCentimeter,
        Area,
        "cm²",
        Strategy::Linear(0.0001),
        [
            "cm2",
            "cm^2",
            "cm²",
            "square centimeter",
            "square centimeters",
            "squarecentimeter",
            "squarecentimeters",
            "sq_cm",
            "sqcm"
        ]
    ),
    unit!(
        SquareKilometer,
        Area,
        "km²",
        Strategy::Linear(1_000_000.0),
        [
            "km2",
            "km^2",
            "km²",
            "square kilometer",
            "square kilometers",
            "squarekilometer",
            "squarekilometers",
            "sq_km",
            "sqkm"
        ]
    ),
    unit!(
        SquareInch,
        Area,
        "in²",
        Strategy::Linear(0.000_645_16),
        [
            "in2",
            "in^2",
            "in²",
            "square inch",
            "square inches",
            "squareinch",
            "squareinches",
            "sq_in",
            "sqin"
        ]
    ),
    unit!(
        SquareYard,
        Area,
        "yd²",
        Strategy::Linear(0.836_127_36),
        [
            "yd2",
            "yd^2",
            "yd²",
            "square yard",
            "square yards",
            "squareyard",
            "squareyards",
            "sq_yd",
            "sqyd"
        ]
    ),
    unit!(
        SquareMile,
        Area,
        "mi²",
        Strategy::Linear(2_589_988.110_336),
        [
            "mi2",
            "mi^2",
            "mi²",
            "square mile",
            "square miles",
            "squaremile",
            "squaremiles",
            "sq_mi",
            "sqmi"
        ]
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
        Strategy::Linear(4046.856_422_4),
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
        Strategy::Linear(MILE_METERS / 3600.0),
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
        Knot,
        Speed,
        "kn",
        Strategy::Linear(1852.0 / 3600.0),
        ["kn", "kt", "knot", "knots"]
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
        Kilopascal,
        Pressure,
        "kPa",
        Strategy::Linear(1000.0),
        ["kpa", "kilopascal", "kilopascals"]
    ),
    unit!(
        Megapascal,
        Pressure,
        "MPa",
        Strategy::Linear(1_000_000.0),
        ["mpa", "megapascal", "megapascals"]
    ),
    unit!(
        Torr,
        Pressure,
        "torr",
        Strategy::Linear(101_325.0 / 760.0),
        ["torr"]
    ),
    unit!(
        MillimeterOfMercury,
        Pressure,
        "mmHg",
        Strategy::Linear(133.322_387_415),
        ["mmhg", "millimeter of mercury", "millimeters of mercury"]
    ),
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
        Megajoule,
        Energy,
        "mj",
        Strategy::Linear(1_000_000.0),
        ["mj", "megajoule", "megajoules"]
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
    (unit!(
        Megawatt,
        Power,
        "mw",
        Strategy::Linear(1_000_000.0),
        ["mw", "megawatt", "megawatts"]
    ))
    .with_case_sensitive_aliases(&["MW"]),
    (unit!(
        Milliwatt,
        Power,
        "mwatt",
        Strategy::Linear(0.001),
        ["mwatt", "milliwatt", "milliwatts"]
    ))
    .with_case_sensitive_aliases(&["mW"]),
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
    (unit!(Byte, Data, "byte", Strategy::Linear(1.0), ["byte", "bytes"]))
        .with_case_sensitive_aliases(&["B"]),
    (unit!(
        Kilobyte,
        Data,
        "kb",
        Strategy::Linear(1000.0),
        ["kb", "kilobyte", "kilobytes"]
    ))
    .with_case_sensitive_aliases(&["KB"]),
    (unit!(
        Kibibyte,
        Data,
        "kib",
        Strategy::Linear(1024.0),
        ["kib", "kibibyte", "kibibytes"]
    ))
    .with_case_sensitive_aliases(&["KiB"]),
    (unit!(
        Kilobit,
        Data,
        "kbit",
        Strategy::Linear(125.0),
        ["kbit", "kilobit", "kilobits"]
    ))
    .with_case_sensitive_aliases(&["Kb"]),
    (unit!(
        Kibibit,
        Data,
        "kibit",
        Strategy::Linear(128.0),
        ["kibit", "kibibit", "kibibits"]
    ))
    .with_case_sensitive_aliases(&["Kib"]),
    (unit!(
        Megabyte,
        Data,
        "mb",
        Strategy::Linear(1_000_000.0),
        ["mb", "megabyte", "megabytes"]
    ))
    .with_case_sensitive_aliases(&["MB"]),
    (unit!(
        Mebibyte,
        Data,
        "mib",
        Strategy::Linear(1_048_576.0),
        ["mib", "mebibyte", "mebibytes"]
    ))
    .with_case_sensitive_aliases(&["MiB"]),
    (unit!(
        Megabit,
        Data,
        "mbit",
        Strategy::Linear(125_000.0),
        ["mbit", "megabit", "megabits"]
    ))
    .with_case_sensitive_aliases(&["Mb", "Mbit"]),
    (unit!(
        Mebibit,
        Data,
        "mibit",
        Strategy::Linear(131_072.0),
        ["mibit", "mebibit", "mebibits"]
    ))
    .with_case_sensitive_aliases(&["Mib", "Mibit"]),
    (unit!(
        Gigabyte,
        Data,
        "gb",
        Strategy::Linear(1_000_000_000.0),
        ["gb", "gigabyte", "gigabytes"]
    ))
    .with_case_sensitive_aliases(&["GB"]),
    (unit!(
        Gibibyte,
        Data,
        "gib",
        Strategy::Linear(1_073_741_824.0),
        ["gib", "gibibyte", "gibibytes"]
    ))
    .with_case_sensitive_aliases(&["GiB"]),
    (unit!(
        Gigabit,
        Data,
        "gbit",
        Strategy::Linear(125_000_000.0),
        ["gbit", "gigabit", "gigabits"]
    ))
    .with_case_sensitive_aliases(&["Gb", "Gbit"]),
    (unit!(
        Gibibit,
        Data,
        "gibit",
        Strategy::Linear(134_217_728.0),
        ["gibit", "gibibit", "gibibits"]
    ))
    .with_case_sensitive_aliases(&["Gib", "Gibit"]),
    (unit!(
        Terabyte,
        Data,
        "tb",
        Strategy::Linear(1_000_000_000_000.0),
        ["tb", "terabyte", "terabytes"]
    ))
    .with_case_sensitive_aliases(&["TB"]),
    (unit!(
        Tebibyte,
        Data,
        "tib",
        Strategy::Linear(1_099_511_627_776.0),
        ["tib", "tebibyte", "tebibytes"]
    ))
    .with_case_sensitive_aliases(&["TiB"]),
    (unit!(
        Terabit,
        Data,
        "tbit",
        Strategy::Linear(125_000_000_000.0),
        ["tbit", "terabit", "terabits"]
    ))
    .with_case_sensitive_aliases(&["Tb", "Tbit"]),
    (unit!(
        Tebibit,
        Data,
        "tibit",
        Strategy::Linear(137_438_953_472.0),
        ["tibit", "tebibit", "tebibits"]
    ))
    .with_case_sensitive_aliases(&["Tib", "Tibit"]),
    unit!(
        BitPerSecond,
        DataRate,
        "bit/s",
        Strategy::Linear(1.0),
        [
            "bps",
            "b/s",
            "bit/s",
            "bits/s",
            "bit per second",
            "bits per second"
        ]
    ),
    (unit!(
        BytePerSecond,
        DataRate,
        "B/s",
        Strategy::Linear(8.0),
        ["byte/s", "bytes/s", "byte per second", "bytes per second"]
    ))
    .with_case_sensitive_aliases(&["B/s"]),
    (unit!(
        KilobitPerSecond,
        DataRate,
        "kbps",
        Strategy::Linear(1000.0),
        [
            "kilobit/s",
            "kilobits/s",
            "kilobit per second",
            "kilobits per second"
        ]
    ))
    .with_case_sensitive_aliases(&["kbps", "kbit/s", "kb/s"]),
    (unit!(
        MegabitPerSecond,
        DataRate,
        "Mbps",
        Strategy::Linear(1_000_000.0),
        [
            "megabit/s",
            "megabits/s",
            "megabit per second",
            "megabits per second"
        ]
    ))
    .with_case_sensitive_aliases(&["Mbps", "Mb/s"]),
    (unit!(
        GigabitPerSecond,
        DataRate,
        "Gbps",
        Strategy::Linear(1_000_000_000.0),
        [
            "gigabit/s",
            "gigabits/s",
            "gigabit per second",
            "gigabits per second"
        ]
    ))
    .with_case_sensitive_aliases(&["Gbps", "Gb/s"]),
    (unit!(
        KilobytePerSecond,
        DataRate,
        "KB/s",
        Strategy::Linear(8_000.0),
        [
            "kilobyte/s",
            "kilobytes/s",
            "kilobyte per second",
            "kilobytes per second"
        ]
    ))
    .with_case_sensitive_aliases(&["KB/s"]),
    (unit!(
        MegabytePerSecond,
        DataRate,
        "MB/s",
        Strategy::Linear(8_000_000.0),
        [
            "megabyte/s",
            "megabytes/s",
            "megabyte per second",
            "megabytes per second"
        ]
    ))
    .with_case_sensitive_aliases(&["MB/s"]),
    (unit!(
        GigabytePerSecond,
        DataRate,
        "GB/s",
        Strategy::Linear(8_000_000_000.0),
        [
            "gigabyte/s",
            "gigabytes/s",
            "gigabyte per second",
            "gigabytes per second"
        ]
    ))
    .with_case_sensitive_aliases(&["GB/s"]),
    (unit!(
        KibibitPerSecond,
        DataRate,
        "Kib/s",
        Strategy::Linear(1024.0),
        [
            "kibibit/s",
            "kibibits/s",
            "kibibit per second",
            "kibibits per second"
        ]
    ))
    .with_case_sensitive_aliases(&["Kib/s"]),
    (unit!(
        MebibitPerSecond,
        DataRate,
        "Mib/s",
        Strategy::Linear(1_048_576.0),
        [
            "mebibit/s",
            "mebibits/s",
            "mebibit per second",
            "mebibits per second"
        ]
    ))
    .with_case_sensitive_aliases(&["Mib/s"]),
    (unit!(
        GibibitPerSecond,
        DataRate,
        "Gib/s",
        Strategy::Linear(1_073_741_824.0),
        [
            "gibibit/s",
            "gibibits/s",
            "gibibit per second",
            "gibibits per second"
        ]
    ))
    .with_case_sensitive_aliases(&["Gib/s"]),
    (unit!(
        KibibytePerSecond,
        DataRate,
        "KiB/s",
        Strategy::Linear(8_192.0),
        [
            "kibibyte/s",
            "kibibytes/s",
            "kibibyte per second",
            "kibibytes per second"
        ]
    ))
    .with_case_sensitive_aliases(&["KiB/s"]),
    (unit!(
        MebibytePerSecond,
        DataRate,
        "MiB/s",
        Strategy::Linear(8_388_608.0),
        [
            "mebibyte/s",
            "mebibytes/s",
            "mebibyte per second",
            "mebibytes per second"
        ]
    ))
    .with_case_sensitive_aliases(&["MiB/s"]),
    (unit!(
        GibibytePerSecond,
        DataRate,
        "GiB/s",
        Strategy::Linear(8_589_934_592.0),
        [
            "gibibyte/s",
            "gibibytes/s",
            "gibibyte per second",
            "gibibytes per second"
        ]
    ))
    .with_case_sensitive_aliases(&["GiB/s"]),
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
    (unit!(
        Month,
        Duration,
        "month",
        Strategy::Linear(2_592_000.0),
        ["month", "months", "mo"]
    ))
    .with_approximation(Approximation::ThirtyDayMonth),
    (unit!(
        Year,
        Duration,
        "year",
        Strategy::Linear(31_536_000.0),
        ["year", "years", "yr"]
    ))
    .with_approximation(Approximation::ThreeHundredSixtyFiveDayYear),
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
    (unit!(
        Newton,
        Force,
        "N",
        Strategy::Linear(1.0),
        ["n", "newton", "newtons"]
    ))
    .with_case_sensitive_aliases(&["N"]),
    (unit!(
        Kilonewton,
        Force,
        "kN",
        Strategy::Linear(1000.0),
        ["kilonewton", "kilonewtons"]
    ))
    .with_case_sensitive_aliases(&["kN"]),
    unit!(
        PoundForce,
        Force,
        "lbf",
        Strategy::Linear(4.448_221_615_260_5),
        [
            "lbf",
            "pound-force",
            "pound force",
            "pounds-force",
            "pounds force"
        ]
    ),
    (unit!(
        NewtonMeter,
        Torque,
        "N·m",
        Strategy::Linear(1.0),
        [
            "newton meter",
            "newton meters",
            "newton-meter",
            "newton-meters",
            "newton metre",
            "newton metres",
            "newton-metre",
            "newton-metres"
        ]
    ))
    .with_case_sensitive_aliases(&["Nm", "N·m", "N*m"]),
    unit!(
        PoundFootTorque,
        Torque,
        "lb-ft",
        Strategy::Linear(1.355_817_948_331_400_4),
        [
            "lb-ft",
            "lb ft",
            "lbft",
            "pound-foot",
            "pound-feet",
            "pound foot",
            "pound feet",
            "pound-force foot"
        ]
    ),
    unit!(
        PoundInchTorque,
        Torque,
        "lb-in",
        Strategy::Linear(0.112_984_829_027_616_7),
        [
            "lb-in",
            "lb in",
            "lbin",
            "pound-inch",
            "pound-inches",
            "pound inch",
            "pound inches",
            "pound-force inch"
        ]
    ),
    unit!(
        Hertz,
        Frequency,
        "Hz",
        Strategy::Linear(1.0),
        ["hz", "hertz"]
    ),
    unit!(
        Kilohertz,
        Frequency,
        "kHz",
        Strategy::Linear(1000.0),
        ["khz", "kilohertz"]
    ),
    unit!(
        Megahertz,
        Frequency,
        "MHz",
        Strategy::Linear(1_000_000.0),
        ["mhz", "megahertz"]
    ),
    unit!(
        Gigahertz,
        Frequency,
        "GHz",
        Strategy::Linear(1_000_000_000.0),
        ["ghz", "gigahertz"]
    ),
];

/// Returns all current unit definitions in catalog order.
pub fn catalog() -> &'static [UnitDefinition] {
    UNIT_CATALOG
}

/// Resolves a current unit name or alias without duplicating catalog knowledge.
pub fn unit_by_alias(alias: &str) -> Option<Unit> {
    if let Some(definition) = UNIT_CATALOG.iter().find(|definition| {
        definition
            .case_sensitive_aliases
            .iter()
            .any(|candidate| *candidate == alias)
    }) {
        return Some(definition.unit);
    }

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
            Self::MilesPerUsGallon => value * US_MPG_TO_KPL,
            Self::MilesPerImperialGallon => value * IMPERIAL_MPG_TO_KPL,
        }
    }

    fn from_kilometers_per_liter(self, value: f64) -> f64 {
        match self {
            Self::KilometerPerLiter => value,
            Self::LiterPer100Kilometers => 100.0 / value,
            Self::MilesPerUsGallon => value / US_MPG_TO_KPL,
            Self::MilesPerImperialGallon => value / IMPERIAL_MPG_TO_KPL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Approximation, Category, MeasurementSystem, Unit, catalog, convert, unit_by_alias,
    };

    fn assert_close(actual: f64, expected: f64) {
        let tolerance = 1e-8 * expected.abs().max(1.0);
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual} (tolerance {tolerance})"
        );
    }

    #[test]
    fn catalog_covers_and_labels_each_category() {
        for &category in Category::ALL {
            assert!(catalog().iter().any(|unit| unit.category == category));
            assert!(!category.display_name().is_empty());
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
            for alias in definition.case_sensitive_aliases {
                assert_eq!(
                    unit_by_alias(alias),
                    Some(definition.unit),
                    "exact alias {alias:?} should resolve to {:?}",
                    definition.unit
                );
            }
        }
    }

    #[test]
    fn converts_kilometers_to_miles() {
        assert_close(
            convert(1.0, Unit::Kilometer, Unit::Mile).unwrap(),
            1000.0 / 1609.344,
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
            1.0 / 3.785411784,
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
            30.0 * (1.609344 / 3.785411784),
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
            1.0 / 0.45359237,
        );
    }

    #[test]
    fn converts_square_meters_to_square_feet() {
        assert_close(
            convert(1.0, Unit::SquareMeter, Unit::SquareFoot).unwrap(),
            1.0 / 0.09290304,
        );
    }

    #[test]
    fn converts_kilometers_per_hour_to_miles_per_hour() {
        assert_close(
            convert(100.0, Unit::KilometerPerHour, Unit::MilePerHour).unwrap(),
            100.0 * 1000.0 / 1609.344,
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

        let month = catalog()
            .iter()
            .find(|definition| definition.unit == Unit::Month)
            .unwrap();
        let year = catalog()
            .iter()
            .find(|definition| definition.unit == Unit::Year)
            .unwrap();
        assert_eq!(month.approximation, Some(Approximation::ThirtyDayMonth));
        assert_eq!(
            year.approximation,
            Some(Approximation::ThreeHundredSixtyFiveDayYear)
        );
    }

    #[test]
    fn converts_yards_and_square_and_cubic_units() {
        assert_close(convert(1.0, Unit::Yard, Unit::Foot).unwrap(), 3.0);
        assert_close(
            convert(1.0, Unit::SquareKilometer, Unit::SquareMeter).unwrap(),
            1_000_000.0,
        );
        assert_close(
            convert(1.0, Unit::SquareYard, Unit::SquareMeter).unwrap(),
            0.836_127_36,
        );
        assert_close(
            convert(1.0, Unit::CubicMeter, Unit::CubicCentimeter).unwrap(),
            1_000_000.0,
        );
        assert_close(
            convert(1.0, Unit::CubicCentimeter, Unit::Milliliter).unwrap(),
            1.0,
        );

        assert_eq!(unit_by_alias("m2"), Some(Unit::SquareMeter));
        assert_eq!(unit_by_alias("m^2"), Some(Unit::SquareMeter));
        assert_eq!(unit_by_alias("m²"), Some(Unit::SquareMeter));
        assert_eq!(unit_by_alias("square meters"), Some(Unit::SquareMeter));
        assert_eq!(unit_by_alias("cm³"), Some(Unit::CubicCentimeter));
        assert_eq!(unit_by_alias("m^3"), Some(Unit::CubicMeter));
    }

    #[test]
    fn exact_customary_definitions_remain_coherent() {
        assert_close(convert(1.0, Unit::Mile, Unit::Yard).unwrap(), 1760.0);
        assert_close(convert(1.0, Unit::Pound, Unit::Ounce).unwrap(), 16.0);
        assert_close(
            convert(1.0, Unit::Gallon, Unit::UsFluidOunce).unwrap(),
            128.0,
        );
        assert_close(convert(1.0, Unit::Gallon, Unit::Cup).unwrap(), 16.0);
        assert_close(
            convert(1.0, Unit::ImperialGallon, Unit::ImperialFluidOunce).unwrap(),
            160.0,
        );
        assert_close(
            convert(1.0, Unit::Acre, Unit::SquareFoot).unwrap(),
            43_560.0,
        );
        assert_close(
            convert(1.0, Unit::SquareYard, Unit::SquareFoot).unwrap(),
            9.0,
        );
        assert_close(
            convert(1.0, Unit::MilesPerImperialGallon, Unit::KilometerPerLiter).unwrap(),
            1.609344 / 4.54609,
        );
    }

    #[test]
    fn converts_new_mass_units_with_unambiguous_ton_aliases() {
        assert_close(
            convert(1.0, Unit::Milligram, Unit::Kilogram).unwrap(),
            0.000_001,
        );
        assert_close(
            convert(1.0, Unit::Stone, Unit::Kilogram).unwrap(),
            6.350_293_18,
        );
        assert_close(convert(1.0, Unit::Tonne, Unit::Kilogram).unwrap(), 1000.0);
        assert_close(
            convert(1.0, Unit::UsShortTon, Unit::Kilogram).unwrap(),
            907.18474,
        );
        assert_eq!(unit_by_alias("metric ton"), Some(Unit::Tonne));
        assert_eq!(unit_by_alias("short ton"), Some(Unit::UsShortTon));
        assert_eq!(unit_by_alias("ton"), Some(Unit::UsShortTon));
        assert_eq!(unit_by_alias("oz"), Some(Unit::Ounce));
    }

    #[test]
    fn us_and_imperial_volume_units_are_distinct_and_tagged() {
        assert_close(
            convert(1.0, Unit::Gallon, Unit::ImperialGallon).unwrap(),
            3.785411784 / 4.54609,
        );
        assert_close(
            convert(1.0, Unit::UsFluidOunce, Unit::Milliliter).unwrap(),
            29.573_529_562_5,
        );
        assert_close(
            convert(1.0, Unit::ImperialFluidOunce, Unit::Milliliter).unwrap(),
            28.413_062_5,
        );
        assert_close(
            convert(1.0, Unit::Centiliter, Unit::Milliliter).unwrap(),
            10.0,
        );
        assert_close(convert(1.0, Unit::Cup, Unit::UsFluidOunce).unwrap(), 8.0);
        assert_close(convert(1.0, Unit::UsQuart, Unit::UsPint).unwrap(), 2.0);
        assert_close(
            convert(1.0, Unit::ImperialGallon, Unit::ImperialPint).unwrap(),
            8.0,
        );

        assert_eq!(unit_by_alias("gallon"), Some(Unit::Gallon));
        assert_eq!(unit_by_alias("imperial gallon"), Some(Unit::ImperialGallon));
        assert_eq!(unit_by_alias("fl oz"), Some(Unit::UsFluidOunce));
        assert_eq!(
            unit_by_alias("imperial fl oz"),
            Some(Unit::ImperialFluidOunce)
        );

        let us_gallon = catalog()
            .iter()
            .find(|definition| definition.unit == Unit::Gallon)
            .unwrap();
        let imperial_gallon = catalog()
            .iter()
            .find(|definition| definition.unit == Unit::ImperialGallon)
            .unwrap();
        assert_eq!(
            us_gallon.measurement_system,
            Some(MeasurementSystem::UsCustomary)
        );
        assert_eq!(
            imperial_gallon.measurement_system,
            Some(MeasurementSystem::Imperial)
        );
    }

    #[test]
    fn exact_case_data_aliases_coexist_with_legacy_lowercase_aliases() {
        assert_eq!(unit_by_alias("MB"), Some(Unit::Megabyte));
        assert_eq!(unit_by_alias("Mb"), Some(Unit::Megabit));
        assert_eq!(unit_by_alias("MiB"), Some(Unit::Mebibyte));
        assert_eq!(unit_by_alias("Mib"), Some(Unit::Mebibit));
        assert_eq!(unit_by_alias("GB"), Some(Unit::Gigabyte));
        assert_eq!(unit_by_alias("Gb"), Some(Unit::Gigabit));
        assert_eq!(unit_by_alias("B"), Some(Unit::Byte));
        assert_eq!(unit_by_alias("b"), Some(Unit::Bit));
        assert_eq!(unit_by_alias("mb"), Some(Unit::Megabyte));
        assert_eq!(unit_by_alias("mib"), Some(Unit::Mebibyte));
        assert_eq!(unit_by_alias("kb"), Some(Unit::Kilobyte));
        assert_eq!(unit_by_alias("kib"), Some(Unit::Kibibyte));

        assert_close(
            convert(1.0, Unit::Mebibyte, Unit::Megabyte).unwrap(),
            1.048576,
        );
        assert_close(convert(1.0, Unit::Megabyte, Unit::Megabit).unwrap(), 8.0);
        assert_close(convert(1.0, Unit::Kibibyte, Unit::Kibibit).unwrap(), 8.0);
    }

    #[test]
    fn data_rates_distinguish_bits_and_bytes_without_case_folding() {
        assert_eq!(unit_by_alias("Mbps"), Some(Unit::MegabitPerSecond));
        assert_eq!(unit_by_alias("MB/s"), Some(Unit::MegabytePerSecond));
        assert_eq!(unit_by_alias("Mb/s"), Some(Unit::MegabitPerSecond));
        assert_eq!(unit_by_alias("Kib/s"), Some(Unit::KibibitPerSecond));
        assert_eq!(unit_by_alias("KiB/s"), Some(Unit::KibibytePerSecond));
        assert_eq!(unit_by_alias("Mib/s"), Some(Unit::MebibitPerSecond));
        assert_eq!(unit_by_alias("MiB/s"), Some(Unit::MebibytePerSecond));
        assert_eq!(unit_by_alias("MIB/s"), None);
        assert_close(
            convert(1.0, Unit::BytePerSecond, Unit::BitPerSecond).unwrap(),
            8.0,
        );
        assert_close(
            convert(1.0, Unit::MegabytePerSecond, Unit::MegabitPerSecond).unwrap(),
            8.0,
        );
        assert_close(
            convert(1.0, Unit::KibibytePerSecond, Unit::KibibitPerSecond).unwrap(),
            8.0,
        );
        assert_close(
            convert(1.0, Unit::GigabitPerSecond, Unit::MegabitPerSecond).unwrap(),
            1000.0,
        );
    }

    #[test]
    fn converts_new_speed_and_pressure_units() {
        assert_close(
            convert(1.0, Unit::Knot, Unit::KilometerPerHour).unwrap(),
            1.852,
        );
        assert_close(
            convert(1.0, Unit::Kilopascal, Unit::Pascal).unwrap(),
            1000.0,
        );
        assert_close(
            convert(1.0, Unit::Megapascal, Unit::Kilopascal).unwrap(),
            1000.0,
        );
        assert_close(convert(760.0, Unit::Torr, Unit::Atmosphere).unwrap(), 1.0);
        assert_close(
            convert(760.0, Unit::MillimeterOfMercury, Unit::Atmosphere).unwrap(),
            760.0 * 133.322_387_415 / 101_325.0,
        );
        assert_eq!(unit_by_alias("knot"), Some(Unit::Knot));
        assert_eq!(unit_by_alias("mmHg"), Some(Unit::MillimeterOfMercury));
    }

    #[test]
    fn converts_megajoules_and_case_aware_power_aliases() {
        assert_close(
            convert(1.0, Unit::Megajoule, Unit::KilowattHour).unwrap(),
            1_000_000.0 / 3_600_000.0,
        );
        assert_eq!(unit_by_alias("mw"), Some(Unit::Megawatt));
        assert_eq!(unit_by_alias("mwatt"), Some(Unit::Milliwatt));
        assert_eq!(unit_by_alias("MW"), Some(Unit::Megawatt));
        assert_eq!(unit_by_alias("mW"), Some(Unit::Milliwatt));
        assert_close(convert(1.0, Unit::Milliwatt, Unit::Watt).unwrap(), 0.001);
    }

    #[test]
    fn force_torque_and_frequency_have_separate_categories() {
        assert_close(
            convert(1.0, Unit::Kilonewton, Unit::Newton).unwrap(),
            1000.0,
        );
        assert_close(
            convert(1.0, Unit::PoundForce, Unit::Newton).unwrap(),
            4.448_221_615_260_5,
        );
        assert_close(
            convert(1.0, Unit::PoundFootTorque, Unit::NewtonMeter).unwrap(),
            1.355_817_948_331_400_4,
        );
        assert_close(
            convert(1.0, Unit::PoundInchTorque, Unit::NewtonMeter).unwrap(),
            0.112_984_829_027_616_7,
        );
        assert_close(
            convert(1.0, Unit::Gigahertz, Unit::Megahertz).unwrap(),
            1000.0,
        );

        assert_eq!(unit_by_alias("Nm"), Some(Unit::NewtonMeter));
        assert_eq!(unit_by_alias("N·m"), Some(Unit::NewtonMeter));
        assert_eq!(unit_by_alias("nm"), Some(Unit::NauticalMile));
        assert_eq!(unit_by_alias("kN"), Some(Unit::Kilonewton));
        assert_eq!(unit_by_alias("kn"), Some(Unit::Knot));
        assert_eq!(unit_by_alias("lb-ft"), Some(Unit::PoundFootTorque));
        assert_eq!(unit_by_alias("ft-lb"), Some(Unit::FootPound));
        assert_ne!(Unit::PoundFootTorque.category(), Unit::FootPound.category());
    }

    const PI_FOR_TEST: f64 = 3.141592653589793;
}
