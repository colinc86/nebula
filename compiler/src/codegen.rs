/// Code generation: Nebula AST → target language source code.
///
/// Supports: Python, JavaScript, Go, Swift, Kotlin
use crate::ast::*;
use crate::lexer::{StringPart, Token};

/// snake_case → camelCase. Idempotent on already-camelCase input.
fn snake_to_camel(s: &str) -> String {
    if !s.contains('_') { return s.to_string(); }
    let mut out = String::new();
    let mut up = false;
    for ch in s.chars() {
        if ch == '_' { up = true; continue; }
        if up {
            out.extend(ch.to_uppercase());
            up = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// snake_case or lowercase → PascalCase (TitleCase first char + camelCase rest).
fn snake_to_pascal(s: &str) -> String {
    let camel = snake_to_camel(s);
    let mut chars = camel.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().chain(chars).collect(),
    }
}

/// Convert an interpolation token to target language text.
/// `self_keyword` controls what `sf` maps to in the target language.
fn interpolation_token_to_target(tok: &Token, self_keyword: &str) -> String {
    match tok {
        Token::Ident(n) => n.clone(),
        Token::SelfValue => self_keyword.into(),
        Token::Dot => ".".into(),
        Token::LeftParen => "(".into(),
        Token::RightParen => ")".into(),
        Token::LeftBracket => "[".into(),
        Token::RightBracket => "]".into(),
        Token::Comma => ", ".into(),
        Token::Plus => " + ".into(),
        Token::Minus => " - ".into(),
        Token::Star => " * ".into(),
        Token::Slash => " / ".into(),
        Token::Percent => " % ".into(),
        Token::Int(n) => n.to_string(),
        Token::Float(f) => format!("{f}"),
        Token::String(s) => {
            let inner: String = s.parts.iter().map(|p| match p {
                StringPart::Literal(t) => t.clone(),
                _ => "...".into(),
            }).collect();
            format!("\"{inner}\"")
        }
        // Tight colon — the canonical case inside interpolation is the Python
        // f-string format spec `{x:.2f}` where any space would corrupt the
        // spec. Pre-spaced forms like `{a, b}` already separate with Comma.
        Token::Colon => ":".into(),
        // `=` appears in Python's f-string debug form `{name=}`. Without
        // this arm it fell through to "?" (observed: `{name=}` → `{name?}`).
        Token::Assign => "=".into(),
        _ => "?".into(),
    }
}

/// Convert an interpolation token back to Nebula source text.
/// Used by the formatter and codegen for faithful reconstruction.
pub fn interpolation_token_to_nebula(tok: &Token) -> String {
    match tok {
        Token::Ident(n) => n.clone(),
        Token::SelfValue => "sf".into(),
        Token::Dot => ".".into(),
        Token::LeftParen => "(".into(),
        Token::RightParen => ")".into(),
        Token::LeftBracket => "[".into(),
        Token::RightBracket => "]".into(),
        Token::Comma => ", ".into(),
        Token::Plus => " + ".into(),
        Token::Minus => " - ".into(),
        Token::Star => " * ".into(),
        Token::Slash => " / ".into(),
        Token::Percent => " % ".into(),
        Token::Int(n) => n.to_string(),
        Token::Float(f) => format!("{f}"),
        Token::String(s) => {
            // Nested string literal inside interpolation
            let inner: String = s.parts.iter().map(|p| match p {
                StringPart::Literal(t) => t.clone(),
                StringPart::Interpolation(_) => "...".into(),
            }).collect();
            format!("\"{inner}\"")
        }
        Token::Colon => ":".into(),
        Token::Assign => "=".into(),
        Token::Eq => " == ".into(),
        Token::Less => " < ".into(),
        Token::Greater => " > ".into(),
        Token::NotEq => " != ".into(),
        Token::NilCoalesce => " ?? ".into(),
        Token::Question => "?".into(),
        Token::Bool(b) => if *b { "tt" } else { "ff" }.into(),
        Token::Nil => "nil".into(),
        _ => "?".into(),
    }
}

pub fn to_python(program: &Program) -> String {
    let mut gen = PythonGen { indent: 0 };
    gen.gen_program(program)
}

pub fn to_javascript(program: &Program) -> String {
    let mut gen = JsGen { indent: 0 };
    gen.gen_program(program)
}

pub fn to_go(program: &Program) -> String {
    let mut gen = GoGen { indent: 0, package_name: None, scopes: vec![std::collections::HashSet::new()] };
    gen.gen_program(program)
}

/// Generate Go source with an explicit package name. Iter-24 Bug A fix: the
/// Go ingester discards package declarations (converts them to Nebula
/// comments), so the package name can't be round-tripped. Callers that know
/// the target file's directory (e.g. `tool_write_code` in mcp.rs) can derive
/// the package name from sibling files and pass it here. Default `to_go`
/// remains `package main`.
pub fn to_go_with_package(program: &Program, package_name: Option<&str>) -> String {
    let mut gen = GoGen {
        indent: 0,
        package_name: package_name.map(str::to_owned),
        scopes: vec![std::collections::HashSet::new()],
    };
    gen.gen_program(program)
}

pub fn to_swift(program: &Program) -> String {
    let mut gen = SwiftGen { indent: 0 };
    gen.gen_program(program)
}

pub fn to_kotlin(program: &Program) -> String {
    let mut gen = KotlinGen { indent: 0, scopes: vec![std::collections::HashSet::new()] };
    gen.gen_program(program)
}

pub fn to_typescript(program: &Program) -> String {
    let mut gen = TsGen { indent: 0 };
    gen.gen_program(program)
}

pub fn to_rust_lang(program: &Program) -> String {
    let mut gen = RustGen { indent: 0, scopes: vec![std::collections::HashSet::new()], enum_struct_variants: std::collections::HashMap::new(), all_enum_variants: std::collections::HashSet::new(), current_fn_returns_string: false };
    gen.gen_program(program)
}

pub fn to_c(program: &Program) -> String {
    let mut gen = CGen { indent: 0, scopes: vec![std::collections::HashSet::new()] };
    gen.gen_program(program)
}

struct PythonGen {
    indent: usize,
}

impl PythonGen {
    fn ind(&self) -> String {
        "    ".repeat(self.indent)
    }

    fn gen_program(&mut self, program: &Program) -> String {
        let mut out = String::new();
        for stmt in &program.statements {
            out.push_str(&self.gen_statement(stmt));
            out.push('\n');
        }
        out
    }

    fn gen_statement(&mut self, stmt: &Statement) -> String {
        match stmt {
            Statement::Let { name, value, .. } => {
                format!("{}{} = {}", self.ind(), name, self.gen_expr(value))
            }
            Statement::Mut { name, value, .. } => {
                format!("{}{} = {}", self.ind(), name, self.gen_expr(value))
            }
            Statement::Const { name, value, .. } => {
                format!("{}{} = {}  # const", self.ind(), name, self.gen_expr(value))
            }
            Statement::FnDecl {
                name, params, body, ..
            } => {
                let py_params: Vec<String> = params
                    .iter()
                    .filter(|p| p.name != "sf")
                    .map(|p| {
                        if let Some(default) = &p.default {
                            format!("{}={}", p.name, self.gen_expr(default))
                        } else {
                            p.name.clone()
                        }
                    })
                    .collect();

                let has_self = params.iter().any(|p| p.name == "sf");
                let all_params = if has_self {
                    let mut v = vec!["self".to_string()];
                    v.extend(py_params);
                    v
                } else {
                    py_params
                };

                let mut out =
                    format!("{}def {}({}):", self.ind(), name, all_params.join(", "));
                out.push('\n');
                self.indent += 1;
                out.push_str(&self.gen_fn_body(body));
                self.indent -= 1;
                out
            }
            Statement::StructDecl {
                name, fields, ..
            } => {
                let mut out = format!("{}class {}:", self.ind(), name);
                out.push('\n');
                self.indent += 1;

                // __init__
                let field_names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                out.push_str(&format!(
                    "{}def __init__(self, {}):\n",
                    self.ind(),
                    field_names.join(", ")
                ));
                self.indent += 1;
                for f in &field_names {
                    out.push_str(&format!("{}self.{f} = {f}\n", self.ind()));
                }
                self.indent -= 1;
                self.indent -= 1;
                out
            }
            Statement::ImplBlock {
                target, methods, ..
            } => {
                let mut out = String::new();
                for m in methods {
                    if let Statement::FnDecl {
                        name: mname,
                        params,
                        body,
                        ..
                    } = m
                    {
                        // Translate self methods as class methods
                        let py_params: Vec<String> = params
                            .iter()
                            .map(|p| {
                                if p.name == "sf" {
                                    "self".to_string()
                                } else if let Some(default) = &p.default {
                                    format!("{}={}", p.name, self.gen_expr(default))
                                } else {
                                    p.name.clone()
                                }
                            })
                            .collect();

                        let is_static = !params.iter().any(|p| p.name == "sf");
                        if is_static {
                            out.push_str(&format!(
                                "{}# Static method on {target}\n",
                                self.ind()
                            ));
                        }

                        out.push_str(&format!(
                            "{}def {}({}):\n",
                            self.ind(),
                            mname,
                            if is_static {
                                py_params.join(", ")
                            } else {
                                py_params.join(", ")
                            }
                        ));
                        self.indent += 1;
                        out.push_str(&self.gen_fn_body(body));
                        self.indent -= 1;
                        out.push('\n');
                    }
                }
                // Note: proper class method injection would need a second pass.
                // For v0.1, we emit standalone functions.
                format!(
                    "{}# Methods for {target}\n{out}",
                    self.ind()
                )
            }
            Statement::Expr(expr) => {
                format!("{}{}", self.ind(), self.gen_expr(expr))
            }
            Statement::Return(Some(expr)) => {
                format!("{}return {}", self.ind(), self.gen_expr(expr))
            }
            Statement::Return(None) => format!("{}return", self.ind()),
            Statement::Break(_) => format!("{}break", self.ind()),
            Statement::Continue => format!("{}continue", self.ind()),
            Statement::Throw(Some(e)) => format!("{}raise {}", self.ind(), self.gen_expr(e)),
            Statement::Throw(None) => format!("{}raise", self.ind()),
            Statement::Assignment { target, op, value } => {
                let op_str = match op {
                    AssignOp::Assign => "=",
                    AssignOp::AddAssign => "+=",
                    AssignOp::SubAssign => "-=",
                    AssignOp::MulAssign => "*=",
                    AssignOp::DivAssign => "/=",
                    AssignOp::ModAssign => "%=",
                };
                format!(
                    "{}{} {} {}",
                    self.ind(),
                    self.gen_expr(target),
                    op_str,
                    self.gen_expr(value)
                )
            }
            Statement::Use { path, items, alias } => {
                let module = path.join(".");
                if let Some(items) = items {
                    format!("{}from {} import {}", self.ind(), module, items.join(", "))
                } else if let Some(alias) = alias {
                    format!("{}import {} as {}", self.ind(), module, alias)
                } else {
                    format!("{}import {}", self.ind(), module)
                }
            }
            Statement::LetDestructure { names, value, .. } => {
                format!("{}{} = {}", self.ind(), names.join(", "), self.gen_expr(value))
            }
            Statement::TypeAlias { name, type_expr } => {
                format!("{}{} = {}", self.ind(), name, self.gen_type(type_expr))
            }
            Statement::Module { .. } => {
                // Python has no file-level module statement — module identity
                // is derived from the filesystem path. Emit nothing.
                String::new()
            }
            Statement::EnumDecl { .. } | Statement::TraitDecl { .. } => {
                format!("{}pass  # TODO: translate", self.ind())
            }
        }
    }

    fn gen_block_body(&mut self, block: &Block) -> String {
        self.gen_block_inner(block, false)
    }

    /// Function-body variant: wraps a trailing `block.expr` in `return` so
    /// Nebula's implicit "last expression is the value" rule round-trips
    /// correctly. Inner blocks (for/while/if bodies) use `gen_block_body`
    /// which emits trailing exprs as plain statements.
    fn gen_fn_body(&mut self, block: &Block) -> String {
        self.gen_block_inner(block, true)
    }

    fn gen_block_inner(&mut self, block: &Block, is_fn_body: bool) -> String {
        let mut out = String::new();
        if block.statements.is_empty() && block.expr.is_none() {
            out.push_str(&format!("{}pass\n", self.ind()));
            return out;
        }
        for stmt in &block.statements {
            out.push_str(&self.gen_statement(stmt));
            out.push('\n');
        }
        if let Some(expr) = &block.expr {
            if PythonGen::is_statement_expr(expr) {
                // Control-flow "expressions" in Nebula translate to statements
                // in Python — for/while/loop return unit, and multi-line
                // if/match can't be wrapped in `return`. The expression's own
                // gen_expr prepends its leading indent, so we don't.
                out.push_str(&self.gen_expr(expr));
                if !out.ends_with('\n') {
                    out.push('\n');
                }
            } else if is_fn_body {
                // Only the top-level function body turns a trailing expression
                // into an explicit `return`.
                out.push_str(&format!("{}return {}\n", self.ind(), self.gen_expr(expr)));
            } else {
                // Inner block: trailing expression is an expression statement.
                out.push_str(&format!("{}{}\n", self.ind(), self.gen_expr(expr)));
            }
        }
        out
    }

    /// Returns true if the expression should be emitted as a Python statement
    /// (no `return` wrapping) rather than an inline expression.
    fn is_statement_expr(expr: &Expr) -> bool {
        match expr {
            Expr::For { .. }
            | Expr::While { .. }
            | Expr::Loop { .. }
            | Expr::TryCatch { .. }
            | Expr::Match { .. } => true,
            // Multi-line if/match can't be wrapped in `return`. The ternary
            // fast-path in gen_expr handles pure-value ifs before we get here.
            Expr::If { then_block, else_block, .. } => {
                let then_is_value = then_block.statements.is_empty()
                    && then_block.expr.is_some();
                let else_is_value = else_block
                    .as_ref()
                    .map(|eb| eb.statements.is_empty() && eb.expr.is_some())
                    .unwrap_or(false);
                !(then_is_value && else_is_value)
            }
            Expr::Match { .. } => true,
            _ => false,
        }
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n) => n.to_string(),
            Expr::FloatLit(f) => {
                if *f == (*f as i64) as f64 {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
            Expr::BoolLit(b) => {
                if *b {
                    "True".to_string()
                } else {
                    "False".to_string()
                }
            }
            Expr::StringLit(s) => {
                let mut out = String::from("f\"");
                for part in &s.parts {
                    match part {
                        StringPart::Literal(text) => out.push_str(text),
                        StringPart::Interpolation(tokens) => {
                            out.push('{');
                            for tok in tokens {
                                out.push_str(&interpolation_token_to_target(tok, "self"));
                            }
                            out.push('}');
                        }
                    }
                }
                out.push('"');
                // If no interpolation, drop the f prefix
                if !s.parts.iter().any(|p| matches!(p, StringPart::Interpolation(_))) {
                    out = format!("\"{}\"", &out[2..out.len() - 1]);
                }
                out
            }
            Expr::CharLit(c) => format!("'{c}'"),
            Expr::Nil => "None".to_string(),
            Expr::Ident(name) => name.clone(),

            Expr::BinaryOp { left, op, right } => {
                let op_str = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::Pow => "**",
                    BinOp::Eq => "==",
                    BinOp::NotEq => "!=",
                    BinOp::Less => "<",
                    BinOp::Greater => ">",
                    BinOp::LessEq => "<=",
                    BinOp::GreaterEq => ">=",
                    BinOp::And => "and",
                    BinOp::Or => "or",
                    BinOp::BitAnd => "&",
                    BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::ShiftLeft => "<<",
                    BinOp::ShiftRight => ">>",
                };
                format!(
                    "({} {} {})",
                    self.gen_expr(left),
                    op_str,
                    self.gen_expr(right)
                )
            }

            Expr::UnaryOp { op, operand } => {
                let op_str = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "not ",
                    UnaryOp::BitNot => "~",
                };
                format!("{}{}", op_str, self.gen_expr(operand))
            }

            Expr::Call { callee, args } => {
                let args_str: Vec<String> = args
                    .iter()
                    .map(|a| {
                        if let Some(name) = &a.name {
                            format!("{}={}", name, self.gen_expr(&a.value))
                        } else {
                            self.gen_expr(&a.value)
                        }
                    })
                    .collect();
                // Resolve Nebula function names (including aliases) to Python
                let callee_str = self.gen_expr(callee);
                let py_fn = match callee_str.as_str() {
                    "prt" | "print" | "println" => "print",
                    "ln" | "len" => "len",
                    "tsr" | "to_string" => "str",
                    "toi" | "to_int" => "int",
                    "tof" | "to_float" => "float",
                    "typ" | "type_of" => {
                        // type(x) takes 1 arg; isinstance(x, T) takes 2+.
                        if args_str.len() >= 2 {
                            "isinstance"
                        } else {
                            "type"
                        }
                    }
                    "rng" | "range" => "range",
                    other => other,
                };
                format!("{}({})", py_fn, args_str.join(", "))
            }

            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                let args_str: Vec<String> =
                    args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let obj = self.gen_expr(object);
                // Several Nebula methods don't map to Python methods — they
                // are top-level functions or operators.
                match method.as_str() {
                    "slc" => {
                        return match args_str.len() {
                            0 => format!("{}[:]", obj),
                            1 => format!("{}[{}:]", obj, args_str[0]),
                            2 => format!("{}[{}:{}]", obj, args_str[0], args_str[1]),
                            _ => format!("{}[{}]", obj, args_str.join(":")),
                        };
                    }
                    "slc_from" => {
                        return format!("{}[{}:]", obj, args_str.join(", "));
                    }
                    "slc_step" => {
                        return match args_str.len() {
                            3 => format!("{}[{}:{}:{}]", obj, args_str[0], args_str[1], args_str[2]),
                            _ => format!("{}[{}]", obj, args_str.join(":")),
                        };
                    }
                    "mp" | "map" => {
                        // If the callback is a multi-param lambda, use a list
                        // comprehension instead of `map` — Python's `map`
                        // passes the whole tuple as a single argument which
                        // breaks `lambda x, y: ...`.
                        if let Some(Expr::Lambda { params, body }) = args.first().map(|a| &a.value) {
                            if params.len() > 1 {
                                let names: Vec<&str> =
                                    params.iter().map(|p| p.name.as_str()).collect();
                                return format!(
                                    "[{} for {} in {}]",
                                    self.gen_expr(body),
                                    names.join(", "),
                                    obj,
                                );
                            }
                        }
                        return format!("list(map({}, {}))", args_str.join(", "), obj);
                    }
                    "fl" | "filter" => {
                        if let Some(Expr::Lambda { params, body }) = args.first().map(|a| &a.value) {
                            if params.len() > 1 {
                                let names: Vec<&str> =
                                    params.iter().map(|p| p.name.as_str()).collect();
                                return format!(
                                    "[({}, ) for {} in {} if {}]",
                                    names.join(", "),
                                    names.join(", "),
                                    obj,
                                    self.gen_expr(body),
                                );
                            }
                        }
                        return format!("list(filter({}, {}))", args_str.join(", "), obj);
                    }
                    "fd" | "fold" | "rd" | "reduce" => {
                        // functools.reduce(f, iter [, init])
                        return format!(
                            "__import__('functools').reduce({}, {})",
                            args_str.join(", "),
                            obj,
                        );
                    }
                    "ln" | "len" => return format!("len({})", obj),
                    "con" | "contains" => {
                        // obj.con(x)  →  x in obj
                        if args_str.len() == 1 {
                            return format!("({} in {})", args_str[0], obj);
                        }
                    }
                    "is_empty" => return format!("(len({}) == 0)", obj),
                    "to_set" => return format!("set({})", obj),
                    "to_map" => return format!("dict({})", obj),
                    "rev" | "reverse" => {
                        // `[::-1]` works for both strings and lists, unlike
                        // `list(reversed(x))` which returns a list even for
                        // string input (breaks type).
                        return format!("{}[::-1]", obj);
                    }
                    "srt" | "sort" => return format!("sorted({})", obj),
                    "unq" | "unique" => return format!("list(dict.fromkeys({}))", obj),
                    _ => {}
                }
                let py_method = match method.as_str() {
                    "jn" | "join" => "join",
                    "psh" | "push" => "append",
                    "clr" | "clear" => "clear",
                    "ck" | "contains_key" => "__contains__",
                    "cnt" | "count" => "count",
                    "to_upper" | "upper" => "upper",
                    "to_lower" | "lower" => "lower",
                    other => other,
                };
                format!(
                    "{}.{}({})",
                    obj,
                    py_method,
                    args_str.join(", ")
                )
            }

            Expr::FieldAccess { object, field } => {
                format!("{}.{}", self.gen_expr(object), field)
            }

            Expr::Index { object, index } => {
                // Legacy `xs[a..b]` (Index with a Range inside) → Python slice.
                if let Expr::Range { start, end, inclusive } = index.as_ref() {
                    let s = self.gen_expr(start);
                    let e = self.gen_expr(end);
                    let end_str = if *inclusive {
                        format!("{} + 1", e)
                    } else {
                        e
                    };
                    return format!("{}[{}:{}]", self.gen_expr(object), s, end_str);
                }
                format!("{}[{}]", self.gen_expr(object), self.gen_expr(index))
            }

            Expr::Slice { object, start, end } => {
                let s = start.as_ref().map(|e| self.gen_expr(e)).unwrap_or_default();
                let e_s = end.as_ref().map(|e| self.gen_expr(e)).unwrap_or_default();
                format!("{}[{}:{}]", self.gen_expr(object), s, e_s)
            }

            Expr::Lambda { params, body } => {
                let param_names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                format!("lambda {}: {}", param_names.join(", "), self.gen_expr(body))
            }

            Expr::If {
                condition,
                then_block,
                else_block,
            } => {
                // If both branches are single expressions, use ternary
                if let (Some(then_expr), Some(else_block)) =
                    (&then_block.expr, else_block)
                {
                    if then_block.statements.is_empty() {
                        if let Some(else_expr) = &else_block.expr {
                            if else_block.statements.is_empty() {
                                return format!(
                                    "({} if {} else {})",
                                    self.gen_expr(then_expr),
                                    self.gen_expr(condition),
                                    self.gen_expr(else_expr)
                                );
                            }
                        }
                    }
                }

                // Multi-line if — caller is responsible for prepending the
                // starting indent (via Statement::Expr or gen_block_body).
                let mut out = format!("if {}:\n", self.gen_expr(condition));
                self.indent += 1;
                out.push_str(&self.gen_block_body(then_block));
                self.indent -= 1;
                if let Some(eb) = else_block {
                    out.push_str(&format!("{}else:\n", self.ind()));
                    self.indent += 1;
                    out.push_str(&self.gen_block_body(eb));
                    self.indent -= 1;
                }
                out
            }

            Expr::Match { subject, arms } => {
                let mut out = format!("{}match {}:\n", self.ind(), self.gen_expr(subject));
                self.indent += 1;
                for arm in arms {
                    let pat = self.gen_pattern(&arm.pattern);
                    let guard = arm
                        .guard
                        .as_ref()
                        .map(|g| format!(" if {}", self.gen_expr(g)))
                        .unwrap_or_default();
                    out.push_str(&format!(
                        "{}case {}{}:\n",
                        self.ind(),
                        pat,
                        guard
                    ));
                    self.indent += 1;
                    // Match-arm body can be a Block — emit each statement
                    // directly rather than wrapping the block in `return`,
                    // which would hit the Block-as-expr catch-all and
                    // emit `(...)`.
                    if let Expr::Block(b) = &arm.body {
                        out.push_str(&self.gen_block_body(b));
                    } else {
                        out.push_str(&format!(
                            "{}return {}\n",
                            self.ind(),
                            self.gen_expr(&arm.body)
                        ));
                    }
                    self.indent -= 1;
                }
                self.indent -= 1;
                out
            }

            Expr::Block(block) => {
                if let Some(expr) = &block.expr {
                    if block.statements.is_empty() {
                        return self.gen_expr(expr);
                    }
                }
                // Can't directly represent a block expression in Python
                format!("({})", "...")
            }

            Expr::List(items) => {
                let items_str: Vec<String> =
                    items.iter().map(|e| self.gen_expr(e)).collect();
                format!("[{}]", items_str.join(", "))
            }

            Expr::Map(entries) => {
                let entries_str: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", self.gen_expr(k), self.gen_expr(v)))
                    .collect();
                format!("{{{}}}", entries_str.join(", "))
            }

            Expr::Tuple(items) => {
                let items_str: Vec<String> =
                    items.iter().map(|e| self.gen_expr(e)).collect();
                if items_str.len() == 1 {
                    format!("({},)", items_str[0])
                } else {
                    format!("({})", items_str.join(", "))
                }
            }

            Expr::StructLit { name, fields, .. } => {
                let args: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, self.gen_expr(v)))
                    .collect();
                format!("{}({})", name, args.join(", "))
            }

            Expr::Range {
                start,
                end,
                inclusive,
            } => {
                if *inclusive {
                    format!(
                        "range({}, {} + 1)",
                        self.gen_expr(start),
                        self.gen_expr(end)
                    )
                } else {
                    format!("range({}, {})", self.gen_expr(start), self.gen_expr(end))
                }
            }

            Expr::Pipe { left, right } => {
                // x |> f  =>  f(x)
                // x |> f(a)  =>  f(x, a)
                match right.as_ref() {
                    Expr::Call { callee, args } => {
                        let mut all_args = vec![self.gen_expr(left)];
                        all_args.extend(args.iter().map(|a| self.gen_expr(&a.value)));
                        format!("{}({})", self.gen_expr(callee), all_args.join(", "))
                    }
                    Expr::Ident(name) => {
                        format!("{}({})", name, self.gen_expr(left))
                    }
                    _ => format!("{}({})", self.gen_expr(right), self.gen_expr(left)),
                }
            }

            Expr::NilCoalesce { left, right } => {
                let l = self.gen_expr(left);
                format!("({l} if {l} is not None else {})", self.gen_expr(right))
            }

            Expr::Try(inner) => self.gen_expr(inner),

            Expr::TryCatch { try_block, error_binding, catch_block } => {
                let mut out = format!("{}try:\n", self.ind());
                self.indent += 1;
                out.push_str(&self.gen_block_body(try_block));
                self.indent -= 1;
                out.push_str(&format!("{}except Exception as {}:\n", self.ind(), error_binding));
                self.indent += 1;
                out.push_str(&self.gen_block_body(catch_block));
                self.indent -= 1;
                out
            }

            Expr::For {
                binding,
                index_binding,
                iterable,
                body,
            } => {
                let iter_expr = self.gen_expr(iterable);
                let binding_str = if let Some(idx) = index_binding {
                    format!("{idx}, {binding}")
                } else {
                    binding.clone()
                };
                let enumerate = index_binding.is_some();
                let mut out = if enumerate {
                    format!("for {} in enumerate({}):\n", binding_str, iter_expr)
                } else {
                    format!("for {} in {}:\n", binding_str, iter_expr)
                };
                self.indent += 1;
                out.push_str(&self.gen_block_body(body));
                self.indent -= 1;
                out
            }

            Expr::While { condition, body } => {
                let mut out = format!("while {}:\n", self.gen_expr(condition));
                self.indent += 1;
                out.push_str(&self.gen_block_body(body));
                self.indent -= 1;
                out
            }

            Expr::Loop { body } => {
                let mut out = String::from("while True:\n");
                self.indent += 1;
                out.push_str(&self.gen_block_body(body));
                self.indent -= 1;
                out
            }
        }
    }

    fn gen_type(&self, t: &TypeExpr) -> String {
        match t {
            TypeExpr::Named(n) => match n.as_str() {
                "Int" => "int".into(), "Float" => "float".into(),
                "String" => "str".into(), "Bool" => "bool".into(),
                other => other.to_string(),
            },
            TypeExpr::Generic(name, args) => {
                let a: Vec<String> = args.iter().map(|a| self.gen_type(a)).collect();
                format!("{}[{}]", name, a.join(", "))
            }
            TypeExpr::Option(inner) => format!("Optional[{}]", self.gen_type(inner)),
            _ => "Any".into(),
        }
    }

    fn gen_pattern(&mut self, pattern: &Pattern) -> String {
        match pattern {
            Pattern::Wildcard => "_".to_string(),
            Pattern::Binding(name) => name.clone(),
            Pattern::Literal(expr) => self.gen_expr(expr),
            Pattern::Range(start, end, inclusive) => {
                if *inclusive {
                    format!(
                        "x if {} <= x <= {}",
                        self.gen_expr(start),
                        self.gen_expr(end)
                    )
                } else {
                    format!(
                        "x if {} <= x < {}",
                        self.gen_expr(start),
                        self.gen_expr(end)
                    )
                }
            }
            Pattern::Variant(name, subs) => {
                let children: Vec<String> = subs.iter().map(|p| self.gen_pattern(p)).collect();
                match name.as_str() {
                    "(tuple)" => format!("({})", children.join(", ")),
                    "[array]" => format!("[{}]", children.join(", ")),
                    // Map-pattern key info is lost in Nebula's grammar — fall
                    // back to wildcard so the arm body still runs.
                    "{map}" => "_".to_string(),
                    n if n.starts_with('.') => {
                        if subs.is_empty() { n[1..].to_string() }
                        else { format!("{}({})", &n[1..], children.join(", ")) }
                    }
                    n => {
                        if subs.is_empty() { n.to_string() }
                        else { format!("{}({})", n, children.join(", ")) }
                    }
                }
            }
        }
    }
}

