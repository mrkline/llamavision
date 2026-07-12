use anyhow::{Context, Result};
use fftw::{
    array::AlignedVec,
    plan::{R2CPlan, R2CPlan32},
    types::{Flag, c32},
};
use tracing::*;

use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::SyncSender,
};

use super::pingpong::PingPongReader;

pub fn run(
    width: usize,
    ready: &AtomicBool,
    audio_rx: PingPongReader<Vec<i16>>,
    rows_tx: SyncSender<Vec<f32>>,
) -> Result<()> {
    let in_width = width * 2;
    let window = blackman_harris(in_width);
    let mut ins = AlignedVec::<f32>::new(in_width);
    let mut outs = AlignedVec::<c32>::new(width + 1);
    let mut plan = R2CPlan32::new(&[in_width], &mut ins, &mut outs, Flag::PATIENT)
        .context("fftw plan failed")?;

    // Preallocate a bit more since otherwise we'd resize unless in_width is a direct multiple
    // of audio buffer lengths.
    let mut normalized_audio = VecDeque::with_capacity(width * 3);

    ready.store(true, Ordering::SeqCst);
    loop {
        let f = |samps: &Vec<i16>| {
            for s in samps {
                normalized_audio.push_back(*s as f32 / i16::MAX as f32);
            }
        };
        audio_rx.read(f);
        while normalized_audio.len() >= in_width {
            let row = fft(&normalized_audio, &window, &mut plan, &mut ins, &mut outs)?;
            assert_eq!(row.len(), width);
            rows_tx.send(row)?;
            // Shave in_width off
            normalized_audio.drain(..(in_width / 2)); // 50% FFT overlap
        }
    }
}

#[allow(non_snake_case)]
fn blackman_harris(width: usize) -> Vec<f32> {
    let a0 = 0.35875;
    let a1 = 0.48829;
    let a2 = 0.14128;
    let a3 = 0.01168;
    let N = width as f32;
    use std::f32::consts::PI;
    let w = |n| {
        a0 - a1 * f32::cos(2.0 * PI * n / N) + a2 * f32::cos(4.0 * PI * n / N)
            - a3 * f32::cos(6.0 * PI * n / N)
    };
    let mut bh = Vec::with_capacity(width);
    for n in 0..width {
        bh.push(w(n as f32));
    }
    bh
}

#[instrument(level = "debug", skip_all)]
fn fft(
    normalized_audio: &VecDeque<f32>,
    window: &[f32],
    plan: &mut R2CPlan32,
    ins: &mut AlignedVec<f32>,
    outs: &mut AlignedVec<c32>,
) -> Result<Vec<f32>> {
    // Window
    for (n, i_n) in ins.iter_mut().enumerate() {
        *i_n = window[n] * normalized_audio[n];
    }

    plan.r2c(ins, outs).context("fft failed")?;
    // Get magnitude and normalize.
    let normalize_by = ins.len() as f32; // sqrt this?
    let mut normalized = Vec::with_capacity(outs.len() - 1); // Skip DC
    for o in &outs[1..] {
        let normed = o.norm() / normalize_by;
        normalized.push(20.0 * f32::log10(normed));
    }
    Ok(normalized)
}
