#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GestureDefinition {
    pub name: Option<String>,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureDirection {
    Up,
    Down,
    Left,
    Right,
    UpRight,
    UpLeft,
    DownRight,
    DownLeft,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseErrorKind {
    EmptyInput,
    EmptyName,
    EmptyPoint {
        index: usize,
    },
    MissingCoordinate {
        index: usize,
        coord: usize,
    },
    ExtraCoordinate {
        index: usize,
    },
    InvalidNumber {
        index: usize,
        coord: usize,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.kind)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq)]
pub enum PreprocessError {
    TooFewPoints,
    InvalidSampleCount,
    TooShort { length: f32, min_length: f32 },
}

pub struct PreprocessConfig {
    pub sample_count: usize,
    pub smoothing_window: usize,
    pub min_track_len: f32,
}

const DIRECTION_SAMPLE_COUNT: usize = 64;
const DIRECTION_SMOOTHING_WINDOW: usize = 5;
const INSERT_DELETE_COST: f32 = 0.35;

pub fn preprocess_points_for_directions(
    points: &[Point],
    settings: &crate::plugins::mouse_gestures::settings::MouseGesturePluginSettings,
) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let mut processed = points.to_vec();
    if settings.sampling_enabled {
        let sample_count = DIRECTION_SAMPLE_COUNT.min(processed.len().max(2));
        processed = resample_points(&processed, sample_count);
    }

    if settings.smoothing_enabled {
        processed = smooth_points(&processed, DIRECTION_SMOOTHING_WINDOW);
    }

    processed
}

pub fn direction_sequence(
    points: &[Point],
    settings: &crate::plugins::mouse_gestures::settings::MouseGesturePluginSettings,
) -> Vec<GestureDirection> {
    if points.len() < 2 {
        return Vec::new();
    }

    let segment_threshold = settings.segment_threshold_px.max(0.0);
    let tolerance = settings.direction_tolerance_deg.max(0.0);
    let mut dirs = Vec::new();
    let mut anchor = points[0];
    let mut last_direction: Option<GestureDirection> = None;
    for point in points.iter().skip(1) {
        let dx = point.x - anchor.x;
        let dy = point.y - anchor.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < segment_threshold {
            continue;
        }
        let angle = (-dy).atan2(dx);
        let direction = direction_from_angle(angle);
        if let Some(last) = last_direction {
            if direction == last {
                continue;
            }
            let angle_deg = angle.to_degrees().rem_euclid(360.0);
            let last_angle = direction_center_angle_deg(last);
            let delta = angular_difference_deg(angle_deg, last_angle);
            if delta < tolerance {
                continue;
            }
        }
        dirs.push(direction);
        last_direction = Some(direction);
        anchor = *point;
    }
    dirs
}

pub fn direction_similarity(a: &[GestureDirection], b: &[GestureDirection]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if b.len() == 1 {
        return single_direction_similarity(a, b[0]);
    }
    let distance = levenshtein_distance_weighted(a, b);
    let max_len = a.len().max(b.len()) as f32;
    if max_len == 0.0 {
        0.0
    } else {
        (1.0 - distance / max_len).clamp(0.0, 1.0)
    }
}

fn direction_index(direction: GestureDirection) -> i32 {
    match direction {
        GestureDirection::Right => 0,
        GestureDirection::UpRight => 1,
        GestureDirection::Up => 2,
        GestureDirection::UpLeft => 3,
        GestureDirection::Left => 4,
        GestureDirection::DownLeft => 5,
        GestureDirection::Down => 6,
        GestureDirection::DownRight => 7,
    }
}

fn angular_steps(a: GestureDirection, b: GestureDirection) -> i32 {
    let a_idx = direction_index(a);
    let b_idx = direction_index(b);
    let diff = (a_idx - b_idx).abs();
    diff.min(8 - diff)
}

fn substitution_cost(a: GestureDirection, b: GestureDirection) -> f32 {
    if a == b {
        return 0.0;
    }
    angular_steps(a, b) as f32 / 4.0
}

