//! Going straight to virtual desktop N. Windows has no call for it that is safe to use, but it has the
//! keys Win+Ctrl+Left and Win+Ctrl+Right: the jump is a number of those presses. Pure.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
}

/// How to get from desktop `current` to desktop `target` (both 0-based): which way and how many
/// presses. `None` when there is nothing to do (already there, or `target` does not exist among
/// `count` desktops).
pub fn plan(current: usize, target: usize, count: usize) -> Option<(Direction, u32)> {
    if target >= count || current >= count || current == target {
        return None;
    }
    Some(if target > current { (Direction::Right, (target - current) as u32) } else { (Direction::Left, (current - target) as u32) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_jump_is_the_distance_in_the_right_direction() {
        assert_eq!(plan(0, 3, 5), Some((Direction::Right, 3)));
        assert_eq!(plan(4, 1, 5), Some((Direction::Left, 3)));
        assert_eq!(plan(2, 2, 5), None, "already there");
        assert_eq!(plan(1, 7, 5), None, "no such desktop");
        assert_eq!(plan(9, 1, 5), None, "a current desktop that is not in the list");
    }
}
