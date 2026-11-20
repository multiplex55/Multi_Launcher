use super::super::examples::{CalculationExample, ExampleExpectation};
use super::{ReferenceCategory, ReferenceEntry, ReferenceId};
use crate::unit_conversion::{Category, Unit};

use ReferenceId::{Call, Topic};

macro_rules! unit_topic {
    ($key:literal, $category:ident, $summary:literal, $limits:expr,
        $source:literal => $value:expr, $target:ident, [$($concept:literal),*]) => {
        ReferenceEntry {
            id: Topic($key),
            category: ReferenceCategory::Units,
            spelling: Some("amount source_unit to target_unit"),
            aliases: &[],
            concepts: &[$($concept),*],
            summary: $summary,
            parameters: "Finite source amount and two recognized units in this category. The unit catalog lists the accepted spellings.",
            returns: "Physical quantity in the chosen target unit.",
            limits: $limits,
            example: CalculationExample::new(
                $key, $source, $summary, ExampleExpectation::Physical { value: $value, unit: Unit::$target },
            ),
            variants: &[],
            related: &[Topic("Physical conversions"), Topic("Dimensional arithmetic")],
            unit_category: Some(Category::$category),
        }
    };
}

const GENERAL: &str = "The source and target must have compatible units. Nonfinite or unrepresentable results report an error.";
const CASE: &str = "Use conventional letter case for ambiguous bit/byte and metric spellings. Incompatible units report an error.";

