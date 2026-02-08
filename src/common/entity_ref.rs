use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Note,
    Todo,
    Event,
}

impl EntityRef {
    pub fn new(kind: EntityKind, id: impl Into<String>, title: Option<String>) -> Self {
        Self {
            kind,
            id: id.into(),
            title,
        }
    }

    pub fn display(&self) -> String {
        let kind = match self.kind {
            EntityKind::Note => "note",
            EntityKind::Todo => "todo",
            EntityKind::Event => "event",
        };
        match &self.title {
            Some(title) if !title.is_empty() => format!("{kind}:{} ({title})", self.id),
            _ => format!("{kind}:{}", self.id),
        }
    }
}