fn levenshtein_distance_weighted(a: &[GestureDirection], b: &[GestureDirection]) -> f32 {
    let mut prev: Vec<f32> = (0..=b.len())
        .map(|v| v as f32 * INSERT_DELETE_COST)
        .collect();
    let mut curr = vec![0.0; b.len() + 1];
    for (i, &av) in a.iter().enumerate() {
        curr[0] = (i + 1) as f32 * INSERT_DELETE_COST;
        for (j, &bv) in b.iter().enumerate() {
            let cost = substitution_cost(av, bv);
            curr[j + 1] = (prev[j + 1] + INSERT_DELETE_COST)
                .min(curr[j] + INSERT_DELETE_COST)
                .min(prev[j] + cost);
        }
        prev.clone_from_slice(&curr);
    }
    prev[b.len()]
}

fn direction_vector(direction: GestureDirection) -> Vector {
    let diag = (2.0_f32).sqrt() / 2.0;
    match direction {
        GestureDirection::Right => Vector { x: 1.0, y: 0.0 },
        GestureDirection::UpRight => Vector { x: diag, y: -diag },
        GestureDirection::Up => Vector { x: 0.0, y: -1.0 },
        GestureDirection::UpLeft => Vector { x: -diag, y: -diag },
        GestureDirection::Left => Vector { x: -1.0, y: 0.0 },
        GestureDirection::DownLeft => Vector { x: -diag, y: diag },
        GestureDirection::Down => Vector { x: 0.0, y: 1.0 },
        GestureDirection::DownRight => Vector { x: diag, y: diag },
    }
}

fn single_direction_similarity(a: &[GestureDirection], template: GestureDirection) -> f32 {
    let matches = a
        .iter()
        .filter(|&&direction| angular_steps(direction, template) <= 1)
        .count();
    let purity = matches as f32 / a.len() as f32;
    let gesture_vector = a.iter().fold(Vector { x: 0.0, y: 0.0 }, |acc, &dir| {
        let v = direction_vector(dir);
        Vector {
            x: acc.x + v.x,
            y: acc.y + v.y,
        }
    });
    let template_vector = direction_vector(template);
    let dot = gesture_vector.x * template_vector.x + gesture_vector.y * template_vector.y;
    let agreement = if dot >= 0.0 { 1.0 } else { 0.0 };
    (purity * agreement).clamp(0.0, 1.0)
}

fn direction_from_angle(angle: f32) -> GestureDirection {
    use std::f32::consts::PI;
    let sector = ((angle / (PI / 4.0)).round() as i32).rem_euclid(8);
    match sector {
        0 => GestureDirection::Right,
        1 => GestureDirection::UpRight,
        2 => GestureDirection::Up,
        3 => GestureDirection::UpLeft,
        4 => GestureDirection::Left,
        5 => GestureDirection::DownLeft,
        6 => GestureDirection::Down,
        _ => GestureDirection::DownRight,
    }
}

fn direction_center_angle_deg(direction: GestureDirection) -> f32 {
    match direction {
        GestureDirection::Right => 0.0,
        GestureDirection::UpRight => 45.0,
        GestureDirection::Up => 90.0,
        GestureDirection::UpLeft => 135.0,
        GestureDirection::Left => 180.0,
        GestureDirection::DownLeft => 225.0,
        GestureDirection::Down => 270.0,
        GestureDirection::DownRight => 315.0,
    }
}

fn angular_difference_deg(a: f32, b: f32) -> f32 {
    let diff = (a - b).abs();
    diff.min(360.0 - diff)
}

pub fn parse_gesture(input: &str) -> Result<GestureDefinition, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ParseError {
            kind: ParseErrorKind::EmptyInput,
        });
    }

    let (name, coords) = match trimmed.split_once(':') {
        Some((prefix, rest)) => {
            let name = prefix.trim();
            if name.is_empty() {
                return Err(ParseError {
                    kind: ParseErrorKind::EmptyName,
                });
            }
            (Some(name.to_string()), rest)
        }
        None => (None, trimmed),
    };

    let points = parse_points(coords)?;

    Ok(GestureDefinition { name, points })
}

#[cfg(test)]
mod tests {
    use super::{direction_sequence, direction_similarity, GestureDirection, Point};
    use crate::plugins::mouse_gestures::settings::MouseGesturePluginSettings;

