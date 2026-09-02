#![cfg(feature = "faces")]

use candle_core::{Result, Tensor};
use candle_nn::{conv2d, Conv2d, Conv2dConfig, Module, VarBuilder};
use std::path::Path;

pub const FILE_NAME: &str = "yunet.safetensors";
pub const BYTES: u64 = 223_072;
pub const CREDIT: &str =
    "Face detection by YuNet, MIT licensed, Copyright (c) 2020 Shiqi Yu <shiqi.yu@gmail.com>";

pub const INPUT_SIDE: usize = 640;
pub const STRIDES: [usize; 3] = [8, 16, 32];
pub const KEYPOINTS: usize = 5;
pub const SHRINK_TO: usize = 32;
pub const SCORE_FLOOR: f32 = 0.6;
pub const OVERLAP_CEILING: f32 = 0.3;

#[derive(Debug)]
struct ConvDPUnit {
    conv1: Conv2d,
    conv2: Conv2d,
    relu: bool,
}

impl ConvDPUnit {
    fn load(vb: VarBuilder, in_channels: usize, out_channels: usize, relu: bool) -> Result<Self> {
        let conv1 = conv2d(
            in_channels,
            out_channels,
            1,
            Conv2dConfig {
                padding: 0,
                stride: 1,
                groups: 1,
                dilation: 1,
                ..Default::default()
            },
            vb.pp("conv1"),
        )?;
        let conv2 = conv2d(
            out_channels,
            out_channels,
            3,
            Conv2dConfig {
                padding: 1,
                stride: 1,
                groups: out_channels,
                dilation: 1,
                ..Default::default()
            },
            vb.pp("conv2"),
        )?;
        Ok(Self { conv1, conv2, relu })
    }
}

impl Module for ConvDPUnit {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let xs = self.conv2.forward(&self.conv1.forward(xs)?)?;
        if self.relu {
            xs.relu()
        } else {
            Ok(xs)
        }
    }
}

#[derive(Debug)]
struct Stage {
    first: ConvDPUnit,
    second: ConvDPUnit,
}

impl Stage {
    fn load(vb: VarBuilder, in_channels: usize, mid: usize, out_channels: usize) -> Result<Self> {
        Ok(Self {
            first: ConvDPUnit::load(vb.pp("conv1"), in_channels, mid, true)?,
            second: ConvDPUnit::load(vb.pp("conv2"), mid, out_channels, true)?,
        })
    }
}

impl Module for Stage {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        self.second.forward(&self.first.forward(xs)?)
    }
}

#[derive(Debug)]
struct Head {
    cls: ConvDPUnit,
    obj: ConvDPUnit,
    bbox: ConvDPUnit,
    kps: ConvDPUnit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Face {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub score: f32,
    pub points: [[f32; 2]; KEYPOINTS],
}

impl Face {
    pub fn area(&self) -> f32 {
        self.width.max(0.0) * self.height.max(0.0)
    }

    pub fn overlap(&self, other: &Face) -> f32 {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        let shared = (right - left).max(0.0) * (bottom - top).max(0.0);
        let union = self.area() + other.area() - shared;
        if union <= 0.0 {
            0.0
        } else {
            shared / union
        }
    }