// =============================================================================
// JavaScript Code Generator
// =============================================================================

struct JsGen {
    indent: usize,
}

impl JsGen {
    fn ind(&self) -> String {
        "  ".repeat(self.indent)
    }

    fn gen_program(&mut self, program: &Program) -> String {
        let mut out = String::new();
        // Pair StructDecl with its ImplBlock and emit ES6 class for each.
        let mut consumed: Vec<bool> = vec![false; program.statements.len()];
        for (i, stmt) in program.statements.iter().enumerate() {
            if consumed[i] { continue; }
            if let Statement::StructDecl { name, fields, .. } = stmt {
                // Find a matching ImplBlock in the rest of the program.
                let mut impl_idx: Option<usize> = None;
                for (j, s2) in program.statements.iter().enumerate().skip(i + 1) {
                    if let Statement::ImplBlock { target, .. } = s2 {
                        if target == name { impl_idx = Some(j); break; }
                    }
                }
                if let Some(j) = impl_idx {
                    let methods = if let Statement::ImplBlock { methods, .. } = &program.statements[j] { methods.clone() } else { vec![] };
                    out.push_str(&self.gen_class_for_struct(name, fields, &methods));
                    out.push('\n');
                    consumed[i] = true;
                    consumed[j] = true;
                    continue;
                }
            }
            // ImplBlock without preceding StructDecl: emit class with no fields.
            if let Statement::ImplBlock { target, methods, .. } = stmt {
                out.push_str(&self.gen_class_for_struct(target, &[], methods));
                out.push('\n');
                consumed[i] = true;
                continue;
            }
            out.push_str(&self.gen_statement(stmt));
            out.push('\n');
        }
        out
    }

    fn gen_class_for_struct(&mut self, name: &str, fields: &[crate::ast::Field], methods: &[Statement]) -> String {
        let mut out = format!("{}class {} {{\n", self.ind(), name);
        self.indent += 1;
        // Emit constructor — either from struct fields or from a method named `new`/`constructor`.
        let new_method = methods.iter().find(|m| matches!(m, Statement::FnDecl { name, .. } if name == "new" || name == "constructor"));
        if let Some(Statement::FnDecl { params, body, .. }) = new_method {
            let ps: Vec<String> = params.iter().filter(|p| p.name != "sf").map(|p| p.name.clone()).collect();
            out.push_str(&format!("{}constructor({}) {{\n", self.ind(), ps.join(", ")));
            self.indent += 1;
            // body — emit raw statements without `return` wrapping
            for s in &body.statements {
                out.push_str(&self.gen_statement(s));
                out.push('\n');
            }
            if let Some(e) = &body.expr {
                out.push_str(&format!("{}{};\n", self.ind(), self.gen_expr(e)));
            }
            self.indent -= 1;
            out.push_str(&format!("{}}}\n", self.ind()));
        } else if !fields.is_empty() {
            let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
            out.push_str(&format!("{}constructor({}) {{\n", self.ind(), names.join(", ")));
            self.indent += 1;
            for n in &names { out.push_str(&format!("{}this.{n} = {n};\n", self.ind())); }
            self.indent -= 1;
            out.push_str(&format!("{}}}\n", self.ind()));
        }
        // Other methods.
        for m in methods {
            if let Statement::FnDecl { name: mname, params, body, .. } = m {
                if mname == "new" || mname == "constructor" { continue; }
                let has_self = params.iter().any(|p| p.name == "sf");
                let ps: Vec<String> = params.iter().filter(|p| p.name != "sf").map(|p| p.name.clone()).collect();
                let prefix = if has_self { "" } else { "static " };
                out.push_str(&format!("{}{}{}({}) {{\n", self.ind(), prefix, mname, ps.join(", ")));
                self.indent += 1;
                out.push_str(&self.gen_block_body(body));
                self.indent -= 1;
                out.push_str(&format!("{}}}\n", self.ind()));
            }
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}", self.ind()));
        out
    }

    fn gen_statement(&mut self, stmt: &Statement) -> String {
        match stmt {
            Statement::Let { name, value, .. } => {
                format!("{}const {} = {};", self.ind(), name, self.gen_expr(value))
            }
            Statement::Mut { name, value, .. } => {
                format!("{}let {} = {};", self.ind(), name, self.gen_expr(value))
            }
            Statement::Const { name, value, .. } => {
                format!("{}const {} = {};", self.ind(), name, self.gen_expr(value))
            }
            Statement::FnDecl {
                name, params, body, ..
            } => {
                let ps: Vec<String> = params
                    .iter()
                    .filter(|p| p.name != "sf")
                    .map(|p| {
                        if let Some(default) = &p.default {
                            format!("{} = {}", p.name, self.gen_expr(default))
                        } else {
                            p.name.clone()
                        }
                    })
                    .collect();

                let mut out =
                    format!("{}function {}({}) {{\n", self.ind(), name, ps.join(", "));
                self.indent += 1;
                out.push_str(&self.gen_block_body(body));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::StructDecl {
                name, fields, ..
            } => {
                let field_names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                let mut out = format!("{}class {} {{\n", self.ind(), name);
                self.indent += 1;
                out.push_str(&format!(
                    "{}constructor({}) {{\n",
                    self.ind(),
                    field_names.join(", ")
                ));
                self.indent += 1;
                for f in &field_names {
                    out.push_str(&format!("{}this.{f} = {f};\n", self.ind()));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}\n", self.ind()));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::ImplBlock {
                target, methods, ..
            } => {
                let mut out = String::new();
                for m in methods {
                    if let Statement::FnDecl {
                        name: mname,
                        params,
                        body,
                        ..
                    } = m
                    {
                        let has_self = params.iter().any(|p| p.name == "sf");
                        let ps: Vec<String> = params
                            .iter()
                            .filter(|p| p.name != "sf")
                            .map(|p| {
                                if let Some(default) = &p.default {
                                    format!("{} = {}", p.name, self.gen_expr(default))
                                } else {
                                    p.name.clone()
                                }
                            })
                            .collect();

                        if has_self {
                            out.push_str(&format!(
                                "{}{}.prototype.{} = function({}) {{\n",
                                self.ind(),
                                target,
                                mname,
                                ps.join(", ")
                            ));
                        } else {
                            out.push_str(&format!(
                                "{}{}.{} = function({}) {{\n",
                                self.ind(),
                                target,
                                mname,
                                ps.join(", ")
                            ));
                        }
                        self.indent += 1;
                        out.push_str(&self.gen_block_body(body));
                        self.indent -= 1;
                        out.push_str(&format!("{}}};\n", self.ind()));
                    }
                }
                out
            }
            Statement::Expr(expr) => {
                format!("{}{};", self.ind(), self.gen_expr(expr))
            }
            Statement::Return(Some(expr)) => {
                format!("{}return {};", self.ind(), self.gen_expr(expr))
            }
            Statement::Return(None) => format!("{}return;", self.ind()),
            Statement::Break(_) => format!("{}break;", self.ind()),
            Statement::Continue => format!("{}continue;", self.ind()),
            Statement::Throw(Some(e)) => format!("{}throw {};", self.ind(), self.gen_expr(e)),
            Statement::Throw(None) => format!("{}throw;", self.ind()),
            Statement::Assignment { target, op, value } => {
                let op_str = match op {
                    AssignOp::Assign => "=",
                    AssignOp::AddAssign => "+=",
                    AssignOp::SubAssign => "-=",
                    AssignOp::MulAssign => "*=",
                    AssignOp::DivAssign => "/=",
                    AssignOp::ModAssign => "%=",
                };
                format!(
                    "{}{} {} {};",
                    self.ind(),
                    self.gen_expr(target),
                    op_str,
                    self.gen_expr(value)
                )
            }
            Statement::Use { path, .. } => {
                format!(
                    "{}// import {}",
                    self.ind(),
                    path.join(".")
                )
            }
            Statement::LetDestructure { names, value, mutable } => {
                let kw = if *mutable { "let" } else { "const" };
                format!("{}{} [{}] = {};", self.ind(), kw, names.join(", "), self.gen_expr(value))
            }
            Statement::TypeAlias { name, .. } => {
                format!("{}// type {} = ...", self.ind(), name)
            }
            Statement::Module { path } => {
                // JS/TS modules are file-scoped; no explicit module statement.
                // Leave an informational comment so round-trips don't lose the
                // module hint, but don't emit anything that affects semantics.
                format!("{}// module {}", self.ind(), path.join("."))
            }
            Statement::EnumDecl { .. } | Statement::TraitDecl { .. } => {
                format!("{}// TODO: translate", self.ind())
            }
        }
    }

    fn gen_block_body(&mut self, block: &Block) -> String {
        let mut out = String::new();
        for stmt in &block.statements {
            out.push_str(&self.gen_statement(stmt));
            out.push('\n');
        }
        if let Some(expr) = &block.expr {
            // Don't add return before statement-like expressions
            let is_stmt_like = matches!(expr.as_ref(),
                Expr::For { .. } | Expr::While { .. } | Expr::Loop { .. }
                | Expr::If { .. } | Expr::Match { .. } | Expr::TryCatch { .. }
            );
            if is_stmt_like {
                out.push_str(&format!("{}{}\n", self.ind(), self.gen_expr(expr)));
            } else {
                out.push_str(&format!("{}return {};\n", self.ind(), self.gen_expr(expr)));
            }
        }
        if out.is_empty() {
            out.push_str(&format!("{}// empty\n", self.ind()));
        }
        out
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n) => n.to_string(),
            Expr::FloatLit(f) => {
                if *f == (*f as i64) as f64 {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
            Expr::BoolLit(b) => b.to_string(),
            Expr::StringLit(s) => {
                let mut out = String::from("`");
                for part in &s.parts {
                    match part {
                        StringPart::Literal(text) => out.push_str(text),
                        StringPart::Interpolation(tokens) => {
                            out.push_str("${");
                            for tok in tokens {
                                out.push_str(&interpolation_token_to_target(tok, "this"));
                            }
                            out.push('}');
                        }
                    }
                }
                out.push('`');
                // If no interpolation, use regular quotes
                if !s.parts.iter().any(|p| matches!(p, StringPart::Interpolation(_))) {
                    let inner = &out[1..out.len() - 1];
                    return format!("\"{}\"", inner);
                }
                out
            }
            Expr::CharLit(c) => format!("\"{}\"", c),
            Expr::Nil => "null".to_string(),
            Expr::Ident(name) => {
                if name == "sf" {
                    "this".to_string()
                } else {
                    name.clone()
                }
            }

            Expr::BinaryOp { left, op, right } => {
                let op_str = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::Pow => "**",
                    BinOp::Eq => "===",
                    BinOp::NotEq => "!==",
                    BinOp::Less => "<",
                    BinOp::Greater => ">",
                    BinOp::LessEq => "<=",
                    BinOp::GreaterEq => ">=",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                    BinOp::BitAnd => "&",
                    BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::ShiftLeft => "<<",
                    BinOp::ShiftRight => ">>",
                };
                format!(
                    "({} {} {})",
                    self.gen_expr(left),
                    op_str,
                    self.gen_expr(right)
                )
            }

            Expr::UnaryOp { op, operand } => {
                let op_str = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "!",
                    UnaryOp::BitNot => "~",
                };
                format!("{}{}", op_str, self.gen_expr(operand))
            }

            Expr::Call { callee, args } => {
                let args_str: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(&a.value))
                    .collect();
                // Map common builtins
                let callee_str = self.gen_expr(callee);
                match callee_str.as_str() {
                    "prt" | "print" | "println" => {
                        format!("console.log({})", args_str.join(", "))
                    }
                    "tsr" | "to_string" => format!("String({})", args_str.join(", ")),
                    "toi" | "to_int" => format!("parseInt({})", args_str.join(", ")),
                    "tof" | "to_float" => format!("parseFloat({})", args_str.join(", ")),
                    "typ" | "type_of" => format!("typeof {}", args_str.join(", ")),
                    "ln" | "len" => format!("{}.length", args_str.join(", ")),
                    _ => format!("{}({})", callee_str, args_str.join(", "))
                }
            }

            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                let args_str: Vec<String> =
                    args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let obj = self.gen_expr(object);
                // Resolve short aliases, then map to JS
                let resolved = match method.as_str() {
                    "mp" => "map", "fl" => "filter", "fd" => "fold", "rd" => "reduce",
                    "srt" => "sort", "rv" => "reverse", "jn" => "join", "psh" => "push",
                    "con" => "contains", "ck" => "contains_key", "ln" => "len",
                    "unq" => "unique", "cnt" => "count",
                    other => other,
                };
                let js_method = match resolved {
                    "fold" | "reduce" => "reduce",
                    "any" => "some",
                    "all" => "every",
                    "to_upper" => "toUpperCase",
                    "to_lower" => "toLowerCase",
                    "starts_with" => "startsWith",
                    "ends_with" => "endsWith",
                    "is_empty" => "length === 0 /*",
                    "push" => "concat",
                    "contains" => "includes",
                    other => other,
                };
                if js_method.contains("/*") {
                    return format!("{}.length === 0", obj);
                }
                // len() → .length (property, not method)
                if method == "len" {
                    return format!("{}.length", obj);
                }
                format!("{}.{}({})", obj, js_method, args_str.join(", "))
            }

            Expr::FieldAccess { object, field } => {
                format!("{}.{}", self.gen_expr(object), field)
            }

            Expr::Index { object, index } => {
                format!("{}[{}]", self.gen_expr(object), self.gen_expr(index))
            }

            Expr::Slice { object, start, end } => {
                // JS: no slice-subscript syntax; use .slice(start, end).
                let obj = self.gen_expr(object);
                match (start.as_ref(), end.as_ref()) {
                    (None, None) => format!("{}.slice()", obj),
                    (Some(s), None) => format!("{}.slice({})", obj, self.gen_expr(s)),
                    (None, Some(e)) => format!("{}.slice(0, {})", obj, self.gen_expr(e)),
                    (Some(s), Some(e)) => format!("{}.slice({}, {})", obj, self.gen_expr(s), self.gen_expr(e)),
                }
            }

            Expr::Lambda { params, body } => {
                let ps: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                if matches!(body.as_ref(), Expr::Block(_)) {
                    format!("({}) => {}", ps.join(", "), self.gen_expr(body))
                } else {
                    format!("({}) => {}", ps.join(", "), self.gen_expr(body))
                }
            }

            Expr::If {
                condition,
                then_block,
                else_block,
            } => {
                // Ternary if both branches are single expressions
                if let (Some(then_expr), Some(else_b)) = (&then_block.expr, else_block) {
                    if then_block.statements.is_empty() {
                        if let Some(else_expr) = &else_b.expr {
                            if else_b.statements.is_empty() {
                                return format!(
                                    "({} ? {} : {})",
                                    self.gen_expr(condition),
                                    self.gen_expr(then_expr),
                                    self.gen_expr(else_expr)
                                );
                            }
                        }
                    }
                }

                let mut out = format!("if ({}) {{\n", self.gen_expr(condition));
                self.indent += 1;
                out.push_str(&self.gen_block_body(then_block));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                if let Some(eb) = else_block {
                    out.push_str(" else {\n");
                    self.indent += 1;
                    out.push_str(&self.gen_block_body(eb));
                    self.indent -= 1;
                    out.push_str(&format!("{}}}", self.ind()));
                }
                out
            }

            Expr::Match { subject, arms } => {
                let subj = self.gen_expr(subject);
                let mut out = format!("(() => {{\n");
                self.indent += 1;
                // Generate if/else chain
                for (i, arm) in arms.iter().enumerate() {
                    let prefix = if i == 0 { "if" } else { "} else if" };
                    match &arm.pattern {
                        Pattern::Wildcard | Pattern::Binding(_) => {
                            if let Some(guard) = &arm.guard {
                                // Wildcard with guard: if (guard) { ... }
                                out.push_str(&format!(
                                    "{}{} ({}) {{\n",
                                    self.ind(),
                                    prefix,
                                    self.gen_expr(guard)
                                ));
                                self.indent += 1;
                                out.push_str(&format!(
                                    "{}return {};\n",
                                    self.ind(),
                                    self.gen_expr(&arm.body)
                                ));
                                self.indent -= 1;
                            } else if i == 0 {
                                out.push_str(&format!(
                                    "{}return {};\n",
                                    self.ind(),
                                    self.gen_expr(&arm.body)
                                ));
                            } else {
                                out.push_str(&format!(
                                    "{}}} else {{\n",
                                    self.ind()
                                ));
                                self.indent += 1;
                                out.push_str(&format!(
                                    "{}return {};\n",
                                    self.ind(),
                                    self.gen_expr(&arm.body)
                                ));
                                self.indent -= 1;
                                out.push_str(&format!("{}}}\n", self.ind()));
                            }
                        }
                        Pattern::Literal(lit) => {
                            out.push_str(&format!(
                                "{}{} ({} === {}) {{\n",
                                self.ind(),
                                prefix,
                                subj,
                                self.gen_expr(lit)
                            ));
                            self.indent += 1;
                            out.push_str(&format!(
                                "{}return {};\n",
                                self.ind(),
                                self.gen_expr(&arm.body)
                            ));
                            self.indent -= 1;
                        }
                        Pattern::Range(start, end, inclusive) => {
                            let cmp = if *inclusive { "<=" } else { "<" };
                            out.push_str(&format!(
                                "{}{} ({} >= {} && {} {} {}) {{\n",
                                self.ind(),
                                prefix,
                                subj,
                                self.gen_expr(start),
                                subj,
                                cmp,
                                self.gen_expr(end)
                            ));
                            self.indent += 1;
                            out.push_str(&format!(
                                "{}return {};\n",
                                self.ind(),
                                self.gen_expr(&arm.body)
                            ));
                            self.indent -= 1;
                        }
                        Pattern::Variant(name, _) => {
                            out.push_str(&format!(
                                "{}{} ({} === \"{}\") {{\n",
                                self.ind(),
                                prefix,
                                subj,
                                name
                            ));
                            self.indent += 1;
                            out.push_str(&format!(
                                "{}return {};\n",
                                self.ind(),
                                self.gen_expr(&arm.body)
                            ));
                            self.indent -= 1;
                        }
                    }
                }
                self.indent -= 1;
                out.push_str(&format!("{}}})()", self.ind()));
                out
            }

            Expr::Block(block) => {
                if let Some(expr) = &block.expr {
                    if block.statements.is_empty() {
                        return self.gen_expr(expr);
                    }
                }
                "(() => { /* block */ })()".to_string()
            }

            Expr::List(items) => {
                let items_str: Vec<String> =
                    items.iter().map(|e| self.gen_expr(e)).collect();
                format!("[{}]", items_str.join(", "))
            }

            Expr::Map(entries) => {
                let es: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", self.gen_expr(k), self.gen_expr(v)))
                    .collect();
                format!("{{ {} }}", es.join(", "))
            }

            Expr::Tuple(items) => {
                let items_str: Vec<String> =
                    items.iter().map(|e| self.gen_expr(e)).collect();
                format!("[{}]", items_str.join(", "))
            }

            Expr::StructLit { name, fields, .. } => {
                let args: Vec<String> = fields
                    .iter()
                    .map(|(_, v)| self.gen_expr(v))
                    .collect();
                format!("new {}({})", name, args.join(", "))
            }

            Expr::Range {
                start,
                end,
                inclusive,
            } => {
                if *inclusive {
                    format!(
                        "Array.from({{length: {} - {} + 1}}, (_, i) => i + {})",
                        self.gen_expr(end),
                        self.gen_expr(start),
                        self.gen_expr(start)
                    )
                } else {
                    format!(
                        "Array.from({{length: {} - {}}}, (_, i) => i + {})",
                        self.gen_expr(end),
                        self.gen_expr(start),
                        self.gen_expr(start)
                    )
                }
            }

            Expr::Pipe { left, right } => {
                match right.as_ref() {
                    Expr::Call { callee, args } => {
                        let mut all_args = vec![self.gen_expr(left)];
                        all_args.extend(args.iter().map(|a| self.gen_expr(&a.value)));
                        format!("{}({})", self.gen_expr(callee), all_args.join(", "))
                    }
                    Expr::Ident(name) => {
                        format!("{}({})", name, self.gen_expr(left))
                    }
                    _ => format!("{}({})", self.gen_expr(right), self.gen_expr(left)),
                }
            }

            Expr::NilCoalesce { left, right } => {
                format!("({} ?? {})", self.gen_expr(left), self.gen_expr(right))
            }

            Expr::Try(inner) => self.gen_expr(inner),

            Expr::TryCatch { try_block, error_binding, catch_block } => {
                let mut out = format!("{}try {{\n", self.ind());
                self.indent += 1; out.push_str(&self.gen_block_body(try_block)); self.indent -= 1;
                out.push_str(&format!("{}}} catch ({}) {{\n", self.ind(), error_binding));
                self.indent += 1; out.push_str(&self.gen_block_body(catch_block)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }

            Expr::For {
                binding,
                index_binding,
                iterable,
                body,
            } => {
                let iter_expr = self.gen_expr(iterable);
                let mut out = if let Some(idx) = index_binding {
                    format!(
                        "for (const [{}, {}] of {}.entries()) {{\n",
                        idx, binding, iter_expr
                    )
                } else {
                    format!("for (const {} of {}) {{\n", binding, iter_expr)
                };
                self.indent += 1;
                out.push_str(&self.gen_block_body(body));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }

            Expr::While { condition, body } => {
                let mut out = format!("while ({}) {{\n", self.gen_expr(condition));
                self.indent += 1;
                out.push_str(&self.gen_block_body(body));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }

            Expr::Loop { body } => {
                let mut out = format!("while (true) {{\n");
                self.indent += 1;
                out.push_str(&self.gen_block_body(body));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
        }
    }
}

