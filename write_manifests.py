"""Source of truth for v0.2 manifests and I/O schemas (examples/<id>/{manifest,input.schema,output.schema}.json).
The Rust crates embed these files at compile time."""
import json, os
SPEC="aeoi/0.3"; C="aeoi://schemas/common/0.3/"
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

# =====================================================================================  batch 2 (v0.3)
V3="0.3.0"
BACK3=lambda routine:{"library":"aeoi-control (pure Rust) + nalgebra 0.33","routine":routine,"version":V3,"binding":"native","license":"MIT OR Apache-2.0"}
MAT_ERR=["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.SPARSE_STRUCTURE","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN"]
TAIL_ERR=["E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"]
LW=["W.VALIDATION.LEADING_ZEROS_STRIPPED","W.PERFORMANCE.SPARSE_DENSIFIED"]
CVEC=lambda d:dict(M,description=d)
LTI={"$ref":C+"LtiSystem"}
EXEC={"$ref":C+"ExecOptions"}
CPLX_LIST={"type":"array","items":{"$ref":C+"Complex"}}
def man(id_,title,desc,cat,tags,pre,notfor,algo,refs,routine,tol,limits,errors,warnings,sparse=SS_SPARSE,siso_only=False,det="platform_bitwise"):
    return {"spec":SPEC,"id":id_,"version":V3,"title":title,"description":desc,"category":cat,"tags":tags,
     "applicability":{"preconditions":pre,"not_for":notfor,"siso_only":siso_only},
     "provenance":{"algorithm":algo,"references":refs,"clean_room":CLEAN},
     "backend":BACK3(routine),"determinism":det,"tolerance_class":tol,"sparse_support":sparse,"encodings":ENC,"limits":limits,
     "input_schema":{"$ref":"./input.schema.json"},"output_schema":{"$ref":"./output.schema.json"},
     "conformance":{"target":"L3","current":"L2"},"accuracy_grade":{"target":"A","current":"B"},
     "credentials":f"aeoi://operators/{id_}/{V3}/credentials","errors":errors,"warnings":warnings,
     "mcp":{"tool_name":id_.replace(".","_"),"annotations":ANN}}
