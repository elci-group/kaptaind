use git2::{Diff, DiffOptions, Repository};
use std::path::Path;

pub struct Repo {
    pub inner: Repository,
}

impl Repo {
    pub fn open(path: &Path) -> Result<Self, git2::Error> {
        Ok(Self {
            inner: Repository::open(path)?,
        })
    }

    pub fn diff_workdir(&self) -> Result<Diff<'_>, git2::Error> {
        let head = match self.inner.head() {
            Ok(head) => Some(head.peel_to_tree()?),
            Err(_) => None, // Unborn branch (empty repo)
        };

        let mut opts = DiffOptions::new();
        self.inner.diff_tree_to_workdir_with_index(head.as_ref(), Some(&mut opts))
    }

    pub fn is_clean(&self) -> Result<bool, git2::Error> {
        let statuses = self.inner.statuses(None)?;
        Ok(statuses.is_empty())
    }
}
