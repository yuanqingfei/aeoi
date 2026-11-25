//! The AEOI expression language (spec 3.15, v0.4.8): how a model such as an ODE right-hand side travels as
//! data. Safe (nothing is executed: a fixed grammar and a fixed function table), deterministic (the value is
//! the AST evaluated left to right in IEEE-754 binary64, no reassociation; elementary functions from the
//! platform libm, which is part of the platform for `platform_bitwise`, P-1).
//!
//! ```text
//! expr    = sum
//! sum     = product { ("+" | "-") product }          left-associative
//! product = unary { ("*" | "/") unary }              left-associative
//! unary   = ("+" | "-") unary | power
//! power   = primary [ "^" unary ]                    right-associative: 2^3^2 = 2^(3^2); -x^2 = -(x^2)
//! primary = number | name | name "(" expr { "," expr } ")" | "(" expr ")"
//! number  = digits [ "." digits ] [ ("e" | "E") [ "+" | "-" ] digits ]  |  "." digits [ exponent ]
//! name    = ( letter | "_" ) { letter | digit | "_" }    ASCII only
//! ```
use aeoi_core::error::OpError;
use serde_json::{json, Value};

pub const MAX_LEN: usize = 10_000;
pub const MAX_NODES: usize = 10_000;
pub const MAX_DEPTH: usize = 200;

/// Functions and their arity. Order is part of nothing; names are reserved.
pub const FUNCS: &[(&str, usize)] = &[
    ("sin", 1), ("cos", 1), ("tan", 1), ("asin", 1), ("acos", 1), ("atan", 1), ("atan2", 2),
    ("sinh", 1), ("cosh", 1), ("tanh", 1), ("exp", 1), ("log", 1), ("log10", 1), ("sqrt", 1),
    ("abs", 1), ("min", 2), ("max", 2), ("hypot", 2), ("sign", 1),
];
/// Reserved names that are not functions.
pub const CONSTANTS: &[(&str, f64)] = &[("pi", std::f64::consts::PI)];

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Num(f64),
    Var(usize),
    Neg(Box<Node>),
    Add(Box<Node>, Box<Node>),
    Sub(Box<Node>, Box<Node>),
    Mul(Box<Node>, Box<Node>),
    Div(Box<Node>, Box<Node>),
    Pow(Box<Node>, Box<Node>),
    Call(&'static str, Vec<Node>),
}

/// A static error: where (0-based character offset into the expression) and why.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprError {
    pub position: usize,
    pub kind: &'static str, // syntax | unknown_name | arity | too_long | too_deep | too_many_nodes | non_ascii | not_finite
    pub message: String,
}

impl ExprError {
    pub fn details(&self) -> Value {
        json!({"position": self.position, "kind": self.kind, "reason": self.message})
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Name(String),
    Op(char),
    End,
}

fn lex(s: &str) -> Result<Vec<(Tok, usize)>, ExprError> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        if !b[i].is_ascii() {
            let pos = s[..i].chars().count();
            return Err(ExprError { position: pos, kind: "non_ascii", message: "表达式只允许 ASCII 字符（运算符用 * / ^，不用 × ÷ ·）".into() });
        }
        if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && i + 1 < b.len() && (b[i + 1] as char).is_ascii_digit()) {
            let st = i;
            while i < b.len() && (b[i] as char).is_ascii_digit() {
                i += 1;
            }
            if i < b.len() && b[i] == b'.' {
                i += 1;
                while i < b.len() && (b[i] as char).is_ascii_digit() {
                    i += 1;
                }
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                let mut j = i + 1;
                if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                    j += 1;
                }
                if j < b.len() && (b[j] as char).is_ascii_digit() {
                    i = j;
                    while i < b.len() && (b[i] as char).is_ascii_digit() {
                        i += 1;
                    }
                } else {
                    return Err(ExprError { position: i, kind: "syntax", message: "指数部分缺少数字".into() });
                }
            }
            let v: f64 = s[st..i].parse().map_err(|_| ExprError { position: st, kind: "syntax", message: format!("无法解析数字 {}", &s[st..i]) })?;
            if !v.is_finite() {
                return Err(ExprError { position: st, kind: "not_finite", message: format!("数字 {} 超出 f64 范围", &s[st..i]) });
            }
            out.push((Tok::Num(v), st));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let st = i;
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push((Tok::Name(s[st..i].to_string()), st));
        } else if "+-*/^(),".contains(c) {
            out.push((Tok::Op(c), i));
            i += 1;
        } else {
            return Err(ExprError { position: i, kind: "syntax", message: format!("不认识的字符 '{c}'（允许 + - * / ^ ( ) , 数字与名字）") });
        }
    }
    out.push((Tok::End, b.len()));
    Ok(out)
}

