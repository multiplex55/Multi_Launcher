use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirMode {
    Four,
    Eight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Left,
    Right,
    Up,
    Down,
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
}

impl Dir {
    fn token(self, mode: DirMode) -> char {
        match mode {
            DirMode::Four => match self {
                Dir::Left => 'L',
                Dir::Right => 'R',
                Dir::Up => 'U',
                Dir::Down => 'D',
                Dir::UpLeft | Dir::DownLeft => 'L',
                Dir::UpRight | Dir::DownRight => 'R',
            },
            DirMode::Eight => match self {
                Dir::DownLeft => '1',
                Dir::Down => '2',
                Dir::DownRight => '3',
                Dir::Left => '4',
                Dir::Right => '6',
                Dir::UpLeft => '7',
                Dir::Up => '8',
                Dir::UpRight => '9',
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Point {
    x: f32,
    y: f32,
}

impl From<(f32, f32)> for Point {
    fn from(value: (f32, f32)) -> Self {
        Self {
            x: value.0,
            y: value.1,
        }
    }
}

#[derive(Debug)]
pub struct GestureTracker {
    dir_mode: DirMode,
    threshold_px: f32,
    long_threshold_x: f32,
    long_threshold_y: f32,
    max_tokens: usize,
    tokens: Vec<char>,
    anchor_point: Option<Point>,
    last_point: Option<Point>,
    last_dir: Option<Dir>,
    last_time_ms: Option<u64>,
}

impl GestureTracker {
    pub fn new(
        dir_mode: DirMode,
        threshold_px: f32,
        long_threshold_x: f32,
        long_threshold_y: f32,
        max_tokens: usize,
    ) -> Self {
        Self {
            dir_mode,
            threshold_px,
            long_threshold_x,
            long_threshold_y,
            max_tokens,
            tokens: Vec::new(),
            anchor_point: None,
            last_point: None,
            last_dir: None,
            last_time_ms: None,
        }
    }

    pub fn feed_point(&mut self, point: (f32, f32), at_ms: u64) -> Option<char> {
        let point = Point::from(point);
        self.last_time_ms = Some(at_ms);

        if self.last_point.is_none() {
            self.last_point = Some(point);
            self.anchor_point = Some(point);
            return None;
        }

        self.last_point = Some(point);

        let anchor = match self.anchor_point {
            Some(anchor) => anchor,
            None => {
                self.anchor_point = Some(point);
                return None;
            }
        };

        let dx = point.x - anchor.x;
        let dy = point.y - anchor.y;
        let dist_sq = dx * dx + dy * dy;
        if dist_sq < self.threshold_px * self.threshold_px {
            return None;
        }

        let dir = match direction_from_delta(dx, dy, self.dir_mode) {
            Some(dir) => dir,
            None => return None,
        };

        match self.last_dir {
            Some(last_dir) if last_dir == dir => {
                if self.should_repeat(dir, dx, dy) {
                    return self.emit(dir, point);
                }
            }
            _ => {
                return self.emit(dir, point);
            }
        }

        None
    }

    pub fn tokens(&self) -> &[char] {
        &self.tokens
    }

    pub fn tokens_string(&self) -> String {
        self.tokens.iter().collect()
    }

    pub fn should_click(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn reset(&mut self) {
        self.tokens.clear();
        self.anchor_point = None;
        self.last_point = None;
        self.last_dir = None;
        self.last_time_ms = None;
    }

    fn emit(&mut self, dir: Dir, point: Point) -> Option<char> {
        self.anchor_point = Some(point);
        self.last_dir = Some(dir);
        let token = dir.token(self.dir_mode);
        if self.tokens.last().copied() == Some(token) {
            return None;
        }
        if self.tokens.len() < self.max_tokens {
            self.tokens.push(token);
            return Some(token);
        }
        None
    }

    fn should_repeat(&self, dir: Dir, dx: f32, dy: f32) -> bool {
        match dir {
            Dir::Left | Dir::Right => dx.abs() >= self.long_threshold_x,
            Dir::Up | Dir::Down => dy.abs() >= self.long_threshold_y,
            Dir::UpLeft | Dir::UpRight | Dir::DownLeft | Dir::DownRight => {
                dx.abs() >= self.long_threshold_x && dy.abs() >= self.long_threshold_y
            }
        }
    }
}

fn direction_from_delta(dx: f32, dy: f32, mode: DirMode) -> Option<Dir> {
    let abs_x = dx.abs();
    let abs_y = dy.abs();
    if abs_x == 0.0 && abs_y == 0.0 {
        return None;
    }

    let is_right = dx > 0.0;
    let is_down = dy > 0.0;

    match mode {
        DirMode::Four => {
            if abs_x >= abs_y {
                Some(if is_right { Dir::Right } else { Dir::Left })
            } else {
                Some(if is_down { Dir::Down } else { Dir::Up })
            }
        }
        DirMode::Eight => {
            if abs_x == 0.0 {
                return Some(if is_down { Dir::Down } else { Dir::Up });
            }
            if abs_y == 0.0 {
                return Some(if is_right { Dir::Right } else { Dir::Left });
            }
            Some(match (is_right, is_down) {
                (true, true) => Dir::DownRight,
                (true, false) => Dir::UpRight,
                (false, true) => Dir::DownLeft,
                (false, false) => Dir::UpLeft,
            })
        }
    }
}
