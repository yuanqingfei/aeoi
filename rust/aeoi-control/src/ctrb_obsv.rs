//! control.analysis.ctrb_obsv — controllability / observability matrices, numerical rank, Gramians (spec 10.6).
use crate::check_states;
use aeoi_core::error::{OpError, Warning};
use aeoi_core::lti::{eigenvalues, parse_lti, Lti, SsData};
use aeoi_core::matrix::{dense_real, dense_real_nd};
use aeoi_core::sysalg::{lyap_bs, lyap_residual, pbh, LyapKind};
use aeoi_core::{num, opt_num, ExecCtx, HostCtx, Operator, Output};
use nalgebra::DMatrix;
use serde_json::{json, Value};

operator_struct!(CtrbObsv, "control.analysis.ctrb_obsv");

pub struct CoInput {
    pub ss: SsData,
    pub kind: &'static str,
    pub gramians: bool,
    pub rtol: Option<f64>,
}

pub fn ctrb(a: &DMatrix<f64>, b: &DMatrix<f64>) -> DMatrix<f64> {
    let (n, m) = (a.nrows(), b.ncols());
    let mut k = DMatrix::zeros(n, n * m);
    let mut blk = b.clone();
    for i in 0..n {
        k.view_mut((0, i * m), (n, m)).copy_from(&blk);
        blk = a * blk;
    }
    k
}

/// (rank, singular values descending, tol used, gap ratio σ_r/σ_{r+1})
fn num_rank(m: &DMatrix<f64>, rtol: Option<f64>) -> (usize, Vec<f64>, f64, Option<f64>) {
    if m.is_empty() {
        return (0, vec![], 0.0, None);
    }
    let mut sv: Vec<f64> = m.clone().svd(false, false).singular_values.iter().cloned().collect();
    sv.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let rt = rtol.unwrap_or(m.nrows().max(m.ncols()) as f64 * f64::EPSILON);
    let tol = sv[0] * rt;
    let r = sv.iter().filter(|s| **s > tol).count();
    let gap = if r > 0 && r < sv.len() { Some(sv[r - 1] / sv[r].max(f64::MIN_POSITIVE)) } else { None };
    (r, sv, tol, gap)
}

