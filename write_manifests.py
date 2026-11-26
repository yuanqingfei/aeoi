"""Source of truth for v0.2 manifests and I/O schemas (examples/<id>/{manifest,input.schema,output.schema}.json).
The Rust crates embed these files at compile time."""
import json, os
SPEC="aeoi/0.4"; C="aeoi://schemas/common/0.4#/$defs/"  # standard JSON Schema 2020-12 $ref (spec 3.9 J-1)
def w(p,o):
    os.makedirs(os.path.dirname(p),exist_ok=True); json.dump(o,open(p,"w"),indent=2,ensure_ascii=False); open(p,"a").write("\n")
CLEAN={"declared":True,"sources":"仅公开文献与开源许可兼容的参考；算子命名沿用 MATLAB 的公开函数名，参考数据只来自开源实现（python-control/scipy/mpmath），没有使用 MATLAB 生成数据"}
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
LQR_V="0.3.0"  # 0.2.0 → 0.3.0 (G-2: accuracy improvement; false HAMILTONIAN_IMAG_AXIS_EIG fixed)
lqr={"spec":SPEC,"id":"control.design.lqr","version":LQR_V,"title":"Continuous-time LQR",
 "description":"求解连续时间代数 Riccati 方程 (CARE)，返回最优状态反馈增益 K（u=−Kx）、Riccati 解 S 与闭环极点，使 J=∫(xᵀQx+uᵀRu+2xᵀNu)dt 最小。",
 "category":"design","tags":["optimal-control","riccati","lti"],
 "applicability":{"preconditions":["R 对称正定","Q 对称；[[Q,N],[Nᵀ,R]] 对称半正定","(A,B) 可镇定","Hamiltonian 矩阵在虚轴上无特征值（等价于 (Q−NR⁻¹Nᵀ, A−BR⁻¹Nᵀ) 在虚轴上无不可观模态）"],
   "not_for":["离散系统（将来用 control.design.dlqr）","有约束的 MPC"],"siso_only":False},
 "provenance":{"algorithm":"消去交叉项后构造 Hamiltonian 矩阵 H=[[Ā,−BR⁻¹Bᵀ],[−Q̄,−Āᵀ]]。虚轴检验在辛对角平衡后的 H̃=D⁻¹HD 上做（D=diag(d,d⁻¹)，d_i 为 2 的幂，坐标下降最小化逐元 ℓ₁ 范数，等价于状态尺度变换）：min|Re λ(H̃)| ≤ 10³·u·‖H̃‖₁ 只作为怀疑，用 PBH 检验 (Q̄,Ā) 在近虚轴特征值处是否不可观来确认，否则照常求解并在事后检验闭环稳定裕度；在 H 上用带行列式缩放的 Newton 迭代计算 sign(H)，由 (sign(H)+I) 的零空间最小二乘解出 S；随后在原坐标做 K 形式 Newton–Kleinman 修正（缺陷校正形式）：K=R⁻¹(BᵀX+Nᵀ)（Cholesky + 补偿残差迭代精化），残差 E=(A−BK)ᵀX+X(A−BK)+Q−NK−KᵀNᵀ+KᵀRK 用补偿点积（Dot2）计算，ΔX 解 (A−BK)ᵀΔX+ΔX(A−BK)+E=0（复 Schur Bartels–Stewart），X←X+ΔX；‖ΔX‖_F ≤ 2ε‖X‖_F 或在 1e-8‖X‖_F 以下不再减小即停，至多 20 步，结果须保持 A−BK Hurwitz；最终 K 由 S 加一步未施加的 Newton 修正量（S 的低位部分）精化求得",
   "references":[{"type":"paper","cite":"J. D. Roberts, Linear model reduction and solution of the algebraic Riccati equation by use of the sign function, Int. J. Control 32(4), 1980","doi":"10.1080/00207178008922881"},
     {"type":"paper","cite":"R. Byers, Solving the algebraic Riccati equation with the matrix sign function, Linear Algebra Appl. 85, 1987","doi":"10.1016/0024-3795(87)90222-9"},
     {"type":"paper","cite":"C. S. Kenney, A. J. Laub, The matrix sign function, IEEE TAC 40(8), 1995","doi":"10.1109/9.402226"},
     {"type":"paper","cite":"D. L. Kleinman, On an iterative technique for Riccati equation computations, IEEE TAC 13(1), 1968","doi":"10.1109/TAC.1968.1098829"},
     {"type":"paper","cite":"P. Benner, Symplectic balancing of Hamiltonian matrices, SIAM J. Sci. Comput. 22(5), 2001","doi":"10.1137/S1064827500367993"},
     {"type":"paper","cite":"R. H. Bartels, G. W. Stewart, Solution of the matrix equation AX + XB = C, Comm. ACM 15(9), 1972","doi":"10.1145/361573.361582"},
     {"type":"textbook","cite":"B. D. O. Anderson, J. B. Moore, Optimal Control: Linear Quadratic Methods, 1990"}],
   "clean_room":CLEAN},
 "backend":BACK("lqr::solve_care (matrix sign function + K-form Newton–Kleinman defect correction; balanced imaginary-axis test)"),
 "determinism":"platform_bitwise","tolerance_class":"T2",
 "sparse_support":{p:{"policy":"densify","max_dense_elements":1000000} for p in ["/A","/B","/Q","/R","/N"]},"encodings":ENC,
 "limits":{"max_states":200,"max_states_for_refinement":200,"max_sign_iterations":100,"max_newton_iterations":20},
 "input_schema":{"$ref":"./input.schema.json"},"output_schema":{"$ref":"./output.schema.json"},
 "conformance":{"target":"L3","current":"L2"},"accuracy_grade":{"target":"A","current":"B"},
 "credentials":f"aeoi://operators/control.design.lqr/{LQR_V}/credentials",
 "errors":["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.NOT_SYMMETRIC","E.VALIDATION.SPARSE_STRUCTURE","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN",
   "E.DOMAIN.R_NOT_POSITIVE_DEFINITE","E.DOMAIN.WEIGHT_NOT_PSD","E.DOMAIN.NOT_STABILIZABLE","E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG","E.NUMERICAL.NO_CONVERGENCE","E.NUMERICAL.SINGULAR","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 "warnings":["W.NUMERICAL.LARGE_RESIDUAL","W.NUMERICAL.NEAR_UNCONTROLLABLE","W.NUMERICAL.ILL_CONDITIONED","W.VALIDATION.SYMMETRIZED","W.PERFORMANCE.SPARSE_DENSIFIED"],
 "mcp":{"tool_name":"control_design_lqr","annotations":ANN}}
lqr_in={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":f"aeoi://schemas/control.design.lqr/{LQR_V}/input","type":"object","additionalProperties":False,
 "required":["A","B","Q","R"],"properties":{k:{"$ref":C+"Matrix"} for k in ["A","B","Q","R","N"]}|{
  "symmetry_tolerance":{"type":"number","exclusiveMinimum":0,"default":1e-10,"description":"相对容差 ‖X−Xᵀ‖_F ≤ tol·‖X‖_F；超出即 E.VALIDATION.NOT_SYMMETRIC，范围内（但非精确对称）对称化并给 W.VALIDATION.SYMMETRIZED"},
  "exec":{"$ref":C+"ExecOptions"}},
 "x-aeoi-shape-constraints":["A: [n,n]","B: [n,m]","Q: [n,n]","R: [m,m]","N: [n,m]，缺省为 0"]}
M={"$ref":C+"Matrix"}
lqr_out={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":f"aeoi://schemas/control.design.lqr/{LQR_V}/output","type":"object","required":["K","S","closed_loop_poles"],
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
def man(id_,title,desc,cat,tags,pre,notfor,algo,refs,routine,tol,limits,errors,warnings,sparse=SS_SPARSE,siso_only=False,det="platform_bitwise",ver=V3,backend=None):
    return {"spec":SPEC,"id":id_,"version":ver,"title":title,"description":desc,"category":cat,"tags":tags,
     "applicability":{"preconditions":pre,"not_for":notfor,"siso_only":siso_only},
     "provenance":{"algorithm":algo,"references":refs,"clean_room":CLEAN},
     "backend":dict(backend or BACK3(routine),version=ver),"determinism":det,"tolerance_class":tol,"sparse_support":sparse,"encodings":ENC,"limits":limits,
     "input_schema":{"$ref":"./input.schema.json"},"output_schema":{"$ref":"./output.schema.json"},
     "conformance":{"target":"L3","current":"L2"},"accuracy_grade":{"target":"A","current":"B"},
     "credentials":f"aeoi://operators/{id_}/{ver}/credentials","errors":errors+([] if "E.VALIDATION.METHOD_UNAVAILABLE" in errors else ["E.VALIDATION.METHOD_UNAVAILABLE"]),"warnings":warnings,
     "mcp":{"tool_name":id_.replace(".","_"),"annotations":ANN}}
def ins(id_,props,req,extra=None,ver=V3):
    o={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":f"aeoi://schemas/{id_}/{ver}/input","type":"object","additionalProperties":False,"required":req,"properties":dict(props,exec=EXEC)}
    if extra: o.update(extra)
    return o
def outs(id_,props,req,ver=V3):
    return {"$schema":"https://json-schema.org/draft/2020-12/schema","$id":f"aeoi://schemas/{id_}/{ver}/output","type":"object","required":req,"properties":props}
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
co=man("control.analysis.ctrb_obsv","Controllability / observability","判断 LTI 系统是否可控、是否可观（“(A, B) 是否可控”“可控性矩阵的秩是多少”“系统是否可观”这类问题用本工具；只给出 A、B 时也用本工具，C、D 按 system 的说明补零即可）。计算可控性矩阵 [B AB … A^{n−1}B] 与可观性矩阵 [C; CA; …; CA^{n−1}]、基于 SVD 的数值秩，以及（可选）由 Lyapunov 方程得到的可控/可观 Gramian。高阶系统的 Krylov 矩阵严重病态：Krylov 秩与 PBH 检验不一致时 receipt.claim 为 unverified，receipt 列出不可依赖的字段（rank、full_rank、minimal），不要把它们当结论。设计控制器不需要先调用本工具。","analysis",["lti","controllability","observability","gramian"],
 ["system 为 proper；tf/zpk 先按可控标准型实现（结果依赖实现，给出 warning）","Gramian 需系统渐近稳定（连续 Re λ < 0；离散 |λ| < 1）"],["大规模系统（n > 100）的秩判定：Krylov 矩阵病态，应改用 PBH/阶梯形","控制器设计、带宽/裕度检查（用 control_analysis_loopsens，或 control_lti_series / control_lti_feedback 的具名角色）"],
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
 ["ts（采样周期）必须由用户明确给出；用户没给时不要调用本工具，先询问用户，不得假设 0.1 s 等常用值","system 为连续时间（ts = 0）且 proper","tustin 要求 1 − p·T/2 ≠ 0（无极点位于 s = 2/T）","prewarp_frequency 仅用于 tustin，且 < π/ts"],["非线性系统","带时滞的系统（v0.3 不支持）","d2c（v0.3 未实现，见 §12）"],
 "ss 层面计算，tf/zpk 先实现为可控标准型（MIMO tf 逐元素离散化后再拼装，结果仍为 tf）。zoh：expm([[A,B],[0,0]]·T) 得 Φ、Γ（Van Loan 1978；Higham 2005 Padé-13）；foh：expm([[A·T, B·T, 0],[0, 0, I],[0, 0, 0]]) 的块 M11、M12、M13，A_d=M11，B_d=M12−M13+M11·M13，C_d=C，D_d=D+C·M13（三角保持形式，与 scipy cont2discrete 'foh' 相同的状态坐标）；tustin：α=T_w/2，A_d=(I−αA)⁻¹(I+αA)，B_d=(I−αA)⁻¹B·T_w，C_d=C(I−αA)⁻¹，D_d=D+α·C·B_d（与 scipy 'bilinear' 相同坐标）；预畸变 T_w = 2·tan(ω_w·T/2)/ω_w，否则 T_w=T。输出种类缺省与输入相同",
 [{"type":"paper","cite":"C. Van Loan, Computing integrals involving the matrix exponential, IEEE TAC 23(3), 1978","doi":"10.1109/TAC.1978.1101743"},R_HIGHAM,R_FPE],
 "c2d::discretize","T2",{"max_states":200},COMMON_LTI_ERR+["E.DOMAIN.NOT_CONTINUOUS","E.VALIDATION.OPTION_CONFLICT","E.DOMAIN.ABOVE_NYQUIST","E.NUMERICAL.SINGULAR","E.DOMAIN.MIMO_NOT_SUPPORTED","E.DOMAIN.DEGENERATE_SYSTEM"],
 ["W.DOMAIN.NONMINIMAL_REALIZATION","W.ACCURACY.ALIASING"]+LW)
c2d_in=ins("control.lti.c2d",{"system":LTI,"ts":{"type":"number","exclusiveMinimum":0,"description":"采样周期，unit s","x-aeoi-provenance":"user"},
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
 ["A 方阵，Q 同形（Q 可以不对称）","连续：λ_i + λ̄_j ≠ 0（对所有特征值对）；离散：λ_i·λ̄_j ≠ 1"],["Sylvester 方程（v0.3 未暴露）","大规模稀疏（低秩 ADI 类方法）"],
 "Bartels–Stewart：复 Schur 分解 A = U T Uᴴ（T 上三角），Q̃ = Uᴴ Q U；连续逐列（j 从 n−1 到 0）解 (T + t̄_jj I) y_j = −q̃_j − Σ_{k>j} t̄_jk y_k，离散解 (t̄_jj T − I) y_j = −q̃_j − T Σ_{k>j} t̄_jk y_k，均为上三角回代；X = Re(U Y Uᴴ)。当启发式前向误差估计 ‖R‖_F/(sep_min·‖X‖_F) > 1e-12（方程接近奇异，如离散 ρ(A) → 1）时做迭代精化：用补偿算术（Dot2，Dekker 无 FMA 乘法，跨平台逐位相同）计算残差 R(X)，复用同一 Schur 分解解修正量 δ，X ← X + δ，至多 4 步，‖δ‖_F ≤ 2u‖X‖_F 或修正量未减半即停。可解性分离度 sep_min = min|λ_i+λ̄_j|（离散 min|λ_i λ̄_j − 1|）≤ 10³·u·‖A‖_F（离散 10³·u·max(1,‖A‖_F)²）时报 E.DOMAIN.LYAP_SINGULAR。Q 对称时输出对称化",
 [R_BS,R_GVL,{"type":"paper","cite":"S. J. Hammarling, Numerical solution of the stable, non-negative definite Lyapunov equation, IMA J. Numer. Anal. 2(3), 1982","doi":"10.1093/imanum/2.3.303"},
  {"type":"paper","cite":"T. Ogita, S. M. Rump, S. Oishi, Accurate sum and dot product, SIAM J. Sci. Comput. 26(6), 2005","doi":"10.1137/030601818"},
  {"type":"textbook","cite":"N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM, 2002（迭代精化，第 12 章）"}],
 "sysalg::lyap_refined (complex Schur Bartels–Stewart + compensated-residual iterative refinement)","T2",{"max_states":300,"max_refinement_steps":4},MAT_ERR+["E.DOMAIN.LYAP_SINGULAR","E.NUMERICAL.NO_CONVERGENCE"]+TAIL_ERR,
 ["W.DOMAIN.UNSTABLE_SYSTEM","W.NUMERICAL.ILL_CONDITIONED","W.NUMERICAL.LARGE_RESIDUAL","W.PERFORMANCE.SPARSE_DENSIFIED"],  # v0.4.4: no W.VALIDATION.SYMMETRIZED, lyap never symmetrizes an input (10.10)
 sparse={p:{"policy":"densify","max_dense_elements":1000000} for p in ["/A","/Q"]})
ly_in=ins("control.math.lyap",{"A":M,"Q":M,"type":{"enum":["continuous","discrete"],"default":"continuous"}},["A","Q"],{"x-aeoi-shape-constraints":["A: [n,n]","Q: [n,n]"]})
ly_out=outs("control.math.lyap",{"X":dict(M,description="[n,n]；Q 对称时 X 对称")},["X"])
# 0.3.0 → 0.4.0 (G-2: accuracy improvement — iterative refinement near singularity)
LYAP_V="0.4.0"
for _d in (ly,ly_in,ly_out):
    for _k in ("version","credentials","$id"):
        if _k in _d: _d[_k]=_d[_k].replace(V3,LYAP_V)

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

# =====================================================================================  batch 3 (v0.3.5): interconnection, pid, stepinfo, pade
CONN_SPARSE={p:{"policy":"densify","max_dense_elements":1000000} for p in
 ["/sys1/A","/sys1/B","/sys1/C","/sys1/D","/sys2/A","/sys2/B","/sys2/C","/sys2/D"]}
CONN_ERR=COMMON_LTI_ERR+["E.DOMAIN.SAMPLE_TIME_MISMATCH","E.VALIDATION.SHAPE"]
CONN_WARN=["W.DOMAIN.IMPROPER_OPERAND","W.DOMAIN.IMPROPER_RESULT","W.DOMAIN.NONMINIMAL_REALIZATION","W.NUMERICAL.ILL_CONDITIONED"]+LW
CONN_PRE=["两个操作数的 ts 必须完全相同（连续都是 0，或同一个采样周期）","维度按串联 / 并联 / 反馈的规则匹配","不做零极点对消；minimal 为 null","improper 的 tf/zpk（相对阶 ≤ 8）可以作为操作数，但 ss 输出会报 E.DOMAIN.IMPROPER_SYSTEM"]
def conn_out(id_, reals):
    return outs(id_, {
        "system": dict(LTI, description="互联后的系统，含 ts 与 time_unit，可直接作为下一个算子的 system"),
        "realization": {"enum": reals, "description": "多项式算术或块状态空间公式，见 spec I-6"},
        "minimal": {"type": ["boolean", "null"]},
        "proper": {"type": "boolean"},
        "composition": {"type": "object", "description": "operation、operands[]（role 与 sha256：操作数 JSON 的 JCS 哈希）、sign、implicit_unity、loop_role（open_loop = L、closed_loop = T、unspecified = 用 sys1/sys2 调用，角色由调用方负责；spec K-3）、topology、lineage（只回显，不核验）"},
    }, ["system", "realization", "proper", "composition"])
OUTK={"enum":["same","tf","ss"],"default":"same","description":"same：SISO 的 tf/zpk 用多项式，否则用 ss 块公式；tf：SISO 多项式或 ss2tf；ss：块公式"}
LINEAGE={"type":"array","items":{"type":"string"},"maxItems":8,"description":"可选。上游 receipt.fingerprint.request 列表。只写入 composition.lineage，算子不核验，缺省不是错误。"}
def conn_in(id_, extra, req, ver=V3):
    props={"sys1":LTI,"sys2":LTI,"output_kind":dict(OUTK),"lineage":LINEAGE}
    props.update(extra)
    return ins(id_, props, req, ver=ver)
V36="0.4.0"  # v0.3.6: named loop roles (K-1..K-3), PID option conflicts (F-8)
ALG="SISO 且两个操作数都是 tf 或 zpk：多项式算术，不对消。y = sys2(sys1(u)) 即 num = conv(num2, num1)、den = conv(den2, den1)。任一操作数为 ss、MIMO，或 output_kind=ss：块公式 A=[[A1,0],[B2 C1, A2]]，B=[[B1],[B2 D1]]，C=[D2 C1, C2]，D=D2 D1。状态按 sys1 在前。"
ALG_P="SISO tf/zpk：num = conv(num1, den2)+conv(num2, den1)，den = conv(den1, den2)（低次项对齐）。ss / MIMO：A=diag(A1,A2)，B=[[B1],[B2]]，C=[C1 C2]，D=D1+D2。"
ALG_F="方程（规范 I-5）：u1 = u + sign·y2，y1 = sys1(u1)，u2 = y1，y2 = sys2(u2)，y = y1。sign 默认 -1（负反馈）。SISO tf：num = conv(num1, den2)，den = conv(den1, den2) − sign·conv(num1, num2)。省略 sys2 表示与 sys1 同维的单位增益。代数环 E = I − sign·D2·D1 的 rcond ≤ n·u·1e4 时报 E.DOMAIN.ALGEBRAIC_LOOP。"
R_GVL_PADE={"type":"textbook","cite":"G. H. Golub, C. F. Van Loan, Matrix Computations, 3rd ed., Johns Hopkins, 1996, Algorithm 11.3.1（指数函数的 Padé 分子递推）"}
R_VAJTA={"type":"paper","cite":"M. Vajta, Some remarks on Padé-approximations, 3rd TEMPUS-INTCOM Symposium, 2000"}
R_OGATA={"type":"textbook","cite":"K. Ogata, Modern Control Engineering, 5th ed., Prentice-Hall（串联、并联、反馈；阶跃指标）"}
series=man("control.lti.series","Series interconnection","串联。推荐用具名角色：controller 与 plant，结果是 L = plant·controller（控制器输出进对象），单位反馈时它就是开环 L，送给 margin；composition.loop_role = open_loop。也可用 sys1/sys2：结果是 sys2·sys1（sys1 的输出进 sys2），角色由调用方负责。两种写法不能混用。结果 system 原样传给下一个工具，不要手抄系数。","transform",["lti","interconnection"],
 CONN_PRE+["具名角色（controller、plant）与位置参数（sys1、sys2）二选一，混用报 E.VALIDATION.OPTION_CONFLICT","具名调用必须同时给出 controller 和 plant"],["自动零极点对消（minreal）","不同采样周期","把串联结果当成闭环 T（串联只得到开环 L，闭环用 control_lti_feedback 的具名角色或 control_analysis_loopsens）"],ALG,[R_OGATA,R_KAI],"connect::series_ss / polynomial product","T1",{"max_states":200,"max_degree":64,"max_relative_degree":8},
 CONN_ERR+["E.VALIDATION.OPTION_CONFLICT"],CONN_WARN,sparse=dict(CONN_SPARSE,**{f"/{r}/{x}":{"policy":"densify","max_dense_elements":1000000} for r in ("plant","controller") for x in "ABCD"}),ver=V36)
series_in=conn_in("control.lti.series",{"controller":LTI,"plant":LTI},[],ver=V36)
series_out=conn_out("control.lti.series",["polynomial_product","series_block","ss2tf_characteristic_polynomial"]); series_out["$id"]=series_out["$id"].replace(V3,V36)
par=man("control.lti.parallel","Parallel interconnection","并联：同一输入，输出相加，结果是 sys1+sys2。","transform",["lti","interconnection"],
 CONN_PRE,["自动零极点对消","不同采样周期"],ALG_P,[R_OGATA,R_KAI],"connect::parallel_ss / polynomial sum","T1",{"max_states":200,"max_degree":64,"max_relative_degree":8},
 CONN_ERR,CONN_WARN,sparse=CONN_SPARSE)
par_in=conn_in("control.lti.parallel",{},["sys1","sys2"])
par_out=conn_out("control.lti.parallel",["polynomial_sum","parallel_block","ss2tf_characteristic_polynomial"])
fb=man("control.lti.feedback","Feedback interconnection","闭环。推荐用具名角色：plant（必填）、controller（可省略 = 1）、sensor（反馈通路，可省略 = 单位反馈）。拓扑固定为 r → e = r − sensor·y → controller → plant → y，结果是闭环 T = plant·controller/(1 + sensor·plant·controller)（r→y），composition.loop_role = closed_loop：送给 stepinfo/step，不要送给 margin（裕度要用开环 L）。控制器永远在前向通路。也可用 sys1（前向）/sys2（反馈通路），角色由调用方负责；两种写法不能混用。","transform",["lti","interconnection","feedback"],
 CONN_PRE+["具名角色（plant、controller、sensor）与位置参数（sys1、sys2）二选一，混用报 E.VALIDATION.OPTION_CONFLICT","sign ∈ {+1,-1}，默认 -1；这是定义默认值，不是测量值","E = I − sign·D₂·D₁ 可逆"],["自动零极点对消","把 sign 留空时当成 0","把控制器放进反馈通路（具名调用里 controller 总在前向通路；sensor 才是反馈通路）","求开环裕度（闭环 T 不是开环 L；用 control_lti_series 的具名角色或 control_analysis_loopsens）"],ALG_F,[R_OGATA,R_KAI],"connect::feedback_ss / polynomial feedback","T2",{"max_states":200,"max_degree":64,"max_relative_degree":8},
 CONN_ERR+["E.DOMAIN.ALGEBRAIC_LOOP","E.VALIDATION.OPTION_CONFLICT"],CONN_WARN,sparse=dict(CONN_SPARSE,**{f"/{r}/{x}":{"policy":"densify","max_dense_elements":1000000} for r in ("plant","controller","sensor") for x in "ABCD"}),ver=V36)
fb_in=conn_in("control.lti.feedback",{"sign":{"enum":[-1,1],"default":-1,"x-aeoi-provenance":"convention"},"plant":LTI,"controller":LTI,"sensor":LTI},[],ver=V36)
fb_out=conn_out("control.lti.feedback",["polynomial_feedback","feedback_block","ss2tf_characteristic_polynomial"]); fb_out["$id"]=fb_out["$id"].replace(V3,V36)
pid=man("control.design.pid","PID controller","按 parallel 或 standard 形式构造 PID，返回可被其他算子使用的 tf（或 proper 时的 ss）。两种形式的字段互斥：parallel 只用 Kp、Ki、Kd、Tf；standard 只用 Kp、Ti、Td、Tf。不要的项直接省略，不要填 0 占位（尤其不要写 Ti = 0：没有积分就省略 Ti，或改用 parallel 并省略 Ki）。混用报 E.VALIDATION.OPTION_CONFLICT，附可直接应用的 JSON Patch。Tf = 0 表示不加微分滤波，微分项非零时结果 improper。返回的 result.system 原样作为 control_lti_series / control_lti_feedback 的 controller。","design",["pid","lti"],
 ["form=parallel：C = Kp + Ki/s + Kd·s/(Tf·s+1)；Ki 或 Kd 为 0 时该项不出现（精确去掉引入的因子，不是数值对消）","form=standard：C = Kp·(1 + 1/(Ti·s) + Td·s/(Tf·s+1))；省略 Ti 表示没有积分，Ti = 0 不是“没有积分”（会报 E.VALIDATION.OPTION_CONFLICT 并建议省略 Ti）","form 省略时默认 parallel；若只给了 standard 的字段（Ti/Td），报 OPTION_CONFLICT，补丁是加上 form = standard","Tf = 0 且有微分项时结果 improper，并给出 W.DOMAIN.IMPROPER_RESULT","增益是调用方给出的设计参数，没有安全默认值"],
 ["pidtune（目标函数、对象类和唯一性都还没写成规范，见 §12.2）","离散 PID"],
 "parallel：分子 [Kd, Kp, Ki]、分母 [1, 0]，Ki=0 时约去 s；Tf>0 时分母为 s(Tf·s+1)。standard 把同一分解写成 Kp、Ti、Td。零增益项按定义删除。",
 [R_OGATA,{"type":"textbook","cite":"K. J. Åström, T. Hägglund, Advanced PID Control, ISA, 2006（parallel 与 standard 两种参数化）"}],
 "pid::parallel / pid::standard","T1",{"max_relative_degree":1},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.RANGE","E.VALIDATION.OPTION_CONFLICT","E.DOMAIN.IMPROPER_SYSTEM","E.VALIDATION.NON_FINITE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.DOMAIN.IMPROPER_RESULT"],sparse={})
pid["version"]=V36; pid["credentials"]=pid["credentials"].replace(V3,V36); pid["backend"]["version"]=V36
pid_in=ins("control.design.pid",{
 "form":{"enum":["parallel","standard"],"default":"parallel","x-aeoi-provenance":"convention"},
 "Kp":{"type":"number","x-aeoi-provenance":"supplied"},
 "Ki":{"type":"number","default":0,"x-aeoi-provenance":"convention"},
 "Kd":{"type":"number","default":0,"x-aeoi-provenance":"convention"},
 "Ti":{"type":"number","x-aeoi-provenance":"supplied"},
 "Td":{"type":"number","minimum":0,"default":0,"x-aeoi-provenance":"convention"},
 "Tf":{"type":"number","minimum":0,"default":0,"x-aeoi-provenance":"convention"},
 "output_kind":{"enum":["tf","ss"],"default":"tf"},
 "lineage":LINEAGE,
},["Kp"],ver=V36)
pid_out=outs("control.design.pid",{
 "system":dict(LTI,description="控制器，ts=0。proper 时也可为 ss"),
 "realization":{"enum":["pid_closed_form","controllable_canonical"]},
 "minimal":{"type":["boolean","null"]},
 "proper":{"type":"boolean"},
 "parameters":{"type":"object","description":"回显的形式与增益，含规范默认值（Ki/Kd/Tf 缺省为 0）"},
 "composition":{"type":"object"},
},["system","realization","proper","parameters","composition"],ver=V36)
si=man("control.lti.stepinfo","Step-response metrics","在显式时间网格上给出阶跃指标：上升时间、调节时间、超调、欠调、峰值、峰值时间、稳态值。阈值有规范默认值。不稳定或不在窗口内调节的通道把依赖稳态的指标设为 null，峰值仍给出。","analysis",["time-domain","lti","stepinfo"],
 ["采样与 control.lti.step 相同（连续为 ZOH 精确递推）","时间指标是网格点，不插值：与连续穿越点之差 ≤ dt，记为 time_quantization_s，不占用 T2 的 rtol","rise_time_limits 默认 [0.1, 0.9]，settling_threshold 默认 0.02；这是指标定义，不是被测对象的物理参数","稳态值取 DC 增益，不取最后一个采样"],
 ["把未调节写成 Inf（窗口太短只说明没观察到，不说明永不调节）","插值到网格之间"],
 "先调用与 step 相同的递推得到 y[k]。上升时间 = t[k_hi]−t[k_lo]，k_lo/k_hi 是 sgn·(y−阈值·y∞) 第一次 ≥ 0 的下标。调节时间 = 最后一次 |y/y∞−1| ≥ 阈值 的下一格；若末点仍在带外则为 null，并给 W.DOMAIN.NOT_SETTLED。超调、欠调为百分数。不稳定：依赖 y∞ 的字段为 null，peak 为窗口内 max|y|，window_limited=true。",
 [R_OGATA,{"type":"software","cite":"python-control 0.10 step_info 文档（仅作网格定义对照：不插值、百分数超调）"}],
 "stepinfo::channel","T2",{"max_grid_points":10000000,"max_states":200},
 COMMON_LTI_ERR+["E.VALIDATION.GRID","E.VALIDATION.RANGE","E.RESOURCE.COMPUTE_LIMIT","E.RESOURCE.OUTPUT_TOO_LARGE"],
 ["W.DOMAIN.UNSTABLE_SYSTEM","W.DOMAIN.NOT_SETTLED","W.DOMAIN.INFINITE_DC_GAIN","W.DOMAIN.ZERO_DC_GAIN","W.ACCURACY.GRID_TOO_COARSE","W.ACCURACY.AUTO_GRID"]+LW)
si_in=ins("control.lti.stepinfo",{
 "system":LTI,
 "time":step_in["properties"]["time"],
 "rise_time_limits":{"type":"array","items":{"type":"number","minimum":0,"maximum":1},"minItems":2,"maxItems":2,"default":[0.1,0.9],"x-aeoi-provenance":"convention"},
 "settling_threshold":{"type":"number","exclusiveMinimum":0,"maximum":0.5,"default":0.02,"x-aeoi-provenance":"convention"},
},["system","time"])
si_out=outs("control.lti.stepinfo",{
 "definition":{"type":"object","description":"回显的阈值、grid_point 量化、超调单位 %、稳态来源 dcgain"},
 "stable":{"type":"boolean"},
 "n_grid_points":{"type":"integer"},
 "info":{"description":"SISO 为一个对象；MIMO 为 [ny][nu]。字段：rise_time、settling_time、settling_min、settling_max、overshoot（%）、undershoot（%）、peak、peak_time、steady_state、settled、window_limited。不适用的 value 为 null；无界稳态为 \"Inf\"。"},
},["definition","stable","n_grid_points","info"])
pade=man("control.lti.pade","Padé delay approximation","e^{-θ s} 的 [N/N] Padé 近似，返回 proper 的 biproper tf。delay 与 order 都没有默认值。逼近误差写在 result.approximation，与系数舍入分开。","transform",["delay","pade","lti"],
 ["delay > 0 由调用方给出，算子不假设","order N ∈ 1..=20 由调用方给出，算子不假设 N=1","只做对角近似（分子分母同阶），因此 |R(jω)| 在精确算术下为 1","结果是对 e^{-θ s} 的近似，不是舍入意义下的精确延迟"],
 ["非对角 Padé（分子阶次 ≠ N）","把逼近误差写成系数的舍入界"],
 "Golub & Van Loan 算法 11.3.1 的递推：降幂系数，常数项初值 1，第 k 步乘以 ∓θ·(N−k+1)/((2N−k+1)·k)，再把分母首项归一为 1。匹配 e^{-θ s} 的泰勒级数到 s^{2N}。result.approximation.next_coefficient 是 s^{2N+1} 的系数差；certificate 是 ω_k=(1/θ)·10^{−2+4k/31}（k=0..31）上 |e^{-jωθ}−R(jω)| 的最大值，bound 为 null（没有在本规范里证明的严格上界）。",
 [R_GVL_PADE,R_VAJTA],"pade::pade_tf","T1",{"max_order":20},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.RANGE","E.VALIDATION.NON_FINITE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 [])
pade_in=ins("control.lti.pade",{
 "delay":{"type":"number","exclusiveMinimum":0,"x-aeoi-provenance":"supplied"},
 "order":{"type":"integer","minimum":1,"maximum":20,"x-aeoi-provenance":"supplied"},
 "lineage":LINEAGE,
},["delay","order"])
pade_out=outs("control.lti.pade",{
 "system":dict(LTI,description="连续 tf，ts=0，分母首项为 1"),
 "realization":{"const":"pade_coefficients"},
 "minimal":{"type":["boolean","null"]},
 "proper":{"const":True},
 "approximation":{"type":"object","description":"target、method、matched_order、next_coefficient、certificate、bound（无严格界时为 null）"},
 "composition":{"type":"object"},
},["system","realization","proper","approximation","composition"])

# ---- v0.3.6: control.analysis.loopsens — one-shot loop analysis from named roles (spec 3.13 K-4/K-5)
LS_SPARSE={f"/{r}/{x}":{"policy":"densify","max_dense_elements":1000000} for r in ("plant","controller","sensor") for x in "ABCD"}
QN=lambda d:{"oneOf":[{"type":"null"},dict(Q,description=d)]}
ls=man("control.analysis.loopsens","Loop analysis from plant and controller",
 "一步得到单回路负反馈的开环、闭环和指标。输入对象 plant、控制器 controller（省略 = 1）、反馈通路 sensor（省略 = 单位反馈）。拓扑固定：r → e = r − sensor·y → controller → plant → y。返回 open_loop L = sensor·plant·controller（裕度、穿越频率作用在 L 上）、closed_loop T = plant·controller/(1+L)（r→y，阶跃指标作用在 T 上）、sensitivity S = 1/(1+L)；闭环极点与稳定性；L 的增益/相位裕度；两种带宽并各给 rad/s 与 Hz 两个字段：gain_crossover_*（|L| = 1 的开环穿越频率，常被叫作“回路带宽”）和 bandwidth_*（|T| 比直流增益低 3 dB 的闭环带宽）；给出 time 时还返回 T 的阶跃指标。结果等于依次调用 series、feedback、margin、stepinfo 的结果（同一 build 逐位相同），只是不会接错。",
 "analysis",["lti","feedback","frequency-domain","time-domain","loop"],
 ["SISO","plant、controller、sensor 的 ts 完全相同","controller 可以是 improper（理想微分）；open_loop / closed_loop 必须 proper 才能求裕度和阶跃指标，否则该项为 null 并给 W.DOMAIN.ANALYSIS_SKIPPED","负反馈；正反馈或其他拓扑用 control_lti_feedback 的位置参数","带宽定义：bandwidth_* 为 |T(jω)| 第一次降到 |T(0)| 以下 |bandwidth_drop_db| dB 的频率（默认 −3 dB）；gain_crossover_* 为 |L(jω)| = 1 的频率（多个时取 margin 选中的那个，全部见 margins.crossings.gain）","Hz 字段 = rad/s 字段 / (2π)，两者来自同一个数"],
 ["MIMO","时滞（先用 control_lti_pade 得到有理近似，再作为 plant 的一部分串联）","自动整定（pidtune 尚未定义，见 spec A-4）","非线性"],
 "forward = series(controller → plant)，L = series(forward → sensor)，T = feedback(forward, sensor, sign = −1)，S = feedback(1, L)：与 control.lti.series / control.lti.feedback 同一实现（spec I-5、I-6）。闭环极点为 T 的极点。裕度 = control.analysis.margin(L)。闭环带宽：连续系统令 P(ω) = |N(jω)|² − 10^{drop/10}·|T(0)|²·|D(jω)|²（实系数偶多项式），取最小正实根，再在 |T| 上二分修正；离散系统在 (0, π/ts) 上做 4000 点对数网格扫描后二分。阶跃指标 = control.lti.stepinfo(T, time)。",
 [R_OGATA,{"type":"textbook","cite":"K. J. Åström, R. M. Murray, Feedback Systems, 2nd ed., Princeton, 2021, ch.11–12（回路传递函数、灵敏度函数、带宽定义）"},{"type":"software","cite":"python-control 0.10 bandwidth / stability_margins 文档（仅作定义对照）"}],
 "loopsens::analyze (connect + margin + stepinfo)","T3",{"max_states":100,"max_degree":64,"max_relative_degree":8,"bandwidth_scan_points":4000,"max_grid_points":10000000},
 COMMON_LTI_ERR+["E.DOMAIN.SAMPLE_TIME_MISMATCH","E.DOMAIN.MIMO_NOT_SUPPORTED","E.DOMAIN.ALGEBRAIC_LOOP","E.VALIDATION.GRID","E.VALIDATION.RANGE","E.RESOURCE.COMPUTE_LIMIT","E.RESOURCE.OUTPUT_TOO_LARGE"],
 None,sparse=LS_SPARSE,siso_only=True,ver=V36)
ls_in=ins("control.analysis.loopsens",{
 "plant":LTI,"controller":LTI,"sensor":LTI,
 "time":dict(step_in["properties"]["time"]),
 "rise_time_limits":{"type":"array","items":{"type":"number","minimum":0,"maximum":1},"minItems":2,"maxItems":2,"default":[0.1,0.9],"x-aeoi-provenance":"convention"},
 "settling_threshold":{"type":"number","exclusiveMinimum":0,"maximum":0.5,"default":0.02,"x-aeoi-provenance":"convention"},
 "bandwidth_drop_db":{"type":"number","exclusiveMaximum":0,"minimum":-60,"default":-3,"x-aeoi-provenance":"convention"},
 "lineage":LINEAGE,
},["plant"],ver=V36)
ls_out=outs("control.analysis.loopsens",{
 "open_loop":dict(LTI,description="L = sensor·plant·controller。margin、穿越频率作用在它上面"),
 "closed_loop":dict(LTI,description="T = plant·controller/(1+L)，r→y。stepinfo/step 作用在它上面"),
 "sensitivity":dict(LTI,description="S = 1/(1+L)，r→e"),
 "roles":{"type":"object","description":"每个返回系统的角色说明（open_loop/closed_loop/sensitivity → 文字定义）"},
 "closed_loop_poles":CVEC("c128 向量，T 的极点，按实部升序、虚部降序"),
 "closed_loop_stable":{"type":"boolean","description":"连续：T 的极点全部 Re < 0；离散：|p| < 1"},
 "margins":{"type":["object","null"],"description":"control.analysis.margin(L) 的 result；L 不 proper 时为 null"},
 "gain_crossover_rad_s":QN("unit=rad/s：|L(jω)| = 1 的开环穿越频率（margin 选中的那个）；没有穿越为 \"NaN\""),
 "gain_crossover_hz":QN("unit=Hz：同一个穿越频率 / (2π)"),
 "bandwidth_rad_s":QN("unit=rad/s：闭环 |T| 比 |T(0)| 低 |bandwidth_drop_db| dB 的第一个频率；T(0) = 0 或无穷时为 null，全频段不降为 \"Inf\""),
 "bandwidth_hz":QN("unit=Hz：同一个闭环带宽 / (2π)"),
 "bandwidth_definition":{"type":"object","description":"drop_db、reference（dc_gain）、method（even_polynomial_roots 或 grid_bisection）"},
 "step_metrics":{"type":["object","null"],"description":"给出 time 时为 control.lti.stepinfo(T, time) 的 result，否则为 null"},
 "composition":{"type":"object","description":"operation = loopsens，operands[]（role 与 sha256），controller_implicit、sensor_implicit、topology、lineage"},
},["open_loop","closed_loop","sensitivity","roles","closed_loop_poles","closed_loop_stable","margins","gain_crossover_rad_s","gain_crossover_hz","bandwidth_rad_s","bandwidth_hz","bandwidth_definition","step_metrics","composition"],ver=V36)
# ---- control.lti.lsim (v0.4.5): response to an arbitrary sampled input
V45="0.4.0"
lsim=man("control.lti.lsim","Response to an arbitrary input","计算 LTI 系统对任意采样输入 u 的响应（斜坡、正弦、扰动、实测数据），可给初始状态。采样之间的输入按 interpolation 解释：foh（线性，缺省）或 zoh（保持）；对这个解释，采样点上的响应没有离散化误差。","analysis",["time-domain","lti","simulation"],
 ["system 为 proper","u 的形状 [nu, nt]（第 i 行 = 第 i 个输入通道，第 k 列 = t_k = t0 + k·dt 时刻）；nu = 1 时也可给一维 [nt]","均匀网格：t_k = t0 + k·dt，k = 0..nt−1；非均匀时间戳先重采样","离散系统要求 dt = ts（相对误差 1e-9），interpolation 不适用（给出即 E.VALIDATION.OPTION_CONFLICT，F-8）","x0 只用于 kind = ss 的系统：状态坐标由给定实现定义；tf/zpk 没有状态坐标，给 x0 即 E.VALIDATION.OPTION_CONFLICT（先用 control.lti.convert 转成 ss，那个实现有规范定义）"],
 ["非均匀采样的输入","非线性系统","带时滞的系统（用 control.lti.pade 近似后串联）","采样之间输入形状既不是线性也不是常数时的精确响应（结果对 foh/zoh 的解释精确，对真实输入的误差取决于采样）"],
 "连续系统：对增广矩阵 M = [[A·dt, B·dt, 0], [0, 0, I], [0, 0, 0]] 求矩阵指数（Padé 13 阶 + scaling & squaring），E = expm(M)，Φ = E[x,x]，F1 = E[x,u1]，F2 = E[x,u2]；foh：x_{k+1} = Φx_k + (F1 − F2)u_k + F2·u_{k+1}；zoh：x_{k+1} = Φx_k + F1·u_k（M 去掉第三块）。离散系统：x_{k+1} = Ax_k + Bu_k。y_k = Cx_k + Du_k，x_0 = x0（缺省 0）。与 scipy.signal.lsim（interp=True/False）及 python-control forced_response 的公式相同。tf/zpk 先转为可控标准型实现（与 control.lti.step 相同）。后验检查：在 dt/2 上重新离散化，用同一插值输入跑前 64 步，偶数步状态须与原递推一致",
 [{"type":"paper","cite":"C. Van Loan, Computing integrals involving the matrix exponential, IEEE TAC 23(3), 1978","doi":"10.1109/TAC.1978.1101743"},R_HIGHAM,R_FPE],
 "lsim::simulate (block expm Padé-13 + recursion)","T2",{"max_grid_points":10000000,"max_output_values":1000000,"half_step_check_steps":64},
 COMMON_LTI_ERR+["E.VALIDATION.GRID","E.VALIDATION.OPTION_CONFLICT","E.RESOURCE.COMPUTE_LIMIT","E.RESOURCE.OUTPUT_TOO_LARGE"],
 ["W.DOMAIN.UNSTABLE_SYSTEM","W.ACCURACY.GRID_TOO_COARSE"]+LW,
 sparse=dict(SS_SPARSE,**{"/u":{"policy":"densify","max_dense_elements":10000000}}),ver=V45)
# ---- control.design.pidtune (v0.4.6): PI/PID by crossover frequency + phase margin (A-4)
pt=man("control.design.pidtune","PI/PID tuning by crossover and phase margin","按给定的穿越频率 ωc 与相位裕度整定 PI 或 PID：使开环 L = G·C 满足 |L(jωc)| = 1、∠L(jωc) = −180° + PM，并核验闭环稳定。PI 有封闭解且唯一；PID 用 Ti = α·Td（缺省 α = 4）与微分滤波 Tf = Td/N（缺省 N = 10）固定多出的自由度，Td 取规范规定扫描下的最小解。","design",["pid","tuning","loop-shaping"],
 ["plant 为 SISO、连续、proper","ωc（wc_rad_s）由调用方给出，算子不猜","PI 可达条件：所需控制器相位 ∠C ∈ (−90°, 0°]；PID 可达条件：∠C 落在 H(x) 的相位范围内（见 10.16）","结果闭环必须稳定，否则 E.DOMAIN.TARGET_NOT_ACHIEVABLE"],
 ["离散系统（E.DOMAIN.NOT_CONTINUOUS）","MIMO","带时滞的对象（先用 control.lti.pade 近似后串联）","时域指标（超调、调节时间）直接作为目标","自动选择 ωc"],
 "所需控制器值 C* = e^{j(PM−180°)}/G(jωc)。PI：C(jω) = Kp − j·Ki/ω ⇒ Kp = Re C*，Ki = −ωc·Im C*。PID（标准形，Ti = α·Td，Tf = Td/N）：令 x = ωc·Td，H(x) = 1 − j/(αx) + jx/(1 + jx/N)，在 x_k = 10^{−6 + 12k/4000}（k = 0..4000）上找第一个使 arg H(x) − ∠C* 变号的区间，对 log x 二分到相对宽度 ≤ 4u，Kp = |C*|/|H(x)|，Td = x/ωc；并联增益 Ki = Kp/Ti、Kd = Kp·Td。控制器由 control.design.pid（parallel）构造，回路分析由 control.analysis.loopsens 在同一 JSON 上完成（K-4）",
 [{"type":"textbook","cite":"K. J. Åström, T. Hägglund, Advanced PID Control, ISA 2006（频率点设计、Ti/Td 比与微分滤波）"},
  {"type":"textbook","cite":"G. F. Franklin, J. D. Powell, A. Emami-Naeini, Feedback Control of Dynamic Systems, 8th ed.（相位裕度设计）"}],
 "pidtune::frequency_point (closed-form PI; log-scan + bisection for PID Td)","T2",{"td_scan_points":4001,"td_scan_log10_min":-6,"td_scan_log10_max":6,"max_bisection_iterations":200},
 COMMON_LTI_ERR+["E.DOMAIN.MIMO_NOT_SUPPORTED","E.DOMAIN.NOT_CONTINUOUS","E.DOMAIN.TARGET_NOT_ACHIEVABLE","E.VALIDATION.OPTION_CONFLICT","E.NUMERICAL.NO_CONVERGENCE"],
 ["W.DOMAIN.EXTRA_CROSSOVER","W.DOMAIN.ANALYSIS_SKIPPED"]+LW,siso_only=True,ver="0.4.0")
pt_in=ins("control.design.pidtune",{"plant":LTI,"type":{"enum":["PI","PID"]},
 "wc_rad_s":{"type":"number","exclusiveMinimum":0,"x-aeoi-provenance":"user"},
 "phase_margin_deg":{"type":"number","exclusiveMinimum":0,"maximum":90,"default":60,"x-aeoi-provenance":"convention"},
 "ti_td_ratio":{"type":"number","exclusiveMinimum":0,"default":4,"x-aeoi-provenance":"convention"},
 "derivative_filter_n":{"type":"number","exclusiveMinimum":0,"default":10,"x-aeoi-provenance":"convention"},
 "lineage":{"type":["string","array","null"]}},["plant","type","wc_rad_s"],ver="0.4.0")
pt_out=outs("control.design.pidtune",{"type":{"enum":["PI","PID"]},"controller":dict(LTI,description="control.design.pid（form = parallel）用 gains 构造的控制器；可原样传给 loopsens / feedback"),
 "gains":{"type":"object","description":"并联形式 Kp、Ki、Kd、Tf（与 control.design.pid 的输入相同）"},
 "gains_standard":{"type":"object","description":"标准形式 Kp、Ti、Td、Tf；Ti 为 null 表示没有积分（Ki = 0）"},
 "target":{"type":"object","description":"wc_rad_s、phase_margin_deg（请求的目标）"},
 "achieved":{"type":"object","description":"control.analysis.loopsens(plant, controller) 给出的：closed_loop_stable、gain_crossover_rad_s、phase_margin_deg、gain_margin"},
 "closed_loop_poles":dict(M,description="c128 向量：control.analysis.loopsens 给出的闭环极点"),
 "design":{"type":"object","description":"method = frequency_point；required_controller_phase_deg、required_controller_gain；PID 时 td_search（x = ωc·Td 的扫描与二分）、ti_td_ratio、derivative_filter_n"}},
 ["type","controller","gains","gains_standard","target","achieved","closed_loop_poles","design"],ver="0.4.0")
# ======================================================================================================
# Domains outside control (crate aeoi-numeric). v0.4.7: signal.spectral.fft
BACK_NUM=lambda routine,extra="":{"library":"aeoi-numeric (pure Rust)"+extra,"routine":routine,"binding":"native","license":"MIT OR Apache-2.0"}
V47="0.4.0"
fft=man("signal.spectral.fft","Discrete Fourier transform","计算离散傅里叶变换（正变换或逆变换），并给出每个频点对应的频率。约定全部显式：符号 e^{−2πi·jk/n}、归一化（backward 缺省，与 MATLAB fft/ifft、numpy.fft 相同）、双边或单边、频率轴顺序（numpy fftfreq）。","analysis",["signal","spectral","fft"],
 ["x 为 f64 或 c128，形状 [n] 或 [m, n]（沿最后一维变换，每行一个通道）","n（变换长度）缺省为最后一维长度；更短则截断、更长则在末尾补零（= MATLAB fft(x, n)）","sides = one 只用于实数输入的正变换（返回 n/2 + 1 个非负频点）"],
 ["非均匀采样","窗函数与功率谱密度估计（另设算子）","三维及以上数组"],
 "定义：正变换 X_k = s·Σ_{j=0}^{n−1} x_j·e^{−2πi·jk/n}，逆变换 x_j = s'·Σ_k X_k·e^{+2πi·jk/n}；normalization = backward：s = 1、s' = 1/n；ortho：s = s' = 1/√n；forward：s = 1/n、s' = 1。快速算法：aeoi-core kernels::fft（2 的幂用基 2；合数按最小素因子做混合基按时间抽取；素数 ≤ 31 直接 DFT，更大的素数用 Bluestein），旋转因子用 libm::cos/sin，不使用运行时 SIMD 分派，结果只依赖输入与 n。频率轴：双边按 numpy.fft.fftfreq 顺序 f_k = k·fs/n（k < ⌈n/2⌉），其余为 (k − n)·fs/n；单边按 rfftfreq，f_k = k·fs/n，k = 0..⌊n/2⌋。后验检查：Parseval 能量恒等式（全部输出），以及在 8 个确定的频点上用 O(n) 补偿求和直接计算 DFT（抽查）",
 [{"type":"paper","cite":"J. W. Cooley, J. W. Tukey, An algorithm for the machine calculation of complex Fourier series, Math. Comp. 19, 1965","doi":"10.1090/S0025-5718-1965-0178586-1"},
  {"type":"paper","cite":"L. Bluestein, A linear filtering approach to the computation of discrete Fourier transform, IEEE Trans. Audio Electroacoust. 18(4), 1970","doi":"10.1109/TAU.1970.1162132"},
  {"type":"textbook","cite":"N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM 2002, §24（FFT 的舍入误差界）"}],
 "fft::transform (aeoi-core kernels::fft)","T1",{"max_input_elements":10000000,"max_output_values":10000000,"dft_spot_check_bins":8},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.OPTION_CONFLICT","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.OUTPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.PERFORMANCE.SPARSE_DENSIFIED"],
 sparse={"/x":{"policy":"densify","max_dense_elements":10000000}},ver=V47,backend=BACK_NUM("fft::transform"," + aeoi-core kernels::fft"))
fft_in=ins("signal.spectral.fft",{"x":M,"n":{"type":"integer","minimum":1},
 "direction":{"enum":["forward","inverse"],"default":"forward","x-aeoi-provenance":"convention"},
 "normalization":{"enum":["backward","ortho","forward"],"default":"backward","x-aeoi-provenance":"convention"},
 "sides":{"enum":["two","one"],"default":"two","x-aeoi-provenance":"convention"},
 "sample_rate_hz":{"type":"number","exclusiveMinimum":0,"x-aeoi-provenance":"user"}},["x"],
 {"x-aeoi-shape-constraints":["x: [n] 或 [m, n]，沿最后一维变换"]},ver=V47)
fft_out=outs("signal.spectral.fft",{"X":dict(M,description="c128，形状 [n_out] 或 [m, n_out]；n_out = n（双边）或 ⌊n/2⌋ + 1（单边）"),
 "frequency":dict(Q,description="每个输出频点的频率：unit Hz（给了 sample_rate_hz）或 cycles/sample；双边按 numpy fftfreq 顺序（0、正频率、负频率），单边为 0..fs/2"),
 "n":{"type":"integer","description":"实际变换长度"},"direction":{"enum":["forward","inverse"]},"normalization":{"enum":["backward","ortho","forward"]},"sides":{"enum":["two","one"]},
 "input_length":{"type":"integer","description":"x 最后一维的原长度；与 n 不同表示截断或补零"},"frequency_resolution":dict(Q,description="相邻频点间隔 fs/n")},
 ["X","frequency","n","direction","normalization","sides","input_length","frequency_resolution"],ver=V47)
# ---- v0.4.8: ode.ivp.solve (expression language, spec 3.15)
V48="0.4.0"
EXPR={"type":"string","minLength":1,"maxLength":10000,"description":"AEOI 表达式（3.15）：+ − * / ^、括号、数字、已声明的名字、t、常数 pi，以及固定函数表"}
NAME={"type":"string","pattern":"^[A-Za-z_][A-Za-z0-9_]*$"}
ode=man("ode.ivp.solve","Initial value problem for ODEs","求解常微分方程组的初值问题 dy/dt = f(t, y)，y(t0) = y0。右端 f 用 AEOI 表达式语言写（3.15），状态与参数用名字声明，不按下标引用。显式 Runge–Kutta：Dormand–Prince 5(4)（与 MATLAB ode45、scipy RK45 同一方法），自适应步长按 rtol/atol 控制，输出点用 4 阶稠密输出。并用 10 倍更严的容差重解一遍，报告估计的全局误差。","analysis",["ode","ivp","simulation","runge-kutta"],
 ["states 为不重复的名字，rhs 与之一一对应（第 i 个表达式是 d(states[i])/dt）","initial 给出每个状态的初值","t_span = [t0, tf]，tf ≠ t0；tf < t0 时反向积分","t_eval 在区间内、沿积分方向单调；与 n_out 二选一","非刚性问题；刚性问题会给 W.NUMERICAL.STIFF_PROBLEM"],
 ["刚性问题的高效求解（需要隐式方法，将来作为同一算子的另一方法登记，3.14）","事件检测（过零停止）","微分代数方程（DAE）","带时滞的方程（DDE）","随机微分方程"],
 "Dormand–Prince 5(4)，FSAL，系数与 scipy.integrate.RK45 相同（10.18 OD-3 列出）。误差：err = RMS_i(h·Σ_j E_j K_ji / (atol + rtol·max(|y_i|, |y_new,i|)))；err < 1 接受。步长因子：接受时 min(10, 0.9·err^{−1/5})（err = 0 时 10；刚被拒过的步不放大），拒绝时 max(0.2, 0.9·err^{−1/5})；右端出现非有限值的试探步按拒绝处理（因子 0.2）。最小步长 10·|nextafter(t) − t|。初始步长：Hairer–Nørsett–Wanner II.4 的经验算法（与 scipy select_initial_step 相同）。输出点：落在 (t_old, t] 内的点用该步的 4 阶稠密输出（Shampine P 矩阵）求值，t0 处即 y0。全局误差估计：同一方法以 rtol/10、atol/10 重解，在输出点上比较。刚性检测：Hairer DOPRI5 的 h·λ 估计连续 15 步 > 3.25",
 [{"type":"paper","cite":"J. R. Dormand, P. J. Prince, A family of embedded Runge-Kutta formulae, J. Comput. Appl. Math. 6(1), 1980","doi":"10.1016/0771-050X(80)90013-3"},
  {"type":"paper","cite":"L. F. Shampine, Some practical Runge-Kutta formulas, Math. Comp. 46(173), 1986","doi":"10.1090/S0025-5718-1986-0815836-3"},
  {"type":"textbook","cite":"E. Hairer, S. P. Nørsett, G. Wanner, Solving Ordinary Differential Equations I: Nonstiff Problems, 2nd ed., Springer 1993, §II.4（步长控制、初始步长）与 §IV.2（DOPRI5 刚性检测）"}],
 "ode::dopri54 (explicit Runge-Kutta, adaptive)","T4",{"max_states":1000,"max_steps":1000000,"max_output_points":100000,"max_output_values":1000000,"global_error_refinement":10,"tolerance_consistency_factor":100},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.EXPRESSION","E.VALIDATION.SHAPE","E.VALIDATION.RANGE","E.VALIDATION.GRID","E.VALIDATION.OPTION_CONFLICT","E.VALIDATION.NON_FINITE","E.NUMERICAL.NON_FINITE_EVALUATION","E.NUMERICAL.STEP_SIZE_TOO_SMALL","E.RESOURCE.COMPUTE_LIMIT","E.RESOURCE.OUTPUT_TOO_LARGE","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.NUMERICAL.STIFF_PROBLEM"],sparse={},ver=V48,backend=BACK_NUM("ode::dopri54"))
ode_in=ins("ode.ivp.solve",{"states":{"type":"array","items":NAME,"minItems":1,"maxItems":1000,"uniqueItems":True},
 "rhs":{"type":"array","items":EXPR,"minItems":1,"maxItems":1000},
 "params":{"type":"object","propertyNames":{"pattern":"^[A-Za-z_][A-Za-z0-9_]*$"},"additionalProperties":{"type":"number"}},
 "initial":{"type":"object","additionalProperties":{"type":"number"}},
 "t_span":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2},
 "t_eval":{"type":"array","items":{"type":"number"},"minItems":1,"maxItems":100000},
 "n_out":{"type":"integer","minimum":2,"maximum":100000,"default":101,"x-aeoi-provenance":"convention"},
 "rtol":{"type":"number","exclusiveMinimum":0,"default":0.001,"x-aeoi-provenance":"convention"},
 "atol":{"type":"number","exclusiveMinimum":0,"default":1e-06,"x-aeoi-provenance":"convention"},
 "max_step":{"type":"number","exclusiveMinimum":0},"first_step":{"type":"number","exclusiveMinimum":0}},
 ["states","rhs","initial","t_span"],{"x-aeoi-shape-constraints":["rhs 与 states 等长","initial 的键 = states"]},ver=V48)
ode_out=outs("ode.ivp.solve",{"t":dict(M,description="输出时刻 [n_out]（t_eval，或 t0 到 tf 的 n_out 个等距点，与 numpy.linspace 相同）"),
 "y":dict(M,description="解 [n_states, n_out]：第 i 行是 states[i]"),"states":{"type":"array","items":{"type":"string"}},
 "stats":{"type":"object","description":"n_steps（接受的步）、n_rejected、n_rhs_evals（不含全局误差重解）"},
 "tolerance":{"type":"object","description":"实际使用的 rtol、atol（回显）"},
 "global_error":{"type":"object","description":"以 rtol/10、atol/10 重解得到的估计：max_abs（每个状态在输出点上的最大 |y − y_fine|）与 max_scaled（max |y − y_fine| / (atol + rtol·|y|)）"}},
 ["t","y","states","stats","tolerance","global_error"],ver=V48)
# ---- v0.4.10: linalg.system.solve, interp.grid.interp1
V410="0.4.0"
lsol=man("linalg.system.solve","Solve a linear system","解线性方程组 A·X = B（A 为实数稠密矩阵）。方阵：带部分选主元的 LU 分解，再用补偿算术算残差做至多 3 步迭代精化；超定（行多于列）：Householder QR 求最小二乘解；欠定（行少于列）报错。结果带后向误差、1-范数条件数估计与前向误差估计（= MATLAB 的 A\b 用于方阵与超定的情形）。","analysis",["linalg","solve","least-squares","mldivide"],
 ["A 为 [m, n] 实数矩阵，m ≥ n（方阵或超定）","b 为 [m] 或 [m, k]（k 个右端项，每列一个）","A 列满秩（方阵即非奇异）"],
 ["欠定方程组（m < n；最小范数解将来作为另一方法）","复数方程组（0.4.0 未支持）","大型稀疏方程组（需迭代法或稀疏分解）","特殊结构（对称正定、带状、三角）的专用加速"],
 "方阵：LU 分解 P·A = L·U，部分选主元（每列取 |a_ik| 最大的第一行，与 LAPACK dgetf2 相同），前代/回代求解；迭代精化：r = b − A·x 用补偿算术（Dot2，Ogita–Rump–Oishi，无 FMA）计算，解 A·δ = r，x ← x + δ，‖δ‖∞ ≤ 2u‖x‖∞ 或修正量未减半即停，至多 3 步。超定：Householder QR（LAPACK dgeqr2/dlarfg 约定），x = R⁻¹·(Qᵀb) 的前 n 行；秩判定 |r_jj| ≤ max(m, n)·u·max|r_ii| 报 E.NUMERICAL.SINGULAR。条件数：‖A‖₁·est(‖A⁻¹‖₁)（超定用 R），Hager–Higham 1-范数估计（至多 5 次迭代 + 交错向量）。后向误差：方阵 η = ‖b − A·x‖∞ / (‖A‖∞‖x‖∞ + ‖b‖∞)；超定 η = ‖Aᵀr‖₂ / (‖A‖_F·(‖r‖₂ + ‖A‖_F‖x‖₂))，残差均以补偿算术计算",
 [{"type":"textbook","cite":"N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM 2002, §9（LU）、§12（迭代精化）、§15（条件数估计）、§19–20（QR 与最小二乘）"},
  {"type":"paper","cite":"N. J. Higham, FORTRAN codes for estimating the one-norm of a real or complex matrix, ACM TOMS 14(4), 1988","doi":"10.1145/50063.214386"},
  {"type":"paper","cite":"T. Ogita, S. M. Rump, S. Oishi, Accurate sum and dot product, SIAM J. Sci. Comput. 26(6), 2005","doi":"10.1137/030601818"}],
 "linsolve::solve (LU partial pivoting + refinement / Householder QR)","T1",{"max_dimension":2000,"max_rhs":1000,"max_refinement_steps":3},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN","E.DOMAIN.UNDERDETERMINED","E.NUMERICAL.SINGULAR","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.NUMERICAL.ILL_CONDITIONED","W.PERFORMANCE.SPARSE_DENSIFIED"],
 sparse={"/A":{"policy":"densify","max_dense_elements":4000000}},ver=V410,backend=BACK_NUM("linsolve::solve"))
lsol_in=ins("linalg.system.solve",{"A":M,"b":M,"refinement":{"enum":["auto","none"],"default":"auto","x-aeoi-provenance":"convention"}},["A","b"],
 {"x-aeoi-shape-constraints":["A: [m, n]，m ≥ n","b: [m] 或 [m, k]"]},ver=V410)
lsol_out=outs("linalg.system.solve",{"x":dict(M,description="解：b 为 [m] 时是 [n]，b 为 [m, k] 时是 [n, k]"),
 "residual_norm":{"description":"‖b − A·x‖₂（每个右端项一个；b 为向量时是一个数）"},
 "method":{"enum":["lu_partial_pivoting","householder_qr"]},
 "shape":{"type":"object","description":"m、n、rhs（右端项个数）、kind（square 或 overdetermined）"},
 "refinement_steps":{"type":"integer","description":"迭代精化实际做的步数（各右端项取最大；超定为 0）"}},
 ["x","residual_norm","method","shape","refinement_steps"],ver=V410)
itp=man("interp.grid.interp1","1-D interpolation","一维插值：给定严格递增的采样点 x 与数据 y，求查询点 xq 上的值。方法 linear（缺省）、nearest、previous、next、pchip（保形三次）、spline（not-a-knot 三次样条），与 MATLAB interp1 的同名方法相同。查询点超出数据范围时缺省报错，由调用方显式选择 nan、clamp 或 extrapolate。","analysis",["interpolation","interp1","spline","pchip","table"],
 ["x 为 [n] 严格递增，n ≥ 2","y 为 [n] 或 [k, n]（每行一个通道）","xq 为 [q]，任意顺序"],
 ["散乱数据与多维插值（interp2/griddata）","平滑与拟合（数据有噪声时用拟合，不用插值）","周期样条与其他端点条件"],
 "区间定位：二分查找，x_i ≤ xq < x_{i+1}（xq = x_{n−1} 取最后一段）；xq 恰为节点时直接返回该点的 y。linear：slope·(xq − x_i) + y_i（numpy.interp 的运算顺序）。nearest：xq ≥ x_i/2 + x_{i+1}/2 时取 y_{i+1}，否则 y_i（中点归右，同 MATLAB 与 scipy nearest-up）。previous/next：y_i / y_{i+1}。pchip：Fritsch–Butland 加权调和平均斜率，端点用保形三点公式（Moler, Numerical Computing with MATLAB §3.6）。spline：not-a-knot 端点条件的斜率方程组（三对角，LAPACK dgtsv 部分选主元；n = 3 为过三点的抛物线，n = 2 为直线）。三次段按 scipy PPoly 求值 ((c0·d + c1)·d + c2)·d + c3。范围外：extrapolation = error（缺省，E.DOMAIN.EXTRAPOLATION_REQUIRED）/ nan / clamp（端点值）/ extrapolate（linear 与三次方法延长端点那一段；nearest/previous/next 取端点值）",
 [{"type":"paper","cite":"F. N. Fritsch, J. Butland, A method for constructing local monotone piecewise cubic interpolants, SIAM J. Sci. Stat. Comput. 5(2), 1984","doi":"10.1137/0905021"},
  {"type":"textbook","cite":"C. de Boor, A Practical Guide to Splines, rev. ed., Springer 2001, Ch. IV（not-a-knot 端点条件）"},
  {"type":"textbook","cite":"C. Moler, Numerical Computing with MATLAB, SIAM 2004, §3（pchip、spline）"}],
 "interp1::eval (piecewise linear / cubic Hermite / not-a-knot spline)","T1",{"max_points":10000000,"max_output_values":10000000},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.GRID","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN","E.DOMAIN.EXTRAPOLATION_REQUIRED","E.NUMERICAL.SINGULAR","E.RESOURCE.OUTPUT_TOO_LARGE","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.ACCURACY.EXTRAPOLATED"],sparse={},ver=V410,backend=BACK_NUM("interp1::eval"))
itp_in=ins("interp.grid.interp1",{"x":M,"y":M,"xq":M,
 "method":{"enum":["linear","nearest","previous","next","pchip","spline"],"default":"linear","x-aeoi-provenance":"convention"},
 "extrapolation":{"enum":["error","nan","clamp","extrapolate"],"default":"error","x-aeoi-provenance":"convention"}},["x","y","xq"],
 {"x-aeoi-shape-constraints":["x: [n] 严格递增","y: [n] 或 [k, n]","xq: [q]"]},ver=V410)
itp_out=outs("interp.grid.interp1",{"yq":dict(M,description="插值结果：y 为 [n] 时 [q]，y 为 [k, n] 时 [k, q]；extrapolation = nan 时范围外为 NaN"),
 "method":{"enum":["linear","nearest","previous","next","pchip","spline"]},"extrapolation":{"enum":["error","nan","clamp","extrapolate"]},
 "out_of_range_count":{"type":"integer","description":"落在 [x_0, x_{n−1}] 之外的查询点个数"}},
 ["yq","method","extrapolation","out_of_range_count"],ver=V410)
OUTSEL={"type":"object","additionalProperties":False,"properties":{"return_samples":{"enum":["all","decimated","none"],"default":"all"},"max_samples":{"type":"integer","minimum":2,"default":10001}}}
lsim_in=ins("control.lti.lsim",{"system":LTI,"u":M,
 "time":{"type":"object","additionalProperties":False,"required":["dt","unit"],"properties":{"dt":{"type":"number","exclusiveMinimum":0},"t0":{"type":"number","default":0},"unit":{"const":"s"}}},
 "interpolation":{"enum":["foh","zoh"],"default":"foh","x-aeoi-provenance":"convention"},
 "x0":M,"return_states":{"type":"boolean","default":False},"output":OUTSEL},["system","u","time"],
 {"x-aeoi-shape-constraints":["u: [nu, nt]（nu = 1 时也可 [nt]）","x0: [n]，只用于 ss"]},ver=V45)
lsim_out=outs("control.lti.lsim",{"t":{"oneOf":[{"type":"null"},Q]},"y":{"oneOf":[{"type":"null"},dict(Q,description="shape [ny, nt]")]},
 "x":{"oneOf":[{"type":"null"},dict(M,description="[n, nt]，return_states = true 时给出；坐标由 realization 定义")]},
 "n_grid_points":{"type":"integer"},"interpolation":{"enum":["foh","zoh","none"],"description":"实际使用的插值；离散系统为 none"},
 "realization":{"type":"string","description":"状态坐标所属的实现：given ss，或 tf/zpk 的可控标准型"}},
 ["t","y","x","n_grid_points","interpolation","realization"],ver=V45)
B3={"control.lti.series":(series,series_in,series_out),"control.lti.parallel":(par,par_in,par_out),"control.lti.feedback":(fb,fb_in,fb_out),
 "control.design.pid":(pid,pid_in,pid_out),"control.lti.stepinfo":(si,si_in,si_out),"control.lti.pade":(pade,pade_in,pade_out),
 "control.analysis.loopsens":(ls,ls_in,ls_out),"control.lti.lsim":(lsim,lsim_in,lsim_out),"control.design.pidtune":(pt,pt_in,pt_out),"signal.spectral.fft":(fft,fft_in,fft_out),"ode.ivp.solve":(ode,ode_in,ode_out),"linalg.system.solve":(lsol,lsol_in,lsol_out),"interp.grid.interp1":(itp,itp_in,itp_out)}
_ws=[]
for _m in (series,fb,mg,si):
    _ws+= [w for w in _m["warnings"] if w not in _ws]
ls["warnings"]=_ws+["W.DOMAIN.ANALYSIS_SKIPPED"]
# v0.4 rule L-1: every top-level input property carries a description (meaning, shape, unit, enum values)
# so that function-calling consumers — which drop unknown keywords such as x-aeoi-shape-constraints —
# still see it. DESC wins over a shorter inline description (the inline c2d.ts text lacked the ask-the-user rule).
SYS_D="被分析的 LTI 系统：tf（num/den 按降幂）、ss（A/B/C/D 稠密矩阵，row-major）或 zpk；ts=0 连续，ts>0 为离散采样周期（s）。上一个工具返回了 result.system 时，把那个 JSON 对象原样放在这里，不要手抄或改写系数（spec K-6）"
LQR_D={"A":"状态矩阵 A，[n,n]","B":"输入矩阵 B，[n,m]（单输入时为列向量 [n,1]）","Q":"状态权重 Q，[n,n]，对称半正定","R":"输入权重 R，[m,m]，对称正定（标量写成 [1,1]）","N":"交叉权重 N，[n,m]，缺省为 0"}
DESC={
 "control.lti.bode":{"system":SYS_D,"frequency":"频率网格：explicit（values 严格递增）或 logspace（min..max 共 n 点，对数均匀）；unit 为 rad/s 或 Hz，必须与用户给出的单位一致（ω = 2π·f）","phase":"相位展开与分支选择（可省略）"},
 "control.lti.step":{"system":SYS_D,"time":"时间网格：uniform（t_final、dt，单位 s）或 auto（由算子按系统时间常数选择）；离散系统要求 dt = ts","step_info":"阶跃指标（上升时间、调节时间、超调等）；enabled=true 时计算","output":"返回的采样点：all / decimated（最多 max_samples 点）/ none"},
 "control.lti.impulse":{"system":SYS_D,"time":"时间网格：uniform（t_final、dt，单位 s）或 auto；离散系统要求 dt = ts","output":"返回的采样点：all / decimated（最多 max_samples 点）/ none","discrete_scaling":"离散冲激的幅值约定：unit_area（幅值 1/ts，与连续冲激面积一致，默认）或 unit_pulse（幅值 1，即 Kronecker δ）；连续系统忽略"},
 "control.design.lqr":dict(LQR_D),
 "control.design.dlqr":dict(LQR_D,A="离散状态矩阵 A（x[k+1] = A x[k] + B u[k]），[n,n]"),
 "control.design.place":{"A":"状态矩阵 A，[n,n]","B":"输入矩阵 B，[n,m]","max_sweeps":"多输入 KNV 迭代的最大扫描次数（可省略）"},
 "control.math.lyap":{"A":"系数矩阵 A，[n,n]","Q":"右端矩阵 Q，[n,n]。可以不对称：按原样求解，不对称化（10.10）。Q 对称时解 X 也对称","type":"continuous：A X + X Aᵀ + Q = 0；discrete：A X Aᵀ − X + Q = 0"},
 "control.lti.convert":{"system":SYS_D,"to":"目标表示：tf（传递函数）、ss（状态空间，可控标准型）、zpk（零极点增益，仅 SISO）"},
 "control.lti.poles_zeros":{"system":SYS_D,"dcgain":"是否计算直流增益（连续 s=0，离散 z=1），默认 true"},
 "control.analysis.ctrb_obsv":{"system":SYS_D+"。判断可控性只需 A、B：用户没给 C、D 时用 kind=\"ss\"，C = {dtype:\"f64\", shape:[1,n], data:[0,…,0]}、D = {dtype:\"f64\", shape:[1,m], data:[0,…,0]}（不影响可控性结果，回答时只报告可控性）；判断可观性只需 A、C：没给 B、D 时 B 取 shape [n,1] 全零","gramians":"是否额外计算可控/可观 Gramian（要求系统稳定），默认 false"},
 "control.analysis.margin":{"system":"开环 SISO 系统 L(s) = 控制器·对象（·传感器），按单位负反馈解释；tf/ss/zpk，ts>0 为离散。不要传闭环 T（control_lti_feedback 的结果）；有对象和控制器时可直接用 control_analysis_loopsens，或把 control_lti_series(controller, plant) 返回的 result.system 原样放在这里"},
 "control.lti.c2d":{"system":"待离散化的连续系统（ts 必须为 0）","ts":"采样周期 T（单位 s，> 0）。必须是用户在对话中明确给出的数值；用户没给时不要调用本工具，先询问用户，绝不假设 0.1、0.01 等常用值","method":"离散化方法：zoh（零阶保持，默认）、foh（一阶保持/三角保持）、tustin（双线性，可配合 prewarp_frequency）","output_kind":"输出表示：same（与输入相同，默认）、tf、ss、zpk"},
 "control.lti.series":{"controller":"具名角色：控制器 C（例如 control_design_pid 返回的 result.system，原样粘贴）。与 plant 一起给出，结果是开环 L = plant·controller","plant":"具名角色：被控对象 G。与 controller 一起给出。不要和 sys1/sys2 混用","sys1":"位置参数（不推荐）：前向第一个系统（输出接到 sys2）。tf 系数降幂；ss 为 row-major。ts 必须与 sys2 相同","sys2":"位置参数：第二个系统。结果是 sys2·sys1，不是 sys1·sys2","output_kind":"same（默认：SISO tf/zpk 用多项式，否则 ss 块公式）、tf 或 ss","lineage":"可选。上游调用的 receipt.fingerprint.request。只回显，不核验；没有就不要编"},
 "control.lti.parallel":{"sys1":"并联支路 1，与 sys2 输入数、输出数都相同","sys2":"并联支路 2。结果是 sys1+sys2","output_kind":"same（默认）、tf 或 ss","lineage":"可选。上游 receipt.fingerprint.request，只回显，不核验"},
 "control.lti.feedback":{"plant":"具名角色：被控对象 G（必填，用具名写法时）。结果是闭环 T = G·C/(1 + H·G·C)","controller":"具名角色：控制器 C，在前向通路（e → C → G）。省略表示 C = 1。不要把控制器放到 sensor","sensor":"具名角色：反馈通路 H（传感器/测量动态）。省略表示单位反馈 H = 1。控制器不是 sensor","sys1":"位置参数（不推荐）：前向通道。省略 sys2 时必须是方的（单位反馈）","sys2":"位置参数：反馈通道。省略表示单位增益 D=I。负反馈是默认，不是把 sys2 写成负号","sign":"反馈符号，只能是 -1（负反馈，默认，规范定义）或 +1（正反馈）。这是定义默认值，不是测量值，不需要引用用户原话","output_kind":"same（默认）、tf 或 ss","lineage":"可选。上游 receipt.fingerprint.request，只回显，不核验"},
 "control.design.pid":{"form":"parallel（默认）：C=Kp+Ki/s+Kd·s/(Tf·s+1)。standard：C=Kp·(1+1/(Ti·s)+Td·s/(Tf·s+1))。这是定义默认值","Kp":"比例增益。设计参数，没有默认值；由调用方给定，不需要在用户原话里逐字出现，但不得在用户和题目都没给时擅自假设","Ki":"只用于 form=parallel：积分增益，默认 0（该项不出现）。standard 形式不要出现 Ki","Kd":"只用于 form=parallel：微分增益，默认 0。Tf=0 且 Kd≠0 时结果 improper。standard 形式不要出现 Kd","Ti":"只用于 form=standard：积分时间（秒），必须 > 0。没有积分就省略 Ti，不要写 Ti=0；parallel 形式不要出现 Ti","Td":"只用于 form=standard：微分时间（秒），默认 0。parallel 形式不要出现 Td","Tf":"微分滤波时间常数（秒），默认 0（不加滤波）。有微分项时 Tf=0 得到理想微分","output_kind":"tf（默认）或 ss。improper 时 ss 会报错","lineage":"可选。上游 receipt 指纹，只回显"},
 "control.lti.stepinfo":{"system":SYS_D,"time":"时间网格，与 step 相同：uniform（t_final、dt，单位 s）或 auto。指标相对这个网格定义","rise_time_limits":"上升时间的低/高阈值，默认 [0.1, 0.9]（10% 到 90%）。这是指标定义，不是对象的物理参数；省略即用默认，不必询问","settling_threshold":"调节带半宽，默认 0.02（2%）。定义默认值，省略即用默认"},
 "ode.ivp.solve":{"states":"状态名列表，如 [\"x\", \"v\"]。顺序决定 y 的行序；表达式里按名字引用，不用下标","rhs":"右端表达式列表，与 states 一一对应：第 i 个是 d(states[i])/dt，如 [\"v\", \"-k*x - c*v\"]。可用 t、状态名、参数名、常数 pi、+ − * / ^（右结合，-x^2 = -(x^2)）与函数 sin cos tan asin acos atan atan2 sinh cosh tanh exp log log10 sqrt abs min max hypot sign","params":"参数名到数值，如 {\"k\": 4, \"c\": 0.5}；名字不能与状态名重复","initial":"初值，键为状态名，如 {\"x\": 1, \"v\": 0}；每个状态都要给","t_span":"[t0, tf]；tf < t0 时反向积分","t_eval":"输出时刻（在区间内、沿积分方向单调）。不给时用 n_out 个等距点","n_out":"不给 t_eval 时的输出点数，默认 101（含两端）。与 t_eval 二选一","rtol":"相对容差，默认 1e-3（与 MATLAB ode45、scipy 相同）。要更准就调小，如 1e-8","atol":"绝对容差，默认 1e-6","max_step":"最大步长（可选）","first_step":"初始步长（可选；不给时自动选择）"},
 "linalg.system.solve":{"A":"系数矩阵 [m, n]，row-major（MATLAB 是 column-major，先转置再展平）；m ≥ n。大矩阵可用 b64 或 ref 编码","b":"右端项：[m]（一个）或 [m, k]（k 个，每列一个）","refinement":"auto（缺省：用补偿残差做至多 3 步迭代精化）或 none（只做一次 LU 求解，与 MATLAB 的 A\\b 相同）。只影响方阵"},
 "calculus.quadrature.integral":{"expression":"被积函数 f(x)，3.15 表达式","variable":"积分变量名（缺省 x）","params":"参数名 → 数值","limits":"[a, b]，可以是 \"Inf\" / \"-Inf\"；a > b 时结果取负","rel_tol":"相对容差（缺省 1e-10；MATLAB 是 1e-6）","abs_tol":"绝对容差（缺省 1e-12）"},
 "calculus.quadrature.trapz":{"y":"数据 [n]，或 [k, n]（每行一个通道）","x":"每个数据点的位置 [n]（与 dx 二选一）","dx":"等间距步长（与 x 二选一；带单位意义，不要猜 1）","cumulative":"true 时同时返回部分和（= cumtrapz）"},
 "optim.linear.linprog":{"c":"目标系数 [n]（求最小；求最大时取负）","A":"不等式约束矩阵，每行一个约束 A·x ≤ b","b":"不等式右端","Aeq":"等式约束矩阵","beq":"等式右端","lb":"下界 [n]，无界写 \"-Inf\"；缺省全部无下界（同 MATLAB，scipy 缺省是 0）","ub":"上界 [n]，无界写 \"Inf\""},
 "optim.local.fmincon":{"unknowns":"未知量名","objective":"目标 f(x)，3.15 表达式，求最小","params":"参数名 → 数值","x0":"初值（由用户给出；不同初值可能得到不同的局部最优）","nonlinear_ineq":"非线性不等式约束 g_i(x) ≤ 0 的左端表达式","nonlinear_eq":"非线性等式约束 h_j(x) = 0 的左端表达式","A":"线性不等式 A·x ≤ b","b":"线性不等式右端","Aeq":"线性等式 Aeq·x = beq","beq":"线性等式右端","lb":"下界，无界写 \"-Inf\"","ub":"上界，无界写 \"Inf\"","optimality_tolerance":"相对 KKT 驻定与互补残差的容差（缺省 1e-8）","constraint_tolerance":"约束违反的容差（缺省 1e-8）"},
 "optim.local.fminsearch":{"unknowns":"未知量名","objective":"目标 f(x)，3.15 表达式，可以不光滑","params":"参数名 → 数值","x0":"初值","tol_x":"单纯形直径容差（缺省 1e-4，同 MATLAB）","tol_fun":"函数值极差容差（缺省 1e-4，同 MATLAB）","max_iterations":"迭代上限（缺省 200·n，同 MATLAB）"},
 "nonlinear.root.fzero":{"expression":"f(x)，3.15 表达式语言；求 f = 0","variable":"自变量名（缺省 x）","params":"参数名 → 数值","bracket":"变号区间 [a, b]，a < b，f(a)·f(b) ≤ 0（推荐）","x0":"初值（没有区间时用；算子向外搜索变号区间，找到的根不一定是想要的那个）","xtol":"附加的绝对容差（缺省 0：算到区间半宽 ≤ 2ε|x|）"},
 "nonlinear.system.fsolve":{"unknowns":"未知量名列表","equations":"方程 F_i = 0 的左端表达式，个数等于未知量个数","params":"参数名 → 数值","x0":"初值，按 unknowns 的顺序（由用户给出；不同初值可能收敛到不同的解）"},
 "signal.design.butter":{"order":"阶数 n（由用户决定，不要代填）","type":"low、high、bandpass、bandstop；省略时一个截止频率为 low、两个为 bandpass","domain":"digital（缺省）或 analog","cutoff_hz":"截止频率（Hz）：一个数，或带通/带阻的 [下限, 上限]；数字滤波器必须同时给 sample_rate_hz","cutoff_normalized":"数字滤波器的归一化截止频率（= MATLAB 的 Wn）：0 < Wn < 1，1 = Nyquist（采样率的一半），不是 rad/sample，也不是相对采样率","cutoff_rad_s":"模拟滤波器的截止频率（rad/s）","sample_rate_hz":"采样率（Hz），与 cutoff_hz 一起用"},
 "signal.filter.filter":{"x":"输入信号 [n]，或 [k, n]（每行一个通道）","b":"分子系数，z⁻¹ 升幂（同 MATLAB filter）","a":"分母系数，z⁻¹ 升幂，a[0] ≠ 0","sos":"二阶节 [L, 6]，每行 [b0 b1 b2 a0 a1 a2]（高阶滤波器优先用它）","zi":"初始状态（可选）：b/a 为 max(len(a), len(b)) − 1 个数；sos 为 [L, 2]；多通道可给 [k, …]"},
 "fit.poly.polyfit":{"x":"自变量 [n]","y":"观测值 [n]，与 x 一一对应","degree":"多项式次数 d（模型选择，由用户决定；不要代填）","scaling":"none（缺省：对 x 本身拟合）或 center_scale（对 (x − mean)/std 拟合，mu 随结果返回；x 远离 0 或跨度大时可显著改善条件数，但系数含义改变，须用户确认）"},
 "linalg.decomp.eig":{"A":"实方阵 [n, n]，row-major（MATLAB 是 column-major，先转置再展平；对称矩阵不受影响）","structure":"auto（缺省：A 与 Aᵀ 逐位相等时用对称算法）或 general（总用一般算法，结果为 c128）","vectors":"是否返回特征向量（缺省 true）"},
 "linalg.decomp.svd":{"A":"实矩阵 [m, n]，row-major","mode":"values（只要奇异值）、thin（缺省，= MATLAB svd(A, \"econ\")）或 full（方阵 U、V）"},
 "interp.grid.interp1":{"x":"采样点 [n]，严格递增","y":"采样值：[n]，或 [k, n]（k 个通道，每行一个；MATLAB 习惯按列存，要转置）","xq":"查询点 [q]，任意顺序","method":"linear（缺省）、nearest（中点归右）、previous、next、pchip（保形三次，不过冲）、spline（not-a-knot 三次样条，可能过冲）","extrapolation":"查询点超出 [x 的最小值, 最大值] 时：error（缺省，报错并给出可选补丁）、nan（标成缺失）、clamp（取端点值）、extrapolate（延长端点那一段）。不确定时问用户"},
 "signal.spectral.fft":{"x":"要变换的序列：f64 或 c128，形状 [n] 或 [m, n]（m 个通道，沿最后一维变换，与 lsim 的 [nu, nt] 相同）。长数据可用 b64 或 ref 编码","n":"变换长度。缺省为 x 最后一维的长度；更短则截断，更长则在末尾补零（= MATLAB fft(x, n)）","direction":"forward（缺省）或 inverse","normalization":"backward（缺省：正变换不缩放、逆变换乘 1/n，与 MATLAB、numpy 相同）、ortho（两个方向都乘 1/√n）或 forward（正变换乘 1/n）","sides":"two（缺省，全部 n 个频点）或 one（只返回 0..fs/2，仅用于实数输入的正变换）","sample_rate_hz":"采样率（Hz）。给出时 frequency 以 Hz 为单位，否则以 cycles/sample 为单位"},
 "control.design.pidtune":{"plant":"被控对象 G（SISO、连续、proper）。上一个工具返回了系统就原样粘贴 result.system","type":"PI 或 PID。必填：控制器类型是设计决定，不由算子选","wc_rad_s":"目标增益穿越频率 ωc（rad/s）。必填，由用户或题目给出，算子不猜；不知道就问用户期望的响应速度（ωc 约为闭环带宽）","phase_margin_deg":"目标相位裕度（度），默认 60。定义默认值，省略即可","ti_td_ratio":"只用于 PID：Ti/Td 比，默认 4（Åström–Hägglund）。PI 不要给","derivative_filter_n":"只用于 PID：微分滤波 Tf = Td/N 的 N，默认 10。PI 不要给","lineage":"可选。上游 receipt.fingerprint.request，只回显"},
 "control.lti.lsim":{"system":SYS_D,"u":"输入采样，形状 [nu, nt]：第 i 行是第 i 个输入通道，第 k 列是 t_k = t0 + k·dt 时刻的值。SISO 可给一维 [nt]。长数据可用 b64 或 ref 编码（3.6）","time":"均匀网格：dt（s，> 0），t0（起始时刻，缺省 0），unit 固定 s。离散系统 dt 必须等于 ts。非均匀时间戳先重采样","interpolation":"采样之间的输入怎么解释：foh 线性（缺省，与 scipy lsim、python-control 相同），zoh 保持到下一个采样。离散系统不要给","x0":"初始状态 [n]，缺省 0。只用于 kind = ss：tf/zpk 没有状态坐标，要给初始状态先用 control_lti_convert 转成 ss","return_states":"true 时同时返回状态轨迹 x [n, nt]，缺省 false","output":"返回的采样点：all / decimated（最多 max_samples 点）/ none"},
 "control.analysis.loopsens":{"plant":"被控对象 G（SISO）。上一个工具返回了系统（例如 control_lti_pade 与对象串联的结果、control_lti_convert 的结果）就原样粘贴 result.system","controller":"控制器 C，在前向通路。省略 = 1（只看对象本身的单位负反馈）。PID 用 control_design_pid 的 result.system 原样粘贴","sensor":"反馈通路 H（测量动态）。省略 = 单位反馈。不要把控制器放在这里","time":"可选。给出时返回闭环 T 的阶跃指标（step_metrics）：uniform（t_final、dt，单位 s）或 auto。不给则 step_metrics 为 null","rise_time_limits":"阶跃上升时间阈值，默认 [0.1, 0.9]。定义默认值，省略即可","settling_threshold":"调节带半宽，默认 0.02（2%）。定义默认值，省略即可","bandwidth_drop_db":"闭环带宽的下降量（dB，负数），默认 -3。定义默认值，省略即可","lineage":"可选。上游 receipt.fingerprint.request，只回显"},
 "control.lti.pade":{"delay":"纯时滞 θ（秒），必须 > 0。没有默认值。用户没给、题目也算不出时先询问，不得假设 0.1 或 1。这不是采样周期那种必须逐字引用的测量值：由 t1、t2 算出的 θ 可以填在这里","order":"对角 Padé 阶次 N（1 到 20）。没有默认值，不得假设 N=1。题目没指定时先询问；设计评测的提示若写明了用哪个 N，按提示填写","lineage":"可选。上游 receipt 指纹，只回显"},
}
# ---- v0.4.11: fit.poly.polyfit, linalg.decomp.eig, linalg.decomp.svd
V411="0.4.0"
pfit=man("fit.poly.polyfit","Polynomial least-squares fit","最小二乘多项式拟合：求 d 次多项式 p（系数从最高次到常数项，与 MATLAB polyfit、numpy.polyfit 相同）使 ‖V·p − y‖₂ 最小。Householder QR 求解；结果带每个系数的标准误差、残差范数、自由度、条件数估计与后向误差。病态时照原样拟合并警告，建议（须用户确认）改用中心化缩放 scaling = center_scale（= MATLAB 的 [p, S, mu]）。","analysis",["fit","polynomial","polyfit","regression","least-squares"],
 ["x、y 为等长的 [n] 实数向量","degree（次数 d）由调用方给出","x 至少有 d + 1 个不同的值"],
 ["加权拟合与约束拟合（0.4.0 未支持）","非多项式模型（lsqcurvefit）","外推：拟合多项式在数据范围外通常没有意义"],
 "Vandermonde 矩阵 V[i][j] = t_i^(d−j)（逐列连乘，同 MATLAB polyfit），t = x（scaling = none，缺省）或 t = (x − mean)/std（scaling = center_scale，std 为样本标准差，mean、std 用补偿求和）。Householder QR（与 linalg.system.solve 相同），p = R⁻¹·(Qᵀy)，再用修正半正规方程做至多 3 步精化（δ = R⁻¹R⁻ᵀ·(Vᵀr)，Vᵀr 用补偿算术）。秩判定与条件数都在列缩放后的 R·D⁻¹ 上（D 为 V 的列 2-范数，与 x 的单位无关）：|r_jj| ≤ max(m, n)·u·max|r_ii| 报 E.NUMERICAL.SINGULAR；κ₁ 用 Hager–Higham 估计，> 1e10 时 W.NUMERICAL.ILL_CONDITIONED。标准误差 sqrt(diag((RᵀR)⁻¹))·‖r‖₂/sqrt(n − d − 1)。后向误差 ‖Vsᵀr‖₂ / (‖Vs‖_F·(‖r‖₂ + ‖Vs‖_F‖D·p‖₂))（Vs = V·D⁻¹），残差用补偿算术",
 [{"type":"textbook","cite":"N. J. Higham, Accuracy and Stability of Numerical Algorithms, 2nd ed., SIAM 2002, §19–20（QR 与最小二乘）、§22（Vandermonde 系统）"},
  {"type":"textbook","cite":"G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., JHU Press 2013, §5.3（最小二乘的敏感性）"}],
 "polyfit::fit (Vandermonde + Householder QR)","T1",{"max_degree":30,"max_points":1000000},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.RANGE","E.VALIDATION.OPTION_CONFLICT","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN","E.DOMAIN.UNDERDETERMINED","E.NUMERICAL.SINGULAR","E.NUMERICAL.NON_FINITE_EVALUATION","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.NUMERICAL.ILL_CONDITIONED"],sparse={},ver=V411,backend=BACK_NUM("polyfit::fit"))
pfit_in=ins("fit.poly.polyfit",{"x":M,"y":M,"degree":{"type":"integer","minimum":0,"description":"多项式次数 d","x-aeoi-provenance":"supplied"},
 "scaling":{"enum":["none","center_scale"],"default":"none","x-aeoi-provenance":"convention"}},["x","y","degree"],
 {"x-aeoi-shape-constraints":["x: [n]","y: [n]"]},ver=V411)
pfit_out=outs("fit.poly.polyfit",{"p":dict(M,description="系数 [d + 1]，最高次在前；scaling = center_scale 时是对 t = (x − mu[0])/mu[1] 的多项式"),
 "degree":{"type":"integer"},"scaling":{"enum":["none","center_scale"]},
 "mu":{"description":"scaling = center_scale 时为 [mean(x), std(x)]（样本标准差），否则 null"},
 "residual_norm":{"type":"number","description":"‖y − V·p‖₂"},"dof":{"type":"integer","description":"自由度 n − d − 1"},
 "std_errors":{"description":"每个系数的标准误差 [d + 1]（假设误差独立同分布）；dof = 0 时为 null"}},
 ["p","degree","scaling","mu","residual_norm","dof","std_errors"],ver=V411)
eigm=man("linalg.decomp.eig","Eigenvalues and eigenvectors","实方阵的特征值与特征向量（= MATLAB eig(A) / [V, D] = eig(A)）。A 精确对称时用对称算法：实特征值升序、正交特征向量；否则用一般算法：复特征值按实部升序、共轭对相邻（虚部为正的在前）。特征向量 2-范数为 1，最大模分量为正实数。结果带每个特征对的残差、特征值条件数与误差估计；可能亏损（重特征值）时警告。","analysis",["linalg","eigenvalues","eig","spectral"],
 ["A 为 [n, n] 实数方阵"],["广义特征值问题 eig(A, B)（0.4.0 未支持）","复数矩阵","大型稀疏矩阵的部分特征值（eigs）"],
 "structure = auto（缺省）：A 与 Aᵀ 逐位相等时用对称算法（循环双边 Jacobi：|a_pq| > ε·sqrt(|a_pp·a_qq|) 时旋转，至多 60 轮扫描），否则用一般算法：实 Schur 分解（aeoi-core kernels::schur：Householder 化为 Hessenberg 形，Francis 双位移 QR，实特征值的 2×2 块按 LAPACK dlanv2 标准化；不收敛时做确定性相似变换后重试）→ 复 Schur 形（rsf2csf，Givens 消去 2×2 块）→ 上三角回代求特征向量（小除数按 LAPACK ztrevc 抬到 max(u·|λ|, 最小正规数)），共轭对的第二个向量取第一个的共轭；左特征向量同法求得，特征值条件数 1/|yᴴx|。排序：对称升序；一般按 (实部, |虚部|) 升序，共轭对虚部为正的在前。规一化：‖v‖₂ = 1，最大模分量（并列取下标最小）为正实数。",
 [{"type":"textbook","cite":"G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., JHU Press 2013, §7.5（QR 算法）、§7.6（特征向量）、§8.3（对称 QR）"},
  {"type":"textbook","cite":"J. W. Demmel, Applied Numerical Linear Algebra, SIAM 1997, §4.3、§5.2（扰动理论）"},
  {"type":"textbook","cite":"E. Anderson et al., LAPACK Users' Guide, 3rd ed., SIAM 1999, §4.8（特征问题的误差界）"}],
 "eig::eig (symmetric Jacobi / real Schur + complex triangular back-substitution)","T2",{"max_dimension":500},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN","E.NUMERICAL.NO_CONVERGENCE","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.NUMERICAL.ILL_CONDITIONED","W.PERFORMANCE.SPARSE_DENSIFIED"],sparse={"/A":{"policy":"densify","max_dense_elements":250000}},ver=V411,backend=BACK_NUM("eig::eig"))
eig_in=ins("linalg.decomp.eig",{"A":M,"structure":{"enum":["auto","general"],"default":"auto","x-aeoi-provenance":"convention"},
 "vectors":{"type":"boolean","default":True,"x-aeoi-provenance":"convention"}},["A"],{"x-aeoi-shape-constraints":["A: [n, n]"]},ver=V411)
eig_out=outs("linalg.decomp.eig",{"values":dict(M,description="特征值 [n]：对称算法为 f64（升序），一般算法为 c128（实部升序，共轭对相邻、虚部为正的在前）"),
 "vectors":{"description":"特征向量 [n, n]，第 k 列对应 values[k]（f64 或 c128，与 values 一致）；vectors = false 时为 null"},
 "structure":{"enum":["symmetric","general"],"description":"实际所用的算法"},
 "ordering":{"type":"string","description":"排序规则（回显）"}},["values","vectors","structure","ordering"],ver=V411)
svdm=man("linalg.decomp.svd","Singular value decomposition","实矩阵的奇异值分解 A = U·diag(s)·Vᵀ（= MATLAB svd）。mode = values 只返回奇异值，thin（缺省，= svd(A, \"econ\")）返回 [m, p] 与 [n, p] 的 U、V（p = min(m, n)），full 返回方阵 U、V。奇异值降序；每对奇异向量的符号规定为 u_k 的最大模分量为正。结果带数值秩（容差 max(m, n)·eps(s_max)，eps(x) 为 x 处的浮点间距，同 MATLAB rank）、2-范数条件数、重构残差与正交性。","analysis",["linalg","svd","singular-values","rank"],
 ["A 为 [m, n] 实数矩阵"],["复数矩阵","大型稀疏矩阵的部分奇异值（svds）"],
 "单边 Jacobi（Hestenes）：对 A（m < n 时对 Aᵀ）的列做循环旋转，|g_pᵀg_q| > sqrt(m)·ε·‖g_p‖‖g_q‖ 时旋转，至多 60 轮扫描；σ_k = ‖g_k‖₂，u_k = g_k/σ_k，零奇异值的 u_k 由正交补全给出；按奇异值降序重排；full 模式用 U_thin 的 Householder QR 补全正交基（前 p 列保持 U_thin 不变）。符号：u_k 的最大模分量（并列取下标最小）为正，v_k 随之变号。数值秩：s_k > max(m, n)·eps(s_max) 的个数。检查：‖A − U·diag(s)·Vᵀ‖_F/‖A‖_F 与 ‖UᵀU − I‖、‖VᵀV − I‖ 的最大元。",
 [{"type":"textbook","cite":"G. H. Golub, C. F. Van Loan, Matrix Computations, 4th ed., JHU Press 2013, §8.6.4（单边 Jacobi）"},
  {"type":"paper","cite":"J. Demmel, K. Veselić, Jacobi's method is more accurate than QR, SIAM J. Matrix Anal. Appl. 13(4), 1992","doi":"10.1137/0613074"},
  {"type":"textbook","cite":"J. W. Demmel, Applied Numerical Linear Algebra, SIAM 1997, §5.2（奇异值的扰动）"}],
 "svd::svd (one-sided Jacobi)","T1",{"max_dimension":500},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN","E.NUMERICAL.NO_CONVERGENCE","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.PERFORMANCE.SPARSE_DENSIFIED"],sparse={"/A":{"policy":"densify","max_dense_elements":250000}},ver=V411,backend=BACK_NUM("svd::svd"))
svd_in=ins("linalg.decomp.svd",{"A":M,"mode":{"enum":["values","thin","full"],"default":"thin","x-aeoi-provenance":"convention"}},["A"],{"x-aeoi-shape-constraints":["A: [m, n]"]},ver=V411)
svd_out=outs("linalg.decomp.svd",{"s":dict(M,description="奇异值 [p]，降序，p = min(m, n)"),
 "U":{"description":"左奇异向量：thin [m, p]，full [m, m]；mode = values 时为 null"},
 "V":{"description":"右奇异向量（不是 Vᵀ，同 MATLAB）：thin [n, p]，full [n, n]；mode = values 时为 null"},
 "mode":{"enum":["values","thin","full"]},"rank":{"type":"integer","description":"数值秩：s_k > rank_tol 的个数"},
 "rank_tol":{"type":"number","description":"max(m, n)·eps(s_max)（eps(x) 为 x 处的浮点间距；同 MATLAB rank）"},
 "cond2":{"description":"s_max / s_min（s_min = 0 时为 \"Inf\"）"}},["s","U","V","mode","rank","rank_tol","cond2"],ver=V411)
B3.update({"fit.poly.polyfit":(pfit,pfit_in,pfit_out),"linalg.decomp.eig":(eigm,eig_in,eig_out),"linalg.decomp.svd":(svdm,svd_in,svd_out)})
# ---- v0.4.12: signal.design.butter, signal.filter.filter
V412="0.4.0"
CUT={"oneOf":[{"type":"number","exclusiveMinimum":0},{"type":"array","items":{"type":"number","exclusiveMinimum":0},"minItems":2,"maxItems":2}],"x-aeoi-provenance":"user"}
bw_=man("signal.design.butter","Butterworth filter design","设计巴特沃斯滤波器（= MATLAB butter）：低通、高通、带通、带阻；数字（缺省，双线性变换并预畸变）或模拟。结果同时给出零极点 z/p/k、二阶节 sos（数字滤波器的推荐形式）和多项式 b/a（高阶时可能失去精度，会警告）。截止频率必须显式给出：数字滤波器用 cutoff_hz + sample_rate_hz，或 cutoff_normalized（= MATLAB 的 Wn，1 = Nyquist）；模拟滤波器用 cutoff_rad_s 或 cutoff_hz。","design",["signal","filter","butterworth","iir","butter"],
 ["order（阶数）与截止频率由调用方给出","数字：0 < 截止频率 < Nyquist","带通/带阻：[下限, 上限]，下限 < 上限"],
 ["其他原型（切比雪夫、椭圆、贝塞尔：以后的算子）","FIR 设计","按指标自动选阶（buttord）"],
 "模拟原型 p_k = −exp(iπm/(2n))（m = −n+1, −n+3, …, n−1，同 scipy buttap），频率变换 lp2lp / lp2hp / lp2bp / lp2bs（零极点形式，scipy 约定）；数字：预畸变 w = 4·tan(π·Wn/2) 后做双线性变换（fs = 2），与 scipy.signal.butter、MATLAB butter 相同。b/a 由零极点展开（与 control.lti.convert 同一代码）；sos：极点组按离单位圆由远到近排列，各取最近的零点，增益放在第一节",
 [{"type":"textbook","cite":"A. V. Oppenheim, R. W. Schafer, Discrete-Time Signal Processing, 3rd ed., Pearson 2010, §7.2（双线性变换与巴特沃斯设计）"},
  {"type":"textbook","cite":"T. W. Parks, C. S. Burrus, Digital Filter Design, Wiley 1987, Ch. 7"}],
 "filter::butter (prototype + zpk transforms + bilinear)","T1",{"max_order":30},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.RANGE","E.VALIDATION.OPTION_CONFLICT","E.DOMAIN.ABOVE_NYQUIST","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.NUMERICAL.ILL_CONDITIONED"],sparse={},ver=V412,backend=BACK_NUM("filter::butter"))
bw_in=ins("signal.design.butter",{"order":{"type":"integer","minimum":1,"x-aeoi-provenance":"supplied"},
 "type":{"enum":["low","high","bandpass","bandstop"],"x-aeoi-provenance":"supplied"},
 "domain":{"enum":["digital","analog"],"default":"digital","x-aeoi-provenance":"convention"},
 "cutoff_hz":CUT,"cutoff_normalized":CUT,"cutoff_rad_s":CUT,
 "sample_rate_hz":{"type":"number","exclusiveMinimum":0,"x-aeoi-provenance":"user"}},["order"],ver=V412)
bw_out=outs("signal.design.butter",{"z":{"type":"array","description":"零点 [[re, im], …]"},"p":{"type":"array","description":"极点 [[re, im], …]"},"k":{"type":"number"},
 "sos":{"description":"二阶节 [L, 6]，每行 [b0 b1 b2 a0 a1 a2]（数字滤波器；模拟为 null）"},
 "b":{"type":"array","description":"分子系数（数字：z⁻¹ 升幂；模拟：s 降幂）"},"a":{"type":"array","description":"分母系数，a[0] = 1"},
 "system":{"description":"LtiSystem（zpk）：模拟滤波器 ts = 0；数字滤波器给了采样率时 ts = 1/fs；否则 null"},
 "type":{"enum":["low","high","bandpass","bandstop"]},"order":{"type":"integer"},"domain":{"enum":["digital","analog"]},
 "cutoff":{"type":"object","description":"回显：数字为 normalized（1 = Nyquist）与 hz（给了采样率时），模拟为 rad_s"}},
 ["z","p","k","sos","b","a","system","type","order","domain","cutoff"],ver=V412)
flt=man("signal.filter.filter","Apply a digital filter","用 IIR/FIR 数字滤波器处理信号（= MATLAB filter；sos 形式 = sosfilt）。滤波器给 b/a（z⁻¹ 升幂，a[0] ≠ 0）或二阶节 sos（推荐，数值更稳）；x 每行一个通道；可给初始状态 zi，返回末状态 zf。检查每个样本满足差分方程，极点在单位圆上或外时警告。","analysis",["signal","filter","iir","fir","lfilter","sosfilt"],
 ["x 为 [n] 或 [k, n]（每行一个通道）","b/a 或 sos 恰好给出一种","a[0] ≠ 0（sos 每节 a0 ≠ 0）"],
 ["零相位滤波（filtfilt：以后的算子）","多维滤波（filter2/conv2）"],
 "转置直接 II 型递推（同 MATLAB filter、scipy lfilter）：b、a 补齐到同长并除以 a0；sos 为各节依次的转置直接 II 型。后置条件：每个样本的差分方程残差 |Σa_i·y[n−i] − Σb_i·x[n−i]| ≤ 10·m·u·(Σ|a_i·y| + Σ|b_i·x|)，补偿算术，sos 逐节。稳定性：分母多项式的根（与 control.lti.poles_zeros 同一求根代码）",
 [{"type":"textbook","cite":"A. V. Oppenheim, R. W. Schafer, Discrete-Time Signal Processing, 3rd ed., Pearson 2010, §6.3–6.5（直接 II 型与转置结构）"}],
 "filter::lfilter / sosfilt (transposed direct form II)","T1",{"max_samples":10000000},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN","E.VALIDATION.OPTION_CONFLICT","E.VALIDATION.ZERO_DENOMINATOR","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.DOMAIN.UNSTABLE_SYSTEM"],sparse={},ver=V412,backend=BACK_NUM("filter::lfilter"))
flt_in=ins("signal.filter.filter",{"x":M,"b":{"type":"array","items":{"type":"number"},"minItems":1},"a":{"type":"array","items":{"type":"number"},"minItems":1},"sos":M,"zi":M},["x"],
 {"x-aeoi-shape-constraints":["x: [n] 或 [k, n]","sos: [L, 6]","zi: b/a 为 [max(len) − 1]（或每通道 [k, …]）；sos 为 [L, 2]（或 [k, L, 2]）"]},ver=V412)
flt_out=outs("signal.filter.filter",{"y":dict(M,description="输出，形状同 x"),"zf":dict(M,description="末状态（形状同 zi 的约定），可作为下一段的 zi"),"form":{"enum":["ba","sos"]}},["y","zf","form"],ver=V412)
B3.update({"signal.design.butter":(bw_,bw_in,bw_out),"signal.filter.filter":(flt,flt_in,flt_out)})
# ---- v0.4.13: nonlinear.root.fzero, nonlinear.system.fsolve
V413="0.4.0"
PARAMS={"type":"object","propertyNames":{"pattern":"^[A-Za-z_][A-Za-z0-9_]*$"},"additionalProperties":{"type":"number"},"x-aeoi-provenance":"supplied"}
fz=man("nonlinear.root.fzero","Scalar root finding","求单变量方程 f(x) = 0 的根（= MATLAB fzero）：f 用 3.15 的表达式语言写。优先给变号区间 bracket = [a, b]（根被夹在区间内）；只给初值 x0 时按 MATLAB 的方式向外搜索变号区间，结果回显搜到的区间并警告它不一定是想要的根。Brent 方法（zeroin），结果带最终区间作为误差界，并检查变号点不是极点。","analysis",["root","fzero","nonlinear","equation","brent"],
 ["f 在区间上连续","bracket 两端 f 异号，或给出 x0"],["多项式全部根（roots）","方程组（nonlinear.system.fsolve）","不变号的偶重根"],
 "Brent 方法（Brent 1973 zeroin，MATLAB fzero 的算法）：逆二次插值 / 割线 / 二分，区间半宽 ≤ 2ε|x| + xtol/2 停止。只给 x0 时：dx = |x0|/50（x0 = 0 时 1/50），交替试 x0 − dx、x0 + dx，dx ×√2，直到变号（同 MATLAB）。后置条件：最终区间两端变号；|f(x)| ≤ max(|f(a₀)|, |f(b₀)|)（变号点不是极点）",
 [{"type":"textbook","cite":"R. P. Brent, Algorithms for Minimization without Derivatives, Prentice-Hall 1973, Ch. 4"},
  {"type":"textbook","cite":"G. E. Forsythe, M. A. Malcolm, C. B. Moler, Computer Methods for Mathematical Computations, Prentice-Hall 1977, §7.2（zeroin）"}],
 "roots::fzero (Brent zeroin)","T1",{"max_evaluations":5000,"max_expression_length":4096},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.RANGE","E.VALIDATION.OPTION_CONFLICT","E.VALIDATION.EXPRESSION","E.DOMAIN.NO_SIGN_CHANGE","E.NUMERICAL.NON_FINITE_EVALUATION","E.NUMERICAL.NO_CONVERGENCE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.DOMAIN.BRACKET_SEARCHED"],sparse={},ver=V413,backend=BACK_NUM("roots::fzero"))
fz_in=ins("nonlinear.root.fzero",{"expression":EXPR,"variable":dict(NAME,default="x",**{"x-aeoi-provenance":"convention"}),"params":PARAMS,
 "bracket":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2,"x-aeoi-provenance":"supplied"},
 "x0":{"type":"number","x-aeoi-provenance":"supplied"},"xtol":{"type":"number","minimum":0,"default":0,"x-aeoi-provenance":"convention"}},["expression"],ver=V413)
fz_out=outs("nonlinear.root.fzero",{"x":{"type":["number","string"]},"fx":{"type":["number","string"]},
 "bracket":{"type":"object","description":"initial（给定或搜到的区间）、source（given / searched_from_x0）、final（夹住根的最终区间）"},
 "iterations":{"type":"integer"},"evaluations":{"type":"integer"}},["x","fx","bracket","iterations","evaluations"],ver=V413)
fs=man("nonlinear.system.fsolve","Solve a system of nonlinear equations","解 n 个方程 n 个未知量的非线性方程组 F(x) = 0（= MATLAB fsolve）：方程用 3.15 的表达式语言写，雅可比矩阵由表达式自动微分得到（精确，不是有限差分）。Powell dogleg 信赖域，牛顿步用共享的 LU 内核。不收敛时报错并给出目前最好的点；收敛后报告下一步牛顿修正（误差估计）与雅可比条件数，接近奇异（可能是重根）时警告。","analysis",["nonlinear","system","fsolve","newton","equations"],
 ["方程个数 = 未知量个数","x0（初值）由调用方给出","方程在解附近可微"],["超定/欠定（最小二乘：lsqnonlin，以后的算子）","求全部解（只给出 x0 附近的一个）","不可微的方程（abs、min、max 的折点处导数按约定取值）"],
 "Powell dogleg 信赖域（MATLAB fsolve 的缺省算法）：牛顿步 −J⁻¹F（共享 LU 内核），Cauchy 步 −(‖g‖²/‖Jg‖²)·g，g = JᵀF；ρ < 1/4 收缩、ρ > 3/4 且步长到边界时扩大。雅可比由表达式前向自动微分（3.15 EX-8）。收敛：完整牛顿步 ‖p‖∞ ≤ 4u‖x‖∞ 或 F = 0；信赖域缩到不能再下降而 F ≠ 0 时报 E.NUMERICAL.LOCAL_MINIMUM；迭代上限报 E.NUMERICAL.NO_CONVERGENCE。之后再算一步牛顿修正作误差估计，κ₁(J) 用 Hager–Higham 估计",
 [{"type":"textbook","cite":"J. Nocedal, S. J. Wright, Numerical Optimization, 2nd ed., Springer 2006, §4.1（dogleg）、§11（非线性方程组）"},
  {"type":"textbook","cite":"A. Griewank, A. Walther, Evaluating Derivatives, 2nd ed., SIAM 2008, Ch. 3（前向自动微分）"}],
 "roots::fsolve (dogleg + AD Jacobian + LU kernel)","T1",{"max_unknowns":200,"max_iterations":400,"max_expression_length":4096},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.EXPRESSION","E.NUMERICAL.NON_FINITE_EVALUATION","E.NUMERICAL.NO_CONVERGENCE","E.NUMERICAL.LOCAL_MINIMUM","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 ["W.NUMERICAL.ILL_CONDITIONED"],sparse={},ver=V413,backend=BACK_NUM("roots::fsolve"))
fs_in=ins("nonlinear.system.fsolve",{"unknowns":{"type":"array","items":NAME,"minItems":1,"maxItems":200,"uniqueItems":True},
 "equations":{"type":"array","items":EXPR,"minItems":1,"maxItems":200},"params":PARAMS,
 "x0":{"type":"array","items":{"type":"number"},"minItems":1,"maxItems":200,"x-aeoi-provenance":"supplied"}},["unknowns","equations","x0"],ver=V413)
fs_out=outs("nonlinear.system.fsolve",{"x":{"type":"array","description":"解，按 unknowns 的顺序"},"solution":{"type":"object","description":"未知量名 → 值"},
 "residual":{"type":"array","description":"F(x)"},"iterations":{"type":"integer"},"evaluations":{"type":"integer"},"jacobian":{"const":"automatic_differentiation"}},
 ["x","solution","residual","iterations","evaluations","jacobian"],ver=V413)
B3.update({"nonlinear.root.fzero":(fz,fz_in,fz_out),"nonlinear.system.fsolve":(fs,fs_in,fs_out)})
# ---- v0.4.14: optim.linear.linprog, optim.local.fmincon, optim.local.fminsearch
V414="0.4.0"
BND={"type":"array","items":{"oneOf":[{"type":"number"},{"enum":["Inf","-Inf"]}]},"x-aeoi-provenance":"supplied"}
ROWS={"type":"array","items":{"type":"array","items":{"type":"number"}},"x-aeoi-provenance":"supplied"}
VEC={"type":"array","items":{"type":"number"},"x-aeoi-provenance":"supplied"}
LAM_D="乘子（MATLAB 约定）：∇f + Σλ∇g + Σν∇h = 0；ineq*、lower、upper ≥ 0"
lpm=man("optim.linear.linprog","Linear programming","线性规划（= MATLAB linprog）：min cᵀx，s.t. A·x ≤ b、Aeq·x = beq、lb ≤ x ≤ ub。两阶段单纯形（Bland 规则），给出最优顶点与全部乘子（ineqlin、eqlin、lower、upper，MATLAB 的符号约定），并用 KKT 残差验证。注意：与 MATLAB 一样，变量缺省没有界（scipy 缺省 x ≥ 0）；无解报 E.DOMAIN.INFEASIBLE，无界报 E.DOMAIN.UNBOUNDED。","design",["optimization","linear-programming","linprog","simplex"],
 ["系数有限","变量个数 ≤ max_variables（稠密单纯形）"],["整数规划（intlinprog）","大规模稀疏问题（内点法，以后的方法）"],
 "标准形转换（有限下界平移、只有上界的变量反射、自由变量拆成两个非负变量、上下界都有限时加边界行、不等式加松弛），右端非负化；两阶段稠密单纯形表，Bland 规则选入基与出基变量（不循环）；第一阶段剩余 > 1e-9·max(1, ‖rhs‖∞) 即不可行；入基列无正元即无界。最优基上用 LU 内核重算基解与对偶变量；乘子按 c + Aᵀλ_ineq + Aeqᵀλ_eq − λ_lower + λ_upper = 0 映射。后置条件 kkt：原始可行、对偶可行、互补、驻定残差都 ≤ 1e-9（相对数据尺度）",
 [{"type":"textbook","cite":"V. Chvátal, Linear Programming, Freeman 1983, Ch. 2–3、8（两阶段单纯形与 Bland 规则）"}],
 "lp::solve (two-phase simplex, Bland)","T1",{"max_variables":500,"max_iterations":100000},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.NON_FINITE","E.DOMAIN.INFEASIBLE","E.DOMAIN.UNBOUNDED","E.NUMERICAL.NO_CONVERGENCE","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 [],sparse={},ver=V414,backend=BACK_NUM("lp::solve"))
lp_in=ins("optim.linear.linprog",{"c":VEC,"A":ROWS,"b":VEC,"Aeq":ROWS,"beq":VEC,"lb":BND,"ub":BND},["c"],ver=V414)
lp_out=outs("optim.linear.linprog",{"x":{"type":"array"},"fval":{"type":["number","string"]},"lambda":{"type":"object","description":LAM_D},"iterations":{"type":"integer"},
 "bounds_used":{"type":"object","description":"实际使用的 lb、ub（缺省为 ±Inf）"}},["x","fval","lambda","iterations","bounds_used"],ver=V414)
fmc=man("optim.local.fmincon","Constrained nonlinear minimization","带约束的非线性最小化（= MATLAB fmincon）：目标与非线性约束用 3.15 的表达式语言写（g(x) ≤ 0、h(x) = 0），另可给线性约束与上下界。SQP：梯度由自动微分精确给出，子问题用共享的二次规划内核，阻尼 BFGS 近似 Hessian，ℓ1 价值函数线搜索。只保证局部最优；结果用 KKT 残差验证并给出全部乘子；不收敛时报错并附最好的点。","design",["optimization","nonlinear","constrained","fmincon","sqp"],
 ["目标与约束在可行域附近光滑","x0 由调用方给出"],["全局最优（只给出 x0 附近的局部最优）","整数变量","非光滑问题"],
 "SQP：子问题 min ½pᵀBp + ∇fᵀp，∇g·p ≤ −g、∇h·p = −h（线性约束与边界同样处理），用有效集 QP 内核（可行起点由 LP 内核第一阶段给出）；线性化不相容时把违反的非线性约束右端乘 θ = 1/2, 1/4, … 放宽；ℓ1 价值函数 f + μ(Σmax(0, g) + Σ|h|)，μ ≥ 1.1·max|λ|，Armijo 回溯；阻尼 BFGS（Powell）。收敛：相对 KKT 残差 ≤ optimality_tolerance（缺省 1e-8）且约束违反 ≤ constraint_tolerance（缺省 1e-8）。起点先投影到上下界内",
 [{"type":"textbook","cite":"J. Nocedal, S. J. Wright, Numerical Optimization, 2nd ed., Springer 2006, §16.5（有效集 QP）、§18（SQP）"},
  {"type":"paper","cite":"M. J. D. Powell, A fast algorithm for nonlinearly constrained optimization calculations, Numerical Analysis Dundee 1977, LNM 630","doi":"10.1007/BFb0067703"}],
 "optim::fmincon (SQP + QP kernel + AD gradients)","T2",{"max_unknowns":100,"max_iterations":500,"max_expression_length":4096},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.NON_FINITE","E.VALIDATION.EXPRESSION","E.DOMAIN.INFEASIBLE","E.NUMERICAL.NON_FINITE_EVALUATION","E.NUMERICAL.NO_CONVERGENCE","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 [],sparse={},ver=V414,backend=BACK_NUM("optim::fmincon"))
fmc_in=ins("optim.local.fmincon",{"unknowns":{"type":"array","items":NAME,"minItems":1,"maxItems":100,"uniqueItems":True},"objective":EXPR,"params":PARAMS,
 "x0":VEC,"nonlinear_ineq":{"type":"array","items":EXPR},"nonlinear_eq":{"type":"array","items":EXPR},"A":ROWS,"b":VEC,"Aeq":ROWS,"beq":VEC,"lb":BND,"ub":BND,
 "optimality_tolerance":{"type":"number","exclusiveMinimum":0,"default":1e-8,"x-aeoi-provenance":"convention"},"constraint_tolerance":{"type":"number","exclusiveMinimum":0,"default":1e-8,"x-aeoi-provenance":"convention"}},
 ["unknowns","objective","x0"],ver=V414)
fmc_out=outs("optim.local.fmincon",{"x":{"type":"array"},"solution":{"type":"object"},"fval":{"type":["number","string"]},"lambda":{"type":"object","description":LAM_D},
 "iterations":{"type":"integer"},"evaluations":{"type":"integer"},"first_order_optimality":{"type":["number","string"]},"constraint_violation":{"type":["number","string"]}},
 ["x","solution","fval","lambda","iterations","evaluations","first_order_optimality","constraint_violation"],ver=V414)
fms=man("optim.local.fminsearch","Derivative-free minimization (Nelder–Mead)","无约束最小化，不用导数（= MATLAB fminsearch，Nelder–Mead 单纯形，Lagarias 等人的参数）。目标用 3.15 的表达式语言写，可以不光滑。缺省容差同 MATLAB（tol_x = tol_fun = 1e-4），结果只在这个分辨率下是局部极小；收敛后在每个坐标方向 ±h 处探测，检查确实没有更小的值。不收敛时报错并附最好的点。","analysis",["optimization","nelder-mead","fminsearch","derivative-free"],
 ["x0 由调用方给出"],["有约束的问题（fmincon）","高维问题（> 10 个变量时很慢）","需要高精度的光滑问题（fmincon 更准）"],
 "Nelder–Mead（Lagarias, Reeds, Wright, Wright 1998；同 MATLAB）：反射 1、扩展 2、收缩 1/2、压缩 1/2；初始单纯形为 x0 各坐标加 5%（为 0 的坐标取 0.00025）；顶点按 f 排序，相等按原下标。停止：单纯形直径 ≤ tol_x 且函数值极差 ≤ tol_fun。后置条件 probe_local_minimum：f(x) ≤ f(x ± h·e_j) + tol_fun，h = tol_x·max(1, |x_j|)",
 [{"type":"paper","cite":"J. C. Lagarias, J. A. Reeds, M. H. Wright, P. E. Wright, Convergence properties of the Nelder–Mead simplex method in low dimensions, SIAM J. Optim. 9(1), 1998","doi":"10.1137/S1052623496303470"}],
 "optim::fminsearch (Nelder–Mead)","T3",{"max_unknowns":50,"max_expression_length":4096},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.NON_FINITE","E.VALIDATION.EXPRESSION","E.NUMERICAL.NON_FINITE_EVALUATION","E.NUMERICAL.NO_CONVERGENCE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 [],sparse={},ver=V414,backend=BACK_NUM("optim::fminsearch"))
fms_in=ins("optim.local.fminsearch",{"unknowns":{"type":"array","items":NAME,"minItems":1,"maxItems":50,"uniqueItems":True},"objective":EXPR,"params":PARAMS,"x0":VEC,
 "tol_x":{"type":"number","exclusiveMinimum":0,"default":1e-4,"x-aeoi-provenance":"convention"},"tol_fun":{"type":"number","exclusiveMinimum":0,"default":1e-4,"x-aeoi-provenance":"convention"},
 "max_iterations":{"type":"integer","minimum":1,"x-aeoi-provenance":"convention"}},["unknowns","objective","x0"],ver=V414)
fms_out=outs("optim.local.fminsearch",{"x":{"type":"array"},"solution":{"type":"object"},"fval":{"type":["number","string"]},"iterations":{"type":"integer"},"evaluations":{"type":"integer"},"tolerances":{"type":"object"}},
 ["x","solution","fval","iterations","evaluations","tolerances"],ver=V414)
B3.update({"optim.linear.linprog":(lpm,lp_in,lp_out),"optim.local.fmincon":(fmc,fmc_in,fmc_out),"optim.local.fminsearch":(fms,fms_in,fms_out)})
# ---- v0.4.15: calculus.quadrature.integral, calculus.quadrature.trapz
V415="0.4.0"
LIM={"oneOf":[{"type":"number"},{"enum":["Inf","-Inf"]}]}
intg=man("calculus.quadrature.integral","Adaptive numerical integration","数值积分 ∫ₐᵇ f(x) dx（= MATLAB integral）：f 用 3.15 的表达式语言写，积分限可以是 ±Inf。全局自适应 Gauss–Kronrod (7, 15)，用 MATLAB integral 的变量替换（有限区间端点奇异被减弱、无穷区间映到有限区间）。缺省 rel_tol = 1e-10、abs_tol = 1e-12（比 MATLAB 的 1e-6 / 1e-10 严），结果带误差估计；达不到容差时报错并给出当前估计值。","analysis",["integral","quadrature","gauss-kronrod","calculus"],
 ["f 在积分区间上可积","积分限为数或 ±Inf"],["多重积分（integral2，以后的算子）","离散数据的积分（trapz）","区间内部有奇点时需要分段"],
 "有限 [a, b]：x = (a+b)/2 + (b−a)/4·t(3 − t²)，t ∈ [−1, 1]；(−∞, ∞)：x = t/(1 − t²)；[a, ∞)：x = a + t/(1 − t)；(−∞, b]：x = b − t/(1 − t)（均同 MATLAB integral）。初始 10 段，每次二分误差估计最大的子区间（QUADPACK qk15 与误差公式 resasc·min(1, (200|K−G|/resasc)^1.5)），子区间和用补偿求和；Σ误差 ≤ max(abs_tol, rel_tol·|I|) 停止，子区间上限 650（同 MATLAB）。后置条件：每个子区间再二分一次的积分值与返回值之差 ≤ 容差",
 [{"type":"textbook","cite":"R. Piessens, E. de Doncker-Kapenga, C. W. Überhuber, D. K. Kahaner, QUADPACK, Springer 1983（qk15、qag）"},
  {"type":"paper","cite":"L. F. Shampine, Vectorized adaptive quadrature in MATLAB, J. Comput. Appl. Math. 211(2), 2008","doi":"10.1016/j.cam.2006.11.021"}],
 "quad::integral (adaptive Gauss–Kronrod 7–15)","T1",{"max_intervals":650,"max_expression_length":4096},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.RANGE","E.VALIDATION.EXPRESSION","E.NUMERICAL.NON_FINITE_EVALUATION","E.NUMERICAL.NO_CONVERGENCE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 [],sparse={},ver=V415,backend=BACK_NUM("quad::integral"))
intg_in=ins("calculus.quadrature.integral",{"expression":EXPR,"variable":dict(NAME,default="x",**{"x-aeoi-provenance":"convention"}),"params":PARAMS,
 "limits":{"type":"array","items":LIM,"minItems":2,"maxItems":2,"x-aeoi-provenance":"supplied"},
 "rel_tol":{"type":"number","minimum":0,"default":1e-10,"x-aeoi-provenance":"convention"},"abs_tol":{"type":"number","minimum":0,"default":1e-12,"x-aeoi-provenance":"convention"}},["expression","limits"],ver=V415)
intg_out=outs("calculus.quadrature.integral",{"value":{"type":["number","string"]},"error_estimate":{"type":["number","string"]},"tolerances":{"type":"object"},"limits":{"type":"array"},
 "intervals":{"type":"integer"},"evaluations":{"type":"integer"}},["value","error_estimate","tolerances","limits","intervals","evaluations"],ver=V415)
trz=man("calculus.quadrature.trapz","Trapezoidal integration of data","对离散数据做梯形积分（= MATLAB trapz；cumulative = true 时 = cumtrapz）。y 每行一个通道；采样位置必须显式给出：x（每个点的位置）或 dx（等间距步长）——MATLAB 的 trapz(y) 缺省步长 1，常被误用。补偿求和，对给定数据精确到一次舍入。","analysis",["integral","trapz","cumtrapz","data"],
 ["y 为 [n] 或 [k, n]","x（[n]）或 dx 恰好给出一种"],["由函数求积分（integral）","高阶求积（Simpson 等）"],
 "Σ (x_{k+1} − x_k)·(y_k + y_{k+1})/2，每项拆成两个乘积用补偿算术（Dot2）累加；cumulative 时给出部分和",
 [{"type":"paper","cite":"T. Ogita, S. M. Rump, S. Oishi, Accurate sum and dot product, SIAM J. Sci. Comput. 26(6), 2005","doi":"10.1137/030601818"}],
 "quad::trapz (compensated trapezoidal sum)","T1",{"max_points":10000000},
 ["E.VALIDATION.SCHEMA","E.VALIDATION.SHAPE","E.VALIDATION.DTYPE","E.VALIDATION.NON_FINITE","E.VALIDATION.DIGEST_MISMATCH","E.VALIDATION.REF_UNSUPPORTED","E.VALIDATION.REF_FORBIDDEN","E.VALIDATION.OPTION_CONFLICT","E.RESOURCE.INPUT_TOO_LARGE","E.RESOURCE.TIMEOUT","E.INTERNAL.PANIC"],
 [],sparse={},ver=V415,backend=BACK_NUM("quad::trapz"))
trz_in=ins("calculus.quadrature.trapz",{"y":M,"x":M,"dx":{"type":"number","x-aeoi-provenance":"user"},"cumulative":{"type":"boolean","default":False,"x-aeoi-provenance":"convention"}},["y"],ver=V415)
trz_out=outs("calculus.quadrature.trapz",{"value":{"description":"积分值（y 为 [k, n] 时 [k]）"},"cumulative":{"description":"部分和（cumulative = true 时），形状同 y；否则 null"},"spacing":{"description":"回显：\"x\" 或 {dx}"}},["value","cumulative","spacing"],ver=V415)
B3.update({"calculus.quadrature.integral":(intg,intg_in,intg_out),"calculus.quadrature.trapz":(trz,trz_in,trz_out)})
ALL_OPS={"control.lti.bode":(bode,bode_in,bode_out),"control.lti.step":(step,step_in,step_out),"control.design.lqr":(lqr,lqr_in,lqr_out)}|B2|B3
for op,props in DESC.items():
    for (m_,i_,o_) in [ALL_OPS[op]]:
        for k,d in props.items():
            assert k in i_["properties"], (op,k)
            old=i_["properties"][k].get("description")
            if old and old!=d: print(f"  {op}.{k}: description {old!r} → DESC")
            i_["properties"][k]=dict(json.loads(json.dumps(i_["properties"][k])),description=d)  # copy: property dicts are shared
# ---- result receipts (spec 4.3, v0.3.3): static part of every receipt. postconditions = trust.checks keys that
# MUST be true for a "verified" claim (E-11); diagnostic/property checks (e.g. *_symmetric before symmetrization,
# asymptotically_stable) are deliberately not post-conditions. enforced_by: validation | error:<code> |
# warning:<code> | check:<key> | caller (not checked by the operator).
A=lambda i,st,by:{"id":i,"statement":st,"enforced_by":by}
LTI_A=[A("lti_model","输入系统准确描述被研究对象（单位、符号、row-major 录入）","caller"),A("proper","系统为 proper（deg num ≤ deg den）","error:E.DOMAIN.IMPROPER_SYSTEM"),A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE")]
RECEIPT={
 "control.lti.bode":{"method_id":"aeoi:bode:horner-or-complex-lu","assumptions":LTI_A+[A("grid","频率网格与单位为调用方所需（rad/s 或 Hz）","caller"),A("no_pole_on_grid","频点不落在极点上","warning:W.NUMERICAL.NEAR_POLE")],
   "exactness":{"class":"approximate","bound_kind":"a_priori"},"postconditions":["all_finite","frequencies_strictly_increasing"]},
 "control.lti.step":{"method_id":"aeoi:step:zoh-expm-pade13-recursion","assumptions":LTI_A+[A("zoh_exact","阶跃输入在 ZOH 下无离散化误差（只有 expm 与递推舍入）","validation"),A("grid","时间网格足以分辨动态","warning:W.ACCURACY.GRID_TOO_COARSE")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["all_finite"]},
 "control.lti.impulse":{"method_id":"aeoi:impulse:zoh-expm-pade13-recursion","assumptions":LTI_A+[A("convention","python-control 冲激约定（连续 x0 = B、忽略 D；离散按 discrete_scaling）","validation"),A("grid","时间网格足以分辨动态","warning:W.ACCURACY.GRID_TOO_COARSE")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["all_finite"]},
 "control.design.lqr":{"method_id":"aeoi:lqr:sign-function+kform-newton-kleinman-dot2","assumptions":[A("weights","Q 对称半正定、R 对称正定、[[Q,N],[Nᵀ,R]] ⪰ 0","error:E.DOMAIN.WEIGHT_NOT_PSD"),A("R_pd","R 正定","error:E.DOMAIN.R_NOT_POSITIVE_DEFINITE"),A("stabilizable","(A,B) 可镇定","error:E.DOMAIN.NOT_STABILIZABLE"),A("no_imag_axis_unobservable","(Q̄,Ā) 在虚轴上没有不可观模态","error:E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG"),A("model","A、B、Q、R、N 准确描述设计问题（单位一致、row-major）","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["S_psd","closed_loop_stable"]},
 "control.design.dlqr":{"method_id":"aeoi:dlqr:sda+newton-hewer","assumptions":[A("weights","Q 对称半正定、R 对称正定","error:E.DOMAIN.WEIGHT_NOT_PSD"),A("stabilizable","(A,B) 可镇定","error:E.DOMAIN.NOT_STABILIZABLE"),A("no_unit_circle_unobservable","单位圆上没有不被 Q 观测的模态","error:E.DOMAIN.SYMPLECTIC_UNIT_CIRCLE_EIG"),A("model","离散模型 x[k+1] = A x[k] + B u[k] 与采样周期由调用方保证","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["S_psd","closed_loop_stable"]},
 "control.math.lyap":{"method_id":"aeoi:lyap:complex-schur-bartels-stewart+dot2-refinement","assumptions":[A("nonsingular","方程非奇异（连续 λᵢ + λ̄ⱼ ≠ 0，离散 λᵢλ̄ⱼ ≠ 1）","error:E.DOMAIN.LYAP_SINGULAR"),A("convention","方程约定 A X + X Aᵀ + Q = 0 / A X Aᵀ − X + Q = 0（不是 Aᵀ X + X A）","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":[]},
 "control.lti.convert":{"method_id":"aeoi:convert:canonical-realization+companion-roots","assumptions":LTI_A+[A("nonminimal","不做零极点对消（MIMO 结果为非最小实现）","warning:W.DOMAIN.NONMINIMAL_REALIZATION")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["response_consistent"]},
 "control.lti.poles_zeros":{"method_id":"aeoi:poles-zeros:balanced-qr+emami-naeini-van-dooren","assumptions":LTI_A+[A("nondegenerate","系统满法秩（零点有定义）","error:E.DOMAIN.DEGENERATE_SYSTEM")],
   "exactness":{"class":"approximate","bound_kind":"none"},"postconditions":[]},
 "control.analysis.ctrb_obsv":{"method_id":"aeoi:ctrb-obsv:krylov-svd-rank+pbh+bartels-stewart-gramian","assumptions":LTI_A+[A("rank_threshold","数值秩阈值 max(n,m)·u·σ₁ 有意义（不在阈值附近）","warning:W.NUMERICAL.RANK_AMBIGUOUS")],
   "exactness":{"class":"approximate","bound_kind":"none"},"postconditions":["pbh_agrees_ctrb","pbh_agrees_obsv"]},
 "control.analysis.margin":{"method_id":"aeoi:margin:crossing-polynomial-roots+secant","assumptions":LTI_A+[A("siso","开环 SISO、单位负反馈","error:E.DOMAIN.MIMO_NOT_SUPPORTED"),A("no_delay","没有时滞","caller")],
   "exactness":{"class":"approximate","bound_kind":"none"},"postconditions":["grid_audit_clean"]},
 "control.lti.c2d":{"method_id":"aeoi:c2d:expm-pade13-or-tustin-lu","assumptions":[A("ts_from_user","采样周期 ts 来自用户（不是假设值）","caller"),A("continuous","输入为连续系统","error:E.DOMAIN.NOT_CONTINUOUS"),A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE")],
   "exactness":{"class":"approximate","bound_kind":"a_priori"},"postconditions":["pole_mapping_consistent"]},
 "control.design.place":{"method_id":"aeoi:place:knv0-complex","assumptions":[A("controllable","(A,B) 可控","error:E.DOMAIN.NOT_CONTROLLABLE"),A("multiplicity","期望极点重数 ≤ rank(B)","error:E.DOMAIN.POLE_MULTIPLICITY"),A("nonunique","多输入时 K 不唯一：结果是满足极点配置的一个 K","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["poles_placed","conjugate_closed"]},
 "control.lti.series":{"method_id":"aeoi:series:polynomial-or-block","assumptions":[A("lti_model","输入系统准确描述被研究对象","caller"),A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("improper","相对阶 ≤ 8 的 improper tf 可作为操作数；ss 输出仍拒绝","error:E.DOMAIN.IMPROPER_SYSTEM"),A("same_ts","两个系统采样周期相同","error:E.DOMAIN.SAMPLE_TIME_MISMATCH"),A("dims","sys1 的输出数 = sys2 的输入数","error:E.VALIDATION.SHAPE"),A("no_cancel","不做零极点对消","caller"),A("lineage","lineage 若给出，由调用方保证它指向这些操作数；算子不核验","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_priori"},"postconditions":["response_consistent","sample_times_equal","all_finite"]},
 "control.lti.parallel":{"method_id":"aeoi:parallel:polynomial-or-block","assumptions":[A("lti_model","输入系统准确描述被研究对象","caller"),A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("improper","相对阶 ≤ 8 的 improper tf 可作为操作数；ss 输出仍拒绝","error:E.DOMAIN.IMPROPER_SYSTEM"),A("same_ts","两个系统采样周期相同","error:E.DOMAIN.SAMPLE_TIME_MISMATCH"),A("dims","输入数与输出数都相同","error:E.VALIDATION.SHAPE"),A("no_cancel","不做零极点对消","caller"),A("lineage","lineage 只回显，不核验","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_priori"},"postconditions":["response_consistent","sample_times_equal","all_finite"]},
 "control.lti.feedback":{"method_id":"aeoi:feedback:polynomial-or-block","assumptions":[A("lti_model","输入系统准确描述被研究对象","caller"),A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("improper","相对阶 ≤ 8 的 improper tf 可作为操作数；ss 输出仍拒绝","error:E.DOMAIN.IMPROPER_SYSTEM"),A("same_ts","两个系统采样周期相同","error:E.DOMAIN.SAMPLE_TIME_MISMATCH"),A("dims","反馈维度匹配","error:E.VALIDATION.SHAPE"),A("sign","sign 为 -1 或 +1，默认 -1","validation"),A("well_posed","代数环可逆","error:E.DOMAIN.ALGEBRAIC_LOOP"),A("lineage","lineage 只回显，不核验","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["response_consistent","sample_times_equal","all_finite"]},
 "control.design.pid":{"method_id":"aeoi:pid:closed-form","assumptions":[A("gains","Kp 以及用到的 Ki/Kd/Ti/Td/Tf 由调用方给定；0 表示该项不出现，不是猜测的物理增益","caller"),A("form","parallel 与 standard 的参数不混用","error:E.VALIDATION.OPTION_CONFLICT"),A("no_tune","本算子不自动整定增益","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_priori"},"postconditions":["formula_matches","all_finite"]},
 "control.lti.stepinfo":{"method_id":"aeoi:stepinfo:zoh-grid-metrics","assumptions":LTI_A+[A("grid_definition","时间指标是网格点，不插值；量化误差 ≤ dt","validation"),A("thresholds","阈值用请求里的值；省略则是规范默认 10–90% 与 2%","validation"),A("steady","稳态取 DC 增益，不取最后采样","validation")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["samples_finite","settling_matches_definition"]},
 "control.lti.pade":{"method_id":"aeoi:pade:golub-van-loan-11.3.1","assumptions":[A("delay","delay 由调用方给出，不是假设值","caller"),A("order","阶次由调用方给出，不是假设值","caller"),A("diagonal","只做 [N/N] 对角近似","validation"),A("approximation","结果逼近 e^{-θs}，逼近误差在 approximation，不在系数舍入界里","validation")],
   "exactness":{"class":"approximate","bound_kind":"a_priori"},"postconditions":["taylor_matched","dc_is_one","all_finite"]},
}
RECEIPT["ode.ivp.solve"]={"method_id":"aeoi:ode:dopri54","assumptions":[A("model","states、rhs、params、initial 准确描述要解的方程","caller"),A("expressions","每个 rhs 是合法的 AEOI 表达式，只引用已声明的名字（3.15）","error:E.VALIDATION.EXPRESSION"),A("smooth_nonstiff","f 在积分区间上足够光滑、问题非刚性","warning:W.NUMERICAL.STIFF_PROBLEM"),A("finite_rhs","f 在解经过的点上有限","error:E.NUMERICAL.NON_FINITE_EVALUATION"),A("tolerance_is_local","rtol/atol 控制每步的局部误差；全局误差可能更大，由重解估计","validation")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["all_finite","tolerance_consistent"]}
RECEIPT["signal.spectral.fft"]={"method_id":"aeoi:fft:kernels-mixed-radix","assumptions":[A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("uniform_sampling","x 是均匀采样序列；频率轴按 sample_rate_hz（或 cycles/sample）解释","caller"),A("conventions","符号、归一化、单双边、频率轴顺序按请求或规范默认","validation"),A("periodic_extension","DFT 把 x 视为周期 n 的序列；截断或补零改变频谱（泄漏），不是误差","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_priori"},"postconditions":["parseval","dft_spot_check","all_finite"]}
RECEIPT["linalg.system.solve"]={"method_id":"aeoi:linsolve:lu-refine-or-householder-qr","assumptions":[A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("not_underdetermined","m ≥ n","error:E.DOMAIN.UNDERDETERMINED"),A("full_rank","A 列满秩（方阵非奇异）","error:E.NUMERICAL.SINGULAR"),A("conditioning","κ·u ≪ 1，否则解没有有效数字（前向误差 ≈ κ·η）","warning:W.NUMERICAL.ILL_CONDITIONED"),A("row_major","A 按 row-major 录入","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["backward_stable","all_finite"]}
RECEIPT["calculus.quadrature.integral"]={"method_id":"aeoi:integral:adaptive-gauss-kronrod-7-15","assumptions":[A("expression","被积函数是合法的 3.15 表达式","error:E.VALIDATION.EXPRESSION"),A("integrable","积分收敛（可积，无内部不可积奇点）","error:E.NUMERICAL.NO_CONVERGENCE"),A("finite_values","被积函数在求值点上有限","error:E.NUMERICAL.NON_FINITE_EVALUATION"),A("error_estimate_heuristic","误差估计是启发式的；可能漏掉区间里窄的尖峰","check:refinement_consistent")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["tolerance_met","refinement_consistent","all_finite"]}
RECEIPT["calculus.quadrature.trapz"]={"method_id":"aeoi:trapz:compensated","assumptions":[A("spacing","采样位置（x 或 dx）由调用方显式给出","error:E.VALIDATION.OPTION_CONFLICT"),A("sampling","数据采样足以描述被积函数（梯形公式对数据精确，对真实函数的误差算子无法估计）","caller"),A("channels","y 每行一个通道","caller")],
   "exactness":{"class":"approximate","bound_kind":"none"},"postconditions":["all_finite"]}
RECEIPT["optim.linear.linprog"]={"method_id":"aeoi:linprog:two-phase-simplex-bland","assumptions":[A("finite","所有数据有限","error:E.VALIDATION.NON_FINITE"),A("feasible","约束有可行解","error:E.DOMAIN.INFEASIBLE"),A("bounded","目标在可行域上有下界","error:E.DOMAIN.UNBOUNDED"),A("default_bounds","变量缺省没有界（MATLAB 约定，与 scipy 不同）","validation"),A("unique","最优解可能不唯一；给出的是一个最优顶点","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["kkt","all_finite"]}
RECEIPT["optim.local.fmincon"]={"method_id":"aeoi:fmincon:sqp-bfgs-ad","assumptions":[A("expression","目标与约束是合法的 3.15 表达式","error:E.VALIDATION.EXPRESSION"),A("converges","SQP 从 x0 收敛到 KKT 点","error:E.NUMERICAL.NO_CONVERGENCE"),A("local_only","结果只是局部最优（或满足一阶条件的点；二阶条件未检查），不一定是全局最优","caller"),A("smooth","目标与约束在解附近光滑","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["kkt","all_finite"]}
RECEIPT["optim.local.fminsearch"]={"method_id":"aeoi:fminsearch:nelder-mead-lagarias","assumptions":[A("expression","目标是合法的 3.15 表达式","error:E.VALIDATION.EXPRESSION"),A("converges","单纯形在迭代上限内收缩到容差","error:E.NUMERICAL.NO_CONVERGENCE"),A("local_only","结果只是 tol_x 分辨率下的局部极小；Nelder–Mead 没有一般的收敛保证","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["probe_local_minimum","all_finite"]}
RECEIPT["nonlinear.root.fzero"]={"method_id":"aeoi:fzero:brent-zeroin","assumptions":[A("expression","f 是合法的 3.15 表达式","error:E.VALIDATION.EXPRESSION"),A("sign_change","区间两端变号（给定或搜到）","error:E.DOMAIN.NO_SIGN_CHANGE"),A("continuous","f 在区间上连续（否则变号点可能是极点）","check:not_a_pole"),A("intended_root","求的是区间里的根；只给 x0 时搜到的区间不一定含想要的根","warning:W.DOMAIN.BRACKET_SEARCHED")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["sign_change","not_a_pole","all_finite"]}
RECEIPT["nonlinear.system.fsolve"]={"method_id":"aeoi:fsolve:dogleg-ad-jacobian","assumptions":[A("expression","方程是合法的 3.15 表达式","error:E.VALIDATION.EXPRESSION"),A("square","方程个数 = 未知量个数","error:E.VALIDATION.SHAPE"),A("converges","从 x0 出发收敛到根","error:E.NUMERICAL.NO_CONVERGENCE"),A("isolated_simple_root","解处雅可比非奇异（单根）","warning:W.NUMERICAL.ILL_CONDITIONED"),A("which_root","x0 附近的那个解是用户要的（方程组可能有多个解）","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["newton_step_small","all_finite"]}
RECEIPT["signal.design.butter"]={"method_id":"aeoi:butter:prototype-zpk-bilinear","assumptions":[A("spec_from_user","阶数、类型与截止频率来自用户（含单位）","validation"),A("below_nyquist","数字：截止频率低于 Nyquist","error:E.DOMAIN.ABOVE_NYQUIST"),A("ba_accuracy","b/a 多项式形式未失去精度（否则用 sos 或 zpk）","warning:W.NUMERICAL.ILL_CONDITIONED")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["stable","edge_gain","sos_consistent","all_finite"]}
RECEIPT["signal.filter.filter"]={"method_id":"aeoi:filter:transposed-direct-form-ii","assumptions":[A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("one_form","b/a 与 sos 恰好给出一种","error:E.VALIDATION.OPTION_CONFLICT"),A("nonzero_a0","a0 ≠ 0","error:E.VALIDATION.ZERO_DENOMINATOR"),A("stable","滤波器稳定（否则输出可能无界）","warning:W.DOMAIN.UNSTABLE_SYSTEM"),A("channels","x 每行一个通道","caller"),A("coefficient_order","b、a 是 z⁻¹ 的升幂（MATLAB 约定）","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["difference_equation","all_finite"]}
RECEIPT["fit.poly.polyfit"]={"method_id":"aeoi:polyfit:vandermonde-householder-qr","assumptions":[A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("enough_points","x 至少有 degree + 1 个不同的值","error:E.DOMAIN.UNDERDETERMINED"),A("numerical_rank","单项式基在 f64 下列满秩","error:E.NUMERICAL.SINGULAR"),A("conditioning","κ₁(R·D⁻¹) ≤ 1e10，否则数据的微小扰动会被大幅放大到系数上","warning:W.NUMERICAL.ILL_CONDITIONED"),A("model","数据确实可以用该次数的多项式描述；次数由用户选择","caller"),A("noise_model","std_errors 假设误差独立、等方差","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["backward_stable","all_finite"]}
RECEIPT["linalg.decomp.eig"]={"method_id":"aeoi:eig:symmetric-jacobi-or-schur-backsubstitution","assumptions":[A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("square","A 为方阵","error:E.VALIDATION.SHAPE"),A("converged","Jacobi 扫描或 Schur 的 QR 迭代在上限内收敛","error:E.NUMERICAL.NO_CONVERGENCE"),A("nondefective","特征值条件数 ≤ 1e8，否则可能亏损：特征值只有约一半有效数字，特征向量接近平行","warning:W.NUMERICAL.ILL_CONDITIONED"),A("row_major","A 按 row-major 录入","caller"),A("uniqueness","特征向量只确定到规范规定的规一化；重特征值对应的特征向量只确定到子空间","validation")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["residual_small","orthonormal","all_finite"]}
RECEIPT["linalg.decomp.svd"]={"method_id":"aeoi:svd:one-sided-jacobi","assumptions":[A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("converged","Jacobi 扫描在上限内收敛","error:E.NUMERICAL.NO_CONVERGENCE"),A("row_major","A 按 row-major 录入","caller"),A("uniqueness","奇异向量只确定到规范规定的符号；重奇异值对应的奇异向量只确定到子空间","validation"),A("absolute_accuracy","小奇异值只有绝对精度（≈ u·s_max），没有相对精度","validation")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["reconstructs","orthonormal_u","orthonormal_v","all_finite"]}
RECEIPT["interp.grid.interp1"]={"method_id":"aeoi:interp1:piecewise","assumptions":[A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("strictly_increasing","x 严格递增","error:E.VALIDATION.GRID"),A("in_range","查询点在数据范围内，或调用方显式选择了范围外的处理","error:E.DOMAIN.EXTRAPOLATION_REQUIRED"),A("model","插值反映的是数据之间的一种约定曲线（方法决定），不是真实函数；与真实函数的误差算子无法估计","caller"),A("channels","y 每行一个通道","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_priori"},"postconditions":["all_finite","pieces_interpolate","spline_c2","local_bounds"]}
RECEIPT["control.design.pidtune"]={"method_id":"aeoi:pidtune:frequency-point","assumptions":[A("lti_model","plant 准确描述被控对象（单位、符号）","caller"),A("siso_continuous","SISO、连续、proper","error:E.DOMAIN.NOT_CONTINUOUS"),A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("target_from_user","ωc 由调用方给出；PM、Ti/Td、N 用请求里的值或规范默认","validation"),A("achievable","所需控制器相位在 PI/PID 可达范围内","error:E.DOMAIN.TARGET_NOT_ACHIEVABLE"),A("closed_loop_stable","结果闭环稳定","error:E.DOMAIN.TARGET_NOT_ACHIEVABLE"),A("single_crossover","L 只有一个增益穿越点（否则实际相位裕度可能小于目标）","warning:W.DOMAIN.EXTRA_CROSSOVER")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["target_met","closed_loop_stable","sub_operator_checks","all_finite"]}
RECEIPT["control.lti.lsim"]={"method_id":"aeoi:lsim:block-expm-pade13-recursion","assumptions":LTI_A+[A("interpolation","采样之间的输入按 foh（线性）或 zoh（保持）解释；响应对这个解释精确","validation"),A("uniform_grid","均匀网格 t_k = t0 + k·dt；离散系统 dt = ts","error:E.VALIDATION.GRID"),A("x0_coordinates","x0 属于给定的 ss 实现","error:E.VALIDATION.OPTION_CONFLICT"),A("input_sampling","输入采样足以描述真实输入（结果对真实输入的误差取决于采样）","caller"),A("grid","dt 足以分辨系统动态","warning:W.ACCURACY.GRID_TOO_COARSE")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["all_finite","half_step_consistent"]}
RECEIPT["control.analysis.loopsens"]={"method_id":"aeoi:loopsens:connect+margin+stepinfo","assumptions":[A("lti_model","plant、controller、sensor 准确描述回路（单位、符号、row-major 录入）","caller"),A("topology","单回路负反馈 r → e = r − H·y → C → G → y","validation"),A("finite","所有输入数值有限","error:E.VALIDATION.NON_FINITE"),A("same_ts","三个系统采样周期相同","error:E.DOMAIN.SAMPLE_TIME_MISMATCH"),A("siso","SISO","error:E.DOMAIN.MIMO_NOT_SUPPORTED"),A("proper_for_metrics","裕度与阶跃指标要求 L、T proper，否则该项为 null","warning:W.DOMAIN.ANALYSIS_SKIPPED"),A("definitions","带宽、阶跃阈值用请求里的值；省略则是规范默认 −3 dB、10–90%、2%","validation"),A("no_cancel","不做零极点对消","caller")],
   "exactness":{"class":"approximate","bound_kind":"a_posteriori_heuristic"},"postconditions":["closed_loop_identity","stability_consistent","all_finite","sub_operator_checks"]}
# v0.3.6 E-12: how much of the result each post-condition actually covers. Absent = "full" (evaluated on
# the whole result). "test_points": an identity checked at a few deterministic points (does not shrink
# the result). "sampled": evaluated on a subset/compressed copy of the result → claim at most spot_checked.
COVERAGE={op:{"response_consistent":"test_points"} for op in ("control.lti.series","control.lti.parallel","control.lti.feedback","control.lti.convert")}
COVERAGE.update({"control.design.pid":{"formula_matches":"test_points"},"control.analysis.margin":{"grid_audit_clean":"sampled"},
 "control.analysis.loopsens":{"closed_loop_identity":"test_points","sub_operator_checks":"sampled"},
 "control.lti.lsim":{"half_step_consistent":"test_points"},
 "control.design.pidtune":{"target_met":"test_points","sub_operator_checks":"sampled"},
 "signal.spectral.fft":{"parseval":"full","dft_spot_check":"sampled"},
 "ode.ivp.solve":{"tolerance_consistent":"full"},
 "linalg.system.solve":{"backward_stable":"full"},
 "interp.grid.interp1":{"pieces_interpolate":"full","spline_c2":"full","local_bounds":"full"},
 "fit.poly.polyfit":{"backward_stable":"full"},
 "signal.design.butter":{"stable":"full","edge_gain":"full","sos_consistent":"test_points"},
 "signal.filter.filter":{"difference_equation":"full"},
 "nonlinear.root.fzero":{"sign_change":"full","not_a_pole":"full"},
 "optim.linear.linprog":{"kkt":"full"},"calculus.quadrature.integral":{"tolerance_met":"full","refinement_consistent":"full"},"optim.local.fmincon":{"kkt":"full"},"optim.local.fminsearch":{"probe_local_minimum":"test_points"},
 "nonlinear.system.fsolve":{"newton_step_small":"full"},
 "linalg.decomp.eig":{"residual_small":"full","orthonormal":"full"},
 "linalg.decomp.svd":{"reconstructs":"full","orthonormal_u":"full","orthonormal_v":"full"}})
# v0.3.6 E-13: result fields that are not usable when a post-condition fails (listed in receipt.evidence.unreliable).
GUARDS={"control.analysis.ctrb_obsv":{"pbh_agrees_ctrb":["/ctrb/rank","/ctrb/full_rank","/minimal"],"pbh_agrees_obsv":["/obsv/rank","/obsv/full_rank","/minimal"]},
 "control.analysis.loopsens":{"closed_loop_identity":["/open_loop","/closed_loop","/sensitivity"],"stability_consistent":["/closed_loop_stable"],"sub_operator_checks":["/margins","/gain_crossover_rad_s","/gain_crossover_hz","/step_metrics"]}}
for op in ("control.lti.series","control.lti.parallel","control.lti.feedback"):
    GUARDS[op]={"response_consistent":["/system"]}
GUARDS["control.lti.lsim"]={"half_step_consistent":["/y","/x"]}
GUARDS["ode.ivp.solve"]={"tolerance_consistent":["/y"]}
GUARDS["linalg.system.solve"]={"backward_stable":["/x"]}
GUARDS["interp.grid.interp1"]={"pieces_interpolate":["/yq"],"spline_c2":["/yq"],"local_bounds":["/yq"]}
GUARDS["signal.design.butter"]={"stable":["/z","/p","/k","/sos","/b","/a","/system"],"edge_gain":["/z","/p","/k","/sos","/b","/a","/system"],"sos_consistent":["/sos"]}
GUARDS["calculus.quadrature.integral"]={"tolerance_met":["/value","/error_estimate"],"refinement_consistent":["/value","/error_estimate"]}
GUARDS["optim.linear.linprog"]={"kkt":["/x","/fval","/lambda"]}
GUARDS["optim.local.fmincon"]={"kkt":["/x","/solution","/fval","/lambda"]}
GUARDS["optim.local.fminsearch"]={"probe_local_minimum":["/x","/solution","/fval"]}
GUARDS["nonlinear.root.fzero"]={"sign_change":["/x","/fx"],"not_a_pole":["/x","/fx"]}
GUARDS["nonlinear.system.fsolve"]={"newton_step_small":["/x","/solution"]}
GUARDS["signal.filter.filter"]={"difference_equation":["/y","/zf"]}
GUARDS["fit.poly.polyfit"]={"backward_stable":["/p","/std_errors"]}
GUARDS["linalg.decomp.eig"]={"residual_small":["/values","/vectors"],"orthonormal":["/vectors"]}
GUARDS["linalg.decomp.svd"]={"reconstructs":["/s","/U","/V","/rank","/cond2"],"orthonormal_u":["/U"],"orthonormal_v":["/V"]}
GUARDS["signal.spectral.fft"]={"parseval":["/X"],"dft_spot_check":["/X","/frequency"]}
GUARDS["control.design.pidtune"]={"target_met":["/controller","/gains","/gains_standard"],"sub_operator_checks":["/achieved"]}
for op,c in COVERAGE.items(): RECEIPT[op]["coverage"]=c
for op,g in GUARDS.items(): RECEIPT[op]["guards"]=g
for op,(m,i,o) in ALL_OPS.items():
    m["receipt"]=RECEIPT[op]
    if "E.VALIDATION.METHOD_UNAVAILABLE" not in m["errors"]: m["errors"].append("E.VALIDATION.METHOD_UNAVAILABLE")  # v0.4 3.14, every operator (v0.2 dicts bypass man())
    d=f"examples/{op}/"; w(d+"manifest.json",m); w(d+"input.schema.json",i); w(d+"output.schema.json",o)
print("ok")
