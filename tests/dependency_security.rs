use std::fs;
use std::path::Path;

const MINIMUM_SAFE_QUINN_PROTO: Version = Version {
    major: 0,
    minor: 11,
    patch: 15,
};
const NEXT_INCOMPATIBLE_QUINN_PROTO: Version = Version {
    major: 0,
    minor: 12,
    patch: 0,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

fn parse_version(version: &str) -> Result<Version, String> {
    let components: Vec<_> = version.split('.').collect();
    if components.len() != 3 {
        return Err(format!(
            "expected exactly three numeric components, found `{version}`"
        ));
    }

    let parse_component = |component: &str, name: &str| {
        if component.is_empty() || !component.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(format!(
                "{name} component must contain only decimal digits in `{version}`"
            ));
        }
        component
            .parse::<u64>()
            .map_err(|error| format!("invalid {name} component in `{version}`: {error}"))
    };

    Ok(Version {
        major: parse_component(components[0], "major")?,
        minor: parse_component(components[1], "minor")?,
        patch: parse_component(components[2], "patch")?,
    })
}

fn is_safe_quinn_proto(version: &str) -> Result<bool, String> {
    let version = parse_version(version)?;
    Ok(version >= MINIMUM_SAFE_QUINN_PROTO && version < NEXT_INCOMPATIBLE_QUINN_PROTO)
}

fn package_field<'a>(record: &'a str, field: &str) -> Option<&'a str> {
    let prefix = format!("{field} = \"");
    record
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(&prefix)?.strip_suffix('"'))
}

#[test]
fn resolved_quinn_proto_versions_are_not_vulnerable() {
    let lockfile_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
    let lockfile = fs::read_to_string(&lockfile_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", lockfile_path.display()));

    let versions: Vec<_> = lockfile
        .split("[[package]]")
        .filter(|record| package_field(record, "name") == Some("quinn-proto"))
        .map(|record| {
            package_field(record, "version")
                .unwrap_or_else(|| panic!("quinn-proto package record has no version: {record}"))
        })
        .collect();

    assert!(
        !versions.is_empty(),
        "Cargo.lock contains no exact quinn-proto package record; dependency removal or lockfile parser breakage must be reviewed explicitly"
    );

    for discovered in versions {
        let safe = is_safe_quinn_proto(discovered).unwrap_or_else(|error| {
            panic!("invalid resolved quinn-proto version `{discovered}`: {error}")
        });
        assert!(
            safe,
            "resolved quinn-proto version `{discovered}` is outside the explicitly supported secure range >= 0.11.15 and < 0.12.0; versions before 0.11.15 are vulnerable to fragmented-stream assembler memory exhaustion"
        );
    }
}

#[test]
fn quinn_proto_security_range_cases() {
    assert_eq!(is_safe_quinn_proto("0.11.14"), Ok(false));
    assert_eq!(is_safe_quinn_proto("0.11.15"), Ok(true));
    assert_eq!(is_safe_quinn_proto("0.11.99"), Ok(true));
    assert!(is_safe_quinn_proto("0.11.15-rc.1").is_err());
    assert_eq!(is_safe_quinn_proto("0.12.0"), Ok(false));
}
