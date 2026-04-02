use git2::Repository;

pub fn commit(repo: &Repository, msg: &str) -> Result<(), git2::Error> {
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;

    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;

    let sig = repo.signature()?;
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());

    match parent {
        Some(head) => {
            repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&head])?;
        }
        None => {
            repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[])?;
        }
    }

    Ok(())
}
