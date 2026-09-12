use crate::models::MAX_FIELD_CHARS;
use url::Url;

pub fn normalize_http_url(value: &str, field_name: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field_name}不能为空"));
    }
    if trimmed.len() > MAX_FIELD_CHARS || trimmed.chars().any(|character| character.is_control()) {
        return Err(format!("{field_name}包含非法字符或长度过长"));
    }
    let parsed = Url::parse(trimmed).map_err(|_| format!("{field_name}必须是有效 URL"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(format!("{field_name}仅支持 http 或 https")),
    }
    if parsed.host_str().is_none() {
        return Err(format!("{field_name}缺少主机名"));
    }
    if parsed.port().is_some() || url_has_explicit_port(trimmed) {
        return Err(format!("{field_name}不允许包含显式端口"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(format!("{field_name}不允许包含用户名或密码"));
    }
    Ok(parsed.to_string())
}

pub fn normalize_https_url(value: &str, field_name: &str) -> Result<String, String> {
    let normalized = normalize_http_url(value, field_name)?;
    if Url::parse(&normalized)
        .map_err(|_| format!("{field_name}必须是有效 URL"))?
        .scheme()
        != "https"
    {
        return Err(format!("{field_name}仅支持 https"));
    }
    Ok(normalized)
}

pub fn normalize_cran_mirror_url(value: &str) -> Result<String, String> {
    let normalized = normalize_https_url(value, "CRAN 镜像")?;
    let parsed = Url::parse(&normalized).map_err(|_| "CRAN 镜像必须是有效 URL".to_string())?;
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("CRAN 镜像不允许包含查询参数或片段".to_string());
    }
    Ok(format!("{}/", normalized.trim_end_matches('/')))
}

pub fn url_has_explicit_port(value: &str) -> bool {
    let trimmed = value.trim();
    let Some(scheme_end) = trimmed.find("://") else {
        return false;
    };
    let authority_start = scheme_end + 3;
    let authority_end = trimmed[authority_start..]
        .find(['/', '?', '#'])
        .map(|index| authority_start + index)
        .unwrap_or(trimmed.len());
    let authority = &trimmed[authority_start..authority_end];
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, host_port)| host_port)
        .unwrap_or(authority);
    if let Some(rest) = host_port.strip_prefix('[') {
        return rest
            .find(']')
            .is_some_and(|index| rest[index + 1..].starts_with(':'));
    }
    host_port.contains(':')
}
