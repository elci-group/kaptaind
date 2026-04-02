use git2::Repository;
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

    pub fn is_clean(&self) -> Result<bool, git2::Error> {
        let statuses = self.inner.statuses(None)?;
        Ok(statuses.is_empty())
    }
}
