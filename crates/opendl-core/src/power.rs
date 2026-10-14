use anyhow::Result;
use tracing::info;

pub fn trigger_power_action(action: &str) -> Result<()> {
    match action.to_lowercase().as_str() {
        "sleep" => system_sleep(),
        "shutdown" => system_shutdown(60, "OpenDL download queue completed. Shutting down in 60s."),
        _ => Ok(()),
    }
}

pub fn system_sleep() -> Result<()> {
    info!("Triggering system sleep mode");
    #[cfg(windows)]
    {
        std::process::Command::new("rundll32.exe")
            .args(["powrprof.dll,SetSuspendState", "0,1,0"])
            .spawn()?;
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("systemctl").arg("suspend").spawn();
    }
    Ok(())
}

pub fn system_shutdown(seconds: u32, message: &str) -> Result<()> {
    info!(seconds = seconds, message = message, "Triggering timed system shutdown");
    #[cfg(windows)]
    {
        std::process::Command::new("shutdown")
            .args(["/s", "/t", &seconds.to_string(), "/c", message])
            .spawn()?;
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("shutdown")
            .args(["-h", &format!("+{}", (seconds + 59) / 60)])
            .spawn();
    }
    Ok(())
}

pub fn abort_system_shutdown() -> Result<()> {
    info!("Aborting pending system shutdown");
    #[cfg(windows)]
    {
        std::process::Command::new("shutdown").arg("/a").spawn()?;
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("shutdown").arg("-c").spawn();
    }
    Ok(())
}
