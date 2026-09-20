use super::*;

pub(crate) fn authorized_get(
    client: &Client,
    url: &str,
    settings: &Settings,
) -> Result<RequestBuilder, String> {
    validate_search_request_url(url)?;
    let request = client
        .get(url)
        .header("Accept", "application/vnd.github+json");
    Ok(if should_attach_github_token(url, settings) {
        request.bearer_auth(settings.github_token.trim())
    } else {
        request
    })
}

/// 用 URL 解析比较 host，而非字符串包含检查。
/// 配置的 CRAN 镜像被解析后只取 host 部分（去除结尾 `/`），与请求 URL
/// 的 host 做大小写不敏感匹配；任一侧解析失败均判定为非镜像请求。
fn is_cran_mirror_request_url(url: &str, cran_mirror: &str) -> bool {
    let Ok(parsed_request) = url::Url::parse(url) else {
        return false;
    };
    let Some(request_host) = parsed_request.host_str() else {
        return false;
    };
    let normalized_mirror = cran_mirror.trim_end_matches('/');
    let Ok(parsed_mirror) = url::Url::parse(normalized_mirror) else {
        return false;
    };
    let Some(mirror_host) = parsed_mirror.host_str() else {
        return false;
    };
    request_host.eq_ignore_ascii_case(mirror_host)
}

#[cfg(test)]
type MockGetText = Box<dyn FnMut(&str) -> Result<Option<String>, String>>;

#[cfg(test)]
thread_local! {
    pub static MOCK_GET_TEXT: std::cell::RefCell<Option<MockGetText>> = std::cell::RefCell::new(None);
}

pub(crate) async fn get_text(
    context: &mut SearchContext<'_>,
    url: &str,
) -> Result<Option<String>, String> {
    #[cfg(test)]
    {
        let mock_result = MOCK_GET_TEXT.with(|mock| mock.borrow_mut().as_mut().map(|f| f(url)));
        if let Some(result) = mock_result {
            return result;
        }
    }

    if context.is_stopped() {
        return Ok(None);
    }
    let is_cran_mirror_request = is_cran_mirror_request_url(url, &context.settings.cran_mirror);
    let validation = if is_cran_mirror_request {
        validate_search_request_url_with_mirror(url, Some(&context.settings.cran_mirror))
    } else {
        validate_search_request_url(url)
    };
    if let Err(error) = validation {
        context.log(&error);
        return Err(error);
    }
    if !context.acquire_request_budget() {
        return Ok(None);
    }
    let response = send_request(context, context.client.get(url)).await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        if response.status() == StatusCode::FORBIDDEN && url.contains("api.github.com") {
            context.log("GitHub API 返回 HTTP 403，可能触发频率限制或 Token 权限不足");
            return Err("GitHub API 返回 HTTP 403，可能触发频率限制或 Token 权限不足".to_string());
        }
        return Err(format!("HTTP错误: {}", response.status()));
    }
    let text = read_limited_text(
        response,
        MAX_TEXT_RESPONSE_BYTES,
        context.cancelled,
        context.budget,
        context.deadline,
    )
    .await?;
    Ok(Some(text))
}

#[cfg(test)]
type MockGetJson = Box<dyn FnMut(&str) -> Result<Option<Value>, String>>;

#[cfg(test)]
thread_local! {
    pub static MOCK_GET_JSON: std::cell::RefCell<Option<MockGetJson>> = std::cell::RefCell::new(None);
}

pub(crate) async fn get_json(
    context: &mut SearchContext<'_>,
    url: &str,
) -> Result<Option<Value>, String> {
    #[cfg(test)]
    {
        let mock_result = MOCK_GET_JSON.with(|mock| mock.borrow_mut().as_mut().map(|f| f(url)));
        if let Some(result) = mock_result {
            return result;
        }
    }

    if context.is_stopped() {
        return Ok(None);
    }
    let request = match authorized_get(context.client, url, context.settings) {
        Ok(request) => request,
        Err(error) => {
            context.log(&error);
            return Err(error);
        }
    };
    if !context.acquire_request_budget() {
        return Ok(None);
    }
    let response = send_request(context, request).await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("HTTP错误: {}", response.status()));
    }
    let text = read_limited_text(
        response,
        MAX_JSON_RESPONSE_BYTES,
        context.cancelled,
        context.budget,
        context.deadline,
    )
    .await?;
    serde_json::from_str(&text)
        .map_err(|e| format!("JSON解析失败: {}", e))
        .map(Some)
}

pub(crate) async fn read_limited_text(
    mut response: reqwest::Response,
    limit: usize,
    cancelled: &AtomicBool,
    budget: &RequestBudget,
    deadline: Instant,
) -> Result<String, String> {
    if let Some(length) = response.content_length() {
        if length > limit as u64 {
            return Err("响应内容超过大小限制".to_string());
        }
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = await_or_stop(response.chunk(), cancelled, budget, deadline)
        .await?
        .map_err(|_| "读取响应失败".to_string())?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err("响应内容超过大小限制".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| "响应不是有效 UTF-8".to_string())
}

pub(crate) fn should_attach_github_token(url: &str, settings: &Settings) -> bool {
    !settings.github_token.trim().is_empty()
        && Url::parse(url).ok().is_some_and(|parsed| {
            parsed
                .host_str()
                .is_some_and(|host| host == "api.github.com")
                && validate_search_request_url(url).is_ok()
        })
}

#[cfg(test)]
mod cran_mirror_request_detection_tests {
    use super::is_cran_mirror_request_url;

    #[test]
    fn same_host_matches_regardless_of_path() {
        assert!(is_cran_mirror_request_url(
            "https://cloud.r-project.org/web/packages/dplyr/index.html",
            "https://cloud.r-project.org",
        ));
        assert!(is_cran_mirror_request_url(
            "https://cloud.r-project.org/src/contrib/Archive/dplyr/dplyr_1.0.0.tar.gz",
            "https://cloud.r-project.org/",
        ));
    }

    #[test]
    fn different_host_does_not_match() {
        assert!(!is_cran_mirror_request_url(
            "https://example.com/web/packages/dplyr/index.html",
            "https://cloud.r-project.org",
        ));
    }

    #[test]
    fn case_insensitive_host_match() {
        assert!(is_cran_mirror_request_url(
            "https://CLOUD.R-PROJECT.ORG/web/packages/dplyr",
            "https://cloud.r-project.org",
        ));
    }

    #[test]
    fn invalid_url_returns_false() {
        assert!(!is_cran_mirror_request_url(
            "not a url",
            "https://cloud.r-project.org"
        ));
        assert!(!is_cran_mirror_request_url(
            "https://example.com",
            "not a url"
        ));
    }
}
