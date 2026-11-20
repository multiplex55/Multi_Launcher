use super::examples::{CalculationExample, ExampleExpectation};
use super::function_signature_help;
use crate::unit_conversion::{self, Category, UnitDefinition};

pub(crate) mod insertion;
mod functions;
mod topics;
mod units;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ReferenceId {
    Call(&'static str),
    Topic(&'static str),
}

impl ReferenceId {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Call(name) | Self::Topic(name) => name,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceCategory {
    Mathematics,
    Statistics,
    Vectors,
    ControlFlow,
    ProgrammerExact,
    Units,
    CalendarTime,
    Charts,
}

impl ReferenceCategory {
    pub(crate) const ALL: [Self; 8] = [
        Self::Mathematics,
        Self::Statistics,
        Self::Vectors,
        Self::ControlFlow,
        Self::ProgrammerExact,
        Self::Units,
        Self::CalendarTime,
        Self::Charts,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Mathematics => "Mathematics",
            Self::Statistics => "Statistics",
            Self::Vectors => "Vectors",
            Self::ControlFlow => "Control Flow / Functions",
            Self::ProgrammerExact => "Programmer / Exact Numbers",
            Self::Units => "Units",
            Self::CalendarTime => "Calendar / Time",
            Self::Charts => "Chart Data / Usage",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReferenceEntry {
    pub(crate) id: ReferenceId,
    pub(crate) category: ReferenceCategory,
    spelling: Option<&'static str>,
    pub(crate) aliases: &'static [&'static str],
    pub(crate) concepts: &'static [&'static str],
    pub(crate) summary: &'static str,
    pub(crate) parameters: &'static str,
    pub(crate) returns: &'static str,
    pub(crate) limits: &'static str,
    pub(crate) example: CalculationExample,
    pub(crate) variants: &'static [CalculationExample],
    pub(crate) related: &'static [ReferenceId],
    unit_category: Option<Category>,
}

impl ReferenceEntry {
    pub(crate) fn signature(&self) -> &'static str {
        match self.id {
            ReferenceId::Call(name) => function_signature_help(name).unwrap_or(name),
            ReferenceId::Topic(name) => self.spelling.unwrap_or(name),
        }
    }

    pub(crate) fn unit_definitions(&self) -> impl Iterator<Item = &'static UnitDefinition> {
        self.unit_category.into_iter().flat_map(|category| {
            unit_conversion::catalog()
                .iter()
                .filter(move |unit| unit.category == category)
        })
    }

    fn rank(&self, query: &str, normalized: &str, unit_category: Option<Category>) -> Option<u8> {
        let name = self.id.name();
        if normalized.is_empty() || query == name {
            return Some(0);
        }
        if normalized == name.to_lowercase() {
            return Some(1);
        }
        if self.aliases.contains(&query)
            || self
                .unit_category
                .is_some_and(|category| Some(category) == unit_category)
        {
            return Some(2);
        }
        if self
            .aliases
            .iter()
            .chain(self.concepts)
            .any(|text| text.to_lowercase() == normalized)
        {
            return Some(3);
        }
        if name.to_lowercase().starts_with(normalized)
            || self
                .aliases
                .iter()
                .any(|alias| alias.to_lowercase().starts_with(normalized))
        {
            return Some(4);
        }
        self.search_fields()
            .any(|text| text.to_lowercase().contains(normalized))
            .then_some(5)
    }

    fn search_fields(&self) -> impl Iterator<Item = &'static str> {
        [
            self.id.name(),
            self.category.label(),
            self.signature(),
            self.summary,
            self.parameters,
            self.returns,
            self.limits,
            self.example.source,
        ]
        .into_iter()
        .chain(self.aliases.iter().chain(self.concepts).copied())
        .chain(
            self.variants
                .iter()
                .flat_map(|example| [example.title, example.source]),
        )
        .chain(self.unit_definitions().flat_map(|unit| {
            std::iter::once(unit.symbol)
                .chain(unit.aliases.iter().copied())
                .chain(unit.case_sensitive_aliases.iter().copied())
        }))
    }
}

