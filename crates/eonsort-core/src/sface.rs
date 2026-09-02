#![cfg(feature = "faces")]

use candle_core::{DType, Device, Result, Tensor};
use candle_nn::{conv2d_no_bias, linear, Conv2d, Conv2dConfig, Linear, Module, VarBuilder};
use std::path::Path;

pub const CREDIT: &str =
    "Face recognition by SFace, Apache 2.0, Copyright (c) 2021 Zhong Yaoyao, Deng Weihong";
pub const FILE_NAME: &str = "sface.safetensors";
pub const BYTES: u64 = 38_682_864;

pub const SIDE: usize = 112;
pub const EMBEDDING: usize = 128;
pub const MIDDLE: f64 = 127.5;
pub const SPREAD: f64 = 0.0078125;
pub const CONV_EPSILON: f64 = 1e-3;
pub const TAIL_EPSILON: f64 = 2e-5;
pub const SAME_PERSON: f32 = 0.363;

#[derive(Debug)]
struct Norm {
    scale: Tensor,
    shift: Tensor,
}

impl Norm {
    fn load(vb: VarBuilder, channels: usize, epsilon: f64) -> Result<Self> {
        let gamma = vb.get(channels, "gamma")?;
        let beta = vb.get(channels, "beta")?;
        let mean = vb.get(channels, "mean")?;
        let variance = vb.get(channels, "var")?;

        let scale = (gamma / (variance + epsilon)?.sqrt()?)?;
        let shift = (beta - (&mean * &scale)?)?;
        Ok(Self { scale, shift })
    }

    fn over_planes(&self, xs: &Tensor) -> Result<Tensor> {
        let channels = self.scale.dim(0)?;
        let scale = self.scale.reshape((1, channels, 1, 1))?;
        let shift = self.shift.reshape((1, channels, 1, 1))?;
        xs.broadcast_mul(&scale)?.broadcast_add(&shift)
    }

    fn over_rows(&self, xs: &Tensor) -> Result<Tensor> {
        xs.broadcast_mul(&self.scale)?.broadcast_add(&self.shift)
    }
}

fn prelu(xs: &Tensor, slope: &Tensor) -> Result<Tensor> {
    let channels = slope.dim(0)?;
    let slope = slope.reshape((1, channels, 1, 1))?;
    let under = xs.neg()?.relu()?.broadcast_mul(&slope)?;
    xs.relu()? - under
}

#[derive(Debug)]
struct Unit {
    conv: Conv2d,
    norm: Norm,
    slope: Tensor,
}

impl Unit {
    fn load(
        vb: VarBuilder,
        in_channels: usize,
        out_channels: usize,
        kernel: usize,
        stride: usize,
        groups: usize,
    ) -> Result<Self> {
        let conv = conv2d_no_bias(
            in_channels,
            out_channels,
            kernel,
            Conv2dConfig {
                padding: kernel / 2,
                stride,
                groups,
                dilation: 1,
                ..Default::default()
            },
            vb.pp("conv"),
        )?;
        Ok(Self {
            conv,
            norm: Norm::load(vb.pp("bn"), out_channels, CONV_EPSILON)?,
            slope: vb.get(out_channels, "prelu")?,
        })
    }
}

impl Module for Unit {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let xs = self.norm.over_planes(&self.conv.forward(xs)?)?;
        prelu(&xs, &self.slope)
    }
}

#[derive(Debug)]
struct Block {
    deep: Unit,
    point: Unit,
}

impl Block {
    fn load(
        vb: VarBuilder,
        in_channels: usize,
        out_channels: usize,
        stride: usize,
    ) -> Result<Self> {
        Ok(Self {
            deep: Unit::load(
                vb.pp("dw"),
                in_channels,
                in_channels,
                3,
                stride,
                in_channels,
            )?,
            point: Unit::load(vb.pp("pw"), in_channels, out_channels, 1, 1, 1)?,
        })
    }
}

impl Module for Block {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        self.point.forward(&self.deep.forward(xs)?)
    }
}

const PLAN: [(usize, usize, usize); 13] = [
    (32, 64, 1),
    (64, 128, 2),
    (128, 128, 1),
    (128, 256, 2),
    (256, 256, 1),
    (256, 512, 2),
    (512, 512, 1),
    (512, 512, 1),
    (512, 512, 1),
    (512, 512, 1),
    (512, 512, 1),
    (512, 1024, 2),
    (1024, 1024, 1),
];

#[derive(Debug)]
pub struct SFace {
    stem: Unit,
    blocks: Vec<Block>,
    settle: Norm,
    fc: Linear,
    out: Norm,
}

