use anyhow::{anyhow, Result};
use sdl2 as sdl;

mod pipewire;

fn main() -> Result<()> {
    let context = sdl::init().map_err(|e| anyhow!(e))?;
    let mut event_pump = context.event_pump().map_err(|e| anyhow!(e))?;
    let vidya = context.video().map_err(|e| anyhow!(e))?;

    let w = 1024;
    let h = 600;

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

    let mut pixels: Vec<u8> = Vec::with_capacity((w * h * 3) as usize);
    let mut i = 0;

    println!("{:?}", tex.query());
    println!("{:?}", tex.color_mod());
    println!("{:?}", tex.alpha_mod());

    'renderloop: loop {
        i += 20;
        pixels.clear();
        for y in 0..h {
            let ynorm = (y + i) as f64 / h as f64;
            let v = (f64::cos(ynorm) * 127.0 + 127.0) as u8;
            // println!("{v}");
            for x in 0..w {
                pixels.push(0);
                pixels.push(0);
                pixels.push(v);
            }
        }
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
        tex.update(None, &pixels, (w * 3) as usize)?;
        canvas.copy(&tex, None, None).map_err(|e| anyhow!(e))?;
        canvas.present();
    }
    Ok(())
}
