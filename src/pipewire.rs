use anyhow::Result;
use pipewire as pw;
use pw::spa;
use tracing::*;

use std::sync::mpsc::{SyncSender, TrySendError};

struct UserData {
    tx: SyncSender<Vec<i16>>,
}

pub fn run(tx: SyncSender<Vec<i16>>) -> Result<()> {
    println!("Hello, world!");
    let mainloop = pw::main_loop::MainLoop::new(None)?;
    let context = pw::context::Context::new(&mainloop)?;
    let core = context.connect(None)?;

    let props = pipewire::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::STREAM_CAPTURE_SINK => "true", // configurable source as arg?
        *pw::keys::NODE_ALWAYS_PROCESS => "true",
        *pw::keys::NODE_LATENCY => "512/44100",
    };

    let stream = pw::stream::Stream::new(&core, "llamavision", props)?;

    let data = UserData { tx };

    let _listener = stream
        .add_local_listener_with_user_data(data)
        .param_changed(|_, _user_data, id, param| {
            // NULL means to clear the format
            let Some(param) = param else {
                return;
            };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }

            let (media_type, media_subtype) = match spa::param::format_utils::parse_format(param) {
                Ok(v) => v,
                Err(_) => return,
            };

            if media_type != spa::param::format::MediaType::Audio
                || media_subtype != spa::param::format::MediaSubtype::Raw
            {
                return;
            }

            let mut format = spa::param::audio::AudioInfoRaw::new();
            format
                .parse(param)
                .expect("Failed to parse param changed to AudioInfoRaw");

            assert_eq!(format.format(), spa::param::audio::AudioFormat::S16LE);
            assert_eq!(format.rate(), 44100);
            assert_eq!(format.channels(), 1);
        })
        .process(|stream, user_data| match stream.dequeue_buffer() {
            None => println!("out of buffers"),
            Some(mut buffer) => {
                let datas = buffer.datas_mut();
                if datas.is_empty() {
                    return;
                }

                // Lots-o-cargo cult from the examples, but let's assert some assumptions.
                assert_eq!(datas.len(), 1); // <= 1 buffer per callback
                let data = &mut datas[0];

                let chunk_size = data.chunk().size() as usize;
                let chunk_off = data.chunk().offset() as usize;
                assert_eq!(data.chunk().stride(), 2); // Stride is 2 bytes per sample (s16)

                // Data is some yuge buffer. Pick out the samples specified by the chunk,
                // starting at its offset.
                if let Some(d) = data.data() {
                    let sample_bytes = &d[chunk_off..(chunk_off + chunk_size)];
                    // Cast to a slice of [i16]. If we haven't fucked up the math,
                    // we should have 0 bytes before or after.
                    let (unaligned_pre, samples, unaligned_post): (_, &[i16], _) =
                        unsafe { sample_bytes.align_to() };
                    assert_eq!(unaligned_pre.len(), 0);
                    assert_eq!(unaligned_post.len(), 0);
                    trace!("captured {} samps", samples.len());

                    let to_send = samples.to_owned();
                    match user_data.tx.try_send(to_send) {
                        Ok(()) => (),
                        Err(TrySendError::Full(s)) => {
                            warn!("audio queue full; dropping {} samples", s.len())
                        }
                        Err(TrySendError::Disconnected(_)) => {
                            debug!("audio queue hung up; quitting pipewire");
                            let _ = stream.disconnect();
                        }
                    }
                }
            }
        })
        .register()?;

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::S16LE);
    audio_info.set_rate(44100);
    // Downmix to mono please.
    audio_info.set_channels(1);
    let mut position = [0; spa::param::audio::MAX_CHANNELS];
    position[0] = spa::sys::SPA_AUDIO_CHANNEL_MONO;
    audio_info.set_position(position);
    // Dear god pipewire has its own ABI. Cargo culted from pipewire-rs examples:
    let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        // Use object! macro instead? But AudioInfoRaw is into Vec<Properties>
        &spa::pod::Value::Object(spa::pod::Object {
            type_: spa::sys::SPA_TYPE_OBJECT_Format,
            id: spa::sys::SPA_PARAM_EnumFormat,
            properties: audio_info.into(),
        }),
    )
    .unwrap()
    .0
    .into_inner();

    let mut params = [spa::pod::Pod::from_bytes(&values).unwrap()];

    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    mainloop.run();

    Ok(())
}
