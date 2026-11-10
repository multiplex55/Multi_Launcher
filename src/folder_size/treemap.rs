//! Pure layout for immediate children of an accepted folder-size projection.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Measurement {
    Bytes(u64),
    Unknown,
    Omitted,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Item {
    pub row: usize,
    pub measurement: Measurement,
    pub provisional: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tile {
    pub row: usize,
    pub bytes: u64,
    pub provisional: bool,
    pub rect: Rect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Layout {
    pub total: u128,
    pub tiles: Vec<Tile>,
    pub zero_count: usize,
    pub unknown_count: usize,
    pub omitted_count: usize,
    pub provisional_count: usize,
}

/// Slice the remaining rectangle along its longer axis. Stable row IDs resolve ties.
pub(crate) fn layout(
    items: impl IntoIterator<Item = Item>,
    width: f64,
    height: f64,
) -> Option<Layout> {
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || !(width * height).is_finite()
    {
        return None;
    }
    let mut result = Layout::default();
    let mut positive = Vec::new();
    for item in items {
        if item.provisional {
            result.provisional_count += 1;
        }
        match item.measurement {
            Measurement::Bytes(0) => result.zero_count += 1,
            Measurement::Bytes(bytes) => {
                result.total = result.total.checked_add(u128::from(bytes))?;
                positive.push((item.row, bytes, item.provisional));
            }
            Measurement::Unknown => result.unknown_count += 1,
            Measurement::Omitted => result.omitted_count += 1,
        }
    }
    positive.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut remaining = result.total;
    let mut available = Rect {
        x: 0.0,
        y: 0.0,
        width,
        height,
    };
    for (row, bytes, provisional) in positive {
        let ratio = (bytes as f64 / remaining as f64).clamp(0.0, 1.0);
        let rect = if available.width >= available.height {
            let extent = (available.width * ratio).clamp(0.0, available.width);
            let rect = Rect {
                width: extent,
                ..available
            };
            available.x += extent;
            available.width = (width - available.x).max(0.0);
            rect
        } else {
            let extent = (available.height * ratio).clamp(0.0, available.height);
            let rect = Rect {
                height: extent,
                ..available
            };
            available.y += extent;
            available.height = (height - available.y).max(0.0);
            rect
        };
        result.tiles.push(Tile {
            row,
            bytes,
            provisional,
            rect,
        });
        remaining -= u128::from(bytes);
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treemap_exact_totals_ties_and_unmeasured_entries() {
        let items = [
            Item {
                row: 8,
                measurement: Measurement::Bytes(u64::MAX),
                provisional: true,
            },
            Item {
                row: 2,
                measurement: Measurement::Bytes(u64::MAX),
                provisional: false,
            },
            Item {
                row: 3,
                measurement: Measurement::Bytes(1),
                provisional: false,
            },
            Item {
                row: 4,
                measurement: Measurement::Bytes(0),
                provisional: false,
            },
            Item {
                row: 5,
                measurement: Measurement::Unknown,
                provisional: false,
            },
            Item {
                row: 6,
                measurement: Measurement::Omitted,
                provisional: false,
            },
        ];
        let layout = layout(items, 600.0, 300.0).unwrap();
        assert_eq!(layout.total, u128::from(u64::MAX) * 2 + 1);
        assert_eq!(
            layout.tiles.iter().map(|tile| tile.row).collect::<Vec<_>>(),
            vec![2, 8, 3]
        );
        assert_eq!(
            (
                layout.zero_count,
                layout.unknown_count,
                layout.omitted_count,
                layout.provisional_count
            ),
            (1, 1, 1, 1)
        );
        let mut area = 0.0;
        for (index, tile) in layout.tiles.iter().enumerate() {
            let r = tile.rect;
            assert!(r.x.is_finite() && r.y.is_finite() && r.width >= 0.0 && r.height >= 0.0);
            assert!(r.x + r.width <= 600.0 && r.y + r.height <= 300.0);
            area += r.width * r.height;
            for other in &layout.tiles[..index] {
                let s = other.rect;
                assert!(
                    r.x + r.width <= s.x
                        || s.x + s.width <= r.x
                        || r.y + r.height <= s.y
                        || s.y + s.height <= r.y
                );
            }
        }
        assert!((area - 180_000.0).abs() < 0.001);
        assert!(
            (layout.tiles[0].rect.width * layout.tiles[0].rect.height / area - 0.5).abs() < 1e-10
        );
        assert_eq!(super::layout(items, 600.0, 300.0).unwrap(), layout);
    }

    #[test]
    fn treemap_large_and_many_tiny_children_remain_bounded() {
        let mut items = vec![Item {
            row: 0,
            measurement: Measurement::Bytes(u64::MAX),
            provisional: false,
        }];
        items.extend((1..=128).map(|row| Item {
            row,
            measurement: Measurement::Bytes(1),
            provisional: false,
        }));
        items.push(Item {
            row: 129,
            measurement: Measurement::Bytes(0),
            provisional: false,
        });
        items.push(Item {
            row: 130,
            measurement: Measurement::Unknown,
            provisional: false,
        });
        let layout = layout(items, 600.0, 300.0).unwrap();
        assert_eq!(layout.total, u128::from(u64::MAX) + 128);
        assert_eq!(
            (
                layout.zero_count,
                layout.unknown_count,
                layout.omitted_count
            ),
            (1, 1, 0)
        );
        assert_eq!(
            layout.tiles.iter().map(|tile| tile.row).collect::<Vec<_>>(),
            (0..=128).collect::<Vec<_>>()
        );
        let mut area = 0.0;
        for (index, tile) in layout.tiles.iter().enumerate() {
            let r = tile.rect;
            assert!(
                [r.x, r.y, r.width, r.height]
                    .into_iter()
                    .all(f64::is_finite)
            );
            assert!(r.x >= 0.0 && r.y >= 0.0 && r.width >= 0.0 && r.height >= 0.0);
            assert!(r.x + r.width <= 600.0 && r.y + r.height <= 300.0);
            area += r.width * r.height;
            for other in &layout.tiles[..index] {
                let s = other.rect;
                assert!(
                    r.x + r.width <= s.x
                        || s.x + s.width <= r.x
                        || r.y + r.height <= s.y
                        || s.y + s.height <= r.y
                );
            }
        }
        assert!((area - 180_000.0).abs() < 0.001);
        assert!(
            (layout.tiles[0].rect.width * layout.tiles[0].rect.height / area
                - u64::MAX as f64 / layout.total as f64)
                .abs()
                < 1e-10
        );
    }
    #[test]
    fn treemap_empty_and_invalid_geometry() {
        let item = Item {
            row: 0,
            measurement: Measurement::Bytes(0),
            provisional: false,
        };
        assert!(layout([item], 10.0, 10.0).unwrap().tiles.is_empty());
        for (width, height) in [
            (0.0, 1.0),
            (-1.0, 1.0),
            (f64::NAN, 1.0),
            (1.0, f64::INFINITY),
            (f64::MAX, 2.0),
        ] {
            assert!(layout([item], width, height).is_none());
        }
    }
}
