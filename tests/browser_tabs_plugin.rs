use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::browser_tabs::BrowserTabsPlugin;

fn mock_tabs() -> Vec<(String, String)> {
    vec![
        ("Example Domain".into(), "http://example.com".into()),
        ("Rust Lang".into(), "https://www.rust-lang.org".into()),
    ]
}

#[test]
fn search_without_prefix_returns_empty() {
    let plugin = BrowserTabsPlugin::new_with_fetch(mock_tabs);
    assert!(plugin.search("nope").is_empty());
}

#[test]
fn commands_include_tab() {
    let plugin = BrowserTabsPlugin::new_with_fetch(mock_tabs);
    assert!(plugin.commands().iter().any(|a| a.label == "tab"));
}

#[test]
fn search_filters_tabs() {
    let plugin = BrowserTabsPlugin::new_with_fetch(mock_tabs);
    let results = plugin.search("tab rust");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "Rust Lang");
}

#[cfg(windows)]
#[test]
fn uia_init_failure_returns_empty() {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    unsafe {
        // Initialize COM in multithreaded mode so the plugin's STA init fails.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    let plugin = BrowserTabsPlugin::default();
    assert!(plugin.search("tab").is_empty());

    unsafe {
        CoUninitialize();
    }
}

#[cfg(not(windows))]
fn mock_all_ports_fail() -> Vec<(String, String)> {
    let ports = [9222u16, 9223, 9224, 9225, 9226, 9227, 9228, 9229];
    const MAX_CONSECUTIVE_FAILS: u8 = 3;
    let mut fails = 0u8;
    for _ in ports {
        fails += 1;
        if fails >= MAX_CONSECUTIVE_FAILS {
            break;
        }
    }
    Vec::new()
}

#[cfg(not(windows))]
#[test]
fn remote_debug_failures_return_empty() {
    let plugin = BrowserTabsPlugin::new_with_fetch(mock_all_ports_fail);
    assert!(plugin.search("tab").is_empty());
}
