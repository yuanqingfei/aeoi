"""signal.spectral.fft (profile 10.17 FT-1 .. FT-6), written from the profile text.

The transform is this module's own mixed-radix decimation-in-time FFT (smallest prime factor first,
direct DFT for primes <= 31, Bluestein's chirp-z for larger primes). Complex numbers are kept as two
float64 arrays and every complex product is spelled out as separate IEEE multiplies and adds, so no
operation can be contracted into an FMA or depend on numpy's SIMD dispatch for complex loops.
Twiddle factors come from Python's math.cos / math.sin (glibc) with the exponent j*k reduced mod n
in integer arithmetic first. It is NOT numpy.fft / pocketfft, which produced the python-control
golden values, so those cases test an independent code path.
"""
import math

import numpy as np

import aeoi_common as C
from aeoi_common import Fail

OP_ID = "signal.spectral.fft"
VERSION = "0.4.0"
METHOD_ID = "pyscipy:fft:mixed-radix-dit+bluestein"
MAX_INPUT = 10_000_000
MAX_OUTPUT = 10_000_000
DIRECT_PRIME_MAX = 31

# ------------------------------------------------------------------ the FFT kernel

_TW = {}


def _twiddles(n):
    """cos / sin of 2*pi*r/n for r = 0..n-1 (W^r = cos - i sin for the forward sign)."""
    t = _TW.get(n)
    if t is None:
        c = np.empty(n)
        s = np.empty(n)
        for r in range(n):
            th = 2.0 * math.pi * r / n
            c[r] = math.cos(th)
            s[r] = math.sin(th)
        t = (c, s)
        if n <= 1 << 16:
            _TW[n] = t
    return t


def _smallest_prime_factor(n):
    if n % 2 == 0:
        return 2
    f = 3
    while f * f <= n:
        if n % f == 0:
            return f
        f += 2
    return n


def _cmul(ar, ai, br, bi):
    return ar * br - ai * bi, ar * bi + ai * br


def _fft_forward(re, im):
    """Forward DFT (sign -1) along the last axis of (re, im)."""
    n = re.shape[-1]
    if n == 1:
        return re.copy(), im.copy()
    p = _smallest_prime_factor(n)
    if p == n:
        return _direct(re, im) if n <= DIRECT_PRIME_MAX else _bluestein(re, im)
    m = n // p
    lead = re.shape[:-1]
    # sub[..., q, j] = x[..., j*p + q]
    sr = np.ascontiguousarray(np.swapaxes(re.reshape(lead + (m, p)), -1, -2))
    si = np.ascontiguousarray(np.swapaxes(im.reshape(lead + (m, p)), -1, -2))
    yr, yi = _fft_forward(sr, si)
    reps = (1,) * (yr.ndim - 1) + (p,)
    tr = np.tile(yr, reps)          # tr[..., q, k] = Y_q[k mod m]
    ti = np.tile(yi, reps)
    c, s = _twiddles(n)
    k = np.arange(n)
    acc_r = tr[..., 0, :].copy()
    acc_i = ti[..., 0, :].copy()
    for q in range(1, p):
        idx = (q * k) % n
        wr, wi = c[idx], -s[idx]
        pr, pi = _cmul(tr[..., q, :], ti[..., q, :], wr, wi)
        acc_r = acc_r + pr
        acc_i = acc_i + pi
    return acc_r, acc_i


def _direct(re, im):
    n = re.shape[-1]
    c, s = _twiddles(n)
    k = np.arange(n)
    acc_r = np.zeros(re.shape)
    acc_i = np.zeros(re.shape)
    for j in range(n):
        idx = (j * k) % n
        pr, pi = _cmul(re[..., j:j + 1], im[..., j:j + 1], c[idx], -s[idx])
        acc_r = acc_r + pr
        acc_i = acc_i + pi
    return acc_r, acc_i


_CHIRP = {}