struct Parser<'a> {
    toks: Vec<(Tok, usize)>,
    i: usize,
    names: &'a [String],
    nodes: usize,
}

impl Parser<'_> {
    fn peek(&self) -> &Tok {
        &self.toks[self.i].0
    }
    fn pos(&self) -> usize {
        self.toks[self.i].1
    }
    fn bump(&mut self) -> Result<(), ExprError> {
        self.i += 1;
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(ExprError { position: self.pos(), kind: "too_many_nodes", message: format!("表达式超过 {MAX_NODES} 个记号") });
        }
        Ok(())
    }
    fn err<T>(&self, msg: impl Into<String>) -> Result<T, ExprError> {
        Err(ExprError { position: self.pos(), kind: "syntax", message: msg.into() })
    }
    fn depth(&self, d: usize) -> Result<(), ExprError> {
        if d > MAX_DEPTH {
            return Err(ExprError { position: self.pos(), kind: "too_deep", message: format!("嵌套超过 {MAX_DEPTH} 层") });
        }
        Ok(())
    }
    fn sum(&mut self, d: usize) -> Result<Node, ExprError> {
        self.depth(d)?;
        let mut l = self.product(d + 1)?;
        while let Tok::Op(c @ ('+' | '-')) = *self.peek() {
            self.bump()?;
            let r = self.product(d + 1)?;
            l = if c == '+' { Node::Add(l.into(), r.into()) } else { Node::Sub(l.into(), r.into()) };
        }
        Ok(l)
    }
    fn product(&mut self, d: usize) -> Result<Node, ExprError> {
        self.depth(d)?;
        let mut l = self.unary(d + 1)?;
        while let Tok::Op(c @ ('*' | '/')) = *self.peek() {
            self.bump()?;
            let r = self.unary(d + 1)?;
            l = if c == '*' { Node::Mul(l.into(), r.into()) } else { Node::Div(l.into(), r.into()) };
        }
        Ok(l)
    }
    fn unary(&mut self, d: usize) -> Result<Node, ExprError> {
        self.depth(d)?;
        match *self.peek() {
            Tok::Op('-') => {
                self.bump()?;
                Ok(Node::Neg(self.unary(d + 1)?.into()))
            }
            Tok::Op('+') => {
                self.bump()?;
                self.unary(d + 1)
            }
            _ => self.power(d + 1),
        }
    }
    fn power(&mut self, d: usize) -> Result<Node, ExprError> {
        self.depth(d)?;
        let base = self.primary(d + 1)?;
        if *self.peek() == Tok::Op('^') {
            self.bump()?;
            let e = self.unary(d + 1)?; // right-associative, and allows 2^-1
            return Ok(Node::Pow(base.into(), e.into()));
        }
        Ok(base)
    }
    fn primary(&mut self, d: usize) -> Result<Node, ExprError> {
        self.depth(d)?;
        let pos = self.pos();
        match self.peek().clone() {
            Tok::Num(v) => {
                self.bump()?;
                Ok(Node::Num(v))
            }
            Tok::Op('(') => {
                self.bump()?;
                let e = self.sum(d + 1)?;
                if *self.peek() != Tok::Op(')') {
                    return self.err("缺少右括号 ')'");
                }
                self.bump()?;
                Ok(e)
            }
            Tok::Name(n) => {
                self.bump()?;
                if *self.peek() == Tok::Op('(') {
                    let Some(&(fname, arity)) = FUNCS.iter().find(|(f, _)| *f == n) else {
                        return Err(ExprError { position: pos, kind: "unknown_name", message: format!("'{n}' 不是可用的函数（可用：{}）", FUNCS.iter().map(|f| f.0).collect::<Vec<_>>().join(", ")) });
                    };
                    self.bump()?;
                    let mut args = vec![self.sum(d + 1)?];
                    while *self.peek() == Tok::Op(',') {
                        self.bump()?;
                        args.push(self.sum(d + 1)?);
                    }
                    if *self.peek() != Tok::Op(')') {
                        return self.err(format!("{fname}(…) 缺少右括号 ')'"));
                    }
                    self.bump()?;
                    if args.len() != arity {
                        return Err(ExprError { position: pos, kind: "arity", message: format!("{fname} 需要 {arity} 个参数，给了 {}", args.len()) });
                    }
                    return Ok(Node::Call(fname, args));
                }
                if let Some(k) = self.names.iter().position(|x| *x == n) {
                    return Ok(Node::Var(k));
                }
                if let Some(&(_, v)) = CONSTANTS.iter().find(|(c, _)| *c == n) {
                    return Ok(Node::Num(v));
                }
                if FUNCS.iter().any(|(f, _)| *f == n) {
                    return Err(ExprError { position: pos, kind: "syntax", message: format!("'{n}' 是函数，要写成 {n}(…)") });
                }
                Err(ExprError { position: pos, kind: "unknown_name", message: format!("'{n}' 不是已声明的名字（可用：{}，以及常数 pi）", self.names.join(", ")) })
            }
            Tok::End => self.err("表达式不完整"),
            Tok::Op(c) => self.err(format!("这里不能出现 '{c}'")),
        }
    }
}

