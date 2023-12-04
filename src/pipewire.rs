use std::sync::{RwLock, Arc};

use pipewire::spa::format::{MediaType, MediaSubtype};
use pipewire::spa::param::audio::AudioInfoRaw;
use pipewire::spa::pod::Pod;
use pipewire::{MainLoop, Context, Core, spa};
use pipewire::properties;
use pipewire::spa::Direction;
use pipewire::stream::{StreamFlags, StreamListener};
use pipewire::stream::Stream;

use crate::vis::BufferManager;

pub struct PipewireContainer {
    mainloop: MainLoop,
    _context: Context<MainLoop>,
    _core: Core,
    _listener: StreamListener<StreamData>,
    stream: Stream,
}

#[derive(Default)]
struct StreamData {
	configuration: AudioInfoRaw,
    buffer_manager: Arc<RwLock<BufferManager>>,
}

impl PipewireContainer {
    pub fn new(buffer_manager: Arc<RwLock<BufferManager>>) -> Result<Self, pipewire::Error> {
        pipewire::init();
        let mainloop = MainLoop::new()?;
        let context: Context<MainLoop> = Context::new(&mainloop)?;
        let core = context.connect(None)?;

        let props = properties! {
            *pipewire::keys::MEDIA_TYPE => "Audio",
            *pipewire::keys::MEDIA_CATEGORY => "Capture",
            *pipewire::keys::MEDIA_ROLE => "Music",
        };
    
        let stream = Stream::new(
            &core,
            "audio-capture",
            props,
        )?;
    
        let user_data = StreamData {
            configuration: Default::default(),
            buffer_manager,
        };
    
        let listener = stream.add_local_listener_with_user_data(
            user_data
        )
        .param_changed(|_, id, data, param| {
            let Some(param) = param else {
                return;
            };
            if id != pipewire::spa::param::ParamType::Format.as_raw() {
                return;
            }
    
            let (media_type, media_subtype) =
            match pipewire::spa::param::format_utils::parse_format(param) {
                Ok(v) => v,
                Err(_) => return,
            };
            if media_type != MediaType::Audio 
            || media_subtype != MediaSubtype::Raw
            {
                return;
            }
            data.configuration.parse(param).expect("Expected to be able to parse audio!");
        })
        .process(|_stream, stream_data| {
            if let Some(mut buffer) = _stream.dequeue_buffer() {
                let channels = stream_data.configuration.channels() as usize;
                for channel_index in 0..channels-1 {
                    let channel = buffer.datas_mut().get_mut(channel_index).unwrap();
                    let chunk = channel.chunk(); 
                    let size = chunk.size() as usize;
                    let data = channel.data(); 
                    if let Some(data) = data {
                        let cast_buffer: &[f32] = unsafe {
                            std::slice::from_raw_parts(data.as_ptr().cast(), size / std::mem::size_of::<f32>())
                        };
                        stream_data.buffer_manager.write().unwrap().fill_buffer(cast_buffer, stream_data.configuration.rate());
                    }
                }
            }
        }).register()?;
    
        let mut audio_info = spa::param::audio::AudioInfoRaw::new();
        audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
        let obj = spa::pod::Object {
            type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id: spa::param::ParamType::EnumFormat.as_raw(),
            properties: audio_info.into(),
        };
        let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &spa::pod::Value::Object(obj),
        )
        .unwrap()
        .0
        .into_inner();

        let mut params = [Pod::from_bytes(&values).unwrap()];
        stream.connect(
            Direction::Input,
            None,
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
            &mut params,
        )?;

        Ok(PipewireContainer { 
            mainloop,
            _context: context,
            _core: core,
            _listener: listener,
            stream,
        })
    }

    pub fn run(&self) {
        // TODO: Port to async
        self.mainloop.run()
    }

    pub fn stop(&self) -> Result<(), pipewire::Error> {
        self.stream.disconnect()
    }
}