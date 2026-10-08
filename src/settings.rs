use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

pub fn theme_path() -> Option<PathBuf> {
    let nonempty = |name| {
        env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(target_os = "windows") {
        nonempty("APPDATA")
    } else if cfg!(target_os = "macos") {
        nonempty("HOME").map(|path| path.join("Library/Application Support"))
    } else {
        nonempty("XDG_CONFIG_HOME")
            .filter(|path| path.is_absolute())
            .or_else(|| nonempty("HOME").map(|path| path.join(".config")))
    };
    base.map(|path| path.join("zanger").join("syntax-theme"))
}

pub fn load_theme(path: &Path) -> Option<usize> {
    let name = fs::read_to_string(path).ok()?;
    crate::syntax::SYNTAX_THEMES
        .iter()
        .position(|theme| theme.id == name.trim())
}

pub fn save_theme(path: &Path, index: usize) -> io::Result<()> {
    let theme = crate::syntax::SYNTAX_THEMES
        .get(index)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown syntax theme"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{}\n", theme.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn confirmed_theme_persists_cancel_does_not_and_invalid_value_falls_back() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("zanger-theme-{}-{nonce}", std::process::id()));
        let path = directory.join("syntax-theme");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts");
        let mut app = App::with_theme_path(root.clone(), Some(path.clone()));
        let press = |app: &mut App, code| app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
        press(&mut app, KeyCode::Char('t'));
        press(&mut app, KeyCode::Down);
        assert!(!path.exists());
        press(&mut app, KeyCode::Enter);
        assert_eq!(load_theme(&path), Some(1));
        let mut reopened = App::with_theme_path(root.clone(), Some(path.clone()));
        assert_eq!(reopened.highlighter.theme_index(), 1);
        press(&mut reopened, KeyCode::Char('t'));
        press(&mut reopened, KeyCode::Down);
        press(&mut reopened, KeyCode::Esc);
        assert_eq!(reopened.highlighter.theme_index(), 1);
        assert_eq!(load_theme(&path), Some(1));

        // A directory where a file belongs simulates a save failure on every platform.
        let mut unsavable = App::with_theme_path(root.clone(), Some(directory.clone()));
        press(&mut unsavable, KeyCode::Char('t'));
        press(&mut unsavable, KeyCode::Down);
        press(&mut unsavable, KeyCode::Enter);
        assert_eq!(unsavable.highlighter.theme_index(), 1);
        assert!(
            unsavable
                .notice
                .as_ref()
                .unwrap()
                .contains("could not save")
        );

        fs::write(&path, "unknown-theme").unwrap();
        let fallback = App::with_theme_path(root, Some(path.clone()));
        assert_eq!(fallback.highlighter.theme_index(), 0);
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