def _chirp(n):
    """c_j = exp(-i*pi*j^2/n), j^2 reduced mod 2n in integers."""
    t = _CHIRP.get(n)
    if t is None:
        cr = np.empty(n)
        ci = np.empty(n)
        for j in range(n):
            r = (j * j) % (2 * n)
            th = math.pi * r / n
            cr[j] = math.cos(th)
            ci[j] = -math.sin(th)
        t = (cr, ci)
        _CHIRP[n] = t
    return t


def _bluestein(re, im):
    n = re.shape[-1]
    M = 1
    while M < 2 * n - 1:
        M *= 2
    cr, ci = _chirp(n)
    lead = re.shape[:-1]
    ar = np.zeros(lead + (M,))
    ai = np.zeros(lead + (M,))
    ar[..., :n], ai[..., :n] = _cmul(re, im, cr, ci)
    br = np.zeros(M)
    bi = np.zeros(M)
    br[:n], bi[:n] = cr, -ci
    br[M - n + 1:], bi[M - n + 1:] = cr[1:][::-1], -ci[1:][::-1]
    Ar, Ai = _fft_forward(ar, ai)
    Br, Bi = _fft_forward(br, bi)
    Pr, Pi = _cmul(Ar, Ai, Br, Bi)
    # inverse FFT of size M by conjugation: ifft(z) = conj(fft(conj(z))) / M
    qr, qi = _fft_forward(Pr, -Pi)
    qr, qi = qr[..., :n] / M, -qi[..., :n] / M
    return _cmul(qr, qi, cr, ci)


def transform(re, im, inverse):
    if inverse:
        r, i = _fft_forward(re, -im)
        return r, -i
    return _fft_forward(re, im)


def _uses_bluestein(n):
    while n > 1:
        p = _smallest_prime_factor(n)
        if p == n:
            return n > DIRECT_PRIME_MAX
        n //= p
    return False


# ------------------------------------------------------------------ operator


