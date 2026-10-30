//! Persistent fixed-point combat geometry with deterministic generation.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Obstacle {
    pub min: Point,
    pub max: Point,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub width: u32,
    pub height: u32,
    pub obstacle_count: u32,
    pub max_obstacle_size: u32,
    pub movement: u32,
    pub attack_range: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Map {
    pub region: u64,
    pub seed: u64,
    pub version: u32,
    pub rules: Rules,
    pub obstacles: Vec<Obstacle>,
}
impl Rules {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.width < 4
            || self.height < 4
            || self.obstacle_count > 100_000
            || self.max_obstacle_size == 0
            || self.max_obstacle_size >= self.width.min(self.height)
            || self.movement == 0
            || self.attack_range == 0
        {
            return Err("invalid tactical rules");
        }
        Ok(())
    }
}
impl Map {
    /// Generate once from a versioned seed; persist the returned map thereafter.
    pub fn generate(region: u64, seed: u64, rules: Rules) -> Result<Self, &'static str> {
        rules.validate()?;
        let mut random = seed ^ region.rotate_left(17) ^ 0x73aa_33b9_7754_23c1;
        let mut obstacles = Vec::with_capacity(rules.obstacle_count as usize);
        for _ in 0..rules.obstacle_count {
            let size_x = 1 + next(&mut random) % rules.max_obstacle_size;
            let size_y = 1 + next(&mut random) % rules.max_obstacle_size;
            let x = next(&mut random) % (rules.width - size_x);
            let y = next(&mut random) % (rules.height - size_y);
            obstacles.push(Obstacle {
                min: Point { x, y },
                max: Point {
                    x: x + size_x,
                    y: y + size_y,
                },
            });
        }
        Ok(Self {
            region,
            seed,
            version: 1,
            rules,
            obstacles,
        })
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        self.rules.validate()?;
        if self.version != 1
            || self.obstacles.iter().any(|wall| {
                wall.min.x >= wall.max.x
                    || wall.min.y >= wall.max.y
                    || wall.max.x > self.rules.width
                    || wall.max.y > self.rules.height
            })
        {
            return Err("invalid tactical map");
        }
        Ok(())
    }
    pub fn open(&self, point: Point) -> bool {
        point.x <= self.rules.width
            && point.y <= self.rules.height
            && !self.obstacles.iter().any(|wall| contains(*wall, point))
    }
    pub fn visible(&self, from: Point, to: Point) -> bool {
        self.open(from)
            && self.open(to)
            && !self
                .obstacles
                .iter()
                .any(|wall| intersects(*wall, from, to))
    }
    pub fn can_move(&self, from: Point, to: Point) -> bool {
        distance2(from, to) <= u128::from(self.rules.movement).pow(2) && self.visible(from, to)
    }
    pub fn can_attack(&self, from: Point, to: Point) -> bool {
        distance2(from, to) <= u128::from(self.rules.attack_range).pow(2) && self.visible(from, to)
    }
    /// Validate independent movement intents in parallel against one map generation.
    pub fn batch_moves(&self, moves: &[(Point, Point)]) -> Vec<bool> {
        moves
            .par_iter()
            .map(|&(from, to)| self.can_move(from, to))
            .collect()
    }
}
fn next(state: &mut u64) -> u32 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((z ^ (z >> 31)) >> 32) as u32
}
fn distance2(a: Point, b: Point) -> u128 {
    let x = u128::from(a.x.abs_diff(b.x));
    let y = u128::from(a.y.abs_diff(b.y));
    x * x + y * y
}
fn contains(wall: Obstacle, p: Point) -> bool {
    p.x >= wall.min.x && p.x <= wall.max.x && p.y >= wall.min.y && p.y <= wall.max.y
}
fn cross(a: Point, b: Point, c: Point) -> i128 {
    let x1 = i128::from(b.x) - i128::from(a.x);
    let y1 = i128::from(b.y) - i128::from(a.y);
    let x2 = i128::from(c.x) - i128::from(a.x);
    let y2 = i128::from(c.y) - i128::from(a.y);
    x1 * y2 - y1 * x2
}
fn within(a: Point, b: Point, p: Point) -> bool {
    p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
}
fn segment(a: Point, b: Point, c: Point, d: Point) -> bool {
    let (ab_c, ab_d, cd_a, cd_b) = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    (ab_c == 0 && within(a, b, c))
        || (ab_d == 0 && within(a, b, d))
        || (cd_a == 0 && within(c, d, a))
        || (cd_b == 0 && within(c, d, b))
        || (((ab_c < 0 && ab_d > 0) || (ab_c > 0 && ab_d < 0))
            && ((cd_a < 0 && cd_b > 0) || (cd_a > 0 && cd_b < 0)))
}
fn intersects(wall: Obstacle, a: Point, b: Point) -> bool {
    let corners = [
        wall.min,
        Point {
            x: wall.max.x,
            y: wall.min.y,
        },
        wall.max,
        Point {
            x: wall.min.x,
            y: wall.max.y,
        },
    ];
    (0..4).any(|i| segment(a, b, corners[i], corners[(i + 1) % 4]))
}

#[cfg(test)]
#[path = "../tests/support/geometry.rs"]
mod tests;
