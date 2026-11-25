//! interp.grid.interp1 — 1-D interpolation of tabulated data (spec 10.20, v0.4.10).
//!
//! Methods: linear (numpy.interp arithmetic), nearest (ties → right neighbour, scipy "nearest-up": choose
//! x[i+1] iff xq ≥ x[i]/2 + x[i+1]/2), previous, next, pchip (Fritsch–Butland slopes, scipy
//! PchipInterpolator = MATLAB pchip), spline (cubic, not-a-knot ends = MATLAB spline / scipy CubicSpline).
//! Cubic pieces are evaluated like scipy PPoly: c = [t/h, (m − s_i)/h − t, s_i, y_i], t = (s_i + s_{i+1} − 2m)/h,
//! p = ((c0·d + c1)·d + c2)·d + c3, d = xq − x_i. A query equal to a node returns that node's y exactly.
//! Outside [x_0, x_{n−1}]: extrapolation = error (default, author's decision 2026-11-25) | nan | clamp |
//! extrapolate (linear/cubic: the end piece continued; nearest/previous/next: the end value).
use aeoi_core::error::{modify_input, OpError, Warning};
use aeoi_core::matrix::{parse_array, Array, Elems};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(Interp1, "interp.grid.interp1");

const U: f64 = f64::EPSILON / 2.0;

#[derive(Clone, Copy, PartialEq)]
enum Method {
    Linear,
    Nearest,
    Previous,
    Next,
    Pchip,
    Spline,
}
impl Method {
    fn name(self) -> &'static str {
        match self {
            Method::Linear => "linear",
            Method::Nearest => "nearest",
            Method::Previous => "previous",
            Method::Next => "next",
            Method::Pchip => "pchip",
            Method::Spline => "spline",
        }
    }
    fn cubic(self) -> bool {
        matches!(self, Method::Pchip | Method::Spline)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Extrap {
    Error,
    Nan,
    Clamp,
    Extrapolate,
}
impl Extrap {
    fn name(self) -> &'static str {
        match self {
            Extrap::Error => "error",
            Extrap::Nan => "nan",
            Extrap::Clamp => "clamp",
            Extrap::Extrapolate => "extrapolate",
        }
    }
}

pub struct In {
    x: Vec<f64>,
    ys: Vec<Vec<f64>>,
    y_1d: bool,
    xq: Vec<f64>,
    method: Method,
    extrap: Extrap,
    n_out: usize,
}

pub(crate) fn vec1(v: &Value, path: &str, host: &HostCtx) -> Result<(Vec<f64>, Vec<usize>), OpError> {
    match parse_array(v, path, host, false)? {
        Array::Dense(d) => {
            let data = match &d.data {
                Elems::F64(x) => x.clone(),
                Elems::I64(x) => x.iter().map(|&t| t as f64).collect(),
                _ => return Err(OpError::new("E.VALIDATION.DTYPE", format!("{path} 需要实数（f64 或 i64）。"), "把它转成 f64。").path(&format!("{path}/dtype"))),
            };
            Ok((data, d.shape.clone()))
        }
        Array::Sparse(_) => Err(OpError::new("E.VALIDATION.SHAPE", format!("{path} 不接受稀疏编码。"), "用稠密向量。").path(path)),
    }
}

/// LAPACK dgtsv (tridiagonal, partial pivoting), single right-hand side. dl, d, du are overwritten.
fn gtsv(mut dl: Vec<f64>, mut d: Vec<f64>, mut du: Vec<f64>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = d.len();
    if n == 1 {
        return if d[0] == 0.0 { None } else { Some(vec![b[0] / d[0]]) };
    }
    for i in 0..n - 1 {
        if d[i].abs() >= dl[i].abs() {
            if d[i] == 0.0 {
                return None;
            }
            let fact = dl[i] / d[i];
            d[i + 1] -= fact * du[i];
            b[i + 1] -= fact * b[i];
            dl[i] = 0.0;
        } else {
            let fact = d[i] / dl[i];
            d[i] = dl[i];
            let temp = d[i + 1];
            d[i + 1] = du[i] - fact * temp;
            if i < n - 2 {
                dl[i] = du[i + 1];
                du[i + 1] = -fact * dl[i];
            }
            du[i] = temp;
            let tb = b[i];
            b[i] = b[i + 1];
            b[i + 1] = tb - fact * b[i + 1];
        }
    }
    if d[n - 1] == 0.0 {
        return None;
    }
    // back substitution; after an interchange dl[i] holds the second superdiagonal (dgtsv)
    b[n - 1] /= d[n - 1];
    if n > 1 {
        b[n - 2] = (b[n - 2] - du[n - 2] * b[n - 1]) / d[n - 2];
    }
    for i in (0..n.saturating_sub(2)).rev() {
        b[i] = (b[i] - du[i] * b[i + 1] - dl[i] * b[i + 2]) / d[i];
    }
    Some(b)
}

