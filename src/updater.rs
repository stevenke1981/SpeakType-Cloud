use futures_util::{Stream, StreamExt};
use reqwest::{Client, Url};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub const MANIFEST_URL: &str =
    "https://github.com/stevenke1981/SpeakType-Cloud/releases/latest/download/update-manifest.json";
const RELEASE_PATH_PREFIX: &str = "/stevenke1981/SpeakType-Cloud/releases/download/";
const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_INSTALLER_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct UpdateManifest {
    pub schema_version: u32,
    pub version: String,
    pub installer_url: String,
    pub sha256: String,
}

#[derive(Clone, Debug)]
pub struct StagedUpdate {
    pub version: String,
    pub installer_path: PathBuf,
    pub signer_cert_sha256: String,
    pub(crate) expected_sha256: String,
}

pub fn configured_signer_cert_sha256() -> Result<String, String> {
    validate_configured_fingerprint(option_env!("SPEAKTYPE_UPDATE_SIGNER_CERT_SHA256"))
}

pub async fn check_for_update() -> Result<Option<UpdateManifest>, String> {
    configured_signer_cert_sha256()?;
    let client = secure_client()?;
    let manifest_url = Url::parse(MANIFEST_URL).map_err(|error| error.to_string())?;
    let bytes = download_limited(&client, manifest_url, MAX_MANIFEST_BYTES, false).await?;
    let manifest = parse_manifest(&bytes)?;
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())?;
    let available = Version::parse(&manifest.version).map_err(|error| error.to_string())?;
    Ok((available > current).then_some(manifest))
}

pub async fn stage_update(manifest: &UpdateManifest) -> Result<StagedUpdate, String> {
    let signer_cert_sha256 = configured_signer_cert_sha256()?;
    validate_manifest(manifest)?;
    let client = secure_client()?;
    let installer_url = Url::parse(&manifest.installer_url).map_err(|error| error.to_string())?;
    let bytes = download_limited(&client, installer_url, MAX_INSTALLER_BYTES, true).await?;
    verify_sha256(&bytes, &manifest.sha256)?;

    let directory = std::env::temp_dir()
        .join("SpeakTypeCloud")
        .join("updates")
        .join(&manifest.version);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let installer_path = directory.join(format!("SpeakTypeCloud-Setup-{}.exe", manifest.version));
    let temporary_path = directory.join(format!(
        "SpeakTypeCloud-Setup-{}.{}.download",
        manifest.version,
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary_path)
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    drop(file);

    if let Err(error) = verify_authenticode(&temporary_path, &signer_cert_sha256) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }

    if installer_path.exists() {
        let existing = fs::read(&installer_path).map_err(|error| error.to_string())?;
        verify_sha256(&existing, &manifest.sha256)?;
        fs::remove_file(&temporary_path).map_err(|error| error.to_string())?;
    } else {
        fs::rename(&temporary_path, &installer_path).map_err(|error| error.to_string())?;
    }
    Ok(StagedUpdate {
        version: manifest.version.clone(),
        installer_path,
        signer_cert_sha256,
        expected_sha256: manifest.sha256.clone(),
    })
}

pub fn launch_installer(staged: &StagedUpdate) -> Result<(), String> {
    let configured_signer = configured_signer_cert_sha256()?;
    if configured_signer != staged.signer_cert_sha256 {
        return Err("已暫存更新的簽章信任根與目前程式不一致".to_string());
    }
    let bytes = fs::read(&staged.installer_path).map_err(|error| error.to_string())?;
    verify_sha256(&bytes, &staged.expected_sha256)?;
    verify_authenticode(&staged.installer_path, &configured_signer)?;
    Command::new(&staged.installer_path)
        .spawn()
        .map_err(|error| format!("無法啟動安裝精靈：{error}"))?;
    Ok(())
}

fn secure_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if validate_download_url(attempt.url(), false).is_ok() {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|error| error.to_string())
}

