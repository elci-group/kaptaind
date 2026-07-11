pub trait Pool {
    const MAX_CONN: usize;
    type Conn;

    fn acquire(&self) -> u32;
}
