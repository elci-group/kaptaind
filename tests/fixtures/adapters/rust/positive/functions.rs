pub fn greet(name: &str, count: usize) -> String {
    format!("{name}:{count}")
}

pub async fn fetch(url: &str) -> String {
    url.to_string()
}

fn private_helper() {}