async fn download_limited(
    client: &Client,
    url: Url,
    maximum: usize,
    require_release_path: bool,
) -> Result<Vec<u8>, String> {
    validate_download_url(&url, require_release_path)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    validate_download_url(response.url(), false)?;
    if response
        .content_length()
        .is_some_and(|length| length > maximum as u64)
    {
        return Err("更新檔超過允許大小".to_string());
    }
    collect_limited_stream(response.bytes_stream(), maximum).await
}

async fn collect_limited_stream<S, T, E>(stream: S, maximum: usize) -> Result<Vec<u8>, String>
where
    S: Stream<Item = Result<T, E>>,
    T: AsRef<[u8]>,
    E: std::fmt::Display,
{
    futures_util::pin_mut!(stream);
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        let chunk = chunk.as_ref();
        if chunk.len() > maximum.saturating_sub(bytes.len()) {
            return Err("更新檔超過允許大小".to_string());
        }
        bytes.extend_from_slice(chunk);
    }
    Ok(bytes)
}

fn parse_manifest(bytes: &[u8]) -> Result<UpdateManifest, String> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err("更新資訊超過允許大小".to_string());
    }
    let manifest: UpdateManifest =
        serde_json::from_slice(bytes).map_err(|error| format!("更新資訊格式錯誤：{error}"))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &UpdateManifest) -> Result<(), String> {
    if manifest.schema_version != 1 {
        return Err("不支援的更新資訊版本".to_string());
    }
    Version::parse(&manifest.version).map_err(|error| format!("版本號無效：{error}"))?;
    if manifest.sha256.len() != 64
        || !manifest
            .sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
    {
        return Err("更新資訊的 SHA-256 無效".to_string());
    }
    let url = Url::parse(&manifest.installer_url).map_err(|error| error.to_string())?;
    validate_download_url(&url, true)?;
    let expected_prefix = format!("{RELEASE_PATH_PREFIX}v{}/", manifest.version);
    if !url.path().starts_with(&expected_prefix) || !url.path().ends_with(".exe") {
        return Err("安裝程式 URL 與更新版本不一致".to_string());
    }
    Ok(())
}

fn validate_download_url(url: &Url, require_release_path: bool) -> Result<(), String> {
    if url.scheme() != "https" {
        return Err("更新只能使用 HTTPS".to_string());
    }
    let host = url.host_str().unwrap_or_default();
    let allowed = matches!(
        host,
        "github.com"
            | "api.github.com"
            | "objects.githubusercontent.com"
            | "release-assets.githubusercontent.com"
            | "github-releases.githubusercontent.com"
    );
    if !allowed {
        return Err(format!("更新來源不在允許清單：{host}"));
    }
    if require_release_path
        && (host != "github.com" || !url.path().starts_with(RELEASE_PATH_PREFIX))
    {
        return Err("安裝程式必須來自本專案的 GitHub Release".to_string());
    }
    Ok(())
}

fn verify_sha256(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err("更新安裝程式 SHA-256 不符，已拒絕使用".to_string())
    }
}

fn validate_configured_fingerprint(value: Option<&str>) -> Result<String, String> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "此版本未設定更新簽章信任根；請前往 GitHub Releases 手動更新".to_string())?;
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("更新簽章信任根格式無效；自動更新已停用".to_string());
    }
    Ok(value.to_ascii_lowercase())
}

fn validate_signature_report(report: &str, expected: &str) -> Result<(), String> {
    let Some(actual) = report.trim().strip_prefix("Valid:") else {
        return Err(format!("Authenticode 簽章無效：{}", report.trim()));
    };
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err("Authenticode 簽署憑證與內建信任根不符".to_string())
    }
}

