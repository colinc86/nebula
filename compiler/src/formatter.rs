/// Nebula code formatter: AST → canonical Nebula source.
///
/// Produces consistent, terse output — the canonical form of any Nebula program.
use crate::ast::*;
use crate::lexer::StringPart;

pub fn format(program: &Program) -> String {
    let mut fmt = NebFmt { indent: 0 };
    fmt.fmt_program(program)
}

struct NebFmt {
    indent: usize,
}

impl NebFmt {
    fn ind(&self) -> String {
        "    ".repeat(self.indent)
    }

    fn fmt_program(&mut self, program: &Program) -> String {
        let mut out = String::new();
        let mut prev_was_fn = false;
        for stmt in &program.statements {
            let is_fn = matches!(
                stmt,
                Statement::FnDecl { .. }
                    | Statement::StructDecl { .. }
                    | Statement::EnumDecl { .. }
                    | Statement::TraitDecl { .. }
                    | Statement::ImplBlock { .. }
            );
            // Blank line before top-level declarations
            if is_fn && !out.is_empty() {
                out.push('\n');
            }
            if prev_was_fn && !is_fn && !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&self.fmt_statement(stmt));
            out.push('\n');
            prev_was_fn = is_fn;
        }
        out
    }

    fn fmt_statement(&mut self, stmt: &Statement) -> String {
        match stmt {
            Statement::Let { name, type_ann, value } => {
                let ann = self.fmt_optional_type(type_ann);
                format!("{}{}{} := {}", self.ind(), name, ann, self.fmt_expr(value))
            }
            Statement::Mut { name, type_ann, value } => {
                let ann = self.fmt_optional_type(type_ann);
                format!("{}{}{} = {}", self.ind(), name, ann, self.fmt_expr(value))
            }
            Statement::Const { name, type_ann, value } => {
                let ann = self.fmt_optional_type(type_ann);
                format!("{}{}{} := {}", self.ind(), name, ann, self.fmt_expr(value))
            }
            Statement::FnDecl {
                name,
                params,
                return_type,
                body,
                is_pub,
                is_test,
                decorators,
            } => {
                let mut prefix = String::new();
                for (dname, dargs) in decorators {
                    prefix.push_str(&format!("{}@{}", self.ind(), dname));
                    if !dargs.is_empty() {
                        let args_str: Vec<String> = dargs.iter().map(|a| self.fmt_expr(a)).collect();
                        prefix.push_str(&format!("({})", args_str.join(", ")));
                    }
                    prefix.push('\n');
                }
                if *is_test { prefix.push_str(&format!("{}#[test]\n", self.ind())); }
                let test_prefix = prefix;
                let pub_prefix = if *is_pub { "pub " } else { "" };
                let ps = self.fmt_params(params);
                let ret = return_type
                    .as_ref()
                    .map(|t| format!(" -> {}", self.fmt_type(t)))
                    .unwrap_or_default();

                // Short form if body is a single expression with no statements
                if body.statements.is_empty() {
                    if let Some(expr) = &body.expr {
                        return format!(
                            "{}{}{}def {}({}){} = {}",
                            test_prefix,
                            self.ind(),
                            pub_prefix,
                            name,
                            ps,
                            ret,
                            self.fmt_expr(expr)
                        );
                    }
                }

                let mut out = format!(
                    "{}{}{}def {}({}){} {{\n",
                    test_prefix,
                    self.ind(),
                    pub_prefix,
                    name,
                    ps,
                    ret
                );
                self.indent += 1;
                out.push_str(&self.fmt_block_body(body));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::StructDecl {
                name,
                generics,
                fields,
                is_pub,
            } => {
                let pub_prefix = if *is_pub { "pub " } else { "" };
                let gen = if generics.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", generics.join(", "))
                };
                let mut out = format!("{}{}struct {}{} {{\n", self.ind(), pub_prefix, name, gen);
                self.indent += 1;
                for f in fields {
                    out.push_str(&format!(
                        "{}{}: {}\n",
                        self.ind(),
                        f.name,
                        self.fmt_type(&f.type_ann)
                    ));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::EnumDecl {
                name,
                generics,
                variants,
                is_pub,
            } => {
                let pub_prefix = if *is_pub { "pub " } else { "" };
                let gen = if generics.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", generics.join(", "))
                };
                let mut out = format!("{}{}enum {}{} {{\n", self.ind(), pub_prefix, name, gen);
                self.indent += 1;
                for v in variants {
                    if v.fields.is_empty() {
                        out.push_str(&format!("{}{}\n", self.ind(), v.name));
                    } else {
                        let fs: Vec<String> = v
                            .fields
                            .iter()
                            .map(|f| {
                                if let Some(n) = &f.name {
                                    format!("{}: {}", n, self.fmt_type(&f.type_ann))
                                } else {
                                    self.fmt_type(&f.type_ann)
                                }
                            })
                            .collect();
                        out.push_str(&format!("{}{}({})\n", self.ind(), v.name, fs.join(", ")));
                    }
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::TraitDecl {
                name,
                generics,
                methods,
                is_pub,
            } => {
                let pub_prefix = if *is_pub { "pub " } else { "" };
                let gen = if generics.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", generics.join(", "))
                };
                let mut out = format!("{}{}trait {}{} {{\n", self.ind(), pub_prefix, name, gen);
                self.indent += 1;
                for m in methods {
                    let ps = self.fmt_params(&m.params);
                    let ret = m
                        .return_type
                        .as_ref()
                        .map(|t| format!(" -> {}", self.fmt_type(t)))
                        .unwrap_or_default();
                    if let Some(body) = &m.body {
                        out.push_str(&format!("{}def {}({}){} {{\n", self.ind(), m.name, ps, ret));
                        self.indent += 1;
                        out.push_str(&self.fmt_block_body(body));
                        self.indent -= 1;
                        out.push_str(&format!("{}}}\n", self.ind()));
                    } else {
                        out.push_str(&format!("{}def {}({}){}\n", self.ind(), m.name, ps, ret));
                    }
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::ImplBlock {
                trait_name,
                target,
                methods,
            } => {
                let header = if let Some(t) = trait_name {
                    format!("impl {} for {}", t, target)
                } else {
                    format!("impl {}", target)
                };
                let mut out = format!("{}{} {{\n", self.ind(), header);
                self.indent += 1;
                for (i, m) in methods.iter().enumerate() {
                    if i > 0 {
                        out.push('\n');
                    }
                    out.push_str(&self.fmt_statement(m));
                    out.push('\n');
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
            Statement::Expr(expr) => {
                format!("{}{}", self.ind(), self.fmt_expr(expr))
            }
            Statement::Return(Some(expr)) => {
                format!("{}return {}", self.ind(), self.fmt_expr(expr))
            }
            Statement::Return(None) => format!("{}return", self.ind()),
            Statement::Break(Some(expr)) => {
                format!("{}break {}", self.ind(), self.fmt_expr(expr))
            }
            Statement::Break(None) => format!("{}break", self.ind()),
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
                    self.fmt_expr(target),
                    op_str,
                    self.fmt_expr(value)
                )
            }
            Statement::Use { path, items, alias } => {
                let base = path.join(".");
                if let Some(items) = items {
                    format!("{}use {}.{{{}}}", self.ind(), base, items.join(", "))
                } else if let Some(alias) = alias {
                    format!("{}use {} as {}", self.ind(), base, alias)
                } else {
                    format!("{}use {}", self.ind(), base)
                }
            }
            Statement::Module { path } => {
                format!("{}module {}", self.ind(), path.join("."))
            }
            Statement::TypeAlias { name, type_expr } => {
                format!("{}type {} = {}", self.ind(), name, self.fmt_type(type_expr))
            }
            Statement::LetDestructure { names, value, mutable } => {
                let op = if *mutable { "=" } else { ":=" };
                format!("{}({}) {} {}", self.ind(), names.join(", "), op, self.fmt_expr(value))
            }
        }
    }

    fn fmt_block_body(&mut self, block: &Block) -> String {
        let mut out = String::new();
        for stmt in &block.statements {
            out.push_str(&self.fmt_statement(stmt));
            out.push('\n');
        }
        if let Some(expr) = &block.expr {
            out.push_str(&format!("{}{}\n", self.ind(), self.fmt_expr(expr)));
        }
        out
    }

    fn fmt_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n) => n.to_string(),
            Expr::FloatLit(f) => {
                if *f == (*f as i64) as f64 {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
            Expr::BoolLit(b) => if *b { "True" } else { "False" }.to_string(),
            Expr::StringLit(s) => {
                let mut inner = String::new();
                for part in &s.parts {
                    match part {
                        StringPart::Literal(text) => inner.push_str(text),
                        StringPart::Interpolation(tokens) => {
                            inner.push('{');
                            for tok in tokens {
                                inner.push_str(&crate::codegen::interpolation_token_to_nebula(tok));
                            }
                            inner.push('}');
                        }
                    }
                }
                format!("\"{}\"", inner)
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
                format!("{} {} {}", self.fmt_expr(left), op_str, self.fmt_expr(right))
            }

            Expr::UnaryOp { op, operand } => {
                let op_str = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "not ",
                    UnaryOp::BitNot => "~",
                };
                format!("{}{}", op_str, self.fmt_expr(operand))
            }

            Expr::Call { callee, args } => {
                let args_str = self.fmt_call_args(args);
                format!("{}({})", self.fmt_expr(callee), args_str)
            }

            Expr::MethodCall { object, method, args } => {
                let args_str = self.fmt_call_args(args);
                format!("{}.{}({})", self.fmt_expr(object), method, args_str)
            }

            Expr::FieldAccess { object, field } => {
                format!("{}.{}", self.fmt_expr(object), field)
            }

            Expr::Index { object, index } => {
                format!("{}[{}]", self.fmt_expr(object), self.fmt_expr(index))
            }

            Expr::Lambda { params, body } => {
                let ps: Vec<String> = params
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                format!("|{}| {}", ps.join(", "), self.fmt_expr(body))
            }

            Expr::If { condition, then_block, else_block } => {
                // Inline if both branches are single expressions
                if let (Some(then_expr), Some(else_b)) = (&then_block.expr, else_block) {
                    if then_block.statements.is_empty() {
                        if let Some(else_expr) = &else_b.expr {
                            if else_b.statements.is_empty() {
                                return format!(
                                    "if {} {{ {} }} else {{ {} }}",
                                    self.fmt_expr(condition),
                                    self.fmt_expr(then_expr),
                                    self.fmt_expr(else_expr)
                                );
                            }
                        }
                    }
                }

                let mut out = format!("if {} {{\n", self.fmt_expr(condition));
                self.indent += 1;
                out.push_str(&self.fmt_block_body(then_block));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                if let Some(eb) = else_block {
                    out.push_str(" else {\n");
                    self.indent += 1;
                    out.push_str(&self.fmt_block_body(eb));
                    self.indent -= 1;
                    out.push_str(&format!("{}}}", self.ind()));
                }
                out
            }

            Expr::Match { subject, arms } => {
                let mut out = format!("match {} {{\n", self.fmt_expr(subject));
                self.indent += 1;
                for arm in arms {
                    let pat = self.fmt_pattern(&arm.pattern);
                    let guard = arm
                        .guard
                        .as_ref()
                        .map(|g| format!(" if {}", self.fmt_expr(g)))
                        .unwrap_or_default();
                    out.push_str(&format!(
                        "{}{}{} => {}\n",
                        self.ind(),
                        pat,
                        guard,
                        self.fmt_expr(&arm.body)
                    ));
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }

            Expr::Block(block) => {
                let mut out = format!("{{\n");
                self.indent += 1;
                out.push_str(&self.fmt_block_body(block));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }

            Expr::List(items) => {
                let items_str: Vec<String> =
                    items.iter().map(|e| self.fmt_expr(e)).collect();
                format!("[{}]", items_str.join(", "))
            }

            Expr::Map(entries) => {
                let es: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", self.fmt_expr(k), self.fmt_expr(v)))
                    .collect();
                format!("{{{}}}", es.join(", "))
            }

            Expr::Tuple(items) => {
                let items_str: Vec<String> =
                    items.iter().map(|e| self.fmt_expr(e)).collect();
                format!("({})", items_str.join(", "))
            }

            Expr::StructLit { name, fields, spread } => {
                let mut parts = Vec::new();
                if let Some(s) = spread {
                    parts.push(format!("...{}", self.fmt_expr(s)));
                }
                for (fname, fval) in fields {
                    if let Expr::Ident(vname) = fval {
                        if vname == fname {
                            parts.push(fname.clone()); // shorthand
                            continue;
                        }
                    }
                    parts.push(format!("{}: {}", fname, self.fmt_expr(fval)));
                }
                format!("{} {{ {} }}", name, parts.join(", "))
            }

            Expr::Range { start, end, inclusive } => {
                let op = if *inclusive { "..=" } else { ".." };
                format!("{}{}{}", self.fmt_expr(start), op, self.fmt_expr(end))
            }

            Expr::Pipe { left, right } => {
                format!("{} |> {}", self.fmt_expr(left), self.fmt_expr(right))
            }

            Expr::NilCoalesce { left, right } => {
                format!("{} ?? {}", self.fmt_expr(left), self.fmt_expr(right))
            }

            Expr::Try(inner) => {
                format!("{}?", self.fmt_expr(inner))
            }

            Expr::For { binding, index_binding, iterable, body } => {
                let bind = if let Some(idx) = index_binding {
                    format!("{}, {}", idx, binding)
                } else {
                    binding.clone()
                };
                let mut out = format!("for {} in {} {{\n", bind, self.fmt_expr(iterable));
                self.indent += 1;
                out.push_str(&self.fmt_block_body(body));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }

            Expr::While { condition, body } => {
                let mut out = format!("while {} {{\n", self.fmt_expr(condition));
                self.indent += 1;
                out.push_str(&self.fmt_block_body(body));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }

            Expr::Loop { body } => {
                let mut out = format!("loop {{\n");
                self.indent += 1;
                out.push_str(&self.fmt_block_body(body));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }

            Expr::TryCatch { try_block, error_binding, catch_block } => {
                let mut out = format!("try {{\n");
                self.indent += 1;
                out.push_str(&self.fmt_block_body(try_block));
                self.indent -= 1;
                out.push_str(&format!("{}}} catch {} {{\n", self.ind(), error_binding));
                self.indent += 1;
                out.push_str(&self.fmt_block_body(catch_block));
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.ind()));
                out
            }
        }
    }

    fn fmt_call_args(&mut self, args: &[CallArg]) -> String {
        let strs: Vec<String> = args
            .iter()
            .map(|a| {
                if let Some(name) = &a.name {
                    format!("{}: {}", name, self.fmt_expr(&a.value))
                } else {
                    self.fmt_expr(&a.value)
                }
            })
            .collect();
        strs.join(", ")
    }

    fn fmt_params(&mut self, params: &[Param]) -> String {
        let strs: Vec<String> = params
            .iter()
            .map(|p| {
                let mut s = p.name.clone();
                if let Some(t) = &p.type_ann {
                    s.push_str(&format!(": {}", self.fmt_type(t)));
                }
                if let Some(d) = &p.default {
                    s.push_str(&format!(" = {}", self.fmt_expr(d)));
                }
                s
            })
            .collect();
        strs.join(", ")
    }

    fn fmt_optional_type(&mut self, type_ann: &Option<TypeExpr>) -> String {
        type_ann
            .as_ref()
            .map(|t| format!(": {}", self.fmt_type(t)))
            .unwrap_or_default()
    }

    fn fmt_type(&mut self, type_expr: &TypeExpr) -> String {
        match type_expr {
            TypeExpr::Named(name) => name.clone(),
            TypeExpr::Generic(name, args) => {
                let args_str: Vec<String> = args.iter().map(|a| self.fmt_type(a)).collect();
                format!("{}<{}>", name, args_str.join(", "))
            }
            TypeExpr::Function(params, ret) => {
                let ps: Vec<String> = params.iter().map(|p| self.fmt_type(p)).collect();
                format!("Fn({}) -> {}", ps.join(", "), self.fmt_type(ret))
            }
            TypeExpr::Tuple(items) => {
                let is: Vec<String> = items.iter().map(|i| self.fmt_type(i)).collect();
                format!("({})", is.join(", "))
            }
            TypeExpr::Option(inner) => {
                format!("{}?", self.fmt_type(inner))
            }
        }
    }

    fn fmt_pattern(&mut self, pattern: &Pattern) -> String {
        match pattern {
            Pattern::Wildcard => "_".to_string(),
            Pattern::Binding(name) => name.clone(),
            Pattern::Literal(expr) => self.fmt_expr(expr),
            Pattern::Range(start, end, inclusive) => {
                let op = if *inclusive { "..=" } else { ".." };
                format!("{}{}{}", self.fmt_expr(start), op, self.fmt_expr(end))
            }
            Pattern::Variant(name, subs) => {
                if subs.is_empty() {
                    name.clone()
                } else {
                    let ss: Vec<String> = subs.iter().map(|s| self.fmt_pattern(s)).collect();
                    format!("{}({})", name, ss.join(", "))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    fn fmt(source: &str) -> String {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        format(&program)
    }

    #[test]
    fn test_format_let() {
        // lt/let now format to walrus := (immutable binding)
        assert_eq!(fmt("lt   x  =  42").trim(), "x := 42");
        assert_eq!(fmt("let x = 42").trim(), "x := 42");
        // Walrus := also parses correctly
        assert_eq!(fmt("x := 42").trim(), "x := 42");
    }

    #[test]
    fn test_format_fn_short_form() {
        // Old "fn" parses, formats to canonical "def"
        let result = fmt("fn double(x) = x * 2");
        assert!(result.contains("def double(x) = x * 2"));
        // New "def" also works
        let result2 = fmt("def double(x) = x * 2");
        assert!(result2.contains("def double(x) = x * 2"));
    }

    #[test]
    fn test_format_fn_block() {
        let result = fmt("fn foo(x) {\n  lt y = x\n  y\n}");
        assert!(result.contains("def foo(x) {"));
        assert!(result.contains("    y := x"));
    }

    #[test]
    fn test_format_if_inline() {
        let result = fmt("if x > 0 { 1 } el { 2 }");
        assert!(result.contains("if x > 0 { 1 } else { 2 }"));
    }

    #[test]
    fn test_format_struct() {
        let result = fmt("st Point {\n  x: Float\n  y: Float\n}");
        assert!(result.contains("struct Point {"));
        assert!(result.contains("    x: Float"));
    }

    #[test]
    fn test_format_lambda() {
        let result = fmt("|x, y| x + y");
        assert!(result.contains("|x, y| x + y"));
    }

    #[test]
    fn test_format_match() {
        let result = fmt("mc x {\n  0 => \"zero\"\n  _ => \"other\"\n}");
        assert!(result.contains("match x {"));
        assert!(result.contains("0 => \"zero\""));
        assert!(result.contains("_ => \"other\""));
    }

    #[test]
    fn test_format_pipe() {
        let result = fmt("x |> double");
        assert!(result.contains("x |> double"));
    }

    #[test]
    fn test_format_range() {
        assert!(fmt("0..10").contains("0..10"));
        assert!(fmt("0..=10").contains("0..=10"));
    }

    #[test]
    fn test_roundtrip_preserves_semantics() {
        // Format, re-parse, re-format — should be identical
        let source = "def fact(n) = if n <= 1 { 1 } else { n * fact(n - 1) }";
        let formatted_once = fmt(source);
        let formatted_twice = fmt(formatted_once.trim());
        assert_eq!(formatted_once, formatted_twice);
    }
}
