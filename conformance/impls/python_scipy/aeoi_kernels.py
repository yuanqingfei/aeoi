"""Numerical kernels shared by the control operators and fit.poly.roots (second implementation).

Everything here is this module's own code on top of numpy element-wise arithmetic (no LAPACK):
  * balance()        Parlett-Reinsch diagonal balancing with powers of 2 (exact scaling)
  * hessenberg()     Householder reduction to upper Hessenberg form
  * hqr()            Francis implicit double-shift QR on a Hessenberg matrix, eigenvalues only
                     (the textbook EISPACK-style algorithm: Wilkinson & Reinsch, Handbook II/14;
                     Golub & Van Loan, Matrix Computations, Alg. 7.5.1-7.5.2)
  * eigvals()        balance -> hessenberg -> hqr
  * lu_solve()       real Gaussian elimination with partial pivoting
  * lu_solve_batch() complex Gaussian elimination with partial pivoting, vectorised over a batch
  * expm()           scaling and squaring with Pade 3/5/7/9/13 (Higham 2005, Algorithm 2.3)
  * poly_from_roots(), polyval helpers, poly_roots() (balanced companion + Newton polishing)

Matrix products of small dense matrices use numpy's `@` (whatever BLAS numpy was built with);
the server pins BLAS to one thread.
"""
import math

import numpy as np

U = 2.0 ** -53


def gamma(k):
    """Higham's gamma_k = k u / (1 - k u)."""
    ku = k * U
    return ku / (1.0 - ku) if ku < 1 else math.inf


class NoConvergence(Exception):
    pass


class Singular(Exception):
    pass


# ------------------------------------------------------------------------------------------
# eigenvalues


def balance(a):
    """Diagonal similarity D^-1 A D with D a power of two (Parlett-Reinsch). Returns a new array."""
    a = np.array(a, dtype=float, copy=True)
    n = a.shape[0]
    radix, sqrdx = 2.0, 4.0
    done = False
    while not done:
        done = True
        for i in range(n):
            c = float(np.sum(np.abs(a[:, i]))) - abs(a[i, i])
            r = float(np.sum(np.abs(a[i, :]))) - abs(a[i, i])
            if c == 0.0 or r == 0.0:
                continue
            g = r / radix
            f = 1.0
            s = c + r
            while c < g:
                f *= radix
                c *= sqrdx
            g = r * radix
            while c > g:
                f /= radix
                c /= sqrdx
            if (c + r) / f < 0.95 * s:
                done = False
                a[i, :] *= 1.0 / f
                a[:, i] *= f
    return a


def hessenberg(a):
    """Householder reduction to upper Hessenberg form (similarity transform). Returns a new array."""
    h = np.array(a, dtype=float, copy=True)
    n = h.shape[0]
    for k in range(n - 2):
        x = h[k + 1:, k].copy()
        nx = float(np.linalg.norm(x))
        if nx == 0.0:
            continue
        alpha = -math.copysign(nx, x[0]) if x[0] != 0 else -nx
        v = x
        v[0] -= alpha
        nv = float(np.linalg.norm(v))
        if nv == 0.0:
            continue
        v /= nv
        h[k + 1:, k:] -= 2.0 * np.outer(v, v @ h[k + 1:, k:])
        h[:, k + 1:] -= 2.0 * np.outer(h[:, k + 1:] @ v, v)
        h[k + 2:, k] = 0.0
        h[k + 1, k] = alpha
    return h


