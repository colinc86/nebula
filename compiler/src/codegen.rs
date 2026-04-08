/// Code generation: Nebula AST → target language source code.
///
/// Supports: Python, JavaScript, Go, Swift, Kotlin
use crate::ast::*;
use crate::lexer::{StringPart, Token};

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
        Token::Colon => ": ".into(),
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
        Token::Colon => ": ".into(),
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
    let mut gen = GoGen { indent: 0 };
    gen.gen_program(program)
}

pub fn to_swift(program: &Program) -> String {
    let mut gen = SwiftGen { indent: 0 };
    gen.gen_program(program)
}

pub fn to_kotlin(program: &Program) -> String {
    let mut gen = KotlinGen { indent: 0 };
    gen.gen_program(program)
}

pub fn to_typescript(program: &Program) -> String {
    let mut gen = TsGen { indent: 0 };
    gen.gen_program(program)
}

pub fn to_rust_lang(program: &Program) -> String {
    let mut gen = RustGen { indent: 0 };
    gen.gen_program(program)
}

pub fn to_c(program: &Program) -> String {
    let mut gen = CGen { indent: 0 };
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
                out.push_str(&self.gen_block_body(body));
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
                        out.push_str(&self.gen_block_body(body));
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
            Statement::Use { path, .. } => {
                format!("{}import {}", self.ind(), path.join("."))
            }
            Statement::LetDestructure { names, value, .. } => {
                format!("{}{} = {}", self.ind(), names.join(", "), self.gen_expr(value))
            }
            Statement::TypeAlias { name, type_expr } => {
                format!("{}{} = {}", self.ind(), name, self.gen_type(type_expr))
            }
            Statement::EnumDecl { .. }
            | Statement::TraitDecl { .. }
            | Statement::Module { .. } => {
                format!("{}pass  # TODO: translate", self.ind())
            }
        }
    }

    fn gen_block_body(&mut self, block: &Block) -> String {
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
            out.push_str(&format!("{}return {}\n", self.ind(), self.gen_expr(expr)));
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
                    "typ" | "type_of" => "type",
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
                // Resolve Nebula method names (including short aliases) to Python
                let py_method = match method.as_str() {
                    "mp" | "map" => "map",
                    "fl" | "filter" => "filter",
                    "fd" | "fold" => "fold",
                    "rd" | "reduce" => "reduce",
                    "srt" | "sort" => "sort",
                    "rv" | "reverse" => "reverse",
                    "jn" | "join" => "join",
                    "psh" | "push" => "append",
                    "con" | "contains" => "contains",
                    "ck" | "contains_key" => "contains_key",
                    "ln" | "len" => "len",
                    "unq" | "unique" => "unique",
                    "cnt" | "count" => "count",
                    "to_upper" => "upper",
                    "to_lower" => "lower",
                    "is_empty" => "is_empty",
                    other => other,
                };
                // len() is a global function in Python
                if py_method == "len" {
                    return format!("len({})", obj);
                }
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
                format!("{}[{}]", self.gen_expr(object), self.gen_expr(index))
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

                // Multi-line if
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
                let mut out = format!("match {}:\n", self.gen_expr(subject));
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
                    out.push_str(&format!(
                        "{}return {}\n",
                        self.ind(),
                        self.gen_expr(&arm.body)
                    ));
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
                    format!(
                        "{}for {} in enumerate({}):\n",
                        self.ind(),
                        binding_str,
                        iter_expr
                    )
                } else {
                    format!("{}for {} in {}:\n", self.ind(), binding_str, iter_expr)
                };
                self.indent += 1;
                out.push_str(&self.gen_block_body(body));
                self.indent -= 1;
                out
            }

            Expr::While { condition, body } => {
                let mut out = format!("{}while {}:\n", self.ind(), self.gen_expr(condition));
                self.indent += 1;
                out.push_str(&self.gen_block_body(body));
                self.indent -= 1;
                out
            }

            Expr::Loop { body } => {
                let mut out = format!("{}while True:\n", self.ind());
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
            Pattern::Variant(name, _) => name.clone(),
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
        for stmt in &program.statements {
            out.push_str(&self.gen_statement(stmt));
            out.push('\n');
        }
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
            Statement::EnumDecl { .. }
            | Statement::TraitDecl { .. }
            | Statement::Module { .. } => {
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
                | Expr::If { .. } | Expr::Match { .. }
            );
            if is_stmt_like {
                out.push_str(&format!("{}{};\n", self.ind(), self.gen_expr(expr)));
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
}

impl GoGen {
    fn ind(&self) -> String { "\t".repeat(self.indent) }

    fn gen_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Named(n) => match n.as_str() {
                "Int" => "int".into(),
                "Float" => "float64".into(),
                "Bool" => "bool".into(),
                "String" | "Str" => "string".into(),
                "Error" => "error".into(),
                // Reverse qualified type merging: TimeTime → time.Time, SyncWaitGroup → sync.WaitGroup
                "TimeTime" => "time.Time".into(),
                "TimeDuration" => "time.Duration".into(),
                "SyncMutex" => "sync.Mutex".into(),
                "SyncRWMutex" => "sync.RWMutex".into(),
                "SyncWaitGroup" => "sync.WaitGroup".into(),
                "StringsBuilder" => "strings.Builder".into(),
                "JsonMarshal" => "json.Marshal".into(),
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
        let mut out = "package main\n\n".to_string();
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
        if body.contains("sync.") { imports.push("\"sync\""); }
        if body.contains("time.") { imports.push("\"time\""); }
        if body.contains("json.") { imports.push("\"encoding/json\""); }
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
                match ty {
                    Some(t) => format!("{}var {} {} = {}", self.ind(), name, t, self.gen_expr(value)),
                    None => format!("{}{} := {}", self.ind(), name, self.gen_expr(value)),
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
                let mut out = format!("{}func {}({}){} {{\n", self.ind(), name, ps.join(", "), ret);
                self.indent += 1;
                out.push_str(&self.gen_block(body));
                self.indent -= 1;
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
                format!("{}{} {} {}", self.ind(), self.gen_expr(target), o, self.gen_expr(value))
            }
            Statement::Break(_) => format!("{}break", self.ind()),
            Statement::Continue => format!("{}continue", self.ind()),
            Statement::LetDestructure { names, value, .. } => {
                format!("{}{} := {}", self.ind(), names.join(", "), self.gen_expr(value))
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
                                interpolation_token_to_target(t, "self")
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
                // Reverse Nebula method translations back to Go builtins
                match method.as_str() {
                    "remove" => format!("delete({}, {})", obj, a.join(", ")),
                    "send" => format!("{} <- {}", obj, a.join(", ")),
                    "recv" => format!("<-{}", obj),
                    "close" => format!("close({})", obj),
                    _ => format!("{}.{}({})", obj, method, a.join(", ")),
                }
            }
            Expr::FieldAccess { object, field } => {
                // Preserve field names as-is (Go struct fields keep original casing)
                format!("{}.{}", self.gen_expr(object), field)
            }
            Expr::Index { object, index } => format!("{}[{}]", self.gen_expr(object), self.gen_expr(index)),
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
                let range_var = match index_binding {
                    Some(idx) => format!("{}, {}", idx, binding),
                    None => binding.clone(),
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
                        Pattern::Wildcard | Pattern::Binding(_) => {
                            out.push_str(&format!("{}default:\n", self.ind()));
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
            Statement::LetDestructure { names, value, .. } => {
                format!("{}({}) = {}", self.ind(), names.join(", "), self.gen_expr(value))
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
            Expr::For { binding, iterable, body, .. } => {
                let mut out = format!("for {} in {} {{\n", binding, self.gen_expr(iterable));
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
                for arm in arms {
                    match &arm.pattern {
                        Pattern::Wildcard | Pattern::Binding(_) => {
                            if let Some(guard) = &arm.guard {
                                out.push_str(&format!("{}case _ where {}:\n", self.ind(), self.gen_expr(guard)));
                            } else {
                                out.push_str(&format!("{}default:\n", self.ind()));
                            }
                        }
                        Pattern::Literal(lit) => {
                            out.push_str(&format!("{}case {}:\n", self.ind(), self.gen_expr(lit)));
                        }
                        Pattern::Range(start, end, inclusive) => {
                            let op = if *inclusive { "..." } else { "..<" };
                            out.push_str(&format!("{}case {}{}{}:\n", self.ind(), self.gen_expr(start), op, self.gen_expr(end)));
                        }
                        _ => out.push_str(&format!("{}// pattern TODO\n", self.ind())),
                    }
                    self.indent += 1;
                    out.push_str(&format!("{}return {}\n", self.ind(), self.gen_expr(&arm.body)));
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
}

impl KotlinGen {
    fn ind(&self) -> String { "    ".repeat(self.indent) }

    fn gen_program(&mut self, program: &Program) -> String {
        let mut out = String::new();
        for stmt in &program.statements { out.push_str(&self.gen_stmt(stmt)); out.push('\n'); }
        out
    }

    fn gen_stmt(&mut self, stmt: &Statement) -> String {
        match stmt {
            Statement::Let { name, value, .. } | Statement::Const { name, value, .. } => {
                format!("{}val {} = {}", self.ind(), name, self.gen_expr(value))
            }
            Statement::Mut { name, value, .. } => {
                format!("{}var {} = {}", self.ind(), name, self.gen_expr(value))
            }
            Statement::FnDecl { name, params, body, .. } => {
                let ps: Vec<String> = params.iter()
                    .filter(|p| p.name != "sf")
                    .map(|p| format!("{}: Any", p.name))
                    .collect();
                let mut out = format!("{}fun {}({}): Any {{\n", self.ind(), name, ps.join(", "));
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::StructDecl { name, fields, .. } => {
                let fs: Vec<String> = fields.iter().map(|f| format!("val {}: Any", f.name)).collect();
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
                format!("{}{} {} {}", self.ind(), self.gen_expr(target), o, self.gen_expr(value))
            }
            Statement::Break(_) => format!("{}break", self.ind()),
            Statement::Continue => format!("{}continue", self.ind()),
            Statement::LetDestructure { names, value, .. } => {
                format!("{}({}) = {}", self.ind(), names.join(", "), self.gen_expr(value))
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
                else { format!("{}({})", c, a.join(", ")) }
            }
            Expr::MethodCall { object, method, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let m = match method.as_str() {
                    "fold" => "fold", "any" => "any", "all" => "all",
                    "to_upper" => "uppercase", "to_lower" => "lowercase",
                    other => other,
                };
                format!("{}.{}({})", self.gen_expr(object), m, a.join(", "))
            }
            Expr::FieldAccess { object, field } => format!("{}.{}", self.gen_expr(object), field),
            Expr::Index { object, index } => format!("{}[{}]", self.gen_expr(object), self.gen_expr(index)),
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
            Expr::For { binding, iterable, body, .. } => {
                let mut out = format!("for ({} in {}) {{\n", binding, self.gen_expr(iterable));
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
                format!("listOf({})", i.join(", "))
            }
            Expr::NilCoalesce { left, right } => format!("({} ?: {})", self.gen_expr(left), self.gen_expr(right)),
            Expr::Range { start, end, inclusive } => {
                if *inclusive { format!("{}..{}", self.gen_expr(start), self.gen_expr(end)) }
                else { format!("{} until {}", self.gen_expr(start), self.gen_expr(end)) }
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
                        _ => out.push_str(&format!("{}// pattern TODO\n", self.ind())),
                    }
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
}

impl RustGen {
    fn ind(&self) -> String { "    ".repeat(self.indent) }

    fn gen_program(&mut self, program: &Program) -> String {
        let mut out = String::new();
        for stmt in &program.statements { out.push_str(&self.gen_stmt(stmt)); out.push('\n'); }
        out
    }

    fn neb_type(&self, t: &TypeExpr) -> String {
        match t {
            TypeExpr::Named(n) => match n.as_str() {
                "Int" => "i64".into(), "Float" => "f64".into(),
                "String" => "String".into(), "Bool" => "bool".into(),
                "Char" => "char".into(), "Nil" => "()".into(),
                other => other.to_string(),
            },
            TypeExpr::Generic(name, args) => {
                let a: Vec<String> = args.iter().map(|a| self.neb_type(a)).collect();
                match name.as_str() {
                    "List" => format!("Vec<{}>", a.first().unwrap_or(&"_".into())),
                    "Map" => format!("HashMap<{}, {}>", a.first().unwrap_or(&"String".into()), a.get(1).unwrap_or(&"_".into())),
                    _ => format!("{}<{}>", name, a.join(", ")),
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
                format!("{}let {}{} = {};", self.ind(), name, t, self.gen_expr(value))
            }
            Statement::Mut { name, type_ann, value } => {
                let t = type_ann.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                format!("{}let mut {}{} = {};", self.ind(), name, t, self.gen_expr(value))
            }
            Statement::Const { name, type_ann, value } => {
                let t = type_ann.as_ref().map(|t| format!(": {}", self.neb_type(t))).unwrap_or_default();
                format!("{}const {}{} = {};", self.ind(), name, t, self.gen_expr(value))
            }
            Statement::FnDecl { name, params, return_type, body, .. } => {
                let ps: Vec<String> = params.iter().filter(|p| p.name != "sf")
                    .map(|p| {
                        let t = p.type_ann.as_ref().map(|t| self.neb_type(t)).unwrap_or("_".into());
                        format!("{}: {}", p.name, t)
                    }).collect();
                let ret = return_type.as_ref().map(|t| format!(" -> {}", self.neb_type(t))).unwrap_or_default();
                let mut out = format!("{}fn {}({}){} {{\n", self.ind(), name, ps.join(", "), ret);
                self.indent += 1; out.push_str(&self.gen_block(body)); self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::StructDecl { name, fields, generics, .. } => {
                let gen = if generics.is_empty() { String::new() } else { format!("<{}>", generics.join(", ")) };
                let mut out = format!("{}struct {}{} {{\n", self.ind(), name, gen);
                self.indent += 1;
                for f in fields { out.push_str(&format!("{}{}: {},\n", self.ind(), f.name, self.neb_type(&f.type_ann))); }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::EnumDecl { name, variants, generics, .. } => {
                let gen = if generics.is_empty() { String::new() } else { format!("<{}>", generics.join(", ")) };
                let mut out = format!("{}enum {}{} {{\n", self.ind(), name, gen);
                self.indent += 1;
                for v in variants {
                    if v.fields.is_empty() {
                        out.push_str(&format!("{}{},\n", self.ind(), v.name));
                    } else {
                        let fs: Vec<String> = v.fields.iter().map(|f| {
                            if let Some(n) = &f.name { format!("{}: {}", n, self.neb_type(&f.type_ann)) }
                            else { self.neb_type(&f.type_ann) }
                        }).collect();
                        out.push_str(&format!("{}{}({}),\n", self.ind(), v.name, fs.join(", ")));
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
                for m in methods { out.push_str(&self.gen_stmt(m)); out.push('\n'); }
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
                    out.push_str(&format!("{}fn {}({}){}{}\n", self.ind(), m.name, ps.join(", "), ret, if m.body.is_some() { " { ... }" } else { ";" }));
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
            Statement::Assignment { target, op, value } => {
                let o = match op { AssignOp::Assign => "=", AssignOp::AddAssign => "+=", AssignOp::SubAssign => "-=", AssignOp::MulAssign => "*=", AssignOp::DivAssign => "/=", AssignOp::ModAssign => "%=" };
                format!("{}{} {} {};", self.ind(), self.gen_expr(target), o, self.gen_expr(value))
            }
            Statement::Use { path, .. } => format!("{}use {};", self.ind(), path.join("::")),
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
        let mut out = String::new();
        for s in &block.statements { out.push_str(&self.gen_stmt(s)); out.push('\n'); }
        if let Some(e) = &block.expr { out.push_str(&format!("{}{}\n", self.ind(), self.gen_expr(e))); }
        out
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n) => n.to_string(),
            Expr::FloatLit(f) => format!("{f:?}"),
            Expr::BoolLit(b) => b.to_string(),
            Expr::StringLit(s) => {
                let has_interp = s.parts.iter().any(|p| matches!(p, StringPart::Interpolation(_)));
                if !has_interp {
                    let text: String = s.parts.iter().map(|p| if let StringPart::Literal(t) = p { t.as_str() } else { "" }).collect();
                    return format!("\"{}\"", text);
                }
                let mut out = String::from("format!(\"");
                let mut args = Vec::new();
                for part in &s.parts {
                    match part {
                        StringPart::Literal(t) => out.push_str(&t.replace('\n', "\\n").replace('\t', "\\t")),
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
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                let c = self.gen_expr(callee);
                match c.as_str() {
                    "prt" | "print" | "println" => format!("println!(\"{}\", {})", "{}".repeat(a.len()), a.join(", ")),
                    _ => format!("{}({})", c, a.join(", ")),
                }
            }
            Expr::MethodCall { object, method, args } => {
                let a: Vec<String> = args.iter().map(|a| self.gen_expr(&a.value)).collect();
                format!("{}.{}({})", self.gen_expr(object), method, a.join(", "))
            }
            Expr::FieldAccess { object, field } => format!("{}.{}", self.gen_expr(object), field),
            Expr::Index { object, index } => format!("{}[{}]", self.gen_expr(object), self.gen_expr(index)),
            Expr::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                format!("|{}| {}", ps.join(", "), self.gen_expr(body))
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
                    let pat = match &arm.pattern {
                        Pattern::Wildcard => "_".into(),
                        Pattern::Binding(n) => n.clone(),
                        Pattern::Literal(e) => self.gen_expr(e),
                        Pattern::Range(s, e, inc) => format!("{}..{}{}", self.gen_expr(s), if *inc { "=" } else { "" }, self.gen_expr(e)),
                        Pattern::Variant(n, _) => n.clone(),
                    };
                    let guard = arm.guard.as_ref().map(|g| format!(" if {}", self.gen_expr(g))).unwrap_or_default();
                    out.push_str(&format!("{}{}{} => {},\n", self.ind(), pat, guard, self.gen_expr(&arm.body)));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Expr::For { binding, iterable, body, .. } => {
                let mut out = format!("for {} in {} {{\n", binding, self.gen_expr(iterable));
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
            _ => "/* TODO */".into(),
        }
    }
}

// =============================================================================
// C Code Generator
// =============================================================================

struct CGen {
    indent: usize,
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
                "String" => "const char*".into(),
                "Char" => "char".into(),
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
                format!("{}{} {} = {};", self.ind(), t, name, self.gen_expr(value))
            }
            Statement::Mut { name, type_ann, value } => {
                let t = type_ann.as_ref().map(|t| self.c_type(t)).unwrap_or_else(|| self.infer_c_type(value));
                format!("{}{} {} = {};", self.ind(), t, name, self.gen_expr(value))
            }
            Statement::FnDecl { name, params, return_type, body, .. } => {
                let ret = return_type.as_ref().map(|t| self.c_type(t)).unwrap_or("void".into());
                let ps: Vec<String> = params.iter().filter(|p| p.name != "sf")
                    .map(|p| {
                        let t = p.type_ann.as_ref().map(|t| self.c_type(t)).unwrap_or("void*".into());
                        format!("{} {}", t, p.name)
                    }).collect();
                let mut out = format!("{}{} {}({}) {{\n", self.ind(), ret, name, ps.join(", "));
                self.indent += 1;
                out.push_str(&self.gen_block(body));
                self.indent -= 1;
                out.push_str(&format!("{}}}\n", self.ind()));
                out
            }
            Statement::Expr(e) => format!("{}{};", self.ind(), self.gen_expr(e)),
            Statement::Return(Some(e)) => format!("{}return {};", self.ind(), self.gen_expr(e)),
            Statement::Return(None) => format!("{}return;", self.ind()),
            Statement::Break(_) => format!("{}break;", self.ind()),
            Statement::Continue => format!("{}continue;", self.ind()),
            Statement::Assignment { target, op, value } => {
                let o = match op {
                    AssignOp::Assign => "=", AssignOp::AddAssign => "+=",
                    AssignOp::SubAssign => "-=", AssignOp::MulAssign => "*=",
                    AssignOp::DivAssign => "/=", AssignOp::ModAssign => "%=",
                };
                format!("{}{} {} {};", self.ind(), self.gen_expr(target), o, self.gen_expr(value))
            }
            Statement::LetDestructure { names, value, .. } => {
                format!("{}/* destructure */ {} = {};", self.ind(), names.join(", "), self.gen_expr(value))
            }
            _ => format!("{}/* TODO */", self.ind()),
        }
    }

    fn gen_block(&mut self, block: &Block) -> String {
        let mut out = String::new();
        for s in &block.statements { out.push_str(&self.gen_stmt(s)); out.push('\n'); }
        if let Some(e) = &block.expr { out.push_str(&format!("{}return {};\n", self.ind(), self.gen_expr(e))); }
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
            _ => "auto".into(), // C23 auto or just placeholder
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
        let go = translate_go("lt x = 42");
        assert!(go.contains("x := 42"));
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
        assert!(go.contains("for x := range items"));
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
        assert!(go.contains(r#"name := "World""#));
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
