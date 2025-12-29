pub fn cache_key(parts: &[&str]) -> String {
    let mut combined = String::new();
    for part in parts {
        combined.push_str(part);
    }
    format!("{:x}", md5::compute(combined.as_bytes()))
}
