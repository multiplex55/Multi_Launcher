//! Checked, unsanitized Dashboard editor transactions.
use super::config::{DashboardConfig, GridConfig, SlotConfig, dashboard_config_transaction_guard};
use super::widgets::WidgetRegistry;
use serde::de::{Deserialize, Deserializer, Error, MapAccess, Visitor};
use serde_json::value::{RawValue, to_raw_value};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

type RawObject = BTreeMap<String, Box<RawValue>>;

#[derive(Clone, Debug)]
enum Fields {
    Object(RawObject),
    Array(Vec<Box<RawValue>>),
}

#[derive(Clone, Debug)]
struct RawRecord {
    original: Box<RawValue>,
    fields: Fields,
    changed: bool,
}

impl RawRecord {
    fn parse(raw: Box<RawValue>) -> anyhow::Result<Self> {
        let fields = if raw.get().trim_start().starts_with('[') {
            Fields::Array(serde_json::from_str(raw.get())?)
        } else {
            Fields::Object(unique_object(raw.get().as_bytes())?)
        };
        Ok(Self {
            original: raw,
            fields,
            changed: false,
        })
    }

    fn get(&self, key: &str, index: usize) -> Option<&RawValue> {
        match &self.fields {
            Fields::Object(values) => values.get(key).map(AsRef::as_ref),
            Fields::Array(values) => values.get(index).map(AsRef::as_ref),
        }
    }

    fn set(
        &mut self,
        key: &str,
        index: usize,
        value: Option<Box<RawValue>>,
        defaults: &[Box<RawValue>],
    ) {
        match &mut self.fields {
            Fields::Object(values) => {
                if let Some(value) = value {
                    values.insert(key.into(), value);
                } else {
                    values.remove(key);
                }
            }
            Fields::Array(values) => {
                if value.is_none() && index >= values.len() {
                    return;
                }
                while values.len() <= index {
                    values.push(defaults[values.len()].clone());
                }
                values[index] = value.unwrap_or_else(|| defaults[index].clone());
            }
        }
        self.changed = true;
    }

    fn raw(&self) -> anyhow::Result<Box<RawValue>> {
        if !self.changed {
            return Ok(self.original.clone());
        }
        Ok(match &self.fields {
            Fields::Object(values) => to_raw_value(values)?,
            Fields::Array(values) => to_raw_value(values)?,
        })
    }
}

fn values(items: &[serde_json::Value]) -> Vec<Box<RawValue>> {
    items
        .iter()
        .map(|value| to_raw_value(value).expect("JSON values serialize"))
        .collect()
}

fn slot_defaults() -> Vec<Box<RawValue>> {
    values(&[
        serde_json::json!(null),
        serde_json::json!(""),
        serde_json::json!(0),
        serde_json::json!(0),
        serde_json::json!(1),
        serde_json::json!(1),
        serde_json::json!(null),
        serde_json::json!("scroll"),
    ])
}

fn unique_object(bytes: &[u8]) -> anyhow::Result<RawObject> {
    struct Unique(RawObject);
    impl<'de> Deserialize<'de> for Unique {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct ObjectVisitor;
            impl<'de> Visitor<'de> for ObjectVisitor {
                type Value = Unique;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("an object with unique keys")
                }
                fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Unique, M::Error> {
                    let mut object = RawObject::new();
                    while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                        if object.insert(key, value).is_some() {
                            return Err(M::Error::custom("duplicate dashboard member"));
                        }
                    }
                    Ok(Unique(object))
                }
            }
            deserializer.deserialize_map(ObjectVisitor)
        }
    }
    Ok(serde_json::from_slice::<Unique>(bytes)?.0)
}