def hqr(a, max_its=60):
    """Eigenvalues of an upper Hessenberg matrix by the Francis double-shift QR iteration.

    Complex conjugate pairs are returned as exact conjugates (x + p ± i z)."""
    a = np.array(a, dtype=float, copy=True)
    n = a.shape[0]
    wr = np.zeros(n)
    wi = np.zeros(n)
    if n == 0:
        return wr + 0j
    anorm = 0.0
    for i in range(n):
        anorm += float(np.sum(np.abs(a[i, max(i - 1, 0):])))
    nn = n - 1
    t = 0.0
    while nn >= 0:
        its = 0
        while True:
            # look for a single small subdiagonal element
            l = nn
            while l >= 1:
                s = abs(a[l - 1, l - 1]) + abs(a[l, l])
                if s == 0.0:
                    s = anorm
                if abs(a[l, l - 1]) + s == s:
                    a[l, l - 1] = 0.0
                    break
                l -= 1
            x = a[nn, nn]
            if l == nn:                                  # one root found
                wr[nn] = x + t
                wi[nn] = 0.0
                nn -= 1
                break
            y = a[nn - 1, nn - 1]
            w = a[nn, nn - 1] * a[nn - 1, nn]
            if l == nn - 1:                              # two roots found
                p = 0.5 * (y - x)
                q = p * p + w
                z = math.sqrt(abs(q))
                x += t
                if q >= 0.0:
                    z = p + math.copysign(z, p)
                    wr[nn - 1] = wr[nn] = x + z
                    if z != 0.0:
                        wr[nn] = x - w / z
                    wi[nn - 1] = wi[nn] = 0.0
                else:
                    wr[nn - 1] = wr[nn] = x + p
                    wi[nn - 1] = z
                    wi[nn] = -z
                nn -= 2
                break
            if its >= max_its:
                raise NoConvergence(f"QR iteration did not converge for eigenvalue {nn}")
            if its in (10, 20, 30, 40, 50):                # exceptional shift
                t += x
                for i in range(nn + 1):
                    a[i, i] -= x
                s = abs(a[nn, nn - 1]) + abs(a[nn - 1, nn - 2])
                x = y = 0.75 * s
                w = -0.4375 * s * s
            its += 1
            # form the shift and look for two consecutive small subdiagonal elements
            m = nn - 2
            while m >= l:
                z = a[m, m]
                r = x - z
                s = y - z
                p = (r * s - w) / a[m + 1, m] + a[m, m + 1]
                q = a[m + 1, m + 1] - z - r - s
                r = a[m + 2, m + 1]
                s = abs(p) + abs(q) + abs(r)
                p /= s
                q /= s
                r /= s
                if m == l:
                    break
                uu = abs(a[m, m - 1]) * (abs(q) + abs(r))
                vv = abs(p) * (abs(a[m - 1, m - 1]) + abs(z) + abs(a[m + 1, m + 1]))
                if uu + vv == vv:
                    break
                m -= 1
            for i in range(m + 2, nn + 1):
                a[i, i - 2] = 0.0
                if i != m + 2:
                    a[i, i - 3] = 0.0
            # double QR step on rows l..nn and columns m..nn
            k = m
            while k <= nn - 1:
                if k != m:
                    p = a[k, k - 1]
                    q = a[k + 1, k - 1]
                    r = a[k + 2, k - 1] if k != nn - 1 else 0.0
                    x = abs(p) + abs(q) + abs(r)
                    if x != 0.0:
                        p /= x
                        q /= x
                        r /= x
                s = math.copysign(math.sqrt(p * p + q * q + r * r), p)
                if s != 0.0:
                    if k == m:
                        if l != m:
                            a[k, k - 1] = -a[k, k - 1]
                    else:
                        a[k, k - 1] = -s * x
                    p += s
                    x = p / s
                    y = q / s
                    z = r / s
                    q /= p
                    r /= p
                    # row modification (columns k..nn)
                    if k != nn - 1:
                        pr = a[k, k:nn + 1] + q * a[k + 1, k:nn + 1] + r * a[k + 2, k:nn + 1]
                        a[k + 2, k:nn + 1] -= pr * z
                    else:
                        pr = a[k, k:nn + 1] + q * a[k + 1, k:nn + 1]
                    a[k + 1, k:nn + 1] -= pr * y
                    a[k, k:nn + 1] -= pr * x
                    # column modification (rows l..min(nn, k+3))
                    mmin = nn if nn < k + 3 else k + 3
                    if k != nn - 1:
                        pc = x * a[l:mmin + 1, k] + y * a[l:mmin + 1, k + 1] + z * a[l:mmin + 1, k + 2]
                        a[l:mmin + 1, k + 2] -= pc * r
                    else:
                        pc = x * a[l:mmin + 1, k] + y * a[l:mmin + 1, k + 1]
                    a[l:mmin + 1, k + 1] -= pc * q
                    a[l:mmin + 1, k] -= pc
                k += 1
    return wr + 1j * wi


