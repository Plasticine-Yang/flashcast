//! Windows 版本描述（纯逻辑）。
//!
//! 真值来自 `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`：`ProductName`、
//! `DisplayVersion`（Windows 10 2004 起取代 `ReleaseId`）与 `CurrentBuild` + `UBR`
//! （修订号）。注意 `ProductName` 在 Windows 11 上仍会是 `Windows 10 …`（系统没有
//! 更新这个值），因此**不能**用它判断主版本 —— 这里只做描述性拼接，判断留给用户。

/// 拼接面向用户的系统版本描述；没有任何可用取值时返回 `None`。
pub fn format(
    product_name: Option<&str>,
    display_version: Option<&str>,
    current_build: Option<&str>,
    ubr: Option<u32>,
) -> Option<String> {
    let product = clean(product_name);
    let display = clean(display_version);
    let build = clean(current_build);

    let mut out = product.unwrap_or_else(|| "Windows".to_string());
    if let Some(display) = display {
        out.push(' ');
        out.push_str(&display);
    }
    if let Some(build) = build {
        out.push_str("（build ");
        out.push_str(&build);
        if let Some(ubr) = ubr {
            out.push('.');
            out.push_str(&ubr.to_string());
        }
        out.push('）');
    }
    if out == "Windows" {
        return None;
    }
    Some(out)
}

fn clean(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_real_windows_11_version() {
        assert_eq!(
            format(Some("Windows 11 Pro"), Some("24H2"), Some("26100"), Some(1742)).as_deref(),
            Some("Windows 11 Pro 24H2（build 26100.1742）")
        );
    }

    #[test]
    fn missing_pieces_degrade_gracefully() {
        assert_eq!(
            format(Some("Windows 10 Pro"), None, Some("19045"), Some(4046)).as_deref(),
            Some("Windows 10 Pro（build 19045.4046）")
        );
        assert_eq!(
            format(None, None, Some("26100"), None).as_deref(),
            Some("Windows（build 26100）")
        );
        assert_eq!(
            format(Some("Windows 11 Pro"), Some("24H2"), None, None).as_deref(),
            Some("Windows 11 Pro 24H2")
        );
        assert_eq!(format(Some("  "), None, None, Some(1)), None);
        assert_eq!(format(None, None, None, None), None);
    }
}
