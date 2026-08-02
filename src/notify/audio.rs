//! Text-to-speech provider implementations.
//!
//! All network providers are best-effort and log warnings on failure; TTS never blocks
//! the notification pipeline on errors.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::time::Instant;
use tokio::process::Command;
use tracing::Instrument;

/// Provider selection for TTS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TtsProvider {
    #[default]
    Auto,
    System,
    Elevenlabs,
    Openai,
    Azure,
    Google,
    Cartesia,
}

impl std::str::FromStr for TtsProvider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(TtsProvider::Auto),
            "system" => Ok(TtsProvider::System),
            "elevenlabs" | "eleven-labs" => Ok(TtsProvider::Elevenlabs),
            "openai" | "open-ai" => Ok(TtsProvider::Openai),
            "azure" | "microsoft" => Ok(TtsProvider::Azure),
            "google" => Ok(TtsProvider::Google),
            "cartesia" => Ok(TtsProvider::Cartesia),
            // traci: allow -- invalid provider text is a normal parse result surfaced by the caller.
            other => Err(format!("unknown TTS provider: {other}")),
        }
    }
}

/// TTS configuration, usually deserialized from `[notify.tts]` in `kaptaind.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct TtsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub voice: Option<String>,
    /// Minimum seconds between spoken notifications (0 = no rate limit).
    #[serde(default = "default_tts_rate_limit_seconds")]
    pub rate_limit_seconds: u64,
}

fn default_tts_rate_limit_seconds() -> u64 {
    30
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "auto".to_string(),
            voice: None,
            rate_limit_seconds: default_tts_rate_limit_seconds(),
        }
    }
}

static LAST_SPOKEN: std::sync::Mutex<Option<HashMap<String, Instant>>> =
    std::sync::Mutex::new(None);

/// Speak `text` using the configured TTS provider, if enabled.
///
/// This function is fire-and-forget: it spawns a short-lived async task and logs
/// any failures without returning them to the caller.
pub fn speak(text: String, config: &TtsConfig) {
    if !config.enabled {
        return;
    }

    let provider = config.provider.clone();
    let voice = config.voice.clone();
    let rate_limit = config.rate_limit_seconds;

    if rate_limit > 0 && is_rate_limited(rate_limit, &provider) {
        tracing::debug!(component = module_path!(), "TTS rate-limited");
        return;
    }

    let speech_task = async move {
        let provider = resolve_provider(&provider).await;
        if let Err(err) = speak_with_provider(&text, provider, voice.as_deref()).await {
            tracing::warn!(error = %err, provider = ?provider, "TTS failed");
        }
    };
    tokio::spawn(speech_task.in_current_span());
}

fn is_rate_limited(limit_seconds: u64, key: &str) -> bool {
    let now = Instant::now();
    let mut guard = LAST_SPOKEN.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(last) = map.get(key) {
        if now.duration_since(*last).as_secs() < limit_seconds {
            return true;
        }
    }
    map.insert(key.to_string(), now);
    false
}

#[cfg(test)]
fn reset_rate_limiter() {
    let mut guard = LAST_SPOKEN.lock().unwrap_or_else(|e| e.into_inner());
    *guard = None;
}

async fn resolve_provider(configured: &str) -> TtsProvider {
    let parsed = configured
        .parse::<TtsProvider>()
        .unwrap_or(TtsProvider::Auto);
    if parsed != TtsProvider::Auto {
        return parsed;
    }

    // Auto-select: prefer local system TTS when available, otherwise use the first
    // cloud provider key found in the environment.
    if system_tts_available().await {
        return TtsProvider::System;
    }
    if env::var("ELEVENLABS_API_KEY").is_ok() {
        return TtsProvider::Elevenlabs;
    }
    if env::var("OPENAI_API_KEY").is_ok() {
        return TtsProvider::Openai;
    }
    if env::var("AZURE_SPEECH_KEY").is_ok() && env::var("AZURE_SPEECH_REGION").is_ok() {
        return TtsProvider::Azure;
    }
    if env::var("GOOGLE_API_KEY").is_ok() || env::var("GOOGLE_APPLICATION_CREDENTIALS_JSON").is_ok()
    {
        return TtsProvider::Google;
    }
    if env::var("CARTESIA_API_KEY").is_ok() {
        return TtsProvider::Cartesia;
    }
    TtsProvider::System
}

