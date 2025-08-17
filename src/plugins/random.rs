use crate::actions::Action;
use crate::plugin::Plugin;
use rand::distributions::Alphanumeric;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::sync::Mutex;

pub struct RandomPlugin {
    rng: Mutex<StdRng>,
}

impl RandomPlugin {
    /// Create a new plugin using randomness from the operating system.
    pub fn new() -> Self {
        Self {
            rng: Mutex::new(StdRng::from_entropy()),
        }
    }

    /// Create a plugin with a fixed seed (useful for deterministic tests).
    pub fn from_seed(seed: u64) -> Self {
        Self {
            rng: Mutex::new(StdRng::seed_from_u64(seed)),
        }
    }
}

impl Default for RandomPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for RandomPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        let rest = if let Some(r) = crate::common::strip_prefix_ci(trimmed, "rand ") {
            r
        } else if let Some(r) = crate::common::strip_prefix_ci(trimmed, "random ") {
            r
        } else {
            return Vec::new();
        };
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.is_empty() {
            return Vec::new();
        }
        match parts[0] {
            "number" | "num" => {
                if parts.len() != 2 {
                    return Vec::new();
                }
                if let Ok(max) = parts[1].parse::<u64>() {
                    let mut rng = self.rng.lock().unwrap();
                    let value = rng.gen_range(0..=max).to_string();
                    return vec![Action {
                        label: value.clone(),
                        desc: "Random number".into(),
                        action: format!("clipboard:{value}"),
                        args: None,
                    }];
                }
            }
            "dice" => {
                let mut rng = self.rng.lock().unwrap();
                let value = rng.gen_range(1..=6).to_string();
                return vec![Action {
                    label: value.clone(),
                    desc: "Dice roll".into(),
                    action: format!("clipboard:{value}"),
                    args: None,
                }];
            }
            "pw" | "password" => {
                if parts.len() != 2 {
                    return Vec::new();
                }
                if let Ok(len) = parts[1].parse::<usize>() {
                    let mut rng = self.rng.lock().unwrap();
                    let pw: String = (&mut *rng)
                        .sample_iter(&Alphanumeric)
                        .take(len)
                        .map(char::from)
                        .collect();
                    return vec![Action {
                        label: pw.clone(),
                        desc: "Random password".into(),
                        action: format!("clipboard:{pw}"),
                        args: None,
                    }];
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "random"
    }

    fn description(&self) -> &str {
        "Generate random numbers, dice rolls and passwords (prefix: `rand`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "rand number <max>".into(),
                desc: "Random number".into(),
                action: "query:rand number ".into(),
                args: None,
            },
            Action {
                label: "rand dice".into(),
                desc: "Dice".into(),
                action: "query:rand dice".into(),
                args: None,
            },
            Action {
                label: "rand pw <len>".into(),
                desc: "Random password".into(),
                action: "query:rand pw ".into(),
                args: None,
            },
        ]
    }
}
