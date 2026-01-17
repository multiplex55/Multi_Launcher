#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseArgsResult<T> {
    Parsed(T),
    Usage(String),
}

pub fn parse_args<T>(
    args: &[&str],
    usage: &str,
    parser: impl FnOnce(&[&str]) -> Option<T>,
) -> ParseArgsResult<T> {
    if args.is_empty() {
        return ParseArgsResult::Usage(usage.to_string());
    }
    match parser(args) {
        Some(parsed) => ParseArgsResult::Parsed(parsed),
        None => ParseArgsResult::Usage(usage.to_string()),
    }
}
