use std::net::UdpSocket;
use serde::{Serialize,Deserialize};

pub struct NanoleafClient {
    socket: UdpSocket,
    base_url: String,
}

#[derive(Debug)]
pub struct NanoleafError {
    msg: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct NanoleafEffectsResponse {
    effects_list: Vec<String>,
    select: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NanoleafLayoutPanelData {
    pub panel_id: u16,
    pub x: usize,
    pub y: usize,
    pub shape_type: u8,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NanoleafLayoutResponse {
    pub num_panels: usize,
    pub side_length: usize,
    pub position_data: Vec<NanoleafLayoutPanelData>,
}

const EFFECT_SIZE_BYTES: usize = 8;

pub struct NanoleafEffectPayload {
    pub buf: Vec<u8>,
    head: usize,
}

impl NanoleafEffectPayload {
    pub fn new(panels_to_update: usize) -> Self {
        let mut buf = vec![0 as u8; 2 + (EFFECT_SIZE_BYTES*panels_to_update)];
        buf[0] = (panels_to_update >> 8).try_into().unwrap();
        buf[1] = (panels_to_update % 256).try_into().unwrap();
        NanoleafEffectPayload {
            head: 0,
            buf,
        }
    }

    /// Write an effect to the payload to be sent.
    /// `transition_time_cs` is in deciseconds.
    pub fn write_effect(self: &mut Self, panel_id: u16, r: u8, g: u8, b: u8, transition_time_ds: u8) {
        self.buf[self.head + 0] = (panel_id >> 8).try_into().unwrap();
        self.buf[self.head + 1] = (panel_id % 256).try_into().unwrap();
        self.buf[self.head + 2] = r;
        self.buf[self.head + 3] = g;
        self.buf[self.head + 4] = b;
        self.buf[self.head + 5] = 0;
        self.buf[self.head + 6] = 0;
        self.buf[self.head + 7] = transition_time_ds;
        self.head += 8;
    }
}

const UDP_PORT: u16 = 60222;

impl NanoleafClient {
    pub async fn connect(access_token: &str, host: &str, http_port: u16) -> Result<Self, NanoleafError> {
        let base_url = format!("http://{host}:{http_port}/api/v1/{access_token}", host=host, access_token=access_token);

        let effects_result = reqwest::get(format!("{base_url}/effects"))
            .await
            .and_then(|res| res.error_for_status()).map_err(|err| NanoleafError {
                msg: format!("Failed to contact nanoleaf API {:?}", err).to_string(),
            })?.json::<NanoleafEffectsResponse>().await.map_err(|err| NanoleafError {
                msg: format!("Failed to parse JSON from /effects API {:?}", err).to_string(),
            })?;

        if effects_result.select != "*ExtControl*" {
            // Make sure we enable ExtControl
            panic!("Not implemented configuring ExtControl");
        }

        // Now bind
        let socketaddr = format!("{host}:{UDP_PORT}", host=host);

        match UdpSocket::bind("0.0.0.0:0").and_then(|socket| socket.connect(socketaddr).map(|_| socket)) {
            Ok(socket) => {
                Ok(NanoleafClient {
                    socket,
                    base_url,
                })
            },
            Err(e) => {
                Err(NanoleafError {
                    msg: format!("Failed to open UDP socket {:?}", e).to_string(),
                })
            }
        }
    }

    pub async fn get_panels(self: &Self) -> Result<NanoleafLayoutResponse, NanoleafError> {
        reqwest::get(format!("{base_url}/panelLayout/layout", base_url=self.base_url))
        .await
        .and_then(|res| res.error_for_status()).map_err(|err| NanoleafError {
            msg: format!("Failed to contact nanoleaf API {:?}", err).to_string(),
        })?.json::<NanoleafLayoutResponse>().await.map_err(|err| NanoleafError {
            msg: format!("Failed to parse JSON from /panelLayout/layout API {:?}", err).to_string(),
        })
    }

    pub fn send_effect(self: &Self, payload: &NanoleafEffectPayload)->Result<(), std::io::Error> {
        self.socket.send(&payload.buf).map(|_| {})
    }
    
}