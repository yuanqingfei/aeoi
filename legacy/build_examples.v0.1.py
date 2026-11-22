import json, hashlib, os
V=json.load(open("computed_values.json"))
def r(x,n=12): return float(f"{x:.{n}g}")
def canon(o): return json.dumps(o,sort_keys=True,separators=(",",":"),ensure_ascii=False)
def h(o): return "sha256:"+hashlib.sha256(canon(o).encode()).hexdigest()
def w(path,o):
    os.makedirs(os.path.dirname(path),exist_ok=True)
    if isinstance(o,str): open(path,"w").write(o)
    else: json.dump(o,open(path,"w"),indent=2,ensure_ascii=False); open(path,"a").write("\n")
SPEC="aeoi/0.1"
BUILD="aeoi-control 0.1.0-dev+illustrative"
REF={"engine":"python-control 0.10.2 / scipy 1.18.1 / numpy 2.5.5","note":"示例数值由 python-control 在 box 上实际计算，非 MATLAB 输出；timing 与 build 字段为示意"}
G={"kind":"tf","num":[4],"den":[1,1.2,4],"ts":0,"time_unit":"s"}
# ---------------- BODE
bode_manifest={
 "spec":SPEC,"id":"control.lti.bode","version":"0.1.0","title":"Bode frequency response",
 "description":"计算连续或离散 LTI 系统在给定频率点上的幅值（dB）与相位（deg）。",
 "category":"analysis","tags":["frequency-domain","lti"],
 "applicability":{"preconditions":["system 为 proper（分子阶次 ≤ 分母阶次）","离散系统要求 w < π/ts（Nyquist 频率）"],
   "not_for":["非线性系统","时变系统"],"siso_only":False},
 "provenance":{"algorithm":"Hessenberg 化后逐频点求解 (jωI - H)x = b","references":[
   {"type":"paper","cite":"A. J. Laub, Efficient multivariable frequency response computations, IEEE TAC 26(2), 1981","doi":"10.1109/TAC.1981.1102605"},
   {"type":"textbook","cite":"G. F. Franklin, J. D. Powell, A. Emami-Naeini, Feedback Control of Dynamic Systems, ch.6"}],
   "clean_room":{"declared":True,"sources":"仅公开文献与开源实现；MATLAB 仅用于生成对照数据"}},
 "backend":{"library":"SLICOT","routine":"TB05AD","version":"5.9","binding":"ffi","license":"BSD-3-Clause"},
 "determinism":"bitwise","tolerance_class":"T3",
 "input_schema":{"$ref":"./input.schema.json"},
 "output_schema":{"$ref":"./output.schema.json"},
 "conformance":{"target":"L3","current":"draft"},"accuracy_grade":{"target":"A","current":"U"},"credentials":"aeoi://operators/control.lti.bode/0.1.0/credentials",
 "errors":["E.VALIDATION.SCHEMA","E.VALIDATION.UNIT","E.DOMAIN.IMPROPER_SYSTEM","E.DOMAIN.ABOVE_NYQUIST","E.NUMERICAL.POLE_ON_GRID"],
 "warnings":["W.NUMERICAL.NEAR_POLE","W.NUMERICAL.ILL_CONDITIONED"],
 "mcp":{"tool_name":"control_lti_bode","annotations":{"readOnlyHint":True,"idempotentHint":True,"openWorldHint":False}}}
