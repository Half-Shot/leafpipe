use std::{
    error::Error,
    ffi::CStr,
    fs::File,
    os::unix::prelude::FromRawFd,
    os::unix::prelude::RawFd,
    process::exit,
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH}, io::{Read, Seek},
};

use nix::{
    fcntl,
    sys::{memfd, mman, stat},
    unistd,
};

use image::ColorType;

use wayland_client::{
    delegate_noop,
    globals::GlobalList,
    protocol::{
        wl_buffer::WlBuffer, wl_output::WlOutput, wl_shm, wl_shm::Format, wl_shm::WlShm,
        wl_shm_pool::WlShmPool,
    },
    Connection, Dispatch, QueueHandle,
    WEnum::Value,
};

use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1, zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1,
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

struct CaptureFrameState {
    formats: Vec<FrameFormat>,
    state: Option<FrameState>,
    buffer_done: AtomicBool,
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for CaptureFrameState {
    fn event(
        frame: &mut Self,
        _: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                log::debug!("Received Buffer event");
                if let Value(f) = format {
                    frame.formats.push(FrameFormat {
                        format: f,
                        width,
                        height,
                        stride,
                    })
                } else {
                    log::debug!("Received Buffer event with unidentified format");
                    exit(1);
                }
            }
            zwlr_screencopy_frame_v1::Event::Flags { .. } => {
                log::debug!("Received Flags event");
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                // If the frame is successfully copied, a “flags” and a “ready” events are sent. Otherwise, a “failed” event is sent.
                // This is useful when we call .copy on the frame object.
                log::debug!("Received Ready event");
                frame.state.replace(FrameState::Finished);
            }
            zwlr_screencopy_frame_v1::Event::Failed => {
                log::debug!("Received Failed event");
                frame.state.replace(FrameState::Failed);
            }
            zwlr_screencopy_frame_v1::Event::Damage { .. } => {
                log::debug!("Received Damage event");
            }
            zwlr_screencopy_frame_v1::Event::LinuxDmabuf { .. } => {
                log::debug!("Received LinuxDmaBuf event");
            }
            zwlr_screencopy_frame_v1::Event::BufferDone => {
                log::debug!("Received bufferdone event");
                frame.buffer_done.store(true, Ordering::SeqCst);
            }
            _ => unreachable!(),
        };
    }
}

delegate_noop!(CaptureFrameState: ignore WlShm);
delegate_noop!(CaptureFrameState: ignore WlShmPool);
delegate_noop!(CaptureFrameState: ignore WlBuffer);
delegate_noop!(CaptureFrameState: ignore ZwlrScreencopyManagerV1);



/// Type of frame supported by the compositor. For now we only support Argb8888, Xrgb8888, and
/// Xbgr8888.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct FrameFormat {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

/// State of the frame after attemting to copy it's data to a wl_buffer.
#[derive(Debug, Copy, Clone, PartialEq)]
enum FrameState {
    /// Compositor returned a failed event on calling `frame.copy`.
    Failed,
    /// Compositor sent a Ready event on calling `frame.copy`.
    Finished,
}

/// The copied frame comprising of the FrameFormat, ColorType (Rgba8), and a memory backed shm
/// file that holds the image data in it.
#[derive(Debug)]
pub struct FrameCopy {
    pub frame_color_type: ColorType,
    pub data: Vec<u8>,
}

pub struct FrameCapturer {
    pub buffer: wayland_client::protocol::wl_buffer::WlBuffer,
    pub frame_format: FrameFormat,
    pub mem_file: File,
}


pub fn setup_capture(
    globals: &mut GlobalList,
    conn: &mut Connection,
    output: &WlOutput,
) -> Result<FrameCapturer, Box<dyn Error>> {
    let mut state = CaptureFrameState {
        formats: Vec::new(),
        state: None,
        buffer_done: AtomicBool::new(false),
    };
    let mut event_queue = conn.new_event_queue::<CaptureFrameState>();
    let qh = event_queue.handle();

    // Instantiating screencopy manager.
    let screencopy_manager = match globals.bind::<ZwlrScreencopyManagerV1, _, _>(&qh, 3..=3, ()) {
        Ok(x) => x,
        Err(e) => {
            log::error!("Failed to create screencopy manager. Does your compositor implement ZwlrScreencopy?");
            panic!("{:#?}", e);
        }
    };

    // Capture output.
    screencopy_manager.capture_output(0, output, &qh, ());

    while !state.buffer_done.load(Ordering::SeqCst) {
        event_queue.blocking_dispatch(&mut state)?;
    }

    // TODO, better error handling
    let frame_format = *state.formats.get(0).unwrap();

    log::debug!(
        "Received compositor frame buffer format: {:#?}",
        frame_format
    );

    // Bytes of data in the frame = stride * height.
    let frame_bytes = frame_format.stride * frame_format.height;

    // Create an in memory file and return it's file descriptor.
    let mem_fd = create_shm_fd()?;
    let mem_file = unsafe { File::from_raw_fd(mem_fd) };
    mem_file.set_len(frame_bytes as u64)?;

    // Instantiate shm global.
    let shm = globals.bind::<WlShm, _, _>(&qh, 1..=1, ()).unwrap();
    let shm_pool = shm.create_pool(mem_fd, frame_bytes as i32, &qh, ());
    let buffer = shm_pool.create_buffer(
        0,
        frame_format.width as i32,
        frame_format.height as i32,
        frame_format.stride as i32,
        frame_format.format,
        &qh,
        (),
    );

    Ok(FrameCapturer{
        buffer,
        frame_format,
        mem_file,
    })
}

