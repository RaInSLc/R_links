use crate::models::normalize_cran_mirror_url;
const MAX_GENERATE_METHOD_CHARS: usize = 32;
use crate::logic::*;
use crate::models::{GenerateOptions, InputRules, PackageInput, SearchResult};

#[cfg(test)]
pub fn generate_script(
    input: &str,
    options: &GenerateOptions,
    results: &[SearchResult],
) -> Result<String, String> {
    generate_script_with_remote_versions(input, options, results, true)
}

pub fn generate_script_with_rules(
    input: &str,
    options: &GenerateOptions,
    results: &[SearchResult],
    show_remote_version: bool,
    rules: &InputRules,
) -> Result<String, String> {
    let requested_method = normalize_generate_method(&options.method)?;
    validate_search_results_count(results)?;
    let packages = parse_inputs_filtered(input, rules)?;
    generate_script_inner(
        options,
        results,
        show_remote_version,
        requested_method,
        packages,
    )
}

#[cfg(test)]
pub fn generate_script_with_remote_versions(
    input: &str,
    options: &GenerateOptions,
    results: &[SearchResult],
    show_remote_version: bool,
) -> Result<String, String> {
    let requested_method = normalize_generate_method(&options.method)?;
    validate_search_results_count(results)?;
    let packages = parse_inputs(input)?;
    generate_script_inner(
        options,
        results,
        show_remote_version,
        requested_method,
        packages,
    )
}

