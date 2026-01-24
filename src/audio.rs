//! Audio playback for notification sounds

use crate::config::Config;
use crate::analyzer::Status;

#[cfg(feature = "sound")]
use rodio::{Decoder, OutputStream, Sink};

#[cfg(feature = "sound")]
use std::fs::File;
#[cfg(feature = "sound")]
use std::io::BufReader;

/// Play notification sound for the given status
pub fn play_sound(config: &Config, status: Status) -> Result<(), String> {
    if !config.notifications.desktop.sound {
        return Ok(());
    }

    // Try custom sound file first
    let sound_file = get_sound_file_for_status(config, status);

    if let Some(path) = sound_file {
        if play_sound_file(&path, config.notifications.desktop.volume).is_ok() {
            return Ok(());
        }
    }

    // Fall back to system notification sound
    play_system_sound()
}

/// Get custom sound file path for status
fn get_sound_file_for_status(_config: &Config, status: Status) -> Option<String> {
    // Default sound files in config directory
    let config_dir = crate::config::get_config_dir();
    let sound_name = match status {
        Status::TaskComplete | Status::ReviewComplete => "task-complete",
        Status::Question => "question",
        Status::PlanReady => "plan-ready",
        Status::SessionLimitReached | Status::ApiError => "alert",
        Status::Unknown => return None,
    };

    let path = config_dir.join("sounds").join(format!("{}.wav", sound_name));
    if path.exists() {
        return path.to_str().map(String::from);
    }

    // Try mp3
    let path = config_dir.join("sounds").join(format!("{}.mp3", sound_name));
    if path.exists() {
        return path.to_str().map(String::from);
    }

    None
}

/// Play a sound file using rodio (if sound feature is enabled)
#[cfg(feature = "sound")]
fn play_sound_file(path: &str, volume: f32) -> Result<(), String> {
    let file = File::open(path)
        .map_err(|e| format!("Failed to open sound file: {}", e))?;

    let reader = BufReader::new(file);

    let (_stream, stream_handle) = OutputStream::try_default()
        .map_err(|e| format!("Failed to get audio output: {}", e))?;

    let sink = Sink::try_new(&stream_handle)
        .map_err(|e| format!("Failed to create audio sink: {}", e))?;

    let source = Decoder::new(reader)
        .map_err(|e| format!("Failed to decode audio: {}", e))?;

    sink.set_volume(volume);
    sink.append(source);
    sink.sleep_until_end();

    Ok(())
}

/// Stub for when sound feature is disabled
#[cfg(not(feature = "sound"))]
fn play_sound_file(_path: &str, _volume: f32) -> Result<(), String> {
    Err("Sound feature not enabled".to_string())
}

/// Play system notification sound
#[cfg(target_os = "windows")]
fn play_system_sound() -> Result<(), String> {
    use std::process::Command;

    // Use PowerShell to play system notification sound
    let result = Command::new("powershell")
        .args(["-Command", "[System.Media.SystemSounds]::Asterisk.Play()"])
        .output();

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Failed to play system sound: {}", e)),
    }
}

/// Play system notification sound on non-Windows platforms
#[cfg(not(target_os = "windows"))]
fn play_system_sound() -> Result<(), String> {
    // On macOS/Linux, try to use system tools
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let _ = Command::new("afplay")
            .args(["/System/Library/Sounds/Ping.aiff"])
            .output();
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        // Try paplay first (PulseAudio), then aplay (ALSA)
        let _ = Command::new("paplay")
            .args(["/usr/share/sounds/freedesktop/stereo/message.oga"])
            .output()
            .or_else(|_| {
                Command::new("aplay")
                    .args(["/usr/share/sounds/alsa/Front_Center.wav"])
                    .output()
            });
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("System sound not supported on this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_config;

    #[test]
    fn test_get_sound_file_nonexistent() {
        let config = default_config();
        let result = get_sound_file_for_status(&config, Status::TaskComplete);
        // Should be None since default sound files don't exist
        assert!(result.is_none());
    }

    #[test]
    fn test_play_sound_disabled() {
        let mut config = default_config();
        config.notifications.desktop.sound = false;

        let result = play_sound(&config, Status::TaskComplete);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unknown_status_no_sound() {
        let config = default_config();
        let result = get_sound_file_for_status(&config, Status::Unknown);
        assert!(result.is_none());
    }
}