def eigvals(a):
    a = np.asarray(a, dtype=float)
    n = a.shape[0]
    if n == 0:
        return np.zeros(0, dtype=complex)
    if n == 1:
        return np.array([complex(a[0, 0], 0.0)])
    return hqr(hessenberg(balance(a)))


def sort_complex(z):
    """Real part ascending, ties by imaginary part descending (3.10 R-2 / RT-3 order)."""
    z = np.asarray(z, dtype=complex)
    idx = sorted(range(len(z)), key=lambda i: (z[i].real, -z[i].imag))
    return np.array([z[i] for i in idx], dtype=complex), idx


# ------------------------------------------------------------------------------------------
# linear systems


def lu_solve(a, b):
    """Solve A X = B (real) by Gaussian elimination with partial pivoting. Raises Singular on a zero pivot."""
    a = np.array(a, dtype=float, copy=True)
    b = np.array(b, dtype=float, copy=True)
    vec = b.ndim == 1
    if vec:
        b = b.reshape(-1, 1)
    n = a.shape[0]
    for k in range(n):
        p = k + int(np.argmax(np.abs(a[k:, k])))
        if a[p, k] == 0.0:
            raise Singular(f"zero pivot in column {k}")
        if p != k:
            a[[k, p]] = a[[p, k]]
            b[[k, p]] = b[[p, k]]
        f = a[k + 1:, k] / a[k, k]
        a[k + 1:, k:] -= np.outer(f, a[k, k:])
        b[k + 1:] -= np.outer(f, b[k])
    x = np.zeros_like(b)
    for k in range(n - 1, -1, -1):
        x[k] = (b[k] - a[k, k + 1:] @ x[k + 1:]) / a[k, k]
    return x[:, 0] if vec else x


def lu_solve_batch(m, r):
    """Solve M[k] X[k] = R[k] for a batch (complex), partial pivoting per batch element.

    m: [K, n, n], r: [K, n, q]. Returns (X, singular_mask[K]). Singular systems get NaN."""
    m = np.array(m, dtype=complex, copy=True)
    r = np.array(r, dtype=complex, copy=True)
    K, n = m.shape[0], m.shape[1]
    sing = np.zeros(K, dtype=bool)
    ar = np.arange(K)
    for k in range(n):
        p = k + np.argmax(np.abs(m[:, k:, k]), axis=1)
        rows_k = m[ar, k, :].copy()
        m[ar, k, :] = m[ar, p, :]
        m[ar, p, :] = rows_k
        rk = r[ar, k, :].copy()
        r[ar, k, :] = r[ar, p, :]
        r[ar, p, :] = rk
        piv = m[:, k, k]
        z = piv == 0
        sing |= z
        piv = np.where(z, 1.0, piv)
        f = m[:, k + 1:, k] / piv[:, None]
        m[:, k + 1:, k:] -= f[:, :, None] * m[:, k:k + 1, k:]
        r[:, k + 1:, :] -= f[:, :, None] * r[:, k:k + 1, :]
    x = np.zeros_like(r)
    for k in range(n - 1, -1, -1):
        s = r[:, k, :] - np.sum(m[:, k, k + 1:, None] * x[:, k + 1:, :], axis=1)
        piv = np.where(m[:, k, k] == 0, 1.0, m[:, k, k])
        x[:, k, :] = s / piv[:, None]
    x[sing] = np.nan
    return x, sing


