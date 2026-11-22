"""Source of truth for v0.2 manifests and I/O schemas (examples/<id>/{manifest,input.schema,output.schema}.json).
The Rust crates embed these files at compile time."""
import json, os
SPEC="aeoi/0.2"; C="aeoi://schemas/common/0.2/"
def w(p,o):
    os.makedirs(os.path.dirname(p),exist_ok=True); json.dump(o,open(p,"w"),indent=2,ensure_ascii=False); open(p,"a").write("\n")
CLEAN={"declared":True,"sources":"仅公开文献；MATLAB 仅作为黑盒参考（公开 API），v0.2 的 parity 参考为 python-control/scipy"}
BACK=lambda routine:{"library":"aeoi-control (pure Rust) + nalgebra 0.33","routine":routine,"version":"0.2.0","binding":"native","license":"MIT OR Apache-2.0"}
ENC={"input":["json","b64","ref"],"output":["json","b64"]}
ANN={"readOnlyHint":True,"idempotentHint":True,"openWorldHint":False}
SS_SPARSE={p:{"policy":"densify","max_dense_elements":1000000} for p in ["/system/A","/system/B","/system/C","/system/D"]}
COMMON_LTI_ERR=["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.ZERO_DENOMINATOR","E.VALIDATION.NOT_CONJUGATE",
 "E.VALIDATION.SPARSE_STRUCTURE","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN","E.RESOURCE.INPUT_TOO_LARGE","E.DOMAIN.IMPROPER_SYSTEM","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"]
bode={"spec":SPEC,"id":"control.lti.bode","version":"0.2.0","title":"Bode frequency response",
 "description":"计算连续或离散 LTI 系统（tf/ss/zpk，SISO 或 MIMO）在给定频率点上的幅值（dB）与相位（deg）。",
 "category":"analysis","tags":["frequency-domain","lti"],
 "applicability":{"preconditions":["system 为 proper（分子阶次 ≤ 分母阶次；zpk 零点数 ≤ 极点数）","频率点为正、严格递增","离散系统要求 ω ≤ π/ts（Nyquist 频率）"],
   "not_for":["非线性系统","时变系统","带时滞的系统（v0.2 不支持）"],"siso_only":False},
 "provenance":{"algorithm":"tf：分子/分母多项式在 s=jω（离散 z=e^{jωts}）处 Horner 复数求值后相除；zpk：乘积形式 k·Π(s−z)/Π(s−p)；ss：每个频点对 (sI−A)X=B 做部分选主元复数 LU 求解，G=CX+D。相位取 atan2，按频率顺序展开（unwrap），首点落在 (c−180°, c+180°]",
   "references":[{"type":"textbook","cite":"N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM 2002, §5.1（Horner 误差界与条件数）","doi":"10.1137/1.9780898718027"},
     {"type":"textbook","cite":"G. F. Franklin, J. D. Powell, A. Emami-Naeini, Feedback Control of Dynamic Systems, ch.6"},
     {"type":"paper","cite":"A. J. Laub, Efficient multivariable frequency response computations, IEEE TAC 26(2), 1981（Hessenberg 加速，v0.2 未采用）","doi":"10.1109/TAC.1981.1102605"}],
   "clean_room":CLEAN},
 "backend":BACK("bode::evaluate (Horner / zpk product / complex LU)"),
 "determinism":"platform_bitwise","tolerance_class":"T3",
 "sparse_support":SS_SPARSE,"encodings":ENC,
 "limits":{"max_frequency_points":100000,"max_states_for_svd_condition":64},
 "input_schema":{"$ref":"./input.schema.json"},"output_schema":{"$ref":"./output.schema.json"},
 "conformance":{"target":"L3","current":"L2"},"accuracy_grade":{"target":"A","current":"B"},
 "credentials":"aeoi://operators/control.lti.bode/0.2.0/credentials",
 "errors":COMMON_LTI_ERR+["E.VALIDATION.GRID","E.VALIDATION.RANGE","E.DOMAIN.ABOVE_NYQUIST","E.NUMERICAL.POLE_ON_GRID"],
 "warnings":["W.NUMERICAL.NEAR_POLE","W.NUMERICAL.ZERO_ON_GRID","W.VALIDATION.LEADING_ZEROS_STRIPPED","W.PERFORMANCE.SPARSE_DENSIFIED"],
 "mcp":{"tool_name":"control_lti_bode","annotations":ANN}}