/// Slopes for pchip (scipy PchipInterpolator._find_derivatives).
fn pchip_slopes(x: &[f64], y: &[f64]) -> Vec<f64> {
    let n = x.len();
    let h: Vec<f64> = (0..n - 1).map(|i| x[i + 1] - x[i]).collect();
    let m: Vec<f64> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / h[i]).collect();
    if n == 2 {
        return vec![m[0], m[0]];
    }
    let sign = |v: f64| if v > 0.0 { 1.0 } else if v < 0.0 { -1.0 } else { 0.0 };
    let mut d = vec![0.0; n];
    for k in 1..n - 1 {
        let cond = sign(m[k]) != sign(m[k - 1]) || m[k] == 0.0 || m[k - 1] == 0.0;
        if !cond {
            let w1 = 2.0 * h[k] + h[k - 1];
            let w2 = h[k] + 2.0 * h[k - 1];
            let whmean = (w1 / m[k - 1] + w2 / m[k]) / (w1 + w2);
            d[k] = 1.0 / whmean;
        }
    }
    let edge = |h0: f64, h1: f64, m0: f64, m1: f64| {
        let dd = ((2.0 * h0 + h1) * m0 - h0 * m1) / (h0 + h1);
        if sign(dd) != sign(m0) {
            0.0
        } else if sign(m0) != sign(m1) && dd.abs() > 3.0 * m0.abs() {
            3.0 * m0
        } else {
            dd
        }
    };
    d[0] = edge(h[0], h[1], m[0], m[1]);
    d[n - 1] = edge(h[n - 2], h[n - 3], m[n - 2], m[n - 3]);
    d
}

/// Slopes of the not-a-knot cubic spline (scipy CubicSpline, bc_type = not-a-knot).
fn spline_slopes(x: &[f64], y: &[f64]) -> Option<Vec<f64>> {
    let n = x.len();
    let dx: Vec<f64> = (0..n - 1).map(|i| x[i + 1] - x[i]).collect();
    let slope: Vec<f64> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / dx[i]).collect();
    if n == 2 {
        return Some(vec![slope[0], slope[0]]);
    }
    if n == 3 {
        // the parabola through the three points: 3×3 system solved by LU with partial pivoting
        let a = [1.0, 1.0, 0.0, dx[1], 2.0 * (dx[0] + dx[1]), dx[0], 0.0, 1.0, 1.0];
        let b = [2.0 * slope[0], 3.0 * (dx[0] * slope[1] + dx[1] * slope[0]), 2.0 * slope[1]];
        let lu = crate::linsolve::lu_factor(&a, 3);
        if lu.zero_pivot.is_some() {
            return None;
        }
        return Some(lu.solve(&b));
    }
    let mut diag = vec![0.0; n];
    let mut upper = vec![0.0; n - 1]; // A[i, i+1]
    let mut lower = vec![0.0; n - 1]; // A[i+1, i]
    let mut b = vec![0.0; n];
    for i in 1..n - 1 {
        diag[i] = 2.0 * (dx[i - 1] + dx[i]);
        upper[i] = dx[i - 1];
        lower[i - 1] = dx[i];
        b[i] = 3.0 * (dx[i] * slope[i - 1] + dx[i - 1] * slope[i]);
    }
    // not-a-knot start
    diag[0] = dx[1];
    let d0 = x[2] - x[0];
    upper[0] = d0;
    b[0] = ((dx[0] + 2.0 * d0) * dx[1] * slope[0] + dx[0] * dx[0] * slope[1]) / d0;
    // not-a-knot end
    diag[n - 1] = dx[n - 3];
    let d1 = x[n - 1] - x[n - 3];
    lower[n - 2] = d1;
    b[n - 1] = (dx[n - 2] * dx[n - 2] * slope[n - 3] + (2.0 * d1 + dx[n - 2]) * dx[n - 3] * slope[n - 2]) / d1;
    gtsv(lower, diag, upper, b)
}