pub(crate) fn entries() -> impl Iterator<Item = &'static ReferenceEntry> {
    VECTORS
        .iter()
        .chain(functions::FUNCTIONS)
        .chain(topics::TOPICS)
        .chain(units::UNITS)
}

pub(crate) fn entry(id: ReferenceId) -> Option<&'static ReferenceEntry> {
    entries().find(|entry| entry.id == id)
}

pub(crate) fn search(
    query: &str,
    category: Option<ReferenceCategory>,
) -> Vec<&'static ReferenceEntry> {
    let query = query.trim();
    let normalized = query.to_lowercase();
    let unit_category = unit_conversion::unit_by_alias(query).map(|unit| unit.category());
    let mut matches: Vec<_> = entries()
        .filter(|entry| category.is_none_or(|category| entry.category == category))
        .filter_map(|entry| Some((entry.rank(query, &normalized, unit_category)?, entry)))
        .collect();
    matches.sort_by_key(|(rank, entry)| (*rank, entry.id.name()));
    matches.into_iter().map(|(_, entry)| entry).collect()
}

const VECTOR: &str = "Vector of 0–1,024 finite ordinary dimensionless scalars.";
const VECTOR_RESULT: &str = "New vector of ordinary scalars.";
const SCALAR: &str = "Ordinary dimensionless scalar.";
const INDEXED: &str =
    "Vector and zero-based integral ordinary scalar index, 0 ≤ index < len(vector).";
const NONEMPTY: &str = "An empty vector reports an error.";
const CAP: &str = "The new vector is limited to 1,024 components.";
const WINDOW: &str = "Vector and positive integral ordinary scalar window width. No default.";
const TRAILING: &str =
    "Uses complete trailing windows. Widths greater than the input length give [].";
const CALLBACK: &str = "Vector, bare name of a previously declared local formula, and 0–15 extra arguments. The formula receives the current scalar element, followed by the extras.";
const BOOLEAN_CALLBACK: &str = "Formula must return Boolean. Name and arity are checked for empty input. Formula errors or exhausted work stop the whole call.";
const FINITE_RESULT: &str =
    "Empty input gives []. Nonfinite intermediate arithmetic reports an error.";

macro_rules! vector_call {
    ($name:literal, $summary:expr, $params:expr, $returns:expr, $limits:expr,
        $example:literal => $kind:ident($expected:expr), [$($concept:literal),*], [$($related:literal),+]) => {
        ReferenceEntry {
            id: ReferenceId::Call($name),
            category: ReferenceCategory::Vectors,
            spelling: None,
            aliases: &[],
            concepts: &[$($concept),*],
            summary: $summary,
            parameters: $params,
            returns: $returns,
            limits: $limits,
            example: CalculationExample::new(
                $name, $example, $summary, ExampleExpectation::$kind($expected),
            ),
            variants: &[],
            related: &[$(ReferenceId::Call($related)),+],
            unit_category: None,
        }
    };
}