def ins(id_,props,req,extra=None):
    o={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":f"aeoi://schemas/{id_}/{V3}/input","type":"object","additionalProperties":False,"required":req,"properties":dict(props,exec=EXEC)}
    if extra: o.update(extra)
    return o
def outs(id_,props,req):
    return {"$schema":"https://json-schema.org/draft/2020-12/schema","$id":f"aeoi://schemas/{id_}/{V3}/output","type":"object","required":req,"properties":props}
R_HIGHAM={"type":"paper","cite":"N. J. Higham, The scaling and squaring method for the matrix exponential revisited, SIMAX 26(4), 2005","doi":"10.1137/04061101X"}
R_EVD={"type":"paper","cite":"A. Emami-Naeini, P. Van Dooren, Computation of zeros of linear multivariable systems, Automatica 18(4), 1982","doi":"10.1016/0005-1098(82)90070-X"}
R_BS={"type":"paper","cite":"R. H. Bartels, G. W. Stewart, Solution of the matrix equation AX + XB = C, Comm. ACM 15(9), 1972","doi":"10.1145/361573.361582"}
R_KAI={"type":"textbook","cite":"T. Kailath, Linear Systems, Prentice-Hall 1980（实现、可控/可观、PBH 判据）"}
R_GVL={"type":"textbook","cite":"G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., 2013（SVD 数值秩、Schur 分解）"}
R_BAL={"type":"paper","cite":"B. N. Parlett, C. Reinsch, Balancing a matrix for calculation of eigenvalues and eigenvectors, Numer. Math. 13, 1969","doi":"10.1007/BF02165404"}
R_FPE={"type":"textbook","cite":"G. F. Franklin, J. D. Powell, M. Workman, Digital Control of Dynamic Systems, 3rd ed., 1998（ZOH/FOH/Tustin 与预畸变）"}
REAL_NOTE="tf→ss：可控标准型（controller canonical form，A 首行 = −den[1:]/den[0]，B = e₁，与 scipy.signal.tf2ss 及 MATLAB tf2ss 形式相同）；MIMO tf→ss：逐元素可控标准型块对角拼接（非最小实现）"

# ---- control.lti.convert
convert=man("control.lti.convert","LTI representation conversion","在 tf / ss / zpk 三种表示之间转换 LTI 系统，并给出实现约定与转换残差（转换前后频率响应的最大相对差）。","transform",["lti","realization"],
 ["system 为 proper","zpk 只表示 SISO 系统（v0.3）：MIMO → zpk 返回 E.DOMAIN.MIMO_NOT_SUPPORTED"],["最小实现（minreal，v0.3 不做零极点对消）","带时滞的系统"],
 REAL_NOTE+"；ss→tf：den = det(sI−A)（由特征值展开），num_ij = det(sI−A+b_j c_i) + (d_ij−1)·den（与 scipy.signal.ss2tf 相同，非最小，不对消）；tf→zpk：伴随矩阵（Parlett–Reinsch 平衡）特征值求根 + ≤4 步 Newton 修正，k = num[0]/den[0]；ss→zpk：p = eig(A)，z = 不变零点（见 control.lti.poles_zeros），k = 首个非零 Markov 参数 C·A^{r−1}·B（r = #p−#z）；zpk→tf：乘积展开",
 [R_KAI,R_EVD,R_BAL,{"type":"software","cite":"scipy.signal tf2ss / ss2tf 文档（仅作约定对照，未参考实现代码）"}],"sysalg::convert",
 "T2",{"max_states":200},COMMON_LTI_ERR+["E.DOMAIN.MIMO_NOT_SUPPORTED","E.DOMAIN.DEGENERATE_SYSTEM"],
 ["W.DOMAIN.NONMINIMAL_REALIZATION","W.NUMERICAL.CONVERSION_DISCREPANCY"]+LW)
convert_in=ins("control.lti.convert",{"system":LTI,"to":{"enum":["tf","ss","zpk"]},
 "normalize":{"type":"boolean","default":True,"description":"tf 输出时把每个分母归一为首一（monic）"}},["system","to"])
convert_out=outs("control.lti.convert",{"system":dict(LTI,description="转换后的系统；ts 与输入相同"),
 "realization":{"enum":["identity","controllable_canonical","block_diagonal_controllable_canonical","ss2tf_characteristic_polynomial","expanded_product","polynomial_roots","invariant_zeros_eigenvalues_markov_gain"],"description":"所用转换/实现约定"},
 "minimal":{"type":["boolean","null"],"description":"v0.3 不判定最小性时为 null"}},["system","realization"])

# ---- control.lti.poles_zeros
pz=man("control.lti.poles_zeros","Poles, zeros and DC gain","返回 LTI 系统的极点、（不变/传输）零点、DC 增益（连续 s=0，离散 z=1）以及每个极点的自然频率与阻尼比。","analysis",["lti","poles","zeros"],
 ["system 为 proper"],["非线性系统","带时滞的系统"],
 "极点：ss 为 eig(A)；SISO tf 为 roots(den)；MIMO tf 为各元素分母根的并集（即块对角实现的特征值，非最小）；zpk 为 p。零点：SISO tf 为 roots(num)；ss（任意 p×m）为 Rosenbrock 系统矩阵 [[A−λI,B],[C,D]] 的有限不变零点：两遍基于 SVD 数值秩的压缩（Emami-Naeini & Van Dooren 1982 思路：先压缩 D 的行、再压缩 C 的残余行，原系统与对偶系统各一遍）把 D 化为方阵可逆，再取 eig(A_r − B_r D_r⁻¹ C_r)；实现最小时不变零点 = 传输零点。DC 增益：A（或 I−A）非奇异时 D − C A⁻¹ B（离散 D + C(I−A)⁻¹B）；奇异时逐元素按 zpk 形式在 0（或 1）处对消重合零极点后求极限，未对消则为 ±Inf。阻尼：连续 ω_n = |p|、ζ = −Re p/|p|；离散先映射 s = ln(z)/ts",
 [R_EVD,R_KAI,R_BAL,R_GVL],"poles_zeros::analyze","T2",{"max_states":200},COMMON_LTI_ERR+["E.DOMAIN.DEGENERATE_SYSTEM"],
 ["W.DOMAIN.NONMINIMAL_REALIZATION","W.DOMAIN.INFINITE_DC_GAIN","W.DOMAIN.UNDEFINED_DC_GAIN","W.NUMERICAL.ILL_CONDITIONED"]+LW)
pz_in=ins("control.lti.poles_zeros",{"system":LTI,"dcgain":{"type":"boolean","default":True}},["system"])
pz_out=outs("control.lti.poles_zeros",{"poles":CVEC("c128 向量 [n]，按实部升序、虚部降序"),"zeros":CVEC("c128 向量，排序同上"),
 "zero_definition":{"enum":["polynomial_roots","invariant_zeros"]},
 "dcgain":{"oneOf":[{"type":"null"},dict(Q,description="unit=1，shape [ny,nu]；可含 \"Inf\"/\"-Inf\"/\"NaN\"")]},
 "natural_frequency":dict(Q,description="unit=rad/s，[n]，与 poles 同序"),"damping_ratio":CVEC("f64 [n]，与 poles 同序；p=0 时为 \"NaN\""),
 "stable":{"type":"boolean","description":"连续：所有 Re p < 0；离散：所有 |p| < 1"}},["poles","zeros","zero_definition","dcgain","natural_frequency","damping_ratio","stable"])

# ---- control.analysis.ctrb_obsv
co=man("control.analysis.ctrb_obsv","Controllability / observability","计算可控性矩阵 [B AB … A^{n−1}B] 与可观性矩阵 [C; CA; …; CA^{n−1}]、基于 SVD 的数值秩，以及（可选）由 Lyapunov 方程得到的可控/可观 Gramian。","analysis",["lti","controllability","observability","gramian"],
 ["system 为 proper；tf/zpk 先按可控标准型实现（结果依赖实现，给出 warning）","Gramian 需系统渐近稳定（连续 Re λ < 0；离散 |λ| < 1）"],["大规模系统（n > 100）的秩判定：Krylov 矩阵病态，应改用 PBH/阶梯形"],
 "Krylov 块逐列构造；数值秩 = #{σ_i > rtol·σ_max}，rtol 缺省为 max(行,列)·ε（与 numpy.linalg.matrix_rank 相同）；同时对 A 的每个特征值做 PBH 检验 σ_min([A−λI, B])/‖[A,B]‖_F 作为更稳健的旁证，二者不一致时给出 W.NUMERICAL.RANK_AMBIGUOUS。Gramian：连续 A W_c + W_c Aᵀ + BBᵀ = 0、Aᵀ W_o + W_o A + CᵀC = 0；离散 A W_c Aᵀ − W_c + BBᵀ = 0、Aᵀ W_o A − W_o + CᵀC = 0；用复 Schur 形式的 Bartels–Stewart 法求解（同 control.math.lyap）",
 [R_KAI,R_GVL,R_BS],"ctrb_obsv::analyze","T2",{"max_states":100},COMMON_LTI_ERR,
 ["W.DOMAIN.REALIZATION_DEPENDENT","W.NUMERICAL.RANK_AMBIGUOUS","W.DOMAIN.GRAMIAN_UNDEFINED","W.NUMERICAL.ILL_CONDITIONED"]+LW)
RANKB={"type":"object","required":["matrix","rank","singular_values","full_rank"],"properties":{"matrix":M,"rank":{"type":"integer"},"singular_values":M,"full_rank":{"type":"boolean"}}}
co_in=ins("control.analysis.ctrb_obsv",{"system":LTI,"gramians":{"type":"boolean","default":False},
 "rank_rtol":{"type":"number","exclusiveMinimum":0,"description":"相对秩容差 σ_i > rank_rtol·σ_max；缺省 max(行,列)·ε"}},["system"])
co_out=outs("control.analysis.ctrb_obsv",{"ctrb":dict(RANKB,description="[n, n·m]"),"obsv":dict(RANKB,description="[n·p, n]"),
 "gramians":{"oneOf":[{"type":"null"},{"type":"object","required":["controllability","observability"],"properties":{"controllability":M,"observability":M}}]},
 "minimal":{"type":"boolean","description":"ctrb 与 obsv 同时满秩"}},["ctrb","obsv","gramians","minimal"])

# ---- control.analysis.margin
mg=man("control.analysis.margin","Gain / phase / stability margins","计算 SISO 开环系统（负单位反馈）的增益裕度、相位裕度、稳定裕度（到 −1 点的最小距离）及对应交越频率；交越点由多项式求根精确求得，并用密集对数网格的符号变化做漏检复查。","analysis",["frequency-domain","robustness","lti"],
 ["SISO","system 为 proper","离散系统：频率范围 (0, π/ts)"],["MIMO（v0.3 不支持，用 disk margin 类算子）","带时滞的系统"],
 "先转为 tf G=n/d（ss 用 ss2tf）。连续：令 s=jω，n(jω)=n_r(ω)+j n_i(ω)、d 同理，均为实系数多项式；相位交越 Im G=0 ⇔ n_i d_r − n_r d_i = 0；增益交越 |G|=1 ⇔ n_r²+n_i² − d_r²−d_i² = 0；稳定裕度极值 ⇔ d/dω(|n+d|²/|d|²) = 0 且二阶导 > 0。各多项式用平衡伴随矩阵求根并 Newton 修正，取 |Im r| ≤ 1e-6·max(1,|r|) 的实根，再在原 G 上用 Newton 迭代精化。离散：双线性代换 z=(1+w)/(1−w) 把单位圆映为虚轴，在 w=jν 上重复上述步骤，ω = (2/ts)·atan ν。复查：在覆盖全部零极点频率的 4000 点对数网格上检测 |G|−1 与 Im G 的符号变化，漏检者用二分法精化后补入并给出 W.NUMERICAL.CROSSING_RECOVERED。选择规则（与 python-control stability_margins 相同）：GM 取 |log GM| 最小者（相位交越且 Re G ≤ 0），PM = mod(∠G, 360°) − 180° 取 |PM| 最小者，SM 取最小值",
 [{"type":"textbook","cite":"G. F. Franklin, J. D. Powell, A. Emami-Naeini, Feedback Control of Dynamic Systems, ch.6（裕度定义）"},R_BAL,{"type":"software","cite":"python-control stability_margins 文档（仅作选择规则对照）"}],
 "margin::compute","T3",{"max_states":100,"scan_points":4000},COMMON_LTI_ERR+["E.DOMAIN.MIMO_NOT_SUPPORTED"],
 ["W.NUMERICAL.CROSSING_RECOVERED","W.DOMAIN.CLOSED_LOOP_UNSTABLE"]+LW,siso_only=True)
mg_in=ins("control.analysis.margin",{"system":LTI},["system"])
XS={"type":"array","items":{"type":"object"}}
mg_out=outs("control.analysis.margin",{"gain_margin":dict(Q,description="unit=1（幅值比）；无相位交越时为 \"Inf\""),"gain_margin_db":dict(Q,description="unit=dB"),
 "phase_margin":dict(Q,description="unit=deg；无增益交越时为 \"Inf\""),"stability_margin":dict(Q,description="unit=1，min_ω |1+G|；无内部极小时为 \"Inf\""),
 "phase_crossover_frequency":dict(Q,description="unit=rad/s，所选 GM 对应频率；无则 \"NaN\""),"gain_crossover_frequency":dict(Q,description="unit=rad/s"),
 "stability_margin_frequency":dict(Q,description="unit=rad/s"),
 "crossings":{"type":"object","required":["phase","gain","stability"],"properties":{"phase":XS,"gain":XS,"stability":XS},"description":"全部交越点（升序），每项含 w_rad_s 与对应裕度"},
 "closed_loop_stable":{"type":"boolean","description":"负单位反馈闭环 n+d 的根是否全部稳定"}},
 ["gain_margin","gain_margin_db","phase_margin","stability_margin","phase_crossover_frequency","gain_crossover_frequency","stability_margin_frequency","crossings","closed_loop_stable"])

# ---- control.lti.c2d
c2d=man("control.lti.c2d","Continuous-to-discrete conversion","把连续时间 LTI 系统按 zoh / foh / tustin（可选预畸变）离散化为采样周期 ts 的离散系统。","transform",["lti","discretization"],
 ["system 为连续时间（ts = 0）且 proper","tustin 要求 1 − p·T/2 ≠ 0（无极点位于 s = 2/T）","prewarp_frequency 仅用于 tustin，且 < π/ts"],["非线性系统","带时滞的系统（v0.3 不支持）","d2c（v0.3 未实现，见 §12）"],
 "ss 层面计算，tf/zpk 先实现为可控标准型（MIMO tf 逐元素离散化后再拼装，结果仍为 tf）。zoh：expm([[A,B],[0,0]]·T) 得 Φ、Γ（Van Loan 1978；Higham 2005 Padé-13）；foh：expm([[A·T, B·T, 0],[0, 0, I],[0, 0, 0]]) 的块 M11、M12、M13，A_d=M11，B_d=M12−M13+M11·M13，C_d=C，D_d=D+C·M13（三角保持形式，与 scipy cont2discrete 'foh' 相同的状态坐标）；tustin：α=T_w/2，A_d=(I−αA)⁻¹(I+αA)，B_d=(I−αA)⁻¹B·T_w，C_d=C(I−αA)⁻¹，D_d=D+α·C·B_d（与 scipy 'bilinear' 相同坐标）；预畸变 T_w = 2·tan(ω_w·T/2)/ω_w，否则 T_w=T。输出种类缺省与输入相同",
 [{"type":"paper","cite":"C. Van Loan, Computing integrals involving the matrix exponential, IEEE TAC 23(3), 1978","doi":"10.1109/TAC.1978.1101743"},R_HIGHAM,R_FPE],
 "c2d::discretize","T2",{"max_states":200},COMMON_LTI_ERR+["E.DOMAIN.NOT_CONTINUOUS","E.VALIDATION.OPTION_CONFLICT","E.DOMAIN.ABOVE_NYQUIST","E.NUMERICAL.SINGULAR","E.DOMAIN.MIMO_NOT_SUPPORTED","E.DOMAIN.DEGENERATE_SYSTEM"],
 ["W.DOMAIN.NONMINIMAL_REALIZATION","W.ACCURACY.ALIASING"]+LW)
c2d_in=ins("control.lti.c2d",{"system":LTI,"ts":{"type":"number","exclusiveMinimum":0,"description":"采样周期，unit s"},
 "method":{"enum":["zoh","foh","tustin"],"default":"zoh"},"prewarp_frequency":{"type":"number","exclusiveMinimum":0,"description":"rad/s，仅 tustin"},
 "output_kind":{"enum":["same","tf","ss","zpk"],"default":"same"}},["system","ts"])
c2d_out=outs("control.lti.c2d",{"system":dict(LTI,description="离散系统，ts = 输入 ts"),"method":{"enum":["zoh","foh","tustin"]},
 "effective_ts_for_formula":{"type":"number","description":"tustin 预畸变时为 T_w，否则 = ts"}},["system","method"])

# ---- control.lti.impulse
imp=man("control.lti.impulse","Impulse response","计算 LTI 系统在均匀时间网格上的脉冲响应（每个输入通道分别施加）。","analysis",["time-domain","lti"],
 ["system 为 proper","uniform 网格同 control.lti.step：t_k = k·dt，N = floor(t_final/dt + 1e-9) + 1","离散系统要求 dt = ts"],["任意输入（将来用 control.lti.lsim）","带时滞的系统"],
 "连续：y(t) = C·e^{At}·B（t ≥ 0），由 x_0 = B[:,j]、x_{k+1} = Φx_k 递推，Φ = expm(A·dt)（Padé-13 + scaling & squaring），采样值无离散化误差；D ≠ 0 时 t=0 处的 D·δ(t) 无法用采样值表示，不计入 y，单独在 direct_feedthrough 中返回并给出 W.DOMAIN.DIRECT_FEEDTHROUGH_IGNORED。离散：缺省 unit_area（u_0 = 1/ts，与 python-control 及 MATLAB impulse 相同），可选 unit_pulse（u_0 = 1）。tf/zpk 先转为可控标准型",
 [{"type":"paper","cite":"C. Moler, C. Van Loan, Nineteen dubious ways to compute the exponential of a matrix, twenty-five years later, SIAM Review 45(1), 2003","doi":"10.1137/S00361445024180"},R_HIGHAM],
 "step::simulate_impulse (expm Padé-13 + recursion)","T2",{"max_grid_points":10000000,"max_output_values":1000000},
 COMMON_LTI_ERR+["E.VALIDATION.GRID","E.VALIDATION.RANGE","E.RESOURCE.COMPUTE_LIMIT","E.RESOURCE.OUTPUT_TOO_LARGE"],
 ["W.DOMAIN.UNSTABLE_SYSTEM","W.DOMAIN.DIRECT_FEEDTHROUGH_IGNORED","W.ACCURACY.GRID_TOO_COARSE","W.ACCURACY.AUTO_GRID"]+LW)
imp_in=json.loads(json.dumps(step_in)); imp_in["$id"]=f"aeoi://schemas/control.lti.impulse/{V3}/input"; del imp_in["properties"]["step_info"]
imp_in["properties"]["discrete_scaling"]={"enum":["unit_area","unit_pulse"],"default":"unit_area","description":"离散系统的脉冲幅值：unit_area 为 1/ts（缺省），unit_pulse 为 1；连续系统忽略"}
imp_out=outs("control.lti.impulse",{"t":{"oneOf":[{"type":"null"},Q]},"y":{"oneOf":[{"type":"null"},dict(Q,description="shape [ny,nu,nt]")]},"n_grid_points":{"type":"integer"},
 "direct_feedthrough":{"oneOf":[{"type":"null"},dict(M,description="连续系统的 D（t=0 处 D·δ(t) 的权重），未计入 y；D=0 或离散时为 null")]}},["t","y","n_grid_points","direct_feedthrough"])

# ---- control.math.lyap
ly=man("control.math.lyap","Lyapunov equation","求解连续 Lyapunov 方程 A X + X Aᵀ + Q = 0 或离散 Lyapunov 方程 A X Aᵀ − X + Q = 0。","utility",["matrix-equation","lyapunov"],
 ["A 方阵，Q 同形","连续：λ_i + λ̄_j ≠ 0（对所有特征值对）；离散：λ_i·λ̄_j ≠ 1"],["Sylvester 方程（v0.3 未暴露）","大规模稀疏（低秩 ADI 类方法）"],
 "Bartels–Stewart：复 Schur 分解 A = U T Uᴴ（T 上三角），Q̃ = Uᴴ Q U；连续逐列（j 从 n−1 到 0）解 (T + t̄_jj I) y_j = −q̃_j − Σ_{k>j} t̄_jk y_k，离散解 (t̄_jj T − I) y_j = −q̃_j − T Σ_{k>j} t̄_jk y_k，均为上三角回代；X = Re(U Y Uᴴ)。可解性分离度 sep_min = min|λ_i+λ̄_j|（离散 min|λ_i λ̄_j − 1|）≤ 10³·u·‖A‖_F（离散 10³·u·max(1,‖A‖_F)²）时报 E.DOMAIN.LYAP_SINGULAR。Q 对称时输出对称化",
 [R_BS,R_GVL,{"type":"paper","cite":"S. J. Hammarling, Numerical solution of the stable, non-negative definite Lyapunov equation, IMA J. Numer. Anal. 2(3), 1982","doi":"10.1093/imanum/2.3.303"}],
 "sysalg::lyap_bs (complex Schur Bartels–Stewart)","T2",{"max_states":300},MAT_ERR+["E.DOMAIN.LYAP_SINGULAR","E.NUMERICAL.NO_CONVERGENCE"]+TAIL_ERR,
 ["W.DOMAIN.UNSTABLE_SYSTEM","W.NUMERICAL.ILL_CONDITIONED","W.NUMERICAL.LARGE_RESIDUAL","W.VALIDATION.SYMMETRIZED","W.PERFORMANCE.SPARSE_DENSIFIED"],
 sparse={p:{"policy":"densify","max_dense_elements":1000000} for p in ["/A","/Q"]})
ly_in=ins("control.math.lyap",{"A":M,"Q":M,"type":{"enum":["continuous","discrete"],"default":"continuous"}},["A","Q"],{"x-aeoi-shape-constraints":["A: [n,n]","Q: [n,n]"]})
ly_out=outs("control.math.lyap",{"X":dict(M,description="[n,n]；Q 对称时 X 对称")},["X"])

# ---- control.design.place
pl=man("control.design.place","Pole placement","求状态反馈增益 K（u = −Kx）使 A − BK 的特征值为给定极点；多输入时用 KNV 方法 0 选择条件数较小的特征向量组（鲁棒配置）。","design",["pole-placement","state-feedback","lti"],
 ["(A,B) 可控（PBH 判据）","B 列满秩","极点集合关于实轴共轭封闭（相对容差 1e-12）","每个极点的重数 ≤ rank(B)"],["输出反馈配置","离散/连续不区分（只看特征值）"],
 "B = [U0 U1]·[Z;0]（QR 补全为正交阵）；对每个极点 λ_i 求 S_i = null(U1ᵀ(A − λ_i I))（复 QR 补全）；特征向量 x_i ∈ span(S_i)。KNV 方法 0（Kautsky–Nichols–Van Dooren 1985）：逐个把 x_i 替换为 S_i 上与其余列正交补方向 y_i 最接近的单位向量（实极点限制为实向量：取 [Re SᵢᵀY, Im SᵢᵀY] 主方向；复共轭对用复向量，伙伴列取共轭），扫描直至 |det X|（列归一化）相对变化 < 1e-6 或达到最大扫描次数；然后 M = X Λ X⁻¹（取实部），K = Z⁻¹ U0ᵀ (A − M)。单输入时 S_i 一维，K 唯一（等价于 Ackermann 公式但数值更稳）。说明：scipy.signal.place_poles 缺省用 Tits–Yang，本实现用 KNV0 的复数推广，多输入时 K 不唯一，parity 只比较达成极点与（SISO 时）K",
 [{"type":"paper","cite":"J. Kautsky, N. K. Nichols, P. Van Dooren, Robust pole assignment in linear state feedback, Int. J. Control 41(5), 1985","doi":"10.1080/0020718508961188"},
  {"type":"paper","cite":"A. L. Tits, Y. Yang, Globally convergent algorithms for robust pole assignment by state feedback, IEEE TAC 41(10), 1996","doi":"10.1109/9.539425"},R_KAI],
 "place::knv0","T2",{"max_states":100,"max_sweeps":50},MAT_ERR+["E.VALIDATION.NOT_CONJUGATE","E.VALIDATION.RANK_DEFICIENT","E.DOMAIN.NOT_CONTROLLABLE","E.DOMAIN.POLE_MULTIPLICITY","E.NUMERICAL.SINGULAR"]+TAIL_ERR,
 ["W.NUMERICAL.PLACEMENT_INACCURATE","W.NUMERICAL.ILL_CONDITIONED","W.NUMERICAL.NEAR_UNCONTROLLABLE","W.PERFORMANCE.SPARSE_DENSIFIED"],
 sparse={p:{"policy":"densify","max_dense_elements":1000000} for p in ["/A","/B"]})
pl_in=ins("control.design.place",{"A":M,"B":M,"poles":dict(CPLX_LIST,minItems=1,description="期望闭环极点 [re, im]，长度 n，共轭封闭"),
 "max_sweeps":{"type":"integer","minimum":1,"maximum":200,"default":50}},["A","B","poles"])
pl_out=outs("control.design.place",{"K":dict(M,description="[m,n]，u=−Kx"),"achieved_poles":CVEC("eig(A−BK)，c128 [n]，排序同 poles 规范")},["K","achieved_poles"])

# ---- control.design.dlqr
dl=man("control.design.dlqr","Discrete-time LQR","求解离散代数 Riccati 方程 (DARE)，返回最优状态反馈增益 K（u_k=−Kx_k）、Riccati 解 S 与闭环极点，使 J=Σ(xᵀQx+uᵀRu+2xᵀNu) 最小。","design",["optimal-control","riccati","lti","discrete-time"],
 ["R 对称正定","Q 对称；[[Q,N],[Nᵀ,R]] 对称半正定","(A,B) 可镇定（|λ| ≥ 1 的模态可控）","辛矩阵束在单位圆上无特征值（等价于 (Q−NR⁻¹Nᵀ, A−BR⁻¹Nᵀ) 在单位圆上无不可观模态）"],
 ["连续系统（用 control.design.lqr）","有约束的 MPC"],
 "消去交叉项（Ā=A−BR⁻¹Nᵀ，Q̄=Q−NR⁻¹Nᵀ，G=BR⁻¹Bᵀ）后用结构保持倍增算法 SDA：A_{k+1}=A_k(I+G_kH_k)⁻¹A_k，G_{k+1}=G_k+A_k(I+G_kH_k)⁻¹G_kA_kᵀ，H_{k+1}=H_k+A_kᵀH_k(I+G_kH_k)⁻¹A_k，H_0=Q̄，H_k→S（二次收敛）；随后至多 3 步 Newton–Hewer 修正（闭环离散 Lyapunov 方程用 Bartels–Stewart 求解），残差不降即停。K=(R+BᵀSB)⁻¹(BᵀSA+Nᵀ)",
 [{"type":"paper","cite":"E. K.-W. Chu, H.-Y. Fan, W.-W. Lin, A structure-preserving doubling algorithm for continuous-time algebraic Riccati equations, Linear Algebra Appl. 396, 2005","doi":"10.1016/j.laa.2004.10.010"},
  {"type":"paper","cite":"G. A. Hewer, An iterative technique for the computation of the steady state gains for the discrete optimal regulator, IEEE TAC 16(4), 1971","doi":"10.1109/TAC.1971.1099755"},
  {"type":"textbook","cite":"P. Lancaster, L. Rodman, Algebraic Riccati Equations, Oxford 1995"}],
 "dlqr::solve_dare (SDA + Newton–Hewer)","T2",{"max_states":200,"max_sda_iterations":100},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.NOT_SYMMETRIC","E.VALIDATION.SPARSE_STRUCTURE","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN",
  "E.DOMAIN.R_NOT_POSITIVE_DEFINITE","E.DOMAIN.WEIGHT_NOT_PSD","E.DOMAIN.NOT_STABILIZABLE","E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG","E.NUMERICAL.NO_CONVERGENCE","E.NUMERICAL.SINGULAR"]+TAIL_ERR,
 ["W.NUMERICAL.LARGE_RESIDUAL","W.NUMERICAL.NEAR_UNCONTROLLABLE","W.NUMERICAL.ILL_CONDITIONED","W.VALIDATION.SYMMETRIZED","W.PERFORMANCE.SPARSE_DENSIFIED"],
 sparse={p:{"policy":"densify","max_dense_elements":1000000} for p in ["/A","/B","/Q","/R","/N"]})
