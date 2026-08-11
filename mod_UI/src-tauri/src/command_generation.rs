use crate::logic::*;

#[cfg(test)]
pub(crate) fn generate_command(
    value: &str,
    method: &str,
    version: &str,
    conditional: bool,
    mirror: &str,
    install_dependencies: bool,
) -> Result<String, String> {
    generate_command_with_lib(
        value,
        method,
        version,
        conditional,
        mirror,
        install_dependencies,
        "",
    )
}

pub(crate) fn generate_command_with_lib(
    value: &str,
    method: &str,
    version: &str,
    conditional: bool,
    mirror: &str,
    install_dependencies: bool,
    r_lib_path: &str,
) -> Result<String, String> {
    let dependencies = if install_dependencies {
        "TRUE"
    } else {
        "FALSE"
    };
    let local_value = local_package_name(value);
    let escaped_value = escape_r(&local_value);
    let escaped_mirror = escape_r(mirror);
    let lib_arg = if r_lib_path.is_empty() {
        String::new()
    } else {
        format!(", lib = \"{}\"", escape_r(r_lib_path))
    };
    let mut package_name = local_package_name(&extract_package_name(value));
    let mut effective_version = version.to_string();

    let raw = match method {
        "devtools" => {
            let url = normalize_install_archive_url(value)?;
            format!(
                "devtools::install_url(\"{}\", dependencies = {dependencies}{lib_arg})",
                escape_r(&url), lib_arg = lib_arg
            )
        }
        "remotes" => {
            let url = normalize_install_archive_url(value)?;
            format!(
                "remotes::install_url(\"{}\", dependencies = {dependencies}{lib_arg})",
                escape_r(&url), lib_arg = lib_arg
            )
        }
        "github" => {
            let Some(repository) = normalize_github_repository(value) else {
                return Err(format!("{value} 不是有效的 GitHub 仓库标识，应为 owner/repo"));
            };
            package_name = repository
                .rsplit('/')
                .next()
                .unwrap_or(&repository)
                .to_string();

            let repo_with_ref = if !effective_version.is_empty() {
                let clean_ver = if effective_version.starts_with('v') {
                    effective_version.clone()
                } else {
                    format!("v{}", effective_version)
                };
                format!("{}@{}", repository, clean_ver)
            } else {
                repository
            };

            effective_version.clear();
            format!(
                "remotes::install_github(\"{}\", upgrade = \"never\", dependencies = {dependencies}{lib_arg})",
                escape_r(&repo_with_ref), lib_arg = lib_arg
            )
        }
        "base" => format!(
            "install.packages(\"{escaped_value}\", repos = \"{escaped_mirror}\", dependencies = {dependencies}{lib_arg})"
        ),
        "version" => return Ok(format!("packageVersion(\"{escaped_value}\")")),
        "remotesVersion" => {
            if version.is_empty() {
                return Err(format!("{value} 缺少可用于 install_version 的版本号"));
            }
            if !is_clean_version(version) {
                return Err(format!("{value} 的版本号格式不适合 install_version: {version}"));
            }
            format!(
                "remotes::install_version(\"{escaped_value}\", version = \"{}\", repos = \"{escaped_mirror}\", upgrade = \"never\", dependencies = {dependencies}{lib_arg})",
                escape_r(version), lib_arg = lib_arg
            )
        }
        "biocManager" => format!(
            "BiocManager::install(\"{escaped_value}\", update = FALSE, ask = FALSE, dependencies = {dependencies}{lib_arg})"
        ),
        "rForge" => format!(
            "install.packages(\"{escaped_value}\", repos = \"http://R-Forge.R-project.org\", dependencies = {dependencies}{lib_arg})"
        ),
        "biocGit" => {
            let (real_version, bioc_version) =
                version.split_once('|').unwrap_or((version, "3.18"));
            if !is_valid_package_name(value) || value.contains('/') {
                return Err(format!("{value} 不是有效的 Bioconductor 包名"));
            }
            if !is_valid_bioc_version(bioc_version) {
                return Err(format!("Bioconductor 版本格式无效: {bioc_version}"));
            }
            effective_version = real_version.to_string();
            let release = format!("RELEASE_{}", bioc_version.replace('.', "_"));
            format!(
                "remotes::install_git(\"https://git.bioconductor.org/packages/{escaped_value}\", ref = \"{release}\", upgrade = \"never\", dependencies = {dependencies}{lib_arg})"
            )
        }
        "auto" => format!(
            "install.packages(\"{escaped_value}\", repos = \"{escaped_mirror}\", dependencies = {dependencies}{lib_arg})"
        ),
        _ => return Err(format!("不支持的安装方式: {method}")),
    };

    if !conditional || package_name.is_empty() {
        return Ok(raw);
    }

    let version_check = if !effective_version.is_empty() && method != "biocManager" {
        format!(
            " || packageVersion(\"{}\") != \"{}\"",
            escape_r(&package_name),
            escape_r(&effective_version)
        )
    } else {
        String::new()
    };
    let display_version = if effective_version.is_empty() {
        String::new()
    } else {
        format!(" ({})", escape_r(&effective_version))
    };
    Ok(format!(
        "if (!requireNamespace(\"{}\", quietly = TRUE){version_check}) {{\r\n  {raw}\r\n}} else {{\r\n  message(\"{}{display_version} 已存在，跳过安装。\")\r\n}}",
        escape_r(&package_name),
        escape_r(&package_name)
    ))
}
