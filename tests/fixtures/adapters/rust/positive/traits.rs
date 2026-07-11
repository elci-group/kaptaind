pub trait Handler {
    type Output;

    fn handle(&self, input: &[u8]) -> u32;
    fn name(&self) -> &str;
}

trait Private {
    fn m(&self);
}
