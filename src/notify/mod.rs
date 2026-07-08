//! Audio notifications and text-to-speech (TTS) support for kaptaind.
//!
//! The `audio` submodule supports multiple providers driven by environment variables:
//!
//! - `system` — local TTS (`say` on macOS, `espeak` on Linux, PowerShell on Windows).
//! - `elevenlabs` — `ELEVENLABS_API_KEY`, optional `ELEVENLABS_VOICE_ID`.
//! - `openai` — `OPENAI_API_KEY`, optional `OPENAI_TTS_MODEL` / `OPENAI_TTS_VOICE`.
//! - `azure` — `AZURE_SPEECH_KEY`, `AZURE_SPEECH_REGION`.
//! - `google` — `GOOGLE_API_KEY` (Cloud Text-to-Speech) or `GOOGLE_APPLICATION_CREDENTIALS_JSON`.
//! - `cartesia` — `CARTESIA_API_KEY`, optional `CARTESIA_VOICE_ID`.

pub mod audio;
