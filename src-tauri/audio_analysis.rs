//! Beat tracking, from the audio itself.
//!
//! What "Audio Analysis" did before: split the lyrics into lines, read `song.duration`, and cut the
//! song into that many equal slices. It never opened the audio. That is why it finished in a couple
//! of seconds, and why — when the engine reported a 500-second song and delivered a 120-second one —
//! every section landed roughly four times too late, taking the images and the video with it.
//!
//! So the duration is now measured rather than believed, and the cuts land on beats.
//!
//! The method is deliberately plain, because it has to run on whatever machine the app is installed
//! on with no extra dependencies: ffmpeg decodes to mono PCM, and everything after that is
//! arithmetic over a Vec<f32>.
//!
//!   1. RMS per short frame → a coarse loudness envelope.
//!   2. Half-wave-rectified first difference of that envelope → onset strength. Rises in energy are
//!      note attacks; falls are not, which is why the negative half is discarded.
//!   3. Autocorrelation of the onset envelope over lags spanning 60–180 BPM → the beat period. A
//!      periodic pulse train correlates with itself one beat later, and nothing else in the signal
//!      does so as strongly.
//!   4. The best phase for that period, chosen by summing onset strength over each candidate grid.
//!
//! This is an energy-flux tracker, and it is honest about what that means: it locks on well to music
//! with a clear percussive pulse and drifts on rubato or beatless material. Both are reported —
//! `confidence` is the autocorrelation peak's height relative to the envelope's own energy, so a
//! caller can tell a locked-on grid from a guess instead of being handed a number that looks
//! authoritative either way.

use std::process::Stdio;

/// Decoded PCM the analyser works on. Mono, because beat tracking gains nothing from stereo.
const SAMPLE_RATE: u32 = 22_050;
/// ~46 ms of audio. Long enough that one drum hit lands inside a frame, short enough to place it.
const FRAME: usize = 1024;
/// ~11.6 ms between envelope samples — finer than a listener can place a beat.
const HOP: usize = 256;
/// The tempo range worth searching. Below 60 BPM the grid is indistinguishable from half-time, above
/// 180 from double-time, and both extremes are outside anything this app generates.
const MIN_BPM: f64 = 60.0;
const MAX_BPM: f64 = 180.0;

#[derive(Debug, Clone)]
pub struct Beats {
    /// Real playing time of the decoded audio, in seconds — measured, not taken from a request.
    pub duration: f64,
    pub tempo_bpm: f64,
    /// Beat positions in seconds, from the first detected beat to the end of the audio.
    pub beats: Vec<f64>,
    /// 0.0–1.0. How strongly the onset envelope actually repeats at the detected period.
    pub confidence: f64,
}

/// Decode anything ffmpeg can read — a local file or an https URL — to mono f32 samples.
///
/// The URL case is why this shells out rather than parsing an mp3: the engines hand back a tunnel
/// URL, and ffmpeg will stream it without the app having to download and manage a temp file.
pub async fn decode_mono(ffmpeg: &str, input: &str) -> Result<Vec<f32>, String> {
    let out = tokio::process::Command::new(ffmpeg)
        .args([
            "-hide_banner", "-loglevel", "error",
            "-i", input,
            "-vn",
            "-ac", "1",
            "-ar", &SAMPLE_RATE.to_string(),
            "-f", "s16le",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|err| format!("could not run ffmpeg ({ffmpeg}): {err}"))?;

    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(format!("ffmpeg could not decode the audio: {}",
                           why.trim().chars().take(200).collect::<String>()));
    }
    if out.stdout.len() < 4 {
        return Err("ffmpeg returned no audio samples".into());
    }
    Ok(out.stdout
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect())
}

/// Frame-wise RMS, then half-wave-rectified difference: the onset strength envelope.
fn onset_envelope(samples: &[f32]) -> Vec<f32> {
    if samples.len() < FRAME { return Vec::new(); }
    let frames = (samples.len() - FRAME) / HOP + 1;
    let mut rms = Vec::with_capacity(frames);
    for i in 0..frames {
        let start = i * HOP;
        let sum: f32 = samples[start..start + FRAME].iter().map(|s| s * s).sum();
        rms.push((sum / FRAME as f32).sqrt());
    }
    // Only rises count. A note starting is an increase in energy; the decay that follows is not a
    // second onset, and leaving it in would put a spurious peak after every real one.
    let mut env = Vec::with_capacity(rms.len());
    env.push(0.0);
    for i in 1..rms.len() {
        env.push((rms[i] - rms[i - 1]).max(0.0));
    }
    env
}

/// Autocorrelation peak over the plausible tempo range → (period in frames, normalised strength).
fn dominant_period(env: &[f32]) -> Option<(usize, f64)> {
    let fps = SAMPLE_RATE as f64 / HOP as f64;
    let min_lag = (fps * 60.0 / MAX_BPM).round() as usize;
    let max_lag = (fps * 60.0 / MIN_BPM).round() as usize;
    if env.len() <= max_lag + 2 { return None; }

    let energy: f64 = env.iter().map(|v| (*v as f64) * (*v as f64)).sum();
    if energy <= f64::EPSILON { return None; }

    let mut best = (0usize, f64::MIN);
    for lag in min_lag..=max_lag {
        let mut acc = 0.0f64;
        for i in lag..env.len() {
            acc += env[i] as f64 * env[i - lag] as f64;
        }
        // Normalise by overlap so long lags are not penalised for comparing fewer samples.
        let score = acc / (env.len() - lag) as f64;
        if score > best.1 { best = (lag, score); }
    }
    if best.0 == 0 { return None; }
    let mean_energy = energy / env.len() as f64;
    let confidence = if mean_energy > 0.0 { (best.1 / mean_energy).min(1.0) } else { 0.0 };
    Some((best.0, confidence.clamp(0.0, 1.0)))
}

