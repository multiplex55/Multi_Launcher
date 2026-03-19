pub fn apply_layout_restore_plan(
    plan: &crate::windows_layout::LayoutRestorePlan,
) -> anyhow::Result<()> {
    crate::windows_layout::apply_layout_restore_plan(plan)
}
