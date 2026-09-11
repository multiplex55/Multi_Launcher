use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::BTreeMap, fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VirtualDesktopId(String);

impl VirtualDesktopId {
    pub fn parse(value: &str) -> Result<Self, VirtualDesktopError> {
        let value = value.trim().trim_matches(['{', '}']);
        if value.len() != 36
            || !value.chars().enumerate().all(|(index, ch)| match index {
                8 | 13 | 18 | 23 => ch == '-',
                _ => ch.is_ascii_hexdigit(),
            })
        {
            return Err(VirtualDesktopError::new(
                VirtualDesktopErrorKind::InvalidSelector,
                "parse desktop identity",
                format!("Invalid virtual desktop GUID: {value}"),
            )
            .context("desktop_id", value));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VirtualDesktopId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for VirtualDesktopId {
    type Err = VirtualDesktopError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for VirtualDesktopId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for VirtualDesktopId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualDesktopInfo {
    pub id: VirtualDesktopId,
    pub index: u32,
    pub name: Option<String>,
    pub is_current: bool,
}

impl VirtualDesktopInfo {
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("Desktop {}", self.index))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtualDesktopSelector {
    Number(u32),
    Name(String),
    Id(VirtualDesktopId),
}

impl VirtualDesktopSelector {
    pub fn parse(value: &str) -> Result<Self, VirtualDesktopError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(VirtualDesktopError::new(
                VirtualDesktopErrorKind::InvalidSelector,
                "parse desktop selector",
                "Virtual desktop selector cannot be empty",
            ));
        }
        if let Ok(number) = value.parse::<u32>() {
            return Ok(Self::Number(number));
        }
        if let Ok(id) = VirtualDesktopId::parse(value) {
            return Ok(Self::Id(id));
        }
        Ok(Self::Name(value.to_string()))
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Number(number) => number.to_string(),
            Self::Name(name) => name.clone(),
            Self::Id(id) => id.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualDesktopBinding {
    pub id: VirtualDesktopId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualDesktopCapability {
    Enumeration,
    DirectSwitching,
    Creation,
    Closing,
    Renaming,
    WindowMembership,
    WindowMovement,
}

impl fmt::Display for VirtualDesktopCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Enumeration => "enumeration",
            Self::DirectSwitching => "direct switching",
            Self::Creation => "creation",
            Self::Closing => "closing",
            Self::Renaming => "renaming",
            Self::WindowMembership => "window membership",
            Self::WindowMovement => "window movement",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VirtualDesktopCapabilities {
    pub enumeration: bool,
    pub direct_switching: bool,
    pub creation: bool,
    pub closing: bool,
    pub renaming: bool,
    pub window_membership: bool,
    pub window_movement: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualDesktopSnapshot {
    pub desktops: Vec<VirtualDesktopInfo>,
    pub capabilities: VirtualDesktopCapabilities,
}

impl VirtualDesktopSnapshot {
    pub fn current(&self) -> Result<&VirtualDesktopInfo, VirtualDesktopError> {
        self.desktops
            .iter()
            .find(|desktop| desktop.is_current)
            .ok_or_else(|| {
                VirtualDesktopError::new(
                    VirtualDesktopErrorKind::Native,
                    "query current desktop",
                    "Windows did not identify a current virtual desktop",
                )
            })
    }

    pub fn resolve(
        &self,
        selector: &VirtualDesktopSelector,
    ) -> Result<&VirtualDesktopInfo, VirtualDesktopError> {
        let requested = selector.describe();
        let result = match selector {
            VirtualDesktopSelector::Number(0) => {
                return Err(VirtualDesktopError::new(
                    VirtualDesktopErrorKind::InvalidSelector,
                    "resolve desktop",
                    "Virtual desktop number must be at least 1",
                )
                .context("requested_selector", requested));
            }
            VirtualDesktopSelector::Number(number) => self
                .desktops
                .iter()
                .find(|desktop| desktop.index == *number),
            VirtualDesktopSelector::Id(id) => {
                self.desktops.iter().find(|desktop| desktop.id == *id)
            }
            VirtualDesktopSelector::Name(name) => {
                let mut matches = self.desktops.iter().filter(|desktop| {
                    desktop
                        .name
                        .as_deref()
                        .is_some_and(|current| current.eq_ignore_ascii_case(name.trim()))
                });
                let first = matches.next();
                if first.is_some() && matches.next().is_some() {
                    return Err(VirtualDesktopError::new(
                        VirtualDesktopErrorKind::AmbiguousSelector,
                        "resolve desktop",
                        format!("Multiple virtual desktops are named {name:?}"),
                    )
                    .context("requested_selector", requested));
                }
                first
            }
        };
        result.ok_or_else(|| {
            VirtualDesktopError::new(
                VirtualDesktopErrorKind::NotFound,
                "resolve desktop",
                format!("Virtual desktop {requested:?} was not found"),
            )
            .context("requested_selector", requested)
            .context("desktop_count", self.desktops.len().to_string())
        })
    }

    pub fn resolve_binding(
        &self,
        binding: &VirtualDesktopBinding,
    ) -> Result<&VirtualDesktopInfo, VirtualDesktopError> {
        self.desktops
            .iter()
            .find(|desktop| desktop.id == binding.id)
            .ok_or_else(|| {
                let mut error = VirtualDesktopError::new(
                    VirtualDesktopErrorKind::StaleBinding,
                    "resolve desktop binding",
                    format!("Bound virtual desktop {} no longer exists", binding.id),
                )
                .context("desktop_id", binding.id.to_string());
                if let Some(name) = &binding.cached_name {
                    error = error.context("cached_name", name);
                }
                error
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualDesktopErrorKind {
    InvalidSelector,
    NotFound,
    AmbiguousSelector,
    StaleBinding,
    UnsupportedCapability,
    InvalidWindow,
    Native,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualDesktopError {
    pub kind: VirtualDesktopErrorKind,
    pub operation: &'static str,
    pub message: String,
    pub context: BTreeMap<String, String>,
}

impl VirtualDesktopError {
    pub fn new(
        kind: VirtualDesktopErrorKind,
        operation: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            operation,
            message: message.into(),
            context: BTreeMap::new(),
        }
    }

    pub fn unsupported(operation: &'static str, capability: VirtualDesktopCapability) -> Self {
        Self::new(
            VirtualDesktopErrorKind::UnsupportedCapability,
            operation,
            format!("Virtual desktop {capability} is unavailable on this Windows build"),
        )
        .context("capability", capability.to_string())
    }

    pub fn context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

impl fmt::Display for VirtualDesktopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for VirtualDesktopError {}
