//! Bounded immutable sensitivity cases and one coalescing analysis worker.
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use super::document::{CalculationDocument, CalculationValue, LineOutcome, script::AggregateWork};
use super::input::{InputConfig, SampleNumber, replace_literal, sample_number, sample_text};
use super::{DocumentId, DocumentIncarnation, DocumentOptions, LineId};

pub(crate) const MAX_CASES: usize = 1000;
pub(crate) const MAX_OUTPUTS: usize = 8;
pub(crate) const MAX_AGGREGATE_WORK: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "snake_case")]
pub(crate) enum Sampling {
    Step(String),
    Count(usize),
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SensitivityDefinition {
    pub input: LineId,
    pub start: String,
    pub end: String,
    pub sampling: Sampling,
    pub outputs: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SensitivityBasis {
    pub document: DocumentId,
    pub incarnation: DocumentIncarnation,
    pub revision: u64,
    pub options: DocumentOptions,
    pub definition: SensitivityDefinition,
    pub input_name: String,
    pub generation: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct SensitivityRow {
    pub input: String,
    pub intended: SampleNumber,
    pub cells: Vec<Result<CalculationValue, String>>,
}
#[derive(Clone, Debug)]
pub(crate) struct SensitivityTable {
    pub basis: SensitivityBasis,
    pub input_kind: super::input::InputKind,
    pub rows: Vec<SensitivityRow>,
    pub work_units: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SensitivityStatus {
    Idle,
    Pending { completed: usize, total: usize },
    Ready,
    Outdated,
    Canceled,
    Failed(String),
}

#[derive(Clone, Copy)]
pub(crate) enum TableExport { Table, Csv, Markdown }

pub(super) fn output_text(value: &CalculationValue) -> String {
    match value {
        CalculationValue::Scalar(value) | CalculationValue::ApproximateScalar(value) => value.to_string(),
        CalculationValue::PhysicalQuantity { value, unit, .. } => format!("{value} {}", unit.symbol()),
        CalculationValue::DimensionalQuantity(value) => value.display_parts().map(|(number, unit)| format!("{number} {unit}")).unwrap_or_else(|| "unavailable".into()),
        CalculationValue::Integer(value) => value.formatted_decimal(),
        CalculationValue::ExactBase { value, .. } => value.to_string(),
        _ => value.canonical_result().unwrap_or_else(|| "unavailable".into()),
    }
}

pub(super) fn intended_text(sample: &SampleNumber) -> String {
    let SampleNumber::Exact(number) = sample;
    if number.is_integer() { return number.to_integer().to_string(); }
    if let Ok(exact) = super::exact::ExactNumber::fraction(number.numer().clone(), number.denom().clone(), &mut super::document::script::WorkBudget::default()) {
        let text = exact.decimal_display(None);
        if !text.starts_with('≈') { return text; }
    }
    format!("{}/{}", number.numer(), number.denom())
}

pub(crate) fn export_table(table: &SensitivityTable, format: TableExport) -> String {
    let mut rows = Vec::with_capacity(table.rows.len() + 1);
    rows.push(table_header(table));
    rows.extend(table.rows.iter().map(table_row));
    match format {
        TableExport::Table | TableExport::Csv => {
            let separator = if matches!(format, TableExport::Csv) { ',' } else { '\t' };
            rows.iter().map(|row| row.iter().map(|cell| {
                if cell.contains([separator, '"', '\n', '\r']) { format!("\"{}\"", cell.replace('"', "\"\"")) } else { cell.clone() }
            }).collect::<Vec<_>>().join(&separator.to_string())).collect::<Vec<_>>().join("\n")
        }
        TableExport::Markdown => {
            let escape = |text: &str| text.replace('\\', "\\\\").replace('|', "\\|").replace("\r\n", "<br>").replace(['\r', '\n'], "<br>");
            let mut result = Vec::with_capacity(rows.len() + 1);
            for (index, row) in rows.iter().enumerate() {
                result.push(format!("| {} |", row.iter().map(|cell| escape(cell)).collect::<Vec<_>>().join(" | ")));
                if index == 0 { result.push(format!("| {} |", vec!["---"; row.len()].join(" | "))); }
            }
            result.join("\n")
        }
    }
}

pub(crate) fn table_header(table: &SensitivityTable) -> Vec<String> {
    let input = match &table.input_kind {
        super::input::InputKind::Quantity { unit } => format!("{} ({unit})", table.basis.input_name),
        _ => table.basis.input_name.clone(),
    };
    std::iter::once(input).chain(table.basis.definition.outputs.iter().cloned()).collect()
}
pub(crate) fn table_row(row: &SensitivityRow) -> Vec<String> {
    std::iter::once(intended_text(&row.intended)).chain(row.cells.iter().map(|cell| match cell { Ok(value) => output_text(value), Err(error) => format!("Error: {error}") })).collect()
}

#[derive(Clone)]
enum Grid {
    Exact {
        start: BigRational,
        delta: BigRational,
        count: usize,
    },
}
impl Grid {
    fn count(&self) -> usize {
        match self {
            Self::Exact { count, .. } => *count,
        }
    }
    fn at(&self, index: usize) -> SampleNumber {
        match self {
            Self::Exact { start, delta, .. } => {
                SampleNumber::Exact(start + delta * BigRational::from_integer(BigInt::from(index)))
            }
        }
    }
}

fn grid(
    kind: &super::input::InputKind,
    definition: &SensitivityDefinition,
) -> Result<Grid, String> {
    let start = sample_number(kind, &definition.start, false)?;
    let end = sample_number(kind, &definition.end, false)?;
    match (start, end, &definition.sampling) {
        (SampleNumber::Exact(start), SampleNumber::Exact(end), Sampling::Count(count)) => {
            if *count == 0 || *count > MAX_CASES {
                return Err("Sample count must be between 1 and 1000.".into());
            }
            let count = if start == end { 1 } else { *count };
            if count == 0 || count > MAX_CASES || (start != end && count < 2) {
                return Err(
                    "Sample count must be between 2 and 1000 for distinct endpoints.".into(),
                );
            }
            let delta = if count == 1 {
                BigRational::zero()
            } else {
                (&end - &start) / BigRational::from_integer(BigInt::from(count - 1))
            };
            if matches!(
                kind,
                super::input::InputKind::Integer(_) | super::input::InputKind::ExactInteger
            ) && !delta.is_integer()
            {
                return Err("Integer samples must all be integral.".into());
            }
            Ok(Grid::Exact {
                start,
                delta,
                count,
            })
        }
        (SampleNumber::Exact(start), SampleNumber::Exact(end), Sampling::Step(step)) => {
            let SampleNumber::Exact(step) = sample_number(kind, step, true)?;
            if step.is_zero()
                || (start < end && step.is_negative())
                || (start > end && step.is_positive())
            {
                return Err("Step must be nonzero and point toward the end.".into());
            }
            let count = ((&end - &start) / &step)
                .to_integer()
                .to_usize()
                .and_then(|value| value.checked_add(1))
                .ok_or("Sensitivity exceeds the case limit.")?;
            if count > MAX_CASES {
                return Err("Sensitivity exceeds the 1000-case limit.".into());
            }
            Ok(Grid::Exact {
                start,
                delta: step,
                count,
            })
        }
    }
}

pub(super) struct Request {
    pub basis: SensitivityBasis,
    pub source: String,
    pub descriptor: super::document::LiteralInput,
    grid: Grid,
    pub progress: Arc<AtomicUsize>,
}
impl Request {
    pub fn new(
        basis: SensitivityBasis,
        source: String,
        descriptor: super::document::LiteralInput,
    ) -> Result<Self, String> {
        if basis.definition.outputs.is_empty() || basis.definition.outputs.len() > MAX_OUTPUTS {
            return Err("Choose between one and eight outputs.".into());
        }
        let mut names = std::collections::HashSet::new();
        if basis.definition.outputs.iter().any(|name| {
            !names.insert(name)
                || super::document::watch_name(&format!("{{{name}}}")).as_deref() != Some(name)
        }) {
            return Err("Output names must be valid and distinct.".into());
        }
        let grid = grid(&descriptor.kind, &basis.definition)?;
        Ok(Self {
            basis,
            source,
            descriptor,
            grid,
            progress: Arc::new(AtomicUsize::new(0)),
        })
    }
    pub fn count(&self) -> usize {
        self.grid.count()
    }
}

fn supported(value: &CalculationValue) -> bool {
    matches!(
        value,
        CalculationValue::Scalar(_)
            | CalculationValue::ApproximateScalar(_)
            | CalculationValue::Exact(_)
            | CalculationValue::Integer(_)
            | CalculationValue::ExactBase { .. }
            | CalculationValue::PhysicalQuantity { .. }
            | CalculationValue::DimensionalQuantity(_)
    )
}
fn is_quantity(value: &CalculationValue) -> bool {
    matches!(
        value,
        CalculationValue::PhysicalQuantity { .. } | CalculationValue::DimensionalQuantity(_)
    )
}

fn run_cases(
    request: &Request,
    cancellation: Arc<AtomicBool>,
    wake: &dyn Fn(),
) -> Result<SensitivityTable, String> {
    let aggregate = Arc::new(AggregateWork::new(MAX_AGGREGATE_WORK));
    let mut rows = Vec::with_capacity(request.count());
    let mut quantity_basis: Vec<Option<CalculationValue>> =
        vec![None; request.basis.definition.outputs.len()];
    for index in 0..request.count() {
        if cancellation.load(Ordering::Relaxed) {
            return Err("Sensitivity calculation canceled.".into());
        }
        let sampling_bytes = match &request.basis.definition.sampling { Sampling::Step(value) => value.len(), Sampling::Count(_) => 0 };
        let charge = 1
            + request.source.len() / 64
            + super::limits::physical_rows(&request.source)
            + (request.basis.definition.start.len() + request.basis.definition.end.len() + sampling_bytes) / 32;
        aggregate
            .charge(charge)
            .map_err(|_| "Sensitivity exceeds the aggregate work limit.")?;
        let sample = request.grid.at(index);
        let rendered = sample_text(&request.descriptor.kind, &sample);
        let display = rendered
            .clone()
            .unwrap_or_else(|_| format!("sample {}", index + 1));
        let projected = rendered.and_then(|value| {
            let config = InputConfig { anchor: Some(request.descriptor.kind.anchor()), ..Default::default() };
            replace_literal(
                &request.source,
                &request.descriptor,
                &config,
                &value,
            )
        });
        let cells = match projected {
            Err(error) => vec![Err(error); request.basis.definition.outputs.len()],
            Ok(source) => {
                let document =
                    CalculationDocument::with_profile(source, request.basis.options.profile);
                let evaluation = document
                    .evaluate_for_analysis(Arc::clone(&cancellation), Arc::clone(&aggregate));
                if cancellation.load(Ordering::Relaxed) {
                    return Err("Sensitivity calculation canceled.".into());
                }
                if aggregate.exhausted() {
                    return Err("Sensitivity exceeds the aggregate work limit.".into());
                }
                if let Some(diagnostic) = evaluation.diagnostic {
                    return Err(diagnostic.to_string());
                }
                request
                    .basis
                    .definition
                    .outputs
                    .iter()
                    .enumerate()
                    .map(|(column, name)| {
                        let binding = evaluation
                            .final_bindings
                            .get(name)
                            .ok_or_else(|| format!("Output '{name}' is unavailable."))?;
                        match &binding.outcome {
                            LineOutcome::Value(value) if supported(value) => {
                                if let Some(previous) = &quantity_basis[column] {
                                    if is_quantity(previous) != is_quantity(value)
                                        || (is_quantity(value)
                                            && super::quantity::compare_ordering(previous, value)
                                                .is_err())
                                    {
                                        return Err(
                                            "Output dimensions changed between cases.".into()
                                        );
                                    }
                                } else {
                                    quantity_basis[column] = Some(value.clone());
                                }
                                Ok(value.clone())
                            }
                            LineOutcome::Value(_) => {
                                Err("This output type is not supported by sensitivity tables."
                                    .into())
                            }
                            LineOutcome::Error(error) => Err(error.to_string()),
                            _ => Err(format!("Output '{name}' has no value.")),
                        }
                    })
                    .collect()
            }
        };
        rows.push(SensitivityRow {
            input: display,
            intended: sample,
            cells,
        });
        request.progress.store(index + 1, Ordering::Relaxed);
        if (index + 1) % 10 == 0 || index + 1 == request.count() {
            wake();
        }
    }
    Ok(SensitivityTable {
        basis: request.basis.clone(),
        input_kind: request.descriptor.kind.clone(),
        rows,
        work_units: aggregate.used(),
    })
}

struct Completion {
    basis: SensitivityBasis,
    result: Result<SensitivityTable, String>,
}
pub(super) struct SensitivityWorker {
    inner: super::worker::CoalescingWorker<Request, Completion>,
}
impl SensitivityWorker {
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, String> {
        let task_wake = Arc::clone(&wake);
        let task = Arc::new(move |request: Request, cancellation: Arc<AtomicBool>| {
            let basis = request.basis.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_cases(&request, cancellation, &*task_wake)
            }))
            .unwrap_or_else(|_| Err("Sensitivity worker failed.".into()));
            Completion { basis, result }
        });
        Ok(Self {
            inner: super::worker::CoalescingWorker::start(
                "calculation-pad-sensitivity",
                wake,
                task,
            )?,
        })
    }
    pub fn submit(&self, request: Request) {
        self.inner.submit(request);
    }
    pub fn retire(&self) {
        self.inner.retire();
    }
    pub fn take(&self) -> Option<(SensitivityBasis, Result<SensitivityTable, String>)> {
        self.inner
            .take()
            .map(|completion| (completion.basis, completion.result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_preserve_intended_samples_and_escape_cells() {
        let request = make_request("x = 1\ny = x", "0", "1", Sampling::Count(2), &["y"]);
        let mut table = run_cases(&request, Arc::new(AtomicBool::new(false)), &|| {}).unwrap();
        table.rows[0].intended = SampleNumber::Exact(BigRational::new(1.into(), 3.into()));
        table.rows[0].cells[0] = Err("bad, \"quoted\" | line\nnext".into());
        let csv = export_table(&table, TableExport::Csv);
        assert!(csv.contains("1/3,\"Error: bad, \"\"quoted\"\" | line\nnext\""), "{csv}");
        let markdown = export_table(&table, TableExport::Markdown);
        assert!(markdown.contains("1/3") && markdown.contains("\\|") && markdown.contains("<br>"), "{markdown}");
        assert!(export_table(&table, TableExport::Table).contains('\t'));
    }

    #[test]
    fn sensitivity_preserves_radix_within_scalar_limit_and_reports_overflow() {
        let request = make_request("x = 0x5\ny = x", "9007199254740992", "9007199254740993", Sampling::Count(2), &["y"]);
        let config = InputConfig { anchor: Some(request.descriptor.kind.anchor()), ..Default::default() };
        let projected = replace_literal(&request.source, &request.descriptor, &config, "9007199254740992").unwrap();
        assert!(projected.starts_with("x = 0x20000000000000\n"));
        let table = run_cases(&request, Arc::new(AtomicBool::new(false)), &|| {}).unwrap();
        assert_eq!(table.rows.len(), 2);
        assert!(table.rows[0].cells[0].is_ok());
        assert!(table.rows[1].cells[0].is_err());
        let csv = export_table(&table, TableExport::Csv);
        assert!(csv.contains("9007199254740992,9007199254740992"), "{csv}");
        assert!(csv.contains("9007199254740993,Error:"), "{csv}");
    }

    fn make_request(
        source: &str,
        start: &str,
        end: &str,
        sampling: Sampling,
        outputs: &[&str],
    ) -> Request {
        let descriptor = super::super::document::literal_inputs(source).remove(0);
        let definition = SensitivityDefinition {
            input: LineId(1),
            start: start.into(),
            end: end.into(),
            sampling,
            outputs: outputs.iter().map(|name| (*name).into()).collect(),
        };
        let basis = SensitivityBasis {
            document: DocumentId::fresh(),
            incarnation: DocumentIncarnation(DocumentId::fresh()),
            revision: 0,
            options: DocumentOptions::default(),
            input_name: descriptor.name.clone(),
            definition,
            generation: 1,
        };
        Request::new(basis, source.into(), descriptor).unwrap()
    }

    #[test]
    fn xp_fixture_and_authored_decimal_endpoint_use_exact_grid() {
        let source = "ore = 1000\nxp = ore * 56.2\nremaining = 1000000 - xp";
        let request = make_request(
            source,
            "1000",
            "5000",
            Sampling::Step("1000".into()),
            &["xp", "remaining"],
        );
        let table = run_cases(&request, Arc::new(AtomicBool::new(false)), &|| {}).unwrap();
        assert_eq!(table.rows.len(), 5);
        assert!(
            matches!(table.rows[0].cells[0], Ok(CalculationValue::Scalar(v)) if (v - 56200.0).abs() < 1e-6)
        );
        assert!(
            matches!(table.rows[4].cells[1], Ok(CalculationValue::Scalar(v)) if (v - 719000.0).abs() < 1e-6)
        );
        let request = make_request(
            "x = 0\ny = x",
            "0",
            "0.3",
            Sampling::Step("0.1".into()),
            &["y"],
        );
        let table = run_cases(&request, Arc::new(AtomicBool::new(false)), &|| {}).unwrap();
        assert_eq!(table.rows.len(), 4);
        assert_eq!(table.rows[3].input, "0.3");
    }

    #[test]
    fn exact_unsigned_descending_units_and_diagnostic_rows() {
        let request = make_request(
            "x = u128(9007199254740995)\ny = x",
            "9007199254740995",
            "9007199254740993",
            Sampling::Step("-1".into()),
            &["y"],
        );
        let table = run_cases(&request, Arc::new(AtomicBool::new(false)), &|| {}).unwrap();
        assert_eq!(
            table
                .rows
                .iter()
                .map(|row| row.input.as_str())
                .collect::<Vec<_>>(),
            ["9007199254740995", "9007199254740994", "9007199254740993"]
        );
        assert!(
            table
                .rows
                .iter()
                .all(|row| matches!(&row.cells[0], Ok(CalculationValue::Integer(_))))
        );
        let invalid = SensitivityDefinition {
            sampling: Sampling::Count(3),
            start: "0".into(),
            end: "1".into(),
            ..request.basis.definition.clone()
        };
        assert!(grid(&request.descriptor.kind, &invalid).is_err());
        let request = make_request(
            "distance = 1 m\nresult = distance * 2",
            "1",
            "3",
            Sampling::Count(3),
            &["result", "missing"],
        );
        let table = run_cases(&request, Arc::new(AtomicBool::new(false)), &|| {}).unwrap();
        assert_eq!(table.rows.len(), 3);
        assert!(
            table
                .rows
                .iter()
                .all(|row| row.cells[0].is_ok() && row.cells[1].is_err())
        );
        let request = make_request(
            "x = 1\ny = 1 / x",
            "-1",
            "1",
            Sampling::Step("1".into()),
            &["y"],
        );
        let table = run_cases(&request, Arc::new(AtomicBool::new(false)), &|| {}).unwrap();
        assert_eq!(table.rows.len(), 3);
        assert!(table.rows[1].cells[0].is_err());
        let request = make_request(
            "x = decimal(\"0\")\ny = x",
            "0",
            "1",
            Sampling::Count(4),
            &["y"],
        );
        let table = run_cases(&request, Arc::new(AtomicBool::new(false)), &|| {}).unwrap();
        assert_eq!(table.rows.len(), 4);
        assert!(table.rows[1].cells[0].is_err());
        assert!(
            matches!(&table.rows[1].intended, SampleNumber::Exact(value) if value == &BigRational::new(1.into(), 3.into()))
        );
        assert!(table.rows[3].cells[0].is_ok());
    }

    #[test]
    fn aggregate_budget_and_cancellation_do_not_publish_partial_tables() {
        let source = format!("x = 1\n{}y = x", "# padding\n".repeat(250));
        let request = make_request(&source, "0", "999", Sampling::Count(1000), &["y"]);
        let canceled = Arc::new(AtomicBool::new(true));
        assert!(
            run_cases(&request, canceled, &|| {})
                .unwrap_err()
                .contains("canceled")
        );
        let source = "x = 1\ny = sum(range(1024))";
        let request = make_request(source, "0", "999", Sampling::Count(1000), &["y"]);
        assert!(
            run_cases(&request, Arc::new(AtomicBool::new(false)), &|| {})
                .unwrap_err()
                .contains("aggregate")
        );
    }

    #[test]
    fn vector_component_work_stops_sensitivity_without_a_partial_table() {
        let literal = vec!["1"; 1024].join(",");
        let source = format!("x = 1\nv = [{literal}]\nfor i in range(1024):\n    v = v * 1\ny = sum(v)");
        let request = make_request(&source, "0", "19", Sampling::Count(20), &["y"]);
        assert!(run_cases(&request, Arc::new(AtomicBool::new(false)), &|| {}).unwrap_err().contains("aggregate"));
    }
}
