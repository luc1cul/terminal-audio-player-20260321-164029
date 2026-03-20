use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserEntry {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub kind: EntryKind,
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub struct FileBrowser {
    root: PathBuf,
    expanded: BTreeSet<PathBuf>,
    visible_entries: Vec<BrowserEntry>,
    audio_playlist_cache: Vec<PathBuf>,
    selected: usize,
}

impl FileBrowser {
    pub fn new(root: PathBuf) -> io::Result<Self> {
        let root = fs::canonicalize(root)?;
        let mut expanded = BTreeSet::new();
        expanded.insert(root.clone());

        let mut browser = Self {
            root,
            expanded,
            visible_entries: Vec::new(),
            audio_playlist_cache: Vec::new(),
            selected: 0,
        };
        browser.refresh()?;
        Ok(browser)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> &[BrowserEntry] {
        &self.visible_entries
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_entry(&self) -> Option<&BrowserEntry> {
        self.visible_entries.get(self.selected)
    }

    pub fn move_down(&mut self) {
        if !self.visible_entries.is_empty() {
            self.selected = (self.selected + 1).min(self.visible_entries.len().saturating_sub(1));
        }
    }

    pub fn move_up(&mut self) {
        if !self.visible_entries.is_empty() {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    pub fn toggle_selected_directory(&mut self) -> io::Result<()> {
        let Some(entry) = self.selected_entry().cloned() else {
            return Ok(());
        };

        if entry.kind != EntryKind::Directory {
            return Ok(());
        }

        if self.expanded.contains(&entry.path) {
            self.expanded.remove(&entry.path);
        } else {
            self.expanded.insert(entry.path);
        }

        self.refresh()
    }

    pub fn expand_selected_directory(&mut self) -> io::Result<()> {
        let Some(entry) = self.selected_entry().cloned() else {
            return Ok(());
        };

        if entry.kind == EntryKind::Directory && !self.expanded.contains(&entry.path) {
            self.expanded.insert(entry.path);
            self.refresh()?;
        }

        Ok(())
    }

    pub fn collapse_selected_directory_or_parent(&mut self) -> io::Result<()> {
        let Some(entry) = self.selected_entry().cloned() else {
            return Ok(());
        };

        if entry.kind == EntryKind::Directory && self.expanded.contains(&entry.path) {
            self.expanded.remove(&entry.path);
            self.refresh()?;
            return Ok(());
        }

        if let Some(parent) = entry.path.parent()
            && parent != self.root
            && self.expanded.remove(parent)
        {
            self.refresh()?;
            if let Some(index) = self
                .visible_entries
                .iter()
                .position(|visible| visible.path == parent)
            {
                self.selected = index;
            }
        }

        Ok(())
    }

    pub fn selected_audio_selection(&self) -> Option<(PathBuf, Vec<PathBuf>, usize)> {
        let selected = self.selected_entry()?;
        if selected.kind != EntryKind::File {
            return None;
        }

        let playlist = self.audio_playlist();
        let index = playlist.iter().position(|path| path == &selected.path)?;
        Some((selected.path.clone(), playlist, index))
    }

    pub fn audio_playlist(&self) -> Vec<PathBuf> {
        self.audio_playlist_cache.clone()
    }

    fn refresh(&mut self) -> io::Result<()> {
        let mut visible_entries = Vec::new();
        self.collect_children(&self.root, 0, &mut visible_entries)?;
        self.visible_entries = visible_entries;
        self.audio_playlist_cache = self.build_audio_playlist();

        if self.visible_entries.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.visible_entries.len() - 1);
        }

        Ok(())
    }

    fn build_audio_playlist(&self) -> Vec<PathBuf> {
        let mut playlist = WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .map(|entry| entry.into_path())
            .filter(|path| path.is_file() && is_audio_file(path))
            .collect::<Vec<_>>();

        sort_paths(&mut playlist);
        playlist
    }

    fn collect_children(
        &self,
        directory: &Path,
        depth: usize,
        entries: &mut Vec<BrowserEntry>,
    ) -> io::Result<()> {
        let read_dir = match fs::read_dir(directory) {
            Ok(read_dir) => read_dir,
            Err(error) if directory == self.root => return Err(error),
            Err(_) => return Ok(()),
        };

        let mut children = read_dir
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() || is_audio_file(path))
            .collect::<Vec<_>>();

        sort_paths(&mut children);

        for child in children {
            if child.is_dir() {
                let expanded = self.expanded.contains(&child);
                entries.push(BrowserEntry {
                    name: display_name(&child),
                    path: child.clone(),
                    depth,
                    kind: EntryKind::Directory,
                    expanded,
                });

                if expanded {
                    self.collect_children(&child, depth + 1, entries)?;
                }
            } else {
                entries.push(BrowserEntry {
                    name: display_name(&child),
                    path: child,
                    depth,
                    kind: EntryKind::File,
                    expanded: false,
                });
            }
        }

        Ok(())
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn sort_paths(paths: &mut [PathBuf]) {
    paths.sort_by(|left, right| match (left.is_dir(), right.is_dir()) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => display_name(left)
            .to_lowercase()
            .cmp(&display_name(right).to_lowercase()),
    });
}

pub fn is_audio_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("mp3" | "flac" | "wav" | "ogg")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn browser_lists_directories_before_audio_files() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("Album")).unwrap();
        fs::write(root.join("track1.mp3"), b"stub").unwrap();
        fs::write(root.join("notes.txt"), b"ignore").unwrap();

        let browser = FileBrowser::new(root.to_path_buf()).unwrap();
        let names = browser
            .entries()
            .iter()
            .map(|entry| (&entry.name, entry.kind, entry.depth))
            .collect::<Vec<_>>();

        assert_eq!(names.len(), 2);
        assert_eq!(names[0], (&"Album".to_string(), EntryKind::Directory, 0));
        assert_eq!(names[1], (&"track1.mp3".to_string(), EntryKind::File, 0));
    }

    #[test]
    fn expanding_directory_reveals_nested_audio_files() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("Album")).unwrap();
        fs::write(root.join("Album").join("track2.ogg"), b"stub").unwrap();
        fs::write(root.join("track1.mp3"), b"stub").unwrap();

        let mut browser = FileBrowser::new(root.to_path_buf()).unwrap();
        browser.toggle_selected_directory().unwrap();

        let entries = browser.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "Album");
        assert_eq!(entries[0].kind, EntryKind::Directory);
        assert!(entries[0].expanded);
        assert_eq!(entries[1].name, "track2.ogg");
        assert_eq!(entries[1].kind, EntryKind::File);
        assert_eq!(entries[1].depth, 1);
        assert_eq!(entries[2].name, "track1.mp3");
    }

    #[test]
    fn audio_playlist_includes_nested_files_even_when_directory_is_collapsed() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("Album")).unwrap();
        fs::write(root.join("Album").join("track2.ogg"), b"stub").unwrap();
        fs::write(root.join("track1.mp3"), b"stub").unwrap();

        let browser = FileBrowser::new(root.to_path_buf()).unwrap();
        let playlist = browser.audio_playlist();

        assert_eq!(playlist.len(), 2);
        assert!(playlist.iter().any(|path| path.ends_with("track1.mp3")));
        assert!(playlist.iter().any(|path| path.ends_with("track2.ogg")));
    }
}
