% gen_golden_bode.m — 生成 control.lti.bode 的 MATLAB golden 文件（AEOI v0.2-draft）
% 仅调用 MATLAB 公开 API；不读取、不反编译任何 MATLAB 内部实现（clean-room，见规范 §9）。
% 输出：golden/MATLAB/control.lti.bode/0.2.0/<sha256>.json
%
% ★ 内存序（规范 §3.4，MUST）：AEOI 的 data 是 row-major (C order) 展平；MATLAB 数组是 column-major。
%   A(:) 或 reshape(A,1,[]) 会得到 column-major 序 —— 对非对称矩阵这会静默得到 Aᵀ，结果错误且不报错。
%   所有矩阵必须经 aeoi_dense() 写出：data = reshape(A.', 1, [])（先转置再展平）。
%   本脚本对每个矩阵做往返自检（aeoi_dense_check），并包含一个非对称 ss 用例
%   (case_family = memory_order_nonsymmetric)，与 golden/python-control 中同名用例输入相同，可交叉比对。
%
% 本脚本尚未在 MATLAB 上运行过（box 上没有 MATLAB）；首次运行后请核对 memory_order 用例
% 与 python-control 参考值一致（T3 容差内），否则说明展平方向仍有错误。
function gen_golden_bode(outdir, n_cases, seed)
  if nargin < 1, outdir = fullfile('golden','MATLAB','control.lti.bode','0.2.0'); end
  if nargin < 2, n_cases = 200; end
  if nargin < 3, seed = 20261121; end
  rng(seed, 'twister');                     % 用例生成可复现
  if ~exist(outdir, 'dir'), mkdir(outdir); end
  v = ver('control');
  env = struct('engine', 'MATLAB', 'matlab', version, 'control_toolbox', v.Version, ...
               'blas', version('-blas'), 'lapack', version('-lapack'), ...
               'arch', computer('arch'), 'maxNumCompThreads', maxNumCompThreads);

  % ---- family 1: SISO 二阶扫描（无矩阵，tf 系数为向量）
  for i = 1:n_cases
    wn = 10^(-2 + 4*rand); zeta = 0.01 + 1.99*rand;
    num = wn^2; den = [1, 2*zeta*wn, wn^2];
    w = logspace(log10(wn) - 2, log10(wn) + 2, 50);   % 显式频点，避免依赖自动网格
    req = struct('system', struct('kind','tf','num',num,'den',den,'ts',0,'time_unit','s'), ...
                 'frequency', struct('mode','explicit','values',w,'unit','rad/s'));
    write_case(outdir, env, 'siso_2nd_order_sweep', req, tf(num, den), w);
  end

  % ---- family 2: memory_order_nonsymmetric（非对称 A，MIMO ss）
  A = [0 1 0; 0 0 1; -6 -11 -6];    % 伴随形，A ≠ Aᵀ
  B = [0 1; 0 0; 1 2];
  C = [1 0 0; 0 1 1];
  D = zeros(2, 2);
  w = logspace(-2, 2, 30);
  req = struct('system', struct('kind','ss','A',aeoi_dense(A),'B',aeoi_dense(B), ...
                                'C',aeoi_dense(C),'D',aeoi_dense(D),'ts',0,'time_unit','s'), ...
               'frequency', struct('mode','explicit','values',w,'unit','rad/s'));
  write_case(outdir, env, 'memory_order_nonsymmetric', req, ss(A, B, C, D), w);
end

function m = aeoi_dense(M)
  % row-major 展平：先转置再展平。禁止使用 M(:)。
  m = struct('dtype', 'f64', 'shape', size(M), 'data', reshape(M.', 1, []));
  aeoi_dense_check(m, M);
end

function aeoi_dense_check(m, M)
  % 往返自检：按 row-major 还原后必须与原矩阵逐元素相等
  R = reshape(m.data, m.shape(2), m.shape(1)).';
  assert(isequal(R, M), 'AEOI memory-order self-check failed (row-major flatten)');
end

function write_case(outdir, env, family, req, sys, w)
  [mag, ph] = bode(sys, w);                   % mag/ph: ny × nu × nw
  [ny, nu, nw] = size(mag);
  % 输出也按 AEOI 约定 [ny, nu, nw] row-major 展平：先 permute 成 (nw, nu, ny) 再 (:)，等价于 C 序
  magdb = 20*log10(mag);
  c = struct();
  c.spec = 'aeoi/0.2';
  c.operator = 'control.lti.bode';
  c.operator_version = '0.2.0';
  c.case_family = family;
  c.request = req;
  c.expected = struct('w', w, 'shape', [ny nu nw], ...
                      'magnitude_db', reshape(permute(magdb, [3 2 1]), 1, []), ...
                      'phase_deg',    reshape(permute(ph,    [3 2 1]), 1, []));
  c.reference_env = env;
  c.generated_at = char(datetime('now','TimeZone','UTC','Format','yyyy-MM-dd''T''HH:mm:ss''Z'''));
  txt = jsonencode(c, 'ConvertInfAndNaN', false);    % NaN/Inf 由后处理脚本转为 "NaN"/"Inf" 字符串
  h = sha256hex(txt);
  fid = fopen(fullfile(outdir, [h '.json']), 'w'); fwrite(fid, txt, 'char'); fclose(fid);
end

function h = sha256hex(txt)
  md = java.security.MessageDigest.getInstance('SHA-256');
  d = typecast(md.digest(uint8(txt)), 'uint8');
  h = lower(reshape(dec2hex(d)', 1, []));
end