impl SFace {
    pub fn load_from(path: &Path) -> Result<Self> {
        let device = Device::Cpu;
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[path], DType::F32, &device)? };
        Self::load(vb)
    }

    pub fn load(vb: VarBuilder) -> Result<Self> {
        let stem = Unit::load(vb.pp("stem"), 3, 32, 3, 1, 1)?;
        let mut blocks = Vec::with_capacity(PLAN.len());
        for (at, (into, out, stride)) in PLAN.iter().enumerate() {
            blocks.push(Block::load(
                vb.pp(format!("block{}", at + 2)),
                *into,
                *out,
                *stride,
            )?);
        }

        let tail = vb.pp("tail");
        Ok(Self {
            stem,
            blocks,
            settle: Norm::load(tail.pp("bn"), 1024, TAIL_EPSILON)?,
            fc: linear(1024 * 7 * 7, EMBEDDING, tail.pp("fc"))?,
            out: Norm::load(tail.pp("out"), EMBEDDING, TAIL_EPSILON)?,
        })
    }

    pub fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let xs = ((xs - MIDDLE)? * SPREAD)?;
        let mut xs = self.stem.forward(&xs)?;
        for block in &self.blocks {
            xs = block.forward(&xs)?;
        }
        let xs = self.settle.over_planes(&xs)?;
        let xs = xs.flatten_from(1)?;
        self.out.over_rows(&self.fc.forward(&xs)?)
    }

    pub fn embed(&self, face: &image::DynamicImage) -> Result<Vec<f32>> {
        let pixels = planes(face)?;
        self.forward(&pixels)?.flatten_all()?.to_vec1::<f32>()
    }
}

fn planes(face: &image::DynamicImage) -> Result<Tensor> {
    let scaled = face
        .resize_exact(
            SIDE as u32,
            SIDE as u32,
            image::imageops::FilterType::Triangle,
        )
        .to_rgb8();

    let mut planes = vec![0f32; 3 * SIDE * SIDE];
    for (x, y, pixel) in scaled.enumerate_pixels() {
        let at = y as usize * SIDE + x as usize;
        for channel in 0..3 {
            planes[channel * SIDE * SIDE + at] = pixel.0[channel] as f32;
        }
    }

    Tensor::from_vec(planes, (1, 3, SIDE, SIDE), &Device::Cpu)
}

pub fn alike(a: &[f32], b: &[f32]) -> f32 {
    let mut one = a.to_vec();
    let mut two = b.to_vec();
    crate::tags::normalise(&mut one);
    crate::tags::normalise(&mut two);
    crate::tags::cosine(&one, &two)
}

pub fn same_person(a: &[f32], b: &[f32]) -> bool {
    alike(a, b) >= SAME_PERSON
}

