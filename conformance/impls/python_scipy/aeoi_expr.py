"""AEOI expression language (spec 3.15 EX-1 .. EX-7), written from the spec text only.

A tokenizer + recursive-descent parser produce a small tree of tuples; evaluation walks the tree in
binary64, node by node, left to right (EX-4). No host-language eval is used (EX-7).
Elementary functions come from Python's `math` module (the platform C library); domain and range
errors are mapped back to the IEEE results (NaN / +-Inf) that EX-3 requires.
"""
import math
import sys

# 200 nesting levels need about 5 parser frames each (EX-5)
sys.setrecursionlimit(max(sys.getrecursionlimit(), 6000))

MAX_CHARS = 10000
MAX_NODES = 10000
MAX_DEPTH = 200
FUNCS1 = ("sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp", "log", "log10", "sqrt", "abs", "sign")
FUNCS2 = ("atan2", "hypot", "min", "max")
ARITY = {**{f: 1 for f in FUNCS1}, **{f: 2 for f in FUNCS2}}
RESERVED = set(ARITY) | {"pi"}
INF = math.inf
NAN = math.nan


class ExprError(Exception):
    def __init__(self, kind, position, message):
        super().__init__(message)
        self.kind, self.position, self.message = kind, position, message


def valid_name(s):
    if not s or not (s[0].isascii() and (s[0].isalpha() or s[0] == "_")):
        return False
    return all(c.isascii() and (c.isalnum() or c == "_") for c in s)


# ---------------------------------------------------------------- tokenizer

def _tokens(src):
    toks = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if ord(c) > 127:
            raise ExprError("non_ascii", i, f"非 ASCII 字符 {c!r}（乘方写 ^，乘号写 *）")
        if c in " \t\r\n":
            i += 1
            continue
        if c.isdigit() or (c == "." and i + 1 < n and src[i + 1].isdigit()):
            j = i
            while j < n and src[j].isdigit():
                j += 1
            if j < n and src[j] == ".":
                j += 1
                while j < n and src[j].isdigit():
                    j += 1
            if j < n and src[j] in "eE":
                k = j + 1
                if k < n and src[k] in "+-":
                    k += 1
                if k < n and src[k].isdigit():
                    while k < n and src[k].isdigit():
                        k += 1
                    j = k
                else:
                    raise ExprError("syntax", k, "指数部分缺少数字")
            text = src[i:j]
            val = float(text)            # round to nearest (EX-4)
            if not math.isfinite(val):
                raise ExprError("not_finite", i, f"数字字面量 {text} 超出 binary64 范围")
            toks.append(("num", val, i))
            i = j
            continue
        if c.isalpha() or c == "_":
            j = i
            while j < n and src[j].isascii() and (src[j].isalnum() or src[j] == "_"):
                j += 1
            toks.append(("name", src[i:j], i))
            i = j
            continue
        if c == "*" and i + 1 < n and src[i + 1] == "*":
            raise ExprError("syntax", i, "乘方只写作 ^（** 不是本语言的语法）")
        if c in "+-*/^(),":
            toks.append(("op", c, i))
            i += 1
            continue
        raise ExprError("syntax", i, f"不认识的字符 {c!r}")
    toks.append(("end", None, n))
    return toks


# ---------------------------------------------------------------- parser