impl Operator for CtrbObsv {
    type Input = CoInput;
    fn meta(&self) -> &aeoi_core::OperatorMeta {
        &self.meta
    }
    fn validate(&self, args: &Value, host: &HostCtx) -> Result<(CoInput, Vec<Warning>), OpError> {
        let mut w = Vec::new();
        let sys = parse_lti(&args["system"], "/system", host, &mut w, self.meta.max_dense("/system/A"))?;
        let ss = sys.to_ss();
        check_states(&self.meta, ss.a.nrows(), "/system")?;
        if !matches!(sys, Lti::Ss { .. }) {
            w.push(Warning::new("W.DOMAIN.REALIZATION_DEPENDENT", format!("输入为 {}，已按可控标准型实现；可控/可观矩阵与 Gramian 依赖于所选实现（可控性在该实现下恒成立）。", sys.kind())).path("/system"));
        }
        if ss.a.nrows() == 0 {
            return Err(OpError::new("E.VALIDATION.SHAPE", "系统没有状态（n = 0），可控/可观矩阵无定义。", "这是纯增益系统，无需该分析。").path("/system"));
        }
        Ok((CoInput { ss, kind: sys.kind(), gramians: args.get("gramians").and_then(|x| x.as_bool()).unwrap_or(false), rtol: args.get("rank_rtol").and_then(|x| x.as_f64()) }, w))
    }
    fn execute(&self, inp: CoInput, ctx: &ExecCtx) -> Result<Output, OpError> {
        let ss = &inp.ss;
        let n = ss.a.nrows();
        let mut warnings = Vec::new();
        let kc = ctrb(&ss.a, &ss.b);
        let ko = ctrb(&ss.a.transpose(), &ss.c.transpose()).transpose();
        ctx.check()?;
        let (rc, svc, tolc, gapc) = num_rank(&kc, inp.rtol);
        let (ro, svo, tolo, gapo) = num_rank(&ko, inp.rtol);
        // PBH cross-check
        let eig = eigenvalues(&ss.a);
        let pbh_c = eig.iter().map(|l| pbh(&ss.a, &ss.b, *l)).fold(f64::INFINITY, f64::min);
        let pbh_o = eig.iter().map(|l| pbh(&ss.a.transpose(), &ss.c.transpose(), *l)).fold(f64::INFINITY, f64::min);
        let pbh_thr = 1e-10;
        for (name, full, p, gap) in [("可控", rc == n, pbh_c, gapc), ("可观", ro == n, pbh_o, gapo)] {
            if full != (p > pbh_thr) {
                warnings.push(Warning::new("W.NUMERICAL.RANK_AMBIGUOUS", format!("{name}性判定不一致：Krylov 矩阵数值秩判为{}，但 PBH 最小相对奇异值为 {p:.2e}（阈值 {pbh_thr:.0e}）。Krylov 矩阵通常病态，PBH 更可信。", if full { "满秩" } else { "降秩" })).path("/system"));
            } else if let Some(g) = gap {
                if g < 1e3 {
                    warnings.push(Warning::new("W.NUMERICAL.RANK_AMBIGUOUS", format!("{name}性矩阵的奇异值在秩阈值附近没有明显间隙（σ_r/σ_(r+1) = {g:.2e}），数值秩对容差敏感。")).path("/system"));
                }
            }
        }
        let condc = svc[0] / svc[n - 1].max(f64::MIN_POSITIVE);
        if rc == n && condc > 1e12 {
            warnings.push(Warning::new("W.NUMERICAL.ILL_CONDITIONED", format!("可控性矩阵条件数 {condc:.2e} 很大：系统“接近不可控”，或状态尺度差异大；请参考 trust 中的 PBH 值。")).path("/system"));
        }
        // Gramians
        let continuous = ss.ts == 0.0;
        let stable = if continuous { eig.iter().all(|l| l.re < 0.0) } else { eig.iter().all(|l| l.norm() < 1.0) };
        let mut gram_trust = Value::Null;
        let gram = if inp.gramians {
            if !stable {
                warnings.push(Warning::new("W.DOMAIN.GRAMIAN_UNDEFINED", "系统不是渐近稳定的，无限时域 Gramian 不存在，gramians 置为 null。").path("/gramians"));
                Value::Null
            } else {
                let kind = if continuous { LyapKind::Continuous } else { LyapKind::Discrete };
                let qc = &ss.b * ss.b.transpose();
                let qo = ss.c.transpose() * &ss.c;
                let (wc, ic) = lyap_bs(&ss.a, &qc, kind).map_err(|_| OpError::new("E.NUMERICAL.SINGULAR", "Gramian 的 Lyapunov 方程数值奇异。", "系统极点过于接近稳定边界。").path("/system"))?;
                let (wo, io) = lyap_bs(&ss.a.transpose(), &qo, kind).map_err(|_| OpError::new("E.NUMERICAL.SINGULAR", "Gramian 的 Lyapunov 方程数值奇异。", "系统极点过于接近稳定边界。").path("/system"))?;
                let wc = (&wc + wc.transpose()) * 0.5;
                let wo = (&wo + wo.transpose()) * 0.5;
                let (rcn, rcs) = lyap_residual(&ss.a, &qc, &wc, kind);
                let (ron, ros) = lyap_residual(&ss.a.transpose(), &qo, &wo, kind);
                gram_trust = json!({"controllability_rel_residual": rcn / rcs.max(f64::MIN_POSITIVE), "observability_rel_residual": ron / ros.max(f64::MIN_POSITIVE),
                    "separation_min": num(ic.separation.min(io.separation)), "definition": "‖A W + W Aᵀ + BBᵀ‖_F / (2‖A‖_F‖W‖_F + ‖BBᵀ‖_F)（离散：‖A W Aᵀ − W + BBᵀ‖_F / (‖A‖²‖W‖+‖W‖+‖BBᵀ‖)），可观同理"});
                json!({"controllability": dense_real(&wc), "observability": dense_real(&wo)})
            }
        } else {
            Value::Null
        };
        let pack = |k: &DMatrix<f64>, r: usize, sv: &Vec<f64>| json!({"matrix": dense_real(k), "rank": r, "singular_values": dense_real_nd(&[sv.len()], sv), "full_rank": r == n});
        let result = json!({"ctrb": pack(&kc, rc, &svc), "obsv": pack(&ko, ro, &svo), "gramians": gram, "minimal": rc == n && ro == n});
        let trust = json!({
            "convergence": {"applicable": false},
            "residual": gram_trust,
            "condition": {"ctrb_rank_tolerance": tolc, "obsv_rank_tolerance": tolo, "ctrb_sv_gap_at_rank": opt_num(gapc), "obsv_sv_gap_at_rank": opt_num(gapo),
                          "pbh_ctrb_min_rel_sv": num(pbh_c), "pbh_obsv_min_rel_sv": num(pbh_o), "pbh_threshold": pbh_thr, "ctrb_condition": num(condc),
                          "realization": if inp.kind == "ss" { "given ss" } else { "controllable canonical form (scipy tf2ss convention)" }},
            "error_estimate": {"basis": "秩为整数判定（T0）；Krylov 矩阵元素为精确的矩阵乘积（T1）；Gramian 见 residual"},
            "checks": {"pbh_agrees_ctrb": (rc == n) == (pbh_c > pbh_thr), "pbh_agrees_obsv": (ro == n) == (pbh_o > pbh_thr), "asymptotically_stable": stable},
            "accuracy_grade": "B",
            "credentials": Value::Null
        });
        Ok(Output { result, trust, warnings })
    }
}
