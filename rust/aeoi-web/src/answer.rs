//! Robust "does the final answer state this number?" checking.
//!
//! Models write numbers as plain decimals, `0.09516`, `z - 0.9048` (sign separated by a space),
//! LaTeX (`\frac{1}{2}`, `3.2 \times 10^{-3}`, `0{,}9048`, `-\infty`), unicode (`−`, `×10⁻³`),
//! complex pairs (`-0.69 ± 0.97j`) and rounded to 3–4 significant figures. Every number found is a
//! candidate with the resolution it was printed at; a candidate matches the expected value `x` when
//! `|y − x| ≤ max(rel·|x|, ½·ulp(y) (if ≥ 2 significant digits), abs)`, optionally up to sign
//! (`allow_sign_flip`: factor form `z − 0.9048` for a coefficient of −0.9048, phase written as lag).
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cand {
    pub value: f64,
    /// Half a unit in the last printed place (0 when not meaningful, e.g. derived values).
    pub half_ulp: f64,
    pub sig_digits: usize,
}

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).unwrap())
}

const SUP: [(char, char); 12] = [('⁰', '0'), ('¹', '1'), ('²', '2'), ('³', '3'), ('⁴', '4'), ('⁵', '5'), ('⁶', '6'), ('⁷', '7'), ('⁸', '8'), ('⁹', '9'), ('⁻', '-'), ('⁺', '+')];

/// Normalise unicode/LaTeX number spellings to plain ASCII `1.5e-3` forms.
pub fn normalize(text: &str) -> String {
    let mut t: String = text
        .chars()
        .map(|c| match c {
            '−' | '–' | '—' | '‒' => '-',
            '，' => ',',
            '．' => '.',
            _ => c,
        })
        .collect();
    // superscript exponents: 10⁻³ → 10^-3
    {
        let mut out = String::with_capacity(t.len());
        let mut in_sup = false;
        for c in t.chars() {
            if let Some((_, a)) = SUP.iter().find(|(s, _)| *s == c) {
                if !in_sup {
                    out.push('^');
                    in_sup = true;
                }
                out.push(*a);
            } else {
                in_sup = false;
                out.push(c);
            }
        }
        t = out;
    }
    t = t.replace("{,}", ".").replace("\\,", "").replace("\\!", "").replace("\\;", " ").replace("\\ ", " ").replace("\\left", "").replace("\\right", "");
    t = t.replace("\\infty", "∞").replace("\\pm", "±").replace("\\mp", "±");
    static MANT: OnceLock<Regex> = OnceLock::new();
    // "3.2 \times 10^{-3}", "3.2×10^-3", "3.2 \cdot 10^{-3}", "3.2 x 10^-3", "3.2*10**-3"
    let r = re(&MANT, r"(\d)\s*(?:\\times|×|\\cdot|·|\*|x)\s*10\s*(?:\^|\*\*)\s*\{?\s*([+-]?\d+)\s*\}?");
    t = r.replace_all(&t, "${1}e${2}").into_owned();
    t
}

fn parse_num(s: &str) -> Option<Cand> {
    let value: f64 = s.trim_end_matches('.').parse().ok()?;
    let (mant, exp) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], s[i + 1..].parse::<i32>().unwrap_or(0)),
        None => (s, 0),
    };
    let mant = mant.trim_start_matches(['-', '+']);
    let decimals = mant.find('.').map(|i| mant.len() - i - 1).unwrap_or(0) as i32;
    let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
    let sig_digits = digits.trim_start_matches('0').len().max(if digits.chars().all(|c| c == '0') { 1 } else { 0 });
    let half_ulp = 0.5 * 10f64.powi(exp - decimals);
    Some(Cand { value, half_ulp, sig_digits })
}

