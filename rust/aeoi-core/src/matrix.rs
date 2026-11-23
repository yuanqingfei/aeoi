//! Array encodings (spec ch.3.4–3.6): inline JSON dense, base64 dense, file reference, CSR/CSC sparse.
//! Canonical memory order is row-major (C order). All decoders produce row-major element vectors.
use crate::error::{ask_user, modify_input, OpError, Warning};
use crate::host::HostCtx;
use crate::jcs::sha256_hex;
use crate::num;
use base64::Engine;
use nalgebra::{Complex, DMatrix};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    F64,
    C128,
    I64,
    Bool,
}

impl DType {
    pub fn parse(s: &str) -> Option<DType> {
        match s {
            "f64" => Some(DType::F64),
            "c128" => Some(DType::C128),
            "i64" => Some(DType::I64),
            "bool" => Some(DType::Bool),
            _ => None,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            DType::F64 => "f64",
            DType::C128 => "c128",
            DType::I64 => "i64",
            DType::Bool => "bool",
        }
    }
    pub fn itemsize(&self) -> usize {
        match self {
            DType::F64 | DType::I64 => 8,
            DType::C128 => 16,
            DType::Bool => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Elems {
    F64(Vec<f64>),
    C128(Vec<Complex<f64>>),
    I64(Vec<i64>),
    Bool(Vec<bool>),
}

impl Elems {
    pub fn len(&self) -> usize {
        match self {
            Elems::F64(v) => v.len(),
            Elems::C128(v) => v.len(),
            Elems::I64(v) => v.len(),
            Elems::Bool(v) => v.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Little-endian raw buffer (the canonical byte layout for b64/ref and for content digests).
    pub fn to_le_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Elems::F64(v) => v.iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes())),
            Elems::C128(v) => v.iter().for_each(|x| {
                out.extend_from_slice(&x.re.to_le_bytes());
                out.extend_from_slice(&x.im.to_le_bytes())
            }),
            Elems::I64(v) => v.iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes())),
            Elems::Bool(v) => v.iter().for_each(|x| out.push(*x as u8)),
        }
        out
    }
    fn from_le_bytes(dtype: DType, b: &[u8]) -> Elems {
        let f = |c: &[u8]| f64::from_le_bytes(c.try_into().unwrap());
        match dtype {
            DType::F64 => Elems::F64(b.chunks_exact(8).map(f).collect()),
            DType::C128 => Elems::C128(b.chunks_exact(16).map(|c| Complex::new(f(&c[..8]), f(&c[8..]))).collect()),
            DType::I64 => Elems::I64(b.chunks_exact(8).map(|c| i64::from_le_bytes(c.try_into().unwrap())).collect()),
            DType::Bool => Elems::Bool(b.iter().map(|x| *x != 0).collect()),
        }
    }
    fn has_nonfinite(&self) -> Option<usize> {
        match self {
            Elems::F64(v) => v.iter().position(|x| !x.is_finite()),
            Elems::C128(v) => v.iter().position(|x| !x.re.is_finite() || !x.im.is_finite()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Dense {
    pub dtype: DType,
    pub shape: Vec<usize>,
    pub data: Elems, // row-major
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseFormat {
    Csr,
    Csc,
}

#[derive(Debug, Clone)]
pub struct Sparse {
    pub dtype: DType,
    pub m: usize,
    pub n: usize,
    pub format: SparseFormat,
    pub indptr: Vec<usize>,
    pub indices: Vec<usize>,
    pub data: Elems,
    pub sum_duplicates: bool,
}

#[derive(Debug, Clone)]
pub enum Array {
    Dense(Dense),
    Sparse(Sparse),
}

/// Which encoding a value used (for hashing and diagnostics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Json,
    B64,
    Ref,
    Sparse,
}

pub fn is_array_object(v: &Value) -> bool {
    match v.as_object() {
        Some(o) => o.contains_key("dtype") && o.contains_key("shape") && (o.contains_key("data") || o.contains_key("data_b64") || o.contains_key("uri")),
        None => false,
    }
}

pub fn encoding_of(v: &Value) -> Option<Encoding> {
    let o = v.as_object()?;
    if !(o.contains_key("dtype") && o.contains_key("shape")) {
        return None;
    }
    if o.contains_key("format") && o.contains_key("indptr") {
        Some(Encoding::Sparse)
    } else if o.contains_key("data_b64") {
        Some(Encoding::B64)
    } else if o.contains_key("uri") {
        Some(Encoding::Ref)
    } else if o.contains_key("data") {
        Some(Encoding::Json)
    } else {
        None
    }
}

fn verr(code: &str, path: &str, msg: impl Into<String>, hint: &str) -> OpError {
    OpError::new(code, msg, hint).path(path)
}

fn shape_of(o: &Map<String, Value>, path: &str) -> Result<Vec<usize>, OpError> {
    let s = o.get("shape").and_then(|s| s.as_array()).ok_or_else(|| verr("E.VALIDATION.SHAPE", path, "缺少 shape。", "按 Matrix schema 提供 shape。"))?;
    s.iter()
        .map(|x| x.as_u64().map(|u| u as usize).ok_or_else(|| verr("E.VALIDATION.SHAPE", path, "shape 必须为非负整数数组。", "修正 shape。")))
        .collect()
}

fn prod(shape: &[usize], path: &str) -> Result<usize, OpError> {
    shape.iter().try_fold(1usize, |a, &b| a.checked_mul(b)).ok_or_else(|| verr("E.RESOURCE.INPUT_TOO_LARGE", path, "shape 乘积溢出。", "检查 shape。"))
}

fn dtype_of(o: &Map<String, Value>, path: &str) -> Result<DType, OpError> {
    o.get("dtype").and_then(|d| d.as_str()).and_then(DType::parse).ok_or_else(|| verr("E.VALIDATION.DTYPE", path, "dtype 必须为 f64/c128/i64/bool。", "修正 dtype。"))
}

fn parse_scalar_f64(x: &Value) -> Option<f64> {
    match x {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => match s.as_str() {
            "NaN" => Some(f64::NAN),
            "Inf" => Some(f64::INFINITY),
            "-Inf" => Some(f64::NEG_INFINITY),
            _ => None,
        },
        _ => None,
    }
}

fn parse_json_elems(dtype: DType, data: &[Value], path: &str) -> Result<Elems, OpError> {
    let bad = |i: usize| verr("E.VALIDATION.SCHEMA", &format!("{path}/{i}"), format!("第 {i} 个元素与 dtype 不符。"), "按 dtype 修正元素编码（复数用 [re, im]）。");
    Ok(match dtype {
        DType::F64 => Elems::F64(data.iter().enumerate().map(|(i, x)| parse_scalar_f64(x).ok_or_else(|| bad(i))).collect::<Result<_, _>>()?),
        DType::C128 => Elems::C128(
            data.iter()
                .enumerate()
                .map(|(i, x)| {
                    let a = x.as_array().filter(|a| a.len() == 2).ok_or_else(|| bad(i))?;
                    Ok(Complex::new(parse_scalar_f64(&a[0]).ok_or_else(|| bad(i))?, parse_scalar_f64(&a[1]).ok_or_else(|| bad(i))?))
                })
                .collect::<Result<_, OpError>>()?,
        ),
        DType::I64 => Elems::I64(data.iter().enumerate().map(|(i, x)| x.as_i64().ok_or_else(|| bad(i))).collect::<Result<_, _>>()?),
        DType::Bool => Elems::Bool(data.iter().enumerate().map(|(i, x)| x.as_bool().ok_or_else(|| bad(i))).collect::<Result<_, _>>()?),
    })
}

fn check_digest(bytes: &[u8], o: &Map<String, Value>, path: &str) -> Result<String, OpError> {
    let want = o.get("sha256").and_then(|s| s.as_str()).unwrap_or("");
    let got = sha256_hex(bytes);
    if got != want {
        return Err(verr("E.VALIDATION.DIGEST_MISMATCH", path, "解码后缓冲区的 SHA-256 与声明的 sha256 不一致（可能传输损坏、字节序或展平顺序错误）。", "重新对 row-major、小端原始缓冲区计算 sha256 后重发。")
            .detail("declared", Value::from(want))
            .detail("computed", Value::from(got)));
    }
    Ok(got)
}

/// Parse any array encoding. `allow_nonfinite` = whether NaN/Inf elements are accepted (default: reject, spec R-2).
pub fn parse_array(v: &Value, path: &str, host: &HostCtx, allow_nonfinite: bool) -> Result<Array, OpError> {
    let o = v.as_object().ok_or_else(|| verr("E.VALIDATION.SCHEMA", path, "期望一个数组对象。", "按 Matrix schema 提供。"))?;
    let dtype = dtype_of(o, path)?;
    let shape = shape_of(o, path)?;
    let enc = encoding_of(v).ok_or_else(|| verr("E.VALIDATION.SCHEMA", path, "无法识别数组编码（需要 data / data_b64 / uri / format 之一）。", "按 Matrix schema 提供。"))?;
    let arr = match enc {
        Encoding::Json => {
            let n = prod(&shape, path)?;
            if n > host.max_inline_elements {
                return Err(verr("E.RESOURCE.INPUT_TOO_LARGE", path, format!("内联 JSON 数组有 {n} 个元素，超过上限 {}。", host.max_inline_elements), "改用 data_b64 或 uri 引用（见规范 3.6）。"));
            }
            let data = o["data"].as_array().unwrap();
            if data.len() != n {
                return Err(verr("E.VALIDATION.SHAPE", path, format!("data 长度 {} 与 shape {:?} 的元素数 {} 不一致。", data.len(), shape, n), "检查 shape 与 data；注意 data 必须按 row-major 展平。")
                    .detail("data_length", json!(data.len()))
                    .detail("expected", json!(n)));
            }
            Array::Dense(Dense { dtype, shape, data: parse_json_elems(dtype, data, &format!("{path}/data"))? })
        }
        Encoding::B64 => {
            let n = prod(&shape, path)?;
            let need = n.checked_mul(dtype.itemsize()).ok_or_else(|| verr("E.RESOURCE.INPUT_TOO_LARGE", path, "字节数溢出。", "检查 shape。"))?;
            if need > host.max_decoded_bytes {
                return Err(verr("E.RESOURCE.INPUT_TOO_LARGE", path, format!("解码后需 {need} 字节，超过上限 {}。", host.max_decoded_bytes), "改用 uri 引用或拆分问题。"));
            }
            let s = o["data_b64"].as_str().unwrap_or("");
            // size check before decoding (base64 expands 3 -> 4)
            if s.len() / 4 * 3 > need + 3 {
                return Err(verr("E.VALIDATION.SHAPE", path, "data_b64 长度与 shape/dtype 不符。", "检查 shape 与编码。"));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(s)
                .map_err(|e| verr("E.VALIDATION.SCHEMA", path, format!("data_b64 不是合法的标准 base64：{e}"), "使用 RFC 4648 标准字母表并带 padding。"))?;
            if bytes.len() != need {
                return Err(verr("E.VALIDATION.SHAPE", path, format!("解码后 {} 字节，期望 {need} 字节。", bytes.len()), "检查 shape 与 dtype。"));
            }
            check_digest(&bytes, o, path)?;
            Array::Dense(Dense { dtype, shape, data: Elems::from_le_bytes(dtype, &bytes) })
        }
        Encoding::Ref => {
            let n = prod(&shape, path)?;
            let need = n.checked_mul(dtype.itemsize()).unwrap_or(usize::MAX);
            let declared = o.get("byte_length").and_then(|x| x.as_u64()).unwrap_or(u64::MAX) as usize;
            if declared != need {
                return Err(verr("E.VALIDATION.SHAPE", path, format!("byte_length {declared} 与 shape×itemsize = {need} 不符。"), "修正 byte_length。"));
            }
            if need > host.max_decoded_bytes {
                return Err(verr("E.RESOURCE.INPUT_TOO_LARGE", path, format!("引用数据 {need} 字节，超过上限 {}。", host.max_decoded_bytes), "拆分问题或由运维调高上限。"));
            }
            let uri = o["uri"].as_str().unwrap_or("");
            let bytes = resolve_file_ref(uri, need, host, path)?;
            check_digest(&bytes, o, path)?;
            Array::Dense(Dense { dtype, shape, data: Elems::from_le_bytes(dtype, &bytes) })
        }
        Encoding::Sparse => Array::Sparse(parse_sparse(o, dtype, &shape, path, host)?),
    };
    if !allow_nonfinite {
        let (elems, p) = match &arr {
            Array::Dense(d) => (&d.data, path.to_string()),
            Array::Sparse(s) => (&s.data, format!("{path}/data")),
        };
        if let Some(i) = elems.has_nonfinite() {
            return Err(verr("E.VALIDATION.NON_FINITE", &format!("{p}/{i}"), "输入包含 NaN 或 Inf，该字段不接受非有限值。", "检查上游计算，替换非有限元素后重试。"));
        }
    }
    Ok(arr)
}

fn resolve_file_ref(uri: &str, need: usize, host: &HostCtx, path: &str) -> Result<Vec<u8>, OpError> {
    let Some(rest) = uri.strip_prefix("file://") else {
        return Err(verr("E.VALIDATION.REF_UNSUPPORTED", path, format!("不支持的 URI scheme：{uri}。v0.2 服务端只解析 data root 下的 file:// 引用。"), "改用 data_b64，或把文件放到服务端配置的 data root 下并用 file:// 引用。")
            .remedy(ask_user("ask", "确认服务端的 data root 配置")));
    };
    let Some(root) = &host.data_root else {
        return Err(verr("E.VALIDATION.REF_FORBIDDEN", path, "服务端未配置 data root，拒绝解析 file:// 引用。", "改用 data_b64 内联二进制。"));
    };
    let root = std::fs::canonicalize(root).map_err(|_| verr("E.VALIDATION.REF_FORBIDDEN", path, "data root 不可用。", "改用 data_b64。"))?;
    let p = std::fs::canonicalize(rest).map_err(|_| verr("E.VALIDATION.REF_FORBIDDEN", path, "引用的文件不存在或不可访问。", "检查 uri。"))?;
    if !p.starts_with(&root) {
        return Err(verr("E.VALIDATION.REF_FORBIDDEN", path, "引用路径（解析符号链接后）不在 data root 之内。", "只能引用 data root 内的文件。"));
    }
    let meta = std::fs::metadata(&p).map_err(|_| verr("E.VALIDATION.REF_FORBIDDEN", path, "无法读取文件元数据。", "检查 uri。"))?;
    if !meta.is_file() || meta.len() as usize != need {
        return Err(verr("E.VALIDATION.SHAPE", path, format!("文件大小 {} 字节与 byte_length {need} 不符。", meta.len()), "检查文件与 byte_length。"));
    }
    std::fs::read(&p).map_err(|_| verr("E.VALIDATION.REF_FORBIDDEN", path, "读取文件失败。", "检查文件权限。"))
}

fn parse_sparse(o: &Map<String, Value>, dtype: DType, shape: &[usize], path: &str, host: &HostCtx) -> Result<Sparse, OpError> {
    let serr = |msg: String, p: &str| verr("E.VALIDATION.SPARSE_STRUCTURE", p, msg, "按 SparseMatrix 规则修正：indptr 长度 = 主维+1、单调不减、首 0 尾 nnz；段内 indices 严格递增且小于次维。");
    if shape.len() != 2 {
        return Err(serr("稀疏矩阵 shape 必须为 [m, n]。".into(), path));
    }
    let (m, n) = (shape[0], shape[1]);
    let format = match o.get("format").and_then(|f| f.as_str()) {
        Some("csr") => SparseFormat::Csr,
        Some("csc") => SparseFormat::Csc,
        _ => return Err(serr("format 必须为 csr 或 csc。".into(), path)),
    };
    let sum_duplicates = matches!(o.get("duplicates").and_then(|d| d.as_str()), Some("sum"));
    let to_usize = |key: &str| -> Result<Vec<usize>, OpError> {
        o.get(key)
            .and_then(|x| x.as_array())
            .ok_or_else(|| serr(format!("缺少 {key}。"), path))?
            .iter()
            .map(|x| x.as_u64().map(|u| u as usize).ok_or_else(|| serr(format!("{key} 必须为非负整数。"), &format!("{path}/{key}"))))
            .collect()
    };
    let indptr = to_usize("indptr")?;
    let indices = to_usize("indices")?;
    let data_v = o.get("data").and_then(|d| d.as_array()).ok_or_else(|| serr("缺少 data。".into(), path))?;
    if indices.len() > host.max_inline_elements {
        return Err(verr("E.RESOURCE.INPUT_TOO_LARGE", path, "nnz 超过内联上限。", "拆分问题。"));
    }
    let (major, minor) = if format == SparseFormat::Csr { (m, n) } else { (n, m) };
    if indptr.len() != major + 1 {
        return Err(serr(format!("indptr 长度 {} ≠ {}（主维 + 1）。", indptr.len(), major + 1), &format!("{path}/indptr")));
    }
    if indptr[0] != 0 {
        return Err(serr("indptr[0] 必须为 0。".into(), &format!("{path}/indptr/0")));
    }
    let nnz = indices.len();
    if *indptr.last().unwrap() != nnz || data_v.len() != nnz {
        return Err(serr(format!("indptr 末项 {}、indices 长度 {nnz}、data 长度 {} 必须相等。", indptr.last().unwrap(), data_v.len()), path));
    }
    for r in 0..major {
        if indptr[r + 1] < indptr[r] {
            return Err(serr(format!("indptr 在位置 {} 处递减。", r + 1), &format!("{path}/indptr/{}", r + 1)));
        }
        let seg = &indices[indptr[r]..indptr[r + 1]];
        for (k, &c) in seg.iter().enumerate() {
            if c >= minor {
                return Err(verr("E.VALIDATION.SPARSE_STRUCTURE", &format!("{path}/indices/{}", indptr[r] + k), format!("索引 {c} 越界（次维 = {minor}）。"), "索引为 0 基且必须小于次维。"));
            }
            if k > 0 {
                let prev = seg[k - 1];
                if c < prev {
                    return Err(verr("E.VALIDATION.SPARSE_STRUCTURE", &format!("{path}/indices/{}", indptr[r] + k), "段内索引未排序。", "对每段 indices（及对应 data）升序排序后重发。"));
                }
                if c == prev && !sum_duplicates {
                    return Err(verr("E.VALIDATION.SPARSE_STRUCTURE", &format!("{path}/indices/{}", indptr[r] + k), format!("重复索引 {c}。"), "合并重复项，或设置 duplicates: \"sum\" 明确要求求和。")
                        .remedy(modify_input("sum_dups", "声明重复项求和（scipy 约定）", "medium", true, json!([{"op": "add", "path": format!("{path}/duplicates"), "value": "sum"}]))));
                }
            }
        }
    }
    let data = parse_json_elems(dtype, data_v, &format!("{path}/data"))?;
    Ok(Sparse { dtype, m, n, format, indptr, indices, data, sum_duplicates })
}

impl Sparse {
    /// Canonical CSR (duplicates summed). Used for content digests.
    pub fn to_canonical_csr(&self) -> (Vec<usize>, Vec<usize>, Vec<f64>) {
        // f64-only canonicalization is enough for hashing real operators; other dtypes are hashed via LE bytes of densified data.
        let vals: Vec<f64> = match &self.data {
            Elems::F64(v) => v.clone(),
            Elems::I64(v) => v.iter().map(|x| *x as f64).collect(),
            Elems::Bool(v) => v.iter().map(|x| *x as u8 as f64).collect(),
            Elems::C128(v) => v.iter().map(|x| x.re).collect(),
        };
        let mut trip: Vec<(usize, usize, f64)> = Vec::with_capacity(self.indices.len());
        let major = self.indptr.len() - 1;
        for r in 0..major {
            for k in self.indptr[r]..self.indptr[r + 1] {
                let (i, j) = if self.format == SparseFormat::Csr { (r, self.indices[k]) } else { (self.indices[k], r) };
                trip.push((i, j, vals[k]));
            }
        }
        trip.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        let mut counts = vec![0usize; self.m];
        let mut indices = Vec::new();
        let mut data: Vec<f64> = Vec::new();
        let mut prev: Option<(usize, usize)> = None;
        for (i, j, v) in trip {
            if prev == Some((i, j)) {
                *data.last_mut().unwrap() += v;
                continue;
            }
            prev = Some((i, j));
            indices.push(j);
            data.push(v);
            counts[i] += 1;
        }
        let mut indptr = vec![0usize; self.m + 1];
        for r in 0..self.m {
            indptr[r + 1] = indptr[r] + counts[r];
        }
        (indptr, indices, data)
    }
    pub fn digest(&self) -> String {
        let (ip, ix, d) = self.to_canonical_csr();
        let mut b = Vec::new();
        b.extend_from_slice(b"csr");
        b.extend_from_slice(&(self.m as u64).to_le_bytes());
        b.extend_from_slice(&(self.n as u64).to_le_bytes());
        ip.iter().for_each(|x| b.extend_from_slice(&(*x as u64).to_le_bytes()));
        ix.iter().for_each(|x| b.extend_from_slice(&(*x as u64).to_le_bytes()));
        d.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
        sha256_hex(&b)
    }
    pub fn to_dense_f64(&self) -> DMatrix<f64> {
        let mut a = DMatrix::<f64>::zeros(self.m, self.n);
        let (ip, ix, d) = self.to_canonical_csr();
        for i in 0..self.m {
            for k in ip[i]..ip[i + 1] {
                a[(i, ix[k])] += d[k];
            }
        }
        a
    }
}

/// Real 2-D matrix from any encoding. Sparse inputs are densified (caller decides policy & warning).
pub fn to_real_matrix(arr: &Array, path: &str, warnings: &mut Vec<Warning>, max_dense: usize) -> Result<DMatrix<f64>, OpError> {
    match arr {
        Array::Dense(d) => {
            let (r, c) = match d.shape.as_slice() {
                [r, c] => (*r, *c),
                [n] => (*n, 1),
                _ => return Err(verr("E.VALIDATION.SHAPE", path, format!("期望二维矩阵，得到 shape {:?}。", d.shape), "提供 [rows, cols] 形状。")),
            };
            let v: Vec<f64> = match &d.data {
                Elems::F64(v) => v.clone(),
                Elems::I64(v) => v.iter().map(|x| *x as f64).collect(),
                _ => return Err(verr("E.VALIDATION.DTYPE", path, format!("该字段只接受实数（f64/i64），得到 {}。", d.dtype.name()), "传入实数矩阵。")),
            };
            Ok(DMatrix::from_row_slice(r, c, &v))
        }
        Array::Sparse(s) => {
            if !matches!(s.dtype, DType::F64 | DType::I64) {
                return Err(verr("E.VALIDATION.DTYPE", path, "该字段只接受实数稀疏矩阵。", "传入 f64 稀疏矩阵。"));
            }
            if s.m.saturating_mul(s.n) > max_dense {
                return Err(verr("E.RESOURCE.INPUT_TOO_LARGE", path, format!("稀疏矩阵稠密化后 {}×{} 超过该算子上限 {max_dense} 个元素。", s.m, s.n), "该算子以稠密方式计算；请降阶或使用支持稀疏的算子。"));
            }
            warnings.push(Warning::new("W.PERFORMANCE.SPARSE_DENSIFIED", format!("稀疏输入已稠密化为 {}×{} 后计算（manifest sparse_support = densify）。", s.m, s.n)).path(path));
            Ok(s.to_dense_f64())
        }
    }
}

/// Encoding-independent content digest of a parsed array (spec P-2):
/// dense = sha256 of the row-major little-endian raw buffer (identical to the sha256 field of b64/ref encodings).
pub fn digest(arr: &Array) -> String {
    match arr {
        Array::Dense(d) => sha256_hex(&d.data.to_le_bytes()),
        Array::Sparse(s) => s.digest(),
    }
}

/// Canonical form used for input/output hashing: every array object is replaced by
/// {"$array": {"dtype", "shape", "sparse": bool, "sha256"}}. Unparseable objects are left as-is.
pub fn canonicalize_for_hash(v: &Value, host: &HostCtx) -> Value {
    match v {
        Value::Object(o) => {
            if let Some(enc) = encoding_of(v) {
                let digest = match enc {
                    Encoding::B64 | Encoding::Ref => o.get("sha256").cloned(),
                    _ => parse_array(v, "", host, true).ok().map(|a| Value::from(digest(&a))),
                };
                if let Some(d) = digest {
                    return json!({"$array": {"dtype": o["dtype"], "shape": o["shape"], "sparse": enc == Encoding::Sparse, "sha256": d}});
                }
            }
            Value::Object(o.iter().map(|(k, x)| (k.clone(), canonicalize_for_hash(x, host))).collect())
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| canonicalize_for_hash(x, host)).collect()),
        _ => v.clone(),
    }
}

// ---------- output encoders ----------

pub fn dense_real(m: &DMatrix<f64>) -> Value {
    let mut data = Vec::with_capacity(m.len());
    for i in 0..m.nrows() {
        for j in 0..m.ncols() {
            data.push(num(m[(i, j)]));
        }
    }
    json!({"dtype": "f64", "shape": [m.nrows(), m.ncols()], "data": data})
}

pub fn dense_real_nd(shape: &[usize], data: &[f64]) -> Value {
    json!({"dtype": "f64", "shape": shape, "data": data.iter().map(|x| num(*x)).collect::<Vec<_>>()})
}

pub fn complex_vector(v: &[Complex<f64>]) -> Value {
    json!({"dtype": "c128", "shape": [v.len()], "data": v.iter().map(|c| json!([num(c.re), num(c.im)])).collect::<Vec<_>>()})
}

/// Re-encode every inline dense array in `v` as DenseMatrixB64 (exec.output_encoding = "b64").
pub fn encode_b64_inplace(v: &mut Value, host: &HostCtx) {
    if encoding_of(v) == Some(Encoding::Json) {
        if let Ok(Array::Dense(d)) = parse_array(v, "", host, true) {
            let bytes = d.data.to_le_bytes();
            *v = json!({"dtype": d.dtype.name(), "shape": d.shape, "order": "C", "byte_order": "little",
                        "data_b64": base64::engine::general_purpose::STANDARD.encode(&bytes), "sha256": sha256_hex(&bytes)});
            return;
        }
    }
    match v {
        Value::Object(o) => o.values_mut().for_each(|x| encode_b64_inplace(x, host)),
        Value::Array(a) => a.iter_mut().for_each(|x| encode_b64_inplace(x, host)),
        _ => {}
    }
}

/// Inverse of `encode_b64_inplace` for comparisons (receipt::verify): every DenseMatrixB64 / ref array is
/// decoded back to the inline JSON form; anything unparseable is left as-is.
pub fn decode_b64_for_hash(v: &Value, host: &HostCtx) -> Value {
    if matches!(encoding_of(v), Some(Encoding::B64) | Some(Encoding::Ref)) {
        if let Ok(Array::Dense(d)) = parse_array(v, "", host, true) {
            let data: Vec<Value> = match &d.data {
                Elems::F64(x) => x.iter().map(|a| num(*a)).collect(),
                Elems::C128(x) => x.iter().map(|c| json!([num(c.re), num(c.im)])).collect(),
                Elems::I64(x) => x.iter().map(|a| Value::from(*a)).collect(),
                Elems::Bool(x) => x.iter().map(|a| Value::from(*a)).collect(),
            };
            return json!({"dtype": d.dtype.name(), "shape": d.shape, "data": data});
        }
    }
    match v {
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), decode_b64_for_hash(x, host))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(|x| decode_b64_for_hash(x, host)).collect()),
        _ => v.clone(),
    }
}