async fn speak_with_provider(
    text: &str,
    provider: TtsProvider,
    voice: Option<&str>,
) -> anyhow::Result<()> {
    match provider {
        TtsProvider::System => system_speak(text).await,
        TtsProvider::Elevenlabs => elevenlabs_speak(text, voice).await,
        TtsProvider::Openai => openai_speak(text, voice).await,
        TtsProvider::Azure => azure_speak(text, voice).await,
        TtsProvider::Google => google_speak(text, voice).await,
        TtsProvider::Cartesia => cartesia_speak(text, voice).await,
        TtsProvider::Auto => {
            // Auto should have been resolved before this point.
            system_speak(text).await
        }
    }
}

/// Returns true if a local system TTS command is likely available.
async fn system_tts_available() -> bool {
    if cfg!(target_os = "macos") {
        return Command::new("which")
            .arg("say")
            .output()
            .await
            .is_ok_and(|o| o.status.success());
    }
    if cfg!(target_os = "linux") {
        return Command::new("which")
            .arg("espeak")
            .output()
            .await
            .is_ok_and(|o| o.status.success());
    }
    if cfg!(target_os = "windows") {
        return true; // PowerShell with .NET is assumed available on modern Windows.
    }
    false
}

async fn system_speak(text: &str) -> anyhow::Result<()> {
    if cfg!(target_os = "macos") {
        Command::new("say").arg(text).output().await?;
        return Ok(());
    }
    if cfg!(target_os = "linux") {
        Command::new("espeak").arg(text).output().await?;
        return Ok(());
    }
    if cfg!(target_os = "windows") {
        // Deliver the text via an environment variable read by a fixed script, so
        // no user-controlled text is interpolated into the PowerShell command
        // (defeats injection via backtick, $(...), ;, |, &, or newlines).
        let script = "Add-Type -AssemblyName System.Speech; $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer; $synth.Speak($env:KAPTAIND_TTS_TEXT);";
        let _ = Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(script)
            .env("KAPTAIND_TTS_TEXT", text)
            .output()
            .await?;
        return Ok(());
    }
    anyhow::bail!("no system TTS command available on this platform")
}

async fn elevenlabs_speak(text: &str, voice: Option<&str>) -> anyhow::Result<()> {
    let api_key = env::var("ELEVENLABS_API_KEY")?;
    let voice_id = voice
        .map(|v| v.to_string())
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .or_else(|| env::var("ELEVENLABS_VOICE_ID").ok())
        .unwrap_or_else(|| "21m00Tcm4TlvDq8ikWAM".to_string());

    let client = crate::util::http::hardened_client(std::time::Duration::from_secs(30));
    let response = client
        .post(format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{voice_id}/stream"
        ))
        .header("xi-api-key", api_key)
        .json(&serde_json::json!({
            "text": text,
            "model_id": "eleven_monolingual_v1",
            "voice_settings": { "stability": 0.5, "similarity_boost": 0.5 }
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("elevenlabs TTS failed: {}", response.status());
    }

    let audio = response.bytes().await?;
    play_audio_bytes(&audio).await?;
    Ok(())
}

async fn openai_speak(text: &str, voice: Option<&str>) -> anyhow::Result<()> {
    let api_key = env::var("OPENAI_API_KEY")?;
    let model = env::var("OPENAI_TTS_MODEL").unwrap_or_else(|_| "tts-1".to_string());
    let voice = voice
        .map(|v| v.to_string())
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .or_else(|| env::var("OPENAI_TTS_VOICE").ok())
        .unwrap_or_else(|| "alloy".to_string());

    let client = crate::util::http::hardened_client(std::time::Duration::from_secs(30));
    let response = client
        .post("https://api.openai.com/v1/audio/speech")
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": model,
            "input": text,
            "voice": voice,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("openai TTS failed: {}", response.status());
    }

    let audio = response.bytes().await?;
    play_audio_bytes(&audio).await?;
    Ok(())
}

