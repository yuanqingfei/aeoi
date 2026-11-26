目标共 102 个 MATLAB 函数，已覆盖 41 个（40%），由 37 个 AEOI 算子提供。共享内核 33 个：已有 25，待从算子中抽出 0，待新写 8。层号 = 1 + 依赖中最大的层号（无依赖为 0）；由 `roadmap/targets.py` 生成，改清单只改那个文件。

**共享内核**

| 层 | 内核 | 状态 | 依赖 | 内容 |
|---|---|---|---|---|
| 0 | `bracket` | 已有 | — | kernels::bracket：Brent zeroin（fminbnd 时加 Brent 极小） |
| 0 | `delaunay` | 待新写 | — | 二维 Delaunay 三角剖分（稳健谓词） |
| 0 | `filter` | 已有 | — | kernels::filter：直接 II 转置型、SOS 级联 |
| 0 | `libm` | 已有 | — | aeoi-libm：C 数学符号转给编入的 Rust libm |
| 0 | `rk` | 已有 | — | kernels::rk：Dormand–Prince 5(4)（同 scipy RK45）、稠密输出、刚性检测 |
| 0 | `sum` | 已有 | — | kernels::sum：TwoSum、TwoProduct、Dot2、Neumaier 补偿求和 |
| 1 | `dense` | 已有 | `sum` | kernels::dense：LU、QR、Cholesky、范数、1-范数逆估计、双倍精度残差 |
| 1 | `expr` | 已有 | `libm` | aeoi-numeric::expr：表达式求值与自动微分 |
| 1 | `fft` | 已有 | `libm` | kernels::fft：基 2 / 混合基 / Bluestein |
| 1 | `quad` | 已有 | `sum` | kernels::quad：GK(7,15)、全局自适应二分、MATLAB integral 的变量替换 |
| 1 | `stats` | 待新写 | `sum` | 补偿矩、排序与选择、分位数插值约定 |
| 1 | `window` | 待新写 | `libm` | 窗函数、分段平均（Welch） |
| 2 | `cdense` | 已有 | `dense` | kernels::cdense：复数 LU、复数奇异值、复数 QR |
| 2 | `expm` | 已有 | `dense` | linalg::expm：Padé 缩放平方 |
| 2 | `lp` | 已有 | `dense` | kernels::lp：单纯形 |
| 2 | `newton` | 已有 | `dense` | kernels::newton：Powell dogleg 信赖域、后验 Newton 修正与 κ₁(J) |
| 2 | `quasi` | 待新写 | `dense` | 拟 Newton（BFGS）与线搜索 |
| 2 | `schur` | 已有 | `dense` | kernels::schur：Hessenberg + Francis 双位移 QR |
| 2 | `spline` | 已有 | `dense` | kernels::spline：pchip / not-a-knot 斜率、dgtsv、PPoly 系数 |
| 2 | `svd` | 已有 | `dense` | kernels::svd：单边 Jacobi SVD |
| 3 | `bvp` | 待新写 | `newton`, `dense` | 边值问题：三阶段 Lobatto 配置 |
| 3 | `eig` | 已有 | `schur`, `cdense` | kernels::eig：对称 Jacobi、复 Schur、一般特征向量 |
| 3 | `logm` | 待新写 | `schur`, `expm` | 矩阵对数（逆缩放平方），d2c 需要 |
| 3 | `lsq` | 待新写 | `dense`, `svd` | 非线性最小二乘：Levenberg–Marquardt / 信赖域反射 |
| 3 | `qp` | 已有 | `lp`, `dense` | kernels::qp：有效集 QP |
| 3 | `stiff` | 待新写 | `newton`, `dense` | 刚性 ODE：NDF / BDF，带 Jacobian 复用 |
| 4 | `lyap` | 已有 | `eig`, `cdense`, `sum` | sysalg：Bartels–Stewart Lyapunov / Stein，双倍精度残差精化 |
| 4 | `poly` | 已有 | `eig` | aeoi-core::poly：Horner、乘法、求导、友矩阵求根 |
| 5 | `iir` | 已有 | `poly` | kernels::iir：Butterworth 原型、频率变换、双线性变换、zpk→tf/sos、设计流程 |
| 5 | `lti` | 已有 | `poly`, `eig`, `dense` | aeoi-core lti/sysalg：tf/ss/zpk 表示与互转、不变零点 |
| 5 | `riccati` | 已有 | `schur`, `svd`, `lyap` | kernels::riccati：CARE 符号函数 + Newton–Kleinman，DARE 的 SDA + Newton–Hewer |
| 6 | `freqresp` | 已有 | `lti`, `cdense` | kernels::freqresp：jω / e^{jωT} 求值、Horner 误差界、zpk、预解式、相位约定 |
| 6 | `timeresp` | 已有 | `lti`, `expm` | kernels::timeresp：零阶/一阶保持离散化（增广 expm）与递推 |

