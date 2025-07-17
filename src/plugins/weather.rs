use crate::actions::Action;
use crate::plugin::Plugin;

/// Simple plugin that opens weather.com for a given location using the `weather` prefix.
pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "weather ";
        if query.len() >= PREFIX.len()
            && query[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)
        {
            let q = query[PREFIX.len()..].trim();
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

    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "weather".into(), desc: "weather".into(), action: "fill:weather ".into(), args: None }]
    }
}

