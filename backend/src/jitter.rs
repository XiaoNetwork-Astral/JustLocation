//! 拟人化的速度浮动。
//!
//! 复刻规格第 5.3 节的"速度浮动"分支：速度不是恒定值，而是每秒按随机倍率上下浮动，
//! 让轨迹看起来像真的在走路/开车，而不是匀速滑行。
//!
//! 原版的确切规则（规格里写明来自推进函数与其字节码）：
//!
//! - 设 `t` 为自开始以来的整数秒；每次从 `{40,50,60,180,200,240,300}` 里随机取一个阈值 `H`。
//! - 若 `H >= t mod 300`，倍率为 `min(0.8, max(0.4, U))`；否则为 `min(1.2, max(0.8, U + 0.2))`，
//!   其中 `U` 是本次 `Math.random()`。
//! - 速度与基础步频同时乘该倍率；倍率后的速度再形成该 tick 的距离预算。
//! - 进入目的点计算分支时，方位另加随机正负的 `U × 15°`，海拔为路线起点海拔加随机正负的 `U × 3m`。
//!
//! 这里把"取值规则"和"随机源"分开：`Jitter` 只负责按规则算出倍率与扰动，随机数由调用方
//! 传入（生产用系统随机源，测试用确定性序列）。这样上面这套古怪的分段规则可以被测试逐条钉住，
//! 而不必依赖统计性质去猜。
//!
//! 注意 `H >= t mod 300` 并不是"一半概率减速"：`t mod 300` 在 0..300 之间，
//! 而阈值最大只有 300，所以第 0 秒必定命中第一分支（取小倍率），随后随 t 增大逐渐偏向第二分支。

/// 原版用于判定倍率区间的阈值表。
const THRESHOLDS: [u32; 7] = [40, 50, 60, 180, 200, 240, 300];
/// 第一区间的倍率上限与下限。
const SLOW_MAX: f64 = 0.8;
const SLOW_MIN: f64 = 0.4;
/// 第二区间的倍率上限与下限。
const FAST_MAX: f64 = 1.2;
const FAST_MIN: f64 = 0.8;

#[derive(Clone, Debug)]
pub struct Jitter {
    enabled: bool,
    /// 已产生的倍率序列，按整数秒索引；`tick` 推进时追加。
    factors: Vec<f64>,
}

impl Jitter {
    pub fn new(enabled: bool) -> Self {
        Self { enabled, factors: Vec::new() }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// 当前 tick 的倍率。未启用时恒为 1，也就是原样速度。
    pub fn factor(&self) -> f64 {
        if !self.enabled {
            return 1.0;
        }
        self.factors.last().copied().unwrap_or(1.0)
    }

    /// 为一个新的整数秒产生倍率。`random` 每次调用返回 `[0,1)`。
    pub fn advance<F: FnMut() -> f64>(&mut self, seconds: u64, mut random: F) {
        if !self.enabled {
            return;
        }
        let threshold = THRESHOLDS[(random() * THRESHOLDS.len() as f64) as usize % THRESHOLDS.len()];
        let u = random().clamp(0.0, 1.0);
        let phase = (seconds % 300) as u32;
        let factor = if threshold >= phase {
            u.max(SLOW_MIN).min(SLOW_MAX)
        } else {
            (u + 0.2).max(FAST_MIN).min(FAST_MAX)
        };
        self.factors.push(factor);
    }

    /// 方位扰动：随机正负的 `U × 15°`。
    pub fn bearing_offset<F: FnMut() -> f64>(&self, mut random: F) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        let u = random().clamp(0.0, 1.0);
        if random() < 0.5 { -u * 15.0 } else { u * 15.0 }
    }