/// Get a FrameCopy instance with screenshot pixel data for any wl_output object.
pub fn capture_output_frame(
    globals: &mut GlobalList,
    conn: &mut Connection,
    output: &WlOutput,
    capturer: &mut FrameCapturer,
) -> Result<FrameCopy, Box<dyn Error>> {
    let mut state = CaptureFrameState {
        formats: Vec::new(),
        state: None,
        buffer_done: AtomicBool::new(false),
    };
    let mut event_queue = conn.new_event_queue::<CaptureFrameState>();
    let qh = event_queue.handle();

    // Instantiating screencopy manager.
    let screencopy_manager = match globals.bind::<ZwlrScreencopyManagerV1, _, _>(&qh, 3..=3, ()) {
        Ok(x) => x,
        Err(e) => {
            log::error!("Failed to create screencopy manager. Does your compositor implement ZwlrScreencopy?");
            panic!("{:#?}", e);
        }
    };

    // Capture output.
    let frame: ZwlrScreencopyFrameV1 = screencopy_manager.capture_output(0, output, &qh, ());

    log::debug!("Waiting for buffer");
    while !state.buffer_done.load(Ordering::SeqCst) {
        event_queue.blocking_dispatch(&mut state)?;
    }
    log::debug!("Buffer done");

    // Copy the pixel data advertised by the compositor into the buffer we just created.
    frame.copy(&capturer.buffer);

    // On copy the Ready / Failed events are fired by the frame object, so here we check for them.
    loop {
        // Basically reads, if frame state is not None then...
        if let Some(state) = state.state {
            match state {
                FrameState::Failed => {
                    log::error!("Frame copy failed");
                    exit(1);
                }
                FrameState::Finished => {
                    // Create a writeable memory map backed by a mem_file.
                    let mut data = vec![];
                    capturer.mem_file.read_to_end(&mut data).unwrap(); // unsafe { MmapMut::map_mut(&capturer.mem_file)? };
                    capturer.mem_file.rewind().unwrap();
                    let frame_color_type = match capturer.frame_format.format {
                        wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => {
                            // Swap out b with r as these formats are in little endian notation.
                            for chunk in data.chunks_exact_mut(4) {
                                chunk.swap(0, 2);
                            }
                            ColorType::Rgba8
                        }
                        wl_shm::Format::Xbgr8888 => ColorType::Rgba8,
                        unsupported_format => {
                            log::error!("Unsupported buffer format: {:?}", unsupported_format);
                            exit(1);
                        }
                    };
                    return Ok(FrameCopy {
                        frame_color_type,
                        data,
                    });
                }
            }
        }
        event_queue.blocking_dispatch(&mut state)?;
    }
}

/// Return a RawFd to a shm file. We use memfd create on linux and shm_open for BSD support.
/// You don't need to mess around with this function, it is only used by
/// capture_output_frame.
fn create_shm_fd() -> std::io::Result<RawFd> {
    // Only try memfd on linux and freebsd.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    loop {
        // Create a file that closes on succesful execution and seal it's operations.
        match memfd::memfd_create(
            CStr::from_bytes_with_nul(b"pxlha\0").unwrap(),
            memfd::MemFdCreateFlag::MFD_CLOEXEC | memfd::MemFdCreateFlag::MFD_ALLOW_SEALING,
        ) {
            Ok(fd) => {
                // This is only an optimization, so ignore errors.
                // F_SEAL_SRHINK = File cannot be reduced in size.
                // F_SEAL_SEAL = Prevent further calls to fcntl().
                let _ = fcntl::fcntl(
                    fd,
                    fcntl::F_ADD_SEALS(
                        fcntl::SealFlag::F_SEAL_SHRINK | fcntl::SealFlag::F_SEAL_SEAL,
                    ),
                );
                return Ok(fd);
            }
            Err(nix::errno::Errno::EINTR) => continue,
            Err(nix::errno::Errno::ENOSYS) => break,
            Err(errno) => return Err(std::io::Error::from(errno)),
        }
    }

    // Fallback to using shm_open.
    let sys_time = SystemTime::now();
    let mut mem_file_handle = format!(
        "/pxlha-{}",
        sys_time.duration_since(UNIX_EPOCH).unwrap().subsec_nanos()
    );
    loop {
        match mman::shm_open(
            // O_CREAT = Create file if does not exist.
            // O_EXCL = Error if create and file exists.
            // O_RDWR = Open for reading and writing.
            // O_CLOEXEC = Close on succesful execution.
            // S_IRUSR = Set user read permission bit .
            // S_IWUSR = Set user write permission bit.
            mem_file_handle.as_str(),
            fcntl::OFlag::O_CREAT
                | fcntl::OFlag::O_EXCL
                | fcntl::OFlag::O_RDWR
                | fcntl::OFlag::O_CLOEXEC,
            stat::Mode::S_IRUSR | stat::Mode::S_IWUSR,
        ) {
            Ok(fd) => match mman::shm_unlink(mem_file_handle.as_str()) {
                Ok(_) => return Ok(fd),
                Err(errno) => match unistd::close(fd) {
                    Ok(_) => return Err(std::io::Error::from(errno)),
                    Err(errno) => return Err(std::io::Error::from(errno)),
                },
            },
            Err(nix::errno::Errno::EEXIST) => {
                // If a file with that handle exists then change the handle
                mem_file_handle = format!(
                    "/pxlha-{}",
                    sys_time.duration_since(UNIX_EPOCH).unwrap().subsec_nanos()
                );
                continue;
            }
            Err(nix::errno::Errno::EINTR) => continue,
            Err(errno) => return Err(std::io::Error::from(errno)),
        }
    }
}
