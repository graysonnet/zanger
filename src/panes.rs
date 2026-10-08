use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// Remember the preferred split separately from the current terminal's bounds.
#[derive(Default)]
pub struct PaneSplit {
    preferred_left: Option<u16>,
    visible: Option<(u16, u16)>,
}

impl PaneSplit {
    pub fn left_width(&mut self, available: u16, default: u16) -> u16 {
        let minimum = 24.min(available / 2);
        let width = self
            .preferred_left
            .unwrap_or(default)
            .clamp(minimum, available - minimum);
        self.visible = Some((available, width));
        width
    }

    pub fn hide(&mut self) {
        self.visible = None;
    }

    pub fn resize(&mut self, grow: bool, right_focused: bool) {
        let Some((available, current)) = self.visible else {
            return;
        };
        let minimum = 24.min(available / 2);
        let width = if grow != right_focused {
            current.saturating_add(4)
        } else {
            current.saturating_sub(4)
        }
        .clamp(minimum, available - minimum);
        if width != current {
            self.preferred_left = Some(width);
            self.visible = Some((available, width));
        }
    }
}

pub fn resize_direction(key: KeyEvent) -> Option<bool> {
    if key.modifiers != KeyModifiers::ALT {
        return None;
    }
    match key.code {
        KeyCode::Left => Some(false),
        KeyCode::Right => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_panes_resize_with_bounds_and_restore_after_terminal_changes() {
        let mut split = PaneSplit::default();
        assert_eq!(split.left_width(117, 29), 29);
        split.resize(true, false);
        assert_eq!(split.left_width(117, 29), 33);
        split.resize(true, true);
        assert_eq!(split.left_width(117, 29), 29);
        split.resize(false, true);
        assert_eq!(split.left_width(117, 29), 33);
        split.resize(false, false);
        assert_eq!(split.left_width(117, 29), 29);
        for _ in 0..100 {
            split.resize(true, false);
        }
        assert_eq!(split.left_width(117, 29), 93);
        assert_eq!(split.left_width(75, 24), 51);
        split.hide();
        split.resize(false, false);
        assert_eq!(split.left_width(117, 29), 93);
        for _ in 0..100 {
            split.resize(false, false);
        }
        assert_eq!(split.left_width(117, 29), 24);
        split.resize(true, false);
        assert_eq!(split.left_width(117, 29), 28);
        assert_eq!(split.left_width(0, 29), 0);
    }
}
