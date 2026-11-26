//! Optimisation operators (spec 10.28–10.30, v0.4.14): optim.linear.linprog (simplex kernel),
//! optim.local.fmincon (SQP on the QP kernel, gradients by automatic differentiation of 3.15 expressions),
//! optim.local.fminsearch (Nelder–Mead, Lagarias et al. = MATLAB fminsearch).
//! Author's decisions (2026-11-25): fmincon takes bounds, linear and nonlinear constraints; every result is a
//! *local* optimum (stated in the receipt) checked by KKT residuals as post-conditions; not converging is an
//! error with the best point (as fsolve); linprog is a simplex method giving a vertex and the multipliers.
use crate::expr::{check_name, eval, eval_grad, expr_err, parse, parse_params, Node};
use aeoi_core::error::{ask_user, OpError, Warning};
use aeoi_core::kernels::{lp, qp};
use aeoi_core::{num, ExecCtx, HostCtx, Operator, Output};
use serde_json::{json, Value};

operator_struct!(Linprog, "optim.linear.linprog");
operator_struct!(Fmincon, "optim.local.fmincon");
operator_struct!(Fminsearch, "optim.local.fminsearch");

fn fnum(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if s == "Inf" => Some(f64::INFINITY),
        Value::String(s) if s == "-Inf" => Some(f64::NEG_INFINITY),
        _ => None,
    }
}
pub(crate) fn vecf(v: &Value, path: &str, n: Option<usize>) -> Result<Vec<f64>, OpError> {
    let a = v.as_array().ok_or_else(|| OpError::new("E.VALIDATION.SCHEMA", format!("{path} 要是数组。"), "按 schema 给出。").path(path))?;
    let out: Option<Vec<f64>> = a.iter().map(fnum).collect();
    let out = out.ok_or_else(|| OpError::new("E.VALIDATION.SCHEMA", format!("{path} 的元素要是数（边界可用 \"Inf\" / \"-Inf\"）。"), "修正元素。").path(path))?;
    if let Some(n) = n {
        if out.len() != n {
            return Err(OpError::new("E.VALIDATION.SHAPE", format!("{path} 有 {} 个元素，应为 {n} 个。", out.len()), "按未知量个数给出。").path(path).detail("expected_length", json!(n)));
        }
    }
    Ok(out)
}
pub(crate) fn rows(v: &Value, path: &str, n: usize) -> Result<Vec<Vec<f64>>, OpError> {
    match v.as_array() {
        None => Ok(vec![]),
        Some(a) => a.iter().enumerate().map(|(i, r)| {
            let r = vecf(r, &format!("{path}/{i}"), Some(n))?;
            if r.iter().any(|x| !x.is_finite()) {
                return Err(OpError::new("E.VALIDATION.NON_FINITE", format!("{path}/{i} 含非有限值。"), "约束系数必须有限。").path(&format!("{path}/{i}")));
            }
            Ok(r)
        }).collect(),
    }
}
pub(crate) fn bounds(args: &Value, n: usize) -> Result<(Vec<f64>, Vec<f64>), OpError> {
    let lb = match args.get("lb") { Some(v) => vecf(v, "/lb", Some(n))?, None => vec![f64::NEG_INFINITY; n] };
    let ub = match args.get("ub") { Some(v) => vecf(v, "/ub", Some(n))?, None => vec![f64::INFINITY; n] };
    if let Some(j) = (0..n).find(|&j| lb[j] > ub[j] || lb[j] == f64::INFINITY || ub[j] == f64::NEG_INFINITY) {
        return Err(OpError::new("E.DOMAIN.INFEASIBLE", format!("第 {j} 个变量的边界不相容：lb = {}，ub = {}。", lb[j], ub[j]), "检查边界。").path(&format!("/lb/{j}")).remedy(ask_user("bounds", "这个变量的取值范围是什么？")));
    }
    Ok((lb, ub))
}
pub(crate) fn linear(args: &Value, n: usize) -> Result<(Vec<Vec<f64>>, Vec<f64>, Vec<Vec<f64>>, Vec<f64>), OpError> {
    let a = rows(&args["A"], "/A", n)?;
    let b = match args.get("b") { Some(v) => vecf(v, "/b", Some(a.len()))?, None => vec![] };
    let aeq = rows(&args["Aeq"], "/Aeq", n)?;
    let beq = match args.get("beq") { Some(v) => vecf(v, "/beq", Some(aeq.len()))?, None => vec![] };
    if b.len() != a.len() || beq.len() != aeq.len() {
        return Err(OpError::new("E.VALIDATION.SHAPE", "A 与 b（或 Aeq 与 beq）的行数不一致。", "每行约束一个右端项。").path(if b.len() != a.len() { "/b" } else { "/beq" }));
    }
    if b.iter().chain(&beq).any(|v| !v.is_finite()) {
        return Err(OpError::new("E.VALIDATION.NON_FINITE", "b 或 beq 含非有限值。", "右端项必须有限。").path("/b"));
    }
    Ok((a, b, aeq, beq))
}
pub(crate) fn infeasible(msg: String) -> OpError {
    OpError::new("E.DOMAIN.INFEASIBLE", msg, "检查约束是否相互矛盾（符号、单位、上下限）；问清用户哪些约束可以放宽。").path("/").remedy(ask_user("relax", "约束互相矛盾：哪些可以放宽？"))
}