bode_input={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.lti.bode/0.1.0/input",
 "type":"object","additionalProperties":False,"required":["system","frequency"],
 "properties":{
  "system":{"$ref":"aeoi://schemas/common/0.1/LtiSystem"},
  "frequency":{"oneOf":[
    {"type":"object","additionalProperties":False,"required":["mode","values","unit"],"properties":{
      "mode":{"const":"explicit"},"values":{"type":"array","items":{"type":"number","exclusiveMinimum":0},"minItems":1,"maxItems":100000},
      "unit":{"enum":["rad/s","Hz"]}}},
    {"type":"object","additionalProperties":False,"required":["mode","min","max","n","unit"],"properties":{
      "mode":{"const":"logspace"},"min":{"type":"number","exclusiveMinimum":0},"max":{"type":"number","exclusiveMinimum":0},
      "n":{"type":"integer","minimum":2,"maximum":100000},"unit":{"enum":["rad/s","Hz"]}}}]},
  "phase":{"type":"object","additionalProperties":False,"properties":{
     "unwrap":{"type":"boolean","default":True},
     "branch_center_deg":{"type":"number","default":0,"description":"首个频点相位落在 (c-180, c+180] 内"}}},
  "exec":{"$ref":"aeoi://schemas/common/0.1/ExecOptions"}}}
bode_output={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.lti.bode/0.1.0/output",
 "type":"object","required":["w","magnitude","phase"],"properties":{
  "w":{"$ref":"aeoi://schemas/common/0.1/Quantity","description":"频率，unit=rad/s"},
  "magnitude":{"$ref":"aeoi://schemas/common/0.1/Quantity","description":"shape [ny,nu,nw]，unit=dB（20·log10|G|）"},
  "phase":{"$ref":"aeoi://schemas/common/0.1/Quantity","description":"shape [ny,nu,nw]，unit=deg"}}}
w_=V["bode"]["w"]; n=len(w_)
bode_req={"system":G,"frequency":{"mode":"explicit","values":w_,"unit":"rad/s"},"phase":{"unwrap":True},"exec":{"request_id":"req-bode-001"}}
bode_result={
 "w":{"unit":"rad/s","value":{"dtype":"f64","shape":[n],"data":w_}},
 "magnitude":{"unit":"dB","value":{"dtype":"f64","shape":[1,1,n],"data":[r(x) for x in V["bode"]["mag_db"]]}},
 "phase":{"unit":"deg","value":{"dtype":"f64","shape":[1,1,n],"data":[r(x) for x in V["bode"]["phase_deg"]]}}}
def env(op,ver,req,result,trust,warnings=None,status="ok",tclass="T3",det="bitwise",backend=None,ms=0.0):
    return {"spec":SPEC,"status":status,"operator":{"id":op,"version":ver,"build":BUILD},
     "request_id":req["exec"]["request_id"],"result":result,"trust":trust,"warnings":warnings or [],
     "reproducibility":{"determinism":det,"tolerance_class":tclass,"input_hash":h({k:v for k,v in req.items() if k!="exec"}),
       "output_hash":h(result),"platform":"x86_64-linux-gnu","blas":"openblas-0.3.28 (pinned, OPENBLAS_CORETYPE=Haswell, threads=1)","threads":1,"seed":None},
     "timing":{"wall_ms":ms,"illustrative":True},"reference_note":REF}
poles=V["bode"]["poles"]
bode_trust={"convergence":{"applicable":False},"condition":{"max_cond_jwI_minus_A":10.9251346,"realization":"python-control tf2ss 默认实现","min_pole_distance_to_grid_rel":r(min(abs(complex(0,wi)-complex(*p))/max(abs(complex(*p)),1e-300) for wi in w_ for p in poles),6)},
 "residual":None,"error_estimate":{"magnitude_db_abs":1e-10,"phase_deg_abs":1e-9,"basis":"前向误差界 ≈ κ(jωI-A)·u，按最坏频点估计"},
 "accuracy_grade":"A","credentials":"aeoi://operators/control.lti.bode/0.1.0/credentials","derived":{"poles":{"dtype":"c128","shape":[2],"data":[[r(a),r(b)] for a,b in poles]}}}
