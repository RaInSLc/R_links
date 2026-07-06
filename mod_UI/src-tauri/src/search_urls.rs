use crate::logic::{is_valid_package_name, normalize_github_repository};
use crate::models::url_has_explicit_port;
use url::Url;

pub(crate) fn validate_search_request_url(value: &str) -> Result<(), String> {
    let parsed = Url::parse(value).map_err(|_| "检索 URL 无效，已阻止请求".to_string())?;
    if parsed.scheme() != "https"
        || parsed.port().is_some()
        || url_has_explicit_port(value)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err("检索 URL 不在允许范围内，已阻止请求".to_string());
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "检索 URL 缺少主机名，已阻止请求".to_string())?;
    let path = parsed.path();
    let allowed = match host {
        "cloud.r-project.org" => {
            parsed.query().is_none()
                && (is_allowed_cran_package_path(&parsed) || is_allowed_cran_archive_path(&parsed))
        }
        "bioconductor.org" => parsed.query().is_none() && is_allowed_bioc_package_path(&parsed),
        "r-forge.r-project.org" => {
            parsed.query().is_none() && is_allowed_r_forge_path(&parsed)
        }
        "r-universe.dev" => path == "/api/search" && is_allowed_r_universe_query(&parsed),
        "api.github.com" => {
            path == "/search/repositories" && is_allowed_github_search_query(&parsed)
        }
        "raw.githubusercontent.com" => {
            parsed.query().is_none()
                && parsed.path_segments().is_some_and(|segments| {
                    let segments = segments.collect::<Vec<_>>();
                    segments.len() == 4
                        && normalize_github_repository(&format!("{}/{}", segments[0], segments[1]))
                            .is_some()
                        && matches!(segments[2], "HEAD" | "master" | "main" | "devel")
                        && segments[3] == "DESCRIPTION"
                })
        }
        _ => false,
    };

    if allowed {
        Ok(())
    } else {
        Err("检索 URL 不在允许范围内，已阻止请求".to_string())
    }
}

fn is_allowed_cran_package_path(url: &Url) -> bool {
    url.path_segments().is_some_and(|segments| {
        let segments = segments.collect::<Vec<_>>();
        segments.len() == 4
            && segments[0] == "web"
            && segments[1] == "packages"
            && is_valid_search_package_query(segments[2])
            && segments[3] == "index.html"
    })
}

fn is_allowed_cran_archive_path(url: &Url) -> bool {
    url.path_segments().is_some_and(|segments| {
        let segments = segments.collect::<Vec<_>>();
        (segments.len() == 4 || segments.len() == 5)
            && segments[0] == "src"
            && segments[1] == "contrib"
            && segments[2] == "Archive"
            && is_valid_search_package_query(segments[3])
            && (segments.len() == 4 || segments[4].is_empty())
    })
}

fn is_allowed_bioc_package_path(url: &Url) -> bool {
    url.path_segments().is_some_and(|segments| {
        let segments = segments.collect::<Vec<_>>();
        if segments.len() < 5
            || segments[0] != "packages"
            || !is_allowed_bioc_release_segment(segments[1])
        {
            return false;
        }
        match segments.as_slice() {
            [_, _, "bioc" | "workflows", "html", file] => is_allowed_bioc_package_file(file),
            [_, _, "data", "annotation" | "experiment", "html", file] => {
                is_allowed_bioc_package_file(file)
            }
            _ => false,
        }
    })
}

fn is_allowed_bioc_release_segment(value: &str) -> bool {
    value == "release"
        || value.split_once('.').is_some_and(|(major, minor)| {
            !major.is_empty()
                && !minor.is_empty()
                && major.chars().all(|character| character.is_ascii_digit())
                && minor.chars().all(|character| character.is_ascii_digit())
        })
}

fn is_allowed_bioc_package_file(file: &str) -> bool {
    file.strip_suffix(".html")
        .is_some_and(is_valid_search_package_query)
}

fn is_allowed_r_universe_query(url: &Url) -> bool {
    let mut pairs = url.query_pairs();
    let Some((query_key, query_value)) = pairs.next() else {
        return false;
    };
    if query_key != "q"
        || !query_value
            .as_ref()
            .strip_prefix("package:")
            .is_some_and(is_valid_search_package_query)
    {
        return false;
    }

    let Some((limit_key, limit_value)) = pairs.next() else {
        return false;
    };
    limit_key == "limit" && limit_value == "1" && pairs.next().is_none()
}

fn is_allowed_github_search_query(url: &Url) -> bool {
    let mut pairs = url.query_pairs();
    let Some((query_key, query_value)) = pairs.next() else {
        return false;
    };
    if query_key != "q"
        || !query_value
            .as_ref()
            .strip_suffix(" language:R")
            .is_some_and(is_valid_search_package_query)
    {
        return false;
    }

    let Some((sort_key, sort_value)) = pairs.next() else {
        return false;
    };
    if sort_key != "sort" || sort_value != "stars" {
        return false;
    }

    let Some((page_key, page_value)) = pairs.next() else {
        return false;
    };
    page_key == "per_page" && page_value == "10" && pairs.next().is_none()
}

fn is_valid_search_package_query(value: &str) -> bool {
    !value.contains('/') && is_valid_package_name(value)
}

fn is_allowed_r_forge_path(url: &Url) -> bool {
    url.path() == "/src/contrib/PACKAGES"
}