pub(super) const UNITS: &[ReferenceEntry] = &[
    ReferenceEntry {
        id: Topic("Physical conversions"),
        category: ReferenceCategory::Units,
        spelling: Some("amount unit to target_unit, reference to target_unit"),
        aliases: &["to", "TO"],
        concepts: &[
            "convert",
            "unit conversion",
            "measurements",
            "mixed units",
            "fractional units",
        ],
        summary: "Convert measurements using the shared unit catalog. Compatible mixed terms such as feet and inches can be combined.",
        parameters: "One or more finite measurement terms, or an earlier physical result, and a compatible target. A unit amount can use a decimal, scientific notation or a supported fraction.",
        returns: "Physical quantity with the requested unit. Conversion preserves any approximation provenance.",
        limits: "Different categories cannot be converted merely because they have matching base dimensions. Some time units use fixed approximate durations. Temperature and fuel economy permit direct conversions.",
        example: CalculationExample::new(
            "Convert feet and inches to centimeters",
            "6 ft 2 in to cm",
            "The compound length resolves to one physical quantity.",
            ExampleExpectation::Physical {
                value: 187.96,
                unit: Unit::Centimeter,
            },
        ),
        variants: &[],
        related: &[
            Topic("Dimensional arithmetic"),
            Topic("Length units"),
            Topic("Exact radix conversion"),
            Call("add_duration"),
        ],
        unit_category: None,
    },
    ReferenceEntry {
        id: Topic("Dimensional arithmetic"),
        category: ReferenceCategory::Units,
        spelling: Some("quantity + quantity, quantity - quantity, *, /, ^, to target_unit"),
        aliases: &["quantity arithmetic", "compound unit"],
        concepts: &[
            "unit powers",
            "physical quantity",
            "derived unit",
            "dimension",
            "dimensional analysis",
        ],
        summary: "Add or subtract compatible quantities, scale by ordinary numbers, or use multiplication, division and powers to combine dimensions.",
        parameters: "Use linear units for dimensional operands. Addition and subtraction require compatible dimensions. Exponents are integral and each unit/product power has magnitude at most eight.",
        returns: "Physical or dimensional quantity, or a dimensionless result when compatible dimensions cancel.",
        limits: "Temperature and fuel economy are conversion-only. A compound target must have the right dimension. A unit power following a slash applies to the denominator. The parser permits at most 256 tokens.",
        example: CalculationExample::new(
            "Convert a calculated area",
            "area = (2 m) * (3 m)\narea to cm^2",
            "Multiplication yields an area which can be expressed in squared centimeters.",
            ExampleExpectation::Dimensional {
                formatted_result: "60000 cm²",
                approximate: false,
            },
        ),
        variants: &[],
        related: &[
            Topic("Physical conversions"),
            Topic("Area units"),
            Topic("Resource limits"),
        ],
        unit_category: None,
    },
    unit_topic!("Length units", Length, "Convert linear distance.", GENERAL,
        "1 km to m" => 1000.0, Meter, ["distance", "length"]),
    unit_topic!("Area units", Area, "Convert area between square units and land measures.", GENERAL,
        "1 ha to sq_m" => 10000.0, SquareMeter, ["area", "square units"]),
    unit_topic!("Mass units", Mass, "Convert mass between metric, customary and other catalog units.", GENERAL,
        "1 kg to g" => 1000.0, Gram, ["mass", "weight"]),
    unit_topic!("Volume units", Volume, "Convert liquid or cubic volume. US customary and Imperial measures have distinct units.", GENERAL,
        "1 l to ml" => 1000.0, Milliliter, ["volume", "liquid"]),
    unit_topic!("Temperature units", Temperature, "Convert between offset temperature scales.", "Temperature uses conversion-only syntax. It cannot participate in dimensional arithmetic.",
        "-40 C to F" => -40.0, Fahrenheit, ["temperature", "thermal"]),
    unit_topic!("Speed units", Speed, "Convert linear speed.", GENERAL,
        "1 mps to kph" => 3.6, KilometerPerHour, ["speed", "velocity"]),
    unit_topic!("Pressure units", Pressure, "Convert pressure, including atmospheres, pascals and customary units.", GENERAL,
        "1 atm to pa" => 101325.0, Pascal, ["pressure"]),
    unit_topic!("Energy units", Energy, "Convert work or energy. Torque remains a separate category.", GENERAL,
        "1 kj to j" => 1000.0, Joule, ["energy", "work"]),
    unit_topic!("Power units", Power, "Convert power.", CASE,
        "1 kw to w" => 1000.0, Watt, ["power"]),
    unit_topic!("Duration units", Duration, "Convert fixed physical durations.", "A month means about 30 days and a year about 365 days. Use calendar shifts for real months and years.",
        "1 h to min" => 60.0, Minute, ["duration", "physical time"]),
    unit_topic!("Angle units", Angle, "Convert angular measure.", GENERAL,
        "180 deg to rad" => std::f64::consts::PI, Radian, ["angle", "angular"]),
    unit_topic!("Fuel economy units", FuelEconomy, "Convert distance per volume or consumption per distance.", "Fuel economy uses nonlinear conversion-only syntax. Reciprocal conversions require a positive amount.",
        "10 kpl to lp100km" => 10.0, LiterPer100Kilometers, ["fuel economy", "fuel consumption"]),
    unit_topic!("Data units", Data, "Convert digital amounts using decimal and binary prefixes.", CASE,
        "1 MB to bit" => 8000000.0, Bit, ["digital storage", "data", "IEC", "SI"]),
    unit_topic!("Data rate units", DataRate, "Convert digital bandwidth in bits or bytes per second.", CASE,
        "1 MB/s to bit/s" => 8000000.0, BitPerSecond, ["throughput", "bandwidth", "data rate"]),
    unit_topic!("Force units", Force, "Convert linearly applied force.", GENERAL,
        "1 kN to N" => 1000.0, Newton, ["force"]),
    unit_topic!("Torque units", Torque, "Convert turning moment. Energy remains a distinct catalog category.", GENERAL,
        "1 lb-ft to lb-in" => 12.0, PoundInchTorque, ["torque", "moment"]),
    unit_topic!("Frequency units", Frequency, "Convert cycles per unit of time.", GENERAL,
        "1 GHz to MHz" => 1000.0, Megahertz, ["frequency", "hertz"]),
];
