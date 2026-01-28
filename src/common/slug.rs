use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

/// Global lookup of slug base -> next suffix index.
static SLUGS: Lazy<Mutex<HashMap<String, usize>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Reset the slug lookup. Should be called before scanning existing notes.
pub fn reset_slug_lookup() {
    if let Ok(mut m) = SLUGS.lock() {
        m.clear();
    }
}

/// Register an already existing slug so future generations avoid collisions.
pub fn register_slug(slug: &str) {
    let (base, next) = parse_slug(slug);
    if let Ok(mut m) = SLUGS.lock() {
        let entry = m.entry(base.to_string()).or_insert(0);
        *entry = (*entry).max(next + 1);
    }
}

fn parse_slug(s: &str) -> (&str, usize) {
    if let Some((base, num)) = s.rsplit_once('-') {
        if let Ok(n) = num.parse::<usize>() {
            return (base, n);
        }
    }
    (s, 0)
}

/// Convert a title to a filesystem safe slug.
pub fn slugify(title: &str) -> String {
    slug::slugify(title)
}

/// Generate a unique slug for a title, appending numeric suffixes when needed.
pub fn unique_slug(title: &str) -> String {
    let base = slugify(title);
    let mut m = SLUGS.lock().unwrap();
    let count = m.entry(base.clone()).or_insert(0);
    let slug = if *count == 0 {
        base.clone()
    } else {
        format!("{}-{}", base, *count)
    };
    *count += 1;
    slug
}