bode_ok=env("control.lti.bode","0.1.0",bode_req,bode_result,bode_trust,ms=0.08)
bode_err_req={"system":{"kind":"tf","num":[1,0,0],"den":[1,1],"ts":0,"time_unit":"s"},"frequency":{"mode":"logspace","min":0.01,"max":100,"n":200,"unit":"rad/s"},"exec":{"request_id":"req-bode-002"}}
bode_err={"spec":SPEC,"status":"error","operator":{"id":"control.lti.bode","version":"0.1.0","build":BUILD},"request_id":"req-bode-002","result":None,
 "error":{"code":"E.DOMAIN.IMPROPER_SYSTEM","category":"domain","retryable":True,
   "message":"传递函数不是 proper：分子阶次 2 > 分母阶次 1，频率响应在 ω→∞ 时发散，无状态空间实现。",
   "agent_hint":"该算子仅接受 proper 系统。若这是理想微分器，请补充高频极点使其 proper；若输入有误，请检查 num/den 是否颠倒。",
   "details":{"num_degree":2,"den_degree":1,"path":"/system"},
   "remediation":[
     {"id":"add_filter_pole","kind":"modify_input","description":"在分母补一个远高于关心频带的极点 1/(τs+1)，τ=1e-3 s","confidence":"medium","requires_user_confirmation":True,
      "patch":[{"op":"replace","path":"/system/den","value":[0.001,1.001,1]}]},
     {"id":"swap_num_den","kind":"modify_input","description":"若 num/den 写反，交换二者","confidence":"low","requires_user_confirmation":True,
      "patch":[{"op":"replace","path":"/system/num","value":[1,1]},{"op":"replace","path":"/system/den","value":[1,0,0]}]},
     {"id":"ask","kind":"ask_user","description":"向用户确认系统模型是否为理想微分环节"}]},
 "warnings":[],"timing":{"wall_ms":0.01,"illustrative":True}}
# ---------------- STEP
tvals=V["step"]["t"]; yv=V["step"]["y"]; info=V["step"]["info"]
step_manifest=json.loads(json.dumps(bode_manifest))
step_manifest.update({"id":"control.lti.step","title":"Step response","description":"计算 LTI 系统在给定时间网格上的单位阶跃响应，并给出阶跃特性指标（上升时间、调节时间、超调等）。",
 "category":"analysis","tags":["time-domain","lti"],
 "applicability":{"preconditions":["system 为 proper","时间网格均匀、单调递增且 t0=0","若请求 step_info，系统应渐近稳定（否则指标标记为 null 并给出 warning）"],"not_for":["非线性系统","任意输入（用 control.lti.lsim）"],"siso_only":False},
 "provenance":{"algorithm":"零阶保持精确离散化：对增广矩阵 [[A,B],[0,0]]·dt 求矩阵指数后递推（阶跃输入在 ZOH 下无离散化误差）",
   "references":[{"type":"paper","cite":"C. Van Loan, Computing integrals involving the matrix exponential, IEEE TAC 23(3), 1978","doi":"10.1109/TAC.1978.1101743"},
     {"type":"paper","cite":"N. J. Higham, The scaling and squaring method for the matrix exponential revisited, SIMAX 26(4), 2005","doi":"10.1137/04061101X"},
     {"type":"textbook","cite":"K. Ogata, Modern Control Engineering, 5th ed., ch.5（阶跃指标定义）"}],
   "clean_room":{"declared":True,"sources":"仅公开文献与开源实现；MATLAB 仅用于生成对照数据"}},
 "backend":{"library":"faer (pure Rust) expm + 自研递推","version":"0.19","binding":"native","license":"MIT"},
 "determinism":"bitwise","tolerance_class":"T2",
 "input_schema":{"$ref":"./input.schema.json"},"output_schema":{"$ref":"./output.schema.json"},
 "credentials":"aeoi://operators/control.lti.step/0.1.0/credentials",
 "errors":["E.VALIDATION.SCHEMA","E.VALIDATION.GRID","E.DOMAIN.IMPROPER_SYSTEM","E.RESOURCE.OUTPUT_TOO_LARGE"],
 "warnings":["W.DOMAIN.UNSTABLE_SYSTEM","W.ACCURACY.GRID_TOO_COARSE","W.DOMAIN.NOT_SETTLED"],
 "mcp":{"tool_name":"control_lti_step","annotations":{"readOnlyHint":True,"idempotentHint":True,"openWorldHint":False}}})
