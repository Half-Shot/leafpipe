use colors_transform::{Hsl, Rgb, Color};
use image::ColorType;
use crate::backend::FrameCopy;


/**
 * Minimum lightness for a pixel.
 */
const LIGHTNESS_MIN: f32 = 5.0;

/**
 * Minimum lightness for a pixel.
 */
const LIGHTNESS_MAX: f32 = 95.0;

/**
 * Minimum saturation for a pixel.
 */
const SATURATION_MIN: f32 = 10.0;

/**
 * How many pixels to skip in a chunk, for performance.
 */
const SKIP_PIXEL: usize = 8;


pub fn determine_prominent_color(frame_copy: FrameCopy, heatmap: &mut Vec<Vec<Vec<u32>>>) -> Hsl {
    if ColorType::Rgba8 != frame_copy.frame_color_type {
        panic!("Cannot handle frame!")
    };
    let mut most_prominent= Hsl::from(0.0, 0.0, 0.0);
    let mut most_prominent_idx = 0;
    for chunk in frame_copy.data.chunks_exact(4 + (SKIP_PIXEL*4)) {

        let hsl = Rgb::from(chunk[0] as f32, chunk[1] as f32, chunk[2] as f32).to_hsl();

        // Reject any really dark colours.
        if LIGHTNESS_MAX < hsl.get_lightness() || hsl.get_lightness() < LIGHTNESS_MIN {
            continue;
        }
        if hsl.get_saturation() < SATURATION_MIN {
            continue;
        }
        // Split into 36 blocks
        let h_index = (hsl.get_hue() as usize) / 10;
        let s_index = (hsl.get_saturation() as usize) / 5;
        let l_index = (hsl.get_lightness() as usize) / 5;
        let new_prominence = heatmap[h_index][s_index][l_index] + 1;
        // With what's left, primary focus on getting the most prominent colour in the frame.
        heatmap[h_index][s_index][l_index] = new_prominence;
        if new_prominence > most_prominent_idx {
            most_prominent = Hsl::from(
                (h_index * 10) as f32,
                (s_index * 5) as f32,
                (l_index * 5) as f32,
            );
            most_prominent_idx = new_prominence;
        }
    }
    most_prominent
}


#[cfg(test)]
mod test {
    use colors_transform::Color;
    use image::ColorType;
    use test::Bencher;
    use std::fs::File;

    use crate::{visual::prominent_color::determine_prominent_color, backend::FrameCopy};
    
    #[test]
    fn test_determine_prominent_color() {
        let decoder = png::Decoder::new(File::open("samples/gradientrb.png").unwrap());
        let mut reader = decoder.read_info().unwrap();// Allocate the output buffer.
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        let bytes = &buf[..info.buffer_size()];
        let mut heatmap = vec![vec![vec![0u32; 21]; 21]; 37];
    
        let v = determine_prominent_color( FrameCopy {
            frame_color_type: ColorType::Rgba8,
            data: bytes.to_vec(),
        }, &mut heatmap);
    
        assert_eq!(v.get_hue(), 240.0, "Hue value is incorrect");
        assert_eq!(v.get_saturation(), 85.0, "Saturation value is incorrect");
        assert_eq!(v.get_lightness(), 40.0, "Lightness value is incorrect");
    }

    #[bench]
    fn bench_determine_prominent_color_gradient(b: &mut Bencher) {
        let decoder = png::Decoder::new(File::open("samples/gradientrb.png").unwrap());
        let mut reader = decoder.read_info().unwrap();// Allocate the output buffer.
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        let bytes = &buf[..info.buffer_size()];
        let mut heatmap = vec![vec![vec![0u32; 21]; 21]; 37];

        b.iter(|| determine_prominent_color( FrameCopy {
            frame_color_type: ColorType::Rgba8,
            data: bytes.to_vec(),
        },&mut heatmap));
    }

    #[bench]
    fn bench_determine_prominent_color_testcard(b: &mut Bencher) {
        let decoder = png::Decoder::new(File::open("samples/testcard.png").unwrap());
        let mut reader = decoder.read_info().unwrap();// Allocate the output buffer.
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        let bytes = &buf[..info.buffer_size()];
        let mut heatmap = vec![vec![vec![0u32; 21]; 21]; 37];

        b.iter(|| determine_prominent_color( FrameCopy {
            frame_color_type: ColorType::Rgba8,
            data: bytes.to_vec(),
        },&mut heatmap));
    }
}