async fn azure_speak(text: &str, voice: Option<&str>) -> anyhow::Result<()> {
    let key = env::var("AZURE_SPEECH_KEY")?;
    let region = env::var("AZURE_SPEECH_REGION")?;
    if region.is_empty()
        || region.len() > 32
        || !region
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        anyhow::bail!("invalid AZURE_SPEECH_REGION (expected [a-z0-9-]{{1,32}}): {region:?}");
    }
    let voice = voice
        .map(|v| v.to_string())
        .unwrap_or_else(|| "en-US-AriaNeural".to_string());

    let ssml = format!(
        "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'><voice name='{voice}'>{text}</voice></speak>",
        text = escape_xml(text)
    );

    let client = crate::util::http::hardened_client(std::time::Duration::from_secs(30));
    let response = client
        .post(format!(
            "https://{region}.tts.speech.microsoft.com/cognitiveservices/v1"
        ))
        .header("Ocp-Apim-Subscription-Key", key)
        .header("Content-Type", "application/ssml+xml")
        .header(
            "X-Microsoft-OutputFormat",
            "audio-16khz-128kbitrate-mono-mp3",
        )
        .body(ssml)
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("azure TTS failed: {}", response.status());
    }

    let audio = response.bytes().await?;
    play_audio_bytes(&audio).await?;
    Ok(())
}