step_input={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.lti.step/0.1.0/input","type":"object","additionalProperties":False,
 "required":["system","time"],"properties":{"system":{"$ref":"aeoi://schemas/common/0.1/LtiSystem"},
 "time":{"oneOf":[{"type":"object","additionalProperties":False,"required":["mode","t_final","dt","unit"],"properties":{"mode":{"const":"uniform"},"t_final":{"type":"number","exclusiveMinimum":0},"dt":{"type":"number","exclusiveMinimum":0},"unit":{"const":"s"}}},
   {"type":"object","additionalProperties":False,"required":["mode","t_final","unit"],"properties":{"mode":{"const":"auto"},"t_final":{"type":["number","null"]},"unit":{"const":"s"}},"description":"auto 网格不参与 parity 测试"}]},
 "step_info":{"type":"object","additionalProperties":False,"properties":{"enabled":{"type":"boolean","default":True},
   "rise_time_limits":{"type":"array","items":{"type":"number","minimum":0,"maximum":1},"minItems":2,"maxItems":2,"default":[0.1,0.9]},
   "settling_threshold":{"type":"number","exclusiveMinimum":0,"maximum":0.5,"default":0.02}}},
 "output":{"type":"object","properties":{"return_samples":{"enum":["all","decimated","none"],"default":"all"},"max_samples":{"type":"integer","default":10001}}},
 "exec":{"$ref":"aeoi://schemas/common/0.1/ExecOptions"}}}
step_output={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.lti.step/0.1.0/output","type":"object","required":["t","y"],
 "properties":{"t":{"$ref":"aeoi://schemas/common/0.1/Quantity"},"y":{"$ref":"aeoi://schemas/common/0.1/Quantity","description":"shape [ny,nu,nt]"},
 "info":{"type":["object","null"],"description":"每个 (输出,输入) 通道一组指标；SISO 时为单个对象"}}}
step_req={"system":G,"time":{"mode":"uniform","t_final":10,"dt":0.01,"unit":"s"},"step_info":{"enabled":True,"rise_time_limits":[0.1,0.9],"settling_threshold":0.02},"output":{"return_samples":"decimated","max_samples":11},"exec":{"request_id":"req-step-001"}}
step_result={"t":{"unit":"s","value":{"dtype":"f64","shape":[11],"data":tvals},"decimated_from":1001},
 "y":{"unit":"1","value":{"dtype":"f64","shape":[1,1,11],"data":[r(x) for x in yv]}},
 "info":{"rise_time":{"unit":"s","value":r(info["RiseTime"],8)},"settling_time":{"unit":"s","value":r(info["SettlingTime"],8)},
  "overshoot":{"unit":"%","value":r(info["Overshoot"],8)},"peak":{"unit":"1","value":r(info["Peak"],10)},"peak_time":{"unit":"s","value":r(info["PeakTime"],8)},
  "steady_state":{"unit":"1","value":1.0,"source":"dcgain"},"grid_resolution":{"unit":"s","value":0.01}}}
step_trust={"convergence":{"applicable":False},"condition":{"norm1_A_dt":0.04,"realization":"python-control tf2ss 默认实现","scaling_squarings":0},"residual":None,
 "error_estimate":{"y_abs":1e-13,"basis":"expm 后向误差 ≤ u（Higham 2005）+ 递推误差累积 O(N·u)","info_time_metrics_abs_s":0.01,"info_basis":"时间类指标受网格分辨率 dt 限制（线性插值），误差 ≤ dt"},
 "accuracy_grade":"A","credentials":"aeoi://operators/control.lti.step/0.1.0/credentials","derived":{"dcgain":1.0,"asymptotically_stable":True}}
