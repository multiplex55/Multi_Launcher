use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::weather::WeatherPlugin;

#[test]
fn search_returns_action() {
    let plugin = WeatherPlugin;
    let results = plugin.search("weather Berlin");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].action,
        "https://www.weather.com/weather/today/l/Berlin"
    );
}
