use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct SemVer {
    major: u64,
    minor: u64,
    patch: u64,
}

impl SemVer {
    fn parse(version: &str) -> Self {
        let mut parts = version.split('.');

        let major = parts
            .next()
            .unwrap_or("")
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("malformed semver '{version}': invalid major component"));
        let minor = parts
            .next()
            .unwrap_or("")
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("malformed semver '{version}': invalid minor component"));
        let patch = parts
            .next()
            .unwrap_or("")
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("malformed semver '{version}': invalid patch component"));

        assert!(
            parts.next().is_none(),
            "malformed semver '{version}': expected exactly 3 numeric components"
        );

        Self {
            major,
            minor,
            patch,
        }
    }
}

fn extract_package_versions(lockfile: &str, crate_name: &str) -> Vec<String> {
    let mut versions = Vec::new();
    let mut in_package = false;
    let mut current_name: Option<&str> = None;

    for line in lockfile.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            in_package = true;
            current_name = None;
            continue;
        }

        if !in_package {
            continue;
        }

        if let Some(name) = trimmed.strip_prefix("name = \"").and_then(|s| s.strip_suffix('"')) {
            current_name = Some(name);
            continue;
        }

        if let Some(version) = trimmed
            .strip_prefix("version = \"")
            .and_then(|s| s.strip_suffix('"'))
        {
            if current_name == Some(crate_name) {
                versions.push(version.to_string());
            }
            in_package = false;
            current_name = None;
        }
    }

    versions
}

fn assert_floor(lockfile: &str, crate_name: &str, min_version: &str) {
    let versions = extract_package_versions(lockfile, crate_name);
    assert!(
        !versions.is_empty(),
        "crate '{crate_name}' not found in Cargo.lock"
    );

    let unique_versions: BTreeSet<_> = versions.iter().cloned().collect();
    assert_eq!(
        unique_versions.len(),
        1,
        "crate '{crate_name}' appears with multiple versions in Cargo.lock: {:?}",
        unique_versions
    );

    let detected = SemVer::parse(unique_versions.iter().next().expect("set is non-empty"));
    let minimum = SemVer::parse(min_version);
    assert!(
        detected >= minimum,
        "crate '{crate_name}' resolved to version {} but minimum required is {min_version}",
        unique_versions.iter().next().expect("set is non-empty")
    );
}

#[test]
fn lockfile_enforces_dependency_floors() {
    let lockfile_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
    let lockfile = fs::read_to_string(&lockfile_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", lockfile_path.display()));

    assert_floor(&lockfile, "time", "0.3.47");
    assert_floor(&lockfile, "bytes", "1.11.1");
}

#[test]
fn semver_helper_equal() {
    assert_eq!(SemVer::parse("1.2.3"), SemVer::parse("1.2.3"));
}

#[test]
fn semver_helper_greater() {
    assert!(SemVer::parse("1.2.4") > SemVer::parse("1.2.3"));
}

#[test]
fn semver_helper_less() {
    assert!(SemVer::parse("1.2.2") < SemVer::parse("1.2.3"));
}
