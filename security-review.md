# Security Review: downloader.rs

## Summary
- Critical: 1 | High: 2 | Medium: 2 | Low: 1

## Findings

### [CRITICAL] Potential SSRF via User-Controlled URL
- **File:** src-tauri/src/services/downloader.rs:238
- **OWASP:** A10 Server-Side Request Forgery
- **Severity:** Critical
- **CWE:** CWE-918

#### Description
The code accepts a user-controlled URL (`m3u8_url` parameter) and passes it directly to yt-dlp without any validation or allowlist checking. An attacker could provide URLs pointing to internal services, cloud metadata endpoints, or private networks.

#### Vulnerable Code
```rust
// Line 238 - decoded_url is passed directly to yt-dlp
args.push(decoded_url.to_string());
```

#### Recommended Fix
```rust
use url::Url;

fn validate_url(url: &str) -> Result<(), String> {
    let parsed = Url::parse(url)
        .map_err(|_| "Invalid URL format")?;

    // Only allow http and https
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("Only HTTP and HTTPS URLs are allowed".to_string());
    }

    // Block private/internal IP ranges
    let hostname = parsed.host_str().ok_or("Missing hostname")?;

    // Check for localhost
    if hostname == "localhost" || hostname == "127.0.0.1" || hostname == "::1" {
        return Err("Localhost URLs are not allowed".to_string());
    }

    // Additional validation can be added here
    Ok(())
}

// In download_m3u8 function:
validate_url(&decoded_url)?;
```