    #[test]
    fn direction_similarity_prefers_smaller_angular_difference() {
        let base = [GestureDirection::Right];
        let slight = [GestureDirection::UpRight];
        let opposite = [GestureDirection::Left];

        let slight_similarity = direction_similarity(&base, &slight);
        let opposite_similarity = direction_similarity(&base, &opposite);

        assert!(
            slight_similarity > opposite_similarity,
            "expected {slight_similarity} to be greater than {opposite_similarity}"
        );
    }

    #[test]
    fn direction_similarity_identical_sequences_are_max() {
        let sequence = [
            GestureDirection::Up,
            GestureDirection::UpRight,
            GestureDirection::Right,
        ];

        let similarity = direction_similarity(&sequence, &sequence);

        assert_eq!(similarity, 1.0);
    }

    #[test]
    fn direction_similarity_long_up_with_wobble_is_high() {
        let gesture = [
            GestureDirection::Up,
            GestureDirection::UpRight,
            GestureDirection::Up,
            GestureDirection::UpLeft,
            GestureDirection::Up,
            GestureDirection::Up,
        ];
        let template = [
            GestureDirection::Up,
            GestureDirection::Up,
            GestureDirection::Up,
        ];

        let similarity = direction_similarity(&gesture, &template);

        assert!(similarity > 0.8, "expected {similarity} to exceed 0.8");
    }

    #[test]
    fn direction_similarity_up_vs_down_is_low() {
        let gesture = [GestureDirection::Up, GestureDirection::Up];
        let template = [GestureDirection::Down, GestureDirection::Down];

        let similarity = direction_similarity(&gesture, &template);

        assert!(similarity <= 0.3, "expected {similarity} to be at most 0.3");
    }

    #[test]
    fn direction_similarity_insert_delete_tolerant() {
        let gesture = [
            GestureDirection::Up,
            GestureDirection::Up,
            GestureDirection::Up,
            GestureDirection::Up,
        ];
        let template = [GestureDirection::Up, GestureDirection::Up];

        let similarity = direction_similarity(&gesture, &template);

        assert!(similarity > 0.7, "expected {similarity} to stay above 0.7");
    }

    #[test]
    fn direction_similarity_single_direction_purity_scoring() {
        let gesture = [
            GestureDirection::Up,
            GestureDirection::Up,
            GestureDirection::Up,
            GestureDirection::UpLeft,
            GestureDirection::Up,
            GestureDirection::Up,
            GestureDirection::UpLeft,
            GestureDirection::Up,
            GestureDirection::UpLeft,
            GestureDirection::Up,
        ];
        let template = [GestureDirection::Up];

        let similarity = direction_similarity(&gesture, &template);

        assert!(similarity > 0.8, "expected {similarity} to be above 0.8");
    }

    fn settings_with_thresholds(
        segment_threshold_px: f32,
        direction_tolerance_deg: f32,
    ) -> MouseGesturePluginSettings {
        let mut settings = MouseGesturePluginSettings::default();
        settings.segment_threshold_px = segment_threshold_px;
        settings.direction_tolerance_deg = direction_tolerance_deg;
        settings
    }

    #[test]
    fn direction_sequence_long_up_is_single_token() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 0.0, y: -30.0 },
            Point { x: 0.0, y: -60.0 },
        ];
        let settings = settings_with_thresholds(5.0, 5.0);

        let dirs = direction_sequence(&points, &settings);

        assert_eq!(dirs, vec![GestureDirection::Up]);
    }

    #[test]
    fn direction_sequence_ignores_jitter_below_threshold() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 3.0, y: -2.0 },
            Point { x: -1.0, y: 4.0 },
            Point { x: 2.0, y: -3.0 },
        ];
        let settings = settings_with_thresholds(10.0, 5.0);

        let dirs = direction_sequence(&points, &settings);

        assert!(dirs.is_empty());
    }

    #[test]
    fn direction_sequence_diagonal_stays_stable_with_tolerance() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: -8.0 },
            Point { x: 12.0, y: -28.0 },
            Point { x: 24.0, y: -36.0 },
        ];
        let settings = settings_with_thresholds(5.0, 30.0);

        let dirs = direction_sequence(&points, &settings);

        assert_eq!(dirs, vec![GestureDirection::UpRight]);
    }

    #[test]
    fn direction_sequence_compresses_repeated_tokens() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 15.0, y: 0.0 },
            Point { x: 30.0, y: 0.0 },
            Point { x: 45.0, y: 0.0 },
        ];
        let settings = settings_with_thresholds(5.0, 0.0);

        let dirs = direction_sequence(&points, &settings);

        assert_eq!(dirs, vec![GestureDirection::Right]);
    }
}