class _Parser:
    def __init__(self, src, names):
        self.toks = _tokens(src)
        self.i = 0
        self.names = names          # declared variable names -> slot index
        self.nodes = 0

    def peek(self):
        return self.toks[self.i]

    def take(self):
        t = self.toks[self.i]
        self.i += 1
        return t

    def node(self, pos):
        self.nodes += 1
        if self.nodes > MAX_NODES:
            raise ExprError("too_many_nodes", pos, f"语法树节点超过 {MAX_NODES}")

    def check_depth(self, depth, pos):
        if depth > MAX_DEPTH:
            raise ExprError("too_deep", pos, f"嵌套深度超过 {MAX_DEPTH}")

    def expect(self, val):
        t = self.take()
        if t[0] != "op" or t[1] != val:
            raise ExprError("syntax", t[2], f"这里应为 {val!r}")
        return t

    def parse(self):
        e = self.sum(0)
        t = self.peek()
        if t[0] != "end":
            raise ExprError("syntax", t[2], "表达式在这里应已结束")
        return e

    def sum(self, d):
        left = self.product(d)
        while self.peek()[0] == "op" and self.peek()[1] in "+-":
            op = self.take()
            right = self.product(d)
            self.node(op[2])
            left = ("bin", op[1], left, right)
        return left

    def product(self, d):
        left = self.unary(d)
        while self.peek()[0] == "op" and self.peek()[1] in "*/":
            op = self.take()
            right = self.unary(d)
            self.node(op[2])
            left = ("bin", op[1], left, right)
        return left

    # Nesting depth (EX-5) counts parentheses, function calls, unary signs and exponents; a flat
    # chain such as x+x+...+x does not nest.
    def unary(self, d):
        t = self.peek()
        if t[0] == "op" and t[1] in "+-":
            self.take()
            self.check_depth(d + 1, t[2])
            inner = self.unary(d + 1)
            self.node(t[2])
            return ("neg", inner) if t[1] == "-" else ("pos", inner)
        return self.power(d)

    def power(self, d):
        base = self.primary(d)
        if self.peek()[0] == "op" and self.peek()[1] == "^":
            op = self.take()
            self.check_depth(d + 1, op[2])
            exp = self.unary(d + 1)      # right associative, exponent may carry a sign
            self.node(op[2])
            return ("bin", "^", base, exp)
        return base

    def primary(self, d):
        t = self.take()
        if t[0] == "num":
            self.node(t[2])
            return ("num", t[1])
        if t[0] == "name":
            name = t[1]
            nxt = self.peek()
            if nxt[0] == "op" and nxt[1] == "(":
                if name not in ARITY:
                    raise ExprError("unknown_name", t[2], f"未知函数 {name}")
                self.take()
                self.check_depth(d + 1, nxt[2])
                args = [self.sum(d + 1)]
                while self.peek()[0] == "op" and self.peek()[1] == ",":
                    self.take()
                    args.append(self.sum(d + 1))
                self.expect(")")
                if len(args) != ARITY[name]:
                    raise ExprError("arity", t[2], f"{name} 需要 {ARITY[name]} 个参数，给了 {len(args)} 个")
                self.node(t[2])
                return ("call", name, args)
            if name == "pi":
                self.node(t[2])
                return ("num", math.pi)
            if name in ARITY:
                raise ExprError("syntax", nxt[2], f"函数 {name} 后应为 '('")
            if name not in self.names:
                raise ExprError("unknown_name", t[2], f"未声明的名字 {name}")
            self.node(t[2])
            return ("var", self.names[name])
        if t[0] == "op" and t[1] == "(":
            self.check_depth(d + 1, t[2])
            e = self.sum(d + 1)
            self.expect(")")
            return e
        if t[0] == "end":
            raise ExprError("syntax", t[2], "表达式不完整")
        raise ExprError("syntax", t[2], f"这里不能出现 {t[1]!r}")


def _postfix(tree):
    """Flatten the tree into a postfix program without recursion (left-deep chains can be 10^4 long)."""
    out, stack = [], [(tree, False)]
    while stack:
        node, done = stack.pop()
        tag = node[0]
        if tag in ("num", "var"):
            out.append(node)
        elif done:
            out.append((tag, node[1]) if tag in ("bin", "call") else (tag,))
        else:
            stack.append((node, True))
            kids = [node[1]] if tag in ("neg", "pos") else ([node[2], node[3]] if tag == "bin" else node[2])
            for k in reversed(kids):
                stack.append((k, False))
    return out


def compile_expr(src, names):
    """Parse `src` with the declared variable names (list). Returns a tree; raises ExprError."""
    if len(src) > MAX_CHARS:
        raise ExprError("too_long", MAX_CHARS, f"表达式超过 {MAX_CHARS} 个字符")
    for i, c in enumerate(src):
        if ord(c) > 127:
            raise ExprError("non_ascii", i, f"非 ASCII 字符 {c!r}（乘方写 ^，乘号写 *）")
    slots = {nm: k for k, nm in enumerate(names)}
    try:
        return _postfix(_Parser(src, slots).parse())
    except RecursionError:
        err = ExprError("too_deep", 0, "嵌套过深")
    raise err


