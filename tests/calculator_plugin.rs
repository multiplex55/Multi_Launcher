use multi_launcher::plugin::Plugin;
use multi_launcher::plugins_builtin::CalculatorPlugin;

#[test]
fn simple_addition() {
    let plugin = CalculatorPlugin;
    let results = plugin.search("=1+2");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1+2 = 3");
    assert_eq!(results[0].action, "calc:3");
}