/// Cubic Hermite coefficients of piece i (scipy CubicHermiteSpline / PPoly order).
fn coeffs(x: &[f64], y: &[f64], s: &[f64], i: usize) -> [f64; 4] {
    let h = x[i + 1] - x[i];
    let m = (y[i + 1] - y[i]) / h;
    let t = (s[i] + s[i + 1] - 2.0 * m) / h;
    [t / h, (m - s[i]) / h - t, s[i], y[i]]
}
fn horner(c: &[f64; 4], d: f64) -> f64 {
    ((c[0] * d + c[1]) * d + c[2]) * d + c[3]
}

/// Index i with x[i] ≤ v < x[i+1], clamped to [0, n−2].
fn interval(x: &[f64], v: f64) -> usize {
    let n = x.len();
    let i = x.partition_point(|&t| t <= v);
    i.saturating_sub(1).min(n - 2)
}

fn eval_one(x: &[f64], y: &[f64], s: Option<&[f64]>, method: Method, extrap: Extrap, v: f64) -> f64 {
    let n = x.len();
    let (lo, hi) = (x[0], x[n - 1]);
    let outside = v < lo || v > hi;
    if outside {
        match extrap {
            Extrap::Nan | Extrap::Error => return f64::NAN,
            Extrap::Clamp => return if v < lo { y[0] } else { y[n - 1] },
            Extrap::Extrapolate => {
                if matches!(method, Method::Nearest | Method::Previous | Method::Next) {
                    return if v < lo { y[0] } else { y[n - 1] };
                }
            }
        }
    }
    // exact node → its value
    if !outside {
        if let Ok(j) = x.binary_search_by(|t| t.partial_cmp(&v).unwrap()) {
            return y[j];
        }
    }
    let i = interval(x, v);
    match method {
        Method::Linear => {
            let slope = (y[i + 1] - y[i]) / (x[i + 1] - x[i]);
            slope * (v - x[i]) + y[i]
        }
        Method::Nearest => {
            let bound = x[i] / 2.0 + x[i + 1] / 2.0;
            if v >= bound {
                y[i + 1]
            } else {
                y[i]
            }
        }
        Method::Previous => y[i],
        Method::Next => y[i + 1],
        Method::Pchip | Method::Spline => horner(&coeffs(x, y, s.unwrap(), i), v - x[i]),
    }
}

