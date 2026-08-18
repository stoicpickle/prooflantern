use std::{
    fs, io,
    path::{Path, PathBuf},
};

use cap_std::{ambient_authority, fs::Dir};

pub(crate) const CONFIG_DIR: &str = ".proof-lantern";

pub(crate) struct ProjectDirectory {
    pub(crate) canonical_root: PathBuf,
    pub(crate) root: Dir,
}

impl ProjectDirectory {
    pub(crate) fn open(root: impl AsRef<Path>) -> io::Result<Self> {
        let canonical_root = fs::canonicalize(root)?;
        if !canonical_root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "project path is not a directory",
            ));
        }
        let root = Dir::open_ambient_dir(&canonical_root, ambient_authority())?;
        Ok(Self {
            canonical_root,
            root,
        })
    }

    pub(crate) fn config_path(&self) -> PathBuf {
        self.canonical_root.join(CONFIG_DIR)
    }

    pub(crate) fn open_config(&self) -> io::Result<Dir> {
        self.root.open_dir(CONFIG_DIR)
    }
}
