use regex::Regex;

pub(crate) fn normalize_managed_package_line(line: &str) -> Option<String> {
    let normalized = line.replace(['，', '、', '；'], ",");
    let call_re = Regex::new(r"(?i)\b(?:install\.packages|BiocManager::install|pacman::p_load|renv::install|pak::pkg_install)\s*\((.*)\)").ok()?;
    let shell = Regex::new(r#"(?i)\b(?:R|Rscript)\s+-e\s+[\"'](.+)[\"']"#).ok()?;
    let body = if let Some(captures) = call_re.captures(&normalized) {
        captures.get(1)?.as_str().to_string()
    } else {
        let captures = shell.captures(&normalized)?;
        return normalize_managed_package_line(captures.get(1)?.as_str());
    };
    let quoted = Regex::new(r#"[\"']([^\"']+)[\"']"#).ok()?;
    let quoted_values = quoted
        .captures_iter(&body)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
        .collect::<Vec<_>>();
    if !quoted_values.is_empty() {
        return Some(quoted_values.join("\n"));
    }
    Some(
        body.trim()
            .trim_start_matches(|c: char| {
                c == 'c' || c == 'C' || c == 'l' || c == 'i' || c == 's' || c == 't' || c == '('
            })
            .trim_end_matches(')')
            .split([',', ';'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub(crate) fn normalize_markdown_table_line(line: &str) -> Option<String> {
    if !line.starts_with('|') || !line.ends_with('|') {
        return None;
    }
    let cells: Vec<String> = line
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect();
    if cells.is_empty() || cells.iter().all(|cell| is_markdown_separator_cell(cell)) {
        return Some(String::new());
    }
    let package = cells.first().map(String::as_str).unwrap_or_default().trim();
    if package.is_empty() || matches!(package, "包名" | "package" | "Package" | "PACKAGE") {
        return Some(String::new());
    }
    let source_hint = cells
        .get(1)
        .map(String::as_str)
        .unwrap_or_default()
        .replace(['（', '）', '(', ')'], " ");
    Some(format!("{package} {source_hint}").trim().to_string())
}

fn is_markdown_separator_cell(cell: &str) -> bool {
    let trimmed = cell.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|character| matches!(character, '-' | ':' | ' '))
        && trimmed.contains('-')
}