dl_in=json.loads(json.dumps(lqr_in)); dl_in["$id"]=f"aeoi://schemas/control.design.dlqr/{V3}/input"
dl_out=json.loads(json.dumps(lqr_out)); dl_out["$id"]=f"aeoi://schemas/control.design.dlqr/{V3}/output"
dl_out["properties"]["closed_loop_poles"]["description"]="c128 向量 [n]，eig(A−BK)，按实部升序、虚部降序；全部 |λ|<1"
B2={"control.lti.convert":(convert,convert_in,convert_out),"control.lti.poles_zeros":(pz,pz_in,pz_out),"control.analysis.ctrb_obsv":(co,co_in,co_out),
 "control.analysis.margin":(mg,mg_in,mg_out),"control.lti.c2d":(c2d,c2d_in,c2d_out),"control.lti.impulse":(imp,imp_in,imp_out),
 "control.math.lyap":(ly,ly_in,ly_out),"control.design.place":(pl,pl_in,pl_out),"control.design.dlqr":(dl,dl_in,dl_out)}
for op,(m,i,o) in ({"control.lti.bode":(bode,bode_in,bode_out),"control.lti.step":(step,step_in,step_out),"control.design.lqr":(lqr,lqr_in,lqr_out)}|B2).items():
    d=f"examples/{op}/"; w(d+"manifest.json",m); w(d+"input.schema.json",i); w(d+"output.schema.json",o)
print("ok")
