pub struct Db;

impl Db {
    pub fn connect(url: &str) -> Self {
        Db
    }
    pub fn query(&self, sql: &str) -> u32 {
        0
    }
    fn internal(&self) {}
}

pub trait Repo {
    fn find(&self, id: u64) -> u32;
}

impl Repo for Db {
    fn find(&self, id: u64) -> u32 {
        0
    }
}