pub fn beside_the_crate() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join(FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn the_plan_walks_the_picture_down_to_seven_by_seven() {
        let mut side = SIDE;
        for (_, _, stride) in PLAN {
            side /= stride;
        }
        assert_eq!(side, 7, "the tail expects a seven by seven field");
        assert_eq!(PLAN.last().unwrap().1, 1024, "the tail expects 1024 planes");
    }

    #[test]
    fn the_credit_names_the_licence_and_the_authors() {
        assert!(CREDIT.contains("Apache"), "{CREDIT}");
        assert!(CREDIT.contains("Zhong Yaoyao"), "{CREDIT}");
    }

    #[test]
    fn a_normaliser_folds_to_the_affine_it_stands_for() {
        let device = Device::Cpu;
        let hold = |values: Vec<f32>| Tensor::from_vec(values, 2, &device).unwrap();
        let vb = VarBuilder::from_tensors(
            [
                ("gamma".to_string(), hold(vec![2.0, 3.0])),
                ("beta".to_string(), hold(vec![1.0, -1.0])),
                ("mean".to_string(), hold(vec![0.5, 2.0])),
                ("var".to_string(), hold(vec![4.0, 9.0])),
            ]
            .into_iter()
            .collect(),
            DType::F32,
            &device,
        );

        let norm = Norm::load(vb, 2, 0.0).unwrap();
        let xs = Tensor::from_vec(vec![0.5f32, 2.0], (1, 2), &device).unwrap();
        let out = norm.over_rows(&xs).unwrap().flatten_all().unwrap();
        let out = out.to_vec1::<f32>().unwrap();
        assert!((out[0] - 1.0).abs() < 1e-6, "{out:?}");
        assert!((out[1] + 1.0).abs() < 1e-6, "{out:?}");
    }

    #[test]
    fn prelu_leaves_what_is_positive_and_leans_on_what_is_not() {
        let device = Device::Cpu;
        let xs = Tensor::from_vec(vec![2.0f32, -2.0], (1, 2, 1, 1), &device).unwrap();
        let slope = Tensor::from_vec(vec![0.25f32, 0.25], 2, &device).unwrap();
        let out = prelu(&xs, &slope).unwrap().flatten_all().unwrap();
        let out = out.to_vec1::<f32>().unwrap();
        assert!((out[0] - 2.0).abs() < 1e-6, "{out:?}");
        assert!((out[1] + 0.5).abs() < 1e-6, "{out:?}");
    }

    #[test]
    fn two_of_the_same_vector_are_the_same_person() {
        let one = vec![1.0f32, 0.0, 0.0];
        assert!(same_person(&one, &one));
        assert!(!same_person(&one, &[0.0, 1.0, 0.0]));
    }

    #[test]
    fn the_template_points_sit_inside_the_crop() {
        for point in TEMPLATE {
            assert!(point[0] > 0.0 && point[0] < SIDE as f32, "{point:?}");
            assert!(point[1] > 0.0 && point[1] < SIDE as f32, "{point:?}");
        }
    }

    #[test]
    fn a_face_already_on_the_template_needs_no_moving() {
        let fit = fit_to_template(&TEMPLATE);
        assert!((fit.turn - 1.0).abs() < 1e-4, "{fit:?}");
        assert!(fit.lean.abs() < 1e-4, "{fit:?}");
        assert!(fit.across.abs() < 1e-3, "{fit:?}");
        assert!(fit.down.abs() < 1e-3, "{fit:?}");
    }

    #[test]
    fn a_face_twice_the_size_is_halved_again() {
        let doubled = TEMPLATE.map(|p| [p[0] * 2.0, p[1] * 2.0]);
        let fit = fit_to_template(&doubled);
        assert!((fit.turn - 0.5).abs() < 1e-4, "{fit:?}");
        assert!(fit.lean.abs() < 1e-4, "{fit:?}");
    }

    #[test]
    fn a_face_lying_on_its_side_is_stood_back_up() {
        let turned = TEMPLATE.map(|p| [-p[1], p[0]]);
        let fit = fit_to_template(&turned);
        for (at, point) in turned.iter().enumerate() {
            let (x, y) = fit.forward(point[0], point[1]);
            assert!((x - TEMPLATE[at][0]).abs() < 1e-2, "{at}: {x}");
            assert!((y - TEMPLATE[at][1]).abs() < 1e-2, "{at}: {y}");
        }
    }

    #[test]
    fn going_forward_and_back_lands_where_it_started() {
        let moved = TEMPLATE.map(|p| [p[0] * 1.7 + 12.0, p[1] * 1.7 - 5.0]);
        let fit = fit_to_template(&moved);
        for point in moved {
            let (x, y) = fit.forward(point[0], point[1]);
            let (back_x, back_y) = fit.back(x, y);
            assert!(
                (back_x - point[0]).abs() < 1e-2,
                "{back_x} against {}",
                point[0]
            );
            assert!(
                (back_y - point[1]).abs() < 1e-2,
                "{back_y} against {}",
                point[1]
            );
        }
    }

    #[test]
    fn aligning_lands_the_eyes_where_the_template_wants_them() {
        let mut picture = image::RgbImage::new(200, 200);
        for (x, y, pixel) in picture.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x % 256) as u8, (y % 256) as u8, 40]);
        }
        let points = TEMPLATE.map(|p| [p[0] * 1.5 + 20.0, p[1] * 1.5 + 10.0]);
        let cropped = align(&picture, &points);

        assert_eq!(cropped.width(), SIDE as u32);
        assert_eq!(cropped.height(), SIDE as u32);

        let fit = fit_to_template(&points);
        let (x, y) = fit.forward(points[0][0], points[0][1]);
        assert!((x - TEMPLATE[0][0]).abs() < 0.5, "left eye landed at {x}");
        assert!((y - TEMPLATE[0][1]).abs() < 0.5, "left eye landed at {y}");
    }

    #[test]
    fn a_crop_that_runs_off_the_picture_still_fills_every_pixel() {
        let picture = image::RgbImage::from_pixel(20, 20, image::Rgb([7, 8, 9]));
        let points = TEMPLATE.map(|p| [p[0] * 4.0 - 60.0, p[1] * 4.0 - 60.0]);
        let cropped = align(&picture, &points);
        assert!(cropped.pixels().all(|p| p.0 == [7, 8, 9]), "edges leaked");
    }

    #[test]
    fn matches_the_recogniser_opencv_ships() {
        let weights = beside_the_crate();
        let raw = std::fs::read_to_string(fixture_path("reference-feature.json")).unwrap();
        let want: Vec<f32> = serde_json::from_str(&raw).unwrap();
        let image = image::open(fixture_path("probe-sface.png")).unwrap();
        assert_eq!((image.width(), image.height()), (SIDE as u32, SIDE as u32));

        let net = SFace::load_from(&weights).expect("the weights should load");
        let got = net.embed(&image).expect("the face should embed");

        assert_eq!(got.len(), EMBEDDING);
        assert_eq!(want.len(), EMBEDDING);
        let worst = got
            .iter()
            .zip(&want)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        let closeness = alike(&got, &want);
        println!("WORST = {worst:.3e}, COSINE = {closeness:.6}");
        assert!(worst < 1e-3, "drifted from the reference by {worst}");
        assert!(closeness > 0.9999, "the embedding points elsewhere");
    }
}