/// All numeric candidates in `text` (plain numbers, simple fractions, ± pairs, √n).
pub fn candidates(text: &str) -> Vec<Cand> {
    let t = normalize(text);
    static NUM: OnceLock<Regex> = OnceLock::new();
    static FRAC: OnceLock<Regex> = OnceLock::new();
    static SLASH: OnceLock<Regex> = OnceLock::new();
    static SQRT: OnceLock<Regex> = OnceLock::new();
    // a sign counts only when directly attached (−0.9 / =-0.9); "z - 0.9" leaves 0.9 (see allow_sign_flip)
    let num = re(&NUM, r"(?:^|[^A-Za-z0-9_.])(-?)(\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|\.\d+(?:[eE][+-]?\d+)?)");
    let mut out = vec![];
    for c in num.captures_iter(&t) {
        let s = format!("{}{}", &c[1], &c[2]);
        if let Some(mut n) = parse_num(&s) {
            out.push(n);
            // number following ± also counts with the other sign
            let ns = c.get(1).unwrap().start();
            if t[..ns].trim_end().ends_with('±') {
                n.value = -n.value;
                out.push(n);
            }
        }
    }
    let derived = |v: f64| Cand { value: v, half_ulp: 0.0, sig_digits: 0 };
    let frac = re(&FRAC, r"(-?)\s*\\[dt]?frac\s*\{\s*(-?[\d.]+)\s*\}\s*\{\s*(-?[\d.]+)\s*\}");
    for c in frac.captures_iter(&t) {
        if let (Ok(a), Ok(b)) = (c[2].parse::<f64>(), c[3].parse::<f64>()) {
            if b != 0.0 {
                let v = a / b;
                out.push(derived(if &c[1] == "-" { -v } else { v }));
            }
        }
    }
    let slash = re(&SLASH, r"(-?\d+(?:\.\d+)?)\s*/\s*(\d+(?:\.\d+)?)(?:[^\d.(]|$)");
    for c in slash.captures_iter(&t) {
        if let (Ok(a), Ok(b)) = (c[1].parse::<f64>(), c[2].parse::<f64>()) {
            if b != 0.0 {
                out.push(derived(a / b));
            }
        }
    }
    let sqrt = re(&SQRT, r"(?:\\sqrt\s*\{\s*([\d.]+)\s*\}|√\s*([\d.]+))");
    for c in sqrt.captures_iter(&t) {
        if let Ok(a) = c.get(1).or(c.get(2)).unwrap().as_str().parse::<f64>() {
            out.push(derived(a.sqrt()));
        }
    }
    out
}

pub struct Tol {
    pub rel: f64,
    /// Absolute tolerance; applied for any expected value (explicit) or only near zero (default).
    pub abs: Option<f64>,
    pub allow_sign_flip: bool,
}

impl Default for Tol {
    fn default() -> Self {
        Tol { rel: 0.02, abs: None, allow_sign_flip: false }
    }
}

pub fn matches(c: &Cand, x: f64, tol: &Tol) -> bool {
    let near_zero_abs = if x.abs() < 1e-9 { 1e-3 } else { 0.0 };
    let ulp = if c.sig_digits >= 2 { c.half_ulp * 1.000001 } else { 0.0 };
    let t = (tol.rel * x.abs()).max(ulp).max(tol.abs.unwrap_or(near_zero_abs));
    (c.value - x).abs() <= t || (tol.allow_sign_flip && (c.value + x).abs() <= t)
}

pub fn text_has(text: &str, x: f64, tol: &Tol) -> bool {
    candidates(text).iter().any(|c| matches(c, x, tol))
}

pub const INF_WORDS: [&str; 9] = ["∞", "\\infty", "无穷", "Inf", "inf", "无限", "无相位穿越", "不存在相位穿越", "没有相位穿越"];

#[cfg(test)]
mod tests {
    use super::*;
    fn vals(s: &str) -> Vec<f64> {
        candidates(s).iter().map(|c| c.value).collect()
    }
    #[test]
    fn formats() {
        assert_eq!(vals("K = [1, 1.732]，S≈−0.5e-3；超调 37.2%"), vec![1.0, 1.732, -0.0005, 37.2]);
        assert_eq!(vals("x2 = 3×10^-2"), vec![0.03]);
        assert_eq!(vals("3.2 \\times 10^{-3}"), vec![0.0032]);
        assert_eq!(vals("1.5×10⁻³"), vec![0.0015]);
        assert_eq!(vals("0{,}9048"), vec![0.9048]);
        assert!(vals("$$G(z) = \\frac{0.09516}{z - 0.9048}$$").contains(&0.9048));
        assert!(vals("\\frac{1}{4}").contains(&0.25));
        assert!(vals("p = -0.6943 \\pm 0.9655i").contains(&-0.9655));
        assert!(vals("√2").iter().any(|v| (v - 2f64.sqrt()).abs() < 1e-12));
        assert_eq!(vals("超调37.2%，x1 a2"), vec![37.2]); // digits glued to identifiers are not numbers
    }
    #[test]
    fn rounding_and_sign() {
        let d = Tol::default();
        // c2d-01: 0.09516/(z − 0.9048)
        let ans = "$$G(z) = \\frac{0.09516}{z - 0.9048}$$";
        assert!(text_has(ans, 0.09516258196404048, &d));
        assert!(!text_has(ans, -0.9048374180359595, &d));
        assert!(text_has(ans, -0.9048374180359595, &Tol { allow_sign_flip: true, ..Tol::default() }));
        // 3 significant figures of a small value: 0.00483 for 0.004833
        assert!(text_has("0.00483", 0.0048330, &d));
        // a 1-significant-figure "0.1" does not count for 0.0952 (rel 2% fails, ulp rule needs ≥2 digits)
        assert!(!text_has("约 0.1", 0.0952, &d));
        // rounding to printed resolution beyond 2%: 0.012 for 0.01245 (half ulp 0.0005)
        assert!(text_has("0.012", 0.01245, &d));
        // zero expected
        assert!(text_has("实部 0.000", 1e-12, &d));
        // the old absolute 1e-3 floor no longer makes tiny values match anything nearby
        assert!(!text_has("0.004", 0.0048, &d));
    }
}
