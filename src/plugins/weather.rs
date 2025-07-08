use crate::actions::Action;
use crate::plugin::Plugin;

/// Simple plugin that opens weather.com for a given location using the `weather` prefix.
pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if let Some(q) = query.strip_prefix("weather ") {
            let q = q.trim();
            if !q.is_empty() {
                return vec![Action {
                    label: format!("Show weather for {q}"),
                    desc: "Web search".into(),
                    action: format!("https://www.weather.com/weather/today/l/{q}"),
                    args: None,
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "weather"
    }

    fn description(&self) -> &str {
        "Display the weather for a location using weather.com (prefix: `weather`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}

