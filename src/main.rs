#![feature(test)]
extern crate test;

use colors_transform::{Color, Hsl};
use nanoleaf::{NanoleafClient, NanoleafEffectPayload, NanoleafLayoutResponse};
use visual::backend;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::{Connection, QueueHandle};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry;
use core::panic;
use std::ops::Sub;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, RwLock};
use std::{thread, time};
use std::time::{Duration, Instant};
use vis::BufferManager;
use config::{Config, ConfigError};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use crate::slidingwindow::SlidingWindow;

mod audio;
mod slidingwindow;
mod vis;
mod nanoleaf;
mod visual;
mod pipewire;

const LIGHT_INTERVAL: Duration = Duration::from_millis(100);
const MDNS_TIMEOUT: Duration = Duration::from_secs(30);

async fn update_lights(panels: NanoleafLayoutResponse, nanoleaf: NanoleafClient, buffer_manager: Arc<RwLock<BufferManager>>, color_channel: Receiver<Hsl>) {
    // Needs to be over a sliding window.
    let mut window = SlidingWindow::new(64);
    let mut color = Hsl::new();
    loop { 
        let process_start = Instant::now();
        {
            color = color_channel.recv_timeout(Duration::from_millis(30)).unwrap_or( color);

            if let Some(data) = buffer_manager.write().unwrap().fft_interval::<10>(LIGHT_INTERVAL) {
                let mut effect = NanoleafEffectPayload::new(panels.num_panels);
                for (panel_index, panel) in panels.position_data.iter().enumerate() {
                    let (min, max) = window.submit_new(data[panel_index]);
                    let base_int = color.get_lightness() - 10.0;
                    let intensity = (base_int + ((data[panel_index] + min) / max) * 25f32 * (panel_index as f32 + 1.0f32).powf(1.05f32)).clamp(5.0, 80.0);
                    let hsl = Hsl::from(color.get_hue(), color.get_saturation(), intensity);
                    let rgb = hsl.to_rgb().as_tuple();
                    let r = rgb.0.round() as u8;
                    let g = rgb.1.round() as u8;
                    let b = rgb.2.round() as u8;
                    effect.write_effect(panel.panel_id, r, g, b, 1);
                }
                if let Err(err) = nanoleaf.send_effect(&effect) {
                    println!("Failed to send effect to nanoleaf {:?}", err);
                }
            }
        }
        if LIGHT_INTERVAL.ge(&process_start.elapsed()) {
            let sleep_duration = LIGHT_INTERVAL.sub(process_start.elapsed());
            if sleep_duration.ge(&Duration::ZERO) {
                thread::sleep(LIGHT_INTERVAL);
            }
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
                        // TODO: Support IPv6. My system doesn't :(
                        let service_ip = info.get_addresses().iter().find(|addr| addr.is_ipv4()).expect("Service found but with no addresses").to_string();
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

struct AppState;

impl wayland_client::Dispatch<wl_registry::WlRegistry, GlobalListContents> for AppState {
    fn event(
        _: &mut AppState,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<AppState>,
    ) {
    }
}


fn configure_display(pause_duration:time::Duration, output_name: Option<String>) -> std::sync::mpsc::Receiver<Hsl> {
    let conn = Connection::connect_to_env().unwrap();
    let (globals, _) = registry_queue_init::<AppState>(&conn).unwrap();
    let out: WlOutput = if let Some(output_name_result) = output_name {
        visual::output::get_wloutput(
            output_name_result.trim().to_string(),
            visual::output::get_all_outputs(&globals, &conn),
        )
    } else {
        visual::output::get_all_outputs(&globals, &conn)
            .first()
            .unwrap()
            .wl_output
            .clone()
    };

    let mut capturer = backend::setup_capture(&globals,&conn, &out).unwrap();
    let (tx, rx) = channel();

    thread::spawn(move|| {
        log::info!("Capturing frames");
        let mut last_value = Hsl::from(0.0,0.0,0.0);
        let mut heatmap = vec![vec![vec![0u32; 21]; 21]; 37];
        loop {
            let frame_copy = backend::capture_output_frame(
                &globals,
                &conn,
                &out,
                &mut capturer,
            ).unwrap();
            let hsl = visual::prominent_color::determine_prominent_color(frame_copy, &mut heatmap);
            if !last_value.eq(&hsl) {
                log::info!("Sending new hsl {:?}", hsl);
                tx.send(hsl).unwrap();
                last_value = hsl;
            }
            thread::sleep(pause_duration);
        }
    });
    rx
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let config_builder = Config::builder().add_source(config::Environment::with_prefix("LP"));

    let config = if let Some(config_file) = xdg::BaseDirectories::with_prefix("leafpipe").unwrap().find_config_file("config.toml") {
        config_builder.add_source(config::File::from(config_file)).build().unwrap()
    } else {
        config_builder.add_source(config::File::with_name("config.toml")).build().unwrap()
    };

    env_logger::init();
    log::set_max_level(log::LevelFilter::Trace);
    log::trace!("Logger initialized.");

    let buffer_manager: Arc<RwLock<BufferManager>> = Arc::new(RwLock::new(BufferManager::default()));
    let buffer_manager_lights = buffer_manager.clone();

    let pipewire = crate::pipewire::PipewireContainer::new(buffer_manager).expect("Could not configure pipewire");

    let service = discover_host(&config);
    println!("Discovered nanoleaf on {}:{}", service.0, service.1);

    let nanoleaf: NanoleafClient = NanoleafClient::connect(
        config.get_string("nanoleaf_token").expect("Missing nanoleaf_token config"),
        service.0,
        service.1,
    ).await.unwrap();

    // Check we can contact the nanoleaf
    nanoleaf.get_panels().await.expect("Could not contact nanoleaf lights");

    let panels: nanoleaf::NanoleafLayoutResponse = nanoleaf.get_panels().await.unwrap();
    let color_rx = configure_display(Duration::from_millis(33), Some(String::from("DP-1")));

    tokio::spawn(async move { update_lights(panels, nanoleaf, buffer_manager_lights, color_rx).await });
    pipewire.run();
    pipewire.stop().expect("Failed to stop pipewire");
    Ok(())
}