pub(crate) fn generate_script_inner(
    options: &GenerateOptions,
    results: &[SearchResult],
    show_remote_version: bool,
    requested_method: &str,
    packages: Vec<PackageInput>,
) -> Result<String, String> {
    if packages.is_empty() {
        return Ok("等待输入...".to_string());
    }
    let results = sanitize_search_results(results);

    let mirror = if options.mirror.trim().is_empty() {
        "https://cloud.r-project.org".to_string()
    } else {
        normalize_cran_mirror_url(&options.mirror)?
    };
    let binary_mirror = is_binary_package_mirror(&mirror);

    let packages_for_verify = if options.append_verify {
        packages.clone()
    } else {
        Vec::new()
    };

    if requested_method == "checkSystem" {
        return generate_check_system_script(&packages);
    }

    let mut output = Vec::new();
    if options.parallel_install {
        output.push("options(Ncpus = parallel::detectCores())".to_string());
    }
    if binary_mirror {
        output.push(format!(
            "# [RSPM 二进制镜像: {mirror} | 由 R 按当前平台选择预编译包]"
        ));
        let user_library = if options.r_lib_path.trim().is_empty() {
            "file.path(path.expand(\"~\"), \"R\", \"library\")".to_string()
        } else {
            format!("\"{}\"", escape_r(options.r_lib_path.trim()))
        };
        output.push(format!(
            "user_library <- {user_library}\nif (!dir.exists(user_library)) dir.create(user_library, recursive = TRUE, showWarnings = FALSE)\n.libPaths(unique(c(user_library, .libPaths())))"
        ));
        output.push(
            "options(pkgType = if (.Platform$OS.type == \"windows\") \"win.binary\" else if (identical(Sys.info()[[\"sysname\"]], \"Darwin\")) \"mac.binary\" else \"source\")"
                .to_string(),
        );
    }
    for package in packages {
        let mut is_cran_archive = false;
        let is_archive_url = (package.raw.starts_with("http://")
            || package.raw.starts_with("https://"))
            && normalize_github_repository(&package.raw).is_none();
        let is_local_archive = package.source_hint.as_deref() == Some("local");
        if is_archive_url && !matches!(requested_method, "auto" | "devtools" | "remotes") {
            return Err(format!(
                "安装归档 URL 仅支持智能路由、devtools 或 remotes，不能使用 {requested_method}"
            ));
        }
        let mut value = if is_local_archive
            || matches!(requested_method, "devtools" | "remotes")
            || (requested_method == "auto" && is_archive_url)
        {
            package.raw.clone()
        } else {
            package.name.clone()
        };
        let mut version = package.version.clone();
        let mut method = if is_local_archive {
            "local".to_string()
        } else if requested_method == "auto" && is_archive_url {
            "remotes".to_string()
        } else {
            requested_method.to_string()
        };

        if let Some(best) = (!is_archive_url)
            .then(|| choose_best_result(&package.name, &results, package.source_hint.as_deref()))
            .flatten()
        {
            let archive_github_decision = archive_github_decision(
                &package.name,
                best,
                &results,
                options.archive_github_major_gap,
            );
            let best = archive_github_decision
                .as_ref()
                .filter(|decision| decision.use_github)
                .map(|decision| decision.github)
                .unwrap_or(best);
            is_cran_archive = best.source == "cran" && is_cran_archive_result(best);
            let source_label = source_label(&best.source);
            let remote_version = if show_remote_version {
                format!(": v{}", best.latest_version)
            } else {
                String::new()
            };
            if version.is_empty() {
                let status_text = if is_cran_archive {
                    format!("已下架并归档: v{}", best.latest_version)
                } else {
                    format!("已验证{remote_version}")
                };
                output.push(format!("# [{source_label} {status_text} | 自动同步]"));
                if let Some(decision) = archive_github_decision.as_ref() {
                    output.push(decision.comment());
                }
                if (show_remote_version || is_cran_archive)
                    && is_clean_version(&best.latest_version)
                    && best.source != "github"
                {
                    version = best.latest_version.clone();
                }
            } else {
                let status_text = if is_cran_archive {
                    format!("已下架并归档: v{}", best.latest_version)
                } else {
                    format!("最新版本{remote_version}")
                };
                output.push(format!("# [{source_label} {status_text} | 保留指定版本]"));
                if let Some(decision) = archive_github_decision.as_ref() {
                    output.push(decision.comment());
                }
                if best.source == "bioc"
                    && !best.latest_version.is_empty()
                    && best.latest_version != version
                {
                    output.push(format!(
                        "# [提示: Bioconductor 未匹配版本 {version}，将使用 Release]"
                    ));
                }
            }

            if requested_method == "auto" {
                match best.source.as_str() {
                    "biocGit" => {
                        method = "biocGit".to_string();
                        let matched_version = if show_remote_version {
                            best.latest_version.as_str()
                        } else {
                            ""
                        };
                        version = format!("{matched_version}|{}", best.repository);
                    }
                    "bioc" => method = "biocManager".to_string(),
                    "github" => {
                        method = "github".to_string();
                        if !best.repository.is_empty() {
                            value = best.repository.clone();
                        }
                    }
                    "r-forge" => method = "rForge".to_string(),
                    "cran" if is_cran_archive && best.repository.starts_with("https://") => {
                        method = "remotes".to_string();
                        value = best.repository.clone();
                        version.clear();
                    }
                    "cran-binary" => method = "base".to_string(),
                    "cran" if binary_mirror && package.version.is_empty() => {
                        method = "base".to_string();
                        version.clear();
                    }
                    _ => method = "remotesVersion".to_string(),
                }
            }
        } else if !is_archive_url
            && results
                .iter()
                .any(|result| result.package.eq_ignore_ascii_case(&package.name))
        {
            output.push("# [提示: CRAN/Bioconductor/GitHub 均未找到]".to_string());
        }

        if requested_method == "auto"
            && !is_archive_url
            && !is_local_archive
            && !matches!(
                method.as_str(),
                "github" | "biocManager" | "biocGit" | "rForge" | "remotes"
            )
        {
            method = if package.name.contains('/')
                && normalize_github_repository(&package.name).is_some()
            {
                "github".to_string()
            } else if is_clean_version(&version) {
                "remotesVersion".to_string()
            } else {
                "base".to_string()
            };
        }

        let command_mirror = if is_cran_archive {
            "https://cloud.r-project.org".to_string()
        } else {
            mirror.clone()
        };

        if is_local_archive {
            output.push(format!(
                "install.packages(\"{}\", repos = NULL, type = \"source\"{})",
                escape_r(&value),
                if options.r_lib_path.trim().is_empty() {
                    String::new()
                } else {
                    format!(", lib = \"{}\"", escape_r(options.r_lib_path.trim()))
                }
            ));
            continue;
        }
        output.push(generate_command_with_lib(
            &value,
            &method,
            &version,
            options.conditional,
            &command_mirror,
            options.install_dependencies,
            options.r_lib_path.trim(),
        )?);
    }

    let mut script = output.join("\r\n") + "\r\n";
    if options.append_verify && !packages_for_verify.is_empty() {
        let verify = generate_verify_script(&packages_for_verify);
        script.push_str(&verify);
    }
    validate_script_size(&script)?;
    Ok(script)
}

