// Stolen from https://github.com/BlankParenthesis/visualiser/blob/main/src/audio/spa_audio_info_raw.rs

use std::io::{Write, Seek, Cursor};

use libspa_sys::{spa_rectangle as Rectangle, spa_fraction as Fraction};
use pipewire::spa::{pod::{serialize::*, PropertyFlags, Value, ChoiceValue, CanonicalFixedSizedPod}, utils::{Choice, ChoiceEnum, Id, Fd}};

type ChannelPosition = libspa_sys::spa_audio_channel;

// TODO: enums for format and ChannelPosition

pub(crate) struct SpaAudioInfoRaw {
	pub format: libspa_sys::spa_audio_format,
	pub flags: u32,
	pub rate: u32,
	pub channels: Vec<Option<ChannelPosition>>,
}

impl SpaAudioInfoRaw {
	pub fn empty() -> Self {
		Self {
			format: libspa_sys::SPA_AUDIO_FORMAT_UNKNOWN,
			flags: 0,
			rate: 0,
			channels: vec![]
		}
	}
}

impl SpaAudioInfoRaw {
	pub fn as_pod(&self) -> Result<Box<[u8]>, GenError> {
		let mut pod = Vec::<u8>::new();
		let cursor = Cursor::new(&mut pod);
	
		PodSerializer::serialize(cursor, self)?;

		Ok(pod.into_boxed_slice())
	}
}

impl PodSerialize for SpaAudioInfoRaw {
	fn serialize<O: Write + Seek>(
		&self,
		serializer: PodSerializer<O>,
	) -> Result<SerializeSuccess<O>, GenError> {
		let mut object_serializer = serializer.serialize_object(
			libspa_sys::SPA_TYPE_OBJECT_Format,
			libspa_sys::SPA_PARAM_EnumFormat,
		)?;
		object_serializer.serialize_property(
			libspa_sys::SPA_FORMAT_mediaType,
			&Id(libspa_sys::SPA_MEDIA_TYPE_audio),
			PropertyFlags::READONLY,
		)?;
		object_serializer.serialize_property(
			libspa_sys::SPA_FORMAT_mediaSubtype,
			&Id(libspa_sys::SPA_MEDIA_SUBTYPE_raw),
			PropertyFlags::READONLY,
		)?;
		if self.format != libspa_sys::SPA_AUDIO_FORMAT_UNKNOWN {
			object_serializer.serialize_property(
				libspa_sys::SPA_FORMAT_AUDIO_format,
				&Id(self.format),
				PropertyFlags::READONLY,
			)?;
		}
		if self.rate != 0 {
			object_serializer.serialize_property(
				libspa_sys::SPA_FORMAT_AUDIO_rate,
				&Id(self.rate),
				PropertyFlags::READONLY,
			)?;
		}
		if !self.channels.is_empty() {
			object_serializer.serialize_property(
				libspa_sys::SPA_FORMAT_AUDIO_channels,
				&Id(self.channels.len() as u32),
				PropertyFlags::READONLY,
			)?;

			if self.flags & libspa_sys::SPA_AUDIO_FLAG_UNPOSITIONED == 0 {
				let channels = self.channels.iter()
					.map(|c| match c {
						Some(id) => Id(*id),
						None => Id(0),
					})
					.collect::<Vec<_>>();

				object_serializer.serialize_property(
					libspa_sys::SPA_FORMAT_AUDIO_position,
					channels.as_slice(),
					PropertyFlags::READONLY,
				)?;
			}
		}
		object_serializer.end()
	}
}


pub trait ChoiceDefault<T> {
	fn choice_default(&self) -> Result<T, ()>;
}

impl<T: CanonicalFixedSizedPod + Copy> ChoiceDefault<T> for Choice<T> {
	fn choice_default(&self) -> Result<T, ()> {
		Ok(match &self.1 {
			ChoiceEnum::None(value) => *value,
			ChoiceEnum::Range { default, .. } => *default,
			ChoiceEnum::Step { default, .. } => *default,
			ChoiceEnum::Enum { default, .. } => *default,
			ChoiceEnum::Flags { default, .. } => *default,
		})
	}
}

impl ChoiceDefault<i32> for ChoiceValue {
	fn choice_default(&self) -> Result<i32, ()> {
		if let ChoiceValue::Int(choice) = self {
			choice.choice_default()
		} else {
			Err(())
		}
	}
}

impl ChoiceDefault<i64> for ChoiceValue {
	fn choice_default(&self) -> Result<i64, ()> {
		if let ChoiceValue::Long(choice) = self {
			choice.choice_default()
		} else {
			Err(())
		}
	}
}

impl ChoiceDefault<f32> for ChoiceValue {
	fn choice_default(&self) -> Result<f32, ()> {
		if let ChoiceValue::Float(choice) = self {
			choice.choice_default()
		} else {
			Err(())
		}
	}
}

impl ChoiceDefault<f64> for ChoiceValue {
	fn choice_default(&self) -> Result<f64, ()> {
		if let ChoiceValue::Double(choice) = self {
			choice.choice_default()
		} else {
			Err(())
		}
	}
}

impl ChoiceDefault<Id> for ChoiceValue {
	fn choice_default(&self) -> Result<Id, ()> {
		if let ChoiceValue::Id(choice) = self {
			choice.choice_default()
		} else {
			Err(())
		}
	}
}

impl ChoiceDefault<Rectangle> for ChoiceValue {
	fn choice_default(&self) -> Result<Rectangle, ()> {
		if let ChoiceValue::Rectangle(choice) = self {
			choice.choice_default()
		} else {
			Err(())
		}
	}
}

impl ChoiceDefault<Fraction> for ChoiceValue {
	fn choice_default(&self) -> Result<Fraction, ()> {
		if let ChoiceValue::Fraction(choice) = self {
			choice.choice_default()
		} else {
			Err(())
		}
	}
}
impl ChoiceDefault<Fd> for ChoiceValue {
	fn choice_default(&self) -> Result<Fd, ()> {
		if let ChoiceValue::Fd(choice) = self {
			choice.choice_default()
		} else {
			Err(())
		}
	}
}

pub trait Fixate<T> {
	fn fixate(&self) -> Result<T, ()>;
}

impl Fixate<i32> for Value {
	fn fixate(&self) -> Result<i32, ()> {
		match self {
			Value::Int(int) => Ok(*int),
			Value::Choice(choice) => choice.choice_default(),
			_ => Err(()),
		}
	}
}

impl Fixate<Id> for Value {
	fn fixate(&self) -> Result<Id, ()> {
		match self {
			Value::Id(id) => Ok(*id),
			Value::Choice(choice) => choice.choice_default(),
			_ => Err(()),
		}
	}
}

impl Fixate<f32> for Value {
	fn fixate(&self) -> Result<f32, ()> {
		match self {
			Value::Float(id) => Ok(*id),
			Value::Choice(choice) => choice.choice_default(),
			_ => Err(()),
		}
	}
}