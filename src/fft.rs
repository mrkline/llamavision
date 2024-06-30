use anyhow::{Context, Result};
use fftw::{
    array::AlignedVec,
    plan::{R2CPlan, R2CPlan32},
    types::{c32, Flag},
};

use std::sync::mpsc::Receiver;

pub fn run(width: usize, audio_rx: Receiver<Vec<i16>>) -> Result<()> {
    let in_width = width * 2;
    let mut ins = AlignedVec::<f32>::new(in_width);
    let mut outs = AlignedVec::<c32>::new(width + 1);
    let mut plan = R2CPlan32::new(&[in_width], &mut ins, &mut outs, Flag::MEASURE)
        .context("fftw plan failed")?;

    // Preallocate a bit more since otherwise we'd resize unless in_width is a direct multiple
    // of audio buffer lengths.
    let mut normalized_audio: Vec<f32> = Vec::with_capacity(width * 3);

    while let Ok(samps) = audio_rx.recv() {
        for s in samps {
            let normed = s as f64 / i16::MAX as f64;
            // Guard against s being i16::MIN (yay two's comp)
            normalized_audio.push(normed.max(-1.0) as f32);
        }
        if normalized_audio.len() >= in_width {
            let row = fft(&normalized_audio, &mut plan, &mut ins, &mut outs)?;
            assert_eq!(row.len(), width);
            // TODO: send row to UI
            normalized_audio.clear();
        }
    }
    Ok(())
}

fn fft(
    normalized_audio: &[f32],
    plan: &mut R2CPlan32,
    ins: &mut AlignedVec<f32>,
    outs: &mut AlignedVec<c32>,
) -> Result<Vec<f32>> {
    // TODO: WINDOW!
    let in_len = ins.len();
    ins.clone_from_slice(&normalized_audio[..in_len]);
    plan.r2c(ins, outs).context("fft failed")?;
    // Get magnitude and normalize.
    let normalize_by = in_len as f32; // sqrt this?
    let mut normalized = Vec::with_capacity(outs.len() - 1); // Skip DC
    for o in &outs[1..] {
        normalized.push(o.norm() / normalize_by);
    }
    Ok(normalized)
}
