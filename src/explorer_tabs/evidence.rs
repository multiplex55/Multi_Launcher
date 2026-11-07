//! Pure checks for worker-local UIA tab observations. These do not mint native
//! authority; the Windows owner supplies live element-identity comparisons.

pub(super) fn validate_tab_prefix(
    expected: &[Vec<i32>],
    observed: &[Vec<i32>],
    same_elements: &[bool],
    expected_count: usize,
) -> Result<(), &'static str> {
    if observed.len() != expected_count
        || expected.len() > expected_count
        || same_elements.len() != expected.len()
    {
        return Err("Explorer tab count changed");
    }
    for (index, id) in observed.iter().enumerate() {
        if id.is_empty() || observed[..index].iter().any(|old| old == id) {
            return Err("duplicate or empty tab runtime ID");
        }
    }
    for ((old, current), same) in expected.iter().zip(observed).zip(same_elements) {
        if old != current || !same {
            return Err("Explorer tab identity or order changed");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_tabs_backend_tab_admission_rejects_reorder_and_reused_identity() {
        let expected = vec![vec![1], vec![2]];
        assert!(
            validate_tab_prefix(&expected, &[vec![1], vec![2], vec![3]], &[true, true], 3).is_ok()
        );
        assert!(
            validate_tab_prefix(&expected, &[vec![2], vec![1], vec![3]], &[true, true], 3).is_err()
        );
        assert!(
            validate_tab_prefix(&expected, &[vec![1], vec![2], vec![1]], &[true, true], 3).is_err()
        );
        assert!(
            validate_tab_prefix(&expected, &[vec![1], vec![2], vec![3]], &[true, false], 3)
                .is_err()
        );
        assert!(validate_tab_prefix(&expected, &[vec![1], vec![2]], &[true, true], 3).is_err());
    }
}