step_ok=env("control.lti.step","0.1.0",step_req,step_result,step_trust,tclass="T2",ms=0.21)
step_err_req={"system":G,"time":{"mode":"uniform","t_final":10,"dt":1e-7,"unit":"s"},"exec":{"request_id":"req-step-002"}}
step_err={"spec":SPEC,"status":"error","operator":{"id":"control.lti.step","version":"0.1.0","build":BUILD},"request_id":"req-step-002","result":None,
 "error":{"code":"E.RESOURCE.OUTPUT_TOO_LARGE","category":"resource","retryable":True,
  "message":"请求的时间网格包含 100000001 个采样点，超过单次返回上限 1000000。",
  "agent_hint":"缩小 dt 或缩短 t_final，或只返回抽样后的曲线与 step_info。对该系统（最快极点 |p|≈2.0 rad/s），dt=0.01 s 已足够。",
  "details":{"n_samples":100000001,"limit":1000000,"fastest_pole_rad_s":2.0,"path":"/time/dt"},
  "remediation":[{"id":"coarser_grid","kind":"modify_input","description":"dt 改为 0.01 s（≈ 最快时间常数的 1/50）","confidence":"high","requires_user_confirmation":False,
     "patch":[{"op":"replace","path":"/time/dt","value":0.01}]},
   {"id":"decimate","kind":"modify_input","description":"保持 dt，但只返回 1001 个抽样点","confidence":"medium","requires_user_confirmation":False,
     "patch":[{"op":"add","path":"/output","value":{"return_samples":"decimated","max_samples":1001}}]}]},
 "warnings":[],"timing":{"wall_ms":0.01,"illustrative":True}}
# ---------------- LQR
L=V["lqr"]
lqr_manifest=json.loads(json.dumps(bode_manifest))
lqr_manifest.update({"id":"control.design.lqr","title":"Continuous-time LQR","description":"求解连续时间代数 Riccati 方程 (CARE)，返回最优状态反馈增益 K、Riccati 解 S 与闭环极点，使 J=∫(xᵀQx+uᵀRu+2xᵀNu)dt 最小。",
 "category":"design","tags":["optimal-control","riccati","lti"],
 "applicability":{"preconditions":["R 对称正定","[[Q,N],[Nᵀ,R]] 对称半正定","(A,B) 可镇定","(Q−NR⁻¹Nᵀ, A−BR⁻¹Nᵀ) 在虚轴上无不可观模态"],"not_for":["离散系统（用 control.design.dlqr）","有约束的 MPC"],"siso_only":False},
 "provenance":{"algorithm":"Hamiltonian 矩阵（或扩展辛矩阵束）的有序实 Schur 分解，取稳定不变子空间",
   "references":[{"type":"paper","cite":"A. J. Laub, A Schur method for solving algebraic Riccati equations, IEEE TAC 24(6), 1979","doi":"10.1109/TAC.1979.1102178"},
     {"type":"paper","cite":"W. F. Arnold, A. J. Laub, Generalized eigenproblem algorithms and software for algebraic Riccati equations, Proc. IEEE 72(12), 1984","doi":"10.1109/PROC.1984.13083"},
     {"type":"textbook","cite":"B. D. O. Anderson, J. B. Moore, Optimal Control: Linear Quadratic Methods, 1990"}],
   "clean_room":{"declared":True,"sources":"仅公开文献与开源实现；MATLAB 仅用于生成对照数据"}},
 "backend":{"library":"SLICOT","routine":"SB02OD","version":"5.9","binding":"ffi","license":"BSD-3-Clause"},
 "determinism":"bitwise","tolerance_class":"T2",
 "input_schema":{"$ref":"./input.schema.json"},"output_schema":{"$ref":"./output.schema.json"},
 "credentials":"aeoi://operators/control.design.lqr/0.1.0/credentials",
 "errors":["E.VALIDATION.SHAPE","E.VALIDATION.NOT_SYMMETRIC","E.DOMAIN.R_NOT_POSITIVE_DEFINITE","E.DOMAIN.NOT_STABILIZABLE","E.DOMAIN.HAMILTONIAN_IMAG_AXIS_EIG","E.NUMERICAL.ILL_CONDITIONED"],
 "warnings":["W.NUMERICAL.LARGE_RESIDUAL","W.NUMERICAL.NEAR_UNCONTROLLABLE","W.VALIDATION.SYMMETRIZED"],
 "mcp":{"tool_name":"control_design_lqr","annotations":{"readOnlyHint":True,"idempotentHint":True,"openWorldHint":False}}})
