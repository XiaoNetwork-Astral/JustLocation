//! 路线录制：把系统回调里的真实位置累积成一条可回放的轨迹。
//!
//! 单独的录制过程不产生输出，只收集点；停止后交给调用方保存或丢弃。
//! 这与回放共用同一套 `Position`，所以录下来的轨迹可以直接交给 `Playback`。
//!
//! 两条关键约束：
//!
//! - **录制期间不能同时模拟定位。** 模拟运行时系统回调里给出的是我们自己的合成位置，
//!   照单全收会录出一条绕回自身的轨迹。这里由调用方（协议层）在录制开始时拒绝，并在
//!   `Control` 里保证录制状态下不会接受 `start`。
//! - **按距离抽稀。** 静止不动时系统会持续回调同一个点，全部记下来只会撑爆上限。
//!   小于 `min_distance` 的点按重复处理丢弃，与规格里"采集时过滤重复点"的做法一致。
//!
//! 上限沿用路线模型的 128 点：录满即自动停止，并把 `full` 置位，界面据此提示用户
//! 保存后另起一段，而不是静默丢点。

use crate::Position;
use serde::Serialize;

/// 轨迹上限，与 `route::Route` 的 128 点一致，避免录完还要截断。
pub const MAX_POINTS: usize = 128;
/// 默认抽稀距离（米）。步行轨迹的常见采样噪声在几米以内，取 0.5 米只滤掉真正的重复点。
pub const DEFAULT_MIN_DISTANCE: f64 = 0.5;
const EARTH_RADIUS: f64 = 6_371_008.8;

#[derive(Clone, Debug, Default)]
pub struct Recording {
    points: Vec<Position>,
    /// 从开始录到现在的总时长，单调累加，不受暂停影响。
    duration: f64,
    /// 上一条记录的时间戳，用来判断新的采样是否太近。
    last_seconds: f64,
    /// 最后一个被记录的点，用来做距离抽稀。
    last_point: Option<Position>,
    full: bool,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct RecordState {
    pub points: Vec<Position>,
    pub count: usize,
    /// 已录制时长（秒）。
    pub seconds: f64,
    /// 达到上限后自动停止；此时仍可保存已录到的部分。
    pub full: bool,
    /// 因重复或过近被丢弃的采样数，用来解释"为什么点数比预期少"。
    pub skipped: u64,
}

impl Recording {
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否已经录满上限。录满后 `add` 不再接受新点。
    pub fn is_full(&self) -> bool {
        self.full
    }

    pub fn points(&self) -> &[Position] {
        &self.points
    }

    pub fn seconds(&self) -> f64 {
        self.duration
    }

    /// 记录一个真实位置。返回是否真的记下了。
    ///
    /// `seconds` 是本次录制的单调时间戳（从 0 开始），由调用方给出，便于用假时钟测试。
    pub fn add(&mut self, position: Position, seconds: f64) -> Result<bool, &'static str> {
        if self.full {
            return Ok(false);
        }
        if !seconds.is_finite() || seconds < 0.0 {
            return Err("recording time must be finite and not negative");
        }
        position.validate()?;
        // 时间戳回退（例如调用方换了时钟）时按 0 间隔处理，而不是把时长算回去。
        self.duration = self.duration.max(seconds);
        if let Some(previous) = &self.last_point {
            let moved = distance(previous, &position);
            // 没动就丢：与时间无关。系统在静止时会持续回调同一个点，
            // 早先按"距离近且时间也近"判断，结果站着不动超过 0.2 秒就会被重新记一遍。
            if moved < DEFAULT_MIN_DISTANCE {
                return Ok(false);
            }
        }
        self.points.push(position.clone());
        self.last_point = Some(position);
        self.last_seconds = seconds;
        if self.points.len() >= MAX_POINTS {
            self.full = true;
        }
        Ok(true)
    }

    pub fn state(&self, skipped: u64) -> RecordState {
        RecordState {
            points: self.points.clone(),
            count: self.points.len(),
            seconds: self.duration,
            full: self.full,
            skipped,
        }
    }
}