async fn google_speak(text: &str, voice: Option<&str>) -> anyhow::Result<()> {
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let api_key = env::var("GOOGLE_API_KEY").ok();
    // traci: allow -- optional failure is represented by None and handled by the caller.
    let credentials_json = env::var("GOOGLE_APPLICATION_CREDENTIALS_JSON").ok();

    let request_body = serde_json::json!({
        "input": { "text": text },
        "voice": {
            "languageCode": "en-US",
            "name": voice.unwrap_or("en-US-Neural2-F")
        },
        "audioConfig": { "audioEncoding": "MP3" }
    });

    let client = crate::util::http::hardened_client(std::time::Duration::from_secs(30));
    let response = if let Some(key) = api_key {
        client
            .post("https://texttospeech.googleapis.com/v1/text:synthesize")
            .header("X-Goog-Api-Key", key)
            .json(&request_body)
            .send()
            .await?
    } else if credentials_json.is_some() {
        // We have service-account JSON but no API key. The Cloud TTS REST endpoint
        // requires an OAuth2 bearer token, which is more ceremony than we want in
        // this module. Fall back to the API-key path with an empty key so the user
        // gets a clear error.
        anyhow::bail!("Google TTS requires GOOGLE_API_KEY; GOOGLE_APPLICATION_CREDENTIALS_JSON is not yet supported for direct REST TTS");
    } else {
        anyhow::bail!("Google TTS requires GOOGLE_API_KEY");
    };

    if !response.status().is_success() {
        anyhow::bail!("google TTS failed: {}", response.status());
    }

    let data: serde_json::Value = response.json().await?;
    let audio_base64 = data["audioContent"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing audioContent in Google TTS response"))?;
    let audio = crate::util::base64::decode(audio_base64)?;
    play_audio_bytes(&audio).await?;
    Ok(())
}

async fn cartesia_speak(text: &str, voice: Option<&str>) -> anyhow::Result<()> {
    let api_key = env::var("CARTESIA_API_KEY")?;
    let voice_id = voice
        .map(|v| v.to_string())
        // traci: allow -- optional failure is represented by None and handled by the caller.
        .or_else(|| env::var("CARTESIA_VOICE_ID").ok())
        .unwrap_or_else(|| "5347fbd2-11b2-4f18-9c48-03a39978ace1".to_string());

    let client = crate::util::http::hardened_client(std::time::Duration::from_secs(30));
    let response = client
        .post("https://api.cartesia.ai/tts/bytes")
        .header("Cartesia-Version", "2024-06-10")
        .header("X-API-Key", api_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "transcript": text,
            "model_id": "sonic-english",
            "voice": { "mode": "id", "id": voice_id },
            "output_format": { "container": "mp3", "encoding": "mp3", "sample_rate": 44100 }
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("cartesia TTS failed: {}", response.status());
    }

    let audio = response.bytes().await?;
    play_audio_bytes(&audio).await?;
    Ok(())
}

/// Decode and play raw audio bytes using a platform-appropriate command.
async fn play_audio_bytes(audio: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;

    let file_name = format!("kaptaind_tts_{:016x}.mp3", rand::random::<u64>());
    let path = std::env::temp_dir().join(file_name);
    {
        let mut f = std::fs::File::create(&path)?;
        f.write_all(audio)?;
        f.sync_all()?;
    }

    let result = play_audio_file(&path).await;

    if let Err(error) = std::fs::remove_file(&path) {
        tracing::warn!(
            ?error,
            operation = "play_audio_bytes",
            source_line = line!(),
            "best-effort operation failed"
        );
    }

    result
}

async fn play_audio_file(path: &std::path::Path) -> anyhow::Result<()> {
    let path_str = path.to_str().unwrap_or("/tmp/kaptaind_tts.mp3");

    // Try cross-platform ffmpeg/ffplay first.
    if Command::new("which")
        .arg("ffplay")
        .output()
        .await
        .is_ok_and(|o| o.status.success())
    {
        let _ = Command::new("ffplay")
            .args(["-nodisp", "-autoexit", "-loglevel", "quiet", path_str])
            .output()
            .await?;
        return Ok(());
    }

    if cfg!(target_os = "macos") {
        Command::new("afplay").arg(path).output().await?;
    } else if cfg!(target_os = "linux") {
        for player in ["mpg123", "mpv", "cvlc"] {
            if Command::new("which")
                .arg(player)
                .output()
                .await
                .is_ok_and(|o| o.status.success())
            {
                Command::new(player).arg(path_str).output().await?;
                return Ok(());
            }
        }
        anyhow::bail!("no suitable audio player found on Linux (tried ffplay, mpg123, mpv, cvlc)");
    } else if cfg!(target_os = "windows") {
        // System.Media.SoundPlayer only supports WAV, so cloud MP3 playback on Windows
        // requires ffplay (handled above) or another installed player.
        anyhow::bail!("Windows TTS playback requires ffplay to be installed and on PATH");
    } else {
        anyhow::bail!("unsupported platform for audio playback");
    }

    Ok(())
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    // The rate limiter is process-global state (`reset_rate_limiter` /
    // `is_rate_limited`). Serialize the tests that exercise it so parallel
    // threads cannot reset the state out from under one another.
    static RATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn provider_parsing() {
        assert_eq!(
            "elevenlabs".parse::<TtsProvider>().unwrap(),
            TtsProvider::Elevenlabs
        );
        assert_eq!(
            "openai".parse::<TtsProvider>().unwrap(),
            TtsProvider::Openai
        );
        assert!("unknown".parse::<TtsProvider>().is_err());
    }

    #[test]
    fn rate_limiter_allows_first_utterance() {
        let _g = RATE_LOCK.lock().unwrap();
        reset_rate_limiter();
        assert!(!is_rate_limited(5, "first"));
    }

    #[test]
    fn rate_limiter_blocks_duplicate_within_window() {
        let _g = RATE_LOCK.lock().unwrap();
        reset_rate_limiter();
        assert!(!is_rate_limited(5, "dup"));
        assert!(is_rate_limited(5, "dup"));
    }

    #[test]
    fn rate_limiter_disabled_when_zero() {
        let _g = RATE_LOCK.lock().unwrap();
        reset_rate_limiter();
        assert!(!is_rate_limited(0, "zero"));
        assert!(!is_rate_limited(0, "zero"));
    }

    #[test]
    fn xml_escape_handles_special_chars() {
        assert_eq!(
            escape_xml("a & b <c> \"d\" 'e'"),
            "a &amp; b &lt;c&gt; &quot;d&quot; &apos;e&apos;"
        );
    }
}