**目标函数**（同一层内的先后以后按 MATLAB 使用频率排）

| 层 | 类别 | MATLAB | AEOI 算子 | 状态 | 依赖 |
|---|---|---|---|---|---|
| 0 | 信号 | findpeaks | `signal.measure.findpeaks` | 待做 | — |
| 0 | 微积分与 ODE | gradient | `calculus.diff.gradient` | 待做 | — |
| 1 | 信号 | filter | `signal.filter.filter` | 已有 | `filter` |
| 1 | 微积分与 ODE | trapz | `calculus.quadrature.trapz` | 已有 | `sum` |
| 1 | 微积分与 ODE | cumtrapz | `calculus.quadrature.trapz` | 已有 | `sum` |
| 1 | 信号 | filtfilt | `signal.filter.filtfilt` | 待做 | `filter` |
| 1 | 多项式、插值、拟合 | griddata | `interp.scattered.griddata` | 待做 | `delaunay` |
| 2 | 信号 | fft/ifft | `signal.spectral.fft` | 已有 | `fft` |
| 2 | 微积分与 ODE | ode45 | `ode.ivp.solve` | 已有 | `rk`, `expr` |
| 2 | 微积分与 ODE | integral | `calculus.quadrature.integral` | 已有 | `quad`, `expr` |
| 2 | 线性代数 | mldivide/linsolve | `linalg.system.solve` | 已有 | `dense` |
| 2 | 非线性方程与优化 | fzero | `nonlinear.root.fzero` | 已有 | `bracket`, `expr` |
| 2 | 非线性方程与优化 | fminsearch | `optim.local.fminsearch` | 已有 | `expr` |
| 2 | 信号 | conv | `signal.filter.conv` | 待做 | `fft`, `sum` |
| 2 | 信号 | xcorr | `signal.spectral.xcorr` | 待做 | `fft` |
| 2 | 信号 | pwelch | `signal.spectral.pwelch` | 待做 | `window`, `fft` |
| 2 | 信号 | spectrogram | `signal.spectral.spectrogram` | 待做 | `window`, `fft` |
| 2 | 信号 | hilbert | `signal.spectral.hilbert` | 待做 | `fft` |
| 2 | 信号 | fir1 | `signal.design.fir1` | 待做 | `window` |
| 2 | 微积分与 ODE | ode23 | `ode.ivp.solve（method）` | 待做 | `rk`, `expr` |
| 2 | 微积分与 ODE | integral2 | `calculus.quadrature.integral2` | 待做 | `quad`, `expr` |
| 2 | 线性代数 | lu | `linalg.decomp.lu` | 待做 | `dense` |
| 2 | 线性代数 | qr | `linalg.decomp.qr` | 待做 | `dense` |
| 2 | 线性代数 | chol | `linalg.decomp.chol` | 待做 | `dense` |
| 2 | 线性代数 | inv | `linalg.matrix.inv` | 待做 | `dense` |
| 2 | 线性代数 | det | `linalg.matrix.det` | 待做 | `dense` |
| 2 | 统计 | mean | `stats.describe.moments` | 待做 | `stats` |
| 2 | 统计 | std/var | `stats.describe.moments` | 待做 | `stats` |
| 2 | 统计 | median | `stats.describe.quantile` | 待做 | `stats` |
| 2 | 统计 | quantile/prctile | `stats.describe.quantile` | 待做 | `stats` |
| 2 | 统计 | cov/corrcoef | `stats.describe.cov` | 待做 | `stats`, `dense` |
| 2 | 统计 | histcounts | `stats.describe.histcounts` | 待做 | `stats` |
| 2 | 非线性方程与优化 | fminbnd | `optim.scalar.fminbnd` | 待做 | `bracket`, `expr` |
| 3 | 多项式、插值、拟合 | polyfit | `fit.poly.polyfit` | 已有 | `dense`, `svd` |
| 3 | 多项式、插值、拟合 | interp1 | `interp.grid.interp1` | 已有 | `spline` |
| 3 | 控制 | ctrb/obsv | `control.analysis.ctrb_obsv` | 已有 | `svd` |
| 3 | 线性代数 | svd | `linalg.decomp.svd` | 已有 | `svd` |
| 3 | 非线性方程与优化 | fsolve | `nonlinear.system.fsolve` | 已有 | `newton`, `expr` |
| 3 | 非线性方程与优化 | linprog | `optim.linear.linprog` | 已有 | `lp` |
| 3 | 信号 | resample | `signal.rate.resample` | 待做 | **fir1**, `filter` |
| 3 | 多项式、插值、拟合 | spline/pchip | `interp.grid.spline` | 待做 | `spline` |
| 3 | 多项式、插值、拟合 | interp2 | `interp.grid.interp2` | 待做 | `spline` |
| 3 | 线性代数 | schur | `linalg.decomp.schur` | 待做 | `schur` |
| 3 | 线性代数 | norm/cond | `linalg.matrix.norm` | 待做 | `svd`, `dense` |
| 3 | 线性代数 | rank | `linalg.subspace.rank` | 待做 | `svd` |
| 3 | 线性代数 | null/orth | `linalg.subspace.basis` | 待做 | `svd` |
| 3 | 线性代数 | pinv | `linalg.matrix.pinv` | 待做 | `svd` |
| 3 | 线性代数 | lsqminnorm | `linalg.system.lstsq` | 待做 | `dense`, `svd` |
| 3 | 线性代数 | expm | `linalg.func.expm` | 待做 | `expm` |
| 3 | 统计 | regress | `stats.regression.regress` | 待做 | `dense`, `svd` |
| 3 | 非线性方程与优化 | fminunc | `optim.local.fminunc` | 待做 | `quasi`, `expr` |
| 4 | 控制 | place | `control.design.place` | 已有 | `cdense`, `svd`, `eig` |
| 4 | 线性代数 | eig | `linalg.decomp.eig` | 已有 | `eig` |
| 4 | 非线性方程与优化 | fmincon | `optim.local.fmincon` | 已有 | `qp`, `expr` |
| 4 | 多项式、插值、拟合 | lsqcurvefit | `fit.nonlinear.lsqcurvefit` | 待做 | `lsq`, `expr` |
| 4 | 微积分与 ODE | ode15s | `ode.ivp.solve（stiff）` | 待做 | `stiff`, `expr` |
| 4 | 微积分与 ODE | bvp4c | `ode.bvp.solve` | 待做 | `bvp`, `expr` |
| 4 | 微积分与 ODE | pdepe | `pde.parabolic.pdepe` | 待做 | `stiff` |
| 4 | 统计 | fitlm | `stats.regression.fitlm` | 待做 | **regress** |
| 4 | 非线性方程与优化 | quadprog | `optim.quadratic.quadprog` | 待做 | `qp` |
| 4 | 非线性方程与优化 | lsqnonlin | `optim.lsq.lsqnonlin` | 待做 | `lsq`, `expr` |
| 4 | 非线性方程与优化 | lsqlin | `optim.lsq.lsqlin` | 待做 | `qp`, `dense` |
| 5 | 控制 | lyap | `control.math.lyap` | 已有 | `lyap` |
| 5 | 信号 | freqz | `signal.design.freqz` | 待做 | `poly`, `fft` |
| 5 | 多项式、插值、拟合 | polyval | `fit.poly.polyval` | 待做 | `poly`, `sum` |
| 5 | 多项式、插值、拟合 | roots | `fit.poly.roots` | 待做 | `poly` |
| 5 | 多项式、插值、拟合 | deconv | `fit.poly.deconv` | 待做 | `poly` |
| 5 | 控制 | balreal | `control.lti.balreal` | 待做 | `lyap`, `svd` |
| 5 | 控制 | dlyap | `control.math.dlyap` | 待做 | `lyap` |
| 6 | 信号 | butter | `signal.design.butter` | 已有 | `iir` |
| 6 | 控制 | tf | `control.lti.convert` | 已有 | `lti` |
| 6 | 控制 | ss | `control.lti.convert` | 已有 | `lti` |
| 6 | 控制 | zpk | `control.lti.convert` | 已有 | `lti` |
| 6 | 控制 | pole | `control.lti.poles_zeros` | 已有 | `lti` |
| 6 | 控制 | zero | `control.lti.poles_zeros` | 已有 | `lti` |
| 6 | 控制 | feedback | `control.lti.feedback` | 已有 | `lti` |
| 6 | 控制 | series | `control.lti.series` | 已有 | `lti` |
| 6 | 控制 | parallel | `control.lti.parallel` | 已有 | `lti` |
| 6 | 控制 | c2d | `control.lti.c2d` | 已有 | `expm`, `lti` |
| 6 | 控制 | pade | `control.lti.pade` | 已有 | `lti`, `dense` |
| 6 | 控制 | lqr | `control.design.lqr` | 已有 | `riccati` |
| 6 | 控制 | dlqr | `control.design.dlqr` | 已有 | `riccati` |
| 6 | 控制 | pid | `control.design.pid` | 已有 | `lti` |
| 6 | 信号 | cheby1/cheby2 | `signal.design.cheby` | 待做 | `iir` |
| 6 | 信号 | decimate | `signal.rate.decimate` | 待做 | `iir`, **filtfilt** |
| 6 | 控制 | rlocus | `control.analysis.rlocus` | 待做 | `poly`, `lti` |
| 6 | 控制 | d2c | `control.lti.d2c` | 待做 | `logm`, `lti` |
| 6 | 控制 | minreal | `control.lti.minreal` | 待做 | `lti`, `svd` |
| 6 | 控制 | care/icare | `control.math.care` | 待做 | `riccati` |
| 6 | 控制 | dare/idare | `control.math.dare` | 待做 | `riccati` |
| 6 | 控制 | kalman/lqe | `control.design.kalman` | 待做 | `riccati` |
| 7 | 控制 | step | `control.lti.step` | 已有 | `timeresp` |
| 7 | 控制 | impulse | `control.lti.impulse` | 已有 | `timeresp` |
| 7 | 控制 | lsim | `control.lti.lsim` | 已有 | `timeresp` |
| 7 | 控制 | bode | `control.lti.bode` | 已有 | `freqresp` |
| 7 | 控制 | margin | `control.analysis.margin` | 已有 | `freqresp`, `poly` |
| 7 | 控制 | nyquist | `control.lti.nyquist` | 待做 | `freqresp` |
| 7 | 控制 | nichols | `control.lti.nichols` | 待做 | `freqresp` |
| 7 | 控制 | dcgain | `control.lti.dcgain` | 待做 | `lti`, `freqresp` |
| 8 | 控制 | stepinfo | `control.lti.stepinfo` | 已有 | **step** |
| 8 | 控制 | pidtune | `control.design.pidtune` | 已有 | **margin**, **pid** |
| 9 | 控制 | loopsens | `control.analysis.loopsens` | 已有 | **feedback**, **margin**, **stepinfo** |

| 类别 | 目标 | 已覆盖 |
|---|---|---|
| 控制 | 35 | 24 |
| 线性代数 | 15 | 3 |
| 信号 | 15 | 3 |
| 多项式、插值、拟合 | 9 | 2 |
| 微积分与 ODE | 10 | 4 |
| 非线性方程与优化 | 10 | 5 |
| 统计 | 8 | 0 |
| 合计 | 102 | 41（40%） |
