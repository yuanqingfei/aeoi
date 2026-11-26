//! Compensated summation kernel: error-free transformations (Knuth TwoSum, Dekker TwoProduct with
//! Veltkamp splitting; no FMA, so results are bitwise identical on every platform, wasm32 included),
//! the Ogita–Rump–Oishi Dot2 accumulator, and the Neumaier sum. Shared by every operator that needs
//! sums or residuals beyond working precision (spec 11.5 RI-1).

// Error-free transformations (Knuth TwoSum, Dekker TwoProduct with Veltkamp splitting; no FMA so the
// result is bitwise identical on every platform, wasm32 included) and Ogita–Rump–Oishi Dot2.
/// Knuth TwoSum: a + b = s + e exactly.
#[inline]
pub fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

#[inline]
pub fn split(a: f64) -> (f64, f64) {
    let c = 134217729.0 * a; // 2^27 + 1
    let h = c - (c - a);
    (h, a - h)
}

#[inline]
pub fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    (p, al * bl - (((p - ah * bh) - al * bh) - ah * bl))
}

/// Accumulator for Σ xᵢ·yᵢ in roughly twice the working precision (Dot2). `value()` = hi + lo.
#[derive(Clone, Copy, Default)]
pub struct Dot2 {
    s: f64,
    c: f64,
}

impl Dot2 {
    #[inline]
    pub fn add_prod(&mut self, x: f64, y: f64) {
        let (p, pe) = two_prod(x, y);
        let (s, se) = two_sum(self.s, p);
        self.s = s;
        self.c += pe + se;
    }
    #[inline]
    pub fn add(&mut self, x: f64) {
        let (s, se) = two_sum(self.s, x);
        self.s = s;
        self.c += se;
    }
    #[inline]
    pub fn hi_lo(&self) -> (f64, f64) {
        two_sum(self.s, self.c)
    }
}

/// Neumaier (improved Kahan–Babuška) accumulator; `value()` = s + c.
#[derive(Clone, Copy, Default)]
pub struct Neumaier {
    s: f64,
    c: f64,
}

impl Neumaier {
    #[inline]
    pub fn add(&mut self, x: f64) {
        let t = self.s + x;
        self.c += if self.s.abs() >= x.abs() { (self.s - t) + x } else { (x - t) + self.s };
        self.s = t;
    }
    #[inline]
    pub fn value(&self) -> f64 {
        self.s + self.c
    }
}

/// Neumaier sum of a sequence.
pub fn neumaier(v: impl Iterator<Item = f64>) -> f64 {
    let mut a = Neumaier::default();
    for x in v {
        a.add(x);
    }
    a.value()
}
