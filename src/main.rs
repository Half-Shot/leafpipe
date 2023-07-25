use std::ops::Sub;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use std::{net::UdpSocket, vec};
use colors_transform::{Color, Hsl};
use nanoleaf::{NanoleafLayoutResponse, NanoleafEffectPayload};
use pipewire::spa::Direction;
use pipewire::spa::pod::Value;
use pipewire::spa::pod::deserialize::PodDeserializer;
use pipewire::spa::utils::Id;
use pipewire::stream::StreamFlags;
use pipewire::{MainLoop, Context};
use vis::BufferManager;
use std::thread;
use pipewire::prelude::*;
use pipewire::properties;
use influxdb::{Client as InfluxDBClient, WriteQuery};
use influxdb::InfluxDbWriteable;
use chrono::{DateTime, Utc};

use crate::audio::Fixate;
use crate::slidingwindow::SlidingWindow;
use crate::nanoleaf::NanoleafClient;

mod audio;
mod slidingwindow;
mod vis;
mod nanoleaf;

#[derive(InfluxDbWriteable)]
struct PanelValues {
    time: DateTime<Utc>,
    r: u8,
    g: u8,
    b: u8,
    i: f32,
    panel_index: u16,
}

#[derive(Debug)]
struct StreamConfiguration {
	rate: u32,
	channels: usize
}

#[derive(Default)]
struct StreamData {
	configuration: Option<StreamConfiguration>,
    buffer_manager: Arc<RwLock<BufferManager>>,
}

const LIGHT_INTERVAL: Duration = Duration::from_millis(100);

