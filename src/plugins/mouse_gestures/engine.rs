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

#[derive(Debug, Clone, PartialEq)]
pub enum ParseErrorKind {
    EmptyInput,
    EmptyName,
    EmptyPoint { index: usize },
    MissingCoordinate { index: usize, coord: usize },
    ExtraCoordinate { index: usize },
    InvalidNumber { index: usize, coord: usize, value: String },
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
    let mut segment_length = 0.0;
    let mut target_distance = spacing;

    let mut iter = points.iter().skip(1);
    while let Some(point) = iter.next() {
        segment_length = distance(segment_start, *point);
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
        .map(|pair| normalize_vector(Vector {
            x: pair[1].x - pair[0].x,
            y: pair[1].y - pair[0].y,
        }))
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

fn best_predecessor(
    cost: &[Vec<f32>],
    steps: &[Vec<usize>],
    i: usize,
    j: usize,
) -> (f32, usize) {
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
