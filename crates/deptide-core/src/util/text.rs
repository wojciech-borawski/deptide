pub fn slugify(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut pending_dash = false;

    for character in value.trim().chars() {
        let lower = character.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() || lower == '.' || lower == '_' || lower == '-' {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(lower);
        } else {
            pending_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

pub fn unique_name(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|known| known == name) {
        return name.to_string();
    }

    (2..)
        .map(|suffix| format!("{name}-{suffix}"))
        .find(|candidate| !taken.iter().any(|known| known == candidate))
        .expect("an unused suffix always exists")
}

pub fn strip_ansi(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut characters = line.chars().peekable();

    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            output.push(character);
            continue;
        }

        if characters.peek() == Some(&'[') {
            characters.next();
            for escaped in characters.by_ref() {
                if escaped.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }

    output
}

pub fn describe_count(count: usize, first: Option<&str>, noun: &str) -> String {
    match (count, first) {
        (1, Some(first)) => first.to_string(),
        _ => format!("{count} {noun}"),
    }
}