lqr_input={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.design.lqr/0.1.0/input","type":"object","additionalProperties":False,
 "required":["A","B","Q","R"],"properties":{k:{"$ref":"aeoi://schemas/common/0.1/Matrix"} for k in ["A","B","Q","R","N"]}|{
  "symmetry_tolerance":{"type":"number","default":1e-12,"description":"相对容差；超出即 E.VALIDATION.NOT_SYMMETRIC，范围内对称化并给 W.VALIDATION.SYMMETRIZED"},
  "exec":{"$ref":"aeoi://schemas/common/0.1/ExecOptions"}},
 "x-aeoi-shape-constraints":["A: [n,n]","B: [n,m]","Q: [n,n]","R: [m,m]","N: [n,m]，缺省为 0"]}
lqr_output={"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"aeoi://schemas/control.design.lqr/0.1.0/output","type":"object","required":["K","S","closed_loop_poles"],
 "properties":{"K":{"$ref":"aeoi://schemas/common/0.1/Matrix","description":"[m,n]，u=-Kx"},"S":{"$ref":"aeoi://schemas/common/0.1/Matrix","description":"[n,n] 对称半正定"},
 "closed_loop_poles":{"$ref":"aeoi://schemas/common/0.1/Matrix","description":"复向量 [n]，按实部升序、虚部降序排列"}}}
lqr_req={"A":{"dtype":"f64","shape":[2,2],"data":[0,1,0,0]},"B":{"dtype":"f64","shape":[2,1],"data":[0,1]},"Q":{"dtype":"f64","shape":[2,2],"data":[1,0,0,1]},"R":{"dtype":"f64","shape":[1,1],"data":[1]},"exec":{"request_id":"req-lqr-001"}}
def flat(M): return [r(x) for row in M for x in row]
lqr_result={"K":{"dtype":"f64","shape":[1,2],"data":flat(L["K"])},"S":{"dtype":"f64","shape":[2,2],"data":flat(L["S"])},
 "closed_loop_poles":{"dtype":"c128","shape":[2],"data":[[r(a),r(b)] for a,b in sorted(L["E"],key=lambda e:(e[0],-e[1]))]}}
lqr_trust={"convergence":{"applicable":False,"note":"直接法（Schur），无迭代"},
 "residual":{"care_abs_fro":r(L["care_residual_fro"],3),"care_rel":r(L["rel_residual"],3),"definition":"‖AᵀS+SA−SBR⁻¹BᵀS+Q‖_F / ‖S‖_F"},
 "condition":{"cond_R":r(L["cond_R"],6),"cond_S":r(L["cond_S"],6),"ctrb_min_singular_value":r(L["ctrb_sv_min"],6),"rcond_care":None},
 "error_estimate":{"S_rel":1e-14,"basis":"rel_residual × cond 估计（非严格上界）"},
 "checks":{"S_symmetric":True,"S_psd":True,"closed_loop_stable":True},
 "accuracy_grade":"A","credentials":"aeoi://operators/control.design.lqr/0.1.0/credentials"}