/// Parse `src` with the variables `names` (slot k ↔ names[k]).
pub fn parse(src: &str, names: &[String]) -> Result<Node, ExprError> {
    if src.chars().count() > MAX_LEN {
        return Err(ExprError { position: MAX_LEN, kind: "too_long", message: format!("表达式超过 {MAX_LEN} 个字符") });
    }
    let toks = lex(src)?;
    let mut p = Parser { toks, i: 0, names, nodes: 0 };
    if p.toks.len() == 1 {
        return p.err("表达式是空的");
    }
    let e = p.sum(0)?;
    if *p.peek() != Tok::End {
        return p.err("表达式在这里之后还有多余内容");
    }
    Ok(e)
}

/// Value of the AST: children left to right, IEEE-754 binary64, no reassociation or fusion.
pub fn eval(n: &Node, vars: &[f64]) -> f64 {
    match n {
        Node::Num(v) => *v,
        Node::Var(k) => vars[*k],
        Node::Neg(a) => -eval(a, vars),
        Node::Add(a, b) => eval(a, vars) + eval(b, vars),
        Node::Sub(a, b) => eval(a, vars) - eval(b, vars),
        Node::Mul(a, b) => eval(a, vars) * eval(b, vars),
        Node::Div(a, b) => eval(a, vars) / eval(b, vars),
        Node::Pow(a, b) => eval(a, vars).powf(eval(b, vars)),
        Node::Call(f, args) => {
            let x = eval(&args[0], vars);
            match *f {
                "sin" => x.sin(),
                "cos" => x.cos(),
                "tan" => x.tan(),
                "asin" => x.asin(),
                "acos" => x.acos(),
                "atan" => x.atan(),
                "atan2" => x.atan2(eval(&args[1], vars)),
                "sinh" => x.sinh(),
                "cosh" => x.cosh(),
                "tanh" => x.tanh(),
                "exp" => x.exp(),
                "log" => x.ln(),
                "log10" => x.log10(),
                "sqrt" => x.sqrt(),
                "abs" => x.abs(),
                "min" => {
                    let y = eval(&args[1], vars);
                    if x.is_nan() || y.is_nan() { f64::NAN } else { x.min(y) }
                }
                "max" => {
                    let y = eval(&args[1], vars);
                    if x.is_nan() || y.is_nan() { f64::NAN } else { x.max(y) }
                }
                "hypot" => x.hypot(eval(&args[1], vars)),
                "sign" => {
                    if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else if x == 0.0 { 0.0 } else { f64::NAN }
                }
                _ => unreachable!(),
            }
        }
    }
}