fn read(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SlotToken(u64);

#[derive(Clone, Debug)]
pub struct DraftSlot {
    pub token: SlotToken,
    pub slot: SlotConfig,
    raw: RawRecord,
    placement_changed: bool,
}

#[derive(Debug)]
pub struct DashboardDraft {
    path: PathBuf,
    preimage: Option<Vec<u8>>,
    root: RawRecord,
    grid: GridConfig,
    baseline_grid: GridConfig,
    grid_raw: RawRecord,
    grid_changed: bool,
    slots_changed: bool,
    slots: Vec<DraftSlot>,
    next_token: u64,
    committed_unverified: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SaveError {
    Conflict,
    Revoked,
    Invalid(String),
    Store(String),
    CommittedUnverified,
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict => f.write_str("Dashboard changed; reload before saving"),
            Self::Revoked => f.write_str("Dashboard editor is no longer current"),
            Self::Invalid(error) | Self::Store(error) => f.write_str(error),
            Self::CommittedUnverified => f.write_str(
                "Dashboard was written but could not be verified; reload before retrying",
            ),
        }
    }
}
impl std::error::Error for SaveError {}

impl DashboardDraft {
    /// Load the exact source without stock sanitization or any write.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let _permit = crate::common::repair_watch::require_path_read(path)?;
        let _guard = dashboard_config_transaction_guard();
        let preimage = read(path)?;
        let raw = match &preimage {
            None => serde_json::to_vec(&DashboardConfig::default())?,
            Some(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => {
                serde_json::to_vec(&DashboardConfig::default())?
            }
            Some(bytes) => bytes.clone(),
        };
        let config: DashboardConfig = serde_json::from_slice(&raw)?;
        let root = RawRecord::parse(RawValue::from_string(String::from_utf8(raw)?)?)?;
        let grid_raw = RawRecord::parse(match root.get("grid", 1) {
            Some(value) => value.to_owned(),
            None => RawValue::from_string("{}".into())?,
        })?;
        let raw_slots: Vec<Box<RawValue>> = match root.get("slots", 2) {
            Some(value) => serde_json::from_str(value.get())?,
            None => Vec::new(),
        };
        anyhow::ensure!(
            raw_slots.len() == config.slots.len(),
            "Dashboard slot shape changed"
        );
        let mut slots = Vec::with_capacity(raw_slots.len());
        for (index, (raw, slot)) in raw_slots.into_iter().zip(config.slots).enumerate() {
            slots.push(DraftSlot {
                token: SlotToken(index as u64 + 1),
                slot,
                raw: RawRecord::parse(raw)?,
                placement_changed: false,
            });
        }
        Ok(Self {
            path: path.to_owned(),
            preimage,
            root,
            baseline_grid: config.grid.clone(),
            grid: config.grid,
            grid_raw,
            grid_changed: false,
            slots_changed: false,
            next_token: slots.len() as u64 + 1,
            slots,
            committed_unverified: false,
        })
    }

    pub fn grid(&self) -> &GridConfig {
        &self.grid
    }
    pub fn slots(&self) -> &[DraftSlot] {
        &self.slots
    }
    pub fn set_grid(&mut self, grid: GridConfig) {
        self.grid_changed |= self.grid != grid;
        self.grid = grid;
    }

