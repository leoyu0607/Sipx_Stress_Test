/// 音檔讀取與 G.711 編碼
/// 支援：
///   - WAV（8kHz/16kHz mono PCM16，自動重新取樣至 8kHz）
///   - raw G.711 μ-law（直接使用）
/// 輸出：循環播放的 160-byte（20ms @ 8kHz）G.711 PCMU 封包流

use anyhow::{bail, Context, Result};
use std::{fs, path::Path};

/// 音訊來源（已解碼為 G.711 PCMU bytes，可循環）
pub struct AudioSource {
    /// 所有 frames（每個 frame = 160 bytes = 20ms）
    frames:      Vec<Vec<u8>>,
    /// 目前播放位置
    cursor:      usize,
    /// 是否循環播放
    pub looping: bool,
}

impl AudioSource {
    /// 從檔案載入（自動偵測格式）
    pub fn from_file(path: &Path) -> Result<Self> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        let pcmu_bytes = match ext.as_str() {
            "wav"  => Self::load_wav(path)?,
            "ul"   |
            "ulaw" |
            "pcmu" |
            "raw"  => {
                // 直接讀取 raw G.711 μ-law
                fs::read(path).with_context(|| format!("無法讀取音檔: {}", path.display()))?
            }
            other => bail!("不支援的音檔格式: .{}（支援 .wav / .raw / .ul）", other),
        };

        let frames = Self::split_frames(pcmu_bytes);
        if frames.is_empty() {
            bail!("音檔內容為空或太短（最少需要 160 bytes / 20ms）");
        }

        Ok(Self { frames, cursor: 0, looping: true })
    }

    /// 產生靜音來源（用於不提供音檔時）
    pub fn silence() -> Self {
        // 20ms 靜音 = 160 個 PCMU 靜音值（0xFF = G.711 μ-law 零電平）
        let frame = vec![0xFFu8; 160];
        Self { frames: vec![frame; 50], cursor: 0, looping: true }
    }

    /// 取得下一個 20ms frame（160 bytes）
    /// 返回 None 表示播放結束（非循環模式）
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        if self.cursor >= self.frames.len() {
            if self.looping {
                self.cursor = 0;
            } else {
                return None;
            }
        }
        let frame = self.frames[self.cursor].clone();
        self.cursor += 1;
        Some(frame)
    }

    /// 重置到開頭
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// 總 frame 數（每 frame 20ms）
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// 音檔總時長（秒）
    pub fn duration_secs(&self) -> f64 {
        self.frames.len() as f64 * 0.020
    }

    // ── 私有：WAV 解析 ────────────────────────────────────────────

    fn load_wav(path: &Path) -> Result<Vec<u8>> {
        let data = fs::read(path)
            .with_context(|| format!("無法讀取 WAV 檔案: {}", path.display()))?;

        // 最小 WAV header = 44 bytes
        if data.len() < 44 {
            bail!("WAV 檔案過小（< 44 bytes）");
        }

        // RIFF chunk
        if &data[0..4] != b"RIFF" {
            bail!("不是有效的 WAV 檔案（缺少 RIFF header）");
        }
        if &data[8..12] != b"WAVE" {
            bail!("不是 WAVE 格式");
        }

        // 解析 fmt chunk
        let audio_format = u16::from_le_bytes([data[20], data[21]]);
        let channels     = u16::from_le_bytes([data[22], data[23]]);
        let sample_rate  = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
        let bits         = u16::from_le_bytes([data[34], data[35]]);

        // 只接受 PCM16
        if audio_format != 1 {
            bail!("只支援 PCM WAV（format=1），收到 format={}", audio_format);
        }
        if bits != 16 {
            bail!("只支援 16-bit WAV，收到 {}-bit", bits);
        }
        if channels != 1 && channels != 2 {
            bail!("只支援 mono/stereo WAV");
        }

        // 找 data chunk
        let pcm16 = Self::find_wav_data(&data)?;

        // 若為 stereo，取左聲道
        let mono = if channels == 2 {
            Self::stereo_to_mono(&pcm16)
        } else {
            pcm16
        };

        // 若 sample rate != 8000，做簡單重採樣
        let resampled = if sample_rate != 8000 {
            Self::resample(&mono, sample_rate, 8000)
        } else {
            mono
        };

        // PCM16 → G.711 μ-law
        Ok(resampled.iter().map(|&s| linear_to_ulaw(s)).collect())
    }

    fn find_wav_data(data: &[u8]) -> Result<Vec<i16>> {
        let mut i = 12usize;
        while i + 8 <= data.len() {
            let chunk_id   = &data[i..i+4];
            let chunk_size = u32::from_le_bytes([data[i+4],data[i+5],data[i+6],data[i+7]]) as usize;
            if chunk_id == b"data" {
                let start = i + 8;
                let end   = (start + chunk_size).min(data.len());
                let raw   = &data[start..end];
                // bytes → i16 samples
                let samples = raw.chunks_exact(2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]))
                    .collect();
                return Ok(samples);
            }
            i += 8 + chunk_size;
            if chunk_size % 2 != 0 { i += 1; } // padding byte
        }
        bail!("WAV 檔案中找不到 data chunk");
    }

    fn stereo_to_mono(samples: &[i16]) -> Vec<i16> {
        samples.chunks_exact(2)
            .map(|s| ((s[0] as i32 + s[1] as i32) / 2) as i16)
            .collect()
    }

    /// 線性插值重採樣（src_rate → dst_rate）
    fn resample(samples: &[i16], src_rate: u32, dst_rate: u32) -> Vec<i16> {
        if src_rate == dst_rate { return samples.to_vec(); }
        let ratio  = src_rate as f64 / dst_rate as f64;
        let n      = (samples.len() as f64 / ratio) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = pos - idx as f64;
            let a = *samples.get(idx).unwrap_or(&0) as f64;
            let b = *samples.get(idx + 1).unwrap_or(&0) as f64;
            out.push((a + frac * (b - a)) as i16);
        }
        out
    }

    fn split_frames(data: Vec<u8>) -> Vec<Vec<u8>> {
        data.chunks(160)
            .filter(|c| c.len() == 160)
            .map(|c| c.to_vec())
            .collect()
    }
}

