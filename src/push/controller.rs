use git2::{Direction, PushOptions, Repository};

pub fn push(repo: &Repository, branch: &str) -> Result<(), git2::Error> {
    let mut remote = repo.find_remote("origin")?;
    remote.connect(Direction::Push)?;

    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    let mut options = PushOptions::new();
    remote.push(&[&refspec], Some(&mut options))?;

    Ok(())
}