    /// 海拔扰动：随机正负的 `U × 3 米`。
    pub fn altitude_offset<F: FnMut() -> f64>(&self, mut random: F) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        let u = random().clamp(0.0, 1.0);
        if random() < 0.5 { -u * 3.0 } else { u * 3.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 依次返回给定值的随机源，方便把规则钉死。
    fn sequence(values: Vec<f64>) -> impl FnMut() -> f64 {
        let mut index = 0;
        move || {
            let value = values[index.min(values.len() - 1)];
            index += 1;
            value
        }
    }

    #[test]
    fn disabled_jitter_leaves_the_speed_alone() {
        let mut jitter = Jitter::new(false);
        jitter.advance(5, sequence(vec![0.0, 0.0]));
        assert_eq!(jitter.factor(), 1.0);
        assert_eq!(jitter.bearing_offset(sequence(vec![1.0, 0.0])), 0.0);
        assert_eq!(jitter.altitude_offset(sequence(vec![1.0, 0.0])), 0.0);
    }

    #[test]
    fn the_second_after_the_start_always_takes_the_slow_branch() {
        // t mod 300 == 0 时，任何阈值都 >= 0，所以必定落在第一分支。
        for threshold_pick in [0.0, 0.5, 0.99] {
            let mut jitter = Jitter::new(true);
            jitter.advance(0, sequence(vec![threshold_pick, 0.9]));
            // 第一分支上限 0.8：0.9 被夹到 0.8。
            assert_eq!(jitter.factor(), SLOW_MAX, "threshold pick {threshold_pick}");
        }
        let mut jitter = Jitter::new(true);
        jitter.advance(0, sequence(vec![0.0, 0.1]));
        // 下限 0.4：0.1 被抬到 0.4。
        assert_eq!(jitter.factor(), SLOW_MIN);
    }

    #[test]
    fn a_late_second_with_a_small_threshold_takes_the_fast_branch() {
        // 阈值取第 1 个（40），t=100 时 40 < 100，走第二分支：0.5+0.2=0.7 被抬到 0.8。
        let mut jitter = Jitter::new(true);
        jitter.advance(100, sequence(vec![0.0, 0.5]));
        assert_eq!(jitter.factor(), FAST_MIN);
        // 0.99+0.2=1.19 落在区间内。
        let mut jitter = Jitter::new(true);
        jitter.advance(100, sequence(vec![0.0, 0.99]));
        assert!((jitter.factor() - 1.19).abs() < 1e-9);
        // 1.0+0.2 被上限夹到 1.2。
        let mut jitter = Jitter::new(true);
        jitter.advance(100, sequence(vec![0.0, 1.0]));
        assert_eq!(jitter.factor(), FAST_MAX);
    }

    #[test]
    fn the_branch_flips_over_a_full_cycle() {
        // 同一个阈值，t 从 0 走到 300：前段命中第一分支（≤0.8），后段命中第二分支（≥0.8）。
        let early = {
            let mut jitter = Jitter::new(true);
            jitter.advance(10, sequence(vec![0.0, 0.95]));
            jitter.factor()
        };
        let late = {
            let mut jitter = Jitter::new(true);
            jitter.advance(200, sequence(vec![0.0, 0.5]));
            jitter.factor()
        };
        assert!(early <= SLOW_MAX);
        assert!(late >= FAST_MIN);
    }

    #[test]
    fn the_last_factor_is_reused_until_the_next_tick() {
        let mut jitter = Jitter::new(true);
        assert_eq!(jitter.factor(), 1.0, "还没有 tick 时按原速处理");
        jitter.advance(5, sequence(vec![0.0, 0.6]));
        let first = jitter.factor();
        assert_eq!(jitter.factor(), first, "同一个 tick 内读多次必须是同一个值");
        jitter.advance(6, sequence(vec![0.0, 0.99]));
        assert_ne!(jitter.factor(), first);
    }

    #[test]
    fn offsets_stay_inside_the_documented_ranges() {
        let jitter = Jitter::new(true);
        // 方位最大 ±15°，海拔最大 ±3 米，符号由第二个随机数决定。
        let negative = jitter.bearing_offset(sequence(vec![1.0, 0.0]));
        let positive = jitter.bearing_offset(sequence(vec![1.0, 0.9]));
        assert!((negative + 15.0).abs() < 1e-9);
        assert!((positive - 15.0).abs() < 1e-9);
        let low = jitter.altitude_offset(sequence(vec![1.0, 0.0]));
        let high = jitter.altitude_offset(sequence(vec![1.0, 0.9]));
        assert!((low + 3.0).abs() < 1e-9);
        assert!((high - 3.0).abs() < 1e-9);
    }
}