pub const TEMPLATE: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    pub turn: f32,
    pub lean: f32,
    pub across: f32,
    pub down: f32,
}

pub fn fit_to_template(points: &[[f32; 2]; 5]) -> Fit {
    let count = points.len() as f32;
    let mean = |take: &dyn Fn(usize) -> f32| (0..points.len()).map(take).sum::<f32>() / count;

    let from_x = mean(&|i| points[i][0]);
    let from_y = mean(&|i| points[i][1]);
    let onto_x = mean(&|i| TEMPLATE[i][0]);
    let onto_y = mean(&|i| TEMPLATE[i][1]);

    let mut spread = 0.0;
    let mut along = 0.0;
    let mut across = 0.0;
    for (at, point) in points.iter().enumerate() {
        let (x, y) = (point[0] - from_x, point[1] - from_y);
        let (u, v) = (TEMPLATE[at][0] - onto_x, TEMPLATE[at][1] - onto_y);
        spread += x * x + y * y;
        along += x * u + y * v;
        across += x * v - y * u;
    }

    if spread <= f32::EPSILON {
        return Fit {
            turn: 1.0,
            lean: 0.0,
            across: onto_x - from_x,
            down: onto_y - from_y,
        };
    }

    let turn = along / spread;
    let lean = across / spread;
    Fit {
        turn,
        lean,
        across: onto_x - (turn * from_x - lean * from_y),
        down: onto_y - (lean * from_x + turn * from_y),
    }
}

impl Fit {
    pub fn back(&self, x: f32, y: f32) -> (f32, f32) {
        let weight = self.turn * self.turn + self.lean * self.lean;
        if weight <= f32::EPSILON {
            return (0.0, 0.0);
        }
        let (x, y) = (x - self.across, y - self.down);
        (
            (self.turn * x + self.lean * y) / weight,
            (self.turn * y - self.lean * x) / weight,
        )
    }

    pub fn forward(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.turn * x - self.lean * y + self.across,
            self.lean * x + self.turn * y + self.down,
        )
    }
}

pub fn align(picture: &image::RgbImage, points: &[[f32; 2]; 5]) -> image::RgbImage {
    let fit = fit_to_template(points);
    let (width, height) = (picture.width() as i64, picture.height() as i64);
    let mut out = image::RgbImage::new(SIDE as u32, SIDE as u32);

    for down in 0..SIDE {
        for across in 0..SIDE {
            let (x, y) = fit.back(across as f32 + 0.5, down as f32 + 0.5);
            let (x, y) = (x - 0.5, y - 0.5);
            let left = x.floor();
            let top = y.floor();
            let (fx, fy) = (x - left, y - top);

            let mut mixed = [0f32; 3];
            for (corner, (dx, dy)) in [(0, 0), (1, 0), (0, 1), (1, 1)].into_iter().enumerate() {
                let weight = match corner {
                    0 => (1.0 - fx) * (1.0 - fy),
                    1 => fx * (1.0 - fy),
                    2 => (1.0 - fx) * fy,
                    _ => fx * fy,
                };
                let sx = (left as i64 + dx).clamp(0, width - 1) as u32;
                let sy = (top as i64 + dy).clamp(0, height - 1) as u32;
                let pixel = picture.get_pixel(sx, sy);
                for (channel, value) in mixed.iter_mut().enumerate() {
                    *value += weight * pixel.0[channel] as f32;
                }
            }

            out.put_pixel(
                across as u32,
                down as u32,
                image::Rgb([
                    mixed[0].round().clamp(0.0, 255.0) as u8,
                    mixed[1].round().clamp(0.0, 255.0) as u8,
                    mixed[2].round().clamp(0.0, 255.0) as u8,
                ]),
            );
        }
    }
    out
}
