pub fn cache_key(parts: &[&str]) -> String {
    let mut combined = String::new();
    for part in parts {
        // Preserve boundaries so different system/message pairs cannot share a key.
        combined.push_str(&part.len().to_string());
        combined.push(':');
        combined.push_str(part);
    }
    format!("{:x}", md5::compute(combined.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_keys_preserve_field_boundaries() {
        assert_ne!(cache_key(&["ab", "c"]), cache_key(&["a", "bc"]));
        assert_ne!(cache_key(&["", "hi"]), cache_key(&["hi", ""]));
    }
}
