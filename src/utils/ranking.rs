pub fn jaccard_similarity(a: &str, b: &str) -> f32 {
    let tokenize = |text: &str| {
        text.split_whitespace()
            .map(|token| token.to_lowercase())
            .collect::<Vec<_>>()
    };

    let a_tokens = tokenize(a);
    let b_tokens = tokenize(b);

    if a_tokens.is_empty() || b_tokens.is_empty() {
        return 0.0;
    }

    let a_set: std::collections::HashSet<_> = a_tokens.iter().collect();
    let b_set: std::collections::HashSet<_> = b_tokens.iter().collect();

    let intersection = a_set.intersection(&b_set).count() as f32;
    let union = a_set.union(&b_set).count() as f32;

    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}
