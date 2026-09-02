use chrono::Local;
use std::path::PathBuf;
use xcap::Monitor;

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub fn take(outer: Rect, inner: Rect) -> Result<PathBuf, String> {
    let window = visible(outer, inner);
    let middle_x = window.x + window.width as i32 / 2;
    let middle_y = window.y + window.height as i32 / 2;
    let monitor = Monitor::from_point(middle_x, middle_y)
        .or_else(|_| Monitor::from_point(window.x, window.y))
        .map_err(|e| format!("no screen holds the window at {middle_x},{middle_y}: {e}"))?;
    let screen = Rect {
        x: monitor.x().map_err(|e| e.to_string())?,
        y: monitor.y().map_err(|e| e.to_string())?,
        width: monitor.width().map_err(|e| e.to_string())?,
        height: monitor.height().map_err(|e| e.to_string())?,
    };
    let screenshot = monitor
        .capture_image()
        .map_err(|e| format!("capture failed: {e}"))?;
    let cut = frame(screen, screenshot.width(), screenshot.height(), window)
        .ok_or_else(|| "the window sits outside the screen".to_string())?;
    let shot = image::imageops::crop_imm(
        &screenshot,
        cut.x as u32,
        cut.y as u32,
        cut.width,
        cut.height,
    )
    .to_image();
    let path = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join(name(&Local::now().format("%Y%m%d-%H%M%S").to_string()));
    shot.save(&path)
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(path)
}

fn visible(outer: Rect, inner: Rect) -> Rect {
    let top = outer.y.min(inner.y);
    Rect {
        x: inner.x,
        y: top,
        width: inner.width,
        height: (inner.y - top) as u32 + inner.height,
    }
}

fn frame(screen: Rect, shot_width: u32, shot_height: u32, window: Rect) -> Option<Rect> {
    let across = f64::from(shot_width) / f64::from(screen.width.max(1));
    let down = f64::from(shot_height) / f64::from(screen.height.max(1));
    let mut left = (f64::from(window.x - screen.x) * across).round() as i64;
    let mut top = (f64::from(window.y - screen.y) * down).round() as i64;
    let mut width = (f64::from(window.width) * across).round() as i64;
    let mut height = (f64::from(window.height) * down).round() as i64;
    if left < 0 {
        width += left;
        left = 0;
    }
    if top < 0 {
        height += top;
        top = 0;
    }
    width = width.min(i64::from(shot_width) - left);
    height = height.min(i64::from(shot_height) - top);
    if width < 1 || height < 1 {
        return None;
    }
    Some(Rect {
        x: left as i32,
        y: top as i32,
        width: width as u32,
        height: height as u32,
    })
}

fn name(stamp: &str) -> String {
    format!("eonsort-{stamp}.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn cut(screen: Rect, shot: (u32, u32), window: Rect) -> (i32, i32, u32, u32) {
        let got = frame(screen, shot.0, shot.1, window).expect("no frame");
        (got.x, got.y, got.width, got.height)
    }

    #[test]
    fn cuts_the_window_out_of_the_screen() {
        assert_eq!(
            cut(SCREEN, (1920, 1080), rect(100, 50, 800, 600)),
            (100, 50, 800, 600)
        );
    }

    #[test]
    fn counts_from_the_corner_of_the_screen_the_window_is_on() {
        let second = rect(1920, 0, 1920, 1080);
        assert_eq!(
            cut(second, (1920, 1080), rect(2020, 50, 800, 600)),
            (100, 50, 800, 600)
        );
    }

    #[test]
    fn follows_a_screenshot_taken_at_a_different_scale() {
        assert_eq!(
            cut(SCREEN, (3840, 2160), rect(100, 50, 800, 600)),
            (200, 100, 1600, 1200)
        );
    }

    #[test]
    fn keeps_the_cut_inside_a_window_hanging_off_the_left_edge() {
        assert_eq!(
            cut(SCREEN, (1920, 1080), rect(-200, -100, 800, 600)),
            (0, 0, 600, 500)
        );
    }

    #[test]
    fn keeps_the_cut_inside_a_window_hanging_off_the_right_edge() {
        assert_eq!(
            cut(SCREEN, (1920, 1080), rect(1600, 800, 800, 600)),
            (1600, 800, 320, 280)
        );
    }

    #[test]
    fn a_window_off_the_screen_altogether_gives_nothing() {
        assert!(frame(SCREEN, 1920, 1080, rect(-900, 0, 800, 600)).is_none());
    }

    #[test]
    fn leaves_the_invisible_frame_out_but_keeps_the_title_bar() {
        let outer = rect(92, 30, 816, 640);
        let inner = rect(100, 61, 800, 600);
        let seen = visible(outer, inner);
        assert_eq!(
            (seen.x, seen.y, seen.width, seen.height),
            (100, 30, 800, 631)
        );
    }

    #[test]
    fn a_window_without_decoration_stays_as_it_is() {
        let same = rect(100, 50, 800, 600);
        let seen = visible(same, same);
        assert_eq!(
            (seen.x, seen.y, seen.width, seen.height),
            (100, 50, 800, 600)
        );
    }

    #[test]
    fn names_the_file_after_the_program_and_the_moment() {
        assert_eq!(name("20260830-101500"), "eonsort-20260830-101500.png");
    }
}