async fn update_lights(nanoleaf: NanoleafClient, buffer_manager: Arc<RwLock<BufferManager>>, client: InfluxDBClient) {
    let now: Instant = Instant::now();
    let mut last_metric_time = Instant::now();

    let panels = nanoleaf.get_panels().await.unwrap();

    // Needs to be over a sliding window.
    let mut window = SlidingWindow::new(512);
    loop { 
        let process_start = Instant::now();
        {
            if let Some(data) = buffer_manager.write().unwrap().fft_interval::<10>(LIGHT_INTERVAL) {
                let mut effect = NanoleafEffectPayload::new(panels.num_panels);

                let hue = ((now.elapsed().as_secs_f32() / 10.0).sin() * 180.0) + 180.0;
                let saturation = ((now.elapsed().as_secs_f32() / 10.0).sin() * -180.0) + 180.0;
                let mut panel_index: usize = 0;
                let mut readings: Vec<WriteQuery> = Vec::new();

                for panel in &panels.position_data {
                    let (min, max) = window.submit_new(data[panel_index]);
                    let intensity = (((data[panel_index] + min) / max) * 25f32 * (panel_index as f32 + 1.0f32).powf(1.05f32)).clamp(5.0, 80.0);
                    let hsl = Hsl::from(hue, saturation, intensity);
                    let rgb = hsl.to_rgb().as_tuple();
                    let r = unsafe { rgb.0.to_int_unchecked::<u8>() };
                    let g = unsafe { rgb.1.to_int_unchecked::<u8>() };
                    let b = unsafe { rgb.2.to_int_unchecked::<u8>() };

                    readings.push(PanelValues {
                        time: Utc::now(),
                        r: r,
                        g: g,
                        b: b,
                        i: intensity,
                        panel_index: panel_index as u16,
                    }.into_query("panel_rgb"));

                    effect.write_effect(panel.panel_id, r, g, b, 1);
                    panel_index += 1;
                }
                if let Err(err) = nanoleaf.send_effect(&effect) {
                    println!("Failed to send effect to nanoleaf {:?}", err);
                }

                // Occasionally send metrics.
                if last_metric_time.elapsed().as_millis() > 500 {
                    last_metric_time = Instant::now();
                }
            } else {
                println!("No data");
            }
        }

        let sleep_duration = LIGHT_INTERVAL.sub(process_start.elapsed());
        if sleep_duration.ge(&Duration::ZERO) {
            thread::sleep(sleep_duration);
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let influx = InfluxDBClient::new("http://localhost:8086", "leafpipe").with_token("qE5qMT6shkRhT6KylDRU864Izv_MGC2qeWuebjTEN2IIpWoFnxfsKd07GahqhIBe2wtBMlhSVa2zC3ip5I2zww==");
    let nanoleaf: NanoleafClient = NanoleafClient::connect("GuHySGmkcf2zrFhsdEZrax19QhYL01ge", "192.168.1.132", 16021).await.unwrap();
    let panels = nanoleaf.get_panels().await.unwrap();
    println!("{:?}", panels);
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("192.168.1.132:60222")?;

    // 0 3  ‚---> nPanels
    // 1 118 255 0 255 0 0 12  ‚---> Set panel color
    // 2 139 255 255 0 0 0 128  ‚---> Set panel color
    // 0 235 0 255 255 0 1 195 ‚---> Set panel color


    let mainloop = MainLoop::new().unwrap();
    let context = Context::new(&mainloop).unwrap();
    let core = context.connect(None).unwrap();
    let registry = core.get_registry().unwrap();
    let buffer_manager: Arc<RwLock<BufferManager>> = Arc::new(RwLock::new(BufferManager::default()));
    let buffer_manager_lights = buffer_manager.clone();

    // Register a callback to the `global` event on the registry, which notifies of any new global objects
    // appearing on the remote.
    // The callback will only get called as long as we keep the returned listener alive.
    let _listener = registry
        .add_listener_local()
        .register();

    let stream = pipewire::stream::Stream::<StreamData>::with_user_data(
        &mainloop,
        "audio-cap",
        properties! {
            *pipewire::keys::MEDIA_TYPE => "Audio",
            *pipewire::keys::MEDIA_CATEGORY => "Capture",
            *pipewire::keys::MEDIA_ROLE => "Music",
        },
        StreamData {
            configuration: None,
            buffer_manager: buffer_manager,
        },
    )
	.param_changed(|id, data, raw_pod| {
		if id == libspa_sys::SPA_PARAM_Format {
			let pointer = std::ptr::NonNull::new(raw_pod.cast_mut()).unwrap();
			let object = unsafe {
				PodDeserializer::deserialize_ptr::<Value>(pointer).unwrap()
			};

			if let Value::Object(object) = object {
				data.configuration = None;

				let media_type: Id = object.properties.iter()
					.find(|p| p.key == libspa_sys::SPA_FORMAT_mediaType)
					.unwrap().value
					.fixate().unwrap();
				
				let media_subtype: Id = object.properties.iter()
					.find(|p| p.key == libspa_sys::SPA_FORMAT_mediaSubtype)
					.unwrap().value
					.fixate().unwrap();
				
				let format: Id = object.properties.iter()
					.find(|p| p.key == libspa_sys::SPA_FORMAT_AUDIO_format)
					.unwrap().value
					.fixate().unwrap();
				
				let rate: i32 = object.properties.iter()
					.find(|p| p.key == libspa_sys::SPA_FORMAT_AUDIO_rate)
					.unwrap().value
					.fixate().unwrap();
				
				let channels: i32 = object.properties.iter()
					.find(|p| p.key == libspa_sys::SPA_FORMAT_AUDIO_channels)
					.unwrap().value
					.fixate().unwrap();
				
				let is_audio = media_type.0 == libspa_sys::SPA_MEDIA_TYPE_audio;
				let is_raw = media_subtype.0 == libspa_sys::SPA_MEDIA_SUBTYPE_raw;
				if is_audio && is_raw {
					data.configuration = Some(StreamConfiguration {
						rate: rate as u32,
						channels: channels as usize,
					});
                    println!("config: {:?}", data.configuration);
				}
			}
		}
	})
    .process(|_stream, stream_data| {
		if let Some(mut buffer) = _stream.dequeue_buffer() {
            let config = stream_data.configuration.as_ref().unwrap();
			// TODO: this is just the left channel — maybe handle all channels
            for channel_index in 0..config.channels-1 {
                let channel = buffer.datas_mut().get_mut(channel_index).unwrap();
                let chunk = channel.chunk(); 
                let size = chunk.size() as usize;
                let data = channel.data(); 
                if let Some(data) = data {
    
                    let cast_buffer: &[f32] = unsafe {
                        std::slice::from_raw_parts(data.as_ptr().cast(), size / std::mem::size_of::<f32>())
                    };
                    stream_data.buffer_manager.write().unwrap().fill_buffer(cast_buffer, config.rate);
                }
            }
		}
    })
    .create().unwrap();

	let params = audio::SpaAudioInfoRaw::empty().as_pod().unwrap();

	stream.connect(
		Direction::Input,
		None,
		StreamFlags::AUTOCONNECT | StreamFlags::RT_PROCESS | StreamFlags::MAP_BUFFERS,
		&mut [params.as_ptr().cast()],
	).unwrap();
    tokio::spawn(async move { update_lights(nanoleaf, buffer_manager_lights, influx).await });

    mainloop.run();
    stream.disconnect().unwrap();
    Ok(())
}

