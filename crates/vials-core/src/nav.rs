//! Moving around a vial with the arrow keys. Pure: it works from where the items were drawn.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

/// Items on one row have tops closer than this (px).
const SAME_ROW: f32 = 4.0;

/// The item to go to from `from` (or from nowhere) for a key, given each item's (left, top) in order.
/// `None` when there is nothing to go to (no items).
pub fn step(places: &[(f32, f32)], from: Option<usize>, dir: Dir) -> Option<usize> {
    let last = places.len().checked_sub(1)?;
    let Some(from) = from.map(|f| f.min(last)) else {
        return Some(if dir == Dir::End { last } else { 0 });
    };
    Some(match dir {
        Dir::Left => from.saturating_sub(1),
        Dir::Right => (from + 1).min(last),
        Dir::Home => 0,
        Dir::End => last,
        Dir::Up | Dir::Down => {
            let (x, y) = places[from];
            // The nearest row above or below: the closest top on that side.
            let other_rows = places.iter().map(|p| p.1).filter(|top| if dir == Dir::Up { *top < y - SAME_ROW } else { *top > y + SAME_ROW });
            let row = if dir == Dir::Up { other_rows.fold(f32::MIN, f32::max) } else { other_rows.fold(f32::MAX, f32::min) };
            if row == f32::MIN || row == f32::MAX {
                return Some(from);
            }
            places
                .iter()
                .enumerate()
                .filter(|(_, p)| (p.1 - row).abs() <= SAME_ROW)
                .min_by(|a, b| (a.1 .0 - x).abs().total_cmp(&(b.1 .0 - x).abs()))
                .map_or(from, |(i, _)| i)
        }
    })
}

/// The items from `a` to `b` (either way round), for Shift+arrow.
pub fn range(a: usize, b: usize) -> std::ops::RangeInclusive<usize> {
    a.min(b)..=a.max(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three rows of three, 80 px apart, and a last row with two.
    fn grid() -> Vec<(f32, f32)> {
        (0..11).map(|i| ((i % 3) as f32 * 80.0, (i / 3) as f32 * 90.0)).collect()
    }

    #[test]
    fn left_and_right_walk_the_order_and_stop_at_the_ends() {
        let g = grid();
        assert_eq!(step(&g, Some(4), Dir::Right), Some(5));
        assert_eq!(step(&g, Some(4), Dir::Left), Some(3));
        assert_eq!(step(&g, Some(0), Dir::Left), Some(0));
        assert_eq!(step(&g, Some(10), Dir::Right), Some(10));
    }

    #[test]
    fn up_and_down_keep_the_column_and_stay_put_at_the_edge() {
        let g = grid();
        assert_eq!(step(&g, Some(4), Dir::Down), Some(7));
        assert_eq!(step(&g, Some(4), Dir::Up), Some(1));
        assert_eq!(step(&g, Some(1), Dir::Up), Some(1));
        // The last row is short: from the third column go to the nearest item below.
        assert_eq!(step(&g, Some(8), Dir::Down), Some(10), "the nearest column that exists");
        assert_eq!(step(&g, Some(10), Dir::Down), Some(10));
    }

    #[test]
    fn home_end_and_the_first_press_without_a_current_item() {
        let g = grid();
        assert_eq!(step(&g, Some(5), Dir::Home), Some(0));
        assert_eq!(step(&g, Some(5), Dir::End), Some(10));
        assert_eq!(step(&g, None, Dir::Down), Some(0));
        assert_eq!(step(&g, None, Dir::End), Some(10));
        assert_eq!(step(&[], Some(0), Dir::Right), None);
    }

    #[test]
    fn a_range_runs_either_way() {
        assert_eq!(range(2, 5).collect::<Vec<_>>(), vec![2, 3, 4, 5]);
        assert_eq!(range(5, 2).collect::<Vec<_>>(), vec![2, 3, 4, 5]);
    }
}