pub(crate) fn is_binary_package_mirror(mirror: &str) -> bool {
    let normalized = mirror.trim().to_ascii_lowercase();
    normalized.contains("packagemanager.posit.co")
        || normalized.contains("posit.co/rspm")
        || normalized.contains("/rspm/")
}

pub(crate) fn generate_check_system_script(packages: &[PackageInput]) -> Result<String, String> {
    let names = packages
        .iter()
        .map(|item| format!("\"{}\"", escape_r(&local_package_name(&item.name))))
        .collect::<Vec<_>>()
        .join(", ");
    let script = format!(
        "# 1. 定义需要检测的包列表\r\n\
         packages_to_check <- c({names})\r\n\r\n\
         # 2. 逐个检测包是否已安装，并尝试加载捕获报错\r\n\
         check_results <- lapply(packages_to_check, function(p) {{\r\n\
         \x20 installed <- requireNamespace(p, quietly = TRUE)\r\n\
         \x20 version <- if (installed) tryCatch(as.character(packageVersion(p)), error = function(e) NA_character_) else NA_character_\r\n\
         \x20 load_error <- NA_character_\r\n\
         \x20 loaded <- FALSE\r\n\
         \x20 if (installed) {{\r\n\
         \x20   loaded <- tryCatch({{\r\n\
         \x20     suppressPackageStartupMessages(library(p, character.only = TRUE))\r\n\
         \x20     TRUE\r\n\
         \x20   }}, error = function(e) {{\r\n\
         \x20     load_error <<- conditionMessage(e)\r\n\
         \x20     FALSE\r\n\
         \x20   }})\r\n\
         \x20 }}\r\n\
         \x20 data.frame(package = p, installed = installed, loaded = loaded, version = version, error = load_error, stringsAsFactors = FALSE)\r\n\
         }})\r\n\r\n\
         # 3. 汇总输出检测结果\r\n\
         check_results <- do.call(rbind, check_results)\r\n\
         print(check_results, row.names = FALSE)\r\n\r\n\
         failed <- check_results[!check_results$installed | !check_results$loaded, ]\r\n\
         cat(sprintf(\"\\n=== 检测完成: %d/%d 包可正常加载 ===\\n\", sum(check_results$installed & check_results$loaded), nrow(check_results)))\r\n\
         if (nrow(failed) > 0) {{\r\n\
         \x20 cat(\"以下包未安装或加载报错：\\n\")\r\n\
         \x20 print(failed, row.names = FALSE)\r\n\
         }}\r\n"
    );
    validate_script_size(&script)?;
    Ok(script)
}

pub(crate) fn generate_verify_script(packages: &[PackageInput]) -> String {
    let names = packages
        .iter()
        .map(|item| format!("\"{}\"", escape_r(&local_package_name(&item.name))))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "\r\n# ===== 安装结果验证 =====\r\n\
         packages <- c({names})\r\n\
         results <- sapply(packages, function(p) {{\r\n\
         \x20 ver <- tryCatch(as.character(packageVersion(p)), error = function(e) NA)\r\n\
         \x20 if (!is.na(ver)) {{\r\n\
         \x20   cat(sprintf(\"[OK] %s (v%s)\\n\", p, ver))\r\n\
         \x20   return(TRUE)\r\n\
         \x20 }} else {{\r\n\
         \x20   cat(sprintf(\"[FAIL] %s\\n\", p))\r\n\
         \x20   return(FALSE)\r\n\
         \x20 }}\r\n\
         }})\r\n\
         cat(sprintf(\"\\n=== 验证完成: %d/%d 包安装成功 ===\\n\", sum(results, na.rm=TRUE), length(results)))\r\n"
    )
}

pub(crate) fn normalize_generate_method(value: &str) -> Result<&'static str, String> {
    if value.len() > MAX_GENERATE_METHOD_CHARS
        || value.chars().any(|character| character.is_control())
    {
        return Err("安装方式无效".to_string());
    }
    match value.trim() {
        "auto" => Ok("auto"),
        "devtools" => Ok("devtools"),
        "remotes" => Ok("remotes"),
        "github" => Ok("github"),
        "base" => Ok("base"),
        "version" => Ok("version"),
        "biocManager" => Ok("biocManager"),
        "checkSystem" => Ok("checkSystem"),
        _ => Err("不支持的安装方式".to_string()),
    }
}