lqr_ok=env("control.design.lqr","0.1.0",lqr_req,lqr_result,lqr_trust,tclass="T2",ms=0.05)
lqr_err_req={"A":{"dtype":"f64","shape":[2,2],"data":[1,0,0,-1]},"B":{"dtype":"f64","shape":[2,1],"data":[0,1]},"Q":{"dtype":"f64","shape":[2,2],"data":[1,0,0,1]},"R":{"dtype":"f64","shape":[1,1],"data":[1]},"exec":{"request_id":"req-lqr-002"}}
lqr_err={"spec":SPEC,"status":"error","operator":{"id":"control.design.lqr","version":"0.1.0","build":BUILD},"request_id":"req-lqr-002","result":None,
 "error":{"code":"E.DOMAIN.NOT_STABILIZABLE","category":"domain","retryable":False,
  "message":"(A,B) 不可镇定：特征值 λ=1（不稳定）对应的模态不受输入 B 影响，不存在使闭环稳定的状态反馈，CARE 无稳定化解。",
  "agent_hint":"这是模型结构问题，调 Q/R 无效。需要为该模态增加执行器（修改 B），或确认 A 是否录入有误。可先调用 control.lti.ctrb_decomp 查看不可控子空间。",
  "details":{"uncontrollable_eigenvalues":{"dtype":"c128","shape":[1],"data":[[1.0,0.0]]},"ctrb_singular_values":{"dtype":"f64","shape":[2],"data":[r(x) for x in V["lqr_err"]["ctrb_sv"]]},"pbh_test":"rank([A−λI, B]) = 1 < 2 at λ=1","path":"/B"},
  "remediation":[{"id":"inspect","kind":"call_operator","operator":"control.lti.ctrb_decomp","arguments_from":{"A":"/A","B":"/B"},"description":"做 Kalman 可控性分解以定位不可控模态","confidence":"high","requires_user_confirmation":False},
   {"id":"add_actuator","kind":"modify_input","description":"示例：为第一个状态增加一路输入（B 变为 2×2，R 同步扩为 2×2）","confidence":"low","requires_user_confirmation":True,
    "patch":[{"op":"replace","path":"/B","value":{"dtype":"f64","shape":[2,2],"data":[1,0,0,1]}},{"op":"replace","path":"/R","value":{"dtype":"f64","shape":[2,2],"data":[1,0,0,1]}}]},
   {"id":"ask","kind":"ask_user","description":"向用户确认 A 矩阵与执行器配置"}]},
 "warnings":[],"timing":{"wall_ms":0.02,"illustrative":True}}