/// 两点间的大圆距离（米）。与 `route` 模块用同一套公式与半径。
fn distance(a: &Position, b: &Position) -> f64 {
    let dlat = (b.latitude - a.latitude).to_radians();
    let dlon = (b.longitude - a.longitude).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + a.latitude.to_radians().cos() * b.latitude.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * h.clamp(0.0, 1.0).sqrt().asin() * EARTH_RADIUS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(latitude: f64, longitude: f64) -> Position {
        Position::new(latitude, longitude)
    }

    #[test]
    fn repeated_samples_of_a_still_position_are_dropped() {
        let mut recording = Recording::new();
        assert!(recording.add(at(31.0, 121.0), 0.0).unwrap());
        // 站着不动时系统会持续回调同一个点，只有很小的抖动。
        for step in 1..10 {
            assert!(!recording.add(at(31.0, 121.0), step as f64 * 0.5).unwrap());
        }
        assert_eq!(recording.points().len(), 1);
    }

    #[test]
    fn moving_samples_are_kept_even_when_they_arrive_quickly() {
        let mut recording = Recording::new();
        recording.add(at(31.0, 121.0), 0.0).unwrap();
        // 相邻点相差约 11 米，即使时间间隔很短也要记下来。
        assert!(recording.add(at(31.0001, 121.0), 0.1).unwrap());
        assert!(recording.add(at(31.0002, 121.0), 0.2).unwrap());
        assert_eq!(recording.points().len(), 3);
        assert!((recording.seconds() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn the_track_stops_at_the_limit_and_says_so() {
        let mut recording = Recording::new();
        for index in 0..MAX_POINTS + 5 {
            // 每步约 11 米，确保每次都被记录。
            recording.add(at(31.0 + index as f64 * 0.0001, 121.0), index as f64).unwrap();
        }
        assert_eq!(recording.points().len(), MAX_POINTS);
        assert!(recording.is_full());
        // 录满之后不再接受新点，也不再累计时长。
        assert!(!recording.add(at(32.0, 121.0), MAX_POINTS as f64 + 1.0).unwrap());
        assert_eq!(recording.points().len(), MAX_POINTS);
    }

    #[test]
    fn invalid_positions_are_rejected_instead_of_poisoning_the_track() {
        let mut recording = Recording::new();
        let mut broken = at(31.0, 121.0);
        broken.latitude = 91.0;
        assert!(recording.add(broken, 0.0).is_err());
        broken = at(31.0, 121.0);
        broken.accuracy = -1.0;
        assert!(recording.add(broken, 0.0).is_err());
        assert!(recording.add(at(31.0, 121.0), f64::NAN).is_err());
        assert!(recording.points().is_empty());
    }

    #[test]
    fn a_clock_that_goes_backwards_does_not_shrink_the_duration() {
        let mut recording = Recording::new();
        recording.add(at(31.0, 121.0), 10.0).unwrap();
        recording.add(at(31.0001, 121.0), 5.0).unwrap();
        assert!((recording.seconds() - 10.0).abs() < 1e-9);
        assert_eq!(recording.points().len(), 2);
    }

    #[test]
    fn the_state_reports_what_the_panel_needs() {
        let mut recording = Recording::new();
        recording.add(at(31.0, 121.0), 0.0).unwrap();
        recording.add(at(31.0, 121.0), 1.0).unwrap();
        let state = recording.state(1);
        assert_eq!(state.count, 1);
        assert_eq!(state.points.len(), 1);
        assert_eq!(state.skipped, 1);
        assert!(!state.full);
        assert!((state.seconds - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_recorded_track_can_be_played_back_directly() {
        // 录下来的点必须能原样交给回放：这是两个功能的接口约定。
        let mut recording = Recording::new();
        recording.add(at(31.0, 121.0), 0.0).unwrap();
        recording.add(at(31.001, 121.0), 5.0).unwrap();
        let route = crate::route::Route {
            points: recording.points().to_vec(),
            speed: 5.0,
            repeat_count: 1,
            repeat_delay: 0.0,
        };
        assert!(crate::route::Playback::new(route, std::time::Instant::now()).is_ok());
    }
}
