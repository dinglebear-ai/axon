pub fn env_usize_clamped(key: &str, default: usize, min: usize, max: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value >= min)
        .unwrap_or(default)
        .clamp(min, max)
}