    fn scaled(&self, by: f32) -> Face {
        let mut moved = *self;
        moved.x /= by;
        moved.y /= by;
        moved.width /= by;
        moved.height /= by;
        for point in moved.points.iter_mut() {
            point[0] /= by;
            point[1] /= by;
        }
        moved
    }
}

#[derive(Debug)]
pub struct Level {
    pub cls: Tensor,
    pub obj: Tensor,
    pub bbox: Tensor,
    pub kps: Tensor,
    pub stride: usize,
}

#[derive(Debug)]
pub struct YuNet {
    stem: Conv2d,
    model0: ConvDPUnit,
    model1: Stage,
    model2: Stage,
    model3: Stage,
    model4: Stage,
    model5: Stage,
    lateral: [ConvDPUnit; 3],
    heads: [Head; 3],
}

fn pool(xs: &Tensor) -> Result<Tensor> {
    xs.max_pool2d_with_stride(2, 2)
}

fn upsample(xs: &Tensor) -> Result<Tensor> {
    let (_, _, height, width) = xs.dims4()?;
    xs.upsample_nearest2d(height * 2, width * 2)
}

fn spread(xs: &Tensor, channels: usize) -> Result<Tensor> {
    let (batch, _, height, width) = xs.dims4()?;
    xs.permute((0, 2, 3, 1))?
        .contiguous()?
        .reshape((batch, height * width, channels))
}

impl YuNet {
    pub fn load_from(path: &Path) -> Result<Self> {
        let device = candle_core::Device::Cpu;
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[path], candle_core::DType::F32, &device)?
        };
        Self::load(vb)
    }

    pub fn load(vb: VarBuilder) -> Result<Self> {
        let backbone = vb.pp("backbone");
        let stem = conv2d(
            3,
            16,
            3,
            Conv2dConfig {
                padding: 1,
                stride: 2,
                groups: 1,
                dilation: 1,
                ..Default::default()
            },
            backbone.pp("model0").pp("conv1"),
        )?;
        let model0 = ConvDPUnit::load(backbone.pp("model0").pp("conv2"), 16, 16, true)?;
        let model1 = Stage::load(backbone.pp("model1"), 16, 16, 32)?;
        let model2 = Stage::load(backbone.pp("model2"), 32, 32, 64)?;
        let model3 = Stage::load(backbone.pp("model3"), 64, 64, 64)?;
        let model4 = Stage::load(backbone.pp("model4"), 64, 64, 64)?;
        let model5 = Stage::load(backbone.pp("model5"), 64, 64, 64)?;

        let necks = vb.pp("neck").pp("lateral_convs");
        let lateral = [
            ConvDPUnit::load(necks.pp("0"), 64, 64, true)?,
            ConvDPUnit::load(necks.pp("1"), 64, 64, true)?,
            ConvDPUnit::load(necks.pp("2"), 64, 64, true)?,
        ];

        let bbox_head = vb.pp("bbox_head");
        let head = |level: usize| -> Result<Head> {
            let at = |name: &str| bbox_head.pp(name).pp(level.to_string());
            Ok(Head {
                cls: ConvDPUnit::load(at("multi_level_cls"), 64, 1, false)?,
                obj: ConvDPUnit::load(at("multi_level_obj"), 64, 1, false)?,
                bbox: ConvDPUnit::load(at("multi_level_bbox"), 64, 4, false)?,
                kps: ConvDPUnit::load(at("multi_level_kps"), 64, 10, false)?,
            })
        };
        let heads = [head(0)?, head(1)?, head(2)?];

        Ok(Self {
            stem,
            model0,
            model1,
            model2,
            model3,
            model4,
            model5,
            lateral,
            heads,
        })
    }

    pub fn forward(&self, xs: &Tensor) -> Result<Vec<Level>> {
        let xs = self.stem.forward(xs)?.relu()?;
        let xs = self.model0.forward(&xs)?;
        let xs = pool(&xs)?;
        let xs = self.model1.forward(&xs)?;
        let xs = self.model2.forward(&xs)?;
        let xs = pool(&xs)?;
        let c8 = self.model3.forward(&xs)?;
        let xs = pool(&c8)?;
        let c16 = self.model4.forward(&xs)?;
        let xs = pool(&c16)?;
        let c32 = self.model5.forward(&xs)?;

        let p32 = self.lateral[2].forward(&c32)?;
        let p16 = self.lateral[1].forward(&(c16 + upsample(&p32)?)?)?;
        let p8 = self.lateral[0].forward(&(c8 + upsample(&p16)?)?)?;

        let mut levels = Vec::with_capacity(3);
        for (index, feature) in [&p8, &p16, &p32].into_iter().enumerate() {
            let head = &self.heads[index];
            levels.push(Level {
                cls: candle_nn::ops::sigmoid(&spread(&head.cls.forward(feature)?, 1)?)?,
                obj: candle_nn::ops::sigmoid(&spread(&head.obj.forward(feature)?, 1)?)?,
                bbox: spread(&head.bbox.forward(feature)?, 4)?,
                kps: spread(&head.kps.forward(feature)?, KEYPOINTS * 2)?,
                stride: STRIDES[index],
            });
        }
        Ok(levels)
    }
}