def _manifest():
    errors = ["E.VALIDATION.SCHEMA", "E.VALIDATION.SHAPE", "E.VALIDATION.DTYPE", "E.VALIDATION.NON_FINITE",
              "E.VALIDATION.OPTION_CONFLICT", "E.VALIDATION.DIGEST_MISMATCH", "E.VALIDATION.REF_UNSUPPORTED",
              "E.VALIDATION.REF_FORBIDDEN", "E.VALIDATION.METHOD_UNAVAILABLE", "E.RESOURCE.INPUT_TOO_LARGE",
              "E.RESOURCE.OUTPUT_TOO_LARGE", "E.INTERNAL.PANIC"]
    return C.base_manifest(
        OP_ID, VERSION,
        title="Discrete Fourier transform (Python second implementation)",
        description="离散傅里叶变换（正/逆），同时返回每个输出频点的频率。约定按 10.17：符号 e^{−2πi·jk/n}；normalization backward（缺省）/ortho/forward；"
                    "sides two（缺省）/one（只用于实数正变换）；频率轴按 numpy fftfreq 顺序，不做 fftshift。",
        category="transform", tags=["signal", "spectral", "fft"],
        applicability={"preconditions": ["x 为 f64、i64（按实数处理）或 c128，形状 [n] 或 [m, n]，沿最后一维变换",
                                         "x 的全部元素有限", "sides = one 只用于实数输入的正变换"],
                       "not_for": ["非均匀采样", "三维及以上数组", "窗函数、功率谱密度估计"], "siso_only": False},
        provenance={"algorithm": "本实现自写的混合基按时间抽取 FFT：每层按最小素因子 p 分成 p 个子序列递归变换，再乘旋转因子合并；"
                                 "素数长度 ≤ 31 用直接 DFT，更大的素数用 Bluestein chirp-z（2 的幂长度的循环卷积）；"
                                 "复数乘法拆成独立的 IEEE 乘加，旋转因子由 math.cos/sin 计算且 j·k 先做整数取模；逆变换用共轭技巧。",
                    "references": [
                        {"type": "paper", "cite": "J. W. Cooley, J. W. Tukey, An algorithm for the machine calculation of complex Fourier series, Math. Comp. 19 (1965) 297–301", "doi": "10.1090/S0025-5718-1965-0178586-1"},
                        {"type": "paper", "cite": "L. Bluestein, A linear filtering approach to the computation of discrete Fourier transform, IEEE Trans. Audio Electroacoust. 18 (1970) 451–455", "doi": "10.1109/TAU.1970.1162132"},
                        {"type": "textbook", "cite": "N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM 2002, §24.1", "doi": "10.1137/1.9780898718027"}]},
        backend={"library": "aeoi-pyscipy op_fft (Python + numpy elementwise arithmetic)", "routine": "op_fft.transform",
                 "version": C.IMPL_VERSION, "binding": "native", "license": "MIT OR Apache-2.0"},
        determinism="platform_bitwise", tolerance_class="T1",
        sparse_support={"/x": {"policy": "reject"}},
        encodings={"input": ["json", "b64"], "output": ["json", "b64"]},
        limits={"max_input_elements": MAX_INPUT, "max_output_values": MAX_OUTPUT, "direct_dft_prime_max": DIRECT_PRIME_MAX,
                "dft_spot_check_bins": 8},
        accuracy_grade={"target": "B", "current": "C"},
        conformance={"target": "L2", "current": "L1"},
        errors=errors, warnings=[],
        receipt={"method_id": METHOD_ID,
                 "assumptions": [
                     {"id": "finite", "statement": "x 的全部元素有限", "enforced_by": "error:E.VALIDATION.NON_FINITE"},
                     {"id": "conventions", "statement": "符号、归一化、单双边与频率轴顺序按请求或 10.17 的缺省值", "enforced_by": "validation"},
                     {"id": "sides_real_forward", "statement": "单边谱只对实数输入的正变换给出", "enforced_by": "error:E.VALIDATION.OPTION_CONFLICT"},
                     {"id": "uniform_sampling", "statement": "x 是均匀采样序列，sample_rate_hz（若给出）是真实采样率", "enforced_by": "caller"},
                     {"id": "periodic_extension", "statement": "DFT 把（截断或补零后的）x 视为周期 n 的序列", "enforced_by": "caller"}],
                 "exactness": {"class": "approximate", "bound_kind": "a_priori"},
                 "postconditions": ["parseval", "dft_spot_check", "all_finite"],
                 "coverage": {"parseval": "full", "dft_spot_check": "sampled", "all_finite": "full"},
                 "guards": {"parseval": ["/X"], "dft_spot_check": ["/X"], "all_finite": ["/X"]}},
    )