#### Impact
An attacker could:
- Access internal services (e.g., http://192.168.1.1, http://10.0.0.1)
- Query cloud metadata endpoints (e.g., http://169.254.169.254)
- Scan internal networks
- Access admin panels or debug interfaces

---

### [HIGH] Disabled SSL Certificate Verification
- **File:** src-tauri/src/services/downloader.rs:209-210
- **OWASP:** A05 Security Misconfiguration
- **Severity:** High
- **CWE:** CWE-295

#### Description
The code uses `--no-check-certificate` and `--prefer-insecure` flags with yt-dlp, which disables SSL certificate verification. This makes the application vulnerable to man-in-the-middle (MITM) attacks.

#### Vulnerable Code
```rust
// Lines 209-210
"--no-check-certificate".to_string(), // 1. 忽略 SSL 证书错误（解决当前报错）
"--prefer-insecure".to_string(),      // 2. 强制使用不安全连接（备选保障）
```

#### Recommended Fix
Remove these insecure options unless there is a specific operational need. If working with self-signed certificates is required:
1. Add the certificate to the system's trusted store
2. Or use `--client-certificate` with a specific certificate

```rust
// Remove these lines:
"--no-check-certificate".to_string(),
"--prefer-insecure".to_string(),
```

#### Impact
An attacker performing a MITM attack could:
- Intercept downloaded video content
- Inject malicious content
- Steal credentials or tokens in the request

---

### [HIGH] Path Traversal Risk via User-Controlled Output Path
- **File:** src-tauri/src/services/downloader.rs:191, 227, 413
- **OWASP:** A01 Broken Access Control
- **Severity:** High
- **CWE:** CWE-22

#### Description
The `output_path` parameter is used directly without validation, which could allow path traversal attacks. While `video_name` is sanitized via `sanitize_filename()`, the output directory path is not validated.

#### Vulnerable Code
```rust
// Line 191 - output_path used directly
let output_dir = PathBuf::from(output_path);

// Line 227 - output_path used in format string
format!("{}/{}.%(ext)s", output_path, safe_filename),

// Line 413 - video_name used directly (not sanitized here)
let target_path = output_dir.join(format!("{}.{}", video_name, ext));
```

#### Recommended Fix
```rust
use std::path::Path;

fn validate_output_path(base_dir: &Path, user_path: &str) -> Result<PathBuf, String> {
    // Resolve the user-provided path
    let requested_path = base_dir.join(user_path);

    // Ensure the resolved path is still within the base directory
    let canonical_base = base_dir.canonicalize()
        .map_err(|_| "Invalid base directory")?;
    let canonical_requested = requested_path.canonicalize()
        .map_err(|_| "Invalid output path")?;

    if !canonical_requested.starts_with(&canonical_base) {
        return Err("Path traversal detected".to_string());
    }

    Ok(requested_path)
}

// In download_m3u8:
let validated_output_dir = validate_output_path(
    &PathBuf::from("."),  // or a configured download base directory
    output_path
)?;
```

---

### [MEDIUM] Sensitive Information in Logs
- **File:** src-tauri/src/services/downloader.rs:179, 240
- **OWASP:** A09 Security Logging and Monitoring Failures
- **Severity:** Medium
- **CWE:** CWE-532

#### Description
The decoded URL and full download command arguments are logged, which could expose sensitive information such as authentication tokens, API keys, or private URLs.

#### Vulnerable Code
```rust
// Line 179 - Logs full URL including potentially sensitive data
tracing::info!("[DOWNLOAD] URL 解码: {} -> {}", m3u8_url, decoded_url);

// Line 240 - Logs full command with arguments
tracing::info!("[DOWNLOAD] 开始下载: {}", args.join(" "));
```

#### Recommended Fix
```rust
// Sanitize URL before logging
let sanitized_for_log = if decoded_url.len() > 50 {
    format!("{}...[REDACTED]", &decoded_url[..50])
} else {
    decoded_url.clone()
};
tracing::info!("[DOWNLOAD] URL 解码: [REDACTED]");

// Log command without sensitive arguments
tracing::info!("[DOWNLOAD] 开始下载: yt-dlp [ARGS REDACTED]");
```

#### Impact
- Exposed API keys or tokens in logs
- Privacy leakage of URLs users are accessing
- Log files could be accessed by unauthorized parties

---

### [MEDIUM] Missing Rate Limiting on Downloads
- **File:** src-tauri/src/services/downloader.rs
- **OWASP:** A04 Insecure Design
- **Severity:** Medium
- **CWE:** CWE-770

#### Description
There is no rate limiting or concurrency control on the download functionality. While `max_concurrent` is used in `batch_download_concurrent`, individual download requests have no throttling.

#### Recommended Fix
Implement rate limiting at the API/command level:
- Limit the number of concurrent downloads per user
- Add request throttling
- Implement download quotas

---

### [LOW] Incomplete Filename Sanitization
- **File:** src-tauri/src/services/downloader.rs:584-588
- **OWASP:** A05 Security Misconfiguration
- **Severity:** Low
- **CWE:** CWE-20

#### Description
The `sanitize_filename` function only handles a limited set of characters and does not handle null bytes, Unicode homograph attacks, or other edge cases.

#### Vulnerable Code
```rust
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if "/\\?%*:|\"<>".contains(c) { '_' } else { c })
        .collect()
}
```

#### Recommended Fix
```rust
use std::path::Path;

pub fn sanitize_filename(name: &str) -> String {
    // Remove null bytes
    let name = name.replace('\0', "");

    // Use a proper sanitization approach
    let sanitized: String = name
        .chars()
        .map(|c| {
            if "/\\?%*:|\"<>".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();

    // Ensure filename is not empty and doesn't start with a dot
    let result = sanitized.trim();
    if result.is_empty() || result.starts_with('.') {
        return format!("download_{}", uuid::Uuid::new_v4());
    }

    result.to_string()
}
```

---

## Passed Checks

- No use of `eval()`, `exec()`, or `compile()` with user input
- No raw SQL queries with string interpolation
- No use of `pickle.loads()` on untrusted data
- No hardcoded secrets detected in this file
- Uses proper Rust patterns for subprocess execution (args passed as vector, not shell=True)
- Password handling not applicable to this file
- JWT handling not applicable to this file

## Recommendations Summary

1. **Critical Priority:** Add URL validation to prevent SSRF attacks
2. **High Priority:** Remove `--no-check-certificate` and `--prefer-insecure` flags
3. **High Priority:** Validate output path to prevent path traversal
4. **Medium Priority:** Sanitize logs to avoid exposing sensitive data
5. **Medium Priority:** Consider adding rate limiting for downloads
6. **Low Priority:** Improve filename sanitization function