pub fn decode(levels: &[Level], width: usize, height: usize, floor: f32) -> Result<Vec<Face>> {
    let mut found = Vec::new();
    for level in levels {
        let stride = level.stride;
        let across = width / stride;
        let down = height / stride;
        let cls = level.cls.flatten_all()?.to_vec1::<f32>()?;
        let obj = level.obj.flatten_all()?.to_vec1::<f32>()?;
        let bbox = level.bbox.flatten_all()?.to_vec1::<f32>()?;
        let kps = level.kps.flatten_all()?.to_vec1::<f32>()?;

        for row in 0..down {
            for column in 0..across {
                let at = row * across + column;
                let score = (cls[at].clamp(0.0, 1.0) * obj[at].clamp(0.0, 1.0)).sqrt();
                if score < floor {
                    continue;
                }
                let step = stride as f32;
                let middle_x = (column as f32 + bbox[at * 4]) * step;
                let middle_y = (row as f32 + bbox[at * 4 + 1]) * step;
                let span = bbox[at * 4 + 2].exp() * step;
                let rise = bbox[at * 4 + 3].exp() * step;

                let mut points = [[0f32; 2]; KEYPOINTS];
                for (mark, point) in points.iter_mut().enumerate() {
                    point[0] = (column as f32 + kps[at * KEYPOINTS * 2 + mark * 2]) * step;
                    point[1] = (row as f32 + kps[at * KEYPOINTS * 2 + mark * 2 + 1]) * step;
                }

                found.push(Face {
                    x: middle_x - span / 2.0,
                    y: middle_y - rise / 2.0,
                    width: span,
                    height: rise,
                    score,
                    points,
                });
            }
        }
    }
    Ok(found)
}

pub fn winnow(mut found: Vec<Face>, ceiling: f32) -> Vec<Face> {
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Face> = Vec::new();
    for face in found {
        if kept.iter().all(|held| held.overlap(&face) <= ceiling) {
            kept.push(face);
        }
    }
    kept
}

pub fn beside_the_crate() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join(FILE_NAME)
}

fn letterbox(width: u32, height: u32) -> (f32, usize, usize) {
    let longest = width.max(height).max(1) as f32;
    let by = (INPUT_SIDE as f32 / longest).min(1.0);
    let fitted = |side: u32| -> usize {
        let scaled = ((side as f32 * by).round() as usize).max(1);
        scaled.div_ceil(SHRINK_TO) * SHRINK_TO
    };
    (by, fitted(width), fitted(height))
}

impl YuNet {
    pub fn look(&self, path: &Path) -> crate::Result<Vec<Face>> {
        let opened = crate::imageio::open_upright(path).ok_or_else(|| {
            crate::Error::Tagging(format!("{}: cannot be read as a picture", path.display()))
        })?;
        self.look_at(&opened)
    }

    pub fn look_at(&self, image: &image::DynamicImage) -> crate::Result<Vec<Face>> {
        self.look_closely(image, SCORE_FLOOR, OVERLAP_CEILING)
    }

    pub fn look_closely(
        &self,
        image: &image::DynamicImage,
        floor: f32,
        ceiling: f32,
    ) -> crate::Result<Vec<Face>> {
        let stalled = |e: candle_core::Error| crate::Error::Tagging(e.to_string());
        let (by, width, height) = letterbox(image.width(), image.height());
        let pixels = planes(image, by, width, height).map_err(stalled)?;
        let levels = self.forward(&pixels).map_err(stalled)?;
        let found = decode(&levels, width, height, floor).map_err(stalled)?;
        Ok(winnow(found, ceiling)
            .into_iter()
            .map(|face| face.scaled(by))
            .collect())
    }
}