bode_in={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.lti.bode/0.2.0/input",
 "type":"object","additionalProperties":False,"required":["system","frequency"],
 "properties":{"system":{"$ref":C+"LtiSystem"},
  "frequency":{"oneOf":[
    {"type":"object","additionalProperties":False,"required":["mode","values","unit"],"properties":{
      "mode":{"const":"explicit"},"values":{"type":"array","items":{"type":"number","exclusiveMinimum":0},"minItems":1,"maxItems":100000,"description":"必须严格递增"},
      "unit":{"enum":["rad/s","Hz"]}}},
    {"type":"object","additionalProperties":False,"required":["mode","min","max","n","unit"],"properties":{
      "mode":{"const":"logspace"},"min":{"type":"number","exclusiveMinimum":0},"max":{"type":"number","exclusiveMinimum":0},
      "n":{"type":"integer","minimum":2,"maximum":100000},"unit":{"enum":["rad/s","Hz"]}}}]},
  "phase":{"type":"object","additionalProperties":False,"properties":{
     "unwrap":{"type":"boolean","default":True},
     "branch_center_deg":{"type":"number","default":0,"description":"首个频点相位落在 (c-180, c+180] 内；unwrap=false 时每个点都落在该区间"}}},
  "exec":{"$ref":C+"ExecOptions"}}}
Q={"$ref":C+"Quantity"}
bode_out={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.lti.bode/0.2.0/output",
 "type":"object","required":["w","magnitude","phase"],"properties":{
  "w":dict(Q,description="频率，unit 固定为 rad/s（Hz 输入会被换算）"),
  "magnitude":dict(Q,description="shape [ny,nu,nw]，unit=dB（20·log10|G|）；|G|=0 时为 \"-Inf\""),
  "phase":dict(Q,description="shape [ny,nu,nw]，unit=deg；|G|=0 时为 \"NaN\"")}}
step={"spec":SPEC,"id":"control.lti.step","version":"0.2.0","title":"Step response",
 "description":"计算 LTI 系统在均匀时间网格上的单位阶跃响应（每个输入通道分别施加单位阶跃），并给出阶跃特性指标。",
 "category":"analysis","tags":["time-domain","lti"],
 "applicability":{"preconditions":["system 为 proper","uniform 网格：t_k = k·dt，k = 0..N−1，N = floor(t_final/dt + 1e-9) + 1","离散系统要求 dt = ts（相对误差 1e-9）","step_info 需系统渐近稳定，否则为 null 并给出 warning"],
   "not_for":["非线性系统","任意输入（将来用 control.lti.lsim）","带时滞的系统（v0.2 不支持）"],"siso_only":False},
 "provenance":{"algorithm":"连续系统：对增广矩阵 [[A,B],[0,0]]·dt 求矩阵指数（Padé 13 阶 + scaling & squaring）得 Φ、Γ，再递推 x_{k+1}=Φx_k+Γu、y_k=Cx_k+Du；阶跃输入在 ZOH 下无离散化误差。离散系统直接递推。tf/zpk 先转为可控标准型实现。指标按网格点定义，不插值（与 python-control step_info 相同）",
   "references":[{"type":"paper","cite":"C. Van Loan, Computing integrals involving the matrix exponential, IEEE TAC 23(3), 1978","doi":"10.1109/TAC.1978.1101743"},
     {"type":"paper","cite":"N. J. Higham, The scaling and squaring method for the matrix exponential revisited, SIMAX 26(4), 2005","doi":"10.1137/04061101X"},
     {"type":"textbook","cite":"K. Ogata, Modern Control Engineering, 5th ed., ch.5（阶跃指标定义）"}],
   "clean_room":CLEAN},
 "backend":BACK("step::simulate (expm Padé-13 + recursion)"),
 "determinism":"platform_bitwise","tolerance_class":"T2",
 "sparse_support":SS_SPARSE,"encodings":ENC,
 "limits":{"max_grid_points":10000000,"max_output_values":1000000},
 "input_schema":{"$ref":"./input.schema.json"},"output_schema":{"$ref":"./output.schema.json"},
 "conformance":{"target":"L3","current":"L2"},"accuracy_grade":{"target":"A","current":"B"},
 "credentials":"aeoi://operators/control.lti.step/0.2.0/credentials",
 "errors":COMMON_LTI_ERR+["E.VALIDATION.GRID","E.VALIDATION.RANGE","E.RESOURCE.COMPUTE_LIMIT","E.RESOURCE.OUTPUT_TOO_LARGE"],
 "warnings":["W.DOMAIN.UNSTABLE_SYSTEM","W.DOMAIN.NOT_SETTLED","W.DOMAIN.INFINITE_DC_GAIN","W.DOMAIN.ZERO_DC_GAIN","W.ACCURACY.GRID_TOO_COARSE","W.ACCURACY.AUTO_GRID","W.VALIDATION.LEADING_ZEROS_STRIPPED","W.PERFORMANCE.SPARSE_DENSIFIED"],
 "mcp":{"tool_name":"control_lti_step","annotations":ANN}}
