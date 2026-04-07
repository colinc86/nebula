use std::collections::HashMap;
use std::fmt;

use crate::ast::*;

#[derive(Debug)]
pub struct LintWarning {
    pub rule: &'static str,
    pub message: String,
}

impl fmt::Display for LintWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.rule, self.message)
    }
}

pub fn lint(program: &Program) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    let mut ctx = LintContext::new();
    ctx.check_program(program, &mut warnings);
    warnings
}

struct LintContext {
    /// Stack of scopes tracking variable declarations and usage
    scopes: Vec<HashMap<String, VarInfo>>,
}

struct VarInfo {
    used: bool,
    mutable: bool,
    mutated: bool,
}

impl LintContext {
    fn new() -> Self {
        LintContext {
            scopes: vec![HashMap::new()],
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self, warnings: &mut Vec<LintWarning>) {
        if let Some(scope) = self.scopes.pop() {
            for (name, info) in &scope {
                if name == "_" || name == "sf" {
                    continue;
                }
                if !info.used {
                    warnings.push(LintWarning {
                        rule: "unused-variable",
                        message: format!("Variable '{name}' is declared but never used"),
                    });
                }
                if info.mutable && !info.mutated && info.used {
                    warnings.push(LintWarning {
                        rule: "unnecessary-mut",
                        message: format!(
                            "Variable '{name}' is declared as mutable but never mutated"
                        ),
                    });
                }
            }
        }
    }

    fn declare_with_warnings(&mut self, name: &str, mutable: bool, warnings: &mut Vec<LintWarning>) {
        // Check for shadowing in the same scope
        if let Some(scope) = self.scopes.last() {
            if scope.contains_key(name) && name != "_" {
                warnings.push(LintWarning {
                    rule: "shadowed-variable",
                    message: format!("Variable '{name}' shadows a previous declaration in this scope"),
                });
            }
        }
        self.declare(name, mutable);
    }

    fn declare(&mut self, name: &str, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name.to_string(),
                VarInfo {
                    used: false,
                    mutable,
                    mutated: false,
                },
            );
        }
    }

    fn mark_used(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.get_mut(name) {
                info.used = true;
                return;
            }
        }
    }

    fn mark_mutated(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.get_mut(name) {
                info.mutated = true;
                return;
            }
        }
    }

    fn check_program(&mut self, program: &Program, warnings: &mut Vec<LintWarning>) {
        for stmt in &program.statements {
            self.check_statement(stmt, warnings);
        }
        // Pop global scope
        self.pop_scope(warnings);
    }

    fn check_statement(&mut self, stmt: &Statement, warnings: &mut Vec<LintWarning>) {
        match stmt {
            Statement::Let { name, value, .. } => {
                self.check_expr(value, warnings);
                self.declare_with_warnings(name, false, warnings);
            }
            Statement::Mut { name, value, .. } => {
                self.check_expr(value, warnings);
                self.declare_with_warnings(name, true, warnings);
            }
            Statement::Const { name, value, .. } => {
                self.check_expr(value, warnings);
                self.declare_with_warnings(name, false, warnings);
            }
            Statement::FnDecl {
                name, params, body, ..
            } => {
                self.declare(name, false);
                self.mark_used(name); // functions are "used" by being declared at top level
                self.push_scope();
                for p in params {
                    self.declare(&p.name, false);
                }
                self.check_block(body, warnings);
                self.pop_scope(warnings);
            }
            Statement::Expr(expr) => {
                self.check_expr(expr, warnings);
            }
            Statement::Return(Some(expr)) => {
                self.check_expr(expr, warnings);
            }
            Statement::Break(Some(expr)) => {
                self.check_expr(expr, warnings);
            }
            Statement::Assignment { target, value, .. } => {
                if let Expr::Ident(name) = target {
                    self.mark_mutated(name);
                }
                if let Expr::Index { object, .. } = target {
                    if let Expr::Ident(name) = object.as_ref() {
                        self.mark_mutated(name);
                    }
                }
                if let Expr::FieldAccess { object, .. } = target {
                    if let Expr::Ident(name) = object.as_ref() {
                        self.mark_mutated(name);
                    }
                }
                self.check_expr(target, warnings);
                self.check_expr(value, warnings);
            }
            Statement::ImplBlock { methods, .. } => {
                for m in methods {
                    self.check_statement(m, warnings);
                }
            }
            Statement::LetDestructure { names, value, mutable } => {
                self.check_expr(value, warnings);
                for name in names {
                    self.declare(name, *mutable);
                }
            }
            Statement::StructDecl { .. }
            | Statement::EnumDecl { .. }
            | Statement::TraitDecl { .. }
            | Statement::TypeAlias { .. }
            | Statement::Use { .. }
            | Statement::Module { .. }
            | Statement::Return(None)
            | Statement::Break(None)
            | Statement::Continue => {}
        }

        // Detect unreachable code: if a return/break appears, anything after
        // it in the same block is unreachable. (Checked at block level below.)
    }

    fn check_block(&mut self, block: &Block, warnings: &mut Vec<LintWarning>) {
        let mut saw_terminator = false;
        for (i, stmt) in block.statements.iter().enumerate() {
            if saw_terminator {
                warnings.push(LintWarning {
                    rule: "unreachable-code",
                    message: format!(
                        "Statement {} in block is unreachable after return/break",
                        i + 1
                    ),
                });
                break;
            }
            self.check_statement(stmt, warnings);
            if matches!(stmt, Statement::Return(_) | Statement::Break(_)) {
                saw_terminator = true;
            }
        }
        if let Some(expr) = &block.expr {
            if saw_terminator {
                warnings.push(LintWarning {
                    rule: "unreachable-code",
                    message: "Trailing expression is unreachable after return/break".to_string(),
                });
            }
            self.check_expr(expr, warnings);
        }
    }

    fn check_expr(&mut self, expr: &Expr, warnings: &mut Vec<LintWarning>) {
        match expr {
            Expr::Ident(name) => {
                self.mark_used(name);
            }
            Expr::BinaryOp { left, right, .. } => {
                self.check_expr(left, warnings);
                self.check_expr(right, warnings);
            }
            Expr::UnaryOp { operand, .. } => {
                self.check_expr(operand, warnings);
            }
            Expr::Call { callee, args } => {
                self.check_expr(callee, warnings);
                for arg in args {
                    self.check_expr(&arg.value, warnings);
                }
            }
            Expr::MethodCall { object, args, .. } => {
                self.check_expr(object, warnings);
                for arg in args {
                    self.check_expr(&arg.value, warnings);
                }
            }
            Expr::FieldAccess { object, .. } => {
                self.check_expr(object, warnings);
            }
            Expr::Index { object, index } => {
                self.check_expr(object, warnings);
                self.check_expr(index, warnings);
            }
            Expr::Lambda { params, body } => {
                self.push_scope();
                for p in params {
                    self.declare(&p.name, false);
                }
                self.check_expr(body, warnings);
                self.pop_scope(warnings);
            }
            Expr::If {
                condition,
                then_block,
                else_block,
            } => {
                // Detect constant conditions
                match condition.as_ref() {
                    Expr::BoolLit(true) => {
                        warnings.push(LintWarning {
                            rule: "constant-condition",
                            message: "Condition is always true".to_string(),
                        });
                    }
                    Expr::BoolLit(false) => {
                        warnings.push(LintWarning {
                            rule: "constant-condition",
                            message: "Condition is always false — block will never execute".to_string(),
                        });
                    }
                    _ => {}
                }
                self.check_expr(condition, warnings);
                self.push_scope();
                self.check_block(then_block, warnings);
                self.pop_scope(warnings);
                if let Some(eb) = else_block {
                    self.push_scope();
                    self.check_block(eb, warnings);
                    self.pop_scope(warnings);
                }
            }
            Expr::Match { subject, arms } => {
                self.check_expr(subject, warnings);
                for arm in arms {
                    self.push_scope();
                    self.bind_pattern_vars(&arm.pattern);
                    if let Some(guard) = &arm.guard {
                        self.check_expr(guard, warnings);
                    }
                    self.check_expr(&arm.body, warnings);
                    self.pop_scope(warnings);
                }
            }
            Expr::Block(block) => {
                self.push_scope();
                self.check_block(block, warnings);
                self.pop_scope(warnings);
            }
            Expr::List(items) => {
                for item in items {
                    self.check_expr(item, warnings);
                }
            }
            Expr::Map(entries) => {
                for (k, v) in entries {
                    self.check_expr(k, warnings);
                    self.check_expr(v, warnings);
                }
            }
            Expr::Tuple(items) => {
                for item in items {
                    self.check_expr(item, warnings);
                }
            }
            Expr::StructLit { fields, spread, .. } => {
                for (_, v) in fields {
                    self.check_expr(v, warnings);
                }
                if let Some(s) = spread {
                    self.check_expr(s, warnings);
                }
            }
            Expr::Range { start, end, .. } => {
                self.check_expr(start, warnings);
                self.check_expr(end, warnings);
            }
            Expr::Pipe { left, right } => {
                self.check_expr(left, warnings);
                self.check_expr(right, warnings);
            }
            Expr::NilCoalesce { left, right } => {
                self.check_expr(left, warnings);
                self.check_expr(right, warnings);
            }
            Expr::Try(inner) => {
                self.check_expr(inner, warnings);
            }
            Expr::For {
                binding,
                index_binding,
                iterable,
                body,
            } => {
                self.check_expr(iterable, warnings);
                self.push_scope();
                self.declare(binding, false);
                if let Some(idx) = index_binding {
                    self.declare(idx, false);
                }
                self.check_block(body, warnings);
                self.pop_scope(warnings);
            }
            Expr::While { condition, body } => {
                if matches!(condition.as_ref(), Expr::BoolLit(false)) {
                    warnings.push(LintWarning {
                        rule: "constant-condition",
                        message: "While condition is always false — loop will never execute".to_string(),
                    });
                }
                self.check_expr(condition, warnings);
                self.push_scope();
                self.check_block(body, warnings);
                self.pop_scope(warnings);
            }
            Expr::Loop { body } => {
                self.push_scope();
                self.check_block(body, warnings);
                self.pop_scope(warnings);
            }
            Expr::StringLit(s) => {
                // Track variable usage inside string interpolation
                for part in &s.parts {
                    if let crate::lexer::StringPart::Interpolation(tokens) = part {
                        for tok in tokens {
                            if let crate::lexer::Token::Ident(name) = tok {
                                self.mark_used(name);
                            }
                            if let crate::lexer::Token::SelfValue = tok {
                                self.mark_used("sf");
                            }
                        }
                    }
                }
            }
            Expr::TryCatch { try_block, error_binding, catch_block } => {
                self.push_scope();
                self.check_block(try_block, warnings);
                self.pop_scope(warnings);
                self.push_scope();
                self.declare(error_binding, false);
                self.check_block(catch_block, warnings);
                self.pop_scope(warnings);
            }
            // Literals — nothing to lint
            Expr::IntLit(_)
            | Expr::FloatLit(_)
            | Expr::BoolLit(_)
            | Expr::CharLit(_)
            | Expr::Nil => {}
        }
    }

    fn bind_pattern_vars(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Binding(name) => self.declare(name, false),
            Pattern::Variant(_, subs) => {
                for sub in subs {
                    self.bind_pattern_vars(sub);
                }
            }
            Pattern::Wildcard | Pattern::Literal(_) | Pattern::Range(..) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    fn lint_source(source: &str) -> Vec<LintWarning> {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        lint(&program)
    }

    fn has_rule(warnings: &[LintWarning], rule: &str) -> bool {
        warnings.iter().any(|w| w.rule == rule)
    }

    #[test]
    fn test_unused_variable() {
        let w = lint_source("lt x = 42");
        assert!(has_rule(&w, "unused-variable"));
    }

    #[test]
    fn test_used_variable() {
        let w = lint_source("lt x = 42\nprint(x)");
        assert!(!has_rule(&w, "unused-variable"));
    }

    #[test]
    fn test_unnecessary_mut() {
        let w = lint_source("mt x = 42\nprint(x)");
        assert!(has_rule(&w, "unnecessary-mut"));
    }

    #[test]
    fn test_mut_used_correctly() {
        let w = lint_source("mt x = 0\nx += 1\nprint(x)");
        assert!(!has_rule(&w, "unnecessary-mut"));
    }

    #[test]
    fn test_unreachable_code() {
        let w = lint_source(
            "fn foo() {\n  rt 1\n  lt x = 2\n}",
        );
        assert!(has_rule(&w, "unreachable-code"));
    }

    #[test]
    fn test_string_interpolation_counts_as_usage() {
        let w = lint_source(r#"lt name = "world"
"Hello, {name}!""#);
        assert!(!has_rule(&w, "unused-variable"),
            "Variable used in string interpolation should not be flagged as unused");
    }

    #[test]
    fn test_truly_unused_with_interpolation_nearby() {
        let w = lint_source(r#"lt used = "a"
lt unused = "b"
print("Hello, {used}!")"#);
        // `unused` is actually unused, `used` appears in interpolation
        let unused_warnings: Vec<_> = w.iter()
            .filter(|w| w.rule == "unused-variable")
            .collect();
        assert_eq!(unused_warnings.len(), 1);
        assert!(unused_warnings[0].message.contains("unused"));
    }

    #[test]
    fn test_shadowed_variable() {
        let w = lint_source("lt x = 1\nlt x = 2\nprint(x)");
        assert!(has_rule(&w, "shadowed-variable"));
    }

    #[test]
    fn test_no_shadow_across_scopes() {
        // Shadowing across different scopes (fn body vs outer) is fine
        let w = lint_source("lt x = 1\nfn foo() {\n  lt x = 2\n  print(x)\n}\nprint(x)");
        assert!(!has_rule(&w, "shadowed-variable"));
    }

    #[test]
    fn test_constant_condition_true() {
        let w = lint_source("if tt { print(1) }");
        assert!(has_rule(&w, "constant-condition"));
    }

    #[test]
    fn test_constant_condition_false_while() {
        let w = lint_source("wl ff { print(1) }");
        assert!(has_rule(&w, "constant-condition"));
    }

    #[test]
    fn test_no_false_positive_condition() {
        let w = lint_source("lt x = 5\nif x > 0 { print(x) }");
        assert!(!has_rule(&w, "constant-condition"));
    }
}