// =============================================================================
// Go Code Generator
// =============================================================================

struct GoGen {
    indent: usize,
    /// Iter-24 Bug A: overrides the hard-coded `package main` when set. Driven
    /// from `to_go_with_package` / `lang_detect::from_nebula_to_path`.
    package_name: Option<String>,
    /// Stack of declared names per scope. Nebula's bare `x = 5` is a
    /// new-declaration-or-reassignment; Go requires explicit `:=` on first
    /// use. First-use simple-ident Assignment gets promoted to `x := v`.
    scopes: Vec<std::collections::HashSet<String>>,
}

impl GoGen {
    fn ind(&self) -> String { "\t".repeat(self.indent) }
    fn is_declared(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }
    fn declare(&mut self, name: &str) {
        if let Some(top) = self.scopes.last_mut() { top.insert(name.to_string()); }
    }

    fn collect_generics_go(t: &TypeExpr, out: &mut std::collections::BTreeSet<String>) {
        const KNOWN: &[&str] = &["Int", "Int8", "Int16", "Int32", "Int64", "Uint",
            "Uint8", "Uint16", "Uint32", "Uint64", "Byte", "Rune", "Float",
            "Float32", "Float64", "Complex64", "Complex128", "Bool", "String",
            "Str", "Char", "Any", "Error", "Nil"];
        match t {
            TypeExpr::Named(n) => {
                if n.len() == 1 && n.chars().next().unwrap().is_ascii_uppercase() {
                    out.insert(n.clone());
                } else if !KNOWN.contains(&n.as_str()) && !n.contains('.')
                    && n.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                    && !n.is_empty()
                {
                    out.insert(n.clone());
                }
            }
            TypeExpr::Generic(_, args) => { for a in args { Self::collect_generics_go(a, out); } }
            TypeExpr::Option(inner) => Self::collect_generics_go(inner, out),
            TypeExpr::Function(params, ret) => {
                for p in params { Self::collect_generics_go(p, out); }
                Self::collect_generics_go(ret, out);
            }
            TypeExpr::Tuple(items) => { for i in items { Self::collect_generics_go(i, out); } }
        }
    }

    /// Recognise common Go stdlib packages so we can PascalCase their members.
    fn is_go_stdlib_pkg(name: &str) -> bool {
        matches!(name, "fmt" | "strings" | "strconv" | "os" | "io" | "errors"
            | "sort" | "time" | "bytes" | "math" | "unicode" | "bufio"
            | "regexp" | "filepath" | "json" | "log" | "encoding"
            | "context" | "sync" | "atomic" | "reflect" | "runtime"
            | "http" | "url" | "ioutil" | "syscall" | "path"
            | "rand" | "binary" | "base64" | "hex" | "csv"
            | "image" | "color")
    }

