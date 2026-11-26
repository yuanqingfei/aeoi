"""control.lti.convert (profile 10.4), written from the profile and the core spec (3.10 R-5 .. R-9).

tf <-> ss <-> zpk through aeoi_lti: tf -> ss controllable canonical (MIMO: block diagonal per element);
ss -> tf by R-7 (characteristic polynomials from own Francis-QR eigenvalues); zpk -> tf expanded
product; tf -> zpk polynomial roots (balanced companion + Newton); ss -> zpk eig(A), zeros of the zero
dynamics and the first non-negligible Markov parameter (R-8). The frequency response before and after
is compared at 9 fixed test points (trust.residual, postcondition response_consistent).
"""
import math

import numpy as np

import aeoi_common as C
import aeoi_kernels as K
import aeoi_lti as L
from aeoi_common import Fail

OP_ID = "control.lti.convert"
VERSION = "0.3.0"
METHOD_ID = "pyscipy:convert:canonical-realization+francis-qr-roots"
DISCREPANCY = 1e-6


def _manifest():
    errors = L.LTI_ERRORS + ["E.DOMAIN.MIMO_NOT_SUPPORTED", "E.NUMERICAL.NO_CONVERGENCE"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="LTI representation conversion (Python second implementation)",
        description="在 tf / ss / zpk 三种表示之间转换 LTI 系统（连续或离散），给出所用实现约定（realization）与转换前后频率响应的最大相对差。"
                    "zpk 只表示 SISO；不做零极点对消（MIMO 结果为非最小实现）。",
        category="transform", tags=["lti", "realization", "tf2ss", "ss2tf", "zpk"],
        applicability={"preconditions": ["system 为 proper", "zpk 只表示 SISO：MIMO → zpk 返回 E.DOMAIN.MIMO_NOT_SUPPORTED"],
                       "not_for": ["最小实现（minreal）", "带时滞的系统"], "siso_only": False},
        provenance={"algorithm": "tf→ss：可控标准型（A 首行 = −den[1:]/den[0]，B = e₁，C = num − D·den 的尾项，D = num[0]/den[0]）；"
                                 "MIMO tf→ss：逐元素可控标准型按 (i, j) 行主序块对角拼接；ss→tf：den = det(sI−A)、num_ij = det(sI−A+b_j c_i) + (d_ij−1)·den，"
                                 "特征多项式由特征值（自写平衡 + Hessenberg + Francis 双位移 QR）展开，首项精确置为 d_ij 后去前导零；"
                                 "zpk→tf：乘积展开；tf→zpk：平衡友矩阵特征值 + ≤ 4 步只接受 |q| 下降的 Newton 修正，k = num[0]/den[0]；"
                                 "ss→zpk：p = eig(A)；D ≠ 0 时 z = eig(A − BC/d)、k = d；D = 0 时相对阶 r 取首个不可忽略的 Markov 参数"
                                 " h_r = CA^{r−1}B（|h| > 1e3·n·u·‖C‖‖A^{r−1}B‖），z = 零动态 N^T(A − B h_r^{-1} C A^r)N 的特征值"
                                 "（N 为 ker[C; CA; …; CA^{r−1}] 的正交基，Householder QR），k = h_r。"
                                 "转换后在 9 个固定测试点上比较频率响应。",
                    "references": [
                        {"type": "paper", "cite": "A. Emami-Naeini, P. Van Dooren, Computation of zeros of linear multivariable systems, Automatica 18(4), 1982",
                         "doi": "10.1016/0005-1098(82)90070-X"},
                        {"type": "textbook", "cite": "A. Isidori, Nonlinear Control Systems, 3rd ed., Springer 1995, §4.3（SISO 零动态）",
                         "doi": "10.1007/978-1-84628-615-5"},
                        {"type": "paper", "cite": "A. Edelman, H. Murakami, Polynomial roots from companion matrix eigenvalues, Math. Comp. 64(210), 1995",
                         "doi": "10.1090/S0025-5718-1995-1262279-2"}] + L.LTI_REFERENCES},
        backend={"library": "aeoi-pyscipy aeoi_lti + aeoi_kernels (Python + numpy; no LAPACK)",
                 "routine": "op_convert.Convert.execute (aeoi_lti.tf2ss_siso / ss2tf / zpk2tf / tf2zpk / ss2zpk; aeoi_kernels.hqr)",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T2",
        sparse_support={},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_states": L.MAX_STATES, "max_qr_iterations_per_eigenvalue": 60},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors,
        warnings=["W.DOMAIN.NONMINIMAL_REALIZATION", "W.NUMERICAL.CONVERSION_DISCREPANCY", "W.VALIDATION.LEADING_ZEROS_STRIPPED"],
        receipt={"method_id": METHOD_ID,
                 "assumptions": L.LTI_ASSUMPTIONS + [
                     {"id": "nonminimal", "statement": "不做零极点对消（MIMO 结果为非最小实现）", "enforced_by": "warning:W.DOMAIN.NONMINIMAL_REALIZATION"},
                     {"id": "siso_zpk", "statement": "zpk 只表示 SISO 系统", "enforced_by": "error:E.DOMAIN.MIMO_NOT_SUPPORTED"}],
                 "exactness": {"class": "approximate", "bound_kind": "a_posteriori_heuristic"},
                 "postconditions": ["response_consistent"],
                 "coverage": {"response_consistent": "test_points"},
                 "guards": {"response_consistent": ["/system"]}},
    )


def _test_points(s, pole_list):
    if s.discrete:
        th = np.array([math.pi * (k + 0.5) / 9.0 for k in range(9)])
        pts = np.cos(th) + 1j * np.sin(th)
    else:
        sc = max(1.0, float(np.max(np.abs(pole_list)))) if len(pole_list) else 1.0
        pts = 1j * np.array([10.0 ** (-2.0 + 0.5 * k) * sc for k in range(9)])
    keep = []
    for x in pts:
        d = min((abs(x - p) for p in pole_list), default=math.inf)
        if d > 1e-8 * max(1.0, abs(x)):
            keep.append(x)
    return np.array(keep, dtype=complex)


def _make(kind, ts, ny, nu, **kw):
    s = L.LTI(kind, ts, ny, nu)
    for k, v in kw.items():
        setattr(s, k, v)
    return s


class Convert(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        s = L.parse_system(args["system"])
        to = args["to"]
        if to == "zpk" and not s.siso:
            raise Fail("E.DOMAIN.MIMO_NOT_SUPPORTED", f"zpk 表示只支持 SISO 系统，输入为 {s.ny}×{s.nu}（R-8）。", "/to",
                       {"dims": [s.ny, s.nu]},
                       [C.patch_remedy("to_tf", "改为转换到 tf（支持 MIMO）", [{"op": "replace", "path": "/to", "value": "tf"}],
                                       confidence="high", confirm=False),
                        C.patch_remedy("to_ss", "改为转换到 ss", [{"op": "replace", "path": "/to", "value": "ss"}],
                                       confidence="high", confirm=False),
                        C.ask_user("需要 zpk 时逐通道调用（取出单个输出/输入对应的 SISO 子系统）")],
                       agent_hint="逐通道调用，或改用支持 MIMO 的表示（tf/ss）。", retryable=True)
        L.require_proper(s)
        return dict(sys=s, to=to, normalize=bool(args.get("normalize", True)))

    def estimate(self, P):
        s = P["sys"]
        return {"from": s.kind, "to": P["to"], "outputs": s.ny, "inputs": s.nu,
                "states": s.A.shape[0] if s.kind == "ss" else None}

    def execute(self, P):
        s, to = P["sys"], P["to"]
        warnings = list(s.warnings)
        extra = {}
        try:
            out, realization, conv = self._convert(s, to, P["normalize"], extra)
        except K.NoConvergence as e:
            raise Fail("E.NUMERICAL.NO_CONVERGENCE", f"特征值的 QR 迭代没有收敛：{e}", "/system",
                       remediation=[C.ask_user("系统矩阵可能病态或含 NaN 级的尺度差；先检查系数的尺度")])
        if realization in ("block_diagonal_controllable_canonical",) or (realization == "ss2tf_characteristic_polynomial" and not s.siso):
            warnings.append({"code": "W.DOMAIN.NONMINIMAL_REALIZATION",
                             "message": "MIMO 转换得到的是非最小实现（" + ("逐元素块对角" if realization.startswith("block") else "所有元素共享 det(sI−A)") + "），没有做零极点对消",
                             "path": "/system", "details": {"realization": realization}})
        # response consistency at fixed test points
        p0 = L.poles(s)
        pts = _test_points(s, p0)
        G0 = L.freqresp(s, pts)[0]
        G1 = L.freqresp(conv, pts)[0]
        scale = float(np.max(np.abs(G0))) if G0.size else 0.0
        diff = float(np.max(np.abs(G1 - G0))) if G0.size else 0.0
        disc = diff / scale if scale > 0 else diff
        if not math.isfinite(disc):
            disc = math.inf
        ok = disc <= DISCREPANCY
        if not ok:
            warnings.append({"code": "W.NUMERICAL.CONVERSION_DISCREPANCY",
                             "message": f"转换前后的频率响应最大相对差 {disc:.3g} > 1e-6（高阶或重根时多项式表示本身病态）",
                             "path": "/system", "details": {"response_max_rel_discrepancy": disc if math.isfinite(disc) else "Inf"}})
        result = {"system": out, "realization": realization, "minimal": None}
        n_out = conv.A.shape[0] if conv.kind == "ss" else None
        trust = {"convergence": {"applicable": False},
                 "residual": {"response_max_rel_discrepancy": disc,
                              "test_points": int(len(pts)),
                              "definition": "测试点：连续 s = jω，ω = 10^{−2, −1.5, …, 2}·max(1, max|p|)；离散 z = e^{jθ}，θ = π(k + ½)/9，k = 0…8；"
                                            "去掉离输入系统极点 ≤ 1e-8·max(1, |s|) 的点。差值 = max|G_out − G_in| / max|G_in|（全部元素与测试点）"},
                 "condition": {"from": s.kind, "to": to, "output_states": n_out},
                 "error_estimate": {"response_rel": disc,
                                    "basis": "后验：等于上述响应残差（非严格上界；对根与系数本身的误差不作保证）"},
                 "checks": {"response_consistent": bool(ok)},
                 "accuracy_grade": "C", "credentials": None}
        if extra:
            trust["derived"] = extra
        return result, trust, warnings

    @staticmethod
    def _convert(s, to, normalize, extra):
        ts = s.ts
        if s.kind == to:
            if to == "tf":
                num, den = s.num, s.den
                if normalize:
                    num = [[[x / s.den[i][j][0] for x in s.num[i][j]] for j in range(s.nu)] for i in range(s.ny)]
                    den = [[[x / s.den[i][j][0] for x in s.den[i][j]] for j in range(s.nu)] for i in range(s.ny)]
                conv = _make("tf", ts, s.ny, s.nu, num=num, den=den)
                return L.system_json("tf", ts, nested=not s.siso, num=num, den=den), "identity", conv
            if to == "ss":
                return L.system_json("ss", ts, A=s.A, B=s.B, Cm=s.C, D=s.D), "identity", s
            return L.system_json("zpk", ts, z=s.z, p=s.p, k=s.k), "identity", s
        if to == "ss":
            A, B, Cm, D, realization = L.to_ss(s)
            conv = _make("ss", ts, s.ny, s.nu, A=A, B=B, C=Cm, D=D)
            return L.system_json("ss", ts, A=A, B=B, Cm=Cm, D=D), realization, conv
        if to == "tf":
            if s.kind == "ss":
                num, den = L.ss2tf(s.A, s.B, s.C, s.D)
                realization = "ss2tf_characteristic_polynomial"
            else:
                n1, d1 = L.zpk2tf(s.z, s.p, s.k)
                num, den = [[n1]], [[d1]]
                realization = "expanded_product"
            conv = _make("tf", ts, s.ny, s.nu, num=num, den=den)
            return L.system_json("tf", ts, nested=not s.siso, num=num, den=den), realization, conv
        # to zpk (SISO)
        if s.kind == "tf":
            z, p, k = L.tf2zpk(s.num[0][0], s.den[0][0])
            realization = "polynomial_roots"
        else:
            z, p, k, info = L.ss2zpk(s.A, s.B, s.C, s.D)
            extra.update(info)
            realization = "invariant_zeros_eigenvalues_markov_gain"
        conv = _make("zpk", ts, 1, 1, z=z, p=p, k=k)
        return L.system_json("zpk", ts, z=z, p=p, k=k), realization, conv