pub fn serialize_gesture(gesture: &GestureDefinition) -> String {
    let mut output = String::new();
    if let Some(name) = &gesture.name {
        output.push_str(name);
        output.push(':');
    }
    for (idx, point) in gesture.points.iter().enumerate() {
        if idx > 0 {
            output.push('|');
        }
        output.push_str(&format!("{},{}", point.x, point.y));
    }
    output
}

pub fn track_length(points: &[Point]) -> f32 {
    points
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum()
}

pub fn meets_min_track_len(points: &[Point], min_track_len: f32) -> bool {
    track_length(points) >= min_track_len
}

pub fn preprocess_points(
    points: &[Point],
    config: &PreprocessConfig,
) -> Result<Vec<Vector>, PreprocessError> {
    if points.len() < 2 {
        return Err(PreprocessError::TooFewPoints);
    }
    if config.sample_count < 2 {
        return Err(PreprocessError::InvalidSampleCount);
    }

    let length = track_length(points);
    if length < config.min_track_len {
        return Err(PreprocessError::TooShort {
            length,
            min_length: config.min_track_len,
        });
    }

    let resampled = resample_points(points, config.sample_count);
    let smoothed = smooth_points(&resampled, config.smoothing_window);
    Ok(points_to_vectors(&smoothed))
}

fn parse_points(coords: &str) -> Result<Vec<Point>, ParseError> {
    let mut points = Vec::new();
    for (index, segment) in coords.split('|').enumerate() {
        let segment = segment.trim();
        if segment.is_empty() {
            return Err(ParseError {
                kind: ParseErrorKind::EmptyPoint { index },
            });
        }
        let mut parts = segment.split(',');
        let x_part = parts.next().map(str::trim);
        let y_part = parts.next().map(str::trim);
        if parts.next().is_some() {
            return Err(ParseError {
                kind: ParseErrorKind::ExtraCoordinate { index },
            });
        }
        let x_part = x_part.ok_or(ParseError {
            kind: ParseErrorKind::MissingCoordinate { index, coord: 0 },
        })?;
        let y_part = y_part.ok_or(ParseError {
            kind: ParseErrorKind::MissingCoordinate { index, coord: 1 },
        })?;
        if x_part.is_empty() {
            return Err(ParseError {
                kind: ParseErrorKind::MissingCoordinate { index, coord: 0 },
            });
        }
        if y_part.is_empty() {
            return Err(ParseError {
                kind: ParseErrorKind::MissingCoordinate { index, coord: 1 },
            });
        }
        let x = x_part.parse::<f32>().map_err(|_| ParseError {
            kind: ParseErrorKind::InvalidNumber {
                index,
                coord: 0,
                value: x_part.to_string(),
            },
        })?;
        let y = y_part.parse::<f32>().map_err(|_| ParseError {
            kind: ParseErrorKind::InvalidNumber {
                index,
                coord: 1,
                value: y_part.to_string(),
            },
        })?;
        points.push(Point { x, y });
    }

    if points.is_empty() {
        return Err(ParseError {
            kind: ParseErrorKind::EmptyInput,
        });
    }

    Ok(points)
}

