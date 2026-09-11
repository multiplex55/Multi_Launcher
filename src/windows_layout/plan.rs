use crate::plugins::layouts_storage::{LayoutMatch, LayoutWindowState};
use crate::virtual_desktop::{
    VirtualDesktopError, VirtualDesktopId, VirtualDesktopSelector, VirtualDesktopSnapshot,
};

pub fn resolve_layout_desktop(
    snapshot: &VirtualDesktopSnapshot,
    stored: Option<&str>,
) -> Result<Option<VirtualDesktopId>, VirtualDesktopError> {
    let Some(stored) = stored.map(str::trim).filter(|stored| !stored.is_empty()) else {
        return Ok(None);
    };
    let selector = VirtualDesktopId::parse(stored)
        .map(VirtualDesktopSelector::Id)
        .unwrap_or_else(|_| VirtualDesktopSelector::Name(stored.to_string()));
    snapshot
        .resolve(&selector)
        .map(|desktop| Some(desktop.id.clone()))
}

pub fn is_rule_match(rule: &LayoutMatch, candidate: &LayoutMatch) -> bool {
    if rule.app_id.is_none()
        && rule.process.is_none()
        && rule.class.is_none()
        && rule.title.is_none()
    {
        return false;
    }
    let app_ok = match (&rule.app_id, &candidate.app_id) {
        (Some(rule), Some(candidate)) => rule.eq_ignore_ascii_case(candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    let process_ok = match (&rule.process, &candidate.process) {
        (Some(rule), Some(candidate)) => rule.eq_ignore_ascii_case(candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    let class_ok = match (&rule.class, &candidate.class) {
        (Some(rule), Some(candidate)) => rule.eq_ignore_ascii_case(candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    let title_ok = match (&rule.title, &candidate.title) {
        (Some(rule), Some(candidate)) => title_matches(rule, candidate),
        (Some(_), None) => false,
        (None, _) => true,
    };
    app_ok && process_ok && class_ok && title_ok
}

pub fn match_score(saved: &LayoutMatch, candidate: &LayoutMatch) -> Option<u8> {
    if saved.app_id.is_none()
        && saved.process.is_none()
        && saved.class.is_none()
        && saved.title.is_none()
    {
        return None;
    }
    if let (Some(saved), Some(candidate)) = (&saved.app_id, &candidate.app_id)
        && saved.eq_ignore_ascii_case(candidate)
    {
        return Some(4);
    }
    if let (Some(saved), Some(candidate)) = (&saved.process, &candidate.process)
        && saved.eq_ignore_ascii_case(candidate)
    {
        return Some(3);
    }
    if let (Some(saved), Some(candidate)) = (&saved.class, &candidate.class)
        && saved.eq_ignore_ascii_case(candidate)
    {
        return Some(2);
    }
    if let (Some(saved), Some(candidate)) = (&saved.title, &candidate.title)
        && title_matches(saved, candidate)
    {
        return Some(1);
    }
    None
}

pub fn restore_plan_select_best(
    saved: &LayoutMatch,
    candidates: &[LayoutMatch],
) -> Option<(usize, u8)> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(idx, candidate)| match_score(saved, candidate).map(|score| (idx, score)))
        .max_by_key(|(_, score)| *score)
}

fn title_matches(pattern: &str, title: &str) -> bool {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map(|re| re.is_match(title))
        .unwrap_or(false)
}

#[allow(dead_code)]
pub fn desired_state(state: &LayoutWindowState) -> &'static str {
    match state {
        LayoutWindowState::Normal => "normal",
        LayoutWindowState::Maximized => "maximized",
        LayoutWindowState::Minimized => "minimized",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matcher(process: &str, title: &str) -> LayoutMatch {
        LayoutMatch {
            app_id: None,
            process: Some(process.into()),
            class: None,
            title: Some(title.into()),
        }
    }

    #[test]
    fn process_match_scores_above_title_match() {
        let saved = matcher("notepad.exe", "Notes");
        let proc = matcher("notepad.exe", "Else");
        assert_eq!(match_score(&saved, &proc), Some(3));
    }

    #[test]
    fn restore_selection_picks_best_candidate() {
        let saved = matcher("notepad.exe", "Notes");
        let candidates = vec![
            matcher("calc.exe", "Notes"),
            matcher("notepad.exe", "Scratch"),
        ];
        assert_eq!(restore_plan_select_best(&saved, &candidates), Some((1, 3)));
    }

    #[test]
    fn empty_rule_does_not_match() {
        let empty = LayoutMatch {
            app_id: None,
            process: None,
            class: None,
            title: None,
        };
        assert!(!is_rule_match(&empty, &empty));
    }

    fn desktop_snapshot(names: &[&str]) -> VirtualDesktopSnapshot {
        VirtualDesktopSnapshot {
            desktops: names
                .iter()
                .enumerate()
                .map(|(index, name)| crate::virtual_desktop::VirtualDesktopInfo {
                    id: VirtualDesktopId::parse(&format!(
                        "{:08x}-0000-0000-0000-000000000000",
                        index + 1
                    ))
                    .unwrap(),
                    index: index as u32 + 1,
                    name: Some((*name).into()),
                    is_current: index == 0,
                })
                .collect(),
            capabilities: Default::default(),
        }
    }

    #[test]
    fn layout_desktop_resolves_guid_and_unique_legacy_name() {
        let snapshot = desktop_snapshot(&["Home", "Work"]);
        let id = snapshot.desktops[1].id.clone();
        assert_eq!(
            resolve_layout_desktop(&snapshot, Some(id.as_str())).unwrap(),
            Some(id.clone())
        );
        assert_eq!(
            resolve_layout_desktop(&snapshot, Some("work")).unwrap(),
            Some(id)
        );
        assert_eq!(resolve_layout_desktop(&snapshot, None).unwrap(), None);
    }

    #[test]
    fn layout_desktop_rejects_missing_and_ambiguous_names() {
        let snapshot = desktop_snapshot(&["Work", "work"]);
        assert!(resolve_layout_desktop(&snapshot, Some("Missing")).is_err());
        assert_eq!(
            resolve_layout_desktop(&snapshot, Some("WORK"))
                .unwrap_err()
                .kind,
            crate::virtual_desktop::VirtualDesktopErrorKind::AmbiguousSelector
        );
    }
}