    pub fn set_placement(
        &mut self,
        token: SlotToken,
        row: i32,
        col: i32,
        row_span: u8,
        col_span: u8,
    ) -> anyhow::Result<()> {
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.token == token)
            .ok_or_else(|| anyhow::anyhow!("Dashboard slot changed"))?;
        anyhow::ensure!(
            row_span > 0 && col_span > 0,
            "Dashboard spans must be positive"
        );
        let defaults = slot_defaults();
        for (key, index, changed, value) in [
            ("row", 2, slot.slot.row != row, to_raw_value(&row)?),
            ("col", 3, slot.slot.col != col, to_raw_value(&col)?),
            (
                "row_span",
                4,
                slot.slot.row_span != row_span,
                to_raw_value(&row_span)?,
            ),
            (
                "col_span",
                5,
                slot.slot.col_span != col_span,
                to_raw_value(&col_span)?,
            ),
        ] {
            if changed {
                slot.raw.set(key, index, Some(value), &defaults);
                self.slots_changed = true;
                slot.placement_changed = true;
            }
        }
        slot.slot.row = row;
        slot.slot.col = col;
        slot.slot.row_span = row_span;
        slot.slot.col_span = col_span;
        Ok(())
    }

    pub fn add(
        &mut self,
        widget: &str,
        row: i32,
        col: i32,
        registry: &WidgetRegistry,
    ) -> anyhow::Result<SlotToken> {
        let settings = registry
            .default_settings(widget)
            .ok_or_else(|| anyhow::anyhow!("Unknown dashboard widget"))?;
        let mut slot = SlotConfig::with_widget(widget, row, col);
        slot.settings = settings;
        let raw = RawRecord::parse(to_raw_value(&slot)?)?;
        let token = SlotToken(self.next_token);
        self.next_token = self
            .next_token
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Dashboard slot limit reached"))?;
        self.slots.push(DraftSlot {
            token,
            slot,
            raw,
            placement_changed: true,
        });
        self.slots_changed = true;
        Ok(token)
    }

    pub fn remove(&mut self, token: SlotToken) -> anyhow::Result<()> {
        let index = self
            .slots
            .iter()
            .position(|slot| slot.token == token)
            .ok_or_else(|| anyhow::anyhow!("Dashboard slot changed"))?;
        self.slots.remove(index);
        self.slots_changed = true;
        Ok(())
    }

    /// Stock Swap moves widget, settings and id, while placement belongs to the cells.
    pub fn swap(&mut self, first: SlotToken, second: SlotToken) -> anyhow::Result<()> {
        let a = self
            .slots
            .iter()
            .position(|slot| slot.token == first)
            .ok_or_else(|| anyhow::anyhow!("Dashboard slot changed"))?;
        let b = self
            .slots
            .iter()
            .position(|slot| slot.token == second)
            .ok_or_else(|| anyhow::anyhow!("Dashboard slot changed"))?;
        if a == b {
            return Ok(());
        }
        let (left, right) = if a < b {
            let (left, right) = self.slots.split_at_mut(b);
            (&mut left[a], &mut right[0])
        } else {
            let (left, right) = self.slots.split_at_mut(a);
            (&mut right[0], &mut left[b])
        };
        let defaults = slot_defaults();
        for (key, index) in [("widget", 1), ("settings", 6), ("id", 0)] {
            let l = left.raw.get(key, index).map(ToOwned::to_owned);
            let r = right.raw.get(key, index).map(ToOwned::to_owned);
            if l.as_deref().map(RawValue::get) == r.as_deref().map(RawValue::get) {
                continue;
            }
            left.raw.set(key, index, r, &defaults);
            right.raw.set(key, index, l, &defaults);
        }
        std::mem::swap(&mut left.slot.widget, &mut right.slot.widget);
        std::mem::swap(&mut left.slot.settings, &mut right.slot.settings);
        std::mem::swap(&mut left.slot.id, &mut right.slot.id);
        std::mem::swap(&mut left.token, &mut right.token);
        self.slots_changed = true;
        Ok(())
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        let rows = usize::from(self.grid.rows);
        let cols = usize::from(self.grid.cols);
        anyhow::ensure!(
            rows > 0 && cols > 0,
            "Dashboard grid must have rows and columns"
        );
        let mut occupied = vec![vec![false; cols]; rows];
        for item in &self.slots {
            let slot = &item.slot;
            anyhow::ensure!(
                slot.row >= 0 && slot.col >= 0,
                "Invalid dashboard slot placement"
            );
            let row = slot.row as usize;
            let col = slot.col as usize;
            anyhow::ensure!(row < rows && col < cols, "Dashboard slot exceeds the grid");
            let row_span = usize::from(slot.row_span.max(1));
            let col_span = usize::from(slot.col_span.max(1));
            let end_row = row
                + if item.placement_changed {
                    row_span
                } else {
                    row_span.min(rows - row)
                };
            let end_col = col
                + if item.placement_changed {
                    col_span
                } else {
                    col_span.min(cols - col)
                };
            anyhow::ensure!(
                end_row <= rows && end_col <= cols,
                "Dashboard slot exceeds the grid"
            );
            for row in &mut occupied[row..end_row] {
                for cell in &mut row[col..end_col] {
                    anyhow::ensure!(!*cell, "Dashboard slots overlap");
                    *cell = true;
                }
            }
        }
        Ok(())
    }

    pub fn save(&mut self, current: impl Fn() -> bool) -> Result<(), SaveError> {
        self.save_inner(current, || true, read)
    }

    /// Claim the registered writer only at the final checked publication edge.
    pub fn save_with_claim(
        &mut self,
        current: impl Fn() -> bool,
        claim: impl Fn() -> bool,
    ) -> Result<(), SaveError> {
        self.save_inner(current, claim, read)
    }

    fn save_inner(
        &mut self,
        current: impl Fn() -> bool,
        claim: impl Fn() -> bool,
        readback: impl FnOnce(&Path) -> std::io::Result<Option<Vec<u8>>>,
    ) -> Result<(), SaveError> {
        if self.committed_unverified {
            return Err(SaveError::CommittedUnverified);
        }
        self.validate()
            .map_err(|error| SaveError::Invalid(error.to_string()))?;
        let _permit = crate::common::repair_watch::require_path_read(&self.path)
            .map_err(|error| SaveError::Store(error.to_string()))?;
        let _guard = dashboard_config_transaction_guard();
        if read(&self.path).map_err(|error| SaveError::Store(error.to_string()))? != self.preimage {
            return Err(SaveError::Conflict);
        }
        if !current() {
            return Err(SaveError::Revoked);
        }
        let mut root = self.root.clone();
        let mut grid = self.grid_raw.clone();
        let root_defaults = values(&[
            serde_json::json!(1),
            serde_json::json!(GridConfig::default()),
            serde_json::json!([]),
        ]);
        let grid_defaults = values(&[serde_json::json!(3), serde_json::json!(3)]);
        if self.grid_changed && self.grid != self.baseline_grid {
            if self.grid.rows != self.baseline_grid.rows {
                grid.set(
                    "rows",
                    0,
                    Some(
                        to_raw_value(&self.grid.rows)
                            .map_err(|error| SaveError::Store(error.to_string()))?,
                    ),
                    &grid_defaults,
                );
            }
            if self.grid.cols != self.baseline_grid.cols {
                grid.set(
                    "cols",
                    1,
                    Some(
                        to_raw_value(&self.grid.cols)
                            .map_err(|error| SaveError::Store(error.to_string()))?,
                    ),
                    &grid_defaults,
                );
            }
            root.set(
                "grid",
                1,
                Some(
                    grid.raw()
                        .map_err(|error| SaveError::Store(error.to_string()))?,
                ),
                &root_defaults,
            );
        }
        if self.slots_changed {
            let slots: Vec<Box<RawValue>> = self
                .slots
                .iter()
                .map(|slot| slot.raw.raw())
                .collect::<anyhow::Result<_>>()
                .map_err(|error| SaveError::Store(error.to_string()))?;
            root.set(
                "slots",
                2,
                Some(to_raw_value(&slots).map_err(|error| SaveError::Store(error.to_string()))?),
                &root_defaults,
            );
        }
        let candidate = root
            .raw()
            .map_err(|error| SaveError::Store(error.to_string()))?
            .get()
            .as_bytes()
            .to_vec();
        let exact = || read(&self.path).ok().as_ref() == Some(&self.preimage) && current();
        let claimed = std::cell::Cell::new(false);
        let final_check = || {
            exact()
                && (claimed.get() || {
                    let accepted = claim();
                    claimed.set(accepted);
                    accepted
                })
        };
        let result: anyhow::Result<()> = if self.preimage.is_some() {
            crate::common::atomic_file::save_atomic_if(&self.path, &candidate, final_check)
        } else {
            (|| {
                let parent = self
                    .path
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                anyhow::ensure!(exact(), "Dashboard save superseded");
                std::fs::create_dir_all(parent)?;
                anyhow::ensure!(exact(), "Dashboard save superseded");
                let mut staged = tempfile::NamedTempFile::new_in(parent)?;
                staged.write_all(&candidate)?;
                staged.flush()?;
                staged.as_file().sync_all()?;
                anyhow::ensure!(final_check(), "Dashboard save superseded");
                staged
                    .persist_noclobber(&self.path)
                    .map_err(|error| error.error)?;
                Ok(())
            })()
        };
        if let Err(error) = result {
            if !current() {
                return Err(SaveError::Revoked);
            }
            if read(&self.path).ok().as_ref() != Some(&self.preimage) {
                return Err(SaveError::Conflict);
            }
            return Err(SaveError::Store(error.to_string()));
        }
        self.committed_unverified = true;
        if readback(&self.path).ok().flatten().as_deref() != Some(candidate.as_slice()) {
            return Err(SaveError::CommittedUnverified);
        }
        self.preimage = Some(candidate);
        self.root = root;
        self.grid_raw = grid;
        self.baseline_grid = self.grid.clone();
        self.grid_changed = false;
        self.slots_changed = false;
        for slot in &mut self.slots {
            slot.raw =
                RawRecord::parse(slot.raw.raw().map_err(|_| SaveError::CommittedUnverified)?)
                    .map_err(|_| SaveError::CommittedUnverified)?;
            slot.placement_changed = false;
        }
        self.committed_unverified = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{DashboardDraft, SaveError};
    use crate::dashboard::widgets::WidgetRegistry;

    #[test]
    fn raw_slots_keep_unknown_fields_and_swap_exact_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dashboard.json");
        let original = br#"{"unknown":{"duplicate":1,"duplicate":2},"grid":{"rows":2,"cols":2,"extra":[1,2]},"slots":[{"id":"same","widget":"unknown_widget","row":0,"col":0,"settings":{"x":1,"x":2},"cell":"a"},{"id":"same","widget":"timers","row":1,"col":0,"settings":{"count":7},"cell":"b"}]}"#;
        std::fs::write(&path, original).unwrap();
        let mut draft = DashboardDraft::load(&path).unwrap();
        let a = draft.slots()[0].token;
        let b = draft.slots()[1].token;
        assert_ne!(a, b);
        draft.swap(a, b).unwrap();
        assert_eq!(draft.slots()[0].slot.widget, "timers");
        assert_eq!(draft.slots()[0].token, b);
        draft.save(|| true).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains(r#"{"duplicate":1,"duplicate":2}"#));
        assert!(saved.contains(r#"{"x":1,"x":2}"#));
        let value: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(value["slots"][0]["cell"], "a");
        assert_eq!(value["slots"][1]["cell"], "b");
        assert_eq!(value["slots"][1]["widget"], "unknown_widget");
        assert_eq!(value["grid"]["extra"], serde_json::json!([1, 2]));
        draft.remove(b).unwrap();
        assert!(draft.remove(b).is_err());
        assert_eq!(draft.slots()[0].token, a);
    }

    #[test]
    fn checked_save_refuses_conflicts_and_preserves_corrupt_or_competing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dashboard.json");
        std::fs::write(&path, b"{bad").unwrap();
        assert!(DashboardDraft::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{bad");
        std::fs::remove_file(&path).unwrap();
        let mut missing = DashboardDraft::load(&path).unwrap();
        assert!(!path.exists());
        assert_eq!(missing.save(|| false), Err(SaveError::Revoked));
        std::fs::write(&path, b"{}").unwrap();
        assert_eq!(missing.save(|| true), Err(SaveError::Conflict));
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        let mut draft = DashboardDraft::load(&path).unwrap();
        let registry = WidgetRegistry::with_defaults();
        let first = draft.add("timers", 0, 0, &registry).unwrap();
        draft.add("quick_tools", 0, 0, &registry).unwrap();
        assert!(matches!(draft.save(|| true), Err(SaveError::Invalid(_))));
        draft.set_placement(first, 1, 0, 1, 1).unwrap();
        std::fs::write(&path, b"{\"version\":1}").unwrap();
        assert_eq!(draft.save(|| true), Err(SaveError::Conflict));

        let late_path = dir.path().join("late.json");
        let mut late = DashboardDraft::load(&late_path).unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let result = late.save(|| {
            if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 3 {
                std::fs::write(&late_path, b"{\"external\":true}").unwrap();
            }
            true
        });
        assert_eq!(result, Err(SaveError::Conflict));
        assert_eq!(std::fs::read(&late_path).unwrap(), b"{\"external\":true}");
    }

    #[test]
    fn unverified_commit_blocks_retry_until_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dashboard.json");
        let mut draft = DashboardDraft::load(&path).unwrap();
        assert_eq!(
            draft.save_inner(
                || true,
                || true,
                |_| Err(std::io::Error::other("readback failed"))
            ),
            Err(SaveError::CommittedUnverified)
        );
        assert!(path.exists());
        assert_eq!(draft.save(|| true), Err(SaveError::CommittedUnverified));
        assert!(DashboardDraft::load(&path).is_ok());
    }

    #[test]
    fn sparse_and_positional_drafts_preserve_untouched_raw_shapes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sparse.json");
        std::fs::write(&path, br#"{"extra":1,"slots":[{ "widget" : "timers", "row":0, "col":0 },{"widget":"quick_tools","row":1,"col":0,"row_span":0}]}"#).unwrap();
        let mut draft = DashboardDraft::load(&path).unwrap();
        let second = draft.slots()[1].token;
        draft.remove(second).unwrap();
        draft.save(|| true).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains(r#"{ "widget" : "timers", "row":0, "col":0 }"#));
        let root: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert!(root.get("grid").is_none());
        assert!(root["slots"][0].get("row_span").is_none());
        let empty_path = dir.path().join("empty-object.json");
        std::fs::write(&empty_path, b"{}").unwrap();
        let mut empty = DashboardDraft::load(&empty_path).unwrap();
        empty.save(|| true).unwrap();
        assert_eq!(std::fs::read(&empty_path).unwrap(), b"{}");

        let positional = dir.path().join("positional.json");
        std::fs::write(
            &positional,
            br#"[1,[3,3],[[null,"timers",0,0,0,0],[null,"quick_tools",1,0]]]"#,
        )
        .unwrap();
        let mut draft = DashboardDraft::load(&positional).unwrap();
        assert!(draft.validate().is_ok());
        let first = draft.slots()[0].token;
        let second = draft.slots()[1].token;
        draft.set_placement(second, 2, 0, 1, 1).unwrap();
        draft.save(|| true).unwrap();
        let saved = std::fs::read_to_string(&positional).unwrap();
        assert!(saved.starts_with('['));
        assert!(saved.contains(r#"[null,"timers",0,0,0,0]"#));
        assert_eq!(draft.slots()[0].token, first);
        assert_eq!(
            DashboardDraft::load(&positional).unwrap().slots()[1]
                .slot
                .row,
            2
        );
    }

    #[test]
    fn missing_parent_is_created_only_after_authorized_save() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("nested").join("dashboard");
        let path = parent.join("layout.json");
        let mut draft = DashboardDraft::load(&path).unwrap();
        assert_eq!(draft.save(|| false), Err(SaveError::Revoked));
        assert!(!parent.exists());
        draft.save(|| true).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn sparse_swap_and_second_save_keep_acknowledged_placement_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dashboard.json");
        std::fs::write(
            &path,
            br#"[1,[3,3],[[null,"timers",0,0],[null,"quick_tools",1,0]]]"#,
        )
        .unwrap();
        let mut draft = DashboardDraft::load(&path).unwrap();
        let a = draft.slots()[0].token;
        let b = draft.slots()[1].token;
        draft.swap(a, b).unwrap();
        draft.save(|| true).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains(r#"[null,"quick_tools",0,0]"#));
        assert!(saved.contains(r#"[null,"timers",1,0]"#));
        draft.set_placement(b, 0, 0, 2, 1).unwrap();
        draft.set_placement(a, 2, 0, 1, 1).unwrap();
        draft.save(|| true).unwrap();
        draft.set_grid(crate::dashboard::config::GridConfig { rows: 1, cols: 3 });
        draft.remove(a).unwrap();
        assert!(draft.validate().is_ok());
        draft.save(|| true).unwrap();
        assert!(DashboardDraft::load(&path).unwrap().validate().is_ok());
    }
}
