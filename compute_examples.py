import json, numpy as np, control as ct, scipy, scipy.linalg as la, hashlib, platform
out={"_meta":{"generator":"python-control %s, scipy %s, numpy %s, python %s"%(ct.__version__,scipy.__version__,np.__version__,platform.python_version()),
 "note":"Computed on Grok Bot box with python-control/scipy. NOT MATLAB output."}}
# bode
G=ct.tf([4],[1,1.2,4])
w=np.array([0.1,0.5,1.0,2.0,5.0,10.0])
resp=ct.frequency_response(G,w)
mag=np.abs(resp.complex).ravel(); ph=np.unwrap(np.angle(resp.complex).ravel())
gm,pm,wcg,wcp=ct.margin(G)
out["bode"]={"w":w.tolist(),"mag_db":(20*np.log10(mag)).tolist(),"phase_deg":np.degrees(ph).tolist(),
 "gm":gm if np.isfinite(gm) else None,"pm_deg":pm,"wcp":wcp,"poles":[[p.real,p.imag] for p in G.poles()],
 "peak_db":float(20*np.log10(np.max(np.abs(ct.frequency_response(G,np.logspace(-1,1,2001)).complex))))}
# step
T=np.linspace(0,10,11)
tr=ct.step_response(G,T=np.linspace(0,10,1001))
info=ct.step_info(G,SettlingTimeThreshold=0.02,RiseTimeLimits=(0.1,0.9))
y=np.interp(T,tr.time,tr.outputs.ravel())
out["step"]={"t":T.tolist(),"y":y.tolist(),"info":{k:float(v) for k,v in info.items()},"dcgain":float(ct.dcgain(G))}
# lqr
A=np.array([[0.,1.],[0.,0.]]);B=np.array([[0.],[1.]]);Q=np.eye(2);R=np.array([[1.]])
K,S,E=ct.lqr(A,B,Q,R)
res=A.T@S+S@A-S@B@la.solve(R,B.T@S)+Q
Ham=np.block([[A,-B@la.solve(R,B.T)],[-Q,-A.T]])
out["lqr"]={"K":K.tolist(),"S":S.tolist(),"E":[[e.real,e.imag] for e in E],
 "care_residual_fro":float(np.linalg.norm(res,'fro')),"rel_residual":float(np.linalg.norm(res,'fro')/np.linalg.norm(S,'fro')),
 "cond_R":float(np.linalg.cond(R)),"cond_S":float(np.linalg.cond(S)),
 "ctrb_rank":int(np.linalg.matrix_rank(ct.ctrb(A,B))),
 "ctrb_sv_min":float(np.linalg.svd(ct.ctrb(A,B),compute_uv=False).min())}
# uncontrollable example for error
A2=np.array([[1.,0.],[0.,-1.]]);B2=np.array([[0.],[1.]])
sv=np.linalg.svd(ct.ctrb(A2,B2),compute_uv=False)
out["lqr_err"]={"ctrb_sv":sv.tolist(),"uncontrollable_eig":1.0}
print(json.dumps(out,indent=1))
json.dump(out,open("ai-operator-spec/computed_values.json","w"),indent=1)
