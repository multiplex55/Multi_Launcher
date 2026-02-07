use crate::actions::Action;
use crate::plugin::Plugin;
use urlencoding::encode;

/// Simple plugin that opens weather.com for a given location using the `weather` prefix.
pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "weather ";
        if let Some(rest) = crate::common::strip_prefix_ci(query, PREFIX) {
            let q = rest.trim();
            let q = q.trim();
            if !q.is_empty() {
                return vec![Action {
                    label: format!("Show weather for {q}"),
                    desc: "Web search".into(),
                    action: format!("https://www.weather.com/weather/today/l/{}", encode(q)),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
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
        vec![Action {
            label: "weather".into(),
            desc: "Weather".into(),
            action: "query:weather ".into(),
            args: None,
            preview_text: None,
            risk_level: None,
            icon: None,
        }]
    }
}