/// Best alignment of a grid of `period` frames: the offset whose beat positions carry most onset.
fn best_phase(env: &[f32], period: usize) -> usize {
    let mut best = (0usize, f64::MIN);
    for offset in 0..period {
        let mut acc = 0.0f64;
        let mut i = offset;
        while i < env.len() {
            acc += env[i] as f64;
            i += period;
        }
        if acc > best.1 { best = (offset, acc); }
    }
    best.0
}

/// Find the beat grid of already-decoded samples.
pub fn track_beats(samples: &[f32]) -> Beats {
    let duration = samples.len() as f64 / SAMPLE_RATE as f64;
    let env = onset_envelope(samples);
    let fps = SAMPLE_RATE as f64 / HOP as f64;

    let Some((period, confidence)) = dominant_period(&env) else {
        return Beats { duration, tempo_bpm: 0.0, beats: Vec::new(), confidence: 0.0 };
    };
    let offset = best_phase(&env, period);
    let period_s = period as f64 / fps;
    let tempo_bpm = 60.0 / period_s;

    let mut beats = Vec::new();
    let mut t = offset as f64 / fps;
    while t < duration {
        beats.push((t * 1000.0).round() / 1000.0);
        t += period_s;
    }
    Beats { duration, tempo_bpm: (tempo_bpm * 10.0).round() / 10.0, beats, confidence }
}

/// Decode and analyse in one step.
pub async fn analyze(ffmpeg: &str, input: &str) -> Result<Beats, String> {
    let samples = decode_mono(ffmpeg, input).await?;
    Ok(track_beats(&samples))
}

/// Move a boundary onto the nearest beat, but never far enough to reorder or collapse a section.
///
/// The guard matters more than the snapping. Lyric lines are not evenly spaced in time, so an
/// unconstrained snap can pull two neighbouring boundaries onto the same beat and produce a section
/// of zero length — which downstream becomes an image with no time on screen. A boundary that would
/// have to move more than `max_shift` to reach a beat keeps its original position instead.
pub fn snap_to_beat(t: f64, beats: &[f64], max_shift: f64) -> f64 {
    if beats.is_empty() { return t; }
    let mut best = t;
    let mut best_d = f64::MAX;
    for b in beats {
        let d = (b - t).abs();
        if d < best_d { best_d = d; best = *b; }
    }
    if best_d <= max_shift { best } else { t }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A click track at a known tempo must come back at that tempo.
    ///
    /// Synthetic rather than a fixture because the property under test is arithmetic: if the
    /// autocorrelation is reading the envelope correctly, 120 BPM of impulses is 120 BPM out.
    fn click_track(bpm: f64, seconds: f64) -> Vec<f32> {
        let n = (SAMPLE_RATE as f64 * seconds) as usize;
        let period = (SAMPLE_RATE as f64 * 60.0 / bpm) as usize;
        let mut v = vec![0.0f32; n];
        let mut i = 0;
        while i < n {
            // A short decaying burst, which is what an onset detector is meant to see.
            for k in 0..(SAMPLE_RATE as usize / 100) {
                if i + k < n {
                    let decay = 1.0 - (k as f32 / (SAMPLE_RATE as f32 / 100.0));
                    v[i + k] = decay * if k % 2 == 0 { 0.8 } else { -0.8 };
                }
            }
            i += period;
        }
        v
    }

    #[test]
    fn a_click_track_reports_its_own_tempo() {
        for bpm in [90.0, 120.0, 140.0] {
            let b = track_beats(&click_track(bpm, 20.0));
            assert!((b.tempo_bpm - bpm).abs() <= 3.0,
                    "expected ~{bpm} BPM, detected {} BPM", b.tempo_bpm);
            assert!(b.confidence > 0.0, "a clean click track should not report zero confidence");
            assert!(b.beats.len() > 10, "expected a full grid, got {} beats", b.beats.len());
        }
    }

    /// The duration must come from the samples, which is the whole point — the old analysis trusted
    /// a number the engine had echoed back from the request and mistimed everything downstream.
    #[test]
    fn duration_is_measured_from_the_samples() {
        let b = track_beats(&click_track(120.0, 7.5));
        assert!((b.duration - 7.5).abs() < 0.05, "measured {}s, expected 7.5s", b.duration);
    }

    /// Silence has no beat, and saying so is better than inventing one.
    #[test]
    fn silence_reports_no_beat_rather_than_a_guess() {
        let b = track_beats(&vec![0.0f32; SAMPLE_RATE as usize * 5]);
        assert!(b.beats.is_empty());
        assert_eq!(b.confidence, 0.0);
        assert!((b.duration - 5.0).abs() < 0.05);
    }

    /// Too short to hold a bar: report nothing rather than a tempo derived from two frames.
    #[test]
    fn a_clip_shorter_than_the_search_window_is_not_forced_into_a_tempo() {
        let b = track_beats(&click_track(120.0, 0.4));
        assert!(b.beats.is_empty(), "a 0.4s clip cannot support a 60-180 BPM search");
    }

    #[test]
    fn snapping_moves_a_boundary_to_a_nearby_beat_and_leaves_a_distant_one_alone() {
        let beats = vec![0.0, 0.5, 1.0, 1.5, 2.0];
        assert_eq!(snap_to_beat(1.04, &beats, 0.25), 1.0);
        // 3.0 is far past the last beat: snapping it back to 2.0 would be a 1-second lie.
        assert_eq!(snap_to_beat(3.0, &beats, 0.25), 3.0);
    }
}
