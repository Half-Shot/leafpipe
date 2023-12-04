use colors_transform::{Hsl, Rgb, Color};
use image::ColorType;
use crate::backend::FrameCopy;


/**
 * Minimum lightness for a pixel.
 */
const LIGHTNESS_MIN: f32 = 15.0;

/**
 * Maximum lightness for a pixel.
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


pub fn determine_prominent_color(frame_copy: FrameCopy, heatmap: &mut [Vec<Vec<Vec<u32>>>]) -> Vec<Hsl> {
    if ColorType::Rgba8 != frame_copy.frame_color_type {
        panic!("Cannot handle frame!")
    };
    let split_by = heatmap.len();
    let mut most_prominent = vec![Hsl::from(0.0, 0.0, 0.0); split_by];
    let mut most_prominent_idx: Vec<u32> = vec![0; split_by];
    let split_width: u32 = frame_copy.width / split_by as u32;
    let chunk_size = 4 + (SKIP_PIXEL*4);
    
    for (chunk_idx, chunk) in frame_copy.data.chunks_exact(chunk_size).enumerate() {
        let x = ((chunk_idx * chunk_size) / 4) % frame_copy.width as usize;
        let panel_idx = (x as f32 / split_width as f32).floor().min(split_by as f32 - 1.0f32) as usize;


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
        let new_prominence = heatmap[panel_idx][h_index][s_index][l_index] + 1;
        // With what's left, primary focus on getting the most prominent colour in the frame.
        heatmap[panel_idx][h_index][s_index][l_index] = new_prominence;
        if new_prominence > most_prominent_idx[panel_idx] {
            most_prominent[panel_idx] = Hsl::from(
                (h_index * 10) as f32,
                (s_index * 5) as f32,
                (l_index * 5) as f32,
            );
            most_prominent_idx[panel_idx] = new_prominence;
        }
    }
    most_prominent
}


#[cfg(test)]
mod test {
    use colors_transform::Color;
    use image::ColorType;
    use test::Bencher;

    use crate::{visual::prominent_color::determine_prominent_color, backend::FrameCopy};
    
    #[test]
    fn test_determine_prominent_color() {
        let image = image::open("samples/gradientrb.png").unwrap();
        let mut heatmap: Vec<Vec<Vec<Vec<u32>>>> = vec![vec![vec![vec![0u32; 21]; 21]; 37]; 1];
    
        let result = determine_prominent_color( FrameCopy {
            width: image.width(),
            height: image.height(),
            frame_color_type: ColorType::Rgba8,
            data: image.clone().into_bytes(),
        }, &mut heatmap);
        let v = result.get(0).unwrap();
    
        assert_eq!(v.get_hue(), 240.0, "Hue value is incorrect");
        assert_eq!(v.get_saturation(), 85.0, "Saturation value is incorrect");
        assert_eq!(v.get_lightness(), 40.0, "Lightness value is incorrect");
    }

    #[test]
    fn test_determine_prominent_color_multiple_panels() {
        let image = image::open("samples/colortray.png").unwrap();
        let mut heatmap: Vec<Vec<Vec<Vec<u32>>>> = vec![vec![vec![vec![0u32; 21]; 21]; 37]; 4];
    
        let result = determine_prominent_color( FrameCopy {
            width: image.width(),
            height: image.height(),
            frame_color_type: ColorType::Rgba8,
            data: image.clone().into_bytes(),
        }, &mut heatmap);
        let v1 = result.get(0).unwrap();
        let v2 = result.get(1).unwrap();
        let v3 = result.get(2).unwrap();
        let v4 = result.get(2).unwrap();

        println!("v1: {:?}, v2: {:?}, v3: {:?}, v4: {:?}", v1.to_css_string(), v2.to_css_string(), v3.to_css_string(), v4.to_css_string());
    
        assert_eq!(v1.get_hue(), 210.0, "Hue value is incorrect");
        assert_eq!(v1.get_saturation(), 75.0, "Saturation value is incorrect");
        assert_eq!(v1.get_lightness(), 35.0, "Lightness value is incorrect");

        assert_eq!(v2.get_hue(), 240.0, "Hue value is incorrect");
        assert_eq!(v2.get_saturation(), 85.0, "Saturation value is incorrect");
        assert_eq!(v2.get_lightness(), 40.0, "Lightness value is incorrect");

        assert_eq!(v3.get_hue(), 240.0, "Hue value is incorrect");
        assert_eq!(v3.get_saturation(), 85.0, "Saturation value is incorrect");
        assert_eq!(v3.get_lightness(), 40.0, "Lightness value is incorrect");

        assert_eq!(v3.get_hue(), 240.0, "Hue value is incorrect");
        assert_eq!(v3.get_saturation(), 85.0, "Saturation value is incorrect");
        assert_eq!(v3.get_lightness(), 40.0, "Lightness value is incorrect");
    }


    #[bench]
    fn bench_determine_prominent_color_gradient(b: &mut Bencher) {
        let image = image::open("samples/gradientrb.png").unwrap();
        let mut heatmap: Vec<Vec<Vec<Vec<u32>>>> = vec![vec![vec![vec![0u32; 21]; 21]; 37]; 1];

        b.iter(|| determine_prominent_color( FrameCopy {
            width: image.width(),
            height: image.height(),
            frame_color_type: ColorType::Rgba8,
            data: image.clone().into_bytes(),
        },&mut heatmap));
    }

    #[bench]
    fn bench_determine_prominent_color_testcard(b: &mut Bencher) {
        let image = image::open("samples/testcard.png").unwrap();
        let mut heatmap: Vec<Vec<Vec<Vec<u32>>>> = vec![vec![vec![vec![0u32; 21]; 21]; 37]; 1];
        b.iter(|| determine_prominent_color( FrameCopy {
            width: image.width(),
            height: image.height(),
            frame_color_type: ColorType::Rgba8,
            data: image.clone().into_bytes(),
        },&mut heatmap));
    }
}
