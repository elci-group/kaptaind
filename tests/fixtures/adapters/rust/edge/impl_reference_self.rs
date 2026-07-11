pub struct Widget;

pub trait Draw {
    fn draw(&self);
}

impl Draw for &Widget {
    fn draw(&self) {}
}

impl Widget {
    pub fn inherent(&self) {}
}