def check_names(names):
    """EX-2: declared variable names. Returns (kind, name) of the first violation or None."""
    seen = set()
    for nm in names:
        if not valid_name(nm) or nm in RESERVED:
            return "bad_name", nm
        if nm in seen:
            return "duplicate_name", nm
        seen.add(nm)
    return None


def caret_message(src, pos, text):
    line = src.replace("\n", " ").replace("\r", " ").replace("\t", " ")
    return f"{text}\n{line}\n{' ' * max(0, min(pos, len(line)))}^"


# ---------------------------------------------------------------- evaluation (EX-3, EX-4)

def _div(a, b):
    if b == 0.0:
        if a != a or a == 0.0:
            return NAN
        return math.copysign(INF, a) * math.copysign(1.0, b)
    return a / b


def _pow(a, b):
    try:
        return math.pow(a, b)
    except ValueError:              # negative base with non-integer exponent, or 0^negative
        if a == 0.0 and b < 0:
            odd = float(b).is_integer() and int(b) % 2 == 1
            return math.copysign(INF, a) if odd else INF
        return NAN
    except OverflowError:
        odd = float(b).is_integer() and int(b) % 2 == 1
        if abs(a) > 1.0 or (abs(a) < 1.0 and b < 0):
            return -INF if (a < 0 and odd) else INF
        return 0.0


def _safe(fn, x, on_value=NAN):
    try:
        return fn(x)
    except ValueError:
        return on_value
    except OverflowError:
        return INF


def _exp(x):
    try:
        return math.exp(x)
    except OverflowError:
        return INF


def _log(x, f=math.log):
    if x != x:
        return NAN
    if x == 0.0:
        return -INF
    if x < 0:
        return NAN
    if x == INF:
        return INF
    return f(x)


def _sinh(x):
    try:
        return math.sinh(x)
    except OverflowError:
        return math.copysign(INF, x)


def _cosh(x):
    try:
        return math.cosh(x)
    except OverflowError:
        return INF


def _sign(x):
    if x != x:
        return NAN
    if x > 0:
        return 1.0
    if x < 0:
        return -1.0
    return 0.0


def _min(a, b):
    if a != a or b != b:
        return NAN
    return a if a <= b else b


def _max(a, b):
    if a != a or b != b:
        return NAN
    return a if a >= b else b


def _hypot(a, b):
    try:
        return math.hypot(a, b)
    except OverflowError:
        return INF


F1 = {"sin": lambda x: _safe(math.sin, x), "cos": lambda x: _safe(math.cos, x), "tan": lambda x: _safe(math.tan, x),
      "asin": lambda x: _safe(math.asin, x), "acos": lambda x: _safe(math.acos, x), "atan": math.atan,
      "sinh": _sinh, "cosh": _cosh, "tanh": math.tanh, "exp": _exp, "log": _log,
      "log10": lambda x: _log(x, math.log10), "sqrt": lambda x: NAN if x < 0 else (x if x != x else math.sqrt(x)),
      "abs": math.fabs, "sign": _sign}
F2 = {"atan2": math.atan2, "hypot": _hypot, "min": _min, "max": _max}


def evaluate(prog, vals):
    """Evaluate a compiled postfix program with variable values `vals` (indexed by slot), node by node (EX-4)."""
    st = []
    for ins in prog:
        tag = ins[0]
        if tag == "num":
            st.append(ins[1])
        elif tag == "var":
            st.append(vals[ins[1]])
        elif tag == "neg":
            st.append(-st.pop())
        elif tag == "pos":
            pass
        elif tag == "bin":
            b = st.pop()
            a = st.pop()
            op = ins[1]
            if op == "+":
                st.append(a + b)
            elif op == "-":
                st.append(a - b)
            elif op == "*":
                st.append(a * b)
            elif op == "/":
                st.append(_div(a, b))
            else:
                st.append(_pow(a, b))
        else:  # call
            f = ins[1]
            if f in F1:
                st.append(F1[f](st.pop()))
            else:
                b = st.pop()
                a = st.pop()
                st.append(F2[f](a, b))
    return st[0]