// ============================================================================================ linprog
pub struct InLp {
    c: Vec<f64>,
    a: Vec<Vec<f64>>,
    b: Vec<f64>,
    aeq: Vec<Vec<f64>>,
    beq: Vec<f64>,
    lb: Vec<f64>,
    ub: Vec<f64>,
}

impl Operator for Linprog {
    type Input = InLp;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(InLp, Vec<Warning>), OpError> {
        let c = vecf(&args["c"], "/c", None)?;
        let n = c.len();
        let lim = self.meta.manifest["limits"]["max_variables"].as_u64().unwrap() as usize;
        if n > lim {
            return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("{n} 个变量超过上限 {lim}（稠密单纯形）。"), "大规模问题需要稀疏 / 内点法（以后的方法）。").path("/c"));
        }
        if c.iter().any(|v| !v.is_finite()) {
            return Err(OpError::new("E.VALIDATION.NON_FINITE", "c 含非有限值。", "目标系数必须有限。").path("/c"));
        }
        let (a, b, aeq, beq) = linear(args, n)?;
        let (lb, ub) = bounds(args, n)?;
        Ok((InLp { c, a, b, aeq, beq, lb, ub }, Vec::new()))
    }
    fn execute(&self, i: InLp, _ctx: &ExecCtx) -> Result<Output, OpError> {
        let p = lp::LpProblem { c: &i.c, a: &i.a, b: &i.b, aeq: &i.aeq, beq: &i.beq, lb: &i.lb, ub: &i.ub };
        let max_it = self.meta.manifest["limits"]["max_iterations"].as_u64().unwrap() as usize;
        let s = match lp::solve(&p, max_it) {
            Ok(s) => s,
            Err(lp::LpError::Infeasible { violation }) => return Err(infeasible(format!("约束没有可行解（第一阶段最小违反量 {violation:.3e}）。"))),
            Err(lp::LpError::Unbounded { .. }) => return Err(OpError::new("E.DOMAIN.UNBOUNDED", "目标函数在可行域上无下界：可以无限减小。", "通常是漏了约束或边界（注意：本算子与 MATLAB 一样，变量缺省没有下界，scipy 缺省是 x ≥ 0）。").path("/c").remedy(ask_user("missing_bounds", "是否漏了变量的上下界（例如 x ≥ 0）或某个约束？"))),
            Err(lp::LpError::IterationLimit) => return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", "单纯形迭代达到上限。", "问题可能高度退化；缩小规模或改写约束。").path("/")),
        };
        let (pr, du, co, st) = lp::kkt(&p, &s);
        let tol = 1e-9;
        let finite = s.x.iter().all(|v| v.is_finite());
        let vj = |v: &[f64]| json!(v.iter().map(|x| num(*x)).collect::<Vec<_>>());
        let result = json!({
            "x": vj(&s.x), "fval": num(s.fval),
            "lambda": {"ineqlin": vj(&s.ineqlin), "eqlin": vj(&s.eqlin), "lower": vj(&s.lower), "upper": vj(&s.upper)},
            "iterations": s.iterations,
            "bounds_used": {"lb": vj(&i.lb), "ub": vj(&i.ub)},
        });
        let trust = json!({
            "convergence": {"applicable": true, "converged": true, "iterations": s.iterations},
            "residual": {"primal_infeasibility": num(pr), "dual_infeasibility": num(du), "complementarity": num(co), "stationarity": num(st), "threshold": tol,
                "definition": "相对数据尺度的 KKT 残差：c + Aᵀλ_ineq + Aeqᵀλ_eq − λ_lower + λ_upper = 0，λ ≥ 0，λ·松弛 = 0"},
            "condition": {"applicable": false},
            "error_estimate": {"applicable": false, "note": "线性规划的最优解在顶点上；最优解不唯一时（目标与某个约束平行）给出的是其中一个顶点"},
            "checks": {"kkt": pr <= tol && du <= tol && co <= tol && st <= tol, "all_finite": finite},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"method": "两阶段单纯形（Bland 规则）；基解与对偶变量用 LU 内核重算"}
        });
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

