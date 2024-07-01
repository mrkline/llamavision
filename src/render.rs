use anyhow::{anyhow, Result};
use sdl2 as sdl;
use tracing::*;

use std::collections::VecDeque;
use std::sync::mpsc::Receiver;

struct Row {
    vals: Vec<f32>,
    min: f32,
    max: f32,
}

impl Row {
    fn new(vals: Vec<f32>) -> Self {
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for v in &vals {
            min = min.min(*v);
            max = max.max(*v);
        }
        Self { vals, min, max }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
struct GlobalBounds {
    min: f32,
    max: f32,
}

fn global_bounds<'a>(it: impl Iterator<Item = &'a Row>) -> GlobalBounds {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for r in it {
        min = min.min(r.min);
        max = max.max(r.max);
    }
    GlobalBounds { min, max }
}

pub fn run(width: usize, height: usize, rows_rx: Receiver<Vec<f32>>) -> Result<()> {
    let context = sdl::init().map_err(|e| anyhow!(e))?;
    let mut event_pump = context.event_pump().map_err(|e| anyhow!(e))?;
    let vidya = context.video().map_err(|e| anyhow!(e))?;

    let w = width as u32;
    let h = height as u32;

    let mut canvas = vidya
        .window("llamavision", w, h)
        .resizable()
        .build()?
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()?;

    let tc = canvas.texture_creator();
    let mut tex = tc.create_texture_streaming(sdl::pixels::PixelFormatEnum::RGB24, w, h)?;

    let mut rows = VecDeque::with_capacity(height);
    let mut bounds = None;

    'renderloop: while let Ok(new_row) = rows_rx.recv() {
        assert!(rows.len() <= height);
        if rows.len() == height {
            rows.pop_back();
        }
        rows.push_front(Row::new(new_row));

        // Check dB bounds and possibly recolorize
        let new_bounds = global_bounds(rows.iter());
        if bounds != Some(new_bounds) {
            debug!(
                "Redrawing all; new range [{}, {}]",
                new_bounds.min, new_bounds.max
            );
            // TODO: Redraw everything
            bounds = Some(new_bounds);
        } else {
            // TODO: Incremental refresh
        }

        let b = bounds.unwrap();

        // For now redraw everything
        tex.with_lock(None, |pixels, pitch| {
            for (y, _row) in rows.iter().enumerate() {
                let row = &rows[0];
                for x in 0..width {
                    let Pixel { r, g, b } = colorize(normalize(&b, row.vals[x]));
                    let pixbase = y * pitch + x * 3;
                    pixels[pixbase] = r;
                    pixels[pixbase + 1] = g;
                    pixels[pixbase + 2] = b;
                }
            }
        })
        .map_err(|e| anyhow!(e))?;
        canvas.clear();
        for event in event_pump.poll_iter() {
            use sdl::event::Event;
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(sdl::keyboard::Keycode::Escape),
                    ..
                } => break 'renderloop,
                _ => {}
            }
        }
        canvas.copy(&tex, None, None).map_err(|e| anyhow!(e))?;
        canvas.present();
    }
    Ok(())
}

struct Pixel {
    r: u8,
    g: u8,
    b: u8,
}

fn normalize(bounds: &GlobalBounds, v: f32) -> f32 {
    let min = bounds.min.max(-100.0);
    if v <= min {
        0.0
    } else {
        assert!(bounds.max > f32::NEG_INFINITY);
        let range = bounds.max - min;
        (v - min) / range
    }
}

fn colorize(v: f32) -> Pixel {
    assert!(v >= 0.0 && v <= 1.0, "not normal: {v}");
    Pixel {
        r: 0,
        g: (v * 256.0) as u8,
        b: 0,
    }
}