/// Identifier rule for declared names (states, parameters): ASCII name, not reserved.
pub fn check_name(n: &str) -> Result<(), String> {
    let ok = !n.is_empty() && n.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ok {
        return Err(format!("'{n}' 不是合法名字（字母或 _ 开头，只含 ASCII 字母、数字、_）"));
    }
    if n == "t" || FUNCS.iter().any(|f| f.0 == n) || CONSTANTS.iter().any(|c| c.0 == n) {
        return Err(format!("'{n}' 是保留名（t、常数 pi、函数名），不能用作状态或参数名"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn n(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    fn ev(s: &str, names: &[&str], vals: &[f64]) -> f64 {
        eval(&parse(s, &n(names)).unwrap(), vals)
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(ev("1 + 2*3", &[], &[]), 7.0);
        assert_eq!(ev("2^3^2", &[], &[]), 512.0); // right-associative
        assert_eq!(ev("-2^2", &[], &[]), -4.0); // -(2^2)
        assert_eq!(ev("2^-1", &[], &[]), 0.5);
        assert_eq!(ev("8/4/2", &[], &[]), 1.0); // left-associative
        assert_eq!(ev("10-4-3", &[], &[]), 3.0);
        assert_eq!(ev("(1+2)*3", &[], &[]), 9.0);
        assert_eq!(ev(".5e1 + 1E-1", &[], &[]), 5.1);
        assert_eq!(ev("-k*x - c*v", &["x", "v", "k", "c"], &[2.0, 3.0, 4.0, 0.5]), -9.5);
        assert_eq!(ev("atan2(1, 1)*4", &[], &[]), std::f64::consts::PI);
        assert_eq!(ev("pi", &[], &[]), std::f64::consts::PI);
    }

    #[test]
    fn errors_with_position() {
        let e = parse("x + ", &n(&["x"])).unwrap_err();
        assert_eq!((e.kind, e.position), ("syntax", 4));
        let e = parse("x + yy", &n(&["x"])).unwrap_err();
        assert_eq!((e.kind, e.position), ("unknown_name", 4));
        let e = parse("sin(x, 1)", &n(&["x"])).unwrap_err();
        assert_eq!(e.kind, "arity");
        let e = parse("2×x", &n(&["x"])).unwrap_err();
        assert_eq!((e.kind, e.position), ("non_ascii", 1));
        let e = parse("foo(x)", &n(&["x"])).unwrap_err();
        assert_eq!(e.kind, "unknown_name");
        let e = parse("sin", &n(&[])).unwrap_err();
        assert_eq!(e.kind, "syntax");
        let e = parse("(x", &n(&["x"])).unwrap_err();
        assert_eq!(e.kind, "syntax");
        let deep = "(".repeat(300) + "1" + &")".repeat(300);
        assert_eq!(parse(&deep, &[]).unwrap_err().kind, "too_deep");
        assert_eq!(parse("1e999", &[]).unwrap_err().kind, "not_finite");
        assert!(check_name("t").is_err() && check_name("sin").is_err() && check_name("2x").is_err() && check_name("x_1").is_ok());
    }

    /// Left-to-right, no reassociation: (a + b) + c ≠ a + (b + c) in binary64, and the AST decides.
    #[test]
    fn no_reassociation() {
        let (a, b, c) = (1e16, -1e16, 1.0);
        assert_eq!(ev("a + b + c", &["a", "b", "c"], &[a, b, c]), 1.0);
        assert_eq!(ev("a + (b + c)", &["a", "b", "c"], &[a, b, c]), 0.0);
    }
}

/// E.VALIDATION.EXPRESSION for a static error, with a caret under the offending character (shared by every
/// operator that takes expressions).
pub fn expr_err(path: &str, src: &str, e: crate::expr::ExprError) -> OpError {
    let caret = format!("{}\n{}^", src, " ".repeat(e.position.min(src.chars().count())));
    OpError::new("E.VALIDATION.EXPRESSION", format!("{path} 的表达式在第 {} 个字符处有误：{}\n{caret}", e.position, e.message), "按 3.15 的语法改写：只用已声明的状态名、参数名、t、pi、+ − * / ^ 与函数表里的函数。")
        .path(path)
        .detail("expression", json!(src))
        .detail("position", json!(e.position))
        .detail("kind", json!(e.kind))
}


/// Value and gradient of the AST by forward-mode automatic differentiation (spec 3.15 EX-8, v0.4.13).
/// `seed[k]` = Some(j): variable k is the j-th independent variable; `nd` = number of independents. The value
/// part is computed with exactly the operations of `eval` (bit-identical). Derivative conventions at
/// non-differentiable points: abs'(0) = 0, sign' = 0, min/max take the derivative of the chosen argument
/// (the first one on ties), x^y with a constant exponent uses y·x^(y−1).
pub fn eval_grad(n: &Node, vars: &[f64], seed: &[Option<usize>], nd: usize) -> (f64, Vec<f64>) {
    let zero = || vec![0.0; nd];
    let lin = |g: &[f64], s: f64| -> Vec<f64> { g.iter().map(|v| v * s).collect() };
    match n {
        Node::Num(v) => (*v, zero()),
        Node::Var(k) => {
            let mut g = zero();
            if let Some(j) = seed[*k] {
                g[j] = 1.0;
            }
            (vars[*k], g)
        }
        Node::Neg(a) => {
            let (v, g) = eval_grad(a, vars, seed, nd);
            (-v, lin(&g, -1.0))
        }
        Node::Add(a, b) | Node::Sub(a, b) => {
            let (va, ga) = eval_grad(a, vars, seed, nd);
            let (vb, gb) = eval_grad(b, vars, seed, nd);
            let sgn = if matches!(n, Node::Add(..)) { 1.0 } else { -1.0 };
            (if sgn > 0.0 { va + vb } else { va - vb }, ga.iter().zip(&gb).map(|(x, y)| x + sgn * y).collect())
        }
        Node::Mul(a, b) => {
            let (va, ga) = eval_grad(a, vars, seed, nd);
            let (vb, gb) = eval_grad(b, vars, seed, nd);
            (va * vb, ga.iter().zip(&gb).map(|(x, y)| x * vb + va * y).collect())
        }
        Node::Div(a, b) => {
            let (va, ga) = eval_grad(a, vars, seed, nd);
            let (vb, gb) = eval_grad(b, vars, seed, nd);
            let v = va / vb;
            (v, ga.iter().zip(&gb).map(|(x, y)| (x - v * y) / vb).collect())
        }
        Node::Pow(a, b) => {
            let (va, ga) = eval_grad(a, vars, seed, nd);
            let (vb, gb) = eval_grad(b, vars, seed, nd);
            let v = va.powf(vb);
            if gb.iter().all(|x| *x == 0.0) {
                let d = if vb == 0.0 { 0.0 } else { vb * va.powf(vb - 1.0) };
                (v, lin(&ga, d))
            } else {
                let la = va.ln();
                (v, ga.iter().zip(&gb).map(|(x, y)| v * (y * la + vb * x / va)).collect())
            }
        }
        Node::Call(f, args) => {
            let (x, gx) = eval_grad(&args[0], vars, seed, nd);
            let two = |args: &Vec<Node>| eval_grad(&args[1], vars, seed, nd);
            match *f {
                "sin" => (x.sin(), lin(&gx, x.cos())),
                "cos" => (x.cos(), lin(&gx, -x.sin())),
                "tan" => { let t = x.tan(); (t, lin(&gx, 1.0 + t * t)) }
                "asin" => (x.asin(), lin(&gx, 1.0 / (1.0 - x * x).sqrt())),
                "acos" => (x.acos(), lin(&gx, -1.0 / (1.0 - x * x).sqrt())),
                "atan" => (x.atan(), lin(&gx, 1.0 / (1.0 + x * x))),
                "atan2" => {
                    let (y, gy) = two(args);
                    let r2 = x * x + y * y;
                    (x.atan2(y), gx.iter().zip(&gy).map(|(dx, dy)| (y * dx - x * dy) / r2).collect())
                }
                "sinh" => (x.sinh(), lin(&gx, x.cosh())),
                "cosh" => (x.cosh(), lin(&gx, x.sinh())),
                "tanh" => { let t = x.tanh(); (t, lin(&gx, 1.0 - t * t)) }
                "exp" => { let e = x.exp(); (e, lin(&gx, e)) }
                "log" => (x.ln(), lin(&gx, 1.0 / x)),
                "log10" => (x.log10(), lin(&gx, 1.0 / (x * std::f64::consts::LN_10))),
                "sqrt" => { let r = x.sqrt(); (r, lin(&gx, 0.5 / r)) }
                "abs" => (x.abs(), lin(&gx, if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 })),
                "min" | "max" => {
                    let (y, gy) = two(args);
                    if x.is_nan() || y.is_nan() {
                        return (f64::NAN, vec![f64::NAN; nd]);
                    }
                    let pick_x = if *f == "min" { x <= y } else { x >= y };
                    if pick_x { (if *f == "min" { x.min(y) } else { x.max(y) }, gx) } else { (if *f == "min" { x.min(y) } else { x.max(y) }, gy) }
                }
                "hypot" => {
                    let (y, gy) = two(args);
                    let h = x.hypot(y);
                    (h, gx.iter().zip(&gy).map(|(dx, dy)| if h == 0.0 { 0.0 } else { (x * dx + y * dy) / h }).collect())
                }
                "sign" => (if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else if x == 0.0 { 0.0 } else { f64::NAN }, zero()),
                _ => unreachable!(),
            }
        }
    }
}

/// Declared parameters {name: number}: names checked (3.15) and kept distinct from `taken`.
pub fn parse_params(v: &Value, taken: &[String]) -> Result<(Vec<String>, Vec<f64>), aeoi_core::error::OpError> {
    let mut names = Vec::new();
    let mut vals = Vec::new();
    if let Some(o) = v.as_object() {
        for (k, val) in o {
            check_name(k).map_err(|m| aeoi_core::error::OpError::new("E.VALIDATION.EXPRESSION", m, "换一个名字。").path(&format!("/params/{k}")).detail("kind", json!("bad_name")))?;
            if taken.contains(k) {
                return Err(aeoi_core::error::OpError::new("E.VALIDATION.EXPRESSION", format!("'{k}' 已经是未知量（或自变量）的名字，表达式里无法区分。"), "给参数换一个名字。").path(&format!("/params/{k}")).detail("kind", json!("duplicate_name")));
            }
            names.push(k.clone());
            vals.push(val.as_f64().unwrap_or(f64::NAN));
        }
    }
    Ok((names, vals))
}