# parity specs
parity={
"control.lti.bode":"""# Parity test spec — control.lti.bode 0.1.0
operator: control.lti.bode
reference:
  primary: {engine: MATLAB, version: "R2025b", toolbox: "Control System Toolbox", role: ground_truth}
  secondary: {engine: GNU Octave, version: "9.x", package: "control 4.x", role: dev_only}
generator: matlab/gen_golden_bode.m      # MATLAB 只运行公开 API，输出 golden 文件
golden_store: golden/control.lti.bode/0.1.0/*.json   # 内容寻址，文件名 = sha256
tolerance_class: T3
compare:
  magnitude: {quantity: abs_db, atol: 1.0e-9}          # dB 绝对误差
  phase:     {mode: modulo_360_then_abs, atol_deg: 1.0e-8, note: "相位分支约定可能与参考实现不同；先比 mod 360，再单独测 unwrap 连续性"}
  w:         {mode: exact}                              # 频点由请求显式给定，不使用参考实现的自动网格
case_families:
  - {name: siso_2nd_order_sweep, n: 200, params: "wn∈logU[1e-2,1e2], zeta∈U[0.01,2]"}
  - {name: mimo_ss_random_stable, n: 100, params: "n∈[2,30], m,p∈[1,4]"}
  - {name: discrete_ts, n: 50, params: "ts∈{1e-3,0.1,1}, w up to 0.99·π/ts"}
  - {name: near_imag_axis_poles, n: 30, expect: "W.NUMERICAL.NEAR_POLE"}
  - {name: integrators_and_delays_excluded, n: 0, note: "v0.1 不支持 InputDelay"}
  - {name: improper_reject, n: 10, expect: "E.DOMAIN.IMPROPER_SYSTEM"}
pass_rule: "所有 case 在容差内；预期报错的 case 报出相同 code"
""",
"control.lti.step":"""# Parity test spec — control.lti.step 0.1.0
operator: control.lti.step
reference:
  primary: {engine: MATLAB, version: "R2025b", toolbox: "Control System Toolbox", role: ground_truth}
generator: matlab/gen_golden_step.m   # 调用 step(sys, t) 时 t 必须显式给定均匀网格
golden_store: golden/control.lti.step/0.1.0/*.json
tolerance_class: T2
compare:
  y: {rtol: 1.0e-10, atol: 1.0e-12, scale: "max|y_ref|"}
  info.rise_time / settling_time / peak_time: {atol: "dt", note: "插值与阈值判定在网格上进行，允许 1 个网格步的差异"}
  info.overshoot: {atol_pct: 1.0e-6, note: "若峰值落在网格点上则应一致；否则放宽到 grid-limited"}
  info: {definition_check: "rise 10–90%，settling 2%，与参考实现 stepinfo 默认值一致；不同定义时不比较"}
case_families:
  - {name: siso_underdamped, n: 200}
  - {name: siso_overdamped_and_critically_damped, n: 100}
  - {name: mimo_ss_random_stable, n: 100}
  - {name: stiff_spread_poles, n: 50, params: "极点跨 6 个数量级"}
  - {name: unstable_system, n: 20, expect_warning: "W.DOMAIN.UNSTABLE_SYSTEM", note: "y 仍比较；info 期望为 null"}
  - {name: discrete_systems, n: 50}
excluded: ["auto 时间网格（参考实现的自动网格算法未公开且会变化，不作为 parity 对象）"]
""",
"control.design.lqr":"""# Parity test spec — control.design.lqr 0.1.0
operator: control.design.lqr
reference:
  primary: {engine: MATLAB, version: "R2025b", toolbox: "Control System Toolbox", role: ground_truth}
generator: matlab/gen_golden_lqr.m    # [K,S,P] = lqr(A,B,Q,R,N)
golden_store: golden/control.design.lqr/0.1.0/*.json
tolerance_class: T2
compare:
  S: {mode: normwise_rel, rtol: "1.0e-10 · max(1, cond_S/1e2)", note: "条件数缩放"}
  K: {mode: normwise_rel, rtol: "1.0e-10 · max(1, cond_S/1e2)"}
  closed_loop_poles: {mode: matched_set, matching: hungarian, rtol: 1.0e-9}
  invariants: ["CARE 相对残差 ≤ 1e-12", "S 对称半正定", "A−BK 稳定"]   # 不依赖参考实现，也必须通过
case_families:
  - {name: double_integrator_family, n: 20, note: "有解析解，可做 grade A 的第二参考"}
  - {name: random_stabilizable, n: 300, params: "n∈[2,50], m∈[1,5]"}
  - {name: with_cross_term_N, n: 100}
  - {name: ill_conditioned_R, n: 50, params: "cond(R)∈[1e6,1e12]", expect_warning: "W.NUMERICAL.ILL_CONDITIONED"}
  - {name: not_stabilizable, n: 20, expect: "E.DOMAIN.NOT_STABILIZABLE"}
  - {name: R_indefinite, n: 10, expect: "E.DOMAIN.R_NOT_POSITIVE_DEFINITE"}
"""}
E={"control.lti.bode":(bode_manifest,bode_input,bode_output,bode_req,bode_ok,bode_err_req,bode_err),
   "control.lti.step":(step_manifest,step_input,step_output,step_req,step_ok,step_err_req,step_err),
   "control.design.lqr":(lqr_manifest,lqr_input,lqr_output,lqr_req,lqr_ok,lqr_err_req,lqr_err)}
for op,(m,i,o,rq,ok,erq,er) in E.items():
    d=f"examples/{op}/"
    w(d+"manifest.json",m); w(d+"input.schema.json",i); w(d+"output.schema.json",o)
    w(d+"request.json",rq); w(d+"response.success.json",ok); w(d+"request.error.json",erq); w(d+"response.error.json",er)
    w(d+"parity.yaml",parity[op])
print("ok")