step_in={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.lti.step/0.2.0/input","type":"object","additionalProperties":False,
 "required":["system","time"],"properties":{"system":{"$ref":C+"LtiSystem"},
 "time":{"oneOf":[{"type":"object","additionalProperties":False,"required":["mode","t_final","dt","unit"],"properties":{"mode":{"const":"uniform"},"t_final":{"type":"number","exclusiveMinimum":0},"dt":{"type":"number","exclusiveMinimum":0},"unit":{"const":"s"}}},
   {"type":"object","additionalProperties":False,"required":["mode","unit"],"properties":{"mode":{"const":"auto"},"t_final":{"type":["number","null"],"exclusiveMinimum":0},"unit":{"const":"s"}},"description":"启发式网格（见 manifest），不参与 parity 测试"}]},
 "step_info":{"type":"object","additionalProperties":False,"properties":{"enabled":{"type":"boolean","default":True},
   "rise_time_limits":{"type":"array","items":{"type":"number","minimum":0,"maximum":1},"minItems":2,"maxItems":2,"default":[0.1,0.9]},
   "settling_threshold":{"type":"number","exclusiveMinimum":0,"maximum":0.5,"default":0.02}}},
 "output":{"type":"object","additionalProperties":False,"properties":{"return_samples":{"enum":["all","decimated","none"],"default":"all"},"max_samples":{"type":"integer","minimum":2,"default":10001}}},
 "exec":{"$ref":C+"ExecOptions"}}}
step_out={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.lti.step/0.2.0/output","type":"object","required":["t","y","info"],
 "properties":{"t":{"oneOf":[{"type":"null"},Q]},"y":{"oneOf":[{"type":"null"},dict(Q,description="shape [ny,nu,nt]")]},
 "n_grid_points":{"type":"integer"},
 "info":{"type":["object","array","null"],"description":"SISO 为单个对象；MIMO 为 [ny][nu] 嵌套数组，元素可为 null（该通道不稳定或 DC 增益无穷）"}}}