fn resample_points(points: &[Point], sample_count: usize) -> Vec<Point> {
    let total_length = track_length(points);
    if total_length == 0.0 {
        return vec![points[0]; sample_count];
    }

    let spacing = total_length / (sample_count as f32 - 1.0);
    let mut resampled = Vec::with_capacity(sample_count);
    resampled.push(points[0]);

    let mut accumulated = 0.0;
    let mut segment_start = points[0];
    let mut target_distance = spacing;

    let mut iter = points.iter().skip(1);
    while let Some(point) = iter.next() {
        let mut segment_length = distance(segment_start, *point);
        while accumulated + segment_length >= target_distance {
            let remaining = target_distance - accumulated;
            let t = remaining / segment_length;
            let new_point = Point {
                x: segment_start.x + (point.x - segment_start.x) * t,
                y: segment_start.y + (point.y - segment_start.y) * t,
            };
            resampled.push(new_point);
            segment_start = new_point;
            segment_length = distance(segment_start, *point);
            accumulated = 0.0;
            target_distance = spacing;
        }
        accumulated += segment_length;
        segment_start = *point;
    }

    if resampled.len() < sample_count {
        resampled.push(*points.last().unwrap());
    }

    resampled.truncate(sample_count);
    resampled
}

fn smooth_points(points: &[Point], window: usize) -> Vec<Point> {
    if window <= 1 || points.is_empty() {
        return points.to_vec();
    }

    let mut smoothed = Vec::with_capacity(points.len());
    let half = window / 2;
    for idx in 0..points.len() {
        let start = idx.saturating_sub(half);
        let end = (idx + half + 1).min(points.len());
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut count = 0.0;
        for point in &points[start..end] {
            sum_x += point.x;
            sum_y += point.y;
            count += 1.0;
        }
        smoothed.push(Point {
            x: sum_x / count,
            y: sum_y / count,
        });
    }
    smoothed
}

fn points_to_vectors(points: &[Point]) -> Vec<Vector> {
    points
        .windows(2)
        .map(|pair| {
            normalize_vector(Vector {
                x: pair[1].x - pair[0].x,
                y: pair[1].y - pair[0].y,
            })
        })
        .collect()
}

fn normalize_vector(vector: Vector) -> Vector {
    let length = (vector.x * vector.x + vector.y * vector.y).sqrt();
    if length == 0.0 {
        return Vector { x: 0.0, y: 0.0 };
    }
    Vector {
        x: vector.x / length,
        y: vector.y / length,
    }
}

fn distance(a: Point, b: Point) -> f32 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

fn vector_distance(a: Vector, b: Vector) -> f32 {
    let dot = (a.x * b.x + a.y * b.y).clamp(-1.0, 1.0);
    1.0 - dot
}

/// Compute DTW distance between two vector sequences.
///
/// Normalization formula:
/// `normalized = total_cost / path_len`, where `total_cost` is the cumulative DTW
/// cost and `path_len` is the number of steps in the optimal warping path.
/// Each step cost is `1 - dot(a, b)`, yielding a normalized range of `[0, 2]`.
pub fn dtw_distance(vectors_a: &[Vector], vectors_b: &[Vector]) -> f32 {
    if vectors_a.is_empty() || vectors_b.is_empty() {
        return 2.0;
    }

    let rows = vectors_a.len() + 1;
    let cols = vectors_b.len() + 1;
    let mut cost = vec![vec![f32::INFINITY; cols]; rows];
    let mut steps = vec![vec![usize::MAX; cols]; rows];
    cost[0][0] = 0.0;
    steps[0][0] = 0;

    for i in 1..rows {
        for j in 1..cols {
            let step_cost = vector_distance(vectors_a[i - 1], vectors_b[j - 1]);
            let (prev_cost, prev_steps) = best_predecessor(&cost, &steps, i, j);
            cost[i][j] = prev_cost + step_cost;
            steps[i][j] = prev_steps + 1;
        }
    }

    let final_steps = steps[rows - 1][cols - 1].max(1) as f32;
    (cost[rows - 1][cols - 1] / final_steps).clamp(0.0, 2.0)
}

fn best_predecessor(cost: &[Vec<f32>], steps: &[Vec<usize>], i: usize, j: usize) -> (f32, usize) {
    let mut best_cost = cost[i - 1][j - 1];
    let mut best_steps = steps[i - 1][j - 1];

    let candidates = [
        (cost[i - 1][j], steps[i - 1][j]),
        (cost[i][j - 1], steps[i][j - 1]),
    ];

    for (cand_cost, cand_steps) in candidates {
        if cand_cost < best_cost || (cand_cost == best_cost && cand_steps < best_steps) {
            best_cost = cand_cost;
            best_steps = cand_steps;
        }
    }

    (best_cost, best_steps)
}
