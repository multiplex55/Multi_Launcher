use anyhow::{anyhow, Result};
use rdev::Key;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub hotkey: Option<String>,
    pub index_paths: Option<Vec<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: Some("CapsLock".into()),
            index_paths: None,
        }
    }
}

impl Settings {
    pub fn load(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        if content.is_empty() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&content)?)
    }

    pub fn hotkey_key(&self) -> Result<Key> {
        let name = self.hotkey.as_deref().unwrap_or("CapsLock");
        parse_key(name).ok_or_else(|| anyhow!("Unknown key: {}", name))
    }
}

lazy_static::lazy_static! {
    static ref KEY_MAP: HashMap<String, Key> = {
        use Key::*;
        let keys = [
            Alt, AltGr, Backspace, CapsLock, ControlLeft, ControlRight, Delete,
            DownArrow, End, Escape, F1, F10, F11, F12, F2, F3, F4, F5, F6,
            F7, F8, F9, Home, LeftArrow, MetaLeft, MetaRight, PageDown, PageUp,
            Return, RightArrow, ShiftLeft, ShiftRight, Space, Tab, UpArrow,
            PrintScreen, ScrollLock, Pause, NumLock, BackQuote, Num1, Num2, Num3,
            Num4, Num5, Num6, Num7, Num8, Num9, Num0, Minus, Equal, KeyQ, KeyW,
            KeyE, KeyR, KeyT, KeyY, KeyU, KeyI, KeyO, KeyP, LeftBracket,
            RightBracket, KeyA, KeyS, KeyD, KeyF, KeyG, KeyH, KeyJ, KeyK, KeyL,
            SemiColon, Quote, BackSlash, IntlBackslash, KeyZ, KeyX, KeyC, KeyV,
            KeyB, KeyN, KeyM, Comma, Dot, Slash, Insert, KpReturn, KpMinus,
            KpPlus, KpMultiply, KpDivide, Kp0, Kp1, Kp2, Kp3, Kp4, Kp5, Kp6,
            Kp7, Kp8, Kp9, KpDelete, Function
        ];
        let mut m = HashMap::new();
        for k in keys.iter() {
            m.insert(format!("{:?}", k).to_lowercase(), *k);
        }
        m
    };
}

pub fn parse_key(name: &str) -> Option<Key> {
    KEY_MAP.get(&name.to_lowercase()).copied()
}
