pub trait Handler {
    fn handle(&self);
    fn name(&self) -> &str;
}
