use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Map {
    pub width: i64,
    pub obstacles: Vec<(Point, i64)>,
}
pub fn distance2(a: Point, b: Point) -> i128 {
    let x = i128::from(a.x) - i128::from(b.x);
    let y = i128::from(a.y) - i128::from(b.y);
    x * x + y * y
}
impl Map {
    pub fn valid(&self) -> bool {
        (1..=1_000_000).contains(&self.width)
            && self.obstacles.len() <= 256
            && self.obstacles.iter().all(|(p, r)| {
                p.x >= 0
                    && p.y >= 0
                    && p.x <= self.width
                    && p.y <= self.width
                    && *r > 0
                    && *r <= self.width
            })
    }
    pub fn generate(seed: u64, width: i64, count: usize) -> Result<Self> {
        require(
            (100..=1_000_000).contains(&width) && count <= 256,
            "invalid map bounds",
        )?;
        let span = width as u64;
        let obstacles = (0..count)
            .map(|i| {
                (
                    Point {
                        x: (random(seed, i as u64, 0) % span) as i64,
                        y: (random(seed, i as u64, 1) % span) as i64,
                    },
                    width / 40,
                )
            })
            .collect();
        Ok(Self { width, obstacles })
    }
    pub fn contains(&self, p: Point) -> bool {
        self.valid()
            && p.x >= 0
            && p.y >= 0
            && p.x <= self.width
            && p.y <= self.width
            && self
                .obstacles
                .iter()
                .all(|(center, r)| distance2(p, *center) > i128::from(*r).pow(2))
    }
    pub fn clear(&self, a: Point, b: Point) -> bool {
        if !self.contains(a) || !self.contains(b) {
            return false;
        }
        let dx = i128::from(b.x) - i128::from(a.x);
        let dy = i128::from(b.y) - i128::from(a.y);
        let len = dx * dx + dy * dy;
        self.obstacles.iter().all(|(c, r)| {
            if len == 0 {
                return distance2(a, *c) > i128::from(*r).pow(2);
            }
            let cx = i128::from(c.x) - i128::from(a.x);
            let cy = i128::from(c.y) - i128::from(a.y);
            let dot = (cx * dx + cy * dy).clamp(0, len);
            let x = cx * len - dot * dx;
            let y = cy * len - dot * dy;
            x * x + y * y > i128::from(*r).pow(2) * len * len
        })
    }
}