fn planes(image: &image::DynamicImage, by: f32, width: usize, height: usize) -> Result<Tensor> {
    let across = ((image.width() as f32 * by).round() as u32).max(1);
    let down = ((image.height() as f32 * by).round() as u32).max(1);
    let scaled = image
        .resize_exact(across, down, image::imageops::FilterType::Triangle)
        .to_rgb8();

    let mut planes = vec![0f32; 3 * width * height];
    for (x, y, pixel) in scaled.enumerate_pixels() {
        let (x, y) = (x as usize, y as usize);
        if x >= width || y >= height {
            continue;
        }
        let at = y * width + x;
        for (plane, channel) in [2usize, 1, 0].into_iter().enumerate() {
            planes[plane * width * height + at] = pixel.0[channel] as f32;
        }
    }

    Tensor::from_vec(planes, (1, 3, height, width), &candle_core::Device::Cpu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::{DType, Device};
    use std::collections::HashMap;

    fn zeros(shape: &[usize]) -> Tensor {
        Tensor::zeros(shape, DType::F32, &Device::Cpu).unwrap()
    }

    fn conv(store: &mut HashMap<String, Tensor>, at: &str, out: usize, inp: usize, k: usize) {
        store.insert(format!("{at}.weight"), zeros(&[out, inp, k, k]));
        store.insert(format!("{at}.bias"), zeros(&[out]));
    }

    fn unit(store: &mut HashMap<String, Tensor>, at: &str, inp: usize, out: usize) {
        conv(store, &format!("{at}.conv1"), out, inp, 1);
        conv(store, &format!("{at}.conv2"), out, 1, 3);
    }

    fn weights() -> HashMap<String, Tensor> {
        let mut store = HashMap::new();
        conv(&mut store, "backbone.model0.conv1", 16, 3, 3);
        unit(&mut store, "backbone.model0.conv2", 16, 16);
        for (stage, inp, mid, out) in [
            ("model1", 16, 16, 32),
            ("model2", 32, 32, 64),
            ("model3", 64, 64, 64),
            ("model4", 64, 64, 64),
            ("model5", 64, 64, 64),
        ] {
            unit(&mut store, &format!("backbone.{stage}.conv1"), inp, mid);
            unit(&mut store, &format!("backbone.{stage}.conv2"), mid, out);
        }
        for level in 0..3 {
            unit(&mut store, &format!("neck.lateral_convs.{level}"), 64, 64);
            for (name, out) in [
                ("multi_level_cls", 1),
                ("multi_level_obj", 1),
                ("multi_level_bbox", 4),
                ("multi_level_kps", 10),
            ] {
                unit(&mut store, &format!("bbox_head.{name}.{level}"), 64, out);
            }
        }
        store
    }

    fn built() -> YuNet {
        let vb = VarBuilder::from_tensors(weights(), DType::F32, &Device::Cpu);
        YuNet::load(vb).expect("the shapes should line up")
    }

    #[test]
    fn every_weight_the_graph_asks_for_is_the_shape_it_expects() {
        built();
    }

    #[test]
    fn the_weights_are_installed_beside_the_program() {
        let path = beside_the_crate();
        let found = std::fs::metadata(&path).unwrap().len();
        assert_eq!(found, BYTES, "{} is not intact", path.display());
        YuNet::load_from(&path).expect("the shipped weights should load");
        assert!(CREDIT.contains("MIT"), "the credit must name the licence");
        assert!(
            CREDIT.contains("Shiqi Yu"),
            "the credit must name the author"
        );
    }

    #[test]
    fn each_level_reports_one_row_per_cell_of_its_stride() {
        let side = 128;
        let out = built()
            .forward(&zeros(&[1, 3, side, side]))
            .expect("a forward pass should run");

        assert_eq!(out.len(), 3);
        for level in &out {
            let cells = (side / level.stride) * (side / level.stride);
            assert_eq!(level.cls.dims(), &[1, cells, 1], "cls at {}", level.stride);
            assert_eq!(level.obj.dims(), &[1, cells, 1], "obj at {}", level.stride);
            assert_eq!(
                level.bbox.dims(),
                &[1, cells, 4],
                "bbox at {}",
                level.stride
            );
            assert_eq!(
                level.kps.dims(),
                &[1, cells, KEYPOINTS * 2],
                "kps at {}",
                level.stride
            );
        }
    }

    #[test]
    fn the_levels_come_back_finest_first() {
        let out = built().forward(&zeros(&[1, 3, 128, 128])).unwrap();
        let strides: Vec<usize> = out.iter().map(|l| l.stride).collect();
        assert_eq!(strides, vec![8, 16, 32]);
    }

    #[test]
    fn the_scores_are_squeezed_into_nought_to_one() {
        let out = built().forward(&zeros(&[1, 3, 128, 128])).unwrap();
        for level in &out {
            let scores = level.cls.flatten_all().unwrap().to_vec1::<f32>().unwrap();
            for score in scores {
                assert!((0.0..=1.0).contains(&score), "{score} is not a score");
            }
        }
    }

    #[test]
    fn the_input_side_divides_by_every_stride() {
        for stride in STRIDES {
            assert_eq!(INPUT_SIDE % stride, 0, "{stride} does not divide the input");
        }
    }
    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    fn fixture(name: &str) -> Vec<f32> {
        let path = fixture_path(name);
        let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        raw.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    #[test]
    fn matches_the_reference_graph_it_was_written_from() {
        let net = YuNet::load_from(&beside_the_crate()).expect("the shipped weights should load");
        let input = Tensor::from_vec(fixture("probe.f32"), (1, 3, 128, 128), &Device::Cpu).unwrap();
        let levels = net.forward(&input).unwrap();

        let mut worst = 0f32;
        for level in &levels {
            for (name, got) in [
                ("cls", &level.cls),
                ("obj", &level.obj),
                ("bbox", &level.bbox),
                ("kps", &level.kps),
            ] {
                let want = fixture(&format!("exp_{name}_{}.f32", level.stride));
                let got = got.flatten_all().unwrap().to_vec1::<f32>().unwrap();
                assert_eq!(got.len(), want.len(), "{name}_{}", level.stride);
                let diff = got
                    .iter()
                    .zip(&want)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0f32, f32::max);
                println!("{name}_{:<3} max|diff| = {diff:.3e}", level.stride);
                worst = worst.max(diff);
            }
        }
        println!("WORST = {worst:.3e}");
        assert!(worst < 1e-4, "drifted from the reference by {worst}");
    }

    fn face_at(x: f32, y: f32, side: f32, score: f32) -> Face {
        Face {
            x,
            y,
            width: side,
            height: side,
            score,
            points: [[0.0; 2]; KEYPOINTS],
        }
    }

    #[test]
    fn a_box_fully_over_another_overlaps_completely() {
        let one = face_at(0.0, 0.0, 10.0, 1.0);
        assert!((one.overlap(&one) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn boxes_that_never_touch_do_not_overlap() {
        let one = face_at(0.0, 0.0, 10.0, 1.0);
        let far = face_at(100.0, 100.0, 10.0, 1.0);
        assert_eq!(one.overlap(&far), 0.0);
    }

    #[test]
    fn half_a_box_shared_is_a_third_of_the_union() {
        let one = face_at(0.0, 0.0, 10.0, 1.0);
        let half = face_at(5.0, 0.0, 10.0, 1.0);
        assert!((one.overlap(&half) - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn the_weaker_of_two_piled_boxes_is_dropped() {
        let kept = winnow(
            vec![
                face_at(0.0, 0.0, 10.0, 0.4),
                face_at(0.5, 0.5, 10.0, 0.9),
                face_at(80.0, 80.0, 10.0, 0.7),
            ],
            OVERLAP_CEILING,
        );
        assert_eq!(kept.len(), 2, "the pile should collapse to one");
        assert_eq!(kept[0].score, 0.9, "the strongest should lead");
        assert_eq!(kept[1].score, 0.7);
    }

    #[test]
    fn a_cell_decodes_to_the_box_the_formula_says() {
        let device = Device::Cpu;
        let hold = |values: Vec<f32>, channels: usize| {
            Tensor::from_vec(values, (1, 1, channels), &device).unwrap()
        };
        let levels = vec![Level {
            cls: hold(vec![1.0], 1),
            obj: hold(vec![1.0], 1),
            bbox: hold(vec![0.5, 0.5, 0.0, 0.0], 4),
            kps: hold(vec![0.5; KEYPOINTS * 2], KEYPOINTS * 2),
            stride: 32,
        }];

        let found = decode(&levels, 32, 32, 0.5).expect("one cell should decode");
        assert_eq!(found.len(), 1);
        let face = found[0];
        assert!((face.width - 32.0).abs() < 1e-4, "{face:?}");
        assert!((face.height - 32.0).abs() < 1e-4, "{face:?}");
        assert!((face.x - 0.0).abs() < 1e-4, "{face:?}");
        assert!((face.y - 0.0).abs() < 1e-4, "{face:?}");
        assert!((face.score - 1.0).abs() < 1e-6);
        for point in face.points {
            assert!((point[0] - 16.0).abs() < 1e-4, "{point:?}");
            assert!((point[1] - 16.0).abs() < 1e-4, "{point:?}");
        }
    }

    #[test]
    fn a_cell_below_the_floor_is_never_reported() {
        let device = Device::Cpu;
        let hold = |values: Vec<f32>, channels: usize| {
            Tensor::from_vec(values, (1, 1, channels), &device).unwrap()
        };
        let levels = vec![Level {
            cls: hold(vec![0.1], 1),
            obj: hold(vec![0.1], 1),
            bbox: hold(vec![0.5, 0.5, 0.0, 0.0], 4),
            kps: hold(vec![0.5; KEYPOINTS * 2], KEYPOINTS * 2),
            stride: 32,
        }];
        assert!(decode(&levels, 32, 32, SCORE_FLOOR).unwrap().is_empty());
    }

    #[test]
    fn a_small_picture_is_padded_rather_than_stretched() {
        let (by, width, height) = letterbox(100, 50);
        assert_eq!(by, 1.0, "a small picture is never blown up");
        assert_eq!(width % SHRINK_TO, 0);
        assert_eq!(height % SHRINK_TO, 0);
        assert!(width >= 100 && height >= 50, "{width}x{height}");
    }

    #[test]
    fn a_huge_picture_is_brought_down_to_the_input_side() {
        let (by, width, height) = letterbox(4000, 3000);
        assert!((by - INPUT_SIDE as f32 / 4000.0).abs() < 1e-6, "{by}");
        assert!(
            width <= INPUT_SIDE && height <= INPUT_SIDE,
            "{width}x{height}"
        );
        assert_eq!(width % SHRINK_TO, 0);
        assert_eq!(height % SHRINK_TO, 0);
        let stretched = (width as f32 / height as f32) / (4000.0 / 3000.0);
        assert!((stretched - 1.0).abs() < 0.05, "the shape was distorted");
    }

    #[test]
    fn the_boxes_agree_with_the_detector_opencv_ships() {
        let raw = std::fs::read_to_string(fixture_path("reference-faces.json")).unwrap();
        let want: Vec<Vec<f32>> = serde_json::from_str(&raw).unwrap();
        let image = image::open(fixture_path("probe-face.png")).unwrap();
        let (side, floor) = (96usize, 1e-4);
        assert_eq!((image.width(), image.height()), (side as u32, side as u32));

        let net = YuNet::load_from(&beside_the_crate()).expect("the shipped weights should load");
        let pixels = planes(&image, 1.0, side, side).unwrap();
        let levels = net.forward(&pixels).unwrap();
        let mut mine = decode(&levels, side, side, floor).unwrap();
        mine.sort_by(|a, b| b.score.total_cmp(&a.score));

        assert_eq!(mine.len(), want.len(), "a different number of boxes");

        let mut worst_edge = 0f32;
        let mut worst_point = 0f32;
        let mut worst_score = 0f32;
        for (got, expected) in mine.iter().zip(&want) {
            for (edge, against) in [
                (got.x, expected[0]),
                (got.y, expected[1]),
                (got.width, expected[2]),
                (got.height, expected[3]),
            ] {
                worst_edge = worst_edge.max((edge - against).abs());
            }
            for (mark, point) in got.points.iter().enumerate() {
                worst_point = worst_point
                    .max((point[0] - expected[4 + mark * 2]).abs())
                    .max((point[1] - expected[5 + mark * 2]).abs());
            }
            worst_score = worst_score.max((got.score - expected[14]).abs());
        }
        println!(
            "WORST EDGE = {worst_edge:.3e}, KEYPOINT = {worst_point:.3e}, SCORE = {worst_score:.3e}"
        );
        assert!(worst_edge < 0.01, "boxes drifted by {worst_edge} pixels");
        assert!(worst_point < 0.01, "keypoints drifted by {worst_point}");
        assert!(worst_score < 1e-4, "scores drifted by {worst_score}");
    }
}
