use super::model::DiscoverySnapshot;
const MAX_RESULT_ROWS: usize = 350_000;

#[derive(Clone, Copy)]
pub(crate) enum ResultRow {
    VerifiedHeader(usize),
    VerifiedMember { group: usize, member: usize },
    UnverifiedHeader(usize),
    UnverifiedKnownMember { group: usize, member: usize },
    UnverifiedPath { group: usize, path: usize },
}

pub(crate) fn build_result_rows(snapshot: &DiscoverySnapshot, rows: &mut Vec<ResultRow>) -> bool {
    rows.clear();
    let mut truncated = false;
    let mut append = |row| {
        if rows.len() < MAX_RESULT_ROWS {
            rows.push(row);
        } else {
            truncated = true;
        }
    };
    for (group_index, group) in snapshot.verified_duplicate_groups.iter().enumerate() {
        append(ResultRow::VerifiedHeader(group_index));
        for member in 0..group.known_files.len() {
            append(ResultRow::VerifiedMember {
                group: group_index,
                member,
            });
        }
    }
    for (group_index, group) in snapshot.identity_unverified_matches.iter().enumerate() {
        append(ResultRow::UnverifiedHeader(group_index));
        for member in 0..group.known_files.len() {
            append(ResultRow::UnverifiedKnownMember {
                group: group_index,
                member,
            });
        }
        for path in 0..group.unverified_path_indices.len() {
            append(ResultRow::UnverifiedPath {
                group: group_index,
                path,
            });
        }
    }
    truncated
}
