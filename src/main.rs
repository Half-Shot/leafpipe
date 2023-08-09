use colors_transform::{Color, Hsl};
use nanoleaf::{NanoleafClient, NanoleafEffectPayload, NanoleafLayoutResponse};
use pipewire::{MainLoop, Context};
use pipewire::prelude::*;
use pipewire::properties;
use pipewire::spa::Direction;
use pipewire::spa::pod::deserialize::PodDeserializer;
use pipewire::spa::pod::Value;
use pipewire::spa::utils::Id;
use pipewire::stream::StreamFlags;
use core::panic;
use std::ops::Sub;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use vis::BufferManager;
use config::{Config, ConfigError};
use mdns_sd::{ServiceDaemon, ServiceEvent};


use crate::audio::Fixate;
use crate::slidingwindow::SlidingWindow;

mod audio;
mod slidingwindow;
mod vis;
mod nanoleaf;

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
const MDNS_TIMEOUT: Duration = Duration::from_secs(30);

async fn update_lights(panels: NanoleafLayoutResponse, nanoleaf: NanoleafClient, buffer_manager: Arc<RwLock<BufferManager>>) {
    let now: Instant = Instant::now();

    // Needs to be over a sliding window.
    let mut window = SlidingWindow::new(64);
    loop { 
        let process_start = Instant::now();
        {
            if let Some(data) = buffer_manager.write().unwrap().fft_interval::<10>(LIGHT_INTERVAL) {
                let mut effect = NanoleafEffectPayload::new(panels.num_panels);

                let hue = ((now.elapsed().as_secs_f32() / 10.0).sin() * 180.0) + 180.0;
                let saturation = ((now.elapsed().as_secs_f32() / 10.0).sin() * -180.0) + 180.0;
                let mut panel_index: usize = 0;

                for panel in &panels.position_data {
                    let (min, max) = window.submit_new(data[panel_index]);
                    let intensity = (((data[panel_index] + min) / max) * 25f32 * (panel_index as f32 + 1.0f32).powf(1.05f32)).clamp(5.0, 80.0);
                    let hsl = Hsl::from(hue, saturation, intensity);
                    let rgb = hsl.to_rgb().as_tuple();
                    let r = unsafe { rgb.0.to_int_unchecked::<u8>() };
                    let g = unsafe { rgb.1.to_int_unchecked::<u8>() };
                    let b = unsafe { rgb.2.to_int_unchecked::<u8>() };
                    effect.write_effect(panel.panel_id, r, g, b, 1);
                    panel_index += 1;
                }
                if let Err(err) = nanoleaf.send_effect(&effect) {
                    println!("Failed to send effect to nanoleaf {:?}", err);
                }
            }
        }

        let sleep_duration = LIGHT_INTERVAL.sub(process_start.elapsed());
        if sleep_duration.ge(&Duration::ZERO) {
            thread::sleep(sleep_duration);
        }
    }
}

fn discover_host(config: &Config) -> (String, u16) {
    match config.get_string("nanoleaf_host") {
        Ok(config_host) => {
            (
                config_host,
                config.get_int("nanoleaf_port").unwrap_or(nanoleaf::DEFAULT_API_PORT.into()).try_into().expect("Provided nanoleaf_port did not fit in range")
            )
        },
        Err(ConfigError::NotFound(_err)) => {
            println!("Discovering nanoleaf via mdns");
            let mdns: ServiceDaemon = ServiceDaemon::new().expect("Failed to create daemon");
            // Browse for a service type.
            let service_type = "_nanoleafapi._tcp.local.";
            let receiver = mdns.browse(service_type).expect("Failed to browse");
            while let Ok(event) = receiver.recv_timeout(MDNS_TIMEOUT) {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        println!("Discovered service {} {:?}", info.get_fullname(), info.get_addresses());
                        let service_ip = info.get_addresses().iter().next().expect("Service found but with no addresses").to_string();
                        return (service_ip, info.get_port());
                    }
                    _ => {
                        // Not interested in other events.
                    }
                }
            }
            panic!("Failed to find nanoleaf");
        }
        Err(err) => {
            println!("Encountered error with config {:?}", err);
            panic!("Unexpected error handling config")
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let config_builder = Config::builder().add_source(config::Environment::with_prefix("LP"));
    
    let config = if let Some(config_file) = xdg::BaseDirectories::with_prefix("leafpipe").unwrap().find_config_file("config.toml") {
        config_builder.add_source(config::File::from(config_file)).build().unwrap()
    } else {
        config_builder.add_source(config::File::with_name("config.toml")).build().unwrap()
    };

    let service = discover_host(&config);
    println!("Discovered nanoleaf on {}:{}", service.0, service.1);


    let nanoleaf: NanoleafClient = NanoleafClient::connect(
        config.get_string("nanoleaf_token").expect("Missing nanoleaf_token config"),
        service.0,
        service.1,
    ).await.unwrap();

    // Check we can contact the nanoleaf
    nanoleaf.get_panels().await.expect("Could not contact nanoleaf lights");


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
            buffer_manager,
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
				}
			}
		}
	})
    .process(|_stream, stream_data| {
		if let Some(mut buffer) = _stream.dequeue_buffer() {
            let config = stream_data.configuration.as_ref().unwrap();
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
    let panels: nanoleaf::NanoleafLayoutResponse = nanoleaf.get_panels().await.unwrap();
    tokio::spawn(async move { update_lights(panels, nanoleaf, buffer_manager_lights).await });

    mainloop.run();
    stream.disconnect().unwrap();
    Ok(())
}