def inv_norm1_batch(m):
    """||M^-1||_1 for a batch of complex matrices (explicit inverse via lu_solve_batch)."""
    K, n = m.shape[0], m.shape[1]
    eye = np.broadcast_to(np.eye(n, dtype=complex), (K, n, n))
    x, sing = lu_solve_batch(m, eye)
    nrm = np.max(np.sum(np.abs(x), axis=1), axis=1) if n else np.zeros(K)
    nrm[sing] = np.inf
    return nrm


# ------------------------------------------------------------------------------------------
# matrix exponential (Higham 2005)

_PADE = {
    3: [120.0, 60.0, 12.0, 1.0],
    5: [30240.0, 15120.0, 3360.0, 420.0, 30.0, 1.0],
    7: [17297280.0, 8648640.0, 1995840.0, 277200.0, 25200.0, 1512.0, 56.0, 1.0],
    9: [17643225600.0, 8821612800.0, 2075673600.0, 302702400.0, 30270240.0, 2162160.0, 110880.0, 3960.0, 90.0, 1.0],
    13: [64764752532480000.0, 32382376266240000.0, 7771770303897600.0, 1187353796428800.0, 129060195264000.0,
         10559470521600.0, 670442572800.0, 33522128640.0, 1323241920.0, 40840800.0, 960960.0, 16380.0, 182.0, 1.0],
}
_THETA = {3: 1.495585217958292e-2, 5: 2.539398330063230e-1, 7: 9.504178996162932e-1, 9: 2.097847961257068e0,
          13: 5.371920351148152e0}


