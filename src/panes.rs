use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};

/// Named for the divider, matching Vim's :vsplit and :split.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

// Remember the preferred split separately from the current terminal's bounds.
#[derive(Default)]
pub struct PaneSplit {
    preferred_left: Option<u16>,
    preferred_top: Option<u16>,
    orientation: Orientation,
    visible: Option<(u16, u16)>,
}

impl PaneSplit {
    pub fn set_orientation(&mut self, orientation: Orientation) {
        self.orientation = orientation;
        self.hide();
    }

    pub fn stacked(&self) -> bool {
        self.orientation == Orientation::Horizontal
    }

    /// Returns no split when the focused pane should fill a small terminal.
    pub fn areas(
        &mut self,
        area: Rect,
        default_width: u16,
        gap: u16,
        minimum_width: u16,
    ) -> Option<[Rect; 2]> {
        if (self.stacked() && (area.height < 12 + gap || area.width < 24))
            || (!self.stacked() && area.width < minimum_width)
        {
            self.hide();
            return None;
        }
        let (available, default) = if self.stacked() {
            (area.height.saturating_sub(gap), area.height / 2)
        } else {
            (area.width.saturating_sub(gap), default_width)
        };
        let size = self.first_size(available, default);
        let constraints = [
            Constraint::Length(size),
            Constraint::Length(gap),
            Constraint::Min(0),
        ];
        let [first, _, second] = if self.stacked() {
            Layout::vertical(constraints).areas(area)
        } else {
            Layout::horizontal(constraints).areas(area)
        };
        Some([first, second])
    }

    fn first_size(&mut self, available: u16, default: u16) -> u16 {
        let minimum = self.minimum().min(available / 2);
        let width = self
            .preference()
            .unwrap_or(default)
            .clamp(minimum, available - minimum);
        self.visible = Some((available, width));
        width
    }

    fn minimum(&self) -> u16 {
        if self.stacked() { 6 } else { 24 }
    }

    fn preference(&mut self) -> &mut Option<u16> {
        if self.stacked() {
            &mut self.preferred_top
        } else {
            &mut self.preferred_left
        }
    }

    pub fn hide(&mut self) {
        self.visible = None;
    }

    pub fn resize(&mut self, grow: bool, right_focused: bool) {
        let Some((available, current)) = self.visible else {
            return;
        };
        let minimum = self.minimum().min(available / 2);
        let step = if self.stacked() { 2 } else { 4 };
        let width = if grow != right_focused {
            current.saturating_add(step)
        } else {
            current.saturating_sub(step)
        }
        .clamp(minimum, available - minimum);
        if width != current {
            *self.preference() = Some(width);
            self.visible = Some((available, width));
        }
    }
}

pub fn resize_direction(key: KeyEvent, stacked: bool) -> Option<bool> {
    if key.modifiers != KeyModifiers::ALT {
        return None;
    }
    match key.code {
        KeyCode::Left => Some(false),
        KeyCode::Right => Some(true),
        KeyCode::Up if stacked => Some(false),
        KeyCode::Down if stacked => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientations_keep_independent_sizes_and_fall_back_without_losing_preferences() {
        let mut split = PaneSplit::default();
        let area = Rect::new(1, 3, 118, 35);
        let [left, right] = split.areas(area, 30, 1, 76).unwrap();
        assert_eq!(left.y, right.y);
        assert_eq!(right.x, left.right() + 1);
        split.resize(true, false);
        split.set_orientation(Orientation::Horizontal);
        let [top, bottom] = split.areas(area, 30, 1, 76).unwrap();
        assert_eq!(top.width, area.width);
        assert_eq!(bottom.y, top.bottom() + 1);
        split.resize(true, true);
        assert_eq!(
            split.areas(area, 30, 1, 76).unwrap()[0].height,
            top.height - 2
        );
        split.set_orientation(Orientation::Vertical);
        assert_eq!(split.areas(area, 30, 1, 76).unwrap()[0].width, 34);
        split.set_orientation(Orientation::Horizontal);
        assert!(split.areas(Rect::new(0, 0, 60, 10), 30, 1, 76).is_none());
        split.resize(false, false);
        assert_eq!(
            split.areas(area, 30, 1, 76).unwrap()[0].height,
            top.height - 2
        );
        for _ in 0..100 {
            split.resize(true, true);
        }
        assert_eq!(split.areas(area, 30, 1, 76).unwrap()[0].height, 6);
        for (width, height) in [(0, 0), (1, 1), (24, 12), (60, 20), (120, 40)] {
            for orientation in [Orientation::Vertical, Orientation::Horizontal] {
                split.set_orientation(orientation);
                let area = Rect::new(0, 0, width, height);
                if let Some(panes) = split.areas(area, 30, 0, 78) {
                    assert!(
                        panes
                            .iter()
                            .all(|pane| pane.right() <= width && pane.bottom() <= height)
                    );
                }
            }
        }
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT);
        assert_eq!(resize_direction(down, false), None);
        assert_eq!(resize_direction(down, true), Some(true));
    }

    #[test]
    fn focused_panes_resize_with_bounds_and_restore_after_terminal_changes() {
        let mut split = PaneSplit::default();
        assert_eq!(split.first_size(117, 29), 29);
        split.resize(true, false);
        assert_eq!(split.first_size(117, 29), 33);
        split.resize(true, true);
        assert_eq!(split.first_size(117, 29), 29);
        split.resize(false, true);
        assert_eq!(split.first_size(117, 29), 33);
        split.resize(false, false);
        assert_eq!(split.first_size(117, 29), 29);
        for _ in 0..100 {
            split.resize(true, false);
        }
        assert_eq!(split.first_size(117, 29), 93);
        assert_eq!(split.first_size(75, 24), 51);
        split.hide();
        split.resize(false, false);
        assert_eq!(split.first_size(117, 29), 93);
        for _ in 0..100 {
            split.resize(false, false);
        }
        assert_eq!(split.first_size(117, 29), 24);
        split.resize(true, false);
        assert_eq!(split.first_size(117, 29), 28);
        assert_eq!(split.first_size(0, 29), 0);
    }
}