lqr={"spec":SPEC,"id":"control.design.lqr","version":"0.2.0","title":"Continuous-time LQR",
 "description":"求解连续时间代数 Riccati 方程 (CARE)，返回最优状态反馈增益 K（u=−Kx）、Riccati 解 S 与闭环极点，使 J=∫(xᵀQx+uᵀRu+2xᵀNu)dt 最小。",
 "category":"design","tags":["optimal-control","riccati","lti"],
 "applicability":{"preconditions":["R 对称正定","Q 对称；[[Q,N],[Nᵀ,R]] 对称半正定","(A,B) 可镇定","Hamiltonian 矩阵在虚轴上无特征值（等价于 (Q−NR⁻¹Nᵀ, A−BR⁻¹Nᵀ) 在虚轴上无不可观模态）"],
   "not_for":["离散系统（将来用 control.design.dlqr）","有约束的 MPC"],"siso_only":False},
 "provenance":{"algorithm":"消去交叉项后构造 Hamiltonian 矩阵 H=[[A,−BR⁻¹Bᵀ],[−Q,−Aᵀ]]；用带行列式缩放的 Newton 迭代计算矩阵符号函数 sign(H)，由 (sign(H)+I) 的零空间得稳定不变子空间并最小二乘解出 S；随后做至多 3 步 Newton–Kleinman 修正（n ≤ 32，Kronecker 形式解 Lyapunov 方程），残差不降即停",
   "references":[{"type":"paper","cite":"J. D. Roberts, Linear model reduction and solution of the algebraic Riccati equation by use of the sign function, Int. J. Control 32(4), 1980","doi":"10.1080/00207178008922881"},
     {"type":"paper","cite":"R. Byers, Solving the algebraic Riccati equation with the matrix sign function, Linear Algebra Appl. 85, 1987","doi":"10.1016/0024-3795(87)90222-9"},
     {"type":"paper","cite":"C. S. Kenney, A. J. Laub, The matrix sign function, IEEE TAC 40(8), 1995","doi":"10.1109/9.402226"},
     {"type":"paper","cite":"D. L. Kleinman, On an iterative technique for Riccati equation computations, IEEE TAC 13(1), 1968","doi":"10.1109/TAC.1968.1098829"},
     {"type":"textbook","cite":"B. D. O. Anderson, J. B. Moore, Optimal Control: Linear Quadratic Methods, 1990"}],
   "clean_room":CLEAN},
 "backend":BACK("lqr::solve_care (matrix sign function + Newton–Kleinman refinement)"),
 "determinism":"platform_bitwise","tolerance_class":"T2",
 "sparse_support":{p:{"policy":"densify","max_dense_elements":1000000} for p in ["/A","/B","/Q","/R","/N"]},"encodings":ENC,
 "limits":{"max_states":200,"max_states_for_refinement":32,"max_sign_iterations":100},
 "input_schema":{"$ref":"./input.schema.json"},"output_schema":{"$ref":"./output.schema.json"},
 "conformance":{"target":"L3","current":"L2"},"accuracy_grade":{"target":"A","current":"B"},
 "credentials":"aeoi://operators/control.design.lqr/0.2.0/credentials",
 "errors":["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.NOT_SYMMETRIC","E.VALIDATION.SPARSE_STRUCTURE","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN",
   "E.DOMAIN.R_NOT_POSITIVE_DEFINITE","E.DOMAIN.WEIGHT_NOT_PSD","E.DOMAIN.NOT_STABILIZABLE","E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG","E.NUMERICAL.NO_CONVERGENCE","E.NUMERICAL.SINGULAR","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 "warnings":["W.NUMERICAL.LARGE_RESIDUAL","W.NUMERICAL.NEAR_UNCONTROLLABLE","W.NUMERICAL.ILL_CONDITIONED","W.VALIDATION.SYMMETRIZED","W.PERFORMANCE.SPARSE_DENSIFIED"],
 "mcp":{"tool_name":"control_design_lqr","annotations":ANN}}
lqr_in={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.design.lqr/0.2.0/input","type":"object","additionalProperties":False,
 "required":["A","B","Q","R"],"properties":{k:{"$ref":C+"Matrix"} for k in ["A","B","Q","R","N"]}|{
  "symmetry_tolerance":{"type":"number","exclusiveMinimum":0,"default":1e-10,"description":"相对容差 ‖X−Xᵀ‖_F ≤ tol·‖X‖_F；超出即 E.VALIDATION.NOT_SYMMETRIC，范围内（但非精确对称）对称化并给 W.VALIDATION.SYMMETRIZED"},
  "exec":{"$ref":C+"ExecOptions"}},
 "x-aeoi-shape-constraints":["A: [n,n]","B: [n,m]","Q: [n,n]","R: [m,m]","N: [n,m]，缺省为 0"]}
M={"$ref":C+"Matrix"}
lqr_out={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.design.lqr/0.2.0/output","type":"object","required":["K","S","closed_loop_poles"],
 "properties":{"K":dict(M,description="[m,n]，u=−Kx"),"S":dict(M,description="[n,n] 对称半正定"),
 "closed_loop_poles":dict(M,description="c128 向量 [n]，按实部升序、虚部降序排列")}}
for op,(m,i,o) in {"control.lti.bode":(bode,bode_in,bode_out),"control.lti.step":(step,step_in,step_out),"control.design.lqr":(lqr,lqr_in,lqr_out)}.items():
    d=f"examples/{op}/"; w(d+"manifest.json",m); w(d+"input.schema.json",i); w(d+"output.schema.json",o)
print("ok")
