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
    /// Case-insensitive aliases recognized by the shared unit parser.
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
        ["us", "microsecond", "microseconds", "μs", "µs"]
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
        ["l/100km", "l/100 km", "lper100km"]
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

    if let Some(definition) = UNIT_CATALOG
        .iter()
        .find(|definition| definition.symbol == alias)
    {
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

/// Returns whether a catalog unit alias appears as a bounded token in an
/// expression. This lets adapters distinguish likely physical-unit intent
/// without carrying a second alias list.
pub fn contains_unit_alias(input: &str) -> bool {
    input.char_indices().any(|(index, _)| {
        if input[..index]
            .chars()
            .next_back()
            .is_some_and(char::is_alphabetic)
        {
            return false;
        }
        match_unit_prefix(&input[index..]).is_some()
    })
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

fn is_linear_unit(unit: Unit) -> bool {
    matches!(definition(unit).strategy, Strategy::Linear(_))
}

fn conversion_is_in_range(value: f64, source_value: f64, from: Unit, to: Unit) -> bool {
    value.is_finite()
        && !(value == 0.0 && source_value != 0.0 && is_linear_unit(from) && is_linear_unit(to))
}

fn sum_components_in_unit(components: &[QuantityComponent], target: Unit) -> Option<f64> {
    let mut sum = 0.0;
    for component in components {
        let converted = convert(component.value, component.unit, target)?;
        if !conversion_is_in_range(converted, component.value, component.unit, target) {
            return None;
        }
        sum += converted;
        if !sum.is_finite() {
            return None;
        }
    }
    Some(sum)
}

/// Converts between two catalog units, rejecting incompatible categories.
pub fn convert(value: f64, from: Unit, to: Unit) -> Option<f64> {
    if from == to {
        return Some(value);
    }
    let from_definition = definition(from);
    let to_definition = definition(to);
    if from_definition.category != to_definition.category {
        return None;
    }

    match (from_definition.strategy, to_definition.strategy) {
        (Strategy::Linear(from_factor), Strategy::Linear(to_factor)) => {
            Some(value * (from_factor / to_factor))
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

/// One source quantity in a conversion expression.
#[derive(Debug, Clone, PartialEq)]
pub struct QuantityComponent {
    pub value: f64,
    pub unit: Unit,
}

/// Parsed conversion request. The source can contain multiple additive
/// components, while the destination is always a single catalog unit.
#[derive(Debug, Clone, PartialEq)]
pub struct ConversionRequest {
    pub source_expression: String,
    pub destination_expression: String,
    pub components: Vec<QuantityComponent>,
    pub destination: Unit,
}

/// Evaluated conversion and any approximate duration assumptions it used.
#[derive(Debug, Clone, PartialEq)]
pub struct ConversionOutcome {
    pub request: ConversionRequest,
    pub value: f64,
    pub approximations: Vec<Approximation>,
}

impl ConversionOutcome {
    pub fn is_approximate(&self) -> bool {
        !self.approximations.is_empty()
    }
}

/// Structured conversion failures for callers that need to present specific
/// feedback or route other `conv` commands to a different domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversionError {
    InvalidExpression { expression: String },
    InvalidNumber { expression: String },
    UnknownUnit { expression: String },
    IncompatibleUnits { source: Unit, target: Unit },
    NonlinearCompound { unit: Unit },
    OutOfRange { expression: String },
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidExpression { expression } => {
                write!(formatter, "invalid conversion expression: {expression}")
            }
            Self::InvalidNumber { expression } => {
                write!(formatter, "invalid numeric value: {expression}")
            }
            Self::UnknownUnit { expression } => {
                write!(formatter, "unknown unit: {expression}")
            }
            Self::IncompatibleUnits { source, target } => write!(
                formatter,
                "incompatible units: {} and {}",
                source.symbol(),
                target.symbol()
            ),
            Self::NonlinearCompound { unit } => write!(
                formatter,
                "compound quantities are not supported for {}",
                unit.symbol()
            ),
            Self::OutOfRange { expression } => {
                write!(
                    formatter,
                    "value is outside the supported range: {expression}"
                )
            }
        }
    }
}

impl std::error::Error for ConversionError {}

/// Parses a bounded expression such as `6 ft 2 in to cm` using only catalog
/// aliases. The destination must resolve to exactly one unit.
pub fn parse_conversion(input: &str) -> Result<ConversionRequest, ConversionError> {
    let (source_expression, destination_expression) = split_conversion_expression(input)?;
    let source_expression = source_expression.trim();
    let destination_expression = destination_expression.trim();
    if source_expression.is_empty() || destination_expression.is_empty() {
        return Err(ConversionError::InvalidExpression {
            expression: input.trim().to_owned(),
        });
    }

    let destination = match_unit_prefix(destination_expression)
        .filter(|(_, consumed)| *consumed == destination_expression.len())
        .map(|(unit, _)| unit)
        .ok_or_else(|| ConversionError::UnknownUnit {
            expression: destination_expression.to_owned(),
        })?;

    let mut components = Vec::new();
    let mut offset = 0;
    while offset < source_expression.len() {
        offset = skip_whitespace(source_expression, offset);
        if offset == source_expression.len() {
            break;
        }

        let remainder = &source_expression[offset..];
        let (value, number_length) = parse_number_prefix(remainder)?;
        offset += number_length;
        offset = skip_whitespace(source_expression, offset);

        let remainder = &source_expression[offset..];
        let Some((unit, unit_length)) = match_unit_prefix(remainder) else {
            if remainder.starts_with('/') {
                return Err(ConversionError::InvalidNumber {
                    expression: remainder.trim().to_owned(),
                });
            }
            return Err(ConversionError::UnknownUnit {
                expression: remainder.trim().to_owned(),
            });
        };
        components.push(QuantityComponent { value, unit });
        offset += unit_length;

        if offset < source_expression.len() {
            let next = source_expression[offset..].chars().next().unwrap();
            if !next.is_whitespace() && !starts_number(next) {
                return Err(ConversionError::UnknownUnit {
                    expression: source_expression[offset..].trim().to_owned(),
                });
            }
        }
    }

    if components.is_empty() {
        return Err(ConversionError::InvalidExpression {
            expression: input.trim().to_owned(),
        });
    }

    Ok(ConversionRequest {
        source_expression: source_expression.to_owned(),
        destination_expression: destination_expression.to_owned(),
        components,
        destination,
    })
}

/// Parses and evaluates a bounded unit-conversion expression.
pub fn evaluate_conversion(input: &str) -> Result<ConversionOutcome, ConversionError> {
    let request = parse_conversion(input)?;
    evaluate_request(request)
}

/// Evaluates a previously parsed request.
pub fn evaluate_request(request: ConversionRequest) -> Result<ConversionOutcome, ConversionError> {
    let first = request
        .components
        .first()
        .ok_or_else(|| ConversionError::InvalidExpression {
            expression: request.source_expression.clone(),
        })?;
    let source_unit = first.unit;
    let category = source_unit.category();

    for component in request.components.iter().skip(1) {
        if component.unit.category() != category {
            return Err(ConversionError::IncompatibleUnits {
                source: source_unit,
                target: component.unit,
            });
        }
    }
    if request.destination.category() != category {
        return Err(ConversionError::IncompatibleUnits {
            source: source_unit,
            target: request.destination,
        });
    }

    if request.components.len() > 1 {
        for component in &request.components {
            if !matches!(definition(component.unit).strategy, Strategy::Linear(_)) {
                return Err(ConversionError::NonlinearCompound {
                    unit: component.unit,
                });
            }
        }
    }

    let value = if request.components.len() == 1 {
        let converted = convert(first.value, source_unit, request.destination).ok_or(
            ConversionError::IncompatibleUnits {
                source: source_unit,
                target: request.destination,
            },
        )?;
        if !conversion_is_in_range(converted, first.value, source_unit, request.destination) {
            return Err(ConversionError::OutOfRange {
                expression: request.source_expression.clone(),
            });
        }
        converted
    } else {
        let source_sum = sum_components_in_unit(&request.components, source_unit);
        let source_result = source_sum.and_then(|source_value| {
            let converted = convert(source_value, source_unit, request.destination)?;
            conversion_is_in_range(converted, source_value, source_unit, request.destination)
                .then_some(converted)
        });
        source_result
            .or_else(|| sum_components_in_unit(&request.components, request.destination))
            .ok_or_else(|| ConversionError::OutOfRange {
                expression: request.source_expression.clone(),
            })?
    };

    let mut approximations = Vec::new();
    for unit in request
        .components
        .iter()
        .map(|component| component.unit)
        .chain(std::iter::once(request.destination))
    {
        if let Some(approximation) = definition(unit).approximation
            && !approximations.contains(&approximation)
        {
            approximations.push(approximation);
        }
    }

    Ok(ConversionOutcome {
        request,
        value,
        approximations,
    })
}

fn split_conversion_expression(input: &str) -> Result<(&str, &str), ConversionError> {
    let mut separator = None;
    for (index, _) in input.char_indices() {
        let tail = &input[index..];
        if !tail
            .as_bytes()
            .get(..2)
            .is_some_and(|separator| separator.eq_ignore_ascii_case(b"to"))
        {
            continue;
        }
        let previous = input[..index].chars().next_back();
        let next = tail[2..].chars().next();
        if previous.is_some_and(is_word_character) || next.is_some_and(is_word_character) {
            continue;
        }
        if separator.replace((index, index + 2)).is_some() {
            return Err(ConversionError::InvalidExpression {
                expression: input.trim().to_owned(),
            });
        }
    }

    let Some((start, end)) = separator else {
        return Err(ConversionError::InvalidExpression {
            expression: input.trim().to_owned(),
        });
    };
    Ok((&input[..start], &input[end..]))
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn skip_whitespace(input: &str, mut offset: usize) -> usize {
    while let Some(character) = input[offset..].chars().next()
        && character.is_whitespace()
    {
        offset += character.len_utf8();
    }
    offset
}

fn starts_number(character: char) -> bool {
    character.is_ascii_digit()
        || matches!(character, '+' | '-' | '.')
        || unicode_fraction(character).is_some()
}

fn match_unit_prefix(input: &str) -> Option<(Unit, usize)> {
    let mut best = None;

    for definition in UNIT_CATALOG {
        if let Some(consumed) = match_alias_prefix(input, definition.symbol, true)
            && unit_alias_boundary(input, consumed)
        {
            update_alias_match(&mut best, definition.unit, consumed, true);
        }
        for alias in definition.case_sensitive_aliases {
            if let Some(consumed) = match_alias_prefix(input, alias, true)
                && unit_alias_boundary(input, consumed)
            {
                update_alias_match(&mut best, definition.unit, consumed, true);
            }
        }
        for alias in definition.aliases {
            if let Some(consumed) = match_alias_prefix(input, alias, false)
                && unit_alias_boundary(input, consumed)
            {
                update_alias_match(&mut best, definition.unit, consumed, false);
            }
        }
    }

    best.map(|(unit, consumed, _)| (unit, consumed))
}

fn update_alias_match(
    best: &mut Option<(Unit, usize, bool)>,
    unit: Unit,
    consumed: usize,
    exact: bool,
) {
    if best.is_none_or(|(_, best_length, best_exact)| {
        consumed > best_length || (consumed == best_length && exact && !best_exact)
    }) {
        *best = Some((unit, consumed, exact));
    }
}

fn match_alias_prefix(input: &str, alias: &str, case_sensitive: bool) -> Option<usize> {
    let mut input_offset = 0;
    let mut alias_characters = alias.chars().peekable();
    while let Some(alias_character) = alias_characters.next() {
        if alias_character.is_whitespace() {
            let mut found_whitespace = false;
            while let Some(character) = input[input_offset..].chars().next()
                && character.is_whitespace()
            {
                found_whitespace = true;
                input_offset += character.len_utf8();
            }
            if !found_whitespace {
                return None;
            }
            continue;
        }

        let character = input[input_offset..].chars().next()?;
        let matches = if case_sensitive {
            character == alias_character
        } else {
            character.eq_ignore_ascii_case(&alias_character)
        };
        if !matches {
            return None;
        }
        input_offset += character.len_utf8();
    }

    Some(input_offset)
}

fn unit_alias_boundary(input: &str, consumed: usize) -> bool {
    match input[consumed..].chars().next() {
        None => true,
        Some(character) => character.is_whitespace() || starts_number(character),
    }
}

fn parse_number_prefix(input: &str) -> Result<(f64, usize), ConversionError> {
    let invalid = || ConversionError::InvalidNumber {
        expression: input.trim().to_owned(),
    };
    let out_of_range = || ConversionError::OutOfRange {
        expression: input.trim().to_owned(),
    };

    let mut offset = 0;
    let negative = input.starts_with('-');
    if input.starts_with('+') || input.starts_with('-') {
        offset += 1;
    }

    if let Some(character) = input[offset..].chars().next()
        && let Some(fraction) = unicode_fraction(character)
    {
        let value = if negative { -fraction } else { fraction };
        offset += character.len_utf8();
        if !value.is_finite() {
            return Err(out_of_range());
        }
        return Ok((value, offset));
    }

    let bytes = input.as_bytes();
    let integer_start = offset;
    while offset < bytes.len() && bytes[offset].is_ascii_digit() {
        offset += 1;
    }
    let has_integer_digits = offset > integer_start;
    let mut has_decimal = false;
    if bytes.get(offset) == Some(&b'.') {
        has_decimal = true;
        offset += 1;
        let decimal_start = offset;
        while offset < bytes.len() && bytes[offset].is_ascii_digit() {
            offset += 1;
        }
        if !has_integer_digits && decimal_start == offset {
            return Err(invalid());
        }
    } else if !has_integer_digits {
        return Err(invalid());
    }

    let mut has_exponent = false;
    if matches!(bytes.get(offset), Some(b'e' | b'E')) {
        let mut exponent_end = offset + 1;
        if matches!(bytes.get(exponent_end), Some(b'+' | b'-')) {
            exponent_end += 1;
        }
        let exponent_start = exponent_end;
        while exponent_end < bytes.len() && bytes[exponent_end].is_ascii_digit() {
            exponent_end += 1;
        }
        if exponent_end > exponent_start {
            offset = exponent_end;
            has_exponent = true;
        }
    }

    let integer_only = !has_decimal && !has_exponent;
    if integer_only && bytes.get(offset) == Some(&b'/') {
        let numerator: f64 = input[..offset].parse().map_err(|_| out_of_range())?;
        offset += 1;
        let denominator_start = offset;
        while offset < bytes.len() && bytes[offset].is_ascii_digit() {
            offset += 1;
        }
        if denominator_start == offset {
            return Err(invalid());
        }
        let denominator: f64 = input[denominator_start..offset]
            .parse()
            .map_err(|_| out_of_range())?;
        if !numerator.is_finite() || !denominator.is_finite() {
            return Err(out_of_range());
        }
        if denominator == 0.0 {
            return Err(invalid());
        }
        let value = numerator / denominator;
        if !value.is_finite() || (value == 0.0 && numerator != 0.0) {
            return Err(out_of_range());
        }
        return Ok((value, offset));
    }

    let mut value: f64 = input[..offset].parse().map_err(|_| out_of_range())?;
    if !value.is_finite()
        || (value == 0.0
            && contains_nonzero_digit(input[..offset].split(['e', 'E']).next().unwrap_or("")))
    {
        return Err(out_of_range());
    }

    if integer_only {
        let unsigned_whole = value.abs();
        let mut fraction_offset = offset;
        let mut had_space = false;
        while let Some(character) = input[fraction_offset..].chars().next()
            && character.is_whitespace()
        {
            had_space = true;
            fraction_offset += character.len_utf8();
        }
        let fraction_start = if had_space { fraction_offset } else { offset };
        if let Some(character) = input[fraction_start..].chars().next()
            && let Some(fraction) = unicode_fraction(character)
        {
            value = (if negative { -1.0 } else { 1.0 }) * (unsigned_whole + fraction);
            offset = fraction_start + character.len_utf8();
            if !value.is_finite() {
                return Err(out_of_range());
            }
            return Ok((value, offset));
        }

        if had_space
            && input
                .as_bytes()
                .get(fraction_start)
                .is_some_and(u8::is_ascii_digit)
        {
            let mut whole_fraction_end = fraction_start;
            while whole_fraction_end < bytes.len() && bytes[whole_fraction_end].is_ascii_digit() {
                whole_fraction_end += 1;
            }
            if bytes.get(whole_fraction_end) == Some(&b'/') {
                let numerator_start = fraction_start;
                let mut denominator_start = whole_fraction_end + 1;
                while denominator_start < bytes.len() && bytes[denominator_start].is_ascii_digit() {
                    denominator_start += 1;
                }
                if denominator_start == whole_fraction_end + 1 {
                    return Err(invalid());
                }
                let numerator: f64 = input[numerator_start..whole_fraction_end]
                    .parse()
                    .map_err(|_| out_of_range())?;
                let denominator: f64 = input[whole_fraction_end + 1..denominator_start]
                    .parse()
                    .map_err(|_| out_of_range())?;
                if !numerator.is_finite() || !denominator.is_finite() {
                    return Err(out_of_range());
                }
                if denominator == 0.0 {
                    return Err(invalid());
                }
                let magnitude = unsigned_whole + numerator / denominator;
                value = if negative { -magnitude } else { magnitude };
                offset = denominator_start;
                if !value.is_finite() || (value == 0.0 && magnitude != 0.0) {
                    return Err(out_of_range());
                }
                return Ok((value, offset));
            }
        }
    }

    Ok((value, offset))
}

fn contains_nonzero_digit(value: &str) -> bool {
    value
        .chars()
        .any(|character| matches!(character, '1'..='9'))
}

fn unicode_fraction(character: char) -> Option<f64> {
    match character {
        '½' => Some(0.5),
        '¼' => Some(0.25),
        '¾' => Some(0.75),
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
        Approximation, Category, ConversionError, MeasurementSystem, Unit, catalog, convert,
        evaluate_conversion, unit_by_alias,
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
    fn every_catalog_symbol_and_alias_resolves_to_its_unit() {
        for definition in catalog() {
            assert_eq!(
                unit_by_alias(definition.symbol),
                Some(definition.unit),
                "symbol {:?} should resolve to {:?}",
                definition.symbol,
                definition.unit
            );
            let (matched, consumed) = super::match_unit_prefix(definition.symbol)
                .unwrap_or_else(|| panic!("symbol {:?} should parse", definition.symbol));
            assert_eq!(matched, definition.unit, "symbol {:?}", definition.symbol);
            assert_eq!(
                consumed,
                definition.symbol.len(),
                "symbol {:?}",
                definition.symbol
            );
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

    #[test]
    fn parses_simple_signed_decimal_and_case_insensitive_to() {
        let outcome = evaluate_conversion("  10   kilometers   TO   mi  ").unwrap();
        assert_close(outcome.value, 6.213_711_922_373_339);
        assert_eq!(outcome.request.destination, Unit::Mile);
        assert_eq!(outcome.request.source_expression, "10   kilometers");

        let temperature = evaluate_conversion("-40 C to F").unwrap();
        assert_close(temperature.value, -40.0);
        assert_close(evaluate_conversion("32 F to C").unwrap().value, 0.0);
        assert_close(evaluate_conversion("273.15 K to C").unwrap().value, 0.0);
        assert_close(
            evaluate_conversion("1.234e2 m to cm").unwrap().value,
            12_340.0,
        );
    }

    #[test]
    fn resolves_plural_multiword_and_square_aliases_from_the_catalog() {
        assert_close(
            evaluate_conversion("1   kilometers to meter")
                .unwrap()
                .value,
            1000.0,
        );
        assert_close(
            evaluate_conversion("1   us    fluid   ounce to ml")
                .unwrap()
                .value,
            29.573_529_562_5,
        );
        assert_close(evaluate_conversion("1 m2 to m^2").unwrap().value, 1.0);
        assert_close(evaluate_conversion("1 m^2 to m²").unwrap().value, 1.0);
        assert_close(
            evaluate_conversion("1 m² to square meters").unwrap().value,
            1.0,
        );

        assert_close(
            evaluate_conversion("1 MB to bit").unwrap().value,
            8_000_000.0,
        );
        assert_close(
            evaluate_conversion("1 Mb to bit").unwrap().value,
            1_000_000.0,
        );
        assert_eq!(
            evaluate_conversion("1 MB to byte").unwrap().value,
            1_000_000.0
        );
    }

    #[test]
    fn parses_simple_mixed_and_unicode_fractions() {
        assert_close(
            evaluate_conversion("1/2 cup to ml").unwrap().value,
            118.294_118_25,
        );
        assert_close(
            evaluate_conversion("1 1/2 cups to ml").unwrap().value,
            354.882_354_75,
        );
        assert_close(evaluate_conversion("-1 1/2 m to cm").unwrap().value, -150.0);
        assert_close(
            evaluate_conversion("½ cup to ml").unwrap().value,
            118.294_118_25,
        );
        assert_close(
            evaluate_conversion("1½ cup to ml").unwrap().value,
            354.882_354_75,
        );
        assert_close(
            evaluate_conversion("1 ½ cup to ml").unwrap().value,
            354.882_354_75,
        );
        assert_close(evaluate_conversion("¾ in to mm").unwrap().value, 19.05);
        assert_close(evaluate_conversion("1 µs to ns").unwrap().value, 1000.0);
        assert_close(
            evaluate_conversion("10 L/100 km to km/L").unwrap().value,
            10.0,
        );
    }

    #[test]
    fn adds_compatible_linear_compound_quantities() {
        assert_close(
            evaluate_conversion("6 ft 2 in to cm").unwrap().value,
            187.96,
        );
        assert_close(
            evaluate_conversion("5 lb 8 oz to kg").unwrap().value,
            2.494_758_035,
        );
        assert_close(
            evaluate_conversion("1 cup 2 tbsp to ml").unwrap().value,
            266.161_766_062_5,
        );
        assert_close(evaluate_conversion("6ft2in to cm").unwrap().value, 187.96);
        let large_compound = evaluate_conversion("1 mm 1e308 km to mi").unwrap();
        assert!(large_compound.value.is_finite());
        assert!((large_compound.value / 1e308 - 1000.0 / 1609.344).abs() < 1e-12);

        assert_eq!(
            evaluate_conversion("1e308 m -1e308 m to cm").unwrap().value,
            0.0
        );
        assert_eq!(
            evaluate_conversion("1e-323 bit 1e-323 bit 1e-323 bit to byte")
                .unwrap()
                .value,
            f64::from_bits(1)
        );
    }

    #[test]
    fn reports_structured_parser_and_evaluation_failures() {
        assert!(matches!(
            evaluate_conversion("1/0 cup to ml"),
            Err(ConversionError::InvalidNumber { .. })
        ));
        assert!(matches!(
            evaluate_conversion("1/2/3 cup to ml"),
            Err(ConversionError::InvalidNumber { .. })
        ));
        let oversized_denominator = "9".repeat(400);
        let overflowing_fraction = format!("0/{oversized_denominator} m to cm");
        assert!(matches!(
            evaluate_conversion(&overflowing_fraction),
            Err(ConversionError::OutOfRange { .. })
        ));
        assert!(matches!(
            evaluate_conversion("1 ft 1 kg to cm"),
            Err(ConversionError::IncompatibleUnits { .. })
        ));
        assert!(matches!(
            evaluate_conversion("1 C 1 F to K"),
            Err(ConversionError::NonlinearCompound { .. })
        ));
        assert!(matches!(
            evaluate_conversion("1 unknown to cm"),
            Err(ConversionError::UnknownUnit { .. })
        ));
        assert!(matches!(
            evaluate_conversion("1 m to unknown"),
            Err(ConversionError::UnknownUnit { .. })
        ));
        assert!(matches!(
            evaluate_conversion("1 m cm"),
            Err(ConversionError::InvalidExpression { .. })
        ));
        assert!(matches!(
            evaluate_conversion("1 m trailing to cm"),
            Err(ConversionError::InvalidNumber { .. } | ConversionError::UnknownUnit { .. })
        ));
        assert!(matches!(
            evaluate_conversion("1e308 GB to bit"),
            Err(ConversionError::OutOfRange { .. })
        ));
    }

    #[test]
    fn linear_conversions_avoid_intermediate_overflow_and_underflow() {
        let large = evaluate_conversion("1e308 km to mi").unwrap().value;
        assert!(large.is_finite());
        assert!((large / 1e308 - 1000.0 / 1609.344).abs() < 1e-12);

        let tiny = evaluate_conversion("1e-320 mm² to cm²").unwrap().value;
        assert!(tiny > 0.0);
        assert!((tiny / 1e-320 - 0.01).abs() < 0.001);
    }

    #[test]
    fn carries_approximation_metadata_for_month_and_year_conversions() {
        let month = evaluate_conversion("1 month to day").unwrap();
        assert_eq!(month.value, 30.0);
        assert!(month.is_approximate());
        assert_eq!(month.approximations, vec![Approximation::ThirtyDayMonth]);

        let year = evaluate_conversion("1 year to day").unwrap();
        assert_eq!(year.value, 365.0);
        assert!(year.is_approximate());
        assert_eq!(
            year.approximations,
            vec![Approximation::ThreeHundredSixtyFiveDayYear]
        );
    }

    const PI_FOR_TEST: f64 = 3.141592653589793;
}