impl Operator for Interp1 {
    type Input = In;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(In, Vec<Warning>), OpError> {
        let (x, xs) = vec1(&args["x"], "/x", host)?;
        if xs.len() != 1 {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("x 的 shape 是 {xs:?}；采样点要是一维 [n]。"), "把 x 写成 shape [n]。").path("/x/shape"));
        }
        let n = x.len();
        if n < 2 {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("x 只有 {n} 个点；插值至少要 2 个。"), "至少给出两个采样点。").path("/x"));
        }
        if let Some(j) = (0..n - 1).find(|&j| x[j + 1] <= x[j]) {
            let decreasing = (0..n - 1).all(|t| x[t + 1] < x[t]);
            let hint = if decreasing { "x 是严格递减的：把 x 与 y（沿采样那一维）一起倒序后再调用。" } else { "x 必须严格递增：排序（y 跟着同样重排），并合并重复的 x。" };
            return Err(OpError::new("E.VALIDATION.GRID", format!("x 不是严格递增：x[{}] = {} ≥ x[{}] = {}。", j, x[j], j + 1, x[j + 1]), hint).path(&format!("/x/data/{}", j + 1)).detail("index", json!(j + 1)).detail("decreasing", json!(decreasing)));
        }
        let (yv, ysh) = vec1(&args["y"], "/y", host)?;
        let (ys, y_1d): (Vec<Vec<f64>>, bool) = match ysh.as_slice() {
            [len] if *len == n => (vec![yv], true),
            [k, len] if *len == n && *k >= 1 => ((0..*k).map(|r| yv[r * n..(r + 1) * n].to_vec()).collect(), false),
            _ => {
                return Err(OpError::new("E.VALIDATION.SHAPE", format!("y 的 shape 是 {ysh:?}；要求 [n] 或 [k, n]（n = len(x) = {n}，每行一个通道）。"), "MATLAB 习惯按列存通道；这里按行存：[k, n]。若 y 是 [n, k]，先转置。").path("/y/shape").detail("n", json!(n)));
            }
        };
        let (xq, qsh) = vec1(&args["xq"], "/xq", host)?;
        if qsh.len() != 1 {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("xq 的 shape 是 {qsh:?}；查询点要是一维 [q]。"), "把 xq 展成 shape [q]。").path("/xq/shape"));
        }
        let method = match args.get("method").and_then(|v| v.as_str()).unwrap_or("linear") {
            "nearest" => Method::Nearest,
            "previous" => Method::Previous,
            "next" => Method::Next,
            "pchip" => Method::Pchip,
            "spline" => Method::Spline,
            _ => Method::Linear,
        };
        let extrap = match args.get("extrapolation").and_then(|v| v.as_str()).unwrap_or("error") {
            "nan" => Extrap::Nan,
            "clamp" => Extrap::Clamp,
            "extrapolate" => Extrap::Extrapolate,
            _ => Extrap::Error,
        };
        let lim = self.meta.manifest["limits"]["max_output_values"].as_u64().unwrap() as usize;
        if ys.len().saturating_mul(xq.len()) > lim {
            return Err(OpError::new("E.RESOURCE.OUTPUT_TOO_LARGE", format!("输出 {} × {} 个值超过上限 {lim}。", ys.len(), xq.len()), "分批查询。").path("/xq"));
        }
        let (lo, hi) = (x[0], x[n - 1]);
        let outside: Vec<usize> = (0..xq.len()).filter(|&t| xq[t] < lo || xq[t] > hi).collect();
        if extrap == Extrap::Error && !outside.is_empty() {
            let first = outside[0];
            let (qmin, qmax) = xq.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
            let patch = |v: &str| json!([{"op": "add", "path": "/extrapolation", "value": v}]);
            return Err(OpError::new("E.DOMAIN.EXTRAPOLATION_REQUIRED", format!("{} 个查询点在数据范围 [{lo}, {hi}] 之外（第一个是 xq[{first}] = {}；xq 范围 [{qmin}, {qmax}]）。插值只在数据范围内有依据；范围外要不要给值、怎么给，需要调用方决定。", outside.len(), xq[first]), "问用户范围外要哪种处理，再加上 extrapolation：nan（标成缺失）、clamp（取端点值）或 extrapolate（把端点那一段延长）。")
                .path(&format!("/xq/data/{first}"))
                .detail("count", json!(outside.len()))
                .detail("x_range", json!([num(lo), num(hi)]))
                .detail("xq_range", json!([num(qmin), num(qmax)]))
                .remedy(modify_input("extrapolation_nan", "范围外给 NaN（标成缺失）", "medium", true, patch("nan")))
                .remedy(modify_input("extrapolation_clamp", "范围外取最近端点的值", "medium", true, patch("clamp")))
                .remedy(modify_input("extrapolation_extrapolate", "把端点那一段（直线或三次多项式）延长", "low", true, patch("extrapolate"))));
        }
        Ok((In { x, ys, y_1d, xq, method, extrap, n_out: outside.len() }, Vec::new()))
    }

    fn execute(&self, i: In, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.x.len();
        let q = i.xq.len();
        let mut out = Vec::with_capacity(i.ys.len() * q);
        let (mut node_err, mut c2_err, mut shape_viol) = (0.0f64, 0.0f64, 0usize);
        let mut warnings = Vec::new();
        for y in &i.ys {
            let s: Option<Vec<f64>> = match i.method {
                Method::Pchip => Some(pchip_slopes(&i.x, y)),
                Method::Spline => Some(spline_slopes(&i.x, y).ok_or_else(|| OpError::new("E.NUMERICAL.SINGULAR", "三次样条的斜率方程组奇异（采样点间距异常）。", "检查 x 是否有极近的重复点；或改用 pchip/linear。").path("/x"))?),
                _ => None,
            };
            if let Some(s) = &s {
                // each piece reproduces its right end (left end is exact by construction)
                for k in 0..n - 1 {
                    let c = coeffs(&i.x, y, s, k);
                    let h = i.x[k + 1] - i.x[k];
                    let scale = y[k].abs() + y[k + 1].abs() + (s[k].abs() + s[k + 1].abs()) * h;
                    let e = (horner(&c, h) - y[k + 1]).abs() / scale.max(f64::MIN_POSITIVE);
                    node_err = node_err.max(e);
                }
                if i.method == Method::Spline && n >= 3 {
                    // C² at interior knots: p''_{k-1}(h_{k-1}) = 6c0·h + 2c1 vs p''_k(0) = 2c1, relative to a
                    // global scale (largest second-derivative term, plus max|y|/max(h)²) so that data with a
                    // vanishing second derivative (a straight line) is not measured against rounding noise.
                    let hmax = (0..n - 1).map(|k| i.x[k + 1] - i.x[k]).fold(0.0, f64::max);
                    let ymax = y.iter().fold(0.0f64, |m, v| m.max(v.abs()));
                    let mut g = ymax / (hmax * hmax);
                    let mut diffs = Vec::with_capacity(n - 2);
                    for k in 1..n - 1 {
                        let a = coeffs(&i.x, y, s, k - 1);
                        let b = coeffs(&i.x, y, s, k);
                        let h = i.x[k] - i.x[k - 1];
                        let lhs = 6.0 * a[0] * h + 2.0 * a[1];
                        let rhs = 2.0 * b[1];
                        g = g.max((6.0 * a[0] * h).abs() + (2.0 * a[1]).abs()).max(rhs.abs());
                        diffs.push((lhs - rhs).abs());
                    }
                    for d in diffs {
                        c2_err = c2_err.max(d / g.max(f64::MIN_POSITIVE));
                    }
                }
            }
            for &v in &i.xq {
                let r = eval_one(&i.x, y, s.as_deref(), i.method, i.extrap, v);
                if i.method != Method::Spline && v >= i.x[0] && v <= i.x[n - 1] {
                    let k = interval(&i.x, v);
                    let (a, b) = (y[k].min(y[k + 1]), y[k].max(y[k + 1]));
                    let slack = 4.0 * U * (a.abs().max(b.abs()));
                    if r < a - slack || r > b + slack {
                        shape_viol += 1;
                    }
                }
                out.push(r);
            }
            ctx.check()?;
        }
        let tol = 64.0 * U;
        let finite = out.iter().zip(i.xq.iter().cycle()).all(|(r, &v)| r.is_finite() || (i.extrap == Extrap::Nan && (v < i.x[0] || v > i.x[n - 1])));
        // Every post-condition is reported for every method (spec 10.20 IP-6): pieces_interpolate and
        // spline_c2 hold trivially where the method has no cubic pieces / is not a spline; local_bounds
        // (value within the interval's end values) is a property of every method except spline.
        let checks = json!({
            "all_finite": finite,
            "pieces_interpolate": node_err <= tol,
            "spline_c2": c2_err <= 1e3 * tol,
            "local_bounds": shape_viol == 0,
        });
        if i.n_out > 0 && i.extrap == Extrap::Extrapolate {
            warnings.push(Warning::new("W.ACCURACY.EXTRAPOLATED", format!("{} 个查询点在数据范围外，按 {} 的端点那一段外推；外推值没有数据支持，误差无界。", i.n_out, i.method.name())).path("/xq"));
        }
        let shape: Vec<usize> = if i.y_1d { vec![q] } else { vec![i.ys.len(), q] };
        let result = json!({
            "yq": {"dtype": "f64", "shape": shape, "data": out.iter().map(|v| num(*v)).collect::<Vec<_>>()},
            "method": i.method.name(),
            "extrapolation": i.extrap.name(),
            "out_of_range_count": i.n_out,
        });
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": if i.method.cubic() { json!({"piece_end_rel": num(node_err), "spline_c2_rel": if i.method == Method::Spline { num(c2_err) } else { Value::Null }, "definition": "每段三次多项式在右端点处与数据之差 / (|y_k| + |y_{k+1}| + (|s_k| + |s_{k+1}|)·h)；样条：内部节点两侧二阶导之差 / 全局尺度（最大的二阶导项与 max|y|/max(h)² 中的较大者）"}) } else { Value::Null },
            "condition": {"n": n, "min_spacing": num((0..n - 1).map(|k| i.x[k + 1] - i.x[k]).fold(f64::INFINITY, f64::min))},
            "error_estimate": {"kind": "rounding_only", "note": "只含舍入误差（相对量级 ~10u）；插值本身相对未知函数的误差取决于数据，算子无法估计"},
            "checks": checks,
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"local_bounds_violations": shape_viol, "local_bounds_applies": i.method != Method::Spline}
        });
        Ok(Output { result, trust, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;

    fn call(args: Value) -> Value {
        Interp1::new().call(&args, &HostCtx::default())
    }
    fn v(d: &[f64]) -> Value {
        json!({"dtype": "f64", "shape": [d.len()], "data": d})
    }
    fn yq(env: &Value) -> Vec<f64> {
        env["result"]["yq"]["data"].as_array().unwrap().iter().map(|t| t.as_f64().unwrap_or(f64::NAN)).collect()
    }

    #[test]
    fn linear_and_nodes() {
        let env = call(json!({"x": v(&[0.0, 1.0, 3.0]), "y": v(&[0.0, 2.0, 6.0]), "xq": v(&[0.5, 1.0, 2.0, 3.0])}));
        assert_eq!(env["status"], "ok", "{}", env["error"]);
        assert_eq!(yq(&env), vec![1.0, 2.0, 4.0, 6.0]);
    }

    #[test]
    fn spline_reproduces_cubic() {
        let x: Vec<f64> = (0..7).map(|t| t as f64 * 0.5).collect();
        let y: Vec<f64> = x.iter().map(|t| t * t * t - 2.0 * t + 1.0).collect();
        let q = [0.1, 0.77, 1.3, 2.9];
        let env = call(json!({"x": v(&x), "y": v(&y), "xq": v(&q), "method": "spline"}));
        assert_eq!(env["status"], "ok", "{}", env["error"]);
        for (got, t) in yq(&env).iter().zip(q) {
            let want = t * t * t - 2.0 * t + 1.0;
            assert!((got - want).abs() < 1e-13, "{t}: {got} vs {want}");
        }
        assert_eq!(env["trust"]["checks"]["spline_c2"], true);
    }

    #[test]
    fn extrapolation_rules() {
        let base = json!({"x": v(&[0.0, 1.0]), "y": v(&[0.0, 1.0]), "xq": v(&[-1.0, 0.5, 2.0])});
        let e = call(base.clone());
        assert_eq!(e["error"]["code"], "E.DOMAIN.EXTRAPOLATION_REQUIRED");
        assert_eq!(e["error"]["remediation"].as_array().unwrap().len(), 3);
        let mut a = base.clone();
        a["extrapolation"] = json!("clamp");
        assert_eq!(yq(&call(a)), vec![0.0, 0.5, 1.0]);
        let mut a = base.clone();
        a["extrapolation"] = json!("extrapolate");
        let env = call(a);
        assert_eq!(yq(&env), vec![-1.0, 0.5, 2.0]);
        assert!(env["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "W.ACCURACY.EXTRAPOLATED"));
        let mut a = base;
        a["extrapolation"] = json!("nan");
        let r = yq(&call(a));
        assert!(r[0].is_nan() && r[2].is_nan() && r[1] == 0.5);
    }

    #[test]
    fn nearest_ties_go_right_and_grid_errors() {
        let env = call(json!({"x": v(&[0.0, 1.0, 2.0]), "y": v(&[10.0, 20.0, 30.0]), "xq": v(&[0.5, 1.49, 1.5]), "method": "nearest"}));
        assert_eq!(yq(&env), vec![20.0, 20.0, 30.0]);
        let e = call(json!({"x": v(&[0.0, 1.0, 1.0]), "y": v(&[1.0, 2.0, 3.0]), "xq": v(&[0.5])}));
        assert_eq!(e["error"]["code"], "E.VALIDATION.GRID");
    }

    #[test]
    fn pchip_is_monotone() {
        let x = [0.0, 1.0, 2.0, 3.0, 4.0];
        let y = [0.0, 0.0, 1.0, 1.0, 5.0];
        let q: Vec<f64> = (0..=40).map(|t| t as f64 * 0.1).collect();
        let env = call(json!({"x": v(&x), "y": v(&y), "xq": v(&q), "method": "pchip"}));
        assert_eq!(env["trust"]["checks"]["local_bounds"], true);
        let r = yq(&env);
        assert!(r.windows(2).all(|w| w[1] >= w[0] - 1e-15));
    }
}
