use super::MultiManagerView;
use gpui_kit::{App, AppContext as _, Context, Entity, Subscription, Window};
use gpui_kit::component::input::{InputEvent, InputState};
use multi_launcher::multi_manager::model::{MmRect, MmWindow, MmWorkspace};

fn displayed(rect: Option<MmRect>) -> [String; 4] {
    let r = rect.unwrap_or(MmRect { x: 0, y: 0, w: 800, h: 600 });
    [r.x.to_string(), r.y.to_string(), r.w.to_string(), r.h.to_string()]
}

fn rectangle(base: Option<MmRect>, baseline: &[String], values: &[String]) -> Result<Option<MmRect>, &'static str> {
    if baseline == values { return Ok(base); }
    let parsed = values.iter().map(|value| value.parse::<i32>()).collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Enter whole numbers for each rectangle coordinate.")?;
    if !(1..=10000).contains(&parsed[2]) || !(1..=10000).contains(&parsed[3]) {
        return Err("Width and height must be between 1 and 10000.");
    }
    Ok(Some(MmRect { x: parsed[0], y: parsed[1], w: parsed[2], h: parsed[3] }))
}

fn compose_member(expected: &MmWindow, baseline: &[String], values: &[String], home: Option<MmRect>, target: Option<MmRect>) -> Result<MmWindow, &'static str> {
    let mut result = expected.clone();
    if values[0] != baseline[0] { result.alias = values[0].clone(); }
    result.home_rect = rectangle(home, &baseline[1..5], &values[1..5])?;
    result.target_rect = rectangle(target, &baseline[5..9], &values[5..9])?;
    Ok(result)
}

fn swap_rectangles(home: Option<MmRect>, target: Option<MmRect>) -> (Option<MmRect>, Option<MmRect>) { (target, home) }

pub(super) fn member_current(rows: &[MmWorkspace], row: usize, expected_row: &MmWorkspace, member: usize, expected: &MmWindow, issued: u64, current: u64) -> bool {
    super::row_current(rows, row, expected_row, issued, current)
        && rows.get(row).and_then(|workspace| workspace.windows.get(member)) == Some(expected)
}

pub(super) struct MemberDraft {
    pub row: usize,
    pub expected_row: MmWorkspace,
    pub revision: u64,
    pub member: usize,
    pub expected: MmWindow,
    pub inputs: Vec<Entity<InputState>>,
    baseline: Vec<String>,
    home: Option<MmRect>,
    target: Option<MmRect>,
    _subscriptions: Vec<Subscription>,
}
impl MemberDraft {
    pub fn new(row: usize, expected_row: MmWorkspace, revision: u64, member: usize, window: &mut Window, cx: &mut Context<MultiManagerView>) -> Self {
        let expected = expected_row.windows[member].clone();
        let mut baseline = vec![expected.alias.clone()];
        baseline.extend(displayed(expected.home_rect));
        baseline.extend(displayed(expected.target_rect));
        let inputs: Vec<_> = baseline.iter().map(|value| cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_value(value.clone(), window, cx);
            input
        })).collect();
        baseline[0] = inputs[0].read(cx).value().to_string();
        let subscriptions = inputs.iter().map(|input| cx.subscribe(input, |_: &mut MultiManagerView, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) { cx.notify(); }
        })).collect();
        Self { row, expected_row, revision, member, home: expected.home_rect, target: expected.target_rect, expected, inputs, baseline, _subscriptions: subscriptions }
    }
    fn values(&self, cx: &App) -> Vec<String> { self.inputs.iter().map(|input| input.read(cx).value().to_string()).collect() }
    pub fn result(&self, cx: &App) -> Result<MmWindow, &'static str> {
        let values = self.values(cx);
        compose_member(&self.expected, &self.baseline, &values, self.home, self.target)
    }
    pub fn swap(&mut self, window: &mut Window, cx: &mut Context<MultiManagerView>) -> Result<(), &'static str> {
        let values = self.values(cx);
        let home = rectangle(self.home, &self.baseline[1..5], &values[1..5])?;
        let target = rectangle(self.target, &self.baseline[5..9], &values[5..9])?;
        (self.home, self.target) = swap_rectangles(home, target);
        let home_values = displayed(target); let target_values = displayed(home);
        for (offset, value) in home_values.into_iter().chain(target_values).enumerate() {
            self.baseline[offset + 1] = value.clone();
            self.inputs[offset + 1].update(cx, |input, cx| input.set_value(value, window, cx));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{compose_member, displayed, member_current, rectangle, swap_rectangles};
    use multi_launcher::multi_manager::model::{MmRect, MmWindow, MmWorkspace};
    #[test]
    fn member_receipt_rejects_stale_row_revision_and_member() {
        let mut row = MmWorkspace::default();
        row.windows.push(MmWindow::default());
        let expected = row.windows[0].clone();
        assert!(member_current(&[row.clone()], 0, &row, 0, &expected, 4, 4));
        assert!(!member_current(&[row.clone()], 0, &row, 0, &expected, 4, 5));
        assert!(!member_current(&[row.clone()], 0, &row, 1, &expected, 4, 4));
        let mut changed = row.clone(); changed.windows[0].alias = "other".into();
        assert!(!member_current(&[changed], 0, &row, 0, &expected, 4, 4));
    }
    #[test]
    fn member_compose_swaps_actual_optional_rectangles_and_keeps_metadata() {
        let legacy = Some(MmRect { x: -20, y: 4, w: 0, h: 15000 });
        let mut member = MmWindow::default();
        member.alias = "original\nlabel".into(); member.executable = "fixture.exe".into();
        member.hwnd = 42; member.home_rect = None; member.target_rect = legacy;
        let (home, target) = swap_rectangles(member.home_rect, member.target_rect);
        let mut fields = vec!["originallabel".to_string()];
        fields.extend(displayed(home)); fields.extend(displayed(target));
        let changed = compose_member(&member, &fields, &fields, home, target).unwrap();
        assert_eq!(changed.home_rect, legacy); assert_eq!(changed.target_rect, None);
        assert_eq!(changed.alias, member.alias); assert_eq!(changed.executable, member.executable);
        assert_eq!(changed.hwnd, member.hwnd);
        let same = compose_member(&member, &fields, &fields, member.home_rect, member.target_rect).unwrap();
        assert_eq!(same, member);
    }
    #[test]
    fn member_rect_preserves_absent_and_legacy_until_edited() {
        let legacy = Some(MmRect { x: -4, y: 3, w: 0, h: 12000 });
        for value in [None, legacy] { let fields = displayed(value); assert_eq!(rectangle(value, &fields, &fields), Ok(value)); }
        let fields = displayed(legacy); let mut changed = fields.clone(); changed[0] = "7".into();
        assert!(rectangle(legacy, &fields, &changed).is_err());
        changed[2] = "10".into(); changed[3] = "20".into(); assert_eq!(rectangle(legacy, &fields, &changed).unwrap().unwrap().x, 7);
        changed[0] = "-".into(); assert!(rectangle(legacy, &fields, &changed).is_err());
    }
}
