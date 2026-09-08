/// Convierte un texto libre en un slug apto para nombre de archivo:
/// alfanumérico en minúsculas, separado por guiones, sin repeticiones.
pub fn slugify(input: &str) -> String {
    let normalized: String = input
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let collapsed = normalized
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        "ariadne".to_string()
    } else {
        collapsed
    }
}