// ============================================================================================ shared: expressions
struct Model {
    names: Vec<String>,
    n: usize,
    obj: Node,
    ineq: Vec<Node>,
    eq: Vec<Node>,
    params: Vec<f64>,
}
impl Model {
    fn vars(&self, x: &[f64]) -> Vec<f64> {
        let mut v = x.to_vec();
        v.extend_from_slice(&self.params);
        v
    }
    fn seed(&self) -> Vec<Option<usize>> {
        (0..self.n + self.params.len()).map(|k| if k < self.n { Some(k) } else { None }).collect()
    }
    fn f(&self, x: &[f64]) -> f64 {
        eval(&self.obj, &self.vars(x))
    }
    fn fg(&self, x: &[f64]) -> (f64, Vec<f64>) {
        eval_grad(&self.obj, &self.vars(x), &self.seed(), self.n)
    }
    fn cg(&self, nodes: &[Node], x: &[f64]) -> (Vec<f64>, Vec<Vec<f64>>) {
        let v = self.vars(x);
        let s = self.seed();
        nodes.iter().map(|e| eval_grad(e, &v, &s, self.n)).unzip()
    }
}

fn model(args: &Value, constraints: bool) -> Result<(Model, Vec<f64>), OpError> {
    let names: Vec<String> = args["unknowns"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    for (k, u) in names.iter().enumerate() {
        check_name(u).map_err(|m| OpError::new("E.VALIDATION.EXPRESSION", m, "换一个名字。").path(&format!("/unknowns/{k}")).detail("kind", json!("bad_name")))?;
        if names[..k].contains(u) {
            return Err(OpError::new("E.VALIDATION.EXPRESSION", format!("未知量 '{u}' 重复。"), "每个未知量只写一次。").path(&format!("/unknowns/{k}")).detail("kind", json!("duplicate_name")));
        }
    }
    let n = names.len();
    let x0 = vecf(&args["x0"], "/x0", Some(n))?;
    if x0.iter().any(|v| !v.is_finite()) {
        return Err(OpError::new("E.VALIDATION.NON_FINITE", "x0 含非有限值。", "给出有限的初值。").path("/x0"));
    }
    let (pn, pv) = parse_params(&args["params"], &names)?;
    let mut all = names.clone();
    all.extend(pn);
    let src = args["objective"].as_str().unwrap();
    let obj = parse(src, &all).map_err(|e| expr_err("/objective", src, e))?;
    let mut ineq = Vec::new();
    let mut eq = Vec::new();
    if constraints {
        for (key, out) in [("nonlinear_ineq", &mut ineq), ("nonlinear_eq", &mut eq)] {
            if let Some(a) = args.get(key).and_then(|v| v.as_array()) {
                for (k, s) in a.iter().enumerate() {
                    let s = s.as_str().unwrap();
                    out.push(parse(s, &all).map_err(|e| expr_err(&format!("/{key}/{k}"), s, e))?);
                }
            }
        }
    }
    Ok((Model { names, n, obj, ineq, eq, params: pv }, x0))
}

fn solution_obj(names: &[String], x: &[f64]) -> Value {
    Value::Object(names.iter().zip(x).map(|(k, v)| (k.clone(), num(*v))).collect())
}

// ============================================================================================ fminsearch
pub struct InNm {
    m: Model,
    x0: Vec<f64>,
    tolx: f64,
    tolf: f64,
    max_iter: usize,
}

impl Operator for Fminsearch {
    type Input = InNm;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(InNm, Vec<Warning>), OpError> {
        let (m, x0) = model(args, false)?;
        let n = m.n;
        let tolx = args.get("tol_x").and_then(|v| v.as_f64()).unwrap_or(1e-4);
        let tolf = args.get("tol_fun").and_then(|v| v.as_f64()).unwrap_or(1e-4);
        let max_iter = args.get("max_iterations").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(200 * n);
        Ok((InNm { m, x0, tolx, tolf, max_iter }, Vec::new()))
    }
    fn execute(&self, i: InNm, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.m.n;
        let f = |x: &[f64]| i.m.f(x);
        // Lagarias et al. (MATLAB fminsearch): ρ = 1, χ = 2, ψ = 1/2, σ = 1/2; initial simplex x0 + 5 % per
        // coordinate (0.00025 for a zero coordinate)
        let mut s: Vec<Vec<f64>> = vec![i.x0.clone()];
        for j in 0..n {
            let mut v = i.x0.clone();
            v[j] = if v[j] != 0.0 { 1.05 * v[j] } else { 0.00025 };
            s.push(v);
        }
        let mut fv: Vec<f64> = s.iter().map(|v| f(v)).collect();
        let mut evals = n + 1;
        if fv.iter().any(|v| !v.is_finite()) {
            return Err(OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", "目标函数在初始单纯形上不是有限值。", "换一个初值。").path("/x0"));
        }
        let mut iters = 0usize;
        let order = |s: &mut Vec<Vec<f64>>, fv: &mut Vec<f64>| {
            let mut idx: Vec<usize> = (0..fv.len()).collect();
            idx.sort_by(|&a, &b| fv[a].partial_cmp(&fv[b]).unwrap().then(a.cmp(&b)));
            *s = idx.iter().map(|&k| s[k].clone()).collect();
            *fv = idx.iter().map(|&k| fv[k]).collect();
        };
        order(&mut s, &mut fv);
        let converged = loop {
            let size = (1..=n).map(|k| (0..n).map(|j| (s[k][j] - s[0][j]).abs()).fold(0.0, f64::max)).fold(0.0, f64::max);
            let spread = (1..=n).map(|k| (fv[k] - fv[0]).abs()).fold(0.0, f64::max);
            if size <= i.tolx && spread <= i.tolf {
                break true;
            }
            if iters >= i.max_iter {
                break false;
            }
            iters += 1;
            let xbar: Vec<f64> = (0..n).map(|j| (0..n).map(|k| s[k][j]).sum::<f64>() / n as f64).collect();
            let pt = |t: f64| -> Vec<f64> { (0..n).map(|j| xbar[j] + t * (xbar[j] - s[n][j])).collect() };
            let xr = pt(1.0);
            let fr = f(&xr);
            evals += 1;
            if fr < fv[0] {
                let xe = pt(2.0);
                let fe = f(&xe);
                evals += 1;
                if fe < fr { s[n] = xe; fv[n] = fe; } else { s[n] = xr; fv[n] = fr; }
            } else if fr < fv[n - 1] {
                s[n] = xr;
                fv[n] = fr;
            } else {
                let (xc, fc, inside) = if fr < fv[n] { let x = pt(0.5); let v = f(&x); (x, v, false) } else { let x = pt(-0.5); let v = f(&x); (x, v, true) };
                evals += 1;
                if (!inside && fc <= fr) || (inside && fc < fv[n]) {
                    s[n] = xc;
                    fv[n] = fc;
                } else {
                    for k in 1..=n {
                        s[k] = (0..n).map(|j| s[0][j] + 0.5 * (s[k][j] - s[0][j])).collect();
                        fv[k] = f(&s[k]);
                    }
                    evals += n;
                }
            }
            if fv.iter().any(|v| v.is_nan()) {
                return Err(OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", "目标函数出现 NaN。", "检查目标函数的定义域。").path("/objective"));
            }
            order(&mut s, &mut fv);
            if iters % 64 == 0 {
                ctx.check()?;
            }
        };
        let x = s[0].clone();
        let fx = fv[0];
        if !converged {
            return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("{iters} 次迭代后单纯形仍未收缩到容差以内（f = {fx:.6e}）。"), "增大 max_iterations、放宽容差，或换初值；变量多（> 10）时 Nelder–Mead 很慢，考虑 fmincon。").path("/x0")
                .detail("best", json!({"x": x.iter().map(|v| num(*v)).collect::<Vec<_>>(), "fval": num(fx), "iterations": iters}))
                .remedy(ask_user("better_x0", "能否给一个更好的初值，或放宽容差？")));
        }
        // derivative-free local-minimum test at the resolution tol_x: f(x) ≤ f(x ± h·e_j)
        let mut probe_worst = f64::NEG_INFINITY;
        for j in 0..n {
            let h = i.tolx * x[j].abs().max(1.0);
            for sgn in [-1.0, 1.0] {
                let mut y = x.clone();
                y[j] += sgn * h;
                probe_worst = probe_worst.max(fx - f(&y));
                evals += 1;
            }
        }
        let local_min = probe_worst <= i.tolf;
        let result = json!({
            "x": x.iter().map(|v| num(*v)).collect::<Vec<_>>(), "solution": solution_obj(&i.m.names, &x), "fval": num(fx),
            "iterations": iters, "evaluations": evals, "tolerances": {"tol_x": i.tolx, "tol_fun": i.tolf},
        });
        let trust = json!({
            "convergence": {"applicable": true, "converged": true, "iterations": iters, "criterion": "单纯形直径 ≤ tol_x 且函数值极差 ≤ tol_fun（同 MATLAB）"},
            "residual": {"probe_decrease": num(probe_worst.max(0.0)), "definition": "max_j,± [f(x) − f(x ± h·e_j)]，h = tol_x·max(1, |x_j|)"},
            "condition": {"applicable": false},
            "error_estimate": {"x_abs": i.tolx, "basis": "单纯形直径（Nelder–Mead 没有收敛性保证；结果只是 tol_x 分辨率下的局部极小）"},
            "checks": {"probe_local_minimum": local_min, "all_finite": x.iter().all(|v| v.is_finite()) && fx.is_finite()},
            "accuracy_grade": "C",
            "credentials": Value::Null,
            "derived": {"optimum": "local"}
        });
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

// ============================================================================================ fmincon
pub struct InCon {
    m: Model,
    x0: Vec<f64>,
    a: Vec<Vec<f64>>,
    b: Vec<f64>,
    aeq: Vec<Vec<f64>>,
    beq: Vec<f64>,
    lb: Vec<f64>,
    ub: Vec<f64>,
    tol_opt: f64,
    tol_con: f64,
    max_iter: usize,
}

impl Operator for Fmincon {
    type Input = InCon;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, _host: &HostCtx) -> Result<(InCon, Vec<Warning>), OpError> {
        let (m, x0) = model(args, true)?;
        let n = m.n;
        let lim = self.meta.manifest["limits"]["max_unknowns"].as_u64().unwrap() as usize;
        if n > lim {
            return Err(OpError::new("E.RESOURCE.INPUT_TOO_LARGE", format!("{n} 个未知量超过上限 {lim}。"), "减少未知量。").path("/unknowns"));
        }
        let (a, b, aeq, beq) = linear(args, n)?;
        let (lb, ub) = bounds(args, n)?;
        let tol_opt = args.get("optimality_tolerance").and_then(|v| v.as_f64()).unwrap_or(1e-8);
        let tol_con = args.get("constraint_tolerance").and_then(|v| v.as_f64()).unwrap_or(1e-8);
        let max_iter = self.meta.manifest["limits"]["max_iterations"].as_u64().unwrap() as usize;
        Ok((InCon { m, x0, a, b, aeq, beq, lb, ub, tol_opt, tol_con, max_iter }, Vec::new()))
    }
    fn execute(&self, i: InCon, ctx: &ExecCtx) -> Result<Output, OpError> {
        let n = i.m.n;
        let (mi, me) = (i.m.ineq.len(), i.m.eq.len());
        // all constraints as rows g(x) ≤ 0 / h(x) = 0 with gradients
        let eval_all = |x: &[f64]| -> (f64, Vec<f64>, Vec<f64>, Vec<Vec<f64>>, Vec<f64>, Vec<Vec<f64>>) {
            let (f, g) = i.m.fg(x);
            let (mut ci, mut gi) = i.m.cg(&i.m.ineq, x);
            for (r, bi) in i.a.iter().zip(&i.b) {
                ci.push(r.iter().zip(x).map(|(p, q)| p * q).sum::<f64>() - bi);
                gi.push(r.clone());
            }
            for j in 0..n {
                if i.lb[j].is_finite() {
                    ci.push(i.lb[j] - x[j]);
                    let mut e = vec![0.0; n];
                    e[j] = -1.0;
                    gi.push(e);
                }
                if i.ub[j].is_finite() {
                    ci.push(x[j] - i.ub[j]);
                    let mut e = vec![0.0; n];
                    e[j] = 1.0;
                    gi.push(e);
                }
            }
            let (mut ce, mut ge) = i.m.cg(&i.m.eq, x);
            for (r, bi) in i.aeq.iter().zip(&i.beq) {
                ce.push(r.iter().zip(x).map(|(p, q)| p * q).sum::<f64>() - bi);
                ge.push(r.clone());
            }
            (f, g, ci, gi, ce, ge)
        };
        let viol = |ci: &[f64], ce: &[f64]| ci.iter().fold(0.0f64, |m, v| m.max(*v)).max(ce.iter().fold(0.0f64, |m, v| m.max(v.abs())));
        let l1 = |ci: &[f64], ce: &[f64]| ci.iter().map(|v| v.max(0.0)).sum::<f64>() + ce.iter().map(|v| v.abs()).sum::<f64>();
        let mut x = i.x0.clone();
        // start inside the bounds (MATLAB's SQP does the same)
        for j in 0..n {
            x[j] = x[j].max(i.lb[j]).min(i.ub[j]);
        }
        let (mut f, mut g, mut ci, mut gi, mut ce, mut ge) = eval_all(&x);
        if !f.is_finite() || g.iter().chain(ci.iter()).chain(ce.iter()).any(|v| !v.is_finite()) {
            return Err(OpError::new("E.NUMERICAL.NON_FINITE_EVALUATION", "目标或约束（或其导数）在 x0 处不是有限值。", "换一个初值。").path("/x0"));
        }
        let mut bmat: Vec<f64> = (0..n * n).map(|k| if k % (n + 1) == 0 { 1.0 } else { 0.0 }).collect();
        let mut mu = 1.0f64;
        let (mut lam_i, mut lam_e) = (vec![0.0; ci.len()], vec![0.0; ce.len()]);
        let mut evals = 1usize;
        let mut iters = 0usize;
        let kkt = |f: f64, g: &[f64], gi: &[Vec<f64>], ge: &[Vec<f64>], li: &[f64], le: &[f64], ci: &[f64]| -> (f64, f64) {
            let mut r = g.to_vec();
            for (row, l) in gi.iter().zip(li) {
                for (rj, aj) in r.iter_mut().zip(row) {
                    *rj += l * aj;
                }
            }
            for (row, l) in ge.iter().zip(le) {
                for (rj, aj) in r.iter_mut().zip(row) {
                    *rj += l * aj;
                }
            }
            let stat = r.iter().fold(0.0f64, |m, v| m.max(v.abs())) / g.iter().fold(1.0f64, |m, v| m.max(v.abs()));
            let comp = li.iter().zip(ci).map(|(l, c)| (l * c).abs()).fold(0.0, f64::max) / f.abs().max(1.0);
            (stat, comp)
        };
        let converged = loop {
            let (stat, comp) = kkt(f, &g, &gi, &ge, &lam_i, &lam_e, &ci);
            let v = viol(&ci, &ce);
            if iters > 0 && stat <= i.tol_opt && comp <= i.tol_opt && v <= i.tol_con {
                break true;
            }
            if iters >= i.max_iter {
                break false;
            }
            iters += 1;
            // QP: min ½pᵀBp + gᵀp, gi·p ≤ −ci, ge·p = −ce (relax violated nonlinear rows if inconsistent)
            let mut theta = 1.0;
            let sol = loop {
                let bi: Vec<f64> = ci.iter().map(|c| if *c > 0.0 { -theta * c } else { -c }).collect();
                let be: Vec<f64> = ce.iter().map(|c| -theta * c).collect();
                match qp::solve(&bmat, &g, &gi, &bi, &ge, &be, 50 * (n + gi.len() + ge.len()).max(10)) {
                    Ok(s) => break Some(s),
                    Err(_) if theta > 1e-3 => theta *= 0.5,
                    Err(_) => break None,
                }
            };
            let Some(sol) = sol else {
                return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("第 {iters} 步的线性化约束不相容（放宽后仍无解），当前约束违反量 {v:.3e}：问题可能不可行。"), "检查约束是否相互矛盾；换一个更接近可行域的初值。").path("/x0")
                    .detail("best", json!({"x": x.iter().map(|v| num(*v)).collect::<Vec<_>>(), "fval": num(f), "constraint_violation": num(v)}))
                    .remedy(ask_user("feasible_x0", "能否给一个满足约束的初值，或说明哪些约束可以放宽？")));
            };
            let p = sol.p.clone();
            let lmax = sol.lam_ineq.iter().chain(sol.lam_eq.iter()).fold(0.0f64, |m, v| m.max(v.abs()));
            mu = mu.max(1.1 * lmax + 1e-3);
            let pn = p.iter().fold(0.0f64, |m, v| m.max(v.abs()));
            if pn <= 1e-14 * x.iter().fold(1.0f64, |m, v| m.max(v.abs())) {
                lam_i = sol.lam_ineq;
                lam_e = sol.lam_eq;
                let (stat, comp) = kkt(f, &g, &gi, &ge, &lam_i, &lam_e, &ci);
                if stat <= i.tol_opt && comp <= i.tol_opt && viol(&ci, &ce) <= i.tol_con {
                    break true;
                }
                break false;
            }
            // ℓ1 merit line search
            let phi0 = f + mu * l1(&ci, &ce);
            let dphi = g.iter().zip(&p).map(|(a, b)| a * b).sum::<f64>() - mu * l1(&ci, &ce);
            let mut alpha = 1.0;
            let mut accepted = None;
            for _ in 0..40 {
                let xt: Vec<f64> = x.iter().zip(&p).map(|(a, b)| a + alpha * b).collect();
                let t = eval_all(&xt);
                evals += 1;
                let ok = t.0.is_finite() && t.2.iter().chain(t.4.iter()).all(|v| v.is_finite());
                if ok && t.0 + mu * l1(&t.2, &t.4) <= phi0 + 1e-4 * alpha * dphi.min(0.0) {
                    accepted = Some((xt, t));
                    break;
                }
                alpha *= 0.5;
            }
            let Some((xt, t)) = accepted else {
                lam_i = sol.lam_ineq;
                lam_e = sol.lam_eq;
                break false;
            };
            // damped BFGS (Powell) on the Lagrangian gradient
            let lag = |g: &[f64], gi: &[Vec<f64>], ge: &[Vec<f64>]| -> Vec<f64> {
                let mut r = g.to_vec();
                for (row, l) in gi.iter().zip(&sol.lam_ineq) {
                    for (rj, aj) in r.iter_mut().zip(row) { *rj += l * aj; }
                }
                for (row, l) in ge.iter().zip(&sol.lam_eq) {
                    for (rj, aj) in r.iter_mut().zip(row) { *rj += l * aj; }
                }
                r
            };
            let sv: Vec<f64> = xt.iter().zip(&x).map(|(a, b)| a - b).collect();
            let yv: Vec<f64> = lag(&t.1, &t.3, &t.5).iter().zip(lag(&g, &gi, &ge)).map(|(a, b)| a - b).collect();
            let bs: Vec<f64> = (0..n).map(|r| (0..n).map(|c| bmat[r * n + c] * sv[c]).sum()).collect();
            let sbs: f64 = sv.iter().zip(&bs).map(|(a, b)| a * b).sum();
            let sy: f64 = sv.iter().zip(&yv).map(|(a, b)| a * b).sum();
            if sbs > 0.0 {
                let th = if sy >= 0.2 * sbs { 1.0 } else { 0.8 * sbs / (sbs - sy) };
                let r: Vec<f64> = yv.iter().zip(&bs).map(|(y, b)| th * y + (1.0 - th) * b).collect();
                let sr: f64 = sv.iter().zip(&r).map(|(a, b)| a * b).sum();
                if sr > 0.0 {
                    for a in 0..n {
                        for c in 0..n {
                            bmat[a * n + c] += -bs[a] * bs[c] / sbs + r[a] * r[c] / sr;
                        }
                    }
                }
            }
            x = xt;
            (f, g, ci, gi, ce, ge) = t;
            lam_i = sol.lam_ineq;
            lam_e = sol.lam_eq;
            ctx.check()?;
        };
        let v = viol(&ci, &ce);
        let (stat, comp) = kkt(f, &g, &gi, &ge, &lam_i, &lam_e, &ci);
        if !converged {
            return Err(OpError::new("E.NUMERICAL.NO_CONVERGENCE", format!("{iters} 次 SQP 迭代后未满足 KKT 条件（一阶最优性 {stat:.2e}，约束违反 {v:.2e}）。"), "换初值，或放宽 optimality_tolerance / constraint_tolerance；检查目标与约束在可行域附近是否光滑。").path("/x0")
                .detail("best", json!({"x": x.iter().map(|q| num(*q)).collect::<Vec<_>>(), "fval": num(f), "first_order_optimality": num(stat), "constraint_violation": num(v)}))
                .remedy(ask_user("better_x0", "能否给一个更好的初值？")));
        }
        // split multipliers back to MATLAB's groups
        let mut k = 0;
        let take = |k: &mut usize, m: usize| -> Vec<f64> { let v = lam_i[*k..*k + m].to_vec(); *k += m; v };
        let ineqnonlin = take(&mut k, mi);
        let ineqlin = take(&mut k, i.a.len());
        let mut lower = vec![0.0; n];
        let mut upper = vec![0.0; n];
        for j in 0..n {
            if i.lb[j].is_finite() { lower[j] = lam_i[k]; k += 1; }
            if i.ub[j].is_finite() { upper[j] = lam_i[k]; k += 1; }
        }
        let eqnonlin = lam_e[..me].to_vec();
        let eqlin = lam_e[me..].to_vec();
        let vj = |v: &[f64]| json!(v.iter().map(|q| num(*q)).collect::<Vec<_>>());
        let result = json!({
            "x": vj(&x), "solution": solution_obj(&i.m.names, &x), "fval": num(f),
            "lambda": {"ineqnonlin": vj(&ineqnonlin), "eqnonlin": vj(&eqnonlin), "ineqlin": vj(&ineqlin), "eqlin": vj(&eqlin), "lower": vj(&lower), "upper": vj(&upper)},
            "iterations": iters, "evaluations": evals, "first_order_optimality": num(stat), "constraint_violation": num(v),
        });
        let trust = json!({
            "convergence": {"applicable": true, "converged": true, "iterations": iters, "criterion": format!("相对 KKT 残差 ≤ {:e}，约束违反 ≤ {:e}", i.tol_opt, i.tol_con)},
            "residual": {"stationarity": num(stat), "complementarity": num(comp), "constraint_violation": num(v),
                "definition": "stationarity = ‖∇f + Σλ∇g + Σν∇h‖∞ / max(1, ‖∇f‖∞)；complementarity = max|λ_i·g_i| / max(1, |f|)"},
            "condition": {"applicable": false},
            "error_estimate": {"applicable": false, "note": "KKT 残差小只说明是一个局部最优（或鞍点，二阶条件未检查）"},
            "checks": {"kkt": stat <= i.tol_opt && comp <= i.tol_opt && v <= i.tol_con, "all_finite": x.iter().all(|q| q.is_finite()) && f.is_finite()},
            "accuracy_grade": "B",
            "credentials": Value::Null,
            "derived": {"optimum": "local", "method": "SQP：阻尼 BFGS + 有效集 QP（共享内核）+ ℓ1 价值函数线搜索；梯度由自动微分（3.15 EX-8）"}
        });
        Ok(Output { result, trust, warnings: Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeoi_core::DynOperator;
    #[test]
    fn linprog_basic() {
        // max x + y s.t. x + 2y ≤ 4, 3x + y ≤ 6, x, y ≥ 0  →  (1.6, 1.2)
        let e = Linprog::new().call(&json!({"c": [-1.0, -1.0], "A": [[1.0, 2.0], [3.0, 1.0]], "b": [4.0, 6.0], "lb": [0.0, 0.0]}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        let x: Vec<f64> = e["result"]["x"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!((x[0] - 1.6).abs() < 1e-14 && (x[1] - 1.2).abs() < 1e-14, "{x:?}");
        let e = Linprog::new().call(&json!({"c": [-1.0], "lb": [0.0]}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.DOMAIN.UNBOUNDED");
        let e = Linprog::new().call(&json!({"c": [1.0], "A": [[1.0]], "b": [-1.0], "lb": [0.0]}), &HostCtx::default());
        assert_eq!(e["error"]["code"], "E.DOMAIN.INFEASIBLE");
    }
    #[test]
    fn fmincon_disk() {
        // min x + y s.t. x² + y² ≤ 2  →  (−1, −1)
        let e = Fmincon::new().call(&json!({"unknowns": ["x", "y"], "objective": "x + y", "nonlinear_ineq": ["x^2 + y^2 - 2"], "x0": [0.5, 0.0]}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        let x: Vec<f64> = e["result"]["x"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!((x[0] + 1.0).abs() < 1e-8 && (x[1] + 1.0).abs() < 1e-8, "{e}");
    }
    #[test]
    fn fminsearch_rosenbrock() {
        let e = Fminsearch::new().call(&json!({"unknowns": ["x", "y"], "objective": "100*(y - x^2)^2 + (1 - x)^2", "x0": [-1.2, 1.0]}), &HostCtx::default());
        assert_eq!(e["status"], "ok", "{e}");
        let x: Vec<f64> = e["result"]["x"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!((x[0] - 1.0).abs() < 1e-3 && (x[1] - 1.0).abs() < 1e-3, "{e}");
    }
}