    fn gen_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Named(n) => match n.as_str() {
                // Iter-24 Bug B fix: width-specific numeric types must lower
                // to their Go native forms. Previously these fell through the
                // `other => other.to_string()` catch-all and leaked into
                // output as `Float64`, `Int64`, etc. Keep this list in sync
                // with the Go arm of text_level_translate in lang_detect.rs.
                "Int" => "int".into(),
                "Int8" => "int8".into(),
                "Int16" => "int16".into(),
                "Int32" => "int32".into(),
                "Int64" => "int64".into(),
                "Uint" => "uint".into(),
                "Uint8" | "Byte" => "byte".into(),
                "Uint16" => "uint16".into(),
                "Uint32" => "uint32".into(),
                "Uint64" => "uint64".into(),
                "Rune" => "rune".into(),
                "Float" | "Float64" => "float64".into(),
                "Float32" => "float32".into(),
                "Complex64" => "complex64".into(),
                "Complex128" => "complex128".into(),
                "Bool" => "bool".into(),
                "String" | "Str" => "string".into(),
                "Any" => "interface{}".into(),
                "Error" => "error".into(),
                // Reverse qualified type merging: TimeTime → time.Time, SyncWaitGroup → sync.WaitGroup
                "TimeTime" => "time.Time".into(),
                "TimeDuration" => "time.Duration".into(),
                "SyncMutex" => "sync.Mutex".into(),
                "SyncRWMutex" => "sync.RWMutex".into(),
                "SyncWaitGroup" => "sync.WaitGroup".into(),
                "StringsBuilder" => "strings.Builder".into(),
                "JsonMarshal" => "json.Marshal".into(),
                "IoReader" => "io.Reader".into(),
                "IoWriter" => "io.Writer".into(),
                "IoReadWriter" => "io.ReadWriter".into(),
                "IoReadCloser" => "io.ReadCloser".into(),
                "IoWriteCloser" => "io.WriteCloser".into(),
                "OsFile" => "*os.File".into(),
                "BytesBuffer" => "bytes.Buffer".into(),
                "ContextContext" => "context.Context".into(),
                "HttpRequest" => "*http.Request".into(),
                "HttpResponse" => "*http.Response".into(),
                "HttpHandler" => "http.Handler".into(),
                "HttpResponseWriter" => "http.ResponseWriter".into(),
                "ErrorsError" => "errors.Error".into(),
                "NetConn" => "net.Conn".into(),
                "NetListener" => "net.Listener".into(),
                "TestingT" => "*testing.T".into(),
                "TestingB" => "*testing.B".into(),
                "TestingM" => "*testing.M".into(),
                other => other.to_string(),
            },
            TypeExpr::Generic(name, args) => {
                let a: Vec<String> = args.iter().map(|t| self.gen_type(t)).collect();
                match name.as_str() {
                    "List" | "Array" => format!("[]{}", a.first().map(|s| s.as_str()).unwrap_or("interface{}")),
                    "Map" if a.len() == 2 => format!("map[{}]{}", a[0], a[1]),
                    "Channel" => format!("chan {}", a.first().map(|s| s.as_str()).unwrap_or("interface{}")),
                    "Ptr" => format!("*{}", a.first().map(|s| s.as_str()).unwrap_or("interface{}")),
                    _ => format!("{}<{}>", name, a.join(", ")),
                }
            }
            TypeExpr::Function(params, ret) => {
                let ps: Vec<String> = params.iter().map(|t| self.gen_type(t)).collect();
                format!("func({}) {}", ps.join(", "), self.gen_type(ret))
            }
            TypeExpr::Tuple(types) => {
                let t: Vec<String> = types.iter().map(|t| self.gen_type(t)).collect();
                format!("({})", t.join(", "))
            }
            TypeExpr::Option(inner) => format!("*{}", self.gen_type(inner)),
        }
    }

    fn gen_param_type(&self, p: &Param) -> String {
        p.type_ann.as_ref().map(|t| self.gen_type(t)).unwrap_or_else(|| "interface{}".into())
    }

    fn gen_program(&mut self, program: &Program) -> String {
        // Collect imports by scanning the program
        let pkg = self.package_name.as_deref().unwrap_or("main");
        let mut out = format!("package {}\n\n", pkg);
        // We'll add imports after scanning
        let mut body = String::new();
        for stmt in &program.statements {
            body.push_str(&self.gen_stmt(stmt));
            body.push('\n');
        }
        // Simple import detection
        let mut imports = Vec::new();
        if body.contains("fmt.") { imports.push("\"fmt\""); }
        if body.contains("math.") { imports.push("\"math\""); }
        if body.contains("sort.") { imports.push("\"sort\""); }
        if body.contains("strings.") { imports.push("\"strings\""); }
        if body.contains("strconv.") { imports.push("\"strconv\""); }
        if body.contains("sync.") { imports.push("\"sync\""); }
        if body.contains("time.") { imports.push("\"time\""); }
        if body.contains("json.") { imports.push("\"encoding/json\""); }
        if body.contains("io.") { imports.push("\"io\""); }
        if body.contains("os.") { imports.push("\"os\""); }
        if body.contains("errors.") { imports.push("\"errors\""); }
        if body.contains("bytes.") { imports.push("\"bytes\""); }
        if body.contains("context.") { imports.push("\"context\""); }
        if body.contains("http.") { imports.push("\"net/http\""); }
        if body.contains("net.") && !body.contains("net/http") { imports.push("\"net\""); }
        if body.contains("bufio.") { imports.push("\"bufio\""); }
        if body.contains("regexp.") { imports.push("\"regexp\""); }
        if body.contains("log.") { imports.push("\"log\""); }
        if body.contains("testing.") { imports.push("\"testing\""); }
        if !imports.is_empty() {
            out.push_str("import (\n");
            for imp in &imports {
                out.push_str(&format!("\t{}\n", imp));
            }
            out.push_str(")\n\n");
        }
        out.push_str(&body);
        out
    }

    fn gen_stmt(&mut self, stmt: &Statement) -> String {
        match stmt {
            Statement::Let { name, value, type_ann, .. } | Statement::Const { name, value, type_ann, .. } => {
                let ty = type_ann.as_ref().map(|t| self.gen_type(t));
                let v = self.gen_expr(value);
                self.declare(name);
                let top_level = self.indent == 0;
                match (top_level, ty) {
                    (true, Some(t)) => format!("var {} {} = {}", name, t, v),
                    (true, None) => format!("const {} = {}", name, v),
                    (false, Some(t)) => format!("{}var {} {} = {}", self.ind(), name, t, v),
                    (false, None) => format!("{}{} := {}", self.ind(), name, v),
                }
            }
            Statement::Mut { name, value, type_ann, .. } => {
                // Check for go_var("type") pattern in the AST before generating
                if let Expr::Call { callee, args } = value {
                    if let Expr::Ident(ref fn_name) = **callee {
                        if fn_name == "go_var" {
                            if let Some(arg) = args.first() {
                                if let Expr::StringLit(s) = &arg.value {
                                    let ty = s.parts.iter().map(|p| match p {
                                        StringPart::Literal(t) => t.as_str(),
                                        _ => "",
                                    }).collect::<String>();
                                    return format!("{}var {} {}", self.ind(), name, ty);
                                }
                            }
                        }
                    }
                }
                let val = self.gen_expr(value);
                let ty = type_ann.as_ref().map(|t| self.gen_type(t));
                self.declare(name);
                match ty {
                    Some(t) => format!("{}var {} {} = {}", self.ind(), name, t, val),
                    None => {
                        if val == "nil" {
                            format!("{}var {} interface{{}}", self.ind(), name)
                        } else {
                            format!("{}{} := {}", self.ind(), name, val)
                        }
                    },
                }
            }
            Statement::FnDecl { name, params, body, return_type, .. } => {
                let ps: Vec<String> = params.iter()
                    .filter(|p| p.name != "sf")
                    .map(|p| format!("{} {}", p.name, self.gen_param_type(p)))
                    .collect();
                let ret = return_type.as_ref()
                    .map(|t| format!(" {}", self.gen_type(t)))
                    .unwrap_or_default();
                // Recover generic params from signature: single uppercase
                // letters not otherwise defined as types — emit as
                // `func name[T any, U any](...)`.
                let mut gen_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                for p in params.iter().filter(|p| p.name != "sf") {
                    if let Some(t) = &p.type_ann { Self::collect_generics_go(t, &mut gen_set); }
                }
                if let Some(rt) = return_type { Self::collect_generics_go(rt, &mut gen_set); }
                let gen_str = if gen_set.is_empty() { String::new() }
                    else {
                        let items: Vec<String> = gen_set.iter().map(|g| format!("{} any", g)).collect();
                        format!("[{}]", items.join(", "))
                    };
                self.scopes.push(std::collections::HashSet::new());
                for p in params { self.declare(&p.name); }
                let mut out = format!("{}func {}{}({}){} {{\n", self.ind(), name, gen_str, ps.join(", "), ret);
                self.indent += 1;
                out.push_str(&self.gen_block(body));
                self.indent -= 1;
                self.scopes.pop();
                out.push_str(&format!("{}}}\n", self.ind()));
                out
            }
            Statement::StructDecl { name, fields, .. } => {
                let mut out = format!("{}type {} struct {{\n", self.ind(), name);
                self.indent += 1;
                for f in fields {
                    let ty = self.gen_type(&f.type_ann);
                    // Preserve field names as-is (Go struct fields keep original casing)
                    out.push_str(&format!("{}{} {}\n", self.ind(), f.name, ty));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}\n", self.ind()));
                out
            }
            Statement::EnumDecl { name, variants, .. } => {
                let mut out = format!("{}type {} int\n\n{}const (\n", self.ind(), name, self.ind());
                self.indent += 1;
                for (i, v) in variants.iter().enumerate() {
                    if i == 0 {
                        out.push_str(&format!("{}{} {} = iota\n", self.ind(), v.name, name));
                    } else {
                        out.push_str(&format!("{}{}\n", self.ind(), v.name));
                    }
                }
                self.indent -= 1;
                out.push_str(&format!("{})\n", self.ind()));
                out
            }
            Statement::ImplBlock { target, methods, .. } => {
                // Use first lowercase letter of type as receiver name
                // Use "sf" as receiver name — matches ingested Nebula convention
                let recv_char = "sf";
                let mut out = String::new();
                for method in methods {
                    if let Statement::FnDecl { name, params, body, return_type, .. } = method {
                        // Check if sf is mutable (pointer receiver) or not (value receiver)
                        let is_mut_receiver = params.iter()
                            .any(|p| p.name == "sf" && p.type_ann.is_some());
                        let ps: Vec<String> = params.iter()
                            .filter(|p| p.name != "sf")
                            .map(|p| format!("{} {}", p.name, self.gen_param_type(p)))
                            .collect();
                        let ret = return_type.as_ref()
                            .map(|t| format!(" {}", self.gen_type(t)))
                            .unwrap_or_default();
                        let _ = is_mut_receiver;
                        let recv = format!("{} {}", recv_char, target);
                        out.push_str(&format!("{}func ({}) {}({}){} {{\n",
                            self.ind(), recv, name, ps.join(", "), ret));
                        self.indent += 1;
                        out.push_str(&self.gen_block(body));
                        self.indent -= 1;
                        out.push_str(&format!("{}}}\n\n", self.ind()));
                    }
                }
                out
            }
            Statement::TraitDecl { name, methods, .. } => {
                let mut out = format!("{}type {} interface {{\n", self.ind(), name);
                self.indent += 1;
                for m in methods {
                    let ps: Vec<String> = m.params.iter()
                        .filter(|p| p.name != "sf")
                        .map(|p| format!("{} {}", p.name, self.gen_param_type(p)))
                        .collect();
                    let ret = m.return_type.as_ref()
                        .map(|t| format!(" {}", self.gen_type(t)))
                        .unwrap_or_default();
                    out.push_str(&format!("{}{}({}){}\n", self.ind(), m.name, ps.join(", "), ret));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}\n", self.ind()));
                out
            }
            Statement::TypeAlias { name, type_expr } => {
                format!("{}type {} {}\n", self.ind(), name, self.gen_type(type_expr))
            }
            Statement::Expr(expr) => format!("{}{}", self.ind(), self.gen_expr(expr)),
            Statement::Return(Some(e)) => format!("{}return {}", self.ind(), self.gen_expr(e)),
            Statement::Return(None) => format!("{}return", self.ind()),
            Statement::Assignment { target, op, value } => {
                let o = match op {
                    AssignOp::Assign => "=", AssignOp::AddAssign => "+=",
                    AssignOp::SubAssign => "-=", AssignOp::MulAssign => "*=",
                    AssignOp::DivAssign => "/=", AssignOp::ModAssign => "%=",
                };
                // Nebula bare `x = 5` at first use becomes Go `x := 5`.
                // Subsequent reassigns stay `x = 5`.
                if matches!(op, AssignOp::Assign) {
                    if let Expr::Ident(name) = target {
                        if !self.is_declared(name) {
                            let v = self.gen_expr(value);
                            self.declare(name);
                            return format!("{}{} := {}", self.ind(), name, v);
                        }
                    }
                }
                format!("{}{} {} {}", self.ind(), self.gen_expr(target), o, self.gen_expr(value))
            }
            Statement::Break(_) => format!("{}break", self.ind()),
            Statement::Continue => format!("{}continue", self.ind()),
            Statement::Throw(Some(e)) => format!("{}panic({})", self.ind(), self.gen_expr(e)),
            Statement::Throw(None) => format!("{}panic(\"error\")", self.ind()),
            Statement::LetDestructure { names, value, .. } => {
                format!("{}{} := {}", self.ind(), names.join(", "), self.gen_expr(value))
            }
            // Iter-24 Bug D: explicit handlers for Use/Module so these don't
            // fall through to the TODO catch-all (which silently corrupts file
            // output). gen_program also collects imports by scanning the body
            // for `fmt.`, `strings.` etc., so these may be redundant with the
            // auto-import block — goimports would dedupe, but we emit them
            // anyway to preserve the explicit intent.
            Statement::Use { path, alias, .. } => {
                let joined = path.join("/");
                if let Some(alias) = alias {
                    format!("{}import {} \"{}\"", self.ind(), alias, joined)
                } else {
                    format!("{}import \"{}\"", self.ind(), joined)
                }
            }
            Statement::Module { .. } => {
                // Package declaration is emitted by gen_program (see Bug A
                // fix via package_name), not as a statement.
                String::new()
            }
            _ => format!("{}// TODO: {:?}", self.ind(), std::mem::discriminant(stmt)),
        }
    }

    fn gen_block(&mut self, block: &Block) -> String {
        let mut out = String::new();
        for s in &block.statements { out.push_str(&self.gen_stmt(s)); out.push('\n'); }
        if let Some(e) = &block.expr {
            match e.as_ref() {
                Expr::If { .. } | Expr::Match { .. } | Expr::For { .. }
                | Expr::While { .. } | Expr::Loop { .. } | Expr::Block(_)
                | Expr::MethodCall { .. } | Expr::Call { .. } => {
                    out.push_str(&format!("{}{}\n", self.ind(), self.gen_expr(e)));
                }
                _ => {
                    out.push_str(&format!("{}return {}\n", self.ind(), self.gen_expr(e)));
                }
            }
        }
        self.scopes.pop();
        out
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n) => n.to_string(),
            Expr::FloatLit(f) => format!("{f:?}"),
            Expr::BoolLit(b) => b.to_string(),
            Expr::StringLit(s) => {
                let mut out = String::from("fmt.Sprintf(\"");
                let mut args = Vec::new();
                for part in &s.parts {
                    match part {
                        StringPart::Literal(t) => out.push_str(&t.replace('\n', "\\n").replace('\t', "\\t")),
                        StringPart::Interpolation(tokens) => {
                            out.push_str("%v");
                            let a: String = tokens.iter().map(|t| {
                                interpolation_token_to_target(t, "sf")
                            }).collect();
                            args.push(a);
                        }
                    }
                }
                out.push('"');
                if !args.is_empty() { out.push_str(&format!(", {}", args.join(", "))); }
                out.push(')');
                if !s.parts.iter().any(|p| matches!(p, StringPart::Interpolation(_))) {
                    // Extract just the string content from fmt.Sprintf("...")
                    // "fmt.Sprintf(\"" = 13 chars, trailing "\")" = 2 chars
                    return format!("\"{}\"", &out[13..out.len()-2]);
                }
                out
            }
            Expr::Nil => "nil".into(),
            Expr::Ident(n) => n.clone(),
            Expr::BinaryOp { left, op, right } => {
                let o = match op {
                    BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/",
                    BinOp::Mod => "%", BinOp::Eq => "==", BinOp::NotEq => "!=",
                    BinOp::Less => "<", BinOp::Greater => ">", BinOp::LessEq => "<=",
                    BinOp::GreaterEq => ">=", BinOp::And => "&&", BinOp::Or => "||",
                    BinOp::BitAnd => "&", BinOp::BitOr => "|", BinOp::BitXor => "^",
                    BinOp::ShiftLeft => "<<", BinOp::ShiftRight => ">>",
                    BinOp::Pow => "/* pow */",
                };
                format!("({} {} {})", self.gen_expr(left), o, self.gen_expr(right))
            }
            Expr::UnaryOp { op, operand } => {
                let o = match op { UnaryOp::Neg => "-", UnaryOp::Not => "!", UnaryOp::BitNot => "^" };
                format!("{}{}", o, self.gen_expr(operand))
            }
            Expr::Call { callee, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let c = self.gen_expr(callee);
                match c.as_str() {
                    "prt" | "print" | "println" => format!("fmt.Println({})", a.join(", ")),
                    "tsr" | "to_string" => format!("fmt.Sprint({})", a.join(", ")),
                    "toi" | "to_int" => format!("int({})", a.join(", ")),
                    "tof" | "to_float" => format!("float64({})", a.join(", ")),
                    "ln" => format!("len({})", a.join(", ")),
                    "psh" => format!("append({})", a.join(", ")),
                    "defer" => format!("defer {}", a.join(", ")),
                    "make" => {
                        let ty = a.first().map(|s| s.trim_matches('"').to_string())
                            .unwrap_or_else(|| "interface{}".into());
                        format!("make({})", ty)
                    }
                    "go_var" => {
                        let ty = a.first().map(|s| s.trim_matches('"').to_string())
                            .unwrap_or_else(|| "interface{}".into());
                        // Emit proper Go zero-value for the type
                        if ty.starts_with("[]") || ty.starts_with("map[") {
                            "nil".into()  // slices and maps default to nil
                        } else {
                            format!("{}{{}}", ty)  // struct types use Type{}
                        }
                    }
                    "__spawn" => format!("go {}", a.join(", ")),
                    "__addr" => format!("&{}", a.join(", ")),
                    "__deref" => format!("*{}", a.join(", ")),
                    "Channel" => {
                        if a.len() >= 2 {
                            format!("make(chan {}, {})", a[0], a[1])
                        } else {
                            format!("make(chan {})", a.first().unwrap_or(&"interface{}".to_string()))
                        }
                    }
                    _ => format!("{}({})", c, a.join(", ")),
                }
            }
            Expr::MethodCall { object, method, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let obj = self.gen_expr(object);
                match method.as_str() {
                    "remove" => format!("delete({}, {})", obj, a.join(", ")),
                    "send" => format!("{} <- {}", obj, a.join(", ")),
                    "recv" => format!("<-{}", obj),
                    "close" => format!("close({})", obj),
                    // `out.append(x)` → `out = append(out, x)` in Go —
                    // append is a builtin, not a method, and returns a new
                    // slice. Emit the assign so the caller's slice is
                    // actually extended.
                    "append" => {
                        let mut all_args = vec![obj.clone()];
                        all_args.extend(a);
                        format!("{} = append({})", obj, all_args.join(", "))
                    }
                    "len" if a.is_empty() => format!("len({})", obj),
                    "cap" if a.is_empty() => format!("cap({})", obj),
                    _ => {
                        let m = if Self::is_go_stdlib_pkg(&obj) {
                            snake_to_pascal(method)
                        } else { method.clone() };
                        format!("{}.{}({})", obj, m, a.join(", "))
                    }
                }
            }
            Expr::FieldAccess { object, field } => {
                let obj = self.gen_expr(object);
                let f = if Self::is_go_stdlib_pkg(&obj) {
                    snake_to_pascal(field)
                } else { field.clone() };
                format!("{}.{}", obj, f)
            }
            Expr::Index { object, index } => format!("{}[{}]", self.gen_expr(object), self.gen_expr(index)),
            Expr::Slice { object, start, end } => {
                // Go: native `xs[a:b]`; either bound may be elided.
                let s = start.as_ref().map(|e| self.gen_expr(e)).unwrap_or_default();
                let e_s = end.as_ref().map(|e| self.gen_expr(e)).unwrap_or_default();
                format!("{}[{}:{}]", self.gen_expr(object), s, e_s)
            }
            Expr::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|p| format!("{} {}", p.name, self.gen_param_type(p))).collect();
                match body.as_ref() {
                    Expr::Block(block) => {
                        // Check if the block returns a comparison → add bool return type
                        let has_bool_return = block.expr.as_ref().map_or(false, |e| {
                            matches!(e.as_ref(), Expr::BinaryOp { op, .. }
                                if matches!(op, BinOp::Less | BinOp::Greater | BinOp::LessEq
                                    | BinOp::GreaterEq | BinOp::Eq | BinOp::NotEq))
                        }) || block.statements.iter().any(|s| {
                            if let Statement::Return(Some(e)) = s {
                                matches!(e, Expr::BinaryOp { op, .. }
                                    if matches!(op, BinOp::Less | BinOp::Greater | BinOp::LessEq
                                        | BinOp::GreaterEq | BinOp::Eq | BinOp::NotEq))
                            } else { false }
                        });
                        let ret = if has_bool_return { " bool" } else { "" };
                        let mut out = format!("func({}){} {{\n", ps.join(", "), ret);
                        self.indent += 1;
                        out.push_str(&self.gen_block(block));
                        self.indent -= 1;
                        out.push_str(&format!("{}}}", self.ind()));
                        out
                    }
                    _ => {
                        format!("func({}) interface{{}} {{ return {} }}", ps.join(", "), self.gen_expr(body))
                    }
                }
            }
            Expr::If { condition, then_block, else_block } => {
                let cond = self.gen_expr(condition);
                let mut out = format!("if {} {{\n", cond);
                self.indent += 1; out.push_str(&self.gen_block(then_block)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                if let Some(eb) = else_block {
                    out.push_str(" else {\n");
                    self.indent += 1; out.push_str(&self.gen_block(eb)); self.indent -= 1;
                    out.push_str(&format!("{}}}", self.ind()));
                }
                out
            }
            Expr::For { binding, index_binding, iterable, body, .. } => {
                // Nebula `for x in xs` means x is the value; in Go the
                // idiomatic form is `for _, x := range xs`.
                // `for i, x in xs` → `for i, x := range xs`.
                let range_var = match index_binding {
                    Some(idx) => format!("{}, {}", idx, binding),
                    None => format!("_, {}", binding),
                };
                let mut out = format!("for {} := range {} {{\n", range_var, self.gen_expr(iterable));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::While { condition, body } => {
                let mut out = format!("for {} {{\n", self.gen_expr(condition));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::Loop { body } => {
                let mut out = format!("for {{\n");
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::List(items) => {
                let i: Vec<String> = items.iter().map(|e| self.gen_expr(e)).collect();
                // Infer list element type from items
                let elem_type = if items.iter().all(|e| matches!(e, Expr::StringLit(_))) {
                    "string"
                } else if items.iter().all(|e| matches!(e, Expr::IntLit(_))) {
                    "int"
                } else if items.iter().all(|e| matches!(e, Expr::FloatLit(_))) {
                    "float64"
                } else {
                    "interface{}"
                };
                format!("[]{}{{{}}}", elem_type, i.join(", "))
            }
            Expr::Range { start, end, .. } => format!("{}:{}", self.gen_expr(start), self.gen_expr(end)),
            Expr::Pipe { left, right } => {
                if let Expr::Ident(n) = right.as_ref() { format!("{}({})", n, self.gen_expr(left)) }
                else { format!("/* pipe */ {}", self.gen_expr(right)) }
            }
            Expr::NilCoalesce { left, right } => {
                let l = self.gen_expr(left);
                format!("(func() interface{{}} {{ if {l} != nil {{ return {l} }}; return {} }}())", self.gen_expr(right))
            }
            Expr::Match { subject, arms } => {
                let subj = self.gen_expr(subject);
                let mut out = format!("switch {} {{\n", subj);
                self.indent += 1;
                for arm in arms {
                    match &arm.pattern {
                        Pattern::Wildcard => {
                            out.push_str(&format!("{}default:\n", self.ind()));
                        }
                        Pattern::Binding(n) if n == "_" => {
                            out.push_str(&format!("{}default:\n", self.ind()));
                        }
                        Pattern::Binding(n) => {
                            // In Nebula's match grammar, a bare identifier is
                            // ambiguous — it could be a pattern variable OR a
                            // constant/enum name. In Go switch cases we prefer
                            // the constant interpretation unless the name is
                            // `_`, which is handled above.
                            out.push_str(&format!("{}case {}:\n", self.ind(), n));
                        }
                        Pattern::Literal(lit) => {
                            out.push_str(&format!("{}case {}:\n", self.ind(), self.gen_expr(lit)));
                        }
                        Pattern::Variant(name, _) => {
                            out.push_str(&format!("{}case {}:\n", self.ind(), name));
                        }
                        _ => {
                            out.push_str(&format!("{}default: // pattern\n", self.ind()));
                        }
                    }
                    self.indent += 1;
                    // Emit body — if it's a block, emit statements; otherwise return
                    match &arm.body {
                        Expr::Block(block) => {
                            out.push_str(&self.gen_block(block));
                        }
                        other => {
                            out.push_str(&format!("{}return {}\n", self.ind(), self.gen_expr(other)));
                        }
                    }
                    self.indent -= 1;
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::StructLit { name, fields, .. } => {
                let fs: Vec<String> = fields.iter()
                    .map(|(k, v)| format!("{}: {}", k, self.gen_expr(v)))
                    .collect();
                format!("{}{{{}}}", name, fs.join(", "))
            }
            Expr::Block(block) => {
                let mut out = "{\n".to_string();
                self.indent += 1;
                out.push_str(&self.gen_block(block));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::Tuple(items) => {
                let i: Vec<String> = items.iter().map(|e| self.gen_expr(e)).collect();
                i.join(", ")
            }
            _ => "/* TODO */".into(),
        }
    }
}

// =============================================================================
// Swift Code Generator
// =============================================================================

struct SwiftGen {
    indent: usize,
}

impl SwiftGen {
    fn swift_pattern(&mut self, p: &Pattern) -> String {
        use Pattern::*;
        match p {
            Wildcard => "_".into(),
            Binding(n) => n.clone(),
            Literal(e) => self.gen_expr(e),
            Range(_, _, _) => "_".into(),
            Variant(_, _) => "_".into(),
        }
    }

    fn ind(&self) -> String { "    ".repeat(self.indent) }

    fn gen_program(&mut self, program: &Program) -> String {
        let mut out = String::new();
        for stmt in &program.statements { out.push_str(&self.gen_stmt(stmt)); out.push('\n'); }
        out
    }

    fn gen_stmt(&mut self, stmt: &Statement) -> String {
        match stmt {
            Statement::Let { name, value, .. } | Statement::Const { name, value, .. } => {
                format!("{}let {} = {}", self.ind(), name, self.gen_expr(value))
            }
            Statement::Mut { name, value, .. } => {
                format!("{}var {} = {}", self.ind(), name, self.gen_expr(value))
            }
            Statement::FnDecl { name, params, body, .. } => {
                let ps: Vec<String> = params.iter()
                    .filter(|p| p.name != "sf")
                    .map(|p| format!("_ {}: Any", p.name))
                    .collect();
                let mut out = format!("{}func {}({}) -> Any {{\n", self.ind(), name, ps.join(", "));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::StructDecl { name, fields, .. } => {
                let mut out = format!("{}struct {} {{\n", self.ind(), name);
                self.indent += 1;
                for f in fields { out.push_str(&format!("{}var {}: Any\n", self.ind(), f.name)); }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::EnumDecl { name, variants, .. } => {
                let mut out = format!("{}enum {} {{\n", self.ind(), name);
                self.indent += 1;
                for v in variants {
                    if v.fields.is_empty() { out.push_str(&format!("{}case {}\n", self.ind(), camelCase(&v.name))); }
                    else {
                        let fs: Vec<String> = v.fields.iter().enumerate()
                            .map(|(i, f)| f.name.clone().unwrap_or_else(|| format!("v{i}")))
                            .map(|n| format!("{n}: Any")).collect();
                        out.push_str(&format!("{}case {}({})\n", self.ind(), camelCase(&v.name), fs.join(", ")));
                    }
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::Expr(e) => format!("{}{}", self.ind(), self.gen_expr(e)),
            Statement::Return(Some(e)) => format!("{}return {}", self.ind(), self.gen_expr(e)),
            Statement::Return(None) => format!("{}return", self.ind()),
            Statement::Assignment { target, op, value } => {
                let o = match op {
                    AssignOp::Assign => "=", AssignOp::AddAssign => "+=",
                    AssignOp::SubAssign => "-=", AssignOp::MulAssign => "*=",
                    AssignOp::DivAssign => "/=", AssignOp::ModAssign => "%=",
                };
                format!("{}{} {} {}", self.ind(), self.gen_expr(target), o, self.gen_expr(value))
            }
            Statement::Break(_) => format!("{}break", self.ind()),
            Statement::Continue => format!("{}continue", self.ind()),
            Statement::Throw(Some(e)) => format!("{}throw {}", self.ind(), self.gen_expr(e)),
            Statement::Throw(None) => format!("{}throw NebulaError.unknown", self.ind()),
            Statement::LetDestructure { names, value, .. } => {
                format!("{}({}) = {}", self.ind(), names.join(", "), self.gen_expr(value))
            }
            // Iter-24 Bug D: explicit Use/Module handlers for Swift.
            Statement::Use { path, .. } => {
                format!("{}import {}", self.ind(), path.first().cloned().unwrap_or_default())
            }
            Statement::Module { .. } => {
                // Swift has no explicit module statement in source files.
                String::new()
            }
            _ => format!("{}// TODO", self.ind()),
        }
    }

    fn gen_block(&mut self, block: &Block) -> String {
        let mut out = String::new();
        for s in &block.statements { out.push_str(&self.gen_stmt(s)); out.push('\n'); }
        if let Some(e) = &block.expr {
            match e.as_ref() {
                Expr::If { .. } | Expr::Match { .. } | Expr::For { .. }
                | Expr::While { .. } | Expr::Loop { .. } | Expr::Block(_)
                | Expr::MethodCall { .. } | Expr::Call { .. } => {
                    out.push_str(&format!("{}{}\n", self.ind(), self.gen_expr(e)));
                }
                _ => {
                    out.push_str(&format!("{}return {}\n", self.ind(), self.gen_expr(e)));
                }
            }
        }
        out
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n) => n.to_string(),
            Expr::FloatLit(f) => format!("{f:?}"),
            Expr::BoolLit(b) => b.to_string(),
            Expr::StringLit(s) => {
                let mut out = String::new();
                let has_interp = s.parts.iter().any(|p| matches!(p, StringPart::Interpolation(_)));
                out.push('"');
                for part in &s.parts {
                    match part {
                        StringPart::Literal(t) => out.push_str(&t.replace('\n', "\\n").replace('\t', "\\t")),
                        StringPart::Interpolation(tokens) => {
                            out.push_str("\\(");
                            for t in tokens {
                                out.push_str(&interpolation_token_to_target(t, "self"));
                            }
                            out.push(')');
                        }
                    }
                }
                out.push('"');
                let _ = has_interp;
                out
            }
            Expr::Nil => "nil".into(),
            Expr::Ident(n) => if n == "sf" { "self".into() } else { n.clone() },
            Expr::BinaryOp { left, op, right } => {
                let o = match op {
                    BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/",
                    BinOp::Mod => "%", BinOp::Eq => "==", BinOp::NotEq => "!=",
                    BinOp::Less => "<", BinOp::Greater => ">", BinOp::LessEq => "<=",
                    BinOp::GreaterEq => ">=", BinOp::And => "&&", BinOp::Or => "||",
                    BinOp::Pow => "/* pow */", _ => "/* op */",
                };
                format!("({} {} {})", self.gen_expr(left), o, self.gen_expr(right))
            }
            Expr::UnaryOp { op, operand } => {
                let o = match op { UnaryOp::Neg => "-", UnaryOp::Not => "!", UnaryOp::BitNot => "~" };
                format!("{}{}", o, self.gen_expr(operand))
            }
            Expr::Call { callee, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let c = self.gen_expr(callee);
                if c == "prt" || c == "print" || c == "println" { format!("print({})", a.join(", ")) }
                else { format!("{}({})", c, a.join(", ")) }
            }
            Expr::MethodCall { object, method, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                format!("{}.{}({})", self.gen_expr(object), method, a.join(", "))
            }
            Expr::FieldAccess { object, field } => format!("{}.{}", self.gen_expr(object), field),
            Expr::Index { object, index } => format!("{}[{}]", self.gen_expr(object), self.gen_expr(index)),
            Expr::Slice { object, start, end } => {
                // Swift: Array slicing via `xs[a..<b]` / `xs[..<b]` / `xs[a...]` / `xs[...]`.
                let obj = self.gen_expr(object);
                match (start.as_ref(), end.as_ref()) {
                    (None, None) => format!("{}[...]", obj),
                    (Some(s), None) => format!("{}[{}...]", obj, self.gen_expr(s)),
                    (None, Some(e)) => format!("{}[..<{}]", obj, self.gen_expr(e)),
                    (Some(s), Some(e)) => format!("{}[{}..<{}]", obj, self.gen_expr(s), self.gen_expr(e)),
                }
            }
            Expr::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|p| format!("{}: Any", p.name)).collect();
                format!("{{ ({}) -> Any in {} }}", ps.join(", "), self.gen_expr(body))
            }
            Expr::If { condition, then_block, else_block } => {
                if let (Some(te), Some(eb)) = (&then_block.expr, else_block) {
                    if then_block.statements.is_empty() {
                        if let Some(ee) = &eb.expr { if eb.statements.is_empty() {
                            return format!("({} ? {} : {})", self.gen_expr(condition), self.gen_expr(te), self.gen_expr(ee));
                        }}
                    }
                }
                let mut out = format!("if {} {{\n", self.gen_expr(condition));
                self.indent += 1; out.push_str(&self.gen_block(then_block)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                if let Some(eb) = else_block { out.push_str(" else {\n"); self.indent += 1; out.push_str(&self.gen_block(eb)); self.indent -= 1; out.push_str(&format!("{}}}", self.ind())); }
                out
            }
            Expr::For { binding, index_binding, iterable, body } => {
                let bind = match index_binding {
                    Some(idx) => format!("({}, {})", idx, binding),
                    None => binding.clone(),
                };
                let mut out = format!("for {} in {} {{\n", bind, self.gen_expr(iterable));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::While { condition, body } => {
                let mut out = format!("while {} {{\n", self.gen_expr(condition));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::Loop { body } => {
                let mut out: String = "while true {\n".into();
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::List(items) => {
                let i: Vec<String> = items.iter().map(|e| self.gen_expr(e)).collect();
                format!("[{}]", i.join(", "))
            }
            Expr::Map(entries) => {
                if entries.is_empty() {
                    return "[:]".into();
                }
                let pairs: Vec<String> = entries.iter()
                    .map(|(k, v)| format!("{}: {}", self.gen_expr(k), self.gen_expr(v)))
                    .collect();
                format!("[{}]", pairs.join(", "))
            }
            Expr::TryCatch { try_block, error_binding, catch_block } => {
                let mut out = String::from("{ () -> Any in\n");
                self.indent += 1;
                out.push_str(&format!("{}do {{\n", self.ind()));
                self.indent += 1;
                out.push_str(&self.gen_block(try_block));
                self.indent -= 1;
                out.push_str(&format!("{}}} catch let {} {{\n", self.ind(), error_binding));
                self.indent += 1;
                out.push_str(&self.gen_block(catch_block));
                self.indent -= 1;
                out.push_str(&format!("{}}}\n", self.ind()));
                self.indent -= 1;
                out.push_str(&format!("{}}}()", self.ind()));
                out
            }
            Expr::NilCoalesce { left, right } => format!("({} ?? {})", self.gen_expr(left), self.gen_expr(right)),
            Expr::Range { start, end, inclusive } => {
                if *inclusive { format!("{}...{}", self.gen_expr(start), self.gen_expr(end)) }
                else { format!("{}..<{}", self.gen_expr(start), self.gen_expr(end)) }
            }
            Expr::Pipe { left, right } => {
                if let Expr::Ident(n) = right.as_ref() { format!("{}({})", n, self.gen_expr(left)) }
                else { format!("/* pipe */ {}", self.gen_expr(right)) }
            }
            Expr::Match { subject, arms } => {
                let subj = self.gen_expr(subject);
                let mut out = format!("{{ () -> Any in\n");
                self.indent += 1;
                out.push_str(&format!("{}switch {} {{\n", self.ind(), subj));
                self.indent += 1;
                let mut saw_default = false;
                for arm in arms {
                    match &arm.pattern {
                        Pattern::Wildcard | Pattern::Binding(_) => {
                            if let Some(guard) = &arm.guard {
                                out.push_str(&format!("{}case _ where {}:\n", self.ind(), self.gen_expr(guard)));
                            } else {
                                out.push_str(&format!("{}default:\n", self.ind()));
                                saw_default = true;
                            }
                        }
                        Pattern::Literal(lit) => {
                            out.push_str(&format!("{}case {}:\n", self.ind(), self.gen_expr(lit)));
                        }
                        Pattern::Range(start, end, inclusive) => {
                            let op = if *inclusive { "..." } else { "..<" };
                            out.push_str(&format!("{}case {}{}{}:\n", self.ind(), self.gen_expr(start), op, self.gen_expr(end)));
                        }
                        Pattern::Variant(name, subs) => {
                            let sub_strs: Vec<String> = subs.iter().map(|p| self.swift_pattern(p)).collect();
                            let guard = arm.guard.as_ref().map(|g| format!(" where {}", self.gen_expr(g))).unwrap_or_default();
                            if name == "(tuple)" {
                                // Tuple pattern `(a, b)`: render as Swift tuple
                                // pattern, binding unbound names with `let` and
                                // leaving literals/wildcards as-is.
                                let parts: Vec<String> = subs.iter().map(|p| match p {
                                    Pattern::Binding(n) => format!("let {}", n),
                                    Pattern::Wildcard => "_".into(),
                                    Pattern::Literal(e) => self.gen_expr(e),
                                    _ => self.swift_pattern(p),
                                }).collect();
                                out.push_str(&format!("{}case ({}){}:\n", self.ind(), parts.join(", "), guard));
                            } else {
                                // Enum pattern: `.variant` or `.variant(x, y)`
                                let last = name.rsplit('.').next().unwrap_or(name);
                                if sub_strs.is_empty() {
                                    out.push_str(&format!("{}case .{}{}:\n", self.ind(), last, guard));
                                } else {
                                    let bindings: Vec<String> = subs.iter().map(|p| match p {
                                        Pattern::Binding(n) => format!("let {}", n),
                                        Pattern::Wildcard => "_".into(),
                                        _ => self.swift_pattern(p),
                                    }).collect();
                                    out.push_str(&format!("{}case .{}({}){}:\n", self.ind(), last, bindings.join(", "), guard));
                                }
                            }
                        }
                    }
                    self.indent += 1;
                    out.push_str(&format!("{}return {}\n", self.ind(), self.gen_expr(&arm.body)));
                    self.indent -= 1;
                }
                // Swift switch must be exhaustive; add a default if none.
                if !saw_default {
                    out.push_str(&format!("{}default:\n", self.ind()));
                    self.indent += 1;
                    out.push_str(&format!("{}fatalError()\n", self.ind()));
                    self.indent -= 1;
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}\n", self.ind()));
                self.indent -= 1;
                out.push_str(&format!("{}}}()", self.ind()));
                out
            }
            _ => "/* TODO */".into(),
        }
    }
}

// =============================================================================
// Kotlin Code Generator
// =============================================================================

struct KotlinGen {
    indent: usize,
    scopes: Vec<std::collections::HashSet<String>>,
}

impl KotlinGen {
    fn ind(&self) -> String { "    ".repeat(self.indent) }

    fn is_declared(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }
    fn declare(&mut self, name: &str) {
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name.to_string());
        }
    }

    fn pascal_k(s: &str) -> String {
        let camel = snake_to_camel(s);
        let mut cs = camel.chars();
        match cs.next() {
            Some(c) => c.to_uppercase().chain(cs).collect(),
            None => String::new(),
        }
    }

    /// Heuristic for names the ingester lowered from Java/Kotlin Pascal types.
    fn is_likely_java_type(s: &str) -> bool {
        // Bail out if the caller already has a dot (qualified path).
        if s.contains('.') { return false; }
        // Simple rule: snake_case with more than one segment is probably a
        // Pascal type broken apart.
        let segs: Vec<&str> = s.split('_').collect();
        segs.len() >= 2 && segs.iter().all(|s| !s.is_empty())
    }

    /// Walk a type expr and collect names that look like generic params
    /// (single uppercase letter, or uppercase-only idents).
    fn collect_generics_k(t: &TypeExpr, out: &mut std::collections::BTreeSet<String>) {
        const KNOWN: &[&str] = &["String", "Str", "Bool", "Boolean", "Char",
            "Int", "Long", "Float", "Double", "Number", "Nil", "List", "Array",
            "Map", "Dictionary", "Set", "Any", "Unit", "None", "Pair", "Triple"];
        match t {
            TypeExpr::Named(n) => {
                if n.len() == 1 && n.chars().next().unwrap().is_ascii_uppercase() {
                    out.insert(n.clone());
                } else if !KNOWN.contains(&n.as_str()) && !n.contains('.')
                    && n.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                    && !n.is_empty()
                {
                    out.insert(n.clone());
                }
            }
            TypeExpr::Generic(_, args) => { for a in args { Self::collect_generics_k(a, out); } }
            TypeExpr::Option(inner) => Self::collect_generics_k(inner, out),
            TypeExpr::Function(params, ret) => {
                for p in params { Self::collect_generics_k(p, out); }
                Self::collect_generics_k(ret, out);
            }
            TypeExpr::Tuple(items) => { for i in items { Self::collect_generics_k(i, out); } }
        }
    }

    fn gen_program(&mut self, program: &Program) -> String {
        let mut out = String::new();
        for stmt in &program.statements { out.push_str(&self.gen_stmt(stmt)); out.push('\n'); }
        out
    }

    fn neb_type(&self, t: &TypeExpr) -> String {
        match t {
            TypeExpr::Named(n) => match n.as_str() {
                "Int" => "Int".into(),
                "Long" => "Long".into(),
                "Float" => "Float".into(),
                "Double" => "Double".into(),
                "Number" => "Double".into(),
                "Str" | "String" => "String".into(),
                "Char" => "Char".into(),
                "Bool" | "Boolean" => "Boolean".into(),
                "List" | "Array" => "List<Any>".into(),
                "Map" | "Dictionary" => "MutableMap<Any, Any>".into(),
                "Set" => "Set<Any>".into(),
                "Any" => "Any".into(),
                "None" | "Unit" => "Unit".into(),
                // A lowercase name with an underscore is almost certainly a
                // Kotlin snake→PascalCase rename casualty from ingest.
                other if other.chars().next().map_or(false, |c| c.is_ascii_lowercase())
                    && other.contains('_') => Self::pascal_k(other),
                other => other.to_string(),
            },
            TypeExpr::Generic(name, args) => {
                let a: Vec<String> = args.iter().map(|a| self.neb_type(a)).collect();
                match name.as_str() {
                    "List" | "Array" => format!("List<{}>", a.first().cloned().unwrap_or_else(|| "Any".into())),
                    "Map" | "Dictionary" => format!("MutableMap<{}, {}>", a.first().cloned().unwrap_or_else(|| "Any".into()), a.get(1).cloned().unwrap_or_else(|| "Any".into())),
                    "Set" => format!("Set<{}>", a.first().cloned().unwrap_or_else(|| "Any".into())),
                    "Option" | "Optional" => format!("{}?", a.first().cloned().unwrap_or_else(|| "Any".into())),
                    "pair" | "Pair" => match a.len() {
                        2 => format!("Pair<{}, {}>", a[0], a[1]),
                        _ => format!("Pair<{}>", a.join(", ")),
                    },
                    "triple" | "Triple" => match a.len() {
                        3 => format!("Triple<{}, {}, {}>", a[0], a[1], a[2]),
                        _ => format!("Triple<{}>", a.join(", ")),
                    },
                    _ => format!("{}<{}>", name, a.join(", ")),
                }
            }
            TypeExpr::Option(inner) => format!("{}?", self.neb_type(inner)),
            TypeExpr::Function(params, ret) => {
                let ps: Vec<String> = params.iter().map(|p| self.neb_type(p)).collect();
                format!("({}) -> {}", ps.join(", "), self.neb_type(ret))
            }
            TypeExpr::Tuple(items) => {
                match items.len() {
                    2 => format!("Pair<{}, {}>", self.neb_type(&items[0]), self.neb_type(&items[1])),
                    3 => format!("Triple<{}, {}, {}>", self.neb_type(&items[0]), self.neb_type(&items[1]), self.neb_type(&items[2])),
                    _ => format!("List<Any>"),
                }
            }
        }
    }

    fn gen_stmt(&mut self, stmt: &Statement) -> String {
        match stmt {
            Statement::Let { name, value, type_ann } | Statement::Const { name, value, type_ann } => {
                let v = self.gen_expr(value);
                self.declare(name);
                let t = type_ann.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                format!("{}val {}{} = {}", self.ind(), name, t, v)
            }
            Statement::Mut { name, value, type_ann } => {
                let v = self.gen_expr(value);
                self.declare(name);
                let t = type_ann.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                format!("{}var {}{} = {}", self.ind(), name, t, v)
            }
            Statement::FnDecl { name, params, body, return_type, .. } => {
                let ps: Vec<String> = params.iter()
                    .filter(|p| p.name != "sf")
                    .map(|p| {
                        let t = p.type_ann.as_ref().map(|t| self.neb_type(t)).unwrap_or_else(|| "Any".into());
                        format!("{}: {}", p.name, t)
                    })
                    .collect();
                let ret = return_type.as_ref().map(|t| self.neb_type(t)).unwrap_or_else(|| "Any".into());
                // Recover stripped generic type params from the signature.
                let mut generic_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                for p in params.iter().filter(|p| p.name != "sf") {
                    if let Some(t) = &p.type_ann { Self::collect_generics_k(t, &mut generic_set); }
                }
                if let Some(rt) = return_type { Self::collect_generics_k(rt, &mut generic_set); }
                let gen_str = if generic_set.is_empty() { String::new() }
                    else { format!(" <{}>", generic_set.iter().cloned().collect::<Vec<_>>().join(", ")) };
                let body_empty = body.statements.is_empty() && body.expr.is_none();
                // Kotlin: `fun <T> name(...)` — generics come BEFORE the name.
                let mut out = format!("{}fun{} {}({}): {} {{\n", self.ind(), gen_str, name, ps.join(", "), ret);
                self.indent += 1;
                self.scopes.push(std::collections::HashSet::new());
                for p in params { self.declare(&p.name); }
                let body_str = self.gen_fn_block(body);
                self.scopes.pop();
                out.push_str(&body_str);
                if body_empty {
                    out.push_str(&format!("{}return Unit\n", self.ind()));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::StructDecl { name, fields, .. } => {
                // Kotlin `data class` requires at least one primary-ctor
                // param. Emit a regular `class` with no body for empty
                // structs (placeholder `_empty: Any` from upstream pad).
                let real_fields: Vec<&crate::ast::Field> = fields.iter()
                    .filter(|f| !(f.name == "_empty" && matches!(&f.type_ann,
                        crate::ast::TypeExpr::Named(n) if n == "Any")))
                    .collect();
                if real_fields.is_empty() {
                    return format!("{}class {}", self.ind(), name);
                }
                let fs: Vec<String> = real_fields.iter().map(|f| format!("val {}: Any", f.name)).collect();
                format!("{}data class {}({})", self.ind(), name, fs.join(", "))
            }
            Statement::EnumDecl { name, variants, .. } => {
                let mut out = format!("{}sealed class {} {{\n", self.ind(), name);
                self.indent += 1;
                for v in variants {
                    if v.fields.is_empty() {
                        out.push_str(&format!("{}object {} : {}()\n", self.ind(), v.name, name));
                    } else {
                        let fs: Vec<String> = v.fields.iter().enumerate()
                            .map(|(i, f)| format!("val {}: Any", f.name.clone().unwrap_or_else(|| format!("v{i}"))))
                            .collect();
                        out.push_str(&format!("{}data class {}({}) : {}()\n", self.ind(), v.name, fs.join(", "), name));
                    }
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::Expr(e) => format!("{}{}", self.ind(), self.gen_expr(e)),
            Statement::Return(Some(e)) => format!("{}return {}", self.ind(), self.gen_expr(e)),
            Statement::Return(None) => format!("{}return", self.ind()),
            Statement::Assignment { target, op, value } => {
                let o = match op {
                    AssignOp::Assign => "=", AssignOp::AddAssign => "+=",
                    AssignOp::SubAssign => "-=", AssignOp::MulAssign => "*=",
                    AssignOp::DivAssign => "/=", AssignOp::ModAssign => "%=",
                };
                if matches!(op, AssignOp::Assign) {
                    if let Expr::Ident(name) = target {
                        if !self.is_declared(name) {
                            let v = self.gen_expr(value);
                            self.declare(name);
                            return format!("{}var {} = {}", self.ind(), name, v);
                        }
                    }
                }
                format!("{}{} {} {}", self.ind(), self.gen_expr(target), o, self.gen_expr(value))
            }
            Statement::Break(_) => format!("{}break", self.ind()),
            Statement::Continue => format!("{}continue", self.ind()),
            Statement::Throw(Some(e)) => format!("{}throw {}", self.ind(), self.gen_expr(e)),
            Statement::Throw(None) => format!("{}throw RuntimeException(\"error\")", self.ind()),
            Statement::LetDestructure { names, value, .. } => {
                format!("{}({}) = {}", self.ind(), names.join(", "), self.gen_expr(value))
            }
            // Iter-24 Bug D: explicit Use/Module handlers for Kotlin.
            Statement::Use { path, .. } => {
                // PascalCase the last segment for Java/Kotlin library imports
                // (e.g. `java.util.array_list` → `java.util.ArrayList`), but
                // preserve wildcard/short package names.
                let mut parts: Vec<String> = path.iter().cloned().collect();
                if let Some(last) = parts.last_mut() {
                    if last != "*" && last.chars().next().map_or(false, |c| c.is_ascii_lowercase()) {
                        *last = Self::pascal_k(last);
                    }
                }
                format!("{}import {}", self.ind(), parts.join("."))
            }
            Statement::Module { path } => {
                format!("{}package {}", self.ind(), path.join("."))
            }
            Statement::TraitDecl { name, methods, generics, .. } => {
                // Collect generics from the trait decl AND any references in
                // its method signatures that aren't explicitly declared, so
                // `interface Producer { fun produce(): T }` → `interface Producer<T>`.
                let mut gen_set: std::collections::BTreeSet<String> = generics.iter().cloned().collect();
                for m in methods {
                    for p in m.params.iter().filter(|p| p.name != "sf") {
                        if let Some(t) = &p.type_ann { Self::collect_generics_k(t, &mut gen_set); }
                    }
                    if let Some(rt) = &m.return_type { Self::collect_generics_k(rt, &mut gen_set); }
                }
                let gen = if gen_set.is_empty() { String::new() }
                    else { format!("<{}>", gen_set.iter().cloned().collect::<Vec<_>>().join(", ")) };
                let mut out = format!("{}interface {}{} {{\n", self.ind(), name, gen);
                self.indent += 1;
                for m in methods {
                    let ps: Vec<String> = m.params.iter().filter(|p| p.name != "sf").map(|p| {
                        let t = p.type_ann.as_ref().map(|t| self.neb_type(t)).unwrap_or_else(|| "Any".into());
                        format!("{}: {}", p.name, t)
                    }).collect();
                    let ret = m.return_type.as_ref().map(|t| self.neb_type(t)).unwrap_or_else(|| "Any".into());
                    match &m.body {
                        Some(b) => {
                            let body_empty = b.statements.is_empty() && b.expr.is_none();
                            if body_empty {
                                out.push_str(&format!("{}fun {}({}): {}\n", self.ind(), m.name, ps.join(", "), ret));
                            } else {
                                out.push_str(&format!("{}fun {}({}): {} {{\n", self.ind(), m.name, ps.join(", "), ret));
                                self.indent += 1;
                                self.scopes.push(std::collections::HashSet::new());
                                for p in &m.params { self.declare(&p.name); }
                                let bs = self.gen_fn_block(b);
                                self.scopes.pop();
                                out.push_str(&bs);
                                self.indent -= 1;
                                out.push_str(&format!("{}}}\n", self.ind()));
                            }
                        }
                        None => {
                            out.push_str(&format!("{}fun {}({}): {}\n", self.ind(), m.name, ps.join(", "), ret));
                        }
                    }
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::ImplBlock { target, methods, .. } => {
                let mut out = String::new();
                for m in methods {
                    if let Statement::FnDecl { name: mname, params, body, return_type, .. } = m {
                        let ps: Vec<String> = params.iter().filter(|p| p.name != "sf").map(|p| {
                            let t = p.type_ann.as_ref().map(|t| self.neb_type(t)).unwrap_or_else(|| "Any".into());
                            format!("{}: {}", p.name, t)
                        }).collect();
                        let ret = return_type.as_ref().map(|t| self.neb_type(t)).unwrap_or_else(|| "Any".into());
                        let has_self = params.iter().any(|p| p.name == "sf");
                        // Recover generics from method signature.
                        let mut gen_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                        for p in params.iter().filter(|p| p.name != "sf") {
                            if let Some(t) = &p.type_ann { Self::collect_generics_k(t, &mut gen_set); }
                        }
                        if let Some(rt) = return_type { Self::collect_generics_k(rt, &mut gen_set); }
                        let gen_str = if gen_set.is_empty() { String::new() }
                            else { format!(" <{}>", gen_set.iter().cloned().collect::<Vec<_>>().join(", ")) };
                        let prefix = if has_self {
                            format!("fun{} {}.{}", gen_str, target, mname)
                        } else {
                            format!("fun{} {}_{}", gen_str, target, mname)
                        };
                        let body_empty = body.statements.is_empty() && body.expr.is_none();
                        out.push_str(&format!("{}{}({}): {} {{\n", self.ind(), prefix, ps.join(", "), ret));
                        self.indent += 1;
                        self.scopes.push(std::collections::HashSet::new());
                        for p in params { self.declare(&p.name); }
                        let bs = self.gen_fn_block(body);
                        self.scopes.pop();
                        out.push_str(&bs);
                        if body_empty {
                            out.push_str(&format!("{}return Unit\n", self.ind()));
                        }
                        self.indent -= 1;
                        out.push_str(&format!("{}}}\n", self.ind()));
                    }
                }
                out.trim_end_matches('\n').to_string()
            }
            _ => format!("{}// TODO", self.ind()),
        }
    }

    fn gen_block(&mut self, block: &Block) -> String {
        self.gen_block_inner(block, false)
    }

    fn gen_fn_block(&mut self, block: &Block) -> String {
        self.gen_block_inner(block, true)
    }

    fn gen_block_inner(&mut self, block: &Block, is_fn_body: bool) -> String {
        self.scopes.push(std::collections::HashSet::new());
        let mut out = String::new();
        for s in &block.statements { out.push_str(&self.gen_stmt(s)); out.push('\n'); }
        if let Some(e) = &block.expr {
            // Kotlin `if`, `when`, `try` are expressions; in a function body
            // they need an explicit `return` to produce the fn's result.
            // `for`/`while`/`loop` genuinely produce Unit and need none.
            let needs_return = is_fn_body && !matches!(e.as_ref(),
                Expr::For { .. } | Expr::While { .. } | Expr::Loop { .. }
                | Expr::Block(_)
            );
            if needs_return {
                out.push_str(&format!("{}return {}\n", self.ind(), self.gen_expr(e)));
            } else {
                out.push_str(&format!("{}{}\n", self.ind(), self.gen_expr(e)));
            }
        }
        self.scopes.pop();
        out
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n) => n.to_string(),
            Expr::FloatLit(f) => format!("{f:?}"),
            Expr::BoolLit(b) => b.to_string(),
            Expr::StringLit(s) => {
                let mut out = String::from("\"");
                for part in &s.parts {
                    match part {
                        StringPart::Literal(t) => out.push_str(&t.replace('\n', "\\n").replace('\t', "\\t")),
                        StringPart::Interpolation(tokens) => {
                            out.push_str("${");
                            for t in tokens {
                                out.push_str(&interpolation_token_to_target(t, "this"));
                            }
                            out.push('}');
                        }
                    }
                }
                out.push('"');
                out
            }
            Expr::Nil => "null".into(),
            Expr::Ident(n) => if n == "sf" { "this".into() } else { n.clone() },
            Expr::BinaryOp { left, op, right } => {
                let o = match op {
                    BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/",
                    BinOp::Mod => "%", BinOp::Eq => "==", BinOp::NotEq => "!=",
                    BinOp::Less => "<", BinOp::Greater => ">", BinOp::LessEq => "<=",
                    BinOp::GreaterEq => ">=", BinOp::And => "&&", BinOp::Or => "||",
                    BinOp::Pow => "/* pow */", _ => "/* op */",
                };
                format!("({} {} {})", self.gen_expr(left), o, self.gen_expr(right))
            }
            Expr::UnaryOp { op, operand } => {
                let o = match op { UnaryOp::Neg => "-", UnaryOp::Not => "!", UnaryOp::BitNot => "inv " };
                format!("{}{}", o, self.gen_expr(operand))
            }
            Expr::Call { callee, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let c = self.gen_expr(callee);
                if c == "prt" || c == "print" || c == "println" { format!("println({})", a.join(", ")) }
                else {
                    let converted = match c.as_str() {
                        "mutable_list_of" => "mutableListOf".into(),
                        "mutable_map_of" => "mutableMapOf".into(),
                        "mutable_set_of" => "mutableSetOf".into(),
                        "list_of" => "listOf".into(),
                        "map_of" => "mapOf".into(),
                        "set_of" => "setOf".into(),
                        "array_of" => "arrayOf".into(),
                        "int_array_of" => "intArrayOf".into(),
                        "array_list" => "ArrayList".into(),
                        "local_date" => "LocalDate".into(),
                        "local_time" => "LocalTime".into(),
                        "local_date_time" => "LocalDateTime".into(),
                        // Pass through — constructor calls that look like
                        // snake_case PascalCase types (e.g. `hash_map`).
                        s if s.contains('_') && s.chars().next().map_or(false, |c| c.is_ascii_lowercase())
                            && Self::is_likely_java_type(s) => Self::pascal_k(s),
                        _ => c,
                    };
                    format!("{}({})", converted, a.join(", "))
                }
            }
            Expr::MethodCall { object, method, args } => {
                let a: Vec<String> = args.iter().map(|ca| {
                    let v = self.gen_expr(&ca.value);
                    if let Some(n) = &ca.name { format!("{} = {}", n, v) } else { v }
                }).collect();
                let m_owned: String = match method.as_str() {
                    "to_upper" => "uppercase".into(),
                    "to_lower" => "lowercase".into(),
                    "with_index" => "withIndex".into(),
                    "to_list" => "toList".into(),
                    "to_set" => "toSet".into(),
                    "to_string" => "toString".into(),
                    "step_by" => "step".into(),
                    "is_empty" => "isEmpty".into(),
                    "to_int" => "toInt".into(),
                    "to_double" => "toDouble".into(),
                    "to_float" => "toFloat".into(),
                    "is_na_n" => "isNaN".into(),
                    other => snake_to_camel(other),
                };
                format!("{}.{}({})", self.gen_expr(object), m_owned, a.join(", "))
            }
            Expr::Tuple(items) => {
                let i: Vec<String> = items.iter().map(|e| self.gen_expr(e)).collect();
                match items.len() {
                    2 => format!("Pair({})", i.join(", ")),
                    3 => format!("Triple({})", i.join(", ")),
                    _ => format!("listOf({})", i.join(", ")),
                }
            }
            Expr::Map(entries) => {
                if entries.is_empty() {
                    return "mutableMapOf<Any, Any>()".into();
                }
                let pairs: Vec<String> = entries.iter()
                    .map(|(k, v)| format!("{} to {}", self.gen_expr(k), self.gen_expr(v)))
                    .collect();
                format!("mutableMapOf({})", pairs.join(", "))
            }
            Expr::TryCatch { try_block, error_binding, catch_block } => {
                let mut out = String::from("try {\n");
                self.indent += 1; out.push_str(&self.gen_block(try_block)); self.indent -= 1;
                out.push_str(&format!("{}}} catch ({}: Exception) {{\n", self.ind(), error_binding));
                self.indent += 1; out.push_str(&self.gen_block(catch_block)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::FieldAccess { object, field } => format!("{}.{}", self.gen_expr(object), field),
            Expr::Index { object, index } => format!("{}[{}]", self.gen_expr(object), self.gen_expr(index)),
            Expr::Slice { object, start, end } => {
                // Kotlin: no subscript slice; use subList(a, b) for List<T>.
                // `xs.subList(a, xs.size)` requires re-evaluating xs — acceptable for
                // idiomatic uses; complex objects with side effects should be bound
                // to a local var upstream.
                let obj = self.gen_expr(object);
                match (start.as_ref(), end.as_ref()) {
                    (None, None) => format!("{}.toList()", obj),
                    (Some(s), None) => format!("{}.subList({}, {}.size)", obj, self.gen_expr(s), obj),
                    (None, Some(e)) => format!("{}.subList(0, {})", obj, self.gen_expr(e)),
                    (Some(s), Some(e)) => format!("{}.subList({}, {})", obj, self.gen_expr(s), self.gen_expr(e)),
                }
            }
            Expr::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                if ps.is_empty() { format!("{{ {} }}", self.gen_expr(body)) }
                else { format!("{{ {} -> {} }}", ps.join(", "), self.gen_expr(body)) }
            }
            Expr::If { condition, then_block, else_block } => {
                if let (Some(te), Some(eb)) = (&then_block.expr, else_block) {
                    if then_block.statements.is_empty() {
                        if let Some(ee) = &eb.expr { if eb.statements.is_empty() {
                            return format!("if ({}) {} else {}", self.gen_expr(condition), self.gen_expr(te), self.gen_expr(ee));
                        }}
                    }
                }
                let mut out = format!("if ({}) {{\n", self.gen_expr(condition));
                self.indent += 1; out.push_str(&self.gen_block(then_block)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                if let Some(eb) = else_block { out.push_str(" else {\n"); self.indent += 1; out.push_str(&self.gen_block(eb)); self.indent -= 1; out.push_str(&format!("{}}}", self.ind())); }
                out
            }
            Expr::For { binding, index_binding, iterable, body } => {
                let bind = match index_binding {
                    Some(idx) => format!("({}, {})", idx, binding),
                    None => binding.clone(),
                };
                let mut out = format!("for ({} in {}) {{\n", bind, self.gen_expr(iterable));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::While { condition, body } => {
                let mut out = format!("while ({}) {{\n", self.gen_expr(condition));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::Loop { body } => {
                let mut out: String = "while (true) {\n".into();
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::List(items) => {
                let i: Vec<String> = items.iter().map(|e| self.gen_expr(e)).collect();
                if items.is_empty() {
                    "mutableListOf<Any>()".into()
                } else {
                    format!("mutableListOf({})", i.join(", "))
                }
            }
            Expr::NilCoalesce { left, right } => format!("({} ?: {})", self.gen_expr(left), self.gen_expr(right)),
            Expr::Range { start, end, inclusive } => {
                if *inclusive { format!("({}..{})", self.gen_expr(start), self.gen_expr(end)) }
                else { format!("({} until {})", self.gen_expr(start), self.gen_expr(end)) }
            }
            Expr::Pipe { left, right } => {
                if let Expr::Ident(n) = right.as_ref() { format!("{}({})", n, self.gen_expr(left)) }
                else { format!("/* pipe */ {}", self.gen_expr(right)) }
            }
            Expr::Match { subject, arms } => {
                // Check if we have guards — use when {} (no subject) for if/else chain
                let has_guards = arms.iter().any(|a| a.guard.is_some());
                if has_guards {
                    let mut out = format!("when {{\n");
                    self.indent += 1;
                    for arm in arms {
                        if let Some(guard) = &arm.guard {
                            out.push_str(&format!("{}{} -> {}\n", self.ind(), self.gen_expr(guard), self.gen_expr(&arm.body)));
                        } else {
                            out.push_str(&format!("{}else -> {}\n", self.ind(), self.gen_expr(&arm.body)));
                        }
                    }
                    self.indent -= 1;
                    out.push_str(&format!("{}}}", self.ind()));
                    return out;
                }

                let subj = self.gen_expr(subject);
                let mut out = format!("when ({}) {{\n", subj);
                self.indent += 1;
                for arm in arms {
                    match &arm.pattern {
                        Pattern::Wildcard | Pattern::Binding(_) => {
                            out.push_str(&format!("{}else -> {}\n", self.ind(), self.gen_expr(&arm.body)));
                        }
                        Pattern::Literal(lit) => {
                            out.push_str(&format!("{}{} -> {}\n", self.ind(), self.gen_expr(lit), self.gen_expr(&arm.body)));
                        }
                        Pattern::Range(start, end, inclusive) => {
                            let op = if *inclusive { ".." } else { " until " };
                            out.push_str(&format!("{}in {}{}{} -> {}\n", self.ind(), self.gen_expr(start), op, self.gen_expr(end), self.gen_expr(&arm.body)));
                        }
                        Pattern::Variant(name, _subs) => {
                            // `Enum.variant` → Kotlin uses `is Enum.Variant` for sealed
                            // or plain `Enum.VARIANT` for enum-class matches. Heuristic:
                            // if the name path looks like `Enum.Name` with a capitalized
                            // last segment, emit that literal; otherwise add `is` prefix.
                            let path = name.replace('.', ".");
                            out.push_str(&format!("{}{} -> {}\n", self.ind(), path, self.gen_expr(&arm.body)));
                        }
                    }
                }
                // Kotlin's `when` as expression must be exhaustive; add a
                // safety else branch if we didn't emit one.
                let has_else = arms.iter().any(|a| matches!(a.pattern, Pattern::Wildcard | Pattern::Binding(_)));
                if !has_else {
                    out.push_str(&format!("{}else -> throw IllegalStateException()\n", self.ind()));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            _ => "/* TODO */".into(),
        }
    }
}

// =============================================================================
// TypeScript Code Generator
// =============================================================================

struct TsGen {
    indent: usize,
}

impl TsGen {
    fn ind(&self) -> String { "  ".repeat(self.indent) }

    fn gen_program(&mut self, program: &Program) -> String {
        let mut out = String::new();
        for stmt in &program.statements { out.push_str(&self.gen_stmt(stmt)); out.push('\n'); }
        out
    }

    fn neb_type(&self, t: &TypeExpr) -> String {
        match t {
            TypeExpr::Named(n) => match n.as_str() {
                "Int" | "Float" => "number".into(),
                "String" | "Char" => "string".into(),
                "Bool" => "boolean".into(),
                "List" => "any[]".into(),
                "Map" => "Record<string, any>".into(),
                other => other.to_string(),
            },
            TypeExpr::Generic(name, args) => {
                let a: Vec<String> = args.iter().map(|a| self.neb_type(a)).collect();
                match name.as_str() {
                    "List" => format!("{}[]", a.first().unwrap_or(&"any".into())),
                    "Map" => format!("Record<{}, {}>", a.first().unwrap_or(&"string".into()), a.get(1).unwrap_or(&"any".into())),
                    _ => format!("{}<{}>", name, a.join(", ")),
                }
            }
            TypeExpr::Option(inner) => format!("{} | null", self.neb_type(inner)),
            TypeExpr::Function(params, ret) => {
                let ps: Vec<String> = params.iter().enumerate().map(|(i, p)| format!("a{}: {}", i, self.neb_type(p))).collect();
                format!("({}) => {}", ps.join(", "), self.neb_type(ret))
            }
            TypeExpr::Tuple(items) => {
                let is: Vec<String> = items.iter().map(|i| self.neb_type(i)).collect();
                format!("[{}]", is.join(", "))
            }
        }
    }

    fn typed_param(&mut self, p: &Param) -> String {
        let t = p.type_ann.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
        if let Some(d) = &p.default {
            format!("{}{} = {}", p.name, t, self.gen_expr(d))
        } else {
            format!("{}{}", p.name, t)
        }
    }

    fn gen_stmt(&mut self, stmt: &Statement) -> String {
        match stmt {
            Statement::Let { name, type_ann, value } => {
                let t = type_ann.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                format!("{}const {}{} = {};", self.ind(), name, t, self.gen_expr(value))
            }
            Statement::Mut { name, type_ann, value } => {
                let t = type_ann.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                format!("{}let {}{} = {};", self.ind(), name, t, self.gen_expr(value))
            }
            Statement::Const { name, type_ann, value } => {
                let t = type_ann.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                format!("{}const {}{} = {};", self.ind(), name, t, self.gen_expr(value))
            }
            Statement::FnDecl { name, params, return_type, body, .. } => {
                let ps: Vec<String> = params.iter().filter(|p| p.name != "sf").map(|p| self.typed_param(p)).collect();
                let ret = return_type.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                let mut out = format!("{}function {}({}){} {{\n", self.ind(), name, ps.join(", "), ret);
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::StructDecl { name, fields, .. } => {
                let mut out = format!("{}interface {} {{\n", self.ind(), name);
                self.indent += 1;
                for f in fields {
                    out.push_str(&format!("{}{}: {};\n", self.ind(), f.name, self.neb_type(&f.type_ann)));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}\n\n", self.ind()));
                // Factory function
                let ps: Vec<String> = fields.iter().map(|f| format!("{}: {}", f.name, self.neb_type(&f.type_ann))).collect();
                let fs: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                out.push_str(&format!("{}function create{}({}): {} {{\n", self.ind(), name, ps.join(", "), name));
                self.indent += 1;
                out.push_str(&format!("{}return {{ {} }};\n", self.ind(), fs.join(", ")));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::ImplBlock { target, methods, .. } => {
                let mut out = String::new();
                for m in methods {
                    if let Statement::FnDecl { name: mname, params, return_type, body, .. } = m {
                        let has_self = params.iter().any(|p| p.name == "sf");
                        let ps: Vec<String> = params.iter().filter(|p| p.name != "sf").map(|p| self.typed_param(p)).collect();
                        let ret = return_type.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                        let self_param = if has_self { format!("self: {}", target) } else { String::new() };
                        let all_ps = if has_self && !ps.is_empty() {
                            format!("{}, {}", self_param, ps.join(", "))
                        } else if has_self {
                            self_param
                        } else {
                            ps.join(", ")
                        };
                        let static_prefix = if has_self { "" } else { "/* static */ " };
                        out.push_str(&format!("{}function {}{}({}){} {{\n", self.ind(), static_prefix, mname, all_ps, ret));
                        self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                        out.push_str(&format!("{}}}\n", self.ind()));
                    }
                }
                out
            }
            Statement::EnumDecl { name, variants, .. } => {
                let simple = variants.iter().all(|v| v.fields.is_empty());
                if simple {
                    let vs: Vec<String> = variants.iter().map(|v| format!("  {} = \"{}\"", v.name, v.name)).collect();
                    format!("{}enum {} {{\n{}\n{}}}", self.ind(), name, vs.join(",\n"), self.ind())
                } else {
                    format!("{}type {} = /* tagged union */;", self.ind(), name)
                }
            }
            Statement::TraitDecl { name, methods, .. } => {
                let mut out = format!("{}interface {} {{\n", self.ind(), name);
                self.indent += 1;
                for m in methods {
                    let ps: Vec<String> = m.params.iter().filter(|p| p.name != "sf").map(|p| self.typed_param(p)).collect();
                    let ret = m.return_type.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                    out.push_str(&format!("{}{}({}){};{}\n", self.ind(), m.name, ps.join(", "), ret, if m.body.is_some() { " // has default" } else { "" }));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::Expr(e) => format!("{}{};", self.ind(), self.gen_expr(e)),
            Statement::Return(Some(e)) => format!("{}return {};", self.ind(), self.gen_expr(e)),
            Statement::Return(None) => format!("{}return;", self.ind()),
            Statement::Break(_) => format!("{}break;", self.ind()),
            Statement::Continue => format!("{}continue;", self.ind()),
            Statement::Throw(Some(e)) => format!("{}throw {};", self.ind(), self.gen_expr(e)),
            Statement::Throw(None) => format!("{}throw new Error();", self.ind()),
            Statement::Assignment { target, op, value } => {
                let o = match op { AssignOp::Assign => "=", AssignOp::AddAssign => "+=", AssignOp::SubAssign => "-=", AssignOp::MulAssign => "*=", AssignOp::DivAssign => "/=", AssignOp::ModAssign => "%=" };
                format!("{}{} {} {};", self.ind(), self.gen_expr(target), o, self.gen_expr(value))
            }
            Statement::Use { path, .. } => format!("{}// import {}", self.ind(), path.join(".")),
            Statement::LetDestructure { names, value, .. } => {
                format!("{}const [{}] = {};", self.ind(), names.join(", "), self.gen_expr(value))
            }
            Statement::TypeAlias { name, type_expr } => {
                format!("{}type {} = {};", self.ind(), name, self.neb_type(type_expr))
            }
            Statement::Module { .. } => String::new(),
        }
    }

    fn gen_block(&mut self, block: &Block) -> String {
        let mut out = String::new();
        for s in &block.statements { out.push_str(&self.gen_stmt(s)); out.push('\n'); }
        if let Some(e) = &block.expr {
            let is_stmt_like = matches!(e.as_ref(),
                Expr::For { .. } | Expr::While { .. } | Expr::Loop { .. }
                | Expr::If { .. } | Expr::Match { .. }
            );
            if is_stmt_like {
                out.push_str(&format!("{}{};\n", self.ind(), self.gen_expr(e)));
            } else {
                out.push_str(&format!("{}return {};\n", self.ind(), self.gen_expr(e)));
            }
        }
        if out.is_empty() { out.push_str(&format!("{}// empty\n", self.ind())); }
        out
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n) => n.to_string(),
            Expr::FloatLit(f) => format!("{f:?}"),
            Expr::BoolLit(b) => b.to_string(),
            Expr::StringLit(s) => {
                let has_interp = s.parts.iter().any(|p| matches!(p, StringPart::Interpolation(_)));
                let mut out = if has_interp { String::from("`") } else { String::from("\"") };
                for part in &s.parts {
                    match part {
                        StringPart::Literal(t) => out.push_str(&t.replace('\n', "\\n").replace('\t', "\\t")),
                        StringPart::Interpolation(tokens) => {
                            out.push_str("${");
                            for tok in tokens { out.push_str(&interpolation_token_to_target(tok, "this")); }
                            out.push('}');
                        }
                    }
                }
                out.push(if has_interp { '`' } else { '"' });
                out
            }
            Expr::CharLit(c) => format!("\"{}\"", c),
            Expr::Nil => "null".into(),
            Expr::Ident(n) => if n == "sf" { "this".into() } else { n.clone() },
            Expr::BinaryOp { left, op, right } => {
                let o = match op {
                    BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/",
                    BinOp::Mod => "%", BinOp::Pow => "**", BinOp::Eq => "===", BinOp::NotEq => "!==",
                    BinOp::Less => "<", BinOp::Greater => ">", BinOp::LessEq => "<=", BinOp::GreaterEq => ">=",
                    BinOp::And => "&&", BinOp::Or => "||", BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^", BinOp::ShiftLeft => "<<", BinOp::ShiftRight => ">>",
                };
                format!("({} {} {})", self.gen_expr(left), o, self.gen_expr(right))
            }
            Expr::UnaryOp { op, operand } => {
                let o = match op { UnaryOp::Neg => "-", UnaryOp::Not => "!", UnaryOp::BitNot => "~" };
                format!("{}{}", o, self.gen_expr(operand))
            }
            Expr::Call { callee, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let c = self.gen_expr(callee);
                match c.as_str() {
                    "prt" | "print" | "println" => format!("console.log({})", a.join(", ")),
                    "tsr" | "to_string" => format!("String({})", a.join(", ")),
                    "toi" | "to_int" => format!("parseInt({})", a.join(", ")),
                    "tof" | "to_float" => format!("parseFloat({})", a.join(", ")),
                    "typ" | "type_of" => format!("typeof {}", a.join(", ")),
                    "ln" | "len" => format!("{}.length", a.join(", ")),
                    _ => format!("{}({})", c, a.join(", ")),
                }
            }
            Expr::MethodCall { object, method, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let obj = self.gen_expr(object);
                // Resolve short aliases first, then map to JS/TS
                let resolved = match method.as_str() {
                    "mp" => "map", "fl" => "filter", "fd" => "fold", "rd" => "reduce",
                    "srt" => "sort", "rv" => "reverse", "jn" => "join", "psh" => "push",
                    "con" => "contains", "ck" => "contains_key", "ln" => "len",
                    "unq" => "unique", "cnt" => "count",
                    other => other,
                };
                match resolved {
                    "len" => return format!("{}.length", obj),
                    "is_empty" => return format!("{}.length === 0", obj),
                    "to_upper" => return format!("{}.toUpperCase()", obj),
                    "to_lower" => return format!("{}.toLowerCase()", obj),
                    "starts_with" => return format!("{}.startsWith({})", obj, a.join(", ")),
                    "ends_with" => return format!("{}.endsWith({})", obj, a.join(", ")),
                    "contains" => return format!("{}.includes({})", obj, a.join(", ")),
                    "push" => return format!("[...{}, {}]", obj, a.join(", ")),
                    "fold" | "reduce" => return format!("{}.reduce({})", obj, a.join(", ")),
                    "any" => return format!("{}.some({})", obj, a.join(", ")),
                    "all" => return format!("{}.every({})", obj, a.join(", ")),
                    _ => {}
                }
                format!("{}.{}({})", obj, resolved, a.join(", "))
            }
            Expr::FieldAccess { object, field } => format!("{}.{}", self.gen_expr(object), field),
            Expr::Index { object, index } => format!("{}[{}]", self.gen_expr(object), self.gen_expr(index)),
            Expr::Slice { object, start, end } => {
                // TS: same as JS — use .slice(start, end) method.
                let obj = self.gen_expr(object);
                match (start.as_ref(), end.as_ref()) {
                    (None, None) => format!("{}.slice()", obj),
                    (Some(s), None) => format!("{}.slice({})", obj, self.gen_expr(s)),
                    (None, Some(e)) => format!("{}.slice(0, {})", obj, self.gen_expr(e)),
                    (Some(s), Some(e)) => format!("{}.slice({}, {})", obj, self.gen_expr(s), self.gen_expr(e)),
                }
            }
            Expr::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|p| self.typed_param(p)).collect();
                format!("({}) => {}", ps.join(", "), self.gen_expr(body))
            }
            Expr::If { condition, then_block, else_block } => {
                if let (Some(te), Some(eb)) = (&then_block.expr, else_block) {
                    if then_block.statements.is_empty() {
                        if let Some(ee) = &eb.expr { if eb.statements.is_empty() {
                            return format!("({} ? {} : {})", self.gen_expr(condition), self.gen_expr(te), self.gen_expr(ee));
                        }}
                    }
                }
                let mut out = format!("if ({}) {{\n", self.gen_expr(condition));
                self.indent += 1; out.push_str(&self.gen_block(then_block)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                if let Some(eb) = else_block {
                    out.push_str(" else {\n");
                    self.indent += 1; out.push_str(&self.gen_block(eb)); self.indent -= 1;
                    out.push_str(&format!("{}}}", self.ind()));
                }
                out
            }
            Expr::Match { subject, arms } => {
                let subj = self.gen_expr(subject);
                let mut out = format!("(() => {{\n");
                self.indent += 1;
                for (i, arm) in arms.iter().enumerate() {
                    let prefix = if i == 0 { "if" } else { "} else if" };
                    match &arm.pattern {
                        Pattern::Wildcard | Pattern::Binding(_) => {
                            if let Some(guard) = &arm.guard {
                                out.push_str(&format!("{}{} ({}) {{\n", self.ind(), prefix, self.gen_expr(guard)));
                                self.indent += 1;
                                out.push_str(&format!("{}return {};\n", self.ind(), self.gen_expr(&arm.body)));
                                self.indent -= 1;
                            } else if i == 0 {
                                out.push_str(&format!("{}return {};\n", self.ind(), self.gen_expr(&arm.body)));
                            } else {
                                out.push_str(&format!("{}}} else {{\n", self.ind()));
                                self.indent += 1;
                                out.push_str(&format!("{}return {};\n", self.ind(), self.gen_expr(&arm.body)));
                                self.indent -= 1;
                                out.push_str(&format!("{}}}\n", self.ind()));
                            }
                        }
                        Pattern::Literal(lit) => {
                            out.push_str(&format!("{}{} ({} === {}) {{\n", self.ind(), prefix, subj, self.gen_expr(lit)));
                            self.indent += 1;
                            out.push_str(&format!("{}return {};\n", self.ind(), self.gen_expr(&arm.body)));
                            self.indent -= 1;
                        }
                        Pattern::Range(start, end, inclusive) => {
                            let cmp = if *inclusive { "<=" } else { "<" };
                            out.push_str(&format!("{}{} ({} >= {} && {} {} {}) {{\n", self.ind(), prefix, subj, self.gen_expr(start), subj, cmp, self.gen_expr(end)));
                            self.indent += 1;
                            out.push_str(&format!("{}return {};\n", self.ind(), self.gen_expr(&arm.body)));
                            self.indent -= 1;
                        }
                        _ => out.push_str(&format!("{}// pattern TODO\n", self.ind())),
                    }
                }
                self.indent -= 1;
                out.push_str(&format!("{}}})()", self.ind()));
                out
            }
            Expr::For { binding, index_binding, iterable, body } => {
                let iter_expr = self.gen_expr(iterable);
                if let Some(idx) = index_binding {
                    let mut out = format!("{}{}.forEach(({}, {}) => {{\n", self.ind(), iter_expr, binding, idx);
                    self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                    out.push_str(&format!("{}}})", self.ind()));
                    out
                } else {
                    let mut out = format!("{}for (const {} of {}) {{\n", self.ind(), binding, iter_expr);
                    self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                    out.push_str(&format!("{}}}", self.ind()));
                    out
                }
            }
            Expr::While { condition, body } => {
                let mut out = format!("{}while ({}) {{\n", self.ind(), self.gen_expr(condition));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::Loop { body } => {
                let mut out = format!("{}while (true) {{\n", self.ind());
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::Block(block) => {
                if let Some(e) = &block.expr { if block.statements.is_empty() { return self.gen_expr(e); } }
                format!("(() => {{ /* block */ }})()")
            }
            Expr::List(items) => {
                let i: Vec<String> = items.iter().map(|e| self.gen_expr(e)).collect();
                format!("[{}]", i.join(", "))
            }
            Expr::Map(entries) => {
                let e: Vec<String> = entries.iter().map(|(k, v)| format!("{}: {}", self.gen_expr(k), self.gen_expr(v))).collect();
                format!("{{ {} }}", e.join(", "))
            }
            Expr::Tuple(items) => {
                let i: Vec<String> = items.iter().map(|e| self.gen_expr(e)).collect();
                format!("[{}] as const", i.join(", "))
            }
            Expr::StructLit { name, fields, .. } => {
                let fs: Vec<String> = fields.iter().map(|(k, v)| {
                    if let Expr::Ident(vn) = v { if vn == k { return k.clone(); } }
                    format!("{}: {}", k, self.gen_expr(v))
                }).collect();
                format!("{{ {} }} as {}", fs.join(", "), name)
            }
            Expr::Range { start, end, .. } => {
                format!("Array.from({{ length: {} - {} }}, (_, i) => i + {})", self.gen_expr(end), self.gen_expr(start), self.gen_expr(start))
            }
            Expr::Pipe { left, right } => {
                match right.as_ref() {
                    Expr::Call { callee, args } => {
                        let mut all = vec![self.gen_expr(left)];
                        all.extend(args.iter().map(|a| self.gen_expr(&a.value)));
                        format!("{}({})", self.gen_expr(callee), all.join(", "))
                    }
                    Expr::Ident(n) => format!("{}({})", n, self.gen_expr(left)),
                    _ => format!("{}({})", self.gen_expr(right), self.gen_expr(left)),
                }
            }
            Expr::NilCoalesce { left, right } => format!("({} ?? {})", self.gen_expr(left), self.gen_expr(right)),
            Expr::Try(inner) => self.gen_expr(inner),
            Expr::TryCatch { try_block, error_binding, catch_block } => {
                let mut out = format!("{}try {{\n", self.ind());
                self.indent += 1; out.push_str(&self.gen_block(try_block)); self.indent -= 1;
                out.push_str(&format!("{}}} catch ({}) {{\n", self.ind(), error_binding));
                self.indent += 1; out.push_str(&self.gen_block(catch_block)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
        }
    }
}

// =============================================================================
// Rust Code Generator
// =============================================================================

struct RustGen {
    indent: usize,
    // Stack of declared names per scope. Nebula's canonicalized bare
    // `x = 5` is ambiguous — it's either a new declaration or a reassign.
    // Python/JS don't care. Rust does. We resolve it here by tracking
    // which names have been declared in enclosing blocks: first use emits
    // `let mut`, subsequent uses emit plain `=`.
    scopes: Vec<std::collections::HashSet<String>>,
    // Map of "Enum::Variant" → struct field names (for variants declared
    // with named fields like `Circle { r: f64 }`). Used to emit match
    // arms in struct-form `Enum::Variant { r }` instead of tuple-form.
    enum_struct_variants: std::collections::HashMap<String, Vec<String>>,
    /// All `(Enum, variant)` pairs seen in the program. Used to tell a
    /// variant constructor from an associated function when both share the
    /// surface `Type::name` form.
    all_enum_variants: std::collections::HashSet<(String, String)>,
    /// Whether the current function being emitted returns `String` —
    /// drives `.to_string()` wrapping on bare string-literal return sites.
    current_fn_returns_string: bool,
}

impl RustGen {
    fn is_declared(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }
    fn declare(&mut self, name: &str) {
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name.to_string());
        }
    }
}

impl RustGen {
    fn ind(&self) -> String { "    ".repeat(self.indent) }

    /// Scan the program for EnumDecls and record per-variant field-shape
    /// (struct-like with named fields, or tuple-like) so match arms can
    /// render the correct form regardless of whether the Pattern lost that info.
    fn collect_enum_variants(program: &Program) -> std::collections::HashMap<String, Vec<String>> {
        let mut struct_variants: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for stmt in &program.statements {
            if let Statement::EnumDecl { name, variants, .. } = stmt {
                for v in variants {
                    let any_named = v.fields.iter().any(|f| f.name.is_some());
                    if any_named {
                        let field_names: Vec<String> = v.fields.iter().enumerate()
                            .map(|(i, f)| f.name.clone().unwrap_or_else(|| format!("f{i}")))
                            .collect();
                        let key = format!("{}::{}", name, Self::pascal(&v.name));
                        struct_variants.insert(key, field_names);
                    }
                }
            }
        }
        struct_variants
    }

    /// Collect all `(Enum, variant)` pairs so codegen can distinguish a
    /// variant constructor (PascalCase) from an associated function
    /// (snake_case) when both share `Type::name` surface syntax.
    fn collect_all_enum_variants(program: &Program) -> std::collections::HashSet<(String, String)> {
        let mut set = std::collections::HashSet::new();
        for stmt in &program.statements {
            if let Statement::EnumDecl { name, variants, .. } = stmt {
                for v in variants {
                    set.insert((name.clone(), v.name.clone()));
                    set.insert((name.clone(), Self::pascal(&v.name)));
                }
            }
        }
        set
    }

    fn gen_program(&mut self, program: &Program) -> String {
        self.enum_struct_variants = Self::collect_enum_variants(program);
        self.all_enum_variants = Self::collect_all_enum_variants(program);
        let mut body = String::new();
        for stmt in &program.statements { body.push_str(&self.gen_stmt(stmt)); body.push('\n'); }
        let mut prefix = String::new();
        let coll_needs: Vec<&str> = ["HashMap", "HashSet", "BTreeMap", "BTreeSet", "VecDeque"]
            .iter()
            .filter(|t| body.contains(&format!("{}<", t))
                || body.contains(&format!("{}::", t))
                || body.contains(&format!(" {}", t)))
            .filter(|t| !body.contains(&format!("std::collections::{}", t))
                && !body.contains(&format!("collections::{{")))
            .copied()
            .collect();
        if !coll_needs.is_empty() {
            prefix.push_str(&format!("use std::collections::{{{}}};\n", coll_needs.join(", ")));
        }
        // Common stdlib types referenced in signatures.
        let path_needs: Vec<&str> = ["Path", "PathBuf"]
            .iter()
            .filter(|t| {
                let has_type = body.contains(&format!(" {}", t))
                    || body.contains(&format!("<{}>", t))
                    || body.contains(&format!(": {}", t))
                    || body.contains(&format!("-> {}", t));
                let already = body.contains(&format!("std::path::{}", t))
                    || body.contains("std::path::{");
                has_type && !already
            })
            .copied()
            .collect();
        if !path_needs.is_empty() {
            prefix.push_str(&format!("use std::path::{{{}}};\n", path_needs.join(", ")));
        }
        let time_needs: Vec<&str> = ["Duration", "Instant"]
            .iter()
            .filter(|t| {
                let has_type = body.contains(&format!(" {}", t))
                    || body.contains(&format!("-> {}", t))
                    || body.contains(&format!("<{}>", t));
                let already = body.contains(&format!("std::time::{}", t))
                    || body.contains("std::time::{");
                has_type && !already
            })
            .copied()
            .collect();
        if !time_needs.is_empty() {
            prefix.push_str(&format!("use std::time::{{{}}};\n", time_needs.join(", ")));
        }
        format!("{}{}", prefix, body)
    }

    /// Render a match pattern for Rust. Converts `Enum.Variant` field-access
    /// shapes to `Enum::Variant` path form and PascalCases the variant.
    fn rust_pattern(&mut self, p: &Pattern) -> String {
        use Pattern::*;
        match p {
            Wildcard => "_".into(),
            Binding(n) => n.clone(),
            Literal(e) => Self::rust_expr_as_pat(e),
            Range(s, e, inc) => format!("{}..{}{}", self.gen_expr(s), if *inc { "=" } else { "" }, self.gen_expr(e)),
            Variant(name, subs) => {
                let path = Self::variant_path(name);
                if subs.is_empty() { return path; }
                let sub_strs: Vec<String> = subs.iter().map(|p| self.rust_pattern(p)).collect();
                // Use struct-form `{ f1, f2 }` when the enum decl says this
                // variant has named fields; tuple-form `(x, y)` otherwise.
                if self.enum_struct_variants.contains_key(&path) {
                    format!("{} {{ {} }}", path, sub_strs.join(", "))
                } else {
                    format!("{}({})", path, sub_strs.join(", "))
                }
            }
        }
    }

    /// When the Literal pattern is `FieldAccess(Named, member)` (e.g. `Shape.Circle`)
    /// or a `Call` to such, render with `::` separator and PascalCase variant.
    fn rust_expr_as_pat(e: &Expr) -> String {
        match e {
            Expr::FieldAccess { object, field } => {
                if let Expr::Ident(o) = object.as_ref() {
                    let f = Self::pascal(field);
                    return format!("{}::{}", o, f);
                }
                format!("{}.{}", Self::rust_expr_as_pat(object), field)
            }
            Expr::IntLit(n) => n.to_string(),
            Expr::StringLit(s) => {
                let mut out = String::from("\"");
                for p in &s.parts {
                    if let StringPart::Literal(t) = p { out.push_str(t); }
                }
                out.push('"'); out
            }
            Expr::BoolLit(b) => b.to_string(),
            Expr::Ident(n) => n.clone(),
            _ => format!("/* unsupported pat: {:?} */", std::mem::discriminant(e)),
        }
    }

    fn pascal(s: &str) -> String {
        let camel = snake_to_camel(s);
        let mut cs = camel.chars();
        match cs.next() {
            Some(c) => c.to_uppercase().chain(cs).collect(),
            None => String::new(),
        }
    }

    /// Return `true` if the name is a well-known Rust path root — a crate
    /// name or a short module from the stdlib — so field/method access
    /// through it should use `::` instead of `.`.
    fn is_rust_path_root(s: &str) -> bool {
        matches!(s,
            "std" | "core" | "alloc" | "crate" | "self" | "super" | "proc_macro"
            | "io" | "fs" | "net" | "env" | "mem" | "str" | "fmt" | "iter"
            | "cmp" | "ops" | "mpsc" | "sync" | "thread" | "atomic"
            | "collections" | "ptr" | "cell" | "rc" | "arc" | "path"
            | "num" | "convert" | "hash" | "time" | "any" | "char" | "vec"
        )
    }

    /// Detect whether the expression chain is rooted at a path-like ident
    /// (so `std.fs.read_to_string` propagates `::` through the whole chain).
    fn chain_rooted_at_path(e: &Expr) -> bool {
        match e {
            Expr::Ident(n) => Self::is_rust_path_root(n),
            Expr::FieldAccess { object, .. } => Self::chain_rooted_at_path(object),
            Expr::MethodCall { object, .. } => Self::chain_rooted_at_path(object),
            _ => false,
        }
    }

    /// Lowercase stdlib type identifiers that lost their case at ingest.
    fn rust_type_rename(s: &str) -> Option<&'static str> {
        Some(match s {
            "path_buf" => "PathBuf",
            "duration" => "Duration",
            "instant" => "Instant",
            "hash_map" => "HashMap",
            "hash_set" => "HashSet",
            "b_tree_map" => "BTreeMap",
            "b_tree_set" => "BTreeSet",
            "vec_deque" => "VecDeque",
            "parse_int_error" => "ParseIntError",
            _ => return None,
        })
    }

    fn variant_path(name: &str) -> String {
        // Accept inputs like `Shape.circle`, `Shape::Circle`, `Circle`.
        let parts: Vec<&str> = name.split(&['.', ':'][..]).filter(|s| !s.is_empty()).collect();
        if parts.len() <= 1 { return Self::pascal(name); }
        let mut out: Vec<String> = parts.iter().take(parts.len() - 1).map(|s| s.to_string()).collect();
        out.push(Self::pascal(parts.last().unwrap()));
        out.join("::")
    }

    /// Walk a type expr and collect names that look like generic params:
    /// single uppercase letters (T, U, F, E, K, V) that aren't known concrete types.
    fn collect_generics(t: &TypeExpr, out: &mut std::collections::BTreeSet<String>) {
        const KNOWN: &[&str] = &["String", "Str", "Bool", "Char", "Int", "Float", "Nil",
            "Vec", "HashMap", "HashSet", "Option", "Result", "Box", "Any", "Self"];
        match t {
            TypeExpr::Named(n) => {
                if n.len() == 1 && n.chars().next().unwrap().is_ascii_uppercase() {
                    out.insert(n.clone());
                } else if !KNOWN.contains(&n.as_str()) && !n.contains('.')
                    && n.chars().next().map_or(false, |c| c.is_ascii_uppercase())
                    && n.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                {
                    out.insert(n.clone());
                }
            }
            TypeExpr::Generic(_, args) => {
                for a in args { Self::collect_generics(a, out); }
            }
            TypeExpr::Option(inner) => Self::collect_generics(inner, out),
            TypeExpr::Function(params, ret) => {
                for p in params { Self::collect_generics(p, out); }
                Self::collect_generics(ret, out);
            }
            TypeExpr::Tuple(items) => {
                for i in items { Self::collect_generics(i, out); }
            }
        }
    }

    fn neb_type(&self, t: &TypeExpr) -> String {
        match t {
            TypeExpr::Named(n) => match n.as_str() {
                "Int" => "i64".into(), "Float" => "f64".into(),
                // `String` in Nebula IL maps to Rust `String` (owned);
                // `Str` is the more common form and maps to `&str`, which is
                // compatible with string literals that appear frequently in
                // match-arm returns. Uses that genuinely need owned strings
                // should emit `.to_string()` or `String::from(...)`.
                "String" | "Str" => "String".into(),
                "Bool" => "bool".into(),
                "Char" => "char".into(), "Nil" => "()".into(),
                // Dotted qualified paths (`io.Reader`, `std.fmt.Display`) use
                // `::` in Rust, not `.`. PascalCase the last segment if it
                // looks like a type, preserving short stdlib module names.
                other if other.contains('.') => {
                    let parts: Vec<&str> = other.split('.').collect();
                    let mut out: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
                    if let Some(last) = out.last_mut() {
                        let short = matches!(last.as_str(), "io" | "fmt" | "fs" | "net" | "str"
                            | "collections" | "path" | "num" | "time" | "convert");
                        if !short && last.chars().next().map_or(false, |c| c.is_ascii_lowercase()) {
                            *last = Self::pascal(last);
                        }
                    }
                    out.join("::")
                }
                // Lowercase named types that aren't primitives likely lost
                // their case during ingest; PascalCase them.
                other if other.chars().next().map_or(false, |c| c.is_ascii_lowercase())
                    && other.contains('_') => Self::pascal(other),
                other => other.to_string(),
            },
            TypeExpr::Generic(name, args) => {
                let a: Vec<String> = args.iter().map(|a| self.neb_type(a)).collect();
                match name.as_str() {
                    "List" => format!("Vec<{}>", a.first().unwrap_or(&"_".into())),
                    "Map" => format!("HashMap<{}, {}>", a.first().unwrap_or(&"String".into()), a.get(1).unwrap_or(&"_".into())),
                    "box" | "Box" => format!("Box<{}>", a.first().unwrap_or(&"_".into())),
                    _ => {
                        // Dotted generic name `io.Result` → `io::Result`
                        let n = if name.contains('.') {
                            let parts: Vec<&str> = name.split('.').collect();
                            let mut out: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
                            if let Some(last) = out.last_mut() {
                                if last.chars().next().map_or(false, |c| c.is_ascii_lowercase()) {
                                    let short = matches!(last.as_str(), "io" | "fmt" | "fs" | "net" | "str"
                                        | "collections" | "path" | "num" | "time" | "convert");
                                    if !short { *last = Self::pascal(last); }
                                }
                            }
                            out.join("::")
                        } else { name.clone() };
                        format!("{}<{}>", n, a.join(", "))
                    }
                }
            }
            TypeExpr::Option(inner) => format!("Option<{}>", self.neb_type(inner)),
            TypeExpr::Function(params, ret) => {
                let ps: Vec<String> = params.iter().map(|p| self.neb_type(p)).collect();
                format!("impl Fn({}) -> {}", ps.join(", "), self.neb_type(ret))
            }
            TypeExpr::Tuple(items) => {
                let is: Vec<String> = items.iter().map(|i| self.neb_type(i)).collect();
                format!("({})", is.join(", "))
            }
        }
    }

    fn gen_stmt(&mut self, stmt: &Statement) -> String {
        match stmt {
            Statement::Let { name, type_ann, value } => {
                let t = type_ann.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                let v = self.gen_expr(value);
                self.declare(name);
                format!("{}let {}{} = {};", self.ind(), name, t, v)
            }
            Statement::Mut { name, type_ann, value } => {
                let t = type_ann.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                let v = self.gen_expr(value);
                self.declare(name);
                format!("{}let mut {}{} = {};", self.ind(), name, t, v)
            }
            Statement::Const { name, type_ann, value } => {
                let t = type_ann.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                let v = self.gen_expr(value);
                self.declare(name);
                format!("{}const {}{} = {};", self.ind(), name, t, v)
            }
            Statement::FnDecl { name, params, return_type, body, .. } => {
                let ps: Vec<String> = params.iter().filter(|p| p.name != "sf")
                    .map(|p| {
                        let t = p.type_ann.as_ref().map(|t| self.neb_type(t)).unwrap_or("_".into());
                        format!("{}: {}", p.name, t)
                    }).collect();
                let ret = return_type.as_ref().map(|t| format!(" -> {}", self.neb_type(t))).unwrap_or_default();
                let mut generic_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                for p in params.iter().filter(|p| p.name != "sf") {
                    if let Some(t) = &p.type_ann { Self::collect_generics(t, &mut generic_set); }
                }
                if let Some(rt) = return_type { Self::collect_generics(rt, &mut generic_set); }
                let generics_str = if generic_set.is_empty() { String::new() }
                    else { format!("<{}>", generic_set.into_iter().collect::<Vec<_>>().join(", ")) };
                // Track whether this fn returns String so match-arm /
                // return-value emission can wrap bare string literals.
                let prev_ret = self.current_fn_returns_string;
                self.current_fn_returns_string = matches!(return_type,
                    Some(TypeExpr::Named(n)) if n == "String" || n == "Str");
                self.scopes.push(std::collections::HashSet::new());
                for p in params { self.declare(&p.name); }
                let mut out = format!("{}fn {}{}({}){} {{\n", self.ind(), name, generics_str, ps.join(", "), ret);
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                self.scopes.pop();
                self.current_fn_returns_string = prev_ret;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::StructDecl { name, fields, generics, .. } => {
                let gen = if generics.is_empty() { String::new() } else { format!("<{}>", generics.join(", ")) };
                let real_fields: Vec<&crate::ast::Field> = fields.iter()
                    .filter(|f| !(f.name == "_empty" && matches!(&f.type_ann,
                        crate::ast::TypeExpr::Named(n) if n == "Any")))
                    .collect();
                if real_fields.is_empty() {
                    return format!("{}struct {}{};", self.ind(), name, gen);
                }
                // Detect tuple-struct shape: fields named `_0`, `_1`, … emit
                // `struct Name(T0, T1, ...);`.
                let is_tuple = real_fields.iter().enumerate().all(|(i, f)| f.name == format!("_{}", i));
                if is_tuple {
                    let tys: Vec<String> = real_fields.iter().map(|f| self.neb_type(&f.type_ann)).collect();
                    return format!("{}struct {}{}({});", self.ind(), name, gen, tys.join(", "));
                }
                let mut out = format!("{}struct {}{} {{\n", self.ind(), name, gen);
                self.indent += 1;
                for f in &real_fields { out.push_str(&format!("{}{}: {},\n", self.ind(), f.name, self.neb_type(&f.type_ann))); }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::EnumDecl { name, variants, generics, .. } => {
                let gen = if generics.is_empty() { String::new() } else { format!("<{}>", generics.join(", ")) };
                let mut out = format!("{}enum {}{} {{\n", self.ind(), name, gen);
                self.indent += 1;
                for v in variants {
                    // Rust enum variants are PascalCase. Convert snake_case →
                    // PascalCase via pascal() (which passes through already-
                    // PascalCase inputs unchanged).
                    let vname = Self::pascal(&v.name);
                    if v.fields.is_empty() {
                        out.push_str(&format!("{}{},\n", self.ind(), vname));
                    } else {
                        let any_named = v.fields.iter().any(|f| f.name.is_some());
                        if any_named {
                            let fs: Vec<String> = v.fields.iter().enumerate().map(|(i, f)| {
                                let fname = f.name.clone().unwrap_or_else(|| format!("f{i}"));
                                format!("{}: {}", fname, self.neb_type(&f.type_ann))
                            }).collect();
                            out.push_str(&format!("{}{} {{ {} }},\n", self.ind(), vname, fs.join(", ")));
                        } else {
                            let fs: Vec<String> = v.fields.iter().map(|f| self.neb_type(&f.type_ann)).collect();
                            out.push_str(&format!("{}{}({}),\n", self.ind(), vname, fs.join(", ")));
                        }
                    }
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::ImplBlock { trait_name, target, methods } => {
                let header = if let Some(t) = trait_name { format!("impl {} for {}", t, target) } else { format!("impl {}", target) };
                let mut out = format!("{}{} {{\n", self.ind(), header);
                self.indent += 1;
                for m in methods {
                    // Emit impl methods directly with proper &self handling,
                    // not via the top-level FnDecl path which lacks receiver logic.
                    if let Statement::FnDecl { name: mname, params, body, return_type, .. } = m {
                        let has_self = params.iter().any(|p| p.name == "sf");
                        let mut ps: Vec<String> = Vec::new();
                        if has_self { ps.push("&self".into()); }
                        for p in params.iter().filter(|p| p.name != "sf") {
                            let t = p.type_ann.as_ref().map(|t| self.neb_type(t)).unwrap_or_else(|| "_".into());
                            ps.push(format!("{}: {}", p.name, t));
                        }
                        let ret = return_type.as_ref().map(|t| format!(" -> {}", self.neb_type(t))).unwrap_or_default();
                        // Recover generics from param/return types
                        let mut generic_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                        for p in params.iter().filter(|p| p.name != "sf") {
                            if let Some(t) = &p.type_ann { Self::collect_generics(t, &mut generic_set); }
                        }
                        if let Some(rt) = return_type { Self::collect_generics(rt, &mut generic_set); }
                        let generics_str = if generic_set.is_empty() { String::new() }
                            else { format!("<{}>", generic_set.into_iter().collect::<Vec<_>>().join(", ")) };
                        out.push_str(&format!("{}fn {}{}({}){} {{\n", self.ind(), mname, generics_str, ps.join(", "), ret));
                        self.indent += 1;
                        self.scopes.push(std::collections::HashSet::new());
                        for p in params { self.declare(&p.name); }
                        out.push_str(&self.gen_block(body));
                        self.scopes.pop();
                        self.indent -= 1;
                        out.push_str(&format!("{}}}\n", self.ind()));
                    } else {
                        out.push_str(&self.gen_stmt(m));
                        out.push('\n');
                    }
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::TraitDecl { name, methods, generics, .. } => {
                let gen = if generics.is_empty() { String::new() } else { format!("<{}>", generics.join(", ")) };
                let mut out = format!("{}trait {}{} {{\n", self.ind(), name, gen);
                self.indent += 1;
                for m in methods {
                    let ps: Vec<String> = m.params.iter().map(|p| {
                        if p.name == "sf" { "&self".into() }
                        else { let t = p.type_ann.as_ref().map(|t| self.neb_type(t)).unwrap_or("_".into()); format!("{}: {}", p.name, t) }
                    }).collect();
                    let ret = m.return_type.as_ref().map(|t| format!(" -> {}", self.neb_type(t))).unwrap_or_default();
                    let tail = match &m.body {
                        Some(b) => {
                            self.indent += 1;
                            let body = self.gen_block(b);
                            self.indent -= 1;
                            format!(" {{\n{}{}}}", body, self.ind())
                        }
                        None => ";".to_string(),
                    };
                    out.push_str(&format!("{}fn {}({}){}{}\n", self.ind(), m.name, ps.join(", "), ret, tail));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::Expr(e) => format!("{}{};", self.ind(), self.gen_expr(e)),
            Statement::Return(Some(e)) => format!("{}return {};", self.ind(), self.gen_expr(e)),
            Statement::Return(None) => format!("{}return;", self.ind()),
            Statement::Break(_) => format!("{}break;", self.ind()),
            Statement::Continue => format!("{}continue;", self.ind()),
            Statement::Throw(Some(e)) => format!("{}panic!(\"{{:?}}\", {});", self.ind(), self.gen_expr(e)),
            Statement::Throw(None) => format!("{}panic!();", self.ind()),
            Statement::Assignment { target, op, value } => {
                let o = match op { AssignOp::Assign => "=", AssignOp::AddAssign => "+=", AssignOp::SubAssign => "-=", AssignOp::MulAssign => "*=", AssignOp::DivAssign => "/=", AssignOp::ModAssign => "%=" };
                // Nebula bare `x = 5` at first use in a block is a new
                // declaration in Python/JS semantics. Rust needs `let mut`.
                // Promote simple-ident, plain-assign, first-use targets to
                // `let mut`; everything else is a plain reassign.
                if matches!(op, AssignOp::Assign) {
                    if let Expr::Ident(name) = target {
                        if !self.is_declared(name) {
                            let v = self.gen_expr(value);
                            self.declare(name);
                            return format!("{}let mut {} = {};", self.ind(), name, v);
                        }
                    }
                }
                format!("{}{} {} {};", self.ind(), self.gen_expr(target), o, self.gen_expr(value))
            }
            Statement::Use { path, .. } => {
                // PascalCase the last segment when it looks like a type
                // (snake_case or all-lowercase), so `std.fmt.display` →
                // `std::fmt::Display`. Leave already-PascalCase or short
                // stdlib-module names (`io`, `fmt`) alone.
                let mut parts: Vec<String> = path.iter().cloned().collect();
                if let Some(last) = parts.last_mut() {
                    let short_stdlib = matches!(last.as_str(),
                        "io" | "fmt" | "fs" | "net" | "env" | "mem" | "str"
                        | "vec" | "iter" | "cmp" | "ops" | "mpsc" | "sync"
                        | "thread" | "atomic" | "collections" | "ptr" | "cell"
                        | "rc" | "arc" | "path" | "num" | "convert" | "hash"
                        | "time" | "any" | "char");
                    if !short_stdlib && last.chars().next().map_or(false, |c| c.is_ascii_lowercase()) {
                        *last = Self::pascal(last);
                    }
                }
                format!("{}use {};", self.ind(), parts.join("::"))
            }
            Statement::LetDestructure { names, value, mutable } => {
                let kw = if *mutable { "let mut" } else { "let" };
                format!("{}{} ({}) = {};", self.ind(), kw, names.join(", "), self.gen_expr(value))
            }
            Statement::TypeAlias { name, type_expr } => {
                format!("{}type {} = {};", self.ind(), name, self.neb_type(type_expr))
            }
            Statement::Module { path } => format!("{}mod {};", self.ind(), path.join("::")),
        }
    }

    fn gen_block(&mut self, block: &Block) -> String {
        self.scopes.push(std::collections::HashSet::new());
        let mut out = String::new();
        for s in &block.statements { out.push_str(&self.gen_stmt(s)); out.push('\n'); }
        if let Some(e) = &block.expr { out.push_str(&format!("{}{}\n", self.ind(), self.gen_expr(e))); }
        self.scopes.pop();
        out
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n) => n.to_string(),
            Expr::FloatLit(f) => format!("{f:?}"),
            Expr::BoolLit(b) => b.to_string(),
            Expr::StringLit(s) => {
                // A non-empty Interpolation becomes a `{}` with an arg.
                // An empty Interpolation (e.g. literal `{}` in the source)
                // stays as `{}` without consuming an arg — lets callers like
                // `panic!("x: {}", v)` get their positional placeholder back.
                let has_real_interp = s.parts.iter().any(|p| matches!(p, StringPart::Interpolation(t) if !t.is_empty()));
                if !has_real_interp {
                    let text: String = s.parts.iter().map(|p| match p {
                        StringPart::Literal(t) => t.as_str().to_string(),
                        StringPart::Interpolation(_) => "{}".to_string(),
                    }).collect();
                    return format!("\"{}\"", text);
                }
                let mut out = String::from("format!(\"");
                let mut args = Vec::new();
                for part in &s.parts {
                    match part {
                        StringPart::Literal(t) => out.push_str(&t.replace('\n', "\\n").replace('\t', "\\t")),
                        StringPart::Interpolation(tokens) if tokens.is_empty() => {
                            // Bare `{}` — leave as positional placeholder with no arg.
                            out.push_str("{}");
                        }
                        StringPart::Interpolation(tokens) => {
                            out.push_str("{}");
                            let a: String = tokens.iter().map(|t| interpolation_token_to_target(t, "self")).collect();
                            args.push(a);
                        }
                    }
                }
                out.push('"');
                if !args.is_empty() { out.push_str(&format!(", {}", args.join(", "))); }
                out.push(')');
                out
            }
            Expr::CharLit(c) => format!("'{c}'"),
            Expr::Nil => "None".into(),
            Expr::Ident(n) => if n == "sf" { "self".into() } else { n.clone() },
            Expr::BinaryOp { left, op, right } => {
                let o = match op {
                    BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/",
                    BinOp::Mod => "%", BinOp::Pow => ".pow", BinOp::Eq => "==", BinOp::NotEq => "!=",
                    BinOp::Less => "<", BinOp::Greater => ">", BinOp::LessEq => "<=", BinOp::GreaterEq => ">=",
                    BinOp::And => "&&", BinOp::Or => "||", BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^", BinOp::ShiftLeft => "<<", BinOp::ShiftRight => ">>",
                };
                format!("({} {} {})", self.gen_expr(left), o, self.gen_expr(right))
            }
            Expr::UnaryOp { op, operand } => {
                let o = match op { UnaryOp::Neg => "-", UnaryOp::Not => "!", UnaryOp::BitNot => "!" };
                format!("{}{}", o, self.gen_expr(operand))
            }
            Expr::Call { callee, args } => {
                let c = self.gen_expr(callee);
                // Internal markers from the ingester.
                if c == "__deref" {
                    let a: Vec<String> = args.iter().map(|arg| self.gen_expr(&arg.value)).collect();
                    return format!("*{}", a.join(""));
                }
                if c == "__addr" {
                    let a: Vec<String> = args.iter().map(|arg| self.gen_expr(&arg.value)).collect();
                    return format!("&{}", a.join(""));
                }
                // Rust format-like calls are macros: pass all args through to
                // `<name>!(...)`. Skip wrapping the first string arg in a nested
                // `format!(...)` call — the macro does its own formatting.
                let is_macro = matches!(c.as_str(),
                    "panic" | "print" | "println" | "eprint" | "eprintln" | "prt"
                    | "format" | "fmt" | "write" | "writeln" | "assert" | "assert_eq"
                    | "debug_assert" | "debug_assert_eq" | "todo" | "unimplemented"
                    | "unreachable" | "dbg" | "vec");
                if is_macro {
                    let name = if c == "prt" || c == "print" { "println".to_string() }
                        else if c == "fmt" { "format".to_string() }
                        else { c };
                    let a: Vec<String> = args.iter().map(|arg| self.gen_expr(&arg.value)).collect();
                    return format!("{}!({})", name, a.join(", "));
                }
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                format!("{}({})", c, a.join(", "))
            }
            Expr::MethodCall { object, method, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                if let Expr::Ident(o) = object.as_ref() {
                    if Self::is_rust_path_root(o) {
                        return format!("{}::{}({})", o, method, a.join(", "));
                    }
                    if o.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                        // Known enum variant → PascalCase constructor call.
                        // Otherwise assume associated function (snake_case).
                        let m = if self.all_enum_variants.contains(&(o.clone(), method.clone())) {
                            Self::pascal(method)
                        } else { method.clone() };
                        return format!("{}::{}({})", o, m, a.join(", "));
                    }
                    if let Some(rn) = Self::rust_type_rename(o) {
                        return format!("{}::{}({})", rn, method, a.join(", "));
                    }
                }
                if Self::chain_rooted_at_path(object) {
                    return format!("{}::{}({})", self.gen_expr(object), method, a.join(", "));
                }
                let m = match method.as_str() {
                    "emp" => "is_empty",
                    "uw" => "unwrap",
                    "clr" => "clear",
                    "str" => "to_string",
                    other => other,
                };
                let obj_str = self.gen_expr(object);
                let needs_parens = matches!(object.as_ref(), Expr::Range { .. });
                // Iterator methods need `.iter()` or `.into_iter()` on a
                // Vec/slice receiver. Without a type inferencer we add it
                // speculatively for well-known iter-only methods.
                let iter_methods = ["enumerate", "map", "filter", "fold", "sum",
                    "product", "count", "any", "all", "find", "min", "max",
                    "zip", "take", "skip", "flat_map", "rev"];
                let needs_iter = iter_methods.contains(&m);
                // Ranges and already-iterator method chains are iterators on
                // their own; only wrap a concrete collection (Vec, &[]) with
                // `.iter()`.
                let already_iter = match object.as_ref() {
                    Expr::Range { .. } => true,
                    Expr::MethodCall { method: m2, .. } => matches!(m2.as_str(),
                        "iter" | "into_iter" | "iter_mut" | "enumerate"
                        | "map" | "filter" | "fold" | "zip" | "take"
                        | "skip" | "flat_map" | "rev" | "chain"),
                    _ => false,
                };
                let obj_wrapped = if needs_iter && !already_iter {
                    if needs_parens { format!("({}).iter()", obj_str) }
                    else { format!("{}.iter()", obj_str) }
                } else if needs_parens { format!("({})", obj_str) }
                else { obj_str };
                format!("{}.{}({})", obj_wrapped, m, a.join(", "))
            }
            Expr::FieldAccess { object, field } => {
                if let Expr::Ident(o) = object.as_ref() {
                    if Self::is_rust_path_root(o) {
                        return format!("{}::{}", o, field);
                    }
                    if o.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                        // Known enum variant → pascal-case. Otherwise it's
                        // likely an associated constant — leave the case alone.
                        if self.all_enum_variants.contains(&(o.clone(), field.clone())) {
                            return format!("{}::{}", o, Self::pascal(field));
                        }
                        return format!("{}::{}", o, field);
                    }
                    if let Some(rn) = Self::rust_type_rename(o) {
                        return format!("{}::{}", rn, field);
                    }
                }
                if Self::chain_rooted_at_path(object) {
                    return format!("{}::{}", self.gen_expr(object), field);
                }
                format!("{}.{}", self.gen_expr(object), field)
            }
            Expr::Index { object, index } => format!("{}[{}]", self.gen_expr(object), self.gen_expr(index)),
            Expr::Slice { object, start, end } => {
                // Rust slice indices must be `usize`. The IL has no usize
                // distinction (everything's `Int`), so cast explicitly.
                let obj = self.gen_expr(object);
                let cast = |e: String| format!("({}) as usize", e);
                match (start.as_ref(), end.as_ref()) {
                    (None, None) => format!("{}.to_vec()", obj),
                    (Some(s), None) => format!("{}[{}..].to_vec()", obj, cast(self.gen_expr(s))),
                    (None, Some(e)) => format!("{}[..{}].to_vec()", obj, cast(self.gen_expr(e))),
                    (Some(s), Some(e)) => format!("{}[{}..{}].to_vec()", obj, cast(self.gen_expr(s)), cast(self.gen_expr(e))),
                }
            }
            Expr::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                // Multi-param lambdas in iter chains (map/filter on an
                // enumerate/zip result) are single-tuple closures in Rust;
                // wrap params in a tuple pattern.
                let pat = if ps.len() >= 2 { format!("({})", ps.join(", ")) } else { ps.join(", ") };
                format!("|{}| {}", pat, self.gen_expr(body))
            }
            Expr::If { condition, then_block, else_block } => {
                let mut out = format!("if {} {{\n", self.gen_expr(condition));
                self.indent += 1; out.push_str(&self.gen_block(then_block)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                if let Some(eb) = else_block {
                    out.push_str(" else {\n");
                    self.indent += 1; out.push_str(&self.gen_block(eb)); self.indent -= 1;
                    out.push_str(&format!("{}}}", self.ind()));
                }
                out
            }
            Expr::Match { subject, arms } => {
                let mut out = format!("match {} {{\n", self.gen_expr(subject));
                self.indent += 1;
                for arm in arms {
                    let pat = self.rust_pattern(&arm.pattern);
                    let guard = arm.guard.as_ref().map(|g| format!(" if {}", self.gen_expr(g))).unwrap_or_default();
                    let body = self.gen_expr(&arm.body);
                    // When the enclosing fn returns String and this arm body
                    // is a bare string literal, coerce with `.to_string()`.
                    let body = if self.current_fn_returns_string
                        && matches!(arm.body, Expr::StringLit(_))
                        && !body.starts_with("format!") {
                        format!("{}.to_string()", body)
                    } else { body };
                    out.push_str(&format!("{}{}{} => {},\n", self.ind(), pat, guard, body));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::For { binding, index_binding, iterable, body } => {
                // `for i, x in xs.enumerate()` → `for (i, x) in xs.iter().enumerate()`.
                let bind = match index_binding {
                    Some(idx) => format!("({}, {})", idx, binding),
                    None => binding.clone(),
                };
                let mut out = format!("for {} in {} {{\n", bind, self.gen_expr(iterable));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::While { condition, body } => {
                let mut out = format!("while {} {{\n", self.gen_expr(condition));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::Loop { body } => {
                let mut out = String::from("loop {\n");
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::List(items) => {
                let i: Vec<String> = items.iter().map(|e| self.gen_expr(e)).collect();
                format!("vec![{}]", i.join(", "))
            }
            Expr::Tuple(items) => {
                let i: Vec<String> = items.iter().map(|e| self.gen_expr(e)).collect();
                format!("({})", i.join(", "))
            }
            Expr::StructLit { name, fields, .. } => {
                let fs: Vec<String> = fields.iter().map(|(k, v)| {
                    if let Expr::Ident(vn) = v { if vn == k { return k.clone(); } }
                    format!("{}: {}", k, self.gen_expr(v))
                }).collect();
                format!("{} {{ {} }}", name, fs.join(", "))
            }
            Expr::Range { start, end, inclusive } => {
                if *inclusive { format!("{}..={}", self.gen_expr(start), self.gen_expr(end)) }
                else { format!("{}..{}", self.gen_expr(start), self.gen_expr(end)) }
            }
            Expr::Pipe { left, right } => {
                if let Expr::Ident(n) = right.as_ref() { format!("{}({})", n, self.gen_expr(left)) }
                else { format!("/* pipe */ {}", self.gen_expr(right)) }
            }
            Expr::NilCoalesce { left, right } => format!("{}.unwrap_or({})", self.gen_expr(left), self.gen_expr(right)),
            Expr::Try(inner) => format!("{}?", self.gen_expr(inner)),
            Expr::Block(block) => {
                // Rust block expression: { stmt; stmt; final_expr }. Used as
                // a closure body (`move || { ... }`) or as a standalone expr.
                let mut out = String::from("{\n");
                self.indent += 1;
                out.push_str(&self.gen_block(block));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::Map(entries) => {
                // Rust: emit as a HashMap via the from-array constructor.
                let pairs: Vec<String> = entries.iter()
                    .map(|(k, v)| format!("({}, {})", self.gen_expr(k), self.gen_expr(v)))
                    .collect();
                format!("HashMap::from([{}])", pairs.join(", "))
            }
            _ => "/* TODO */".into(),
        }
    }
}

// =============================================================================
// C Code Generator
// =============================================================================

struct CGen {
    indent: usize,
    scopes: Vec<std::collections::HashSet<String>>,
}

impl CGen {
    fn is_declared(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }
    fn declare(&mut self, name: &str) {
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name.to_string());
        }
    }
}

impl CGen {
    fn ind(&self) -> String { "    ".repeat(self.indent) }

    fn gen_program(&mut self, program: &Program) -> String {
        let mut out = String::from("#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n#include <math.h>\n\n");

        // Forward declarations + struct/enum definitions first
        for stmt in &program.statements {
            match stmt {
                Statement::StructDecl { name, fields, .. } => {
                    out.push_str(&format!("typedef struct {{\n"));
                    for f in fields {
                        out.push_str(&format!("    {} {};\n", self.c_type(&f.type_ann), f.name));
                    }
                    out.push_str(&format!("}} {};\n\n", name));
                }
                Statement::EnumDecl { name, variants, .. } => {
                    let simple = variants.iter().all(|v| v.fields.is_empty());
                    if simple {
                        let vs: Vec<String> = variants.iter().enumerate()
                            .map(|(i, v)| format!("{}_{} = {}", name, v.name, i))
                            .collect();
                        out.push_str(&format!("typedef enum {{ {} }} {};\n\n", vs.join(", "), name));
                    }
                }
                _ => {}
            }
        }

        // Functions and statements
        for stmt in &program.statements {
            match stmt {
                Statement::StructDecl { .. } | Statement::EnumDecl { .. } => {} // already handled
                _ => { out.push_str(&self.gen_stmt(stmt)); out.push('\n'); }
            }
        }
        out
    }

    fn c_type(&self, t: &TypeExpr) -> String {
        match t {
            TypeExpr::Named(n) => match n.as_str() {
                "Int" => "int64_t".into(),
                "Float" => "double".into(),
                "Bool" => "int".into(),
                "String" | "Str" => "const char*".into(),
                "Char" => "char".into(),
                "Nil" => "void".into(),
                "Any" => "void*".into(),
                // Dotted qualified paths flatten to underscores for C (no
                // native namespace). `io.Reader` → `io_Reader*`.
                other if other.contains('.') => format!("{}*", other.replace('.', "_")),
                other => format!("{}*", other),
            },
            TypeExpr::Option(inner) => format!("{}*", self.c_type(inner)),
            _ => "void*".into(),
        }
    }

    fn gen_stmt(&mut self, stmt: &Statement) -> String {
        match stmt {
            Statement::Let { name, type_ann, value } | Statement::Const { name, type_ann, value } => {
                let t = type_ann.as_ref().map(|t| self.c_type(t)).unwrap_or_else(|| self.infer_c_type(value));
                let v = self.gen_expr(value);
                self.declare(name);
                format!("{}{} {} = {};", self.ind(), t, name, v)
            }
            Statement::Mut { name, type_ann, value } => {
                let t = type_ann.as_ref().map(|t| self.c_type(t)).unwrap_or_else(|| self.infer_c_type(value));
                let v = self.gen_expr(value);
                self.declare(name);
                format!("{}{} {} = {};", self.ind(), t, name, v)
            }
            Statement::FnDecl { name, params, return_type, body, .. } => {
                let ret = return_type.as_ref().map(|t| self.c_type(t)).unwrap_or("void".into());
                let ps: Vec<String> = params.iter().filter(|p| p.name != "sf")
                    .map(|p| {
                        let t = p.type_ann.as_ref().map(|t| self.c_type(t)).unwrap_or("void*".into());
                        format!("{} {}", t, p.name)
                    }).collect();
                self.scopes.push(std::collections::HashSet::new());
                for p in params { self.declare(&p.name); }
                let mut out = format!("{}{} {}({}) {{\n", self.ind(), ret, name, ps.join(", "));
                self.indent += 1;
                out.push_str(&self.gen_block(body));
                self.indent -= 1;
                self.scopes.pop();
                out.push_str(&format!("{}}}\n", self.ind()));
                out
            }
            Statement::Expr(e) => format!("{}{};", self.ind(), self.gen_expr(e)),
            Statement::Return(Some(e)) => format!("{}return {};", self.ind(), self.gen_expr(e)),
            Statement::Return(None) => format!("{}return;", self.ind()),
            Statement::Break(_) => format!("{}break;", self.ind()),
            Statement::Continue => format!("{}continue;", self.ind()),
            Statement::Throw(Some(e)) => format!("{}/* throw */ exit(1); /* {} */", self.ind(), self.gen_expr(e)),
            Statement::Throw(None) => format!("{}exit(1);", self.ind()),
            Statement::Assignment { target, op, value } => {
                let o = match op {
                    AssignOp::Assign => "=", AssignOp::AddAssign => "+=",
                    AssignOp::SubAssign => "-=", AssignOp::MulAssign => "*=",
                    AssignOp::DivAssign => "/=", AssignOp::ModAssign => "%=",
                };
                // Nebula bare `x = 5` at first use is a new binding in the
                // source languages with implicit declaration (Python, JS).
                // C requires explicit declaration — infer a type and emit
                // one for simple-ident first-use assignments.
                if matches!(op, AssignOp::Assign) {
                    if let Expr::Ident(name) = target {
                        if !self.is_declared(name) {
                            let v = self.gen_expr(value);
                            let t = self.infer_c_type(value);
                            self.declare(name);
                            return format!("{}{} {} = {};", self.ind(), t, name, v);
                        }
                    }
                }
                format!("{}{} {} {};", self.ind(), self.gen_expr(target), o, self.gen_expr(value))
            }
            Statement::LetDestructure { names, value, .. } => {
                format!("{}/* destructure */ {} = {};", self.ind(), names.join(", "), self.gen_expr(value))
            }
            // Iter-24 Bug D: explicit Use/Module handlers for C.
            Statement::Use { path, .. } => {
                // Best-effort: treat last path segment as a header name.
                let name = path.last().cloned().unwrap_or_default();
                format!("{}#include <{}.h>", self.ind(), name)
            }
            Statement::Module { .. } => {
                // C has no module statement.
                String::new()
            }
            _ => format!("{}/* TODO */", self.ind()),
        }
    }

    fn gen_block(&mut self, block: &Block) -> String {
        self.scopes.push(std::collections::HashSet::new());
        let mut out = String::new();
        for s in &block.statements { out.push_str(&self.gen_stmt(s)); out.push('\n'); }
        if let Some(e) = &block.expr {
            // Don't wrap statement-like exprs in return
            let is_stmt_like = matches!(e.as_ref(),
                Expr::For { .. } | Expr::While { .. } | Expr::Loop { .. }
                | Expr::If { .. } | Expr::Match { .. } | Expr::TryCatch { .. }
            );
            if is_stmt_like {
                out.push_str(&format!("{}{};\n", self.ind(), self.gen_expr(e)));
            } else {
                out.push_str(&format!("{}return {};\n", self.ind(), self.gen_expr(e)));
            }
        }
        self.scopes.pop();
        out
    }

    fn infer_c_type(&self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(_) => "int64_t".into(),
            Expr::FloatLit(_) => "double".into(),
            Expr::BoolLit(_) => "int".into(),
            Expr::StringLit(_) => "const char*".into(),
            Expr::CharLit(_) => "char".into(),
            Expr::Nil => "void*".into(),
            // Arithmetic and comparisons are typed by their operands; lacking
            // a real type inferencer we fall back to int64_t which matches
            // the dominant integer width Nebula emits.
            Expr::BinaryOp { op, left, right } => match op {
                BinOp::Eq | BinOp::NotEq | BinOp::Less | BinOp::Greater
                | BinOp::LessEq | BinOp::GreaterEq | BinOp::And | BinOp::Or => "int".into(),
                _ => {
                    let l = self.infer_c_type(left);
                    let r = self.infer_c_type(right);
                    if l == "double" || r == "double" { "double".into() } else { "int64_t".into() }
                }
            },
            Expr::UnaryOp { op, operand } => match op {
                UnaryOp::Not => "int".into(),
                _ => self.infer_c_type(operand),
            },
            Expr::Index { .. } | Expr::FieldAccess { .. } | Expr::Call { .. }
            | Expr::MethodCall { .. } | Expr::Ident(_) => "int64_t".into(),
            _ => "int64_t".into(),
        }
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n) => format!("{n}LL"),
            Expr::FloatLit(f) => format!("{f:?}"),
            Expr::BoolLit(b) => if *b { "1" } else { "0" }.into(),
            Expr::StringLit(s) => {
                let has_interp = s.parts.iter().any(|p| matches!(p, StringPart::Interpolation(_)));
                if !has_interp {
                    let text: String = s.parts.iter().map(|p| if let StringPart::Literal(t) = p { t.as_str() } else { "" }).collect();
                    return format!("\"{}\"", text);
                }
                // For interpolated strings, generate sprintf-style
                let mut fmt = String::new();
                let mut args = Vec::new();
                for part in &s.parts {
                    match part {
                        StringPart::Literal(t) => fmt.push_str(t),
                        StringPart::Interpolation(tokens) => {
                            fmt.push_str("%s");
                            let a: String = tokens.iter().map(|t| interpolation_token_to_target(t, "self")).collect();
                            args.push(a);
                        }
                    }
                }
                if args.is_empty() {
                    format!("\"{}\"", fmt)
                } else {
                    format!("/* fmt: \"{}\" with {} */\"{}\"", fmt, args.join(", "), fmt)
                }
            }
            Expr::CharLit(c) => format!("'{c}'"),
            Expr::Nil => "NULL".into(),
            Expr::Ident(n) => if n == "sf" { "self".into() } else { n.clone() },
            Expr::BinaryOp { left, op, right } => {
                let o = match op {
                    BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/",
                    BinOp::Mod => "%", BinOp::Pow => "/* pow */", BinOp::Eq => "==", BinOp::NotEq => "!=",
                    BinOp::Less => "<", BinOp::Greater => ">", BinOp::LessEq => "<=", BinOp::GreaterEq => ">=",
                    BinOp::And => "&&", BinOp::Or => "||", BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^", BinOp::ShiftLeft => "<<", BinOp::ShiftRight => ">>",
                };
                format!("({} {} {})", self.gen_expr(left), o, self.gen_expr(right))
            }
            Expr::UnaryOp { op, operand } => {
                let o = match op { UnaryOp::Neg => "-", UnaryOp::Not => "!", UnaryOp::BitNot => "~" };
                format!("{}{}", o, self.gen_expr(operand))
            }
            Expr::Call { callee, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let c = self.gen_expr(callee);
                match c.as_str() {
                    "prt" | "print" | "println" => {
                        if a.len() == 1 {
                            format!("printf(\"%s\\n\", {})", a[0])
                        } else {
                            format!("printf({})", a.join(", "))
                        }
                    }
                    _ => format!("{}({})", c, a.join(", ")),
                }
            }
            Expr::MethodCall { object, method, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let obj = self.gen_expr(object);
                format!("{}_{}{}{}", obj, method, if a.is_empty() { "" } else { "(" }, if a.is_empty() { "".into() } else { format!("{})", a.join(", ")) })
            }
            Expr::FieldAccess { object, field } => format!("{}.{}", self.gen_expr(object), field),
            Expr::Index { object, index } => format!("{}[{}]", self.gen_expr(object), self.gen_expr(index)),
            Expr::Slice { object, start, end } => {
                // C has no slice type. Emit pointer-plus-start as the closest form;
                // the length component is lost. Hosts using Nebula→C on slice-heavy
                // code should restructure to pass (ptr, len) pairs explicitly.
                let obj = self.gen_expr(object);
                let base = match start.as_ref() {
                    Some(s) => format!("({} + {})", obj, self.gen_expr(s)),
                    None => obj,
                };
                if end.is_some() {
                    format!("{} /* nebula: slice length lost in C */", base)
                } else {
                    base
                }
            }
            Expr::If { condition, then_block, else_block } => {
                if let (Some(te), Some(eb)) = (&then_block.expr, else_block) {
                    if then_block.statements.is_empty() {
                        if let Some(ee) = &eb.expr { if eb.statements.is_empty() {
                            return format!("({} ? {} : {})", self.gen_expr(condition), self.gen_expr(te), self.gen_expr(ee));
                        }}
                    }
                }
                let mut out = format!("if ({}) {{\n", self.gen_expr(condition));
                self.indent += 1; out.push_str(&self.gen_block(then_block)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                if let Some(eb) = else_block {
                    out.push_str(" else {\n");
                    self.indent += 1; out.push_str(&self.gen_block(eb)); self.indent -= 1;
                    out.push_str(&format!("{}}}", self.ind()));
                }
                out
            }
            Expr::For { binding, iterable, body, .. } => {
                let mut out = format!("for (int _i = 0; _i < /* len */; _i++) {{\n");
                self.indent += 1;
                out.push_str(&format!("{}auto {} = {}[_i];\n", self.ind(), binding, self.gen_expr(iterable)));
                out.push_str(&self.gen_block(body));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::While { condition, body } => {
                let mut out = format!("while ({}) {{\n", self.gen_expr(condition));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::Loop { body } => {
                let mut out = String::from("while (1) {\n");
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::Range { start, end, .. } => {
                format!("/* range({}, {}) */", self.gen_expr(start), self.gen_expr(end))
            }
            _ => "/* TODO */".into(),
        }
    }
}

#[allow(dead_code)]
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

#[allow(non_snake_case)]
fn camelCase(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_lowercase().to_string() + c.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    fn translate(source: &str) -> String {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        to_python(&program)
    }

    fn translate_js(source: &str) -> String {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        to_javascript(&program)
    }

    #[test]
    fn test_let_to_python() {
        let py = translate("lt x = 42");
        assert!(py.contains("x = 42"));
    }

    #[test]
    fn test_fn_to_python() {
        let py = translate("fn add(a, b) = a + b");
        assert!(py.contains("def add(a, b):"));
        assert!(py.contains("return (a + b)"));
    }

    #[test]
    fn test_for_to_python() {
        let py = translate("fo x in items {\n  print(x)\n}");
        assert!(py.contains("for x in items:"));
    }

    #[test]
    fn test_if_ternary() {
        let py = translate("if x > 0 { 1 } el { 2 }");
        assert!(py.contains("1 if (x > 0) else 2"));
    }

    #[test]
    fn test_bool_translation() {
        let py = translate("lt a = tt\nlt b = ff");
        assert!(py.contains("True"));
        assert!(py.contains("False"));
    }

    #[test]
    fn test_nil_to_none() {
        let py = translate("lt x = nil");
        assert!(py.contains("None"));
    }

    #[test]
    fn test_pipe_to_function_call() {
        let py = translate("x |> double");
        assert!(py.contains("double(x)"));
    }

    #[test]
    fn test_struct_to_class() {
        let py = translate("st Point {\n  x: Float\n  y: Float\n}");
        assert!(py.contains("class Point:"));
        assert!(py.contains("def __init__(self, x, y):"));
    }

    #[test]
    fn test_lambda_to_python() {
        let py = translate("lt f = |x| x * 2");
        assert!(py.contains("lambda x: (x * 2)"));
    }

    #[test]
    fn test_range_to_python() {
        let py = translate("0..10");
        assert!(py.contains("range(0, 10)"));
    }

    // --- JavaScript tests ---

    #[test]
    fn test_let_to_js() {
        let js = translate_js("lt x = 42");
        assert!(js.contains("const x = 42;"));
    }

    #[test]
    fn test_mut_to_js() {
        let js = translate_js("mt x = 0");
        assert!(js.contains("let x = 0;"));
    }

    #[test]
    fn test_fn_to_js() {
        let js = translate_js("fn add(a, b) = a + b");
        assert!(js.contains("function add(a, b)"));
        assert!(js.contains("return (a + b);"));
    }

    #[test]
    fn test_bool_to_js() {
        let js = translate_js("lt a = tt\nlt b = ff");
        assert!(js.contains("true"));
        assert!(js.contains("false"));
    }

    #[test]
    fn test_nil_to_js() {
        let js = translate_js("lt x = nil");
        assert!(js.contains("null"));
    }

    #[test]
    fn test_equality_to_js() {
        let js = translate_js("x == y");
        assert!(js.contains("==="));
    }

    #[test]
    fn test_print_to_console_log() {
        let js = translate_js("print(42)");
        assert!(js.contains("console.log(42)"));
    }

    #[test]
    fn test_string_interp_to_template_literal() {
        let js = translate_js(r#"lt name = "world"
"Hello, {name}!""#);
        assert!(js.contains("`Hello, ${name}!`"));
    }

    #[test]
    fn test_nil_coalesce_to_js() {
        let js = translate_js("x ?? 42");
        assert!(js.contains("(x ?? 42)"));
    }

    #[test]
    fn test_lambda_to_arrow() {
        let js = translate_js("lt f = |x| x * 2");
        assert!(js.contains("(x) => (x * 2)"));
    }

    #[test]
    fn test_struct_to_js_class() {
        let js = translate_js("st Point {\n  x: Float\n  y: Float\n}");
        assert!(js.contains("class Point {"));
        assert!(js.contains("constructor(x, y)"));
        assert!(js.contains("this.x = x;"));
    }

    #[test]
    fn test_for_to_js() {
        let js = translate_js("fo x in items {\n  print(x)\n}");
        assert!(js.contains("for (const x of items)"));
    }

    #[test]
    fn test_if_ternary_js() {
        let js = translate_js("if x > 0 { 1 } el { 2 }");
        assert!(js.contains("?"));
        assert!(js.contains(":"));
    }

    #[test]
    fn test_pipe_to_js() {
        let js = translate_js("x |> double");
        assert!(js.contains("double(x)"));
    }

    #[test]
    fn test_self_to_this() {
        let js = translate_js(r#"sf.x"#);
        assert!(js.contains("this.x"));
    }

    // --- Go tests ---

    fn translate_go(source: &str) -> String {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        to_go(&program)
    }

    #[test]
    fn test_go_let() {
        // Top-level `lt` now emits `const` (Go forbids `:=` at package level).
        let go = translate_go("lt x = 42");
        assert!(go.contains("const x = 42"), "got: {}", go);
    }

    #[test]
    fn test_go_fn() {
        let go = translate_go("fn add(a, b) = a + b");
        assert!(go.contains("func add("));
    }

    #[test]
    fn test_go_print() {
        let go = translate_go("print(42)");
        assert!(go.contains("fmt.Println(42)"));
    }

    #[test]
    fn test_go_for() {
        let go = translate_go("fo x in items { print(x) }");
        // Single-binding for-in iterates values, so Go form is `_, x := range`.
        assert!(go.contains("for _, x := range items"));
    }

    #[test]
    fn test_go_package_header() {
        let go = translate_go("lt x = 1");
        assert!(go.starts_with("package main"));
    }

    #[test]
    fn test_go_match() {
        let go = translate_go("fn f(x) = mc x {\n  0 => \"zero\"\n  _ => \"other\"\n}");
        assert!(go.contains("case 0"));
        assert!(go.contains("return \"zero\""));
    }

    #[test]
    fn test_go_match_with_guard() {
        let go = translate_go("fn f(x) = mc x {\n  _ if x > 0 => \"pos\"\n  _ => \"neg\"\n}");
        assert!(go.contains("default"));
    }

    #[test]
    fn test_go_string_literal() {
        let go = translate_go(r#"lt name = "World""#);
        assert!(go.contains(r#"const name = "World""#), "got: {}", go);
    }

    // --- Swift tests ---

    fn translate_swift(source: &str) -> String {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        to_swift(&program)
    }

    #[test]
    fn test_swift_let() {
        let sw = translate_swift("lt x = 42");
        assert!(sw.contains("let x = 42"));
    }

    #[test]
    fn test_swift_mut() {
        let sw = translate_swift("mt x = 0");
        assert!(sw.contains("var x = 0"));
    }

    #[test]
    fn test_swift_fn() {
        let sw = translate_swift("fn add(a, b) = a + b");
        assert!(sw.contains("func add("));
    }

    #[test]
    fn test_swift_nil_coalesce() {
        let sw = translate_swift("x ?? 42");
        assert!(sw.contains("?? 42"));
    }

    #[test]
    fn test_swift_range() {
        let sw = translate_swift("0..10");
        assert!(sw.contains("0..<10"));
        let sw2 = translate_swift("0..=10");
        assert!(sw2.contains("0...10"));
    }

    #[test]
    fn test_swift_print() {
        let sw = translate_swift("print(42)");
        assert!(sw.contains("print(42)"));
    }

    // --- Kotlin tests ---

    fn translate_kt(source: &str) -> String {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        to_kotlin(&program)
    }

    #[test]
    fn test_kt_let() {
        let kt = translate_kt("lt x = 42");
        assert!(kt.contains("val x = 42"));
    }

    #[test]
    fn test_kt_mut() {
        let kt = translate_kt("mt x = 0");
        assert!(kt.contains("var x = 0"));
    }

    #[test]
    fn test_kt_fn() {
        let kt = translate_kt("fn add(a, b) = a + b");
        assert!(kt.contains("fun add("));
    }

    #[test]
    fn test_kt_nil_coalesce() {
        let kt = translate_kt("x ?? 42");
        assert!(kt.contains("?: 42"));
    }

    #[test]
    fn test_kt_print() {
        let kt = translate_kt("print(42)");
        assert!(kt.contains("println(42)"));
    }

    #[test]
    fn test_kt_nil() {
        let kt = translate_kt("lt x = nil");
        assert!(kt.contains("null"));
    }

    #[test]
    fn test_kt_struct_to_data_class() {
        let kt = translate_kt("st Pt { x: Int, y: Int }");
        assert!(kt.contains("data class Pt("));
    }

    #[test]
    fn test_kt_lambda() {
        let kt = translate_kt("|x| x * 2");
        assert!(kt.contains("{ x ->"));
    }

    #[test]
    fn test_kt_match_with_guard() {
        let kt = translate_kt("fn f(x) = mc x {\n  _ if x > 0 => \"pos\"\n  _ => \"neg\"\n}");
        assert!(kt.contains("when {"));
        assert!(kt.contains("(x > 0)"));
    }

    #[test]
    fn test_swift_match() {
        let sw = translate_swift("fn f(x) = mc x {\n  0 => \"zero\"\n  _ => \"other\"\n}");
        assert!(sw.contains("switch"));
        assert!(sw.contains("case 0"));
        assert!(sw.contains("default"));
    }

    // --- TypeScript tests ---

    fn translate_ts(source: &str) -> String {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        to_typescript(&program)
    }

    #[test]
    fn test_ts_let_typed() {
        let ts = translate_ts("lt x: Int = 42");
        assert!(ts.contains("const x: number = 42;"));
    }

    #[test]
    fn test_ts_fn_typed() {
        let ts = translate_ts("fn add(a: Int, b: Int) -> Int = a + b");
        assert!(ts.contains("function add(a: number, b: number): number"));
    }

    #[test]
    fn test_ts_struct_to_interface() {
        let ts = translate_ts("st Point {\n  x: Float\n  y: Float\n}");
        assert!(ts.contains("interface Point"));
        assert!(ts.contains("x: number;"));
    }

    #[test]
    fn test_ts_enum() {
        let ts = translate_ts("en Color { Red, Green, Blue }");
        assert!(ts.contains("enum Color"));
        assert!(ts.contains("Red ="));
    }

    #[test]
    fn test_ts_lambda() {
        let ts = translate_ts("lt f = |x| x * 2");
        assert!(ts.contains("(x) =>"));
    }

    #[test]
    fn test_ts_nil() {
        let ts = translate_ts("lt x = nil");
        assert!(ts.contains("null"));
    }

    // --- Rust codegen tests ---

    fn translate_rust(source: &str) -> String {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        to_rust_lang(&program)
    }

    #[test]
    fn test_rust_let() {
        let rs = translate_rust("lt x = 42");
        assert!(rs.contains("let x = 42;"));
    }

    #[test]
    fn test_rust_mut() {
        let rs = translate_rust("mt x = 0");
        assert!(rs.contains("let mut x = 0;"));
    }

    #[test]
    fn test_rust_fn() {
        let rs = translate_rust("fn add(a: Int, b: Int) -> Int = a + b");
        assert!(rs.contains("fn add(a: i64, b: i64) -> i64"));
    }

    #[test]
    fn test_rust_struct() {
        let rs = translate_rust("st Point {\n  x: Float\n  y: Float\n}");
        assert!(rs.contains("struct Point"));
        assert!(rs.contains("x: f64"));
    }

    #[test]
    fn test_rust_enum() {
        let rs = translate_rust("en Color { Red, Green, Blue }");
        assert!(rs.contains("enum Color"));
        assert!(rs.contains("Red,"));
    }

    #[test]
    fn test_rust_match() {
        let rs = translate_rust("fn f(x) = mc x {\n  0 => \"zero\"\n  _ => \"other\"\n}");
        assert!(rs.contains("match"));
        assert!(rs.contains("0 =>"));
        assert!(rs.contains("_ =>"));
    }

    #[test]
    fn test_rust_range() {
        let rs = translate_rust("0..10");
        assert!(rs.contains("0..10"));
        let rs2 = translate_rust("0..=10");
        assert!(rs2.contains("0..=10"));
    }

    #[test]
    fn test_rust_lambda() {
        let rs = translate_rust("|x| x * 2");
        assert!(rs.contains("|x| (x * 2)"));
    }

    // --- C tests ---

    fn translate_c(source: &str) -> String {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        to_c(&program)
    }

    #[test]
    fn test_c_includes() {
        let c = translate_c("lt x = 42");
        assert!(c.contains("#include <stdio.h>"));
    }

    #[test]
    fn test_c_let_typed() {
        let c = translate_c("lt x: Int = 42");
        assert!(c.contains("int64_t x = 42"));
    }

    #[test]
    fn test_c_fn() {
        let c = translate_c("fn add(a: Int, b: Int) -> Int = a + b");
        assert!(c.contains("int64_t add(int64_t a, int64_t b)"));
    }

    #[test]
    fn test_c_struct() {
        let c = translate_c("st Point { x: Float, y: Float }");
        assert!(c.contains("typedef struct"));
        assert!(c.contains("double x"));
        assert!(c.contains("} Point;"));
    }

    #[test]
    fn test_c_print() {
        let c = translate_c("prt(42)");
        assert!(c.contains("printf"));
    }
}
