use crate::actions::Action;
use crate::plugin::Plugin;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use std::sync::Mutex;

pub struct RandomPlugin {
    rng: Mutex<StdRng>,
}

impl RandomPlugin {
    /// Create a new plugin using randomness from the operating system.
    pub fn new() -> Self {
        Self {
            rng: Mutex::new(StdRng::from_rng(&mut rand::rng())),
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
        if trimmed.eq_ignore_ascii_case("password") {
            return vec![Action {
                label: "Open Password Generator".into(),
                desc: "Generate passwords and passphrases".into(),
                action: "password:generator".into(),
                args: None,
            }];
        }
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
                    let mut rng = match self.rng.lock() {
                        Ok(rng) => rng,
                        Err(err) => {
                            tracing::error!(?err, "random plugin rng lock poisoned");
                            return Vec::new();
                        }
                    };
                    let value = rng.random_range(0..=max).to_string();
                    return vec![Action {
                        label: value.clone(),
                        desc: "Random number".into(),
                        action: format!("clipboard:{value}"),
                        args: None,
                    }];
                }
            }
            "dice" => {
                let mut rng = match self.rng.lock() {
                    Ok(rng) => rng,
                    Err(err) => {
                        tracing::error!(?err, "random plugin rng lock poisoned");
                        return Vec::new();
                    }
                };
                let value = rng.random_range(1..=6).to_string();
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
                let raw = parts[1];
                if !raw.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Vec::new();
                }
                if let Ok(length) = raw.parse::<usize>()
                    && crate::password_generator::PASSWORD_LENGTH_RANGE.contains(&length)
                {
                    return vec![Action {
                        label: "Generate and copy password".into(),
                        desc: format!("Password length: {length}"),
                        action: format!("password:copy:{length}"),
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
                label: "password".into(),
                desc: "Open Password Generator".into(),
                action: "query:password".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn password_generator_discovery_is_an_exact_payload_free_open() {
        let plugin = RandomPlugin::from_seed(7);
        let result = plugin.search(" PASSWORD ");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].action, "password:generator");
        assert!(result[0].args.is_none());
        assert_eq!(
            crate::commands::parse_action(&result[0]).unwrap(),
            crate::commands::Command::Dialog(crate::commands::DialogCommand::PasswordGenerator)
        );
        assert!(plugin.search("password extra").is_empty());
        assert!(
            plugin
                .commands()
                .iter()
                .any(|command| command.action == "query:password")
        );
    }

    #[test]
    fn poisoned_rng_lock_falls_back_without_panic() {
        let plugin = RandomPlugin::from_seed(7);
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = plugin.rng.lock().unwrap();
            panic!("poison");
        }));

        let out = plugin.search("rand dice");
        assert!(out.is_empty());
    }
}
