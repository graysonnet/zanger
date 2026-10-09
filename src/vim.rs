use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Pending,
    Top,
    Bottom,
    Page { down: bool, half: bool },
    PaneLeft,
    PaneRight,
    PaneNext,
    Git,
    Fold,
    Open,
    Close,
    ExpandAll,
    CollapseAll,
}

#[derive(Default)]
pub struct VimKeys {
    prefix: Option<char>,
}

impl VimKeys {
    pub fn pending(&self) -> bool {
        self.prefix.is_some()
    }
    pub fn reset(&mut self) {
        self.prefix = None;
    }

    /// Call only in navigation modes. Prefixes consume their next key and never
    /// leak into popups or text inputs; modified letters cannot run plain keys.
    pub fn handle(&mut self, key: KeyEvent) -> Option<Action> {
        if let Some(prefix) = self.prefix.take() {
            if prefix == 'w'
                && key.modifiers == KeyModifiers::CONTROL
                && key.code == KeyCode::Char('w')
            {
                return Some(Action::PaneNext);
            }
            if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                return Some(Action::Pending);
            }
            return Some(match (prefix, key.code) {
                ('g', KeyCode::Char('g')) => Action::Top,
                ('g', KeyCode::Char('b')) => Action::Git,
                ('z', KeyCode::Char('a')) => Action::Fold,
                ('z', KeyCode::Char('o')) => Action::Open,
                ('z', KeyCode::Char('c')) => Action::Close,
                ('z', KeyCode::Char('R')) => Action::ExpandAll,
                ('z', KeyCode::Char('M')) => Action::CollapseAll,
                ('w', KeyCode::Char('h' | 'k')) => Action::PaneLeft,
                ('w', KeyCode::Char('l' | 'j')) => Action::PaneRight,
                ('w', KeyCode::Char('w')) => Action::PaneNext,
                _ => Action::Pending,
            });
        }
        if key.modifiers == KeyModifiers::CONTROL {
            return match key.code {
                KeyCode::Char('d' | 'u' | 'f' | 'b') => Some(Action::Page {
                    down: matches!(key.code, KeyCode::Char('d' | 'f')),
                    half: matches!(key.code, KeyCode::Char('d' | 'u')),
                }),
                KeyCode::Char('w') => {
                    self.prefix = Some('w');
                    Some(Action::Pending)
                }
                _ => None,
            };
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        match key.code {
            KeyCode::Char('g' | 'z') => {
                if let KeyCode::Char(c) = key.code {
                    self.prefix = Some(c);
                }
                Some(Action::Pending)
            }
            KeyCode::Char('G') => Some(Action::Bottom),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sequences_do_not_overlap_or_survive_reset() {
        let mut keys = VimKeys::default();
        let plain = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        assert_eq!(keys.handle(plain('g')), Some(Action::Pending));
        assert_eq!(keys.handle(plain('g')), Some(Action::Top));
        keys.handle(plain('g'));
        assert_eq!(keys.handle(plain('b')), Some(Action::Git));
        keys.handle(plain('g'));
        keys.reset();
        assert_eq!(keys.handle(plain('b')), None);
        keys.handle(plain('z'));
        assert_eq!(keys.handle(plain('q')), Some(Action::Pending));
        assert_eq!(keys.handle(plain('a')), None);
        assert_eq!(
            keys.handle(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)),
            Some(Action::Page {
                down: true,
                half: false
            })
        );
        assert_eq!(
            keys.handle(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::ALT)),
            None
        );
    }
}
