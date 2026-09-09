use windows::core::{Error, Result, GUID, PCWSTR};
use windows::Win32::Foundation::{E_FAIL, HGLOBAL};
use windows::Win32::Media::Audio::WAVEFORMATEX;
use windows::Win32::Media::Speech::{
    ISpStream, ISpVoice, SpStream, SPF_ASYNC, SPF_IS_NOT_XML, SPF_IS_XML, SPF_PURGEBEFORESPEAK,
    SPVOICESTATUS,
};
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL, STREAM_SEEK_SET};

use crate::text;

const WAVE_FORMAT: GUID = GUID::from_u128(0xc31adbae_527f_4ff5_a230_f62bb61ff70c);
const SAMPLE_RATE: u32 = 24_000;
const BYTES_PER_SAMPLE: u16 = 2;
const TAIL_MILLISECONDS: u32 = 2_000;

pub(crate) struct Playback {
    text: String,
    speech_stream: u32,
    silence_stream: u32,
    // Keep the queued input stream alive until this playback is replaced.
    _silence: ISpStream,
}

impl Playback {
    pub(crate) fn is_in_tail(&self, status: &SPVOICESTATUS) -> bool {
        status.ulCurrentStream == self.silence_stream
    }

    pub(crate) fn remaining_text(&self, status: &SPVOICESTATUS) -> Option<&str> {
        if self.is_in_tail(status) {
            return None;
        }
        // Before the new stream reaches the device, its first word is pending.
        let position = if status.ulCurrentStream == self.speech_stream {
            status.ulInputWordPos as usize
        } else {
            0
        };
        let remaining = from_utf16_position(&self.text, position);
        (!remaining.trim().is_empty()).then_some(remaining)
    }
}

fn from_utf16_position(text: &str, position: usize) -> &str {
    let mut units = 0;
    for (offset, character) in text.char_indices() {
        if units + character.len_utf16() > position {
            return &text[offset..];
        }
        units += character.len_utf16();
    }
    ""
}

pub(crate) unsafe fn silence_stream() -> Result<ISpStream> {
    let format = WAVEFORMATEX {
        wFormatTag: 1, // Uncompressed signed 16-bit PCM.
        nChannels: 1,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * u32::from(BYTES_PER_SAMPLE),
        nBlockAlign: BYTES_PER_SAMPLE,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    let bytes =
        vec![0u8; (SAMPLE_RATE * u32::from(BYTES_PER_SAMPLE) * TAIL_MILLISECONDS / 1_000) as usize];
    let memory = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
    let mut written = 0;
    memory
        .Write(
            bytes.as_ptr().cast(),
            bytes.len() as u32,
            Some(&mut written),
        )
        .ok()?;
    if written as usize != bytes.len() {
        return Err(Error::new(
            E_FAIL,
            "Could not prepare the audio tail.".into(),
        ));
    }
    memory.Seek(0, STREAM_SEEK_SET, None)?;
    let stream: ISpStream = CoCreateInstance(&SpStream, None, CLSCTX_ALL)?;
    stream.SetBaseStream(&memory, &WAVE_FORMAT, &format)?;
    Ok(stream)
}

pub(crate) unsafe fn start(engine: &ISpVoice, natural: bool, processed: &str) -> Result<Playback> {
    let silence = silence_stream()?;
    let (payload, mode) = if natural {
        (processed.to_owned(), SPF_IS_NOT_XML)
    } else {
        (text::to_sapi_xml(processed), SPF_IS_XML)
    };
    let wide: Vec<u16> = payload.encode_utf16().chain(Some(0)).collect();
    let mut speech_stream = 0;
    engine.Speak(
        PCWSTR(wide.as_ptr()),
        (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0 | mode.0) as u32,
        Some(&mut speech_stream),
    )?;
    let mut silence_stream = 0;
    // Queue PCM on the SAME voice/output immediately after the text. A sleep or
    // a silence XML tag cannot provide this tail for embedded Narrator voices.
    if let Err(error) = engine.SpeakStream(&silence, SPF_ASYNC.0 as u32, Some(&mut silence_stream))
    {
        let _ = engine.Speak(None, SPF_PURGEBEFORESPEAK.0 as u32, None);
        return Err(error);
    }
    Ok(Playback {
        text: processed.to_owned(),
        speech_stream,
        silence_stream,
        _silence: silence,
    })
}

#[cfg(test)]
mod tests {
    use super::from_utf16_position;

    #[test]
    fn resumes_at_utf16_word_positions_without_splitting_unicode() {
        let text = "A 🦀 café next";
        assert_eq!(from_utf16_position(text, 0), text);
        assert_eq!(from_utf16_position(text, 2), "🦀 café next");
        assert_eq!(from_utf16_position(text, 3), "🦀 café next");
        assert_eq!(from_utf16_position(text, 5), "café next");
        assert_eq!(from_utf16_position(text, 10), "next");
        assert_eq!(from_utf16_position(text, 99), "");
    }
}