class FFT(C.Operator):
    OP_ID = OP_ID
    VERSION = VERSION
    MANIFEST = _manifest()

    def prepare(self, args):
        if "x" not in args:
            raise Fail("E.VALIDATION.SCHEMA", "缺少 x", "/x")
        x = C.read_array(args["x"], "/x", allowed=("f64", "i64", "c128"), max_elements=MAX_INPUT)
        if len(x.shape) not in (1, 2):
            raise Fail("E.VALIDATION.SHAPE", f"x 的形状 {x.shape} 不支持：只接受 [n] 或 [m, n]（FT-2）", "/x/shape",
                       {"shape": x.shape}, [C.ask_user("把数据整理成 [n] 或 [m, n]（每行一个通道）")])
        L = x.shape[-1]
        n = args.get("n", L)
        if n < 1:
            raise Fail("E.VALIDATION.SHAPE", "x 的最后一维长度为 0，且没有给出 n", "/x/shape", {"shape": x.shape})
        direction = args.get("direction", "forward")
        normalization = args.get("normalization", "backward")
        sides = args.get("sides", "two")
        complex_in = x.dtype == "c128"
        if sides == "one" and (complex_in or direction == "inverse"):
            fields = ["sides"] + (["x"] if complex_in else []) + (["direction"] if direction == "inverse" else [])
            why = "复数输入（dtype c128）" if complex_in else "逆变换"
            raise Fail("E.VALIDATION.OPTION_CONFLICT",
                       f"sides = one 只用于实数输入的正变换；这里是{why}，负频率不是正频率的共轭，不能省略（FT-3、F-8）",
                       "/sides", {"conflicting_fields": fields, "sides_given": True},
                       [C.patch_remedy("two_sided", "去掉 sides，返回双边谱", [{"op": "remove", "path": "/sides"}],
                                       confidence="high", confirm=False)],
                       agent_hint="去掉 sides 得到双边谱；实数信号的单边谱要用 f64 数据做正变换")
        m = 1 if len(x.shape) == 1 else x.shape[0]
        n_out = n // 2 + 1 if sides == "one" else n
        if m * n_out > MAX_OUTPUT or m * n > MAX_INPUT:
            raise Fail("E.RESOURCE.INPUT_TOO_LARGE", f"变换规模 m·n = {m * n} 超过上限", "/n", {"limit": MAX_INPUT},
                       [C.ask_user("缩短变换长度或分段处理")])
        return dict(x=x, n=n, L=L, m=m, direction=direction, normalization=normalization, sides=sides,
                    complex_in=complex_in, fs=args.get("sample_rate_hz"), n_out=n_out)

    def estimate(self, P):
        n = P["n"]
        return {"transform_length": n, "channels": P["m"], "output_bins": P["n_out"],
                "flops_estimate": int(5 * n * max(1, math.ceil(math.log2(n))) * P["m"]) if n > 1 else P["m"],
                "bluestein": _uses_bluestein(n)}

    def execute(self, P):
        x, n, L = P["x"], P["n"], P["L"]
        vals = x.values.reshape((P["m"], L)) if len(x.shape) == 2 else x.values.reshape((1, L))
        if x.dtype == "c128":
            xr, xi = vals.real.astype(np.float64), vals.imag.astype(np.float64)
        else:
            xr, xi = vals.astype(np.float64), np.zeros(vals.shape)
        # FT-2: truncate or zero-pad at the end
        if n <= L:
            xr, xi = xr[:, :n].copy(), xi[:, :n].copy()
        else:
            xr = np.concatenate([xr, np.zeros((xr.shape[0], n - L))], axis=1)
            xi = np.concatenate([xi, np.zeros((xi.shape[0], n - L))], axis=1)
        inverse = P["direction"] == "inverse"
        Xr, Xi = transform(xr, xi, inverse)
        norm = P["normalization"]
        # FT-1 scale factor of the requested direction
        if norm == "ortho":
            sq = math.sqrt(n)
            Xr, Xi = Xr / sq, Xi / sq
            sc = 1.0 / sq
        elif (norm == "backward" and inverse) or (norm == "forward" and not inverse):
            Xr, Xi = Xr / n, Xi / n
            sc = 1.0 / n
        else:
            sc = 1.0
        n_out = P["n_out"]
        Xr, Xi = Xr[:, :n_out], Xi[:, :n_out]     # FT-3: one-sided is a prefix of the same transform
        # FT-4 frequency axis
        fs = P["fs"]
        k = np.arange(n_out)
        if P["sides"] == "two":
            kk = np.where(k < (n + 1) // 2, k, k - n).astype(np.float64)
        else:
            kk = k.astype(np.float64)
        freq = (kk * fs) / n if fs is not None else kk / n
        unit = "Hz" if fs is not None else "cycles/sample"
        # FT-6 post-conditions
        lg = math.ceil(math.log2(n)) if n > 1 else 0
        tol = 64 * (lg + 1) * C.U
        parseval_rel = 0.0
        spot_rel = 0.0
        bins = sorted({b for b in (0, 1, n - 1, n // 2, n // 3, n // 4, (2 * n) // 3, (3 * n) // 4) if 0 <= b < n_out})
        c, s = _twiddles(n)
        sgn = 1.0 if inverse else -1.0
        for ch in range(xr.shape[0]):
            e_in = math.fsum((xr[ch] * xr[ch]).tolist() + (xi[ch] * xi[ch]).tolist())
            mag2 = Xr[ch] * Xr[ch] + Xi[ch] * Xi[ch]
            if P["sides"] == "one":
                w = np.full(n_out, 2.0)
                w[0] = 1.0
                if n % 2 == 0:
                    w[-1] = 1.0
                mag2 = mag2 * w
            e_out = math.fsum(mag2.tolist())
            ref = sc * sc * n * e_in
            if ref > 0:
                parseval_rel = max(parseval_rel, abs(e_out - ref) / ref)
            elif e_out != 0:
                parseval_rel = math.inf
            absum = math.fsum(np.hypot(xr[ch], xi[ch]).tolist())
            j = np.arange(n)
            for b in bins:
                idx = (j * b) % n
                wr, wi = c[idx], sgn * s[idx]
                p1, e1 = C.two_prod(xr[ch], wr)
                p2, e2 = C.two_prod(xi[ch], wi)
                p3, e3 = C.two_prod(xr[ch], wi)
                p4, e4 = C.two_prod(xi[ch], wr)
                dr = math.fsum(np.concatenate([p1, e1, -p2, -e2]).tolist()) * sc
                di = math.fsum(np.concatenate([p3, e3, p4, e4]).tolist()) * sc
                diff = math.hypot(Xr[ch, b] - dr, Xi[ch, b] - di)
                den = sc * absum
                spot_rel = max(spot_rel, diff / den if den > 0 else (0.0 if diff == 0 else math.inf))
        finite = bool(np.all(np.isfinite(Xr)) and np.all(np.isfinite(Xi)))
        blue = _uses_bluestein(n)
        bound = (30.0 if blue else 10.0) * max(1, lg) * C.U
        out_shape = (n_out,) if len(x.shape) == 1 else (P["m"], n_out)
        result = {
            "X": C.dense_c128(Xr.reshape(out_shape), Xi.reshape(out_shape)),
            "frequency": {"unit": unit, "value": C.dense_f64(freq)},
            "n": n, "direction": P["direction"], "normalization": norm, "sides": P["sides"], "input_length": L,
            "frequency_resolution": {"unit": unit, "value": (fs / n) if fs is not None else 1.0 / n},
        }
        trust = {
            "convergence": {"applicable": False},
            "residual": {"parseval_rel": parseval_rel, "dft_spot_rel": spot_rel,
                         "definition": "parseval_rel = |Σw|X|² − sc²·n·Σ|x|²| / (sc²·n·Σ|x|²)（各通道最大，单边镜像频点 w = 2）；"
                                       "dft_spot_rel = max |X_k − 直接 DFT_k| / (sc·Σ|x_j|)（直接 DFT 用无误差乘积 + 精确求和）"},
            "condition": {"n": n, "channels": P["m"], "complex_input": P["complex_in"], "bluestein": blue},
            "error_estimate": {"X_rel_normwise": bound,
                               "basis": "先验估计 ‖ΔX‖₂/‖X‖₂ ≲ c·⌈log₂n⌉·u（Higham 2002 §24.1，基 2 时为严格界）；c 取 10，用 Bluestein 时取 30；非严格上界"},
            "checks": {"parseval": bool(parseval_rel <= tol), "dft_spot_check": bool(spot_rel <= tol), "all_finite": finite},
            "accuracy_grade": "C", "credentials": None,
            "derived": {"scale": sc, "spot_bins": bins, "tolerance": tol},
        }
        return result, trust, []