def expm(a):
    """exp(A) by scaling and squaring with a diagonal Pade approximant (Higham 2005, Alg. 2.3).

    Returns (E, info) with info = {"order", "squarings", "norm1"}."""
    a = np.asarray(a, dtype=float)
    n = a.shape[0]
    ident = np.eye(n)
    if n == 0:
        return np.zeros((0, 0)), {"order": 0, "squarings": 0, "norm1": 0.0}
    nrm = float(np.max(np.sum(np.abs(a), axis=0)))
    for mm in (3, 5, 7, 9):
        if nrm <= _THETA[mm]:
            b = _PADE[mm]
            a2 = a @ a
            pw = [ident, a2]
            for _ in range(2, mm // 2 + 1):
                pw.append(pw[-1] @ a2)
            uu = sum(b[2 * j + 1] * pw[j] for j in range(mm // 2 + 1))
            uu = a @ uu
            vv = sum(b[2 * j] * pw[j] for j in range(mm // 2 + 1))
            return lu_solve(vv - uu, vv + uu), {"order": mm, "squarings": 0, "norm1": nrm}
    s = max(0, int(math.ceil(math.log2(nrm / _THETA[13])))) if nrm > _THETA[13] else 0
    a = a / (2.0 ** s)
    b = _PADE[13]
    a2 = a @ a
    a4 = a2 @ a2
    a6 = a2 @ a4
    uu = a @ (a6 @ (b[13] * a6 + b[11] * a4 + b[9] * a2) + b[7] * a6 + b[5] * a4 + b[3] * a2 + b[1] * ident)
    vv = a6 @ (b[12] * a6 + b[10] * a4 + b[8] * a2) + b[6] * a6 + b[4] * a4 + b[2] * a2 + b[0] * ident
    e = lu_solve(vv - uu, vv + uu)
    for _ in range(s):
        e = e @ e
    return e, {"order": 13, "squarings": s, "norm1": nrm}


def matpow(a, k):
    """A^k by binary powering (k >= 0)."""
    n = a.shape[0]
    res = np.eye(n)
    base = np.array(a, dtype=float)
    while k > 0:
        if k & 1:
            res = res @ base
        k >>= 1
        if k:
            base = base @ base
    return res


# ------------------------------------------------------------------------------------------
# polynomials (coefficients in descending powers)


def strip_leading_zeros(c):
    """Remove exact leading zeros; keeps at least one coefficient. Returns (coeffs, removed)."""
    c = [float(x) for x in c]
    k = 0
    while k < len(c) - 1 and c[k] == 0.0:
        k += 1
    if k == len(c) - 1 and c[k] == 0.0:
        return [0.0], len(c) - 1
    return c[k:], k


def poly_from_roots(r):
    """Monic real polynomial with the given roots. Conjugate pairs are combined into real quadratics."""
    r = list(np.asarray(r, dtype=complex))
    c = np.array([1.0])
    used = [False] * len(r)
    for i, z in enumerate(r):
        if used[i]:
            continue
        used[i] = True
        if z.imag == 0.0:
            c = np.convolve(c, [1.0, -z.real])
            continue
        # find the partner conj(z) (nearest unused)
        best, bd = None, math.inf
        for j in range(i + 1, len(r)):
            if not used[j]:
                d = abs(r[j] - z.conjugate())
                if d < bd:
                    best, bd = j, d
        if best is None:
            # lone complex value: use its real part's quadratic with |z|^2 (keeps real coefficients)
            c = np.convolve(c, [1.0, -2.0 * z.real, z.real * z.real + z.imag * z.imag])
            continue
        used[best] = True
        c = np.convolve(c, [1.0, -2.0 * z.real, z.real * z.real + z.imag * z.imag])
    return c


def polyval_c(c, s):
    """Horner evaluation of a real polynomial at complex points s (array)."""
    s = np.asarray(s, dtype=complex)
    v = np.zeros_like(s)
    for a in c:
        v = v * s + a
    return v


def polyval_abs(c, t):
    """sum |a_k| t^k at nonnegative points t (Horner on |a_k|)."""
    t = np.asarray(t, dtype=float)
    v = np.zeros_like(t)
    for a in c:
        v = v * t + abs(a)
    return v


def polyder(c):
    m = len(c) - 1
    if m <= 0:
        return [0.0]
    return [c[k] * (m - k) for k in range(m)]


def poly_roots(q, polish_steps=4):
    """Roots of a real polynomial q (q[0] != 0, degree m >= 1): eigenvalues of the balanced companion
    matrix (own Francis QR) followed by at most `polish_steps` Newton steps per root that are accepted
    only when they decrease |q(z)|. Conjugate pairs stay exact conjugates."""
    q = [float(x) for x in q]
    m = len(q) - 1
    if m == 0:
        return np.zeros(0, dtype=complex)
    if m == 1:
        return np.array([complex(-q[1] / q[0], 0.0)])
    h = np.zeros((m, m))
    h[0, :] = [-x / q[0] for x in q[1:]]
    for i in range(1, m):
        h[i, i - 1] = 1.0
    z = hqr(balance(h))
    dq = polyder(q)
    out = np.array(z, dtype=complex)
    for i in range(m):
        if out[i].imag < 0.0:
            continue           # handled through its partner
        zi = out[i]
        f = polyval_c(q, np.array([zi]))[0]
        for _ in range(polish_steps):
            if f == 0:
                break
            d = polyval_c(dq, np.array([zi]))[0]
            if d == 0:
                break
            zn = zi - f / d
            if zi.imag == 0.0:
                zn = complex(zn.real, 0.0)
            fn = polyval_c(q, np.array([zn]))[0]
            if not (abs(fn) < abs(f)):
                break
            zi, f = zn, fn
        if out[i].imag > 0.0:
            # partner: the entry holding the exact conjugate of the unpolished value
            for j in range(m):
                if j != i and out[j] == out[i].conjugate():
                    out[j] = zi.conjugate()
                    break
        out[i] = zi
    return out