const VECTORS: &[ReferenceEntry] = &[
    vector_call!("append", "Return a copy with one scalar appended.", "Vector and a finite ordinary dimensionless scalar value.", VECTOR_RESULT, CAP,
        "append([10, 20], 30)" => Vector(&[10.0, 20.0, 30.0]), ["add item", "push"], ["insert", "concat"]),
    vector_call!("concat", "Join the two vectors in input order.", "Two vectors of finite ordinary dimensionless scalars.", VECTOR_RESULT, CAP,
        "concat([10, 20], [30, 40])" => Vector(&[10.0, 20.0, 30.0, 40.0]), ["join", "combine vectors"], ["append", "slice"]),
    vector_call!("insert", "Insert a scalar before the given position.", "Vector, ordinary scalar index (integer from 0 through len(vector)), and finite ordinary dimensionless scalar value.", VECTOR_RESULT, CAP,
        "insert([10, 30], 1, 20)" => Vector(&[10.0, 20.0, 30.0]), ["insert element"], ["append", "set_at", "remove_at"]),
    vector_call!("set_at", "Replace the scalar at an existing position in a copy.", "Vector, ordinary scalar index (integer from 0 up to but excluding len(vector)), and finite ordinary dimensionless scalar value.", VECTOR_RESULT, "An absent index reports an error.",
        "set_at([10, 20], 0, 30)" => Vector(&[30.0, 20.0]), ["replace element", "update index"], ["insert", "get"]),
    vector_call!("remove_at", "Return a shorter vector with the indexed element removed.", INDEXED, VECTOR_RESULT, "An absent index reports an error.",
        "remove_at([10, 20, 30], 1)" => Vector(&[10.0, 30.0]), ["delete element by index"], ["get", "pop"]),
    vector_call!("pop", "Remove the last element and return the shortened vector. For the removed scalar, read it separately with last.", VECTOR, VECTOR_RESULT, NONEMPTY,
        "pop([10, 20, 30])" => Vector(&[10.0, 20.0]), ["remove last element", "drop final item", "shorten vector"], ["last", "remove_at"]),
    vector_call!("get", "Read the scalar at an existing position.", INDEXED, SCALAR, "An absent index reports an error.",
        "get([10, 20, 30], 1)" => Scalar(20.0), ["read element at index"], ["first", "last", "set_at"]),
    vector_call!("first", "Read the first scalar.", VECTOR, SCALAR, NONEMPTY,
        "first([10, 20])" => Scalar(10.0), ["leading element"], ["get", "last"]),
    vector_call!("last", "Read the last scalar while leaving the vector intact.", VECTOR, SCALAR, NONEMPTY,
        "last([10, 20])" => Scalar(20.0), ["final element"], ["get", "first", "pop"]),
    vector_call!("len", "Count the components in a vector.", VECTOR, "Nonnegative integral ordinary scalar.", "The result ranges from 0 through 1,024.",
        "len([10, 20])" => Scalar(2.0), ["length", "size", "count elements"], ["slice", "count_if"]),
    vector_call!("slice", "Read a subvector from the inclusive start through the exclusive end.", "Vector and two zero-based integral ordinary scalars, 0 ≤ start ≤ end ≤ len(vector).", VECTOR_RESULT, "Equal endpoints return []. Invalid or reversed bounds report an error.",
        "slice([10, 20, 30, 40], 1, 3)" => Vector(&[20.0, 30.0]), ["subvector", "subset"], ["len", "get"]),
    vector_call!("reverse", "Return components in reverse input order.", VECTOR, VECTOR_RESULT, "Empty input gives [].",
        "reverse([10, 20, 30])" => Vector(&[30.0, 20.0, 10.0]), ["reverse order"], ["sort", "sort_desc"]),
    vector_call!("sort", "Sort components in ascending numeric order.", VECTOR, VECTOR_RESULT, "Stable for ties, including signed zeros. Empty input gives [].",
        "sort([30, 10, 20, 10])" => Vector(&[10.0, 10.0, 20.0, 30.0]), ["lowest to highest", "ascending"], ["sort_desc", "unique"]),
    vector_call!("sort_desc", "Sort components in descending numeric order.", VECTOR, VECTOR_RESULT, "Stable for ties, including signed zeros. Empty input gives [].",
        "sort_desc([10, 30, 20, 10])" => Vector(&[30.0, 20.0, 10.0, 10.0]), ["highest to lowest", "descending"], ["sort", "unique"]),
    vector_call!("unique", "Keep the first occurrence of each component in input order.", VECTOR, VECTOR_RESULT, "Signed zeros count as the same value. Empty input gives [].",
        "unique([20, 10, 20, 30, 10])" => Vector(&[20.0, 10.0, 30.0]), ["distinct", "deduplicate", "remove duplicates"], ["sort", "filter"]),
    vector_call!("cumsum", "Return the sum from the first component through each position.", VECTOR, VECTOR_RESULT, FINITE_RESULT,
        "cumsum([2, 3, 4])" => Vector(&[2.0, 5.0, 9.0]), ["running total", "prefix sum"], ["cumprod", "rolling_sum", "diff"]),
    vector_call!("cumprod", "Return the product from the first component through each position.", VECTOR, VECTOR_RESULT, FINITE_RESULT,
        "cumprod([2, 3, 4])" => Vector(&[2.0, 6.0, 24.0]), ["running product", "prefix product"], ["cumsum"]),
    vector_call!("diff", "Subtract each previous component from the next.", VECTOR, VECTOR_RESULT, "Zero or one component gives []. Nonfinite differences report an error.",
        "diff([2, 5, 9])" => Vector(&[3.0, 4.0]), ["adjacent difference", "consecutive change"], ["percent_changes", "cumsum"]),
    vector_call!("percent_changes", "Return consecutive (current - previous) / previous ratios, where 0.25 means 25%.", VECTOR, VECTOR_RESULT, "Zero or one component gives []. A previous zero or nonfinite arithmetic reports an error.",
        "percent_changes([4, 5, 10])" => Vector(&[0.25, 1.0]), ["percentage changes", "growth rates"], ["diff"]),
    vector_call!("rolling_sum", "Return the sum in each complete trailing window.", WINDOW, VECTOR_RESULT, "Widths greater than the input length give []. Nonfinite intermediate sums report an error.",
        "rolling_sum([2, 3, 7], 2)" => Vector(&[5.0, 10.0]), ["moving sum", "sliding total"], ["cumsum", "rolling_average"]),
    vector_call!("rolling_average", "Return the mean in each complete trailing window.", WINDOW, VECTOR_RESULT, TRAILING,
        "rolling_average([2, 4, 8], 2)" => Vector(&[3.0, 6.0]), ["moving average", "moving mean", "smooth"], ["rolling_sum", "rolling_min", "rolling_max"]),
    vector_call!("rolling_min", "Return the minimum in each complete trailing window.", WINDOW, VECTOR_RESULT, TRAILING,
        "rolling_min([2, 4, 1], 2)" => Vector(&[2.0, 1.0]), ["moving minimum"], ["rolling_max", "rolling_average"]),
    vector_call!("rolling_max", "Return the maximum in each complete trailing window.", WINDOW, VECTOR_RESULT, TRAILING,
        "rolling_max([4, 2, 7], 2)" => Vector(&[4.0, 7.0]), ["moving maximum"], ["rolling_min", "rolling_average"]),
    vector_call!("map", "Apply a local formula to every component, in order.", CALLBACK, VECTOR_RESULT, "Formula must return a finite ordinary dimensionless scalar. Empty input gives [] after validating name and arity. Formula errors or exhausted work stop the whole call.",
        "fn scale(value, factor) = value * factor\nmap([2, 3], scale, 10)" => Vector(&[20.0, 30.0]), ["transform each element", "apply callback"], ["filter", "find_indices"]),
    vector_call!("filter", "Keep the original components for which the local formula returns true.", CALLBACK, VECTOR_RESULT, BOOLEAN_CALLBACK,
        "fn above(value, limit) = value > limit\nfilter([1, 4, 2, 5], above, 2)" => Vector(&[4.0, 5.0]), ["keep matching values", "predicate"], ["map", "count_if", "find_indices"]),
    vector_call!("count_if", "Count the components for which the local formula returns true.", CALLBACK, "Nonnegative integral ordinary scalar. Empty input gives zero after validating the formula.", BOOLEAN_CALLBACK,
        "fn above(value, limit) = value > limit\ncount_if([1, 4, 2, 5], above, 2)" => Scalar(2.0), ["count matching elements", "predicate"], ["filter", "find_indices", "len"]),
    vector_call!("find_indices", "Return the positions for which the local formula returns true.", CALLBACK, "Vector of ordinary scalars representing ascending zero-based indices.", BOOLEAN_CALLBACK,
        "fn above(value, limit) = value > limit\nfind_indices([1, 4, 2, 5], above, 2)" => Vector(&[1.0, 3.0]), ["find matching positions", "predicate"], ["filter", "count_if", "get"]),
    vector_call!("dot", "Sum the pairwise products of two vectors.", "Two equally sized vectors of finite ordinary dimensionless scalars.", SCALAR, "Two empty vectors give zero. Unequal sizes or nonfinite arithmetic report an error.",
        "dot([1, 2], [3, 4])" => Scalar(11.0), ["dot product", "inner product", "·"], ["cross"]),
    vector_call!("cross", "Compute the three-dimensional cross product.", "Two vectors, each with exactly three finite ordinary dimensionless scalars.", VECTOR_RESULT, "Incompatible sizes or nonfinite arithmetic report an error.",
        "cross([1, 0, 0], [0, 1, 0])" => Vector(&[0.0, 0.0, 1.0]), ["cross product", "vector product", "×"], ["dot"]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calculation_pad::examples::assert_example;
    use crate::calculation_pad::{FUNCTIONS, is_vector_callback};

    #[test]
    fn reference_vector_calls_cover_evaluator_inventory_and_run() {
        use crate::calculation_pad::vector::CollectionOperation;
        let documented: std::collections::HashSet<_> =
            VECTORS.iter().map(|entry| entry.id).collect();
        assert_eq!(documented.len(), VECTORS.len());
        for function in FUNCTIONS {
            let name = function.name;
            if CollectionOperation::from_name(name).is_some()
                || is_vector_callback(name)
                || matches!(name, "dot" | "cross")
            {
                assert!(
                    documented.contains(&ReferenceId::Call(name)),
                    "missing {name}"
                );
            }
        }
        for item in VECTORS {
            let name = item.id.name();
            assert!(
                FUNCTIONS.iter().any(|function| function.name == name),
                "unknown {name}"
            );
            assert!(item.signature().starts_with(&format!("{name}(")));
            assert!(
                !item.parameters.is_empty() && !item.returns.is_empty() && !item.limits.is_empty()
            );
            assert!(item.related.iter().all(|id| entry(*id).is_some()), "{name}");
            assert_example(&item.example);
        }
    }

    #[test]
    fn reference_search_finds_pop_by_name_and_intent() {
        let pop = entry(ReferenceId::Call("pop")).unwrap();
        for query in ["pop", " POP ", "remove last element", "shorten vector"] {
            assert_eq!(search(query, None)[0].id, pop.id, "{query}");
        }
        assert_eq!(
            search("dot product", Some(ReferenceCategory::Vectors))[0].id,
            ReferenceId::Call("dot")
        );
        assert!(search("fictional operation", None).is_empty());
        assert_eq!(
            search("", Some(ReferenceCategory::Vectors)).len(),
            VECTORS.len() + 2
        );
        assert!(pop.summary.contains("shortened vector"));
        assert_eq!(pop.example.result_text(), "[10, 20]");
    }

    #[test]
    fn reference_covers_every_evaluator_call_and_all_examples_execute() {
        use std::collections::HashSet;

        let inventory: HashSet<_> = FUNCTIONS.iter().map(|function| function.name).collect();
        let covered: HashSet<_> = entries()
            .filter_map(|item| match item.id {
                ReferenceId::Call(name) => Some(name),
                ReferenceId::Topic(_) => None,
            })
            .collect();
        assert_eq!(inventory, covered);
        let mut ids = HashSet::new();
        for item in entries() {
            assert!(ids.insert(item.id), "duplicate {:?}", item.id);
            assert!(!item.signature().is_empty(), "{:?}", item.id);
            assert!(
                !item.summary.is_empty()
                    && !item.parameters.is_empty()
                    && !item.returns.is_empty()
                    && !item.limits.is_empty(),
                "{:?}",
                item.id
            );
            assert!(
                item.related
                    .iter()
                    .all(|id| *id != item.id && entry(*id).is_some()),
                "{:?}: {:?}",
                item.id,
                item.related
            );
            assert_example(&item.example);
            for example in item.variants {
                assert_example(example);
            }
        }
        for category in ReferenceCategory::ALL {
            assert!(
                entries().any(|item| item.category == category),
                "empty {category:?}"
            );
        }
        for function in FUNCTIONS {
            let item = entry(ReferenceId::Call(function.name)).unwrap();
            let forms: std::collections::BTreeSet<_> = std::iter::once(&item.example)
                .chain(item.variants)
                .flat_map(|example| function.documented_example_arities(example.source))
                .collect();
            if is_vector_callback(function.name) {
                assert_eq!(function.arities(), (2..=17).collect::<Vec<_>>());
                assert!(
                    !forms.is_empty()
                        && forms.iter().all(|arity| function.arities().contains(arity))
                );
            } else {
                assert_eq!(
                    forms.into_iter().collect::<Vec<_>>(),
                    function.arities(),
                    "overloads for {}",
                    function.name
                );
            }
        }
    }

    #[test]
    fn reference_operator_and_unit_inventories_use_authoritative_catalogs() {
        use crate::calculation_pad::operators::ModuloOperators;
        use exmex::MakeOperators;

        for operator in ModuloOperators::make() {
            let spelling = operator.repr();
            let topic = match spelling {
                "@" => continue,
                "+" | "-" | "*" | "/" | "^" => Some("Scalar arithmetic"),
                "%" => Some("Signed remainder"),
                "PI" | "π" | "E" | "e" | "TAU" | "τ" => Some("Scalar constants"),
                _ => None,
            };
            if let Some(topic) = topic {
                let item = entry(ReferenceId::Topic(topic)).unwrap();
                assert!(
                    item.aliases.contains(&spelling),
                    "missing {topic} spelling {spelling}"
                );
            } else {
                assert!(
                    entry(ReferenceId::Call(spelling)).is_some(),
                    "missing operator {spelling}"
                );
            }
        }
        for &category in Category::ALL {
            let documented: Vec<_> = entries()
                .filter(|item| item.unit_category == Some(category))
                .collect();
            assert_eq!(documented.len(), 1, "missing/duplicate {category:?}");
            for unit in documented[0].unit_definitions() {
                for spelling in std::iter::once(unit.symbol)
                    .chain(unit.aliases.iter().copied())
                    .chain(unit.case_sensitive_aliases.iter().copied())
                {
                    assert!(
                        search(spelling, Some(ReferenceCategory::Units))
                            .iter()
                            .any(|item| item.id == documented[0].id),
                        "{category:?}: {spelling}"
                    );
                }
            }
        }
        for (term, id) in [
            ("min", ReferenceId::Call("min")),
            ("PI", ReferenceId::Topic("Scalar constants")),
            ("decimal", ReferenceId::Call("decimal")),
            ("MB", ReferenceId::Topic("Data units")),
            ("Mb", ReferenceId::Topic("Data units")),
            ("MB/s", ReferenceId::Topic("Data rate units")),
            ("convert", ReferenceId::Topic("Physical conversions")),
            ("hexadecimal", ReferenceId::Topic("Exact radix conversion")),
        ] {
            assert_eq!(
                search(term, None).first().map(|item| item.id),
                Some(id),
                "search: {term}"
            );
        }
    }

    #[test]
    fn reference_vector_example_diagnostics_are_current() {
        for (source, diagnostic) in [
            ("pop([])", "vector is empty"),
            ("slice([1, 2], 2, 0)", "slice start must not exceed end"),
            ("percent_changes([0, 1])", "nonzero previous value"),
            ("rolling_sum([1, 2], 0)", "positive integral scalar"),
        ] {
            assert_example(&CalculationExample::new(
                source,
                source,
                "Invalid input",
                ExampleExpectation::ErrorContains(diagnostic),
            ));
        }
    }
}
