pub struct Config {
    pub host: String,
    pub port: u16,
    secret: String,
}

pub struct Point(pub i32, pub i32);

pub struct Marker;

struct Private {
    pub x: i32,
}