/// Helper for tests/clients: build a DenseMatrixB64 object from a row-major f64 slice.
pub fn b64_f64(shape: &[usize], data: &[f64]) -> Value {
    let bytes = Elems::F64(data.to_vec()).to_le_bytes();
    json!({"dtype": "f64", "shape": shape, "order": "C", "byte_order": "little",
           "data_b64": base64::engine::general_purpose::STANDARD.encode(&bytes), "sha256": sha256_hex(&bytes)})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h() -> HostCtx {
        HostCtx::default()
    }
    #[test]
    fn row_major_json() {
        let v = json!({"dtype":"f64","shape":[2,3],"data":[1,2,3,4,5,6]});
        let a = parse_array(&v, "/A", &h(), false).unwrap();
        let m = to_real_matrix(&a, "/A", &mut vec![], 100).unwrap();
        assert_eq!(m[(0, 2)], 3.0);
        assert_eq!(m[(1, 0)], 4.0);
    }
    #[test]
    fn b64_roundtrip_and_digest_equal() {
        let data = [1.0, -2.5, 3.25, 4.0];
        let j = json!({"dtype":"f64","shape":[2,2],"data":data});
        let b = b64_f64(&[2, 2], &data);
        let aj = parse_array(&j, "", &h(), false).unwrap();
        let ab = parse_array(&b, "", &h(), false).unwrap();
        assert_eq!(digest(&aj), digest(&ab));
        assert_eq!(canonicalize_for_hash(&j, &h()), canonicalize_for_hash(&b, &h()));
    }
    #[test]
    fn b64_digest_mismatch() {
        let mut b = b64_f64(&[2], &[1.0, 2.0]);
        b["sha256"] = json!(format!("sha256:{}", "0".repeat(64)));
        assert_eq!(parse_array(&b, "/x", &h(), false).unwrap_err().code, "E.VALIDATION.DIGEST_MISMATCH");
    }
    #[test]
    fn sparse_csr_csc_equal() {
        // [[1,0,2],[0,3,0]]
        let csr = json!({"dtype":"f64","shape":[2,3],"format":"csr","indptr":[0,2,3],"indices":[0,2,1],"data":[1,2,3]});
        let csc = json!({"dtype":"f64","shape":[2,3],"format":"csc","indptr":[0,1,2,3],"indices":[0,1,0],"data":[1,3,2]});
        let a = parse_array(&csr, "", &h(), false).unwrap();
        let b = parse_array(&csc, "", &h(), false).unwrap();
        assert_eq!(digest(&a), digest(&b));
        let m = to_real_matrix(&b, "", &mut vec![], 100).unwrap();
        assert_eq!(m, DMatrix::from_row_slice(2, 3, &[1., 0., 2., 0., 3., 0.]));
    }
    #[test]
    fn sparse_rules() {
        let dup = json!({"dtype":"f64","shape":[1,3],"format":"csr","indptr":[0,2],"indices":[1,1],"data":[1,2]});
        assert_eq!(parse_array(&dup, "/S", &h(), false).unwrap_err().code, "E.VALIDATION.SPARSE_STRUCTURE");
        let mut sum = dup.clone();
        sum["duplicates"] = json!("sum");
        let m = to_real_matrix(&parse_array(&sum, "", &h(), false).unwrap(), "", &mut vec![], 10).unwrap();
        assert_eq!(m[(0, 1)], 3.0);
        let unsorted = json!({"dtype":"f64","shape":[1,3],"format":"csr","indptr":[0,2],"indices":[2,0],"data":[1,2]});
        assert!(parse_array(&unsorted, "", &h(), false).is_err());
        let oob = json!({"dtype":"f64","shape":[1,3],"format":"csr","indptr":[0,1],"indices":[3],"data":[1]});
        assert!(parse_array(&oob, "", &h(), false).is_err());
        let badptr = json!({"dtype":"f64","shape":[2,3],"format":"csr","indptr":[0,1],"indices":[0],"data":[1]});
        assert!(parse_array(&badptr, "", &h(), false).is_err());
    }
    #[test]
    fn nonfinite_rejected_by_default() {
        let v = json!({"dtype":"f64","shape":[2],"data":[1,"NaN"]});
        assert_eq!(parse_array(&v, "/x", &h(), false).unwrap_err().code, "E.VALIDATION.NON_FINITE");
        assert!(parse_array(&v, "/x", &h(), true).is_ok());
    }
    #[test]
    fn file_ref_security() {
        let dir = std::env::temp_dir().join(format!("aeoi-ref-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let data = [1.0f64, 2.0, 3.0, 4.0];
        let bytes = Elems::F64(data.to_vec()).to_le_bytes();
        let f = dir.join("a.bin");
        std::fs::write(&f, &bytes).unwrap();
        let r = json!({"dtype":"f64","shape":[2,2],"order":"C","byte_order":"little","encoding":"raw",
                       "uri": format!("file://{}", f.display()), "sha256": sha256_hex(&bytes), "byte_length": 32});
        // no data root => forbidden
        assert_eq!(parse_array(&r, "/A", &h(), false).unwrap_err().code, "E.VALIDATION.REF_FORBIDDEN");
        let host = HostCtx { data_root: Some(dir.clone()), ..HostCtx::default() };
        assert!(parse_array(&r, "/A", &host, false).is_ok());
        // outside root
        let host2 = HostCtx { data_root: Some(dir.join("sub")), ..HostCtx::default() };
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        assert_eq!(parse_array(&r, "/A", &host2, false).unwrap_err().code, "E.VALIDATION.REF_FORBIDDEN");
        let mut http = r.clone();
        http["uri"] = json!("https://example.com/a.bin");
        assert_eq!(parse_array(&http, "/A", &host, false).unwrap_err().code, "E.VALIDATION.REF_UNSUPPORTED");
        std::fs::remove_dir_all(&dir).ok();
    }
}
