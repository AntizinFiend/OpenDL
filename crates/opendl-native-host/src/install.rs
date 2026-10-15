use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::Path;
use tracing::info;

pub const NATIVE_HOST_NAME: &str = "org.opendl.native";

pub fn install_native_manifests(custom_ext_ids: &[String]) -> Result<()> {
    let current_exe = env::current_exe().context("Failed to obtain current executable path")?;
    let exe_dir = current_exe.parent().context("Failed to get parent directory")?;

    let chrome_manifest_path = exe_dir.join(format!("{}.json", NATIVE_HOST_NAME));
    let firefox_manifest_path = exe_dir.join(format!("{}.mozilla.json", NATIVE_HOST_NAME));

    // Do NOT double escape backslashes; serde_json handles JSON escaping properly.
    let exe_str = current_exe.to_string_lossy().to_string();

    let mut allowed_origins = vec![
        // Deterministic extension ID for OpenDL (derived from manifest key)
        "chrome-extension://hclmjoafklhnljjihblhmnigbbgannlf/".to_string(),
        "chrome-extension://hiigfpegfninmnidhkcjebcbanmhfbdl/".to_string(),
        "chrome-extension://lggbokfckofcgjndaboioakcmincinpo/".to_string(), // Motrix extension compatibility
        "chrome-extension://lombjkbnbdmhdegbiijbdbafcmphcemi/".to_string(),
        "chrome-extension://ibiafnimffjnfjcbclnnpbbbidlphgbp/".to_string(),
    ];

    for ext_id in custom_ext_ids {
        let clean_id = ext_id.trim();
        if !clean_id.is_empty() {
            let origin = if clean_id.starts_with("chrome-extension://") {
                if clean_id.ends_with('/') {
                    clean_id.to_string()
                } else {
                    format!("{}/", clean_id)
                }
            } else {
                format!("chrome-extension://{}/", clean_id)
            };
            if !allowed_origins.contains(&origin) {
                allowed_origins.push(origin);
            }
        }
    }

    let chrome_manifest = serde_json::json!({
        "name": NATIVE_HOST_NAME,
        "description": "OpenDL Native Messaging Host",
        "path": exe_str,
        "type": "stdio",
        "allowed_origins": allowed_origins
    });

    let firefox_manifest = serde_json::json!({
        "name": NATIVE_HOST_NAME,
        "description": "OpenDL Native Messaging Host",
        "path": exe_str,
        "type": "stdio",
        "allowed_extensions": [
            "opendl-extension@opendl.org"
        ]
    });

    fs::write(&chrome_manifest_path, serde_json::to_string_pretty(&chrome_manifest)?)?;
    fs::write(&firefox_manifest_path, serde_json::to_string_pretty(&firefox_manifest)?)?;

    info!(
        chrome = ?chrome_manifest_path,
        firefox = ?firefox_manifest_path,
        "Generated native messaging manifests"
    );

    #[cfg(windows)]
    {
        register_windows_registry(&chrome_manifest_path, &firefox_manifest_path)?;
    }

    println!("[+] Successfully registered {} for Chrome, Brave, Brave-Beta, Edge, Vivaldi, and Firefox!", NATIVE_HOST_NAME);
    println!("    Manifest: {:?}", chrome_manifest_path);
    println!("    Allowed origins: {:?}", allowed_origins);
    Ok(())
}

pub fn uninstall_native_manifests() -> Result<()> {
    #[cfg(windows)]
    {
        unregister_windows_registry()?;
    }
    println!("[+] Unregistered {} native messaging registry keys.", NATIVE_HOST_NAME);
    Ok(())
}

#[cfg(windows)]
fn register_windows_registry(chrome_manifest: &Path, firefox_manifest: &Path) -> Result<()> {
    use std::process::Command;

    let chrome_str = chrome_manifest.to_string_lossy();
    let firefox_str = firefox_manifest.to_string_lossy();

    let reg_commands = [
        (format!("HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME), chrome_str.as_ref()),
        (format!("HKCU\\Software\\Chromium\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME), chrome_str.as_ref()),
        (format!("HKCU\\Software\\BraveSoftware\\Brave-Browser\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME), chrome_str.as_ref()),
        (format!("HKCU\\Software\\BraveSoftware\\Brave-Browser-Beta\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME), chrome_str.as_ref()),
        (format!("HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME), chrome_str.as_ref()),
        (format!("HKCU\\Software\\Vivaldi\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME), chrome_str.as_ref()),
        (format!("HKCU\\Software\\Mozilla\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME), firefox_str.as_ref()),
    ];

    for (key, val) in reg_commands {
        let status = Command::new("reg")
            .args(["add", &key, "/ve", "/t", "REG_SZ", "/d", val, "/f"])
            .status()
            .with_context(|| format!("Failed to execute reg add for {}", key))?;

        if !status.success() {
            anyhow::bail!("reg add failed for key {}", key);
        }
    }

    Ok(())
}

#[cfg(windows)]
fn unregister_windows_registry() -> Result<()> {
    use std::process::Command;

    let keys = [
        format!("HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME),
        format!("HKCU\\Software\\BraveSoftware\\Brave-Browser\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME),
        format!("HKCU\\Software\\BraveSoftware\\Brave-Browser-Beta\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME),
        format!("HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME),
        format!("HKCU\\Software\\Vivaldi\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME),
        format!("HKCU\\Software\\Mozilla\\NativeMessagingHosts\\{}", NATIVE_HOST_NAME),
    ];

    for key in keys {
        let _ = Command::new("reg")
            .args(["delete", &key, "/f"])
            .status();
    }

    Ok(())
}
