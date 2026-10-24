use multi_launcher::commands::{Command, DialogCommand, parse_action};
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::image_compare::ImageComparePlugin;

#[test]
fn imgdiff_plugin_routes_only_its_exact_prefix_to_typed_open() {
    let plugin = ImageComparePlugin;
    assert_eq!(plugin.query_prefixes(), &["imgdiff"]);
    assert_eq!(plugin.capabilities(), &["search"]);

    for query in ["imgdiff", "IMGDIFF", " imgdiff "] {
        let results = plugin.search(query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, "imgdiff:open");
        assert!(results[0].args.is_none());
        assert_eq!(
            parse_action(&results[0]).unwrap(),
            Command::Dialog(DialogCommand::ImageCompare)
        );
    }

    for query in ["imgdiffusion", "imgdiff image.png", "imgdiff:open"] {
        assert!(plugin.search(query).is_empty(), "{query:?}");
    }
}