#[cfg(target_os = "windows")]
fn verify_authenticode(path: &Path, expected_signer: &str) -> Result<(), String> {
    let status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$s=Get-AuthenticodeSignature -LiteralPath $env:SPEAKTYPE_UPDATE_PATH; if ($null -eq $s.SignerCertificate) { 'Unsigned' } elseif ($s.Status -ne 'Valid') { 'Invalid:' + $s.Status } else { $h=[Security.Cryptography.SHA256]::Create(); try { $fp=($h.ComputeHash($s.SignerCertificate.RawData) | ForEach-Object { $_.ToString('x2') }) -join ''; 'Valid:' + $fp } finally { $h.Dispose() } }",
        ])
        .env("SPEAKTYPE_UPDATE_PATH", path)
        .output()
        .map_err(|error| format!("無法驗證 Authenticode：{error}"))?;
    if !status.status.success() {
        return Err("Authenticode 驗證程序失敗".to_string());
    }
    validate_signature_report(&String::from_utf8_lossy(&status.stdout), expected_signer)
}

#[cfg(not(target_os = "windows"))]
fn verify_authenticode(_path: &Path, _expected_signer: &str) -> Result<(), String> {
    Err("此平台無法執行 Windows Authenticode 驗證".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{stream, StreamExt};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn manifest(url: &str, sha256: &str) -> UpdateManifest {
        UpdateManifest {
            schema_version: 1,
            version: "1.2.3".to_string(),
            installer_url: url.to_string(),
            sha256: sha256.to_string(),
        }
    }

    #[test]
    fn accepts_only_this_projects_https_release_url() {
        let valid = manifest(
            "https://github.com/stevenke1981/SpeakType-Cloud/releases/download/v1.2.3/SpeakTypeCloud-Setup-1.2.3.exe",
            &"a".repeat(64),
        );
        assert!(validate_manifest(&valid).is_ok());

        for url in [
            "http://github.com/stevenke1981/SpeakType-Cloud/releases/download/v1.2.3/a.exe",
            "https://example.com/stevenke1981/SpeakType-Cloud/releases/download/v1.2.3/a.exe",
            "https://github.com/attacker/project/releases/download/v1.2.3/a.exe",
        ] {
            assert!(validate_manifest(&manifest(url, &"a".repeat(64))).is_err());
        }
    }

    #[test]
    fn rejects_checksum_mismatch() {
        assert!(verify_sha256(b"installer", &"0".repeat(64)).is_err());
        let expected = format!("{:x}", Sha256::digest(b"installer"));
        assert!(verify_sha256(b"installer", &expected).is_ok());
    }

    #[test]
    fn rejects_unknown_schema_or_invalid_hash() {
        let mut value = manifest(
            "https://github.com/stevenke1981/SpeakType-Cloud/releases/download/v1.2.3/a.exe",
            "not-a-hash",
        );
        assert!(validate_manifest(&value).is_err());
        value.sha256 = "a".repeat(64);
        value.schema_version = 2;
        assert!(validate_manifest(&value).is_err());
    }

    #[test]
    fn updater_requires_a_valid_compile_time_signer_pin() {
        assert!(validate_configured_fingerprint(None).is_err());
        assert!(validate_configured_fingerprint(Some("abc")).is_err());
        assert_eq!(
            validate_configured_fingerprint(Some(&"A".repeat(64))).expect("valid fingerprint"),
            "a".repeat(64)
        );
    }

    #[test]
    fn authenticode_report_rejects_unsigned_invalid_or_wrong_signer() {
        let expected = "a".repeat(64);
        assert!(validate_signature_report("Unsigned", &expected).is_err());
        assert!(validate_signature_report("Invalid:HashMismatch", &expected).is_err());
        assert!(
            validate_signature_report(&format!("Valid:{}", "b".repeat(64)), &expected).is_err()
        );
        assert!(validate_signature_report(&format!("Valid:{expected}"), &expected).is_ok());
    }

    #[tokio::test]
    async fn chunked_download_aborts_as_soon_as_limit_is_exceeded() {
        let polled = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&polled);
        let chunks = stream::iter(vec![
            Ok::<_, std::io::Error>(vec![1_u8; 3]),
            Ok::<_, std::io::Error>(vec![2_u8; 3]),
            Ok::<_, std::io::Error>(vec![3_u8; 3]),
        ])
        .inspect(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
        });

        assert!(collect_limited_stream(chunks, 5).await.is_err());
        assert_eq!(polled.load(Ordering::SeqCst), 2);
    }
}