// ── G.711 μ-law 編碼（ITU-T G.711）──────────────────────────────

/// 16-bit linear PCM → 8-bit μ-law
pub fn linear_to_ulaw(sample: i16) -> u8 {
    const BIAS: i32 = 0x84;
    const CLIP: i32 = 32635;

    let sign  = if sample < 0 { 0x80u8 } else { 0u8 };
    let mut s = sample as i32;
    if s < 0 { s = -s; }
    s += BIAS;
    if s > CLIP { s = CLIP; }

    let exponent = ULAW_EXP_TABLE[((s >> 7) & 0xFF) as usize];
    let mantissa = ((s >> (exponent as u32 + 3)) & 0x0F) as u8;
    let ulaw     = !(sign | ((exponent << 4) as u8) | mantissa);
    ulaw
}

/// μ-law → 16-bit linear（用於 MOS 計算對比）
pub fn ulaw_to_linear(ulaw: u8) -> i16 {
    let ulaw   = !ulaw;
    let sign   = ulaw & 0x80;
    let exp    = ((ulaw >> 4) & 0x07) as u32;
    let mant   = (ulaw & 0x0F) as i32;
    let mut s  = ((mant << 1) | 33) << (exp + 2);
    s         -= 33;
    if sign != 0 { -s as i16 } else { s as i16 }
}

static ULAW_EXP_TABLE: [u8; 256] = [
    0,0,1,1,2,2,2,2,3,3,3,3,3,3,3,3,
    4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,
    5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,
    5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,
    6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,
    6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,
    6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,
    6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,
    7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
    7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
    7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
    7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
    7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
    7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
    7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
    7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
];
