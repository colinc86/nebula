use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;

use crate::ast::*;
use crate::lexer::{StringLiteral, StringPart};

pub struct Interpreter {
    scopes: Vec<Scope>,
    /// Current file path, used for resolving relative imports
    current_file: Option<PathBuf>,
    /// Already-loaded modules (prevents circular imports)
    loaded_modules: HashSet<PathBuf>,
    /// Memoization caches for @memoize functions
    memo_caches: HashMap<String, HashMap<String, Value>>,
    /// Functions marked as @deprecated
    deprecated_fns: HashMap<String, String>,
    /// Track which deprecated warnings have been shown
    deprecated_warned: HashSet<String>,
}

struct Scope {
    variables: HashMap<String, Variable>,
    functions: HashMap<String, FnDef>,
    structs: HashMap<String, StructDef>,
    enums: HashMap<String, EnumDef>,
}

#[derive(Clone)]
struct EnumDef {
    variants: Vec<EnumVariantDef>,
}

#[derive(Clone)]
struct EnumVariantDef {
    name: String,
    field_count: usize,
}

#[derive(Clone)]
struct Variable {
    value: Value,
    mutable: bool,
}

#[derive(Clone)]
struct FnDef {
    params: Vec<Param>,
    body: Block,
}

#[derive(Clone)]
struct StructDef {
    #[allow(dead_code)]
    fields: Vec<Field>,
    methods: HashMap<String, FnDef>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Char(char),
    Nil,
    List(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Tuple(Vec<Value>),
    Struct {
        name: String,
        fields: HashMap<String, Value>,
    },
    Function {
        name: String,
        params: Vec<Param>,
        body: Block,
        closure: Vec<(String, Value, bool)>,
    },
    Lambda {
        params: Vec<Param>,
        body: Box<Expr>,
        closure: Vec<(String, Value, bool)>,
    },
    EnumVariant {
        enum_name: String,
        variant: String,
        fields: Vec<Value>,
    },
    Return(Box<Value>),
    Break(Box<Value>),
    Continue,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(n) => {
                if *n == (*n as i64) as f64 {
                    write!(f, "{n:.1}")
                } else {
                    write!(f, "{n}")
                }
            }
            Value::Bool(b) => write!(f, "{b}"),
            Value::String(s) => write!(f, "{s}"),
            Value::Char(c) => write!(f, "{c}"),
            Value::Nil => write!(f, "nil"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Map(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, ")")
            }
            Value::Struct { name, fields } => {
                write!(f, "{name} {{ ")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, " }}")
            }
            Value::Function { name, .. } => write!(f, "<fn {name}>"),
            Value::Lambda { .. } => write!(f, "<lambda>"),
            Value::EnumVariant {
                enum_name,
                variant,
                fields,
            } => {
                write!(f, "{enum_name}.{variant}")?;
                if !fields.is_empty() {
                    write!(f, "(")?;
                    for (i, v) in fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{v}")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Value::Return(v) => write!(f, "{v}"),
            Value::Break(v) => write!(f, "{v}"),
            Value::Continue => write!(f, ""),
        }
    }
}

impl Value {
    fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Nil => false,
            Value::Int(0) => false,
            Value::String(s) if s.is_empty() => false,
            Value::List(l) if l.is_empty() => false,
            _ => true,
        }
    }

    pub fn type_name(&self) -> &str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Bool(_) => "Bool",
            Value::String(_) => "String",
            Value::Char(_) => "Char",
            Value::Nil => "Nil",
            Value::List(_) => "List",
            Value::Map(_) => "Map",
            Value::Tuple(_) => "Tuple",
            Value::Struct { name, .. } => name,
            Value::Function { .. } | Value::Lambda { .. } => "Function",
            Value::EnumVariant { enum_name, .. } => enum_name,
            Value::Return(_) => "Return",
            Value::Break(_) => "Break",
            Value::Continue => "Continue",
        }
    }
}

/// Free-standing index resolver (avoids borrow conflicts in assignment)
fn resolve_index_static(idx: i64, len: usize) -> usize {
    if idx < 0 {
        let pos = len as i64 + idx;
        if pos < 0 { 0 } else { pos as usize }
    } else {
        idx as usize
    }
}

/// Free-standing equality check (avoids borrow conflicts in assignment)
fn values_equal_static(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Char(a), Value::Char(b)) => a == b,
        (Value::Nil, Value::Nil) => true,
        (Value::List(a), Value::List(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal_static(x, y))
        }
        (Value::Tuple(a), Value::Tuple(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal_static(x, y))
        }
        _ => false,
    }
}

#[derive(Debug)]
pub struct RuntimeError {
    pub message: String,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Runtime error: {}", self.message)
    }
}

impl Scope {
    fn new() -> Self {
        Scope {
            variables: HashMap::new(),
            functions: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
        }
    }
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Interpreter {
            scopes: vec![Scope::new()],
            current_file: None,
            loaded_modules: HashSet::new(),
            memo_caches: HashMap::new(),
            deprecated_fns: HashMap::new(),
            deprecated_warned: HashSet::new(),
        };
        interp.register_builtins();
        interp
    }

    /// Set the file path for module resolution
    pub fn set_file(&mut self, path: &str) {
        let p = PathBuf::from(path);
        if let Ok(canonical) = p.canonicalize() {
            self.loaded_modules.insert(canonical.clone());
            self.current_file = Some(canonical);
        } else {
            self.current_file = Some(p);
        }
    }

    fn exec_use(
        &mut self,
        path: &[String],
        items: &Option<Vec<String>>,
        _alias: &Option<String>,
    ) -> Result<Value, RuntimeError> {
        // Resolve file path: use math.utils → math/utils.neb
        let relative = format!("{}.neb", path.join("/"));

        let base_dir = self
            .current_file
            .as_ref()
            .and_then(|f| f.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));

        let module_path = base_dir.join(&relative);

        // Check for circular imports
        if let Ok(canonical) = module_path.canonicalize() {
            if self.loaded_modules.contains(&canonical) {
                return Ok(Value::Nil); // Already loaded, skip
            }
            self.loaded_modules.insert(canonical);
        }

        // Read the file
        let source = std::fs::read_to_string(&module_path).map_err(|e| RuntimeError {
            message: format!("Cannot load module '{}': {e}", path.join(".")),
        })?;

        // Lex and parse
        let tokens = crate::lexer::tokenize(&source).map_err(|e| RuntimeError {
            message: format!("Error in module '{}': {e}", path.join(".")),
        })?;
        let program = crate::parser::parse(tokens).map_err(|e| RuntimeError {
            message: format!("Error in module '{}': {e}", path.join(".")),
        })?;

        // Execute the module in a fresh scope
        let saved_file = self.current_file.clone();
        if let Ok(canonical) = module_path.canonicalize() {
            self.current_file = Some(canonical);
        }

        self.scopes.push(Scope::new());
        for stmt in &program.statements {
            let val = self.exec_statement(stmt)?;
            if matches!(val, Value::Return(_)) {
                break;
            }
        }

        // Extract items from the module scope
        let module_scope = self.scopes.pop().unwrap();
        self.current_file = saved_file;

        // Import items into the current scope
        let current = self.scopes.last_mut().unwrap();

        if let Some(specific_items) = items {
            // Import specific names: use math.utils.{distance, Point}
            for item_name in specific_items {
                if let Some(var) = module_scope.variables.get(item_name) {
                    current.variables.insert(item_name.clone(), var.clone());
                }
                if let Some(func) = module_scope.functions.get(item_name) {
                    current.functions.insert(item_name.clone(), func.clone());
                }
                if let Some(s) = module_scope.structs.get(item_name) {
                    current.structs.insert(item_name.clone(), s.clone());
                }
                if let Some(e) = module_scope.enums.get(item_name) {
                    current.enums.insert(item_name.clone(), e.clone());
                }
            }
        } else {
            // Import everything (for now — later respect `pub`)
            for (name, var) in &module_scope.variables {
                current.variables.insert(name.clone(), var.clone());
            }
            for (name, func) in &module_scope.functions {
                current.functions.insert(name.clone(), func.clone());
            }
            for (name, s) in &module_scope.structs {
                current.structs.insert(name.clone(), s.clone());
            }
            for (name, e) in &module_scope.enums {
                current.enums.insert(name.clone(), e.clone());
            }
        }

        Ok(Value::Nil)
    }

    fn register_builtins(&mut self) {
        // Built-in functions are handled specially in call_function
    }

    pub fn execute(&mut self, program: &Program) -> Result<(), RuntimeError> {
        for stmt in &program.statements {
            let val = self.exec_statement(stmt)?;
            if matches!(val, Value::Return(_)) {
                break;
            }
        }
        Ok(())
    }

    /// Execute for REPL: returns the value of the last expression statement (if any).
    pub fn execute_interactive(
        &mut self,
        program: &Program,
    ) -> Result<Option<Value>, RuntimeError> {
        let len = program.statements.len();
        let mut last_val = None;
        for (i, stmt) in program.statements.iter().enumerate() {
            let val = self.exec_statement(stmt)?;
            if matches!(val, Value::Return(_)) {
                break;
            }
            // Only show the value of the last statement if it's a bare expression
            if i == len - 1 {
                if let Statement::Expr(_) = stmt {
                    if !matches!(val, Value::Nil) {
                        last_val = Some(val);
                    }
                }
            }
        }
        Ok(last_val)
    }

    fn exec_statement(&mut self, stmt: &Statement) -> Result<Value, RuntimeError> {
        match stmt {
            Statement::Let { name, value, .. } => {
                let val = self.eval_expr(value)?;
                if matches!(val, Value::Return(_)) {
                    return Ok(val); // propagate ? early return
                }
                self.define_var(name, val, false);
                Ok(Value::Nil)
            }
            Statement::Mut { name, value, .. } => {
                let val = self.eval_expr(value)?;
                if matches!(val, Value::Return(_)) {
                    return Ok(val);
                }
                self.define_var(name, val, true);
                Ok(Value::Nil)
            }
            Statement::Const { name, value, .. } => {
                let val = self.eval_expr(value)?;
                if matches!(val, Value::Return(_)) {
                    return Ok(val);
                }
                self.define_var(name, val, false);
                Ok(Value::Nil)
            }
            Statement::FnDecl {
                name, params, body, decorators, ..
            } => {
                // Process decorators
                for (dname, dargs) in decorators {
                    match dname.as_str() {
                        "memoize" => {
                            self.memo_caches.insert(name.clone(), HashMap::new());
                        }
                        "deprecated" => {
                            let msg = if let Some(arg) = dargs.first() {
                                if let Ok(Value::String(s)) = self.eval_expr(arg) { s }
                                else { format!("Function '{}' is deprecated", name) }
                            } else {
                                format!("Function '{}' is deprecated", name)
                            };
                            self.deprecated_fns.insert(name.clone(), msg);
                        }
                        _ => {} // unknown decorators are silently ignored
                    }
                }

                let scope = self.scopes.last_mut().unwrap();
                scope.functions.insert(
                    name.clone(),
                    FnDef {
                        params: params.clone(),
                        body: body.clone(),
                    },
                );
                Ok(Value::Nil)
            }
            Statement::StructDecl {
                name, fields, ..
            } => {
                let scope = self.scopes.last_mut().unwrap();
                scope.structs.insert(
                    name.clone(),
                    StructDef {
                        fields: fields.clone(),
                        methods: HashMap::new(),
                    },
                );
                Ok(Value::Nil)
            }
            Statement::ImplBlock {
                target, methods, ..
            } => {
                let mut method_defs = HashMap::new();
                for method in methods {
                    if let Statement::FnDecl {
                        name, params, body, ..
                    } = method
                    {
                        method_defs.insert(
                            name.clone(),
                            FnDef {
                                params: params.clone(),
                                body: body.clone(),
                            },
                        );
                    }
                }
                // Find the struct and add methods
                for scope in self.scopes.iter_mut().rev() {
                    if let Some(sdef) = scope.structs.get_mut(target) {
                        sdef.methods.extend(method_defs);
                        return Ok(Value::Nil);
                    }
                }
                Err(RuntimeError {
                    message: format!("Struct '{target}' not found for impl block"),
                })
            }
            Statement::Expr(expr) => self.eval_expr(expr),
            Statement::Return(expr) => {
                let val = match expr {
                    Some(e) => self.eval_expr(e)?,
                    None => Value::Nil,
                };
                Ok(Value::Return(Box::new(val)))
            }
            Statement::Break(expr) => {
                let val = match expr {
                    Some(e) => self.eval_expr(e)?,
                    None => Value::Nil,
                };
                Ok(Value::Break(Box::new(val)))
            }
            Statement::Continue => Ok(Value::Continue),
            Statement::Throw(expr) => {
                let val = match expr {
                    Some(e) => self.eval_expr(e)?,
                    None => Value::Nil,
                };
                Err(RuntimeError { message: format!("Uncaught exception: {:?}", val) })
            }
            Statement::Assignment { target, op, value } => {
                let new_val = self.eval_expr(value)?;
                match target {
                    Expr::Ident(name) => {
                        let current = if matches!(op, AssignOp::Assign) {
                            new_val
                        } else {
                            let old = self.get_var(name)?;
                            self.apply_assign_op(op, old, new_val)?
                        };
                        self.set_var(name, current)?;
                    }
                    Expr::FieldAccess { object, field } => {
                        if let Expr::Ident(name) = object.as_ref() {
                            let val = self.eval_expr(value)?;
                            // Get the struct, modify the field
                            for scope in self.scopes.iter_mut().rev() {
                                if let Some(var) = scope.variables.get_mut(name) {
                                    if !var.mutable {
                                        return Err(RuntimeError {
                                            message: format!("Cannot assign to immutable variable '{name}'"),
                                        });
                                    }
                                    if let Value::Struct { fields, .. } = &mut var.value {
                                        fields.insert(field.clone(), val);
                                        return Ok(Value::Nil);
                                    }
                                }
                            }
                            return Err(RuntimeError {
                                message: format!("Cannot assign field '{field}' on '{name}'"),
                            });
                        }
                        return Err(RuntimeError {
                            message: "Complex field assignment not yet supported".to_string(),
                        });
                    }
                    Expr::Index { object, index } => {
                        if let Expr::Ident(name) = object.as_ref() {
                            let idx = self.eval_expr(index)?;
                            let val = if matches!(op, AssignOp::Assign) {
                                new_val
                            } else {
                                let obj = self.get_var(name)?;
                                let old = match (&obj, &idx) {
                                    (Value::List(items), Value::Int(i)) => {
                                        let i = resolve_index_static(*i, items.len());
                                        items.get(i).cloned().unwrap_or(Value::Nil)
                                    }
                                    (Value::Map(entries), key) => {
                                        entries.iter()
                                            .find(|(k, _)| values_equal_static(k, key))
                                            .map(|(_, v)| v.clone())
                                            .unwrap_or(Value::Nil)
                                    }
                                    _ => Value::Nil,
                                };
                                self.apply_assign_op(op, old, new_val)?
                            };

                            for scope in self.scopes.iter_mut().rev() {
                                if let Some(var) = scope.variables.get_mut(name) {
                                    if !var.mutable {
                                        return Err(RuntimeError {
                                            message: format!("Cannot assign to immutable variable '{name}'"),
                                        });
                                    }
                                    match &mut var.value {
                                        Value::List(items) => {
                                            if let Value::Int(i) = &idx {
                                                let i = resolve_index_static(*i, items.len());
                                                if i < items.len() {
                                                    items[i] = val;
                                                } else {
                                                    return Err(RuntimeError {
                                                        message: format!("Index {i} out of bounds (len {})", items.len()),
                                                    });
                                                }
                                            }
                                            return Ok(Value::Nil);
                                        }
                                        Value::Map(entries) => {
                                            for (k, v) in entries.iter_mut() {
                                                if values_equal_static(k, &idx) {
                                                    *v = val;
                                                    return Ok(Value::Nil);
                                                }
                                            }
                                            entries.push((idx, val));
                                            return Ok(Value::Nil);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            return Err(RuntimeError {
                                message: format!("Cannot index-assign on '{name}'"),
                            });
                        }
                        return Err(RuntimeError {
                            message: "Complex index assignment not yet supported".to_string(),
                        });
                    }
                    _ => {
                        return Err(RuntimeError {
                            message: "Invalid assignment target".to_string(),
                        });
                    }
                }
                Ok(Value::Nil)
            }
            Statement::EnumDecl {
                name, variants, ..
            } => {
                let def = EnumDef {
                    variants: variants
                        .iter()
                        .map(|v| EnumVariantDef {
                            name: v.name.clone(),
                            field_count: v.fields.len(),
                        })
                        .collect(),
                };
                let scope = self.scopes.last_mut().unwrap();
                scope.enums.insert(name.clone(), def);
                Ok(Value::Nil)
            }
            Statement::LetDestructure {
                names, value, mutable,
            } => {
                let val = self.eval_expr(value)?;
                if matches!(val, Value::Return(_)) {
                    return Ok(val);
                }
                match val {
                    Value::Tuple(items) => {
                        if items.len() != names.len() {
                            return Err(RuntimeError {
                                message: format!(
                                    "Destructuring mismatch: {} names but tuple has {} elements",
                                    names.len(), items.len()
                                ),
                            });
                        }
                        for (name, item) in names.iter().zip(items) {
                            self.define_var(name, item, *mutable);
                        }
                    }
                    Value::List(items) => {
                        if items.len() != names.len() {
                            return Err(RuntimeError {
                                message: format!(
                                    "Destructuring mismatch: {} names but list has {} elements",
                                    names.len(), items.len()
                                ),
                            });
                        }
                        for (name, item) in names.iter().zip(items) {
                            self.define_var(name, item, *mutable);
                        }
                    }
                    _ => {
                        return Err(RuntimeError {
                            message: format!(
                                "Cannot destructure {} — expected Tuple or List",
                                val.type_name()
                            ),
                        });
                    }
                }
                Ok(Value::Nil)
            }
            Statement::TraitDecl { .. }
            | Statement::Module { .. }
            | Statement::TypeAlias { .. } => Ok(Value::Nil),
            Statement::Use { path, items, alias } => {
                self.exec_use(path, items, alias)
            }
        }
    }

    fn apply_assign_op(
        &self,
        op: &AssignOp,
        left: Value,
        right: Value,
    ) -> Result<Value, RuntimeError> {
        match op {
            AssignOp::Assign => Ok(right),
            AssignOp::AddAssign => self.eval_binary_op(&BinOp::Add, left, right),
            AssignOp::SubAssign => self.eval_binary_op(&BinOp::Sub, left, right),
            AssignOp::MulAssign => self.eval_binary_op(&BinOp::Mul, left, right),
            AssignOp::DivAssign => self.eval_binary_op(&BinOp::Div, left, right),
            AssignOp::ModAssign => self.eval_binary_op(&BinOp::Mod, left, right),
        }
    }

    fn eval_expr(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match expr {
            Expr::IntLit(n) => Ok(Value::Int(*n)),
            Expr::FloatLit(f) => Ok(Value::Float(*f)),
            Expr::BoolLit(b) => Ok(Value::Bool(*b)),
            Expr::StringLit(s) => self.eval_string_literal(s),
            Expr::CharLit(c) => Ok(Value::Char(*c)),
            Expr::Nil => Ok(Value::Nil),

            Expr::Ident(name) => self.get_var(name),

            Expr::BinaryOp { left, op, right } => {
                // Short-circuit for and/or
                if matches!(op, BinOp::And) {
                    let l = self.eval_expr(left)?;
                    if !l.is_truthy() {
                        return Ok(Value::Bool(false));
                    }
                    let r = self.eval_expr(right)?;
                    return Ok(Value::Bool(r.is_truthy()));
                }
                if matches!(op, BinOp::Or) {
                    let l = self.eval_expr(left)?;
                    if l.is_truthy() {
                        return Ok(l);
                    }
                    return self.eval_expr(right);
                }
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                self.eval_binary_op(op, l, r)
            }

            Expr::UnaryOp { op, operand } => {
                let val = self.eval_expr(operand)?;
                match op {
                    UnaryOp::Neg => match val {
                        Value::Int(n) => Ok(Value::Int(-n)),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        _ => Err(RuntimeError {
                            message: format!("Cannot negate {}", val.type_name()),
                        }),
                    },
                    UnaryOp::Not => Ok(Value::Bool(!val.is_truthy())),
                    UnaryOp::BitNot => match val {
                        Value::Int(n) => Ok(Value::Int(!n)),
                        _ => Err(RuntimeError {
                            message: format!("Cannot bitwise-not {}", val.type_name()),
                        }),
                    },
                }
            }

            Expr::Call { callee, args } => {
                let arg_values = self.eval_call_args(args)?;
                match callee.as_ref() {
                    Expr::Ident(name) => self.call_function(name, arg_values),
                    Expr::FieldAccess { object, field } => {
                        if let Expr::Ident(type_name) = object.as_ref() {
                            // Check if this is an enum variant constructor
                            if let Some(edef) = self.find_enum(type_name) {
                                if let Some(vdef) = edef.variants.iter().find(|v| v.name == *field) {
                                    if vdef.field_count > 0 {
                                        let fields: Vec<Value> = arg_values.into_iter().map(|(_, v)| v).collect();
                                        return Ok(Value::EnumVariant {
                                            enum_name: type_name.clone(),
                                            variant: field.clone(),
                                            fields,
                                        });
                                    }
                                }
                            }
                            // Otherwise static method call
                            self.call_static_method(type_name, field, arg_values)
                        } else {
                            Err(RuntimeError {
                                message: "Complex callee not supported".to_string(),
                            })
                        }
                    }
                    _ => {
                        // Try evaluating as a lambda
                        let callee_val = self.eval_expr(callee)?;
                        self.call_value(callee_val, arg_values)
                    }
                }
            }

            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                // Check if this is an enum variant constructor or static method
                if let Expr::Ident(type_name) = object.as_ref() {
                    if type_name.chars().next().map_or(false, |c| c.is_uppercase()) {
                        // Check enum variant constructor first
                        if let Some(edef) = self.find_enum(type_name) {
                            if edef.variants.iter().any(|v| v.name == *method) {
                                let arg_values = self.eval_call_args(args)?;
                                let fields: Vec<Value> = arg_values.into_iter().map(|(_, v)| v).collect();
                                return Ok(Value::EnumVariant {
                                    enum_name: type_name.clone(),
                                    variant: method.clone(),
                                    fields,
                                });
                            }
                        }
                        if self.get_var(type_name).is_err() {
                            let arg_values = self.eval_call_args(args)?;
                            return self.call_static_method(type_name, method, arg_values);
                        }
                    }
                }
                let obj = self.eval_expr(object)?;
                let arg_values = self.eval_call_args(args)?;
                self.call_method(obj, method, arg_values)
            }

            Expr::FieldAccess { object, field } => {
                // Check if this is an enum variant access: EnumName.Variant
                if let Expr::Ident(name) = object.as_ref() {
                    if let Some(edef) = self.find_enum(name) {
                        if let Some(vdef) = edef.variants.iter().find(|v| v.name == *field) {
                            if vdef.field_count == 0 {
                                return Ok(Value::EnumVariant {
                                    enum_name: name.clone(),
                                    variant: field.clone(),
                                    fields: Vec::new(),
                                });
                            }
                            // Variant with fields will be handled by MethodCall / Call
                        }
                    }
                }

                let obj = self.eval_expr(object)?;
                match obj {
                    Value::Struct { fields, .. } => fields.get(field).cloned().ok_or(RuntimeError {
                        message: format!("No field '{field}' on struct"),
                    }),
                    Value::Tuple(items) => {
                        let idx: usize = field.parse().map_err(|_| RuntimeError {
                            message: format!("Invalid tuple index: {field}"),
                        })?;
                        items.get(idx).cloned().ok_or(RuntimeError {
                            message: format!("Tuple index {idx} out of bounds"),
                        })
                    }
                    _ => Err(RuntimeError {
                        message: format!("Cannot access field on {}", obj.type_name()),
                    }),
                }
            }

            Expr::Index { object, index } => {
                // Check for range-based slicing before evaluating index
                if let Expr::Range { start, end, inclusive } = index.as_ref() {
                    let obj = self.eval_expr(object)?;
                    let s = self.eval_expr(start)?;
                    let e = self.eval_expr(end)?;
                    if let (Value::Int(si), Value::Int(ei)) = (&s, &e) {
                        let end_idx = if *inclusive { *ei + 1 } else { *ei };
                        return match obj {
                            Value::List(items) => {
                                let si = self.resolve_index(*si, items.len());
                                let ei = self.resolve_index(end_idx, items.len()).min(items.len());
                                Ok(Value::List(items[si..ei].to_vec()))
                            }
                            Value::String(ref sv) => {
                                let chars: Vec<char> = sv.chars().collect();
                                let si = self.resolve_index(*si, chars.len());
                                let ei = self.resolve_index(end_idx, chars.len()).min(chars.len());
                                Ok(Value::String(chars[si..ei].iter().collect()))
                            }
                            _ => Err(RuntimeError {
                                message: format!("Cannot slice {}", obj.type_name()),
                            }),
                        };
                    }
                }

                let obj = self.eval_expr(object)?;
                let idx = self.eval_expr(index)?;
                match (&obj, &idx) {
                    (Value::List(items), Value::Int(i)) => {
                        let i = self.resolve_index(*i, items.len());
                        items.get(i).cloned().ok_or(RuntimeError {
                            message: format!("Index {i} out of bounds (len {})", items.len()),
                        })
                    }
                    (Value::String(s), Value::Int(i)) => {
                        let i = self.resolve_index(*i, s.chars().count());
                        s.chars().nth(i).map(Value::Char).ok_or(RuntimeError {
                            message: format!("String index {i} out of bounds"),
                        })
                    }
                    (Value::Map(entries), key) => {
                        for (k, v) in entries {
                            if self.values_equal(k, key) {
                                return Ok(v.clone());
                            }
                        }
                        Ok(Value::Nil)
                    }
                    _ => Err(RuntimeError {
                        message: format!(
                            "Cannot index {} with {}",
                            obj.type_name(),
                            idx.type_name()
                        ),
                    }),
                }
            }

            Expr::Slice { object, start, end } => {
                let obj = self.eval_expr(object)?;
                // Resolve start/end, defaulting to [0, len].
                let len = match &obj {
                    Value::List(items) => items.len(),
                    Value::String(s) => s.chars().count(),
                    _ => return Err(RuntimeError {
                        message: format!("Cannot slice {}", obj.type_name()),
                    }),
                };
                let s_idx = match start.as_ref() {
                    Some(e) => match self.eval_expr(e)? {
                        Value::Int(i) => self.resolve_index(i, len),
                        v => return Err(RuntimeError {
                            message: format!("Slice start must be int, got {}", v.type_name()),
                        }),
                    },
                    None => 0,
                };
                let e_idx = match end.as_ref() {
                    Some(e) => match self.eval_expr(e)? {
                        Value::Int(i) => self.resolve_index(i, len).min(len),
                        v => return Err(RuntimeError {
                            message: format!("Slice end must be int, got {}", v.type_name()),
                        }),
                    },
                    None => len,
                };
                let s_idx = s_idx.min(len);
                let e_idx = e_idx.max(s_idx);
                match obj {
                    Value::List(items) => Ok(Value::List(items[s_idx..e_idx].to_vec())),
                    Value::String(s) => {
                        let chars: Vec<char> = s.chars().collect();
                        Ok(Value::String(chars[s_idx..e_idx].iter().collect()))
                    }
                    _ => unreachable!(),
                }
            }

            Expr::Lambda { params, body } => {
                // Capture current scope variables for closure
                let closure = self.capture_closure();
                Ok(Value::Lambda {
                    params: params.clone(),
                    body: Box::new(*body.clone()),
                    closure,
                })
            }

            Expr::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond = self.eval_expr(condition)?;
                if cond.is_truthy() {
                    self.eval_block(then_block)
                } else if let Some(else_b) = else_block {
                    self.eval_block(else_b)
                } else {
                    Ok(Value::Nil)
                }
            }

            Expr::Match { subject, arms } => {
                let val = self.eval_expr(subject)?;
                for arm in arms {
                    if self.pattern_matches(&arm.pattern, &val)? {
                        // Bind pattern variables
                        self.scopes.push(Scope::new());
                        self.bind_pattern(&arm.pattern, &val);
                        if let Some(guard) = &arm.guard {
                            let g = self.eval_expr(guard)?;
                            if !g.is_truthy() {
                                self.scopes.pop();
                                continue;
                            }
                        }
                        let result = self.eval_expr(&arm.body)?;
                        self.scopes.pop();
                        return Ok(result);
                    }
                }
                Err(RuntimeError {
                    message: "No matching arm in match expression".to_string(),
                })
            }

            Expr::Block(block) => self.eval_block(block),

            Expr::List(items) => {
                let values: Result<Vec<_>, _> = items.iter().map(|e| self.eval_expr(e)).collect();
                Ok(Value::List(values?))
            }

            Expr::Map(entries) => {
                let mut pairs = Vec::new();
                for (k, v) in entries {
                    pairs.push((self.eval_expr(k)?, self.eval_expr(v)?));
                }
                Ok(Value::Map(pairs))
            }

            Expr::Tuple(items) => {
                let values: Result<Vec<_>, _> = items.iter().map(|e| self.eval_expr(e)).collect();
                Ok(Value::Tuple(values?))
            }

            Expr::StructLit {
                name,
                fields,
                spread,
            } => {
                let mut field_map = HashMap::new();

                // Apply spread first
                if let Some(spread_expr) = spread {
                    if let Value::Struct {
                        fields: spread_fields,
                        ..
                    } = self.eval_expr(spread_expr)?
                    {
                        field_map = spread_fields;
                    }
                }

                // Then explicit fields (override spread)
                for (fname, fexpr) in fields {
                    let val = self.eval_expr(fexpr)?;
                    field_map.insert(fname.clone(), val);
                }

                Ok(Value::Struct {
                    name: name.clone(),
                    fields: field_map,
                })
            }

            Expr::Range {
                start,
                end,
                inclusive,
            } => {
                let s = self.eval_expr(start)?;
                let e = self.eval_expr(end)?;
                match (&s, &e) {
                    (Value::Int(a), Value::Int(b)) => {
                        let end = if *inclusive { *b + 1 } else { *b };
                        let items: Vec<Value> = (*a..end).map(Value::Int).collect();
                        Ok(Value::List(items))
                    }
                    _ => Err(RuntimeError {
                        message: "Range only supports Int values".to_string(),
                    }),
                }
            }

            Expr::Pipe { left, right } => {
                let val = self.eval_expr(left)?;
                // The right side should be a function call; inject val as first arg
                match right.as_ref() {
                    Expr::Call { callee, args } => {
                        let mut all_args = vec![("", val)];
                        for arg in args {
                            let v = self.eval_expr(&arg.value)?;
                            all_args.push(("", v));
                        }
                        let arg_values: Vec<(Option<String>, Value)> = all_args
                            .into_iter()
                            .map(|(_, v)| (None, v))
                            .collect();
                        match callee.as_ref() {
                            Expr::Ident(name) => self.call_function(name, arg_values),
                            _ => Err(RuntimeError {
                                message: "Pipe target must be a function call".to_string(),
                            }),
                        }
                    }
                    Expr::Ident(name) => {
                        self.call_function(name, vec![(None, val)])
                    }
                    _ => Err(RuntimeError {
                        message: "Pipe target must be a function or function call".to_string(),
                    }),
                }
            }

            Expr::NilCoalesce { left, right } => {
                let val = self.eval_expr(left)?;
                if matches!(val, Value::Nil) {
                    self.eval_expr(right)
                } else {
                    Ok(val)
                }
            }

            Expr::For {
                binding,
                index_binding,
                iterable,
                body,
            } => {
                let iter_val = self.eval_expr(iterable)?;

                // Map iteration: fo k, v in map OR fo entry in map
                if let Value::Map(entries) = iter_val {
                    for (i, (k, v)) in entries.into_iter().enumerate() {
                        self.scopes.push(Scope::new());
                        if let Some(idx_name) = index_binding {
                            // fo idx, val in map — idx=key, val=value
                            self.define_var(idx_name, k, false);
                            self.define_var(binding, v, false);
                        } else {
                            // fo entry in map — entry is a (key, value) tuple
                            self.define_var(binding, Value::Tuple(vec![k, v]), false);
                        }
                        let _ = i; // suppress unused
                        let result = self.eval_block(body)?;
                        self.scopes.pop();
                        match result {
                            Value::Break(bv) => return Ok(*bv),
                            Value::Return(_) => return Ok(result),
                            Value::Continue => continue,
                            _ => {}
                        }
                    }
                    return Ok(Value::Nil);
                }

                let items = match iter_val {
                    Value::List(items) => items,
                    Value::String(s) => s.chars().map(Value::Char).collect(),
                    _ => {
                        return Err(RuntimeError {
                            message: format!("Cannot iterate over {}", iter_val.type_name()),
                        });
                    }
                };

                for (i, item) in items.into_iter().enumerate() {
                    self.scopes.push(Scope::new());
                    self.define_var(binding, item, false);
                    if let Some(idx_name) = index_binding {
                        self.define_var(idx_name, Value::Int(i as i64), false);
                    }
                    let result = self.eval_block(body)?;
                    self.scopes.pop();
                    match result {
                        Value::Break(v) => return Ok(*v),
                        Value::Return(_) => return Ok(result),
                        Value::Continue => continue,
                        _ => {}
                    }
                }
                Ok(Value::Nil)
            }

            Expr::While { condition, body } => {
                loop {
                    let cond = self.eval_expr(condition)?;
                    if !cond.is_truthy() {
                        break;
                    }
                    self.scopes.push(Scope::new());
                    let result = self.eval_block(body)?;
                    self.scopes.pop();
                    match result {
                        Value::Break(v) => return Ok(*v),
                        Value::Return(_) => return Ok(result),
                        Value::Continue => continue,
                        _ => {}
                    }
                }
                Ok(Value::Nil)
            }

            Expr::Loop { body } => loop {
                self.scopes.push(Scope::new());
                let result = self.eval_block(body)?;
                self.scopes.pop();
                match result {
                    Value::Break(v) => return Ok(*v),
                    Value::Return(_) => return Ok(result),
                    Value::Continue => continue,
                    _ => {}
                }
            },

            Expr::Try(inner) => {
                let val = self.eval_expr(inner)?;
                if matches!(val, Value::Nil) {
                    Ok(Value::Return(Box::new(Value::Nil)))
                } else {
                    Ok(val)
                }
            },

            Expr::TryCatch {
                try_block,
                error_binding,
                catch_block,
                finally_block,
            } => {
                let primary = match self.eval_block(try_block) {
                    Ok(val) => Ok(val),
                    Err(e) => {
                        let has_catch = !catch_block.statements.is_empty()
                            || catch_block.expr.is_some()
                            || error_binding != "_";
                        if has_catch {
                            self.scopes.push(Scope::new());
                            self.define_var(error_binding, Value::String(e.message.clone()), false);
                            let result = self.eval_block(catch_block);
                            self.scopes.pop();
                            result
                        } else {
                            Err(e)
                        }
                    }
                };
                // `finally` runs regardless, before the overall result propagates.
                // If the finally block itself errors, that error supersedes.
                if let Some(fb) = finally_block {
                    match self.eval_block(fb) {
                        Ok(_) => primary,
                        Err(fe) => Err(fe),
                    }
                } else {
                    primary
                }
            },
        }
    }

    fn eval_string_literal(&mut self, s: &StringLiteral) -> Result<Value, RuntimeError> {
        let mut result = String::new();
        for part in &s.parts {
            match part {
                StringPart::Literal(text) => result.push_str(text),
                StringPart::Interpolation(tokens) => {
                    let val = self.eval_interpolation_tokens(tokens)?;
                    result.push_str(&val.to_string());
                }
            }
        }
        Ok(Value::String(result))
    }

    fn eval_interpolation_tokens(
        &mut self,
        tokens: &[crate::lexer::Token],
    ) -> Result<Value, RuntimeError> {
        if tokens.is_empty() {
            return Ok(Value::String(String::new()));
        }

        // Convert raw tokens to SpannedTokens and parse as an expression
        let mut spanned: Vec<crate::lexer::SpannedToken> = tokens
            .iter()
            .map(|t| crate::lexer::SpannedToken {
                token: t.clone(),
                line: 0,
                col: 0,
            })
            .collect();
        spanned.push(crate::lexer::SpannedToken {
            token: crate::lexer::Token::Eof,
            line: 0,
            col: 0,
        });

        match crate::parser::parse(spanned) {
            Ok(program) => {
                if let Some(crate::ast::Statement::Expr(expr)) = program.statements.first() {
                    self.eval_expr(expr)
                } else {
                    Ok(Value::String(String::new()))
                }
            }
            Err(_) => {
                // Fallback for unparseable interpolation
                Ok(Value::String("<interpolation>".to_string()))
            }
        }
    }

    fn eval_block(&mut self, block: &Block) -> Result<Value, RuntimeError> {
        self.scopes.push(Scope::new());
        let mut last = Value::Nil;

        for stmt in &block.statements {
            last = self.exec_statement(stmt)?;
            if matches!(last, Value::Return(_) | Value::Break(_) | Value::Continue) {
                self.scopes.pop();
                return Ok(last);
            }
        }

        let result = if let Some(expr) = &block.expr {
            self.eval_expr(expr)?
        } else {
            last
        };

        self.scopes.pop();
        Ok(result)
    }

    fn eval_binary_op(&self, op: &BinOp, left: Value, right: Value) -> Result<Value, RuntimeError> {
        match op {
            BinOp::Add => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
                (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
                (Value::List(a), Value::List(b)) => {
                    let mut r = a.clone();
                    r.extend(b.clone());
                    Ok(Value::List(r))
                }
                _ => Err(self.type_error("+", &left, &right)),
            },
            BinOp::Sub => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - *b as f64)),
                _ => Err(self.type_error("-", &left, &right)),
            },
            BinOp::Mul => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * *b as f64)),
                (Value::String(a), Value::Int(b)) => Ok(Value::String(a.repeat(*b as usize))),
                _ => Err(self.type_error("*", &left, &right)),
            },
            BinOp::Div => match (&left, &right) {
                (_, Value::Int(0)) => Err(RuntimeError {
                    message: "Division by zero".to_string(),
                }),
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 / b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / *b as f64)),
                _ => Err(self.type_error("/", &left, &right)),
            },
            BinOp::Mod => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 % b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a % *b as f64)),
                _ => Err(self.type_error("%", &left, &right)),
            },
            BinOp::Pow => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) if *b >= 0 => {
                    Ok(Value::Int((*a as f64).powi(*b as i32) as i64))
                }
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(*b))),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).powf(*b))),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.powi(*b as i32))),
                _ => Err(self.type_error("**", &left, &right)),
            },

            BinOp::Eq => Ok(Value::Bool(self.values_equal(&left, &right))),
            BinOp::NotEq => Ok(Value::Bool(!self.values_equal(&left, &right))),
            BinOp::Less => self.compare_values(&left, &right, |o| o == std::cmp::Ordering::Less),
            BinOp::Greater => {
                self.compare_values(&left, &right, |o| o == std::cmp::Ordering::Greater)
            }
            BinOp::LessEq => {
                self.compare_values(&left, &right, |o| o != std::cmp::Ordering::Greater)
            }
            BinOp::GreaterEq => {
                self.compare_values(&left, &right, |o| o != std::cmp::Ordering::Less)
            }

            BinOp::And => Ok(Value::Bool(left.is_truthy() && right.is_truthy())),
            BinOp::Or => {
                if left.is_truthy() {
                    Ok(left)
                } else {
                    Ok(right)
                }
            }

            BinOp::BitAnd => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a & b)),
                _ => Err(self.type_error("&", &left, &right)),
            },
            BinOp::BitOr => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a | b)),
                _ => Err(self.type_error("|", &left, &right)),
            },
            BinOp::BitXor => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a ^ b)),
                _ => Err(self.type_error("^", &left, &right)),
            },
            BinOp::ShiftLeft => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a << b)),
                _ => Err(self.type_error("<<", &left, &right)),
            },
            BinOp::ShiftRight => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a >> b)),
                _ => Err(self.type_error(">>", &left, &right)),
            },
        }
    }

    fn type_error(&self, op: &str, left: &Value, right: &Value) -> RuntimeError {
        RuntimeError {
            message: format!(
                "Cannot apply '{op}' to {} and {}",
                left.type_name(),
                right.type_name()
            ),
        }
    }

    fn values_equal(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::List(a), Value::List(b)) => {
                a.len() == b.len()
                    && a.iter().zip(b.iter()).all(|(x, y)| self.values_equal(x, y))
            }
            (Value::Tuple(a), Value::Tuple(b)) => {
                a.len() == b.len()
                    && a.iter().zip(b.iter()).all(|(x, y)| self.values_equal(x, y))
            }
            (Value::Map(a), Value::Map(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| {
                        b.iter().any(|(k2, v2)| self.values_equal(k, k2) && self.values_equal(v, v2))
                    })
            }
            (
                Value::EnumVariant {
                    enum_name: ea,
                    variant: va,
                    fields: fa,
                },
                Value::EnumVariant {
                    enum_name: eb,
                    variant: vb,
                    fields: fb,
                },
            ) => {
                ea == eb
                    && va == vb
                    && fa.len() == fb.len()
                    && fa.iter().zip(fb.iter()).all(|(a, b)| self.values_equal(a, b))
            }
            _ => false,
        }
    }

    fn compare_values(
        &self,
        left: &Value,
        right: &Value,
        pred: impl Fn(std::cmp::Ordering) -> bool,
    ) -> Result<Value, RuntimeError> {
        let ord = match (left, right) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
            (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
            (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)).unwrap_or(std::cmp::Ordering::Equal),
            (Value::String(a), Value::String(b)) => a.cmp(b),
            (Value::Char(a), Value::Char(b)) => a.cmp(b),
            _ => {
                return Err(RuntimeError {
                    message: format!(
                        "Cannot compare {} and {}",
                        left.type_name(),
                        right.type_name()
                    ),
                });
            }
        };
        Ok(Value::Bool(pred(ord)))
    }

    fn eval_call_args(
        &mut self,
        args: &[CallArg],
    ) -> Result<Vec<(Option<String>, Value)>, RuntimeError> {
        let mut result = Vec::new();
        for arg in args {
            let val = self.eval_expr(&arg.value)?;
            result.push((arg.name.clone(), val));
        }
        Ok(result)
    }

    fn call_function(
        &mut self,
        name: &str,
        args: Vec<(Option<String>, Value)>,
    ) -> Result<Value, RuntimeError> {
        // Resolve short aliases for built-in functions
        let name = match name {
            "prt" => "print",
            "ln" => "len",
            "tsr" => "to_string",
            "toi" => "to_int",
            "tof" => "to_float",
            "typ" => "type_of",
            "rng" => "range",
            other => other,
        };

        // Built-in functions
        match name {
            "print" => {
                let parts: Vec<String> = args.into_iter().map(|(_, v)| v.to_string()).collect();
                println!("{}", parts.join(" "));
                return Ok(Value::Nil);
            }
            "println" => {
                let parts: Vec<String> = args.into_iter().map(|(_, v)| v.to_string()).collect();
                println!("{}", parts.join(" "));
                return Ok(Value::Nil);
            }
            "len" => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "len() takes exactly 1 argument".to_string(),
                    });
                }
                let val = &args[0].1;
                return match val {
                    Value::String(s) => Ok(Value::Int(s.len() as i64)),
                    Value::List(l) => Ok(Value::Int(l.len() as i64)),
                    Value::Map(m) => Ok(Value::Int(m.len() as i64)),
                    _ => Err(RuntimeError {
                        message: format!("len() not supported for {}", val.type_name()),
                    }),
                };
            }
            "type_of" => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "type_of() takes exactly 1 argument".to_string(),
                    });
                }
                return Ok(Value::String(args[0].1.type_name().to_string()));
            }
            "to_string" => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "to_string() takes exactly 1 argument".to_string(),
                    });
                }
                return Ok(Value::String(args[0].1.to_string()));
            }
            "to_int" => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "to_int() takes exactly 1 argument".to_string(),
                    });
                }
                return match &args[0].1 {
                    Value::Int(n) => Ok(Value::Int(*n)),
                    Value::Float(f) => Ok(Value::Int(*f as i64)),
                    Value::String(s) => s.parse::<i64>().map(Value::Int).map_err(|_| RuntimeError {
                        message: format!("Cannot convert '{s}' to Int"),
                    }),
                    Value::Bool(b) => Ok(Value::Int(if *b { 1 } else { 0 })),
                    _ => Err(RuntimeError {
                        message: format!("Cannot convert {} to Int", args[0].1.type_name()),
                    }),
                };
            }
            "to_float" => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "to_float() takes exactly 1 argument".to_string(),
                    });
                }
                return match &args[0].1 {
                    Value::Int(n) => Ok(Value::Float(*n as f64)),
                    Value::Float(f) => Ok(Value::Float(*f)),
                    Value::String(s) => {
                        s.parse::<f64>().map(Value::Float).map_err(|_| RuntimeError {
                            message: format!("Cannot convert '{s}' to Float"),
                        })
                    }
                    _ => Err(RuntimeError {
                        message: format!("Cannot convert {} to Float", args[0].1.type_name()),
                    }),
                };
            }
            "assert" => {
                if args.is_empty() {
                    return Err(RuntimeError {
                        message: "assert() requires at least 1 argument".to_string(),
                    });
                }
                if !args[0].1.is_truthy() {
                    let msg = args.get(1).map(|(_, v)| v.to_string()).unwrap_or_else(|| "Assertion failed".to_string());
                    return Err(RuntimeError { message: msg });
                }
                return Ok(Value::Nil);
            }
            "assert_eq" => {
                if args.len() != 2 {
                    return Err(RuntimeError {
                        message: "assert_eq() requires 2 arguments".to_string(),
                    });
                }
                if !self.values_equal(&args[0].1, &args[1].1) {
                    return Err(RuntimeError {
                        message: format!(
                            "Assertion failed: {} != {}",
                            args[0].1, args[1].1
                        ),
                    });
                }
                return Ok(Value::Nil);
            }
            // --- Math builtins ---
            "abs" => {
                let val = &args[0].1;
                return match val {
                    Value::Int(n) => Ok(Value::Int(n.abs())),
                    Value::Float(f) => Ok(Value::Float(f.abs())),
                    _ => Err(RuntimeError {
                        message: format!("abs() not supported for {}", val.type_name()),
                    }),
                };
            }
            "sqrt" => {
                return match &args[0].1 {
                    Value::Float(f) => Ok(Value::Float(f.sqrt())),
                    Value::Int(n) => Ok(Value::Float((*n as f64).sqrt())),
                    _ => Err(RuntimeError {
                        message: "sqrt() requires a number".to_string(),
                    }),
                };
            }
            "floor" => {
                return match &args[0].1 {
                    Value::Float(f) => Ok(Value::Int(f.floor() as i64)),
                    Value::Int(n) => Ok(Value::Int(*n)),
                    _ => Err(RuntimeError {
                        message: "floor() requires a number".to_string(),
                    }),
                };
            }
            "ceil" => {
                return match &args[0].1 {
                    Value::Float(f) => Ok(Value::Int(f.ceil() as i64)),
                    Value::Int(n) => Ok(Value::Int(*n)),
                    _ => Err(RuntimeError {
                        message: "ceil() requires a number".to_string(),
                    }),
                };
            }
            "round" => {
                return match &args[0].1 {
                    Value::Float(f) => Ok(Value::Int(f.round() as i64)),
                    Value::Int(n) => Ok(Value::Int(*n)),
                    _ => Err(RuntimeError {
                        message: "round() requires a number".to_string(),
                    }),
                };
            }
            "min" => {
                if args.len() != 2 {
                    return Err(RuntimeError {
                        message: "min() takes 2 arguments".to_string(),
                    });
                }
                return match (&args[0].1, &args[1].1) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.min(b))),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.min(*b))),
                    (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).min(*b))),
                    (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.min(*b as f64))),
                    _ => Err(RuntimeError {
                        message: "min() requires numbers".to_string(),
                    }),
                };
            }
            "max" => {
                if args.len() != 2 {
                    return Err(RuntimeError {
                        message: "max() takes 2 arguments".to_string(),
                    });
                }
                return match (&args[0].1, &args[1].1) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.max(b))),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.max(*b))),
                    (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).max(*b))),
                    (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.max(*b as f64))),
                    _ => Err(RuntimeError {
                        message: "max() requires numbers".to_string(),
                    }),
                };
            }
            "PI" => return Ok(Value::Float(std::f64::consts::PI)),
            "E" => return Ok(Value::Float(std::f64::consts::E)),

            // --- I/O ---
            "read_line" => {
                let mut input = std::string::String::new();
                if let Some((_, Value::String(prompt))) = args.first() {
                    eprint!("{prompt}");
                }
                std::io::stdin()
                    .read_line(&mut input)
                    .map_err(|e| RuntimeError {
                        message: format!("read_line error: {e}"),
                    })?;
                return Ok(Value::String(input.trim_end_matches('\n').to_string()));
            }

            // --- Collection utilities ---
            "range" => {
                if args.len() == 1 {
                    // range(n) → range(0, n)
                    return match &args[0].1 {
                        Value::Int(end) => {
                            let items: Vec<Value> = (0..*end).map(Value::Int).collect();
                            Ok(Value::List(items))
                        }
                        _ => Err(RuntimeError {
                            message: "range() requires Int arguments".to_string(),
                        }),
                    };
                }
                if args.len() != 2 {
                    return Err(RuntimeError {
                        message: "range() takes 1 or 2 arguments".to_string(),
                    });
                }
                return match (&args[0].1, &args[1].1) {
                    (Value::Int(start), Value::Int(end)) => {
                        let items: Vec<Value> = (*start..*end).map(Value::Int).collect();
                        Ok(Value::List(items))
                    }
                    _ => Err(RuntimeError {
                        message: "range() requires Int arguments".to_string(),
                    }),
                };
            }
            "enumerate" => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "enumerate() takes 1 argument".to_string(),
                    });
                }
                return match &args[0].1 {
                    Value::List(items) => {
                        let pairs: Vec<Value> = items
                            .iter()
                            .enumerate()
                            .map(|(i, v)| Value::Tuple(vec![Value::Int(i as i64), v.clone()]))
                            .collect();
                        Ok(Value::List(pairs))
                    }
                    _ => Err(RuntimeError {
                        message: "enumerate() requires a List".to_string(),
                    }),
                };
            }
            "zip" => {
                if args.len() != 2 {
                    return Err(RuntimeError {
                        message: "zip() takes 2 arguments".to_string(),
                    });
                }
                return match (&args[0].1, &args[1].1) {
                    (Value::List(a), Value::List(b)) => {
                        let pairs: Vec<Value> = a
                            .iter()
                            .zip(b.iter())
                            .map(|(x, y)| Value::Tuple(vec![x.clone(), y.clone()]))
                            .collect();
                        Ok(Value::List(pairs))
                    }
                    _ => Err(RuntimeError {
                        message: "zip() requires two Lists".to_string(),
                    }),
                };
            }
            "assert_contains" => {
                if args.len() != 2 {
                    return Err(RuntimeError {
                        message: "assert_contains() requires 2 arguments (haystack, needle)".to_string(),
                    });
                }
                match (&args[0].1, &args[1].1) {
                    (Value::String(haystack), Value::String(needle)) => {
                        if !haystack.contains(needle.as_str()) {
                            return Err(RuntimeError {
                                message: format!("Assertion failed: '{}' does not contain '{}'", haystack, needle),
                            });
                        }
                    }
                    (Value::List(items), needle) => {
                        if !items.iter().any(|item| self.values_equal(item, needle)) {
                            return Err(RuntimeError {
                                message: format!("Assertion failed: list does not contain {}", needle),
                            });
                        }
                    }
                    _ => {
                        return Err(RuntimeError {
                            message: "assert_contains() requires String/List as first argument".to_string(),
                        });
                    }
                }
                return Ok(Value::Nil);
            }
            "assert_ne" => {
                if args.len() != 2 {
                    return Err(RuntimeError {
                        message: "assert_ne() requires 2 arguments".to_string(),
                    });
                }
                if self.values_equal(&args[0].1, &args[1].1) {
                    return Err(RuntimeError {
                        message: format!(
                            "Assertion failed: {} == {} (expected not equal)",
                            args[0].1, args[1].1
                        ),
                    });
                }
                return Ok(Value::Nil);
            }
            _ => {}
        }

        // Check @deprecated (before any call path)
        if let Some(msg) = self.deprecated_fns.get(name).cloned() {
            if !self.deprecated_warned.contains(name) {
                self.deprecated_warned.insert(name.to_string());
                eprintln!("\x1b[33mwarning\x1b[0m: {msg}");
            }
        }

        // Check if name refers to a variable holding a callable value
        if let Ok(val) = self.get_var(name) {
            match val {
                Value::Lambda { .. } | Value::Function { .. } => {
                    return self.call_value(val, args);
                }
                _ => {}
            }
        }

        // Check @memoize cache
        if self.memo_caches.contains_key(name) {
            let cache_key = args.iter().map(|(_, v)| v.to_string()).collect::<Vec<_>>().join(",");
            if let Some(cached) = self.memo_caches.get(name).and_then(|c| c.get(&cache_key)) {
                return Ok(cached.clone());
            }
        }

        // User-defined functions
        let fn_def = self.find_function(name)?;
        let params = fn_def.params.clone();
        let body = fn_def.body.clone();

        self.scopes.push(Scope::new());
        // Build cache key before bind_args consumes args
        let cache_key = if self.memo_caches.contains_key(name) {
            Some(args.iter().map(|(_, v)| v.to_string()).collect::<Vec<_>>().join(","))
        } else {
            None
        };
        let fn_name = name.to_string();

        self.bind_args(&params, args)?;
        let result = self.eval_block(&body)?;
        self.scopes.pop();

        let final_val = match result {
            Value::Return(v) => *v,
            other => other,
        };

        // Store in memo cache
        if let Some(key) = cache_key {
            if let Some(cache) = self.memo_caches.get_mut(&fn_name) {
                cache.insert(key, final_val.clone());
            }
        }

        Ok(final_val)
    }

    fn call_static_method(
        &mut self,
        type_name: &str,
        method: &str,
        args: Vec<(Option<String>, Value)>,
    ) -> Result<Value, RuntimeError> {
        // Look up the struct and its methods
        let method_def = {
            let mut found = None;
            for scope in self.scopes.iter().rev() {
                if let Some(sdef) = scope.structs.get(type_name) {
                    if let Some(m) = sdef.methods.get(method) {
                        found = Some(m.clone());
                        break;
                    }
                }
            }
            found
        };

        if let Some(fdef) = method_def {
            let params = fdef.params.clone();
            let body = fdef.body.clone();
            self.scopes.push(Scope::new());
            self.bind_args(&params, args)?;
            let result = self.eval_block(&body)?;
            self.scopes.pop();
            match result {
                Value::Return(v) => Ok(*v),
                other => Ok(other),
            }
        } else {
            Err(RuntimeError {
                message: format!("No method '{method}' on type '{type_name}'"),
            })
        }
    }

    fn call_method(
        &mut self,
        object: Value,
        method: &str,
        args: Vec<(Option<String>, Value)>,
    ) -> Result<Value, RuntimeError> {
        // Resolve short method aliases
        let method = match method {
            "ln" => "len",
            "mp" => "map",
            "fl" => "filter",
            "fd" => "fold",
            "rd" => "reduce",
            "rv" => "reverse",
            "srt" => "sort",
            "unq" => "unique",
            "cnt" => "count",
            "jn" => "join",
            "psh" => "push",
            "con" => "contains",
            "ck" => "contains_key",
            _ => method,
        };

        // Built-in methods on types
        match (&object, method) {
            (Value::List(items), "len") => return Ok(Value::Int(items.len() as i64)),
            (Value::List(items), "is_empty") => return Ok(Value::Bool(items.is_empty())),
            (Value::List(items), "map") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "map() takes exactly 1 argument".to_string(),
                    });
                }
                let func = args.into_iter().next().unwrap().1;
                let mut result = Vec::new();
                for item in items {
                    result.push(self.call_value(func.clone(), vec![(None, item.clone())])?);
                }
                return Ok(Value::List(result));
            }
            (Value::List(items), "filter") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "filter() takes exactly 1 argument".to_string(),
                    });
                }
                let func = args.into_iter().next().unwrap().1;
                let mut result = Vec::new();
                for item in items {
                    let keep = self.call_value(func.clone(), vec![(None, item.clone())])?;
                    if keep.is_truthy() {
                        result.push(item.clone());
                    }
                }
                return Ok(Value::List(result));
            }
            (Value::List(items), "fold") => {
                if args.len() != 2 {
                    return Err(RuntimeError {
                        message: "fold() takes exactly 2 arguments".to_string(),
                    });
                }
                let mut iter = args.into_iter();
                let mut acc = iter.next().unwrap().1;
                let func = iter.next().unwrap().1;
                for item in items {
                    acc = self.call_value(func.clone(), vec![(None, acc), (None, item.clone())])?;
                }
                return Ok(acc);
            }
            (Value::List(items), "any") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "any() takes exactly 1 argument".to_string(),
                    });
                }
                let func = args.into_iter().next().unwrap().1;
                for item in items {
                    let result = self.call_value(func.clone(), vec![(None, item.clone())])?;
                    if result.is_truthy() {
                        return Ok(Value::Bool(true));
                    }
                }
                return Ok(Value::Bool(false));
            }
            (Value::List(items), "all") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "all() takes exactly 1 argument".to_string(),
                    });
                }
                let func = args.into_iter().next().unwrap().1;
                for item in items {
                    let result = self.call_value(func.clone(), vec![(None, item.clone())])?;
                    if !result.is_truthy() {
                        return Ok(Value::Bool(false));
                    }
                }
                return Ok(Value::Bool(true));
            }
            (Value::List(items), "find") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "find() takes exactly 1 argument".to_string(),
                    });
                }
                let func = args.into_iter().next().unwrap().1;
                for item in items {
                    let result = self.call_value(func.clone(), vec![(None, item.clone())])?;
                    if result.is_truthy() {
                        return Ok(item.clone());
                    }
                }
                return Ok(Value::Nil);
            }
            (Value::List(items), "reverse") => {
                let mut r = items.clone();
                r.reverse();
                return Ok(Value::List(r));
            }
            (Value::List(items), "contains") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "contains() takes exactly 1 argument".to_string(),
                    });
                }
                let needle = &args[0].1;
                for item in items {
                    if self.values_equal(item, needle) {
                        return Ok(Value::Bool(true));
                    }
                }
                return Ok(Value::Bool(false));
            }
            (Value::List(items), "push") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "push() takes exactly 1 argument".to_string(),
                    });
                }
                let mut new_list = items.clone();
                new_list.push(args.into_iter().next().unwrap().1);
                return Ok(Value::List(new_list));
            }
            (Value::List(items), "join") => {
                let sep = match args.first() {
                    Some((_, Value::String(s))) => s.as_str(),
                    _ => "",
                };
                let parts: Vec<std::string::String> = items.iter().map(|v| v.to_string()).collect();
                return Ok(Value::String(parts.join(sep)));
            }
            (Value::List(items), "sort") => {
                let mut sorted = items.clone();
                sorted.sort_by(|a, b| match (a, b) {
                    (Value::Int(a), Value::Int(b)) => a.cmp(b),
                    (Value::Float(a), Value::Float(b)) => {
                        a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    (Value::String(a), Value::String(b)) => a.cmp(b),
                    _ => std::cmp::Ordering::Equal,
                });
                return Ok(Value::List(sorted));
            }
            (Value::List(items), "enumerate") => {
                let pairs: Vec<Value> = items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| Value::Tuple(vec![Value::Int(i as i64), v.clone()]))
                    .collect();
                return Ok(Value::List(pairs));
            }
            (Value::List(items), "flat_map") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "flat_map() takes exactly 1 argument".to_string(),
                    });
                }
                let func = args.into_iter().next().unwrap().1;
                let mut result = Vec::new();
                for item in items {
                    let val = self.call_value(func.clone(), vec![(None, item.clone())])?;
                    if let Value::List(sub) = val {
                        result.extend(sub);
                    } else {
                        result.push(val);
                    }
                }
                return Ok(Value::List(result));
            }
            (Value::List(items), "each") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "each() takes exactly 1 argument".to_string(),
                    });
                }
                let func = args.into_iter().next().unwrap().1;
                for item in items {
                    self.call_value(func.clone(), vec![(None, item.clone())])?;
                }
                return Ok(Value::Nil);
            }
            (Value::List(items), "first") => {
                return Ok(items.first().cloned().unwrap_or(Value::Nil));
            }
            (Value::List(items), "last") => {
                return Ok(items.last().cloned().unwrap_or(Value::Nil));
            }
            (Value::List(items), "take") => {
                if let Some((_, Value::Int(n))) = args.first() {
                    let n = (*n as usize).min(items.len());
                    return Ok(Value::List(items[..n].to_vec()));
                }
                return Err(RuntimeError {
                    message: "take() requires an Int argument".to_string(),
                });
            }
            (Value::List(items), "skip") => {
                if let Some((_, Value::Int(n))) = args.first() {
                    let n = (*n as usize).min(items.len());
                    return Ok(Value::List(items[n..].to_vec()));
                }
                return Err(RuntimeError {
                    message: "skip() requires an Int argument".to_string(),
                });
            }
            (Value::List(items), "flatten") => {
                let mut result = Vec::new();
                for item in items {
                    if let Value::List(sub) = item {
                        result.extend(sub.clone());
                    } else {
                        result.push(item.clone());
                    }
                }
                return Ok(Value::List(result));
            }
            (Value::List(items), "sum") => {
                let mut int_sum: i64 = 0;
                let mut is_float = false;
                let mut float_sum: f64 = 0.0;
                for item in items {
                    match item {
                        Value::Int(n) => {
                            int_sum += n;
                            float_sum += *n as f64;
                        }
                        Value::Float(f) => {
                            is_float = true;
                            float_sum += f;
                        }
                        _ => return Err(RuntimeError {
                            message: "sum() requires a list of numbers".to_string(),
                        }),
                    }
                }
                return Ok(if is_float { Value::Float(float_sum) } else { Value::Int(int_sum) });
            }
            (Value::List(items), "min") => {
                if items.is_empty() { return Ok(Value::Nil); }
                let mut best = &items[0];
                for item in &items[1..] {
                    match (item, best) {
                        (Value::Int(a), Value::Int(b)) if a < b => best = item,
                        (Value::Float(a), Value::Float(b)) if a < b => best = item,
                        _ => {}
                    }
                }
                return Ok(best.clone());
            }
            (Value::List(items), "max") => {
                if items.is_empty() { return Ok(Value::Nil); }
                let mut best = &items[0];
                for item in &items[1..] {
                    match (item, best) {
                        (Value::Int(a), Value::Int(b)) if a > b => best = item,
                        (Value::Float(a), Value::Float(b)) if a > b => best = item,
                        _ => {}
                    }
                }
                return Ok(best.clone());
            }
            (Value::List(items), "zip") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "zip() takes exactly 1 argument".to_string(),
                    });
                }
                if let Value::List(other) = &args[0].1 {
                    let pairs: Vec<Value> = items
                        .iter()
                        .zip(other.iter())
                        .map(|(a, b)| Value::Tuple(vec![a.clone(), b.clone()]))
                        .collect();
                    return Ok(Value::List(pairs));
                }
                return Err(RuntimeError {
                    message: "zip() requires a List argument".to_string(),
                });
            }
            (Value::List(items), "unique") => {
                let mut result = Vec::new();
                for item in items {
                    if !result.iter().any(|x| self.values_equal(x, item)) {
                        result.push(item.clone());
                    }
                }
                return Ok(Value::List(result));
            }
            (Value::List(items), "count") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "count() takes exactly 1 argument".to_string(),
                    });
                }
                let func = args.into_iter().next().unwrap().1;
                let mut n = 0i64;
                for item in items {
                    let result = self.call_value(func.clone(), vec![(None, item.clone())])?;
                    if result.is_truthy() { n += 1; }
                }
                return Ok(Value::Int(n));
            }
            (Value::List(items), "reduce") => {
                // reduce is fold without initial value — uses first element
                if args.len() != 1 || items.is_empty() {
                    return Err(RuntimeError {
                        message: "reduce() takes 1 argument and requires non-empty list".to_string(),
                    });
                }
                let func = args.into_iter().next().unwrap().1;
                let mut acc = items[0].clone();
                for item in &items[1..] {
                    acc = self.call_value(func.clone(), vec![(None, acc), (None, item.clone())])?;
                }
                return Ok(acc);
            }
            (Value::Map(entries), "len") => return Ok(Value::Int(entries.len() as i64)),
            (Value::Map(entries), "is_empty") => return Ok(Value::Bool(entries.is_empty())),
            (Value::Map(entries), "keys") => {
                let keys: Vec<Value> = entries.iter().map(|(k, _)| k.clone()).collect();
                return Ok(Value::List(keys));
            }
            (Value::Map(entries), "values") => {
                let vals: Vec<Value> = entries.iter().map(|(_, v)| v.clone()).collect();
                return Ok(Value::List(vals));
            }
            (Value::Map(entries), "entries") => {
                let pairs: Vec<Value> = entries
                    .iter()
                    .map(|(k, v)| Value::Tuple(vec![k.clone(), v.clone()]))
                    .collect();
                return Ok(Value::List(pairs));
            }
            (Value::Map(entries), "contains_key") => {
                if args.len() != 1 {
                    return Err(RuntimeError {
                        message: "contains_key() takes exactly 1 argument".to_string(),
                    });
                }
                let needle = &args[0].1;
                let found = entries.iter().any(|(k, _)| self.values_equal(k, needle));
                return Ok(Value::Bool(found));
            }
            (Value::Map(entries), "get") => {
                if args.is_empty() || args.len() > 2 {
                    return Err(RuntimeError {
                        message: "get() takes 1 or 2 arguments (key, default?)".to_string(),
                    });
                }
                let needle = &args[0].1;
                for (k, v) in entries {
                    if self.values_equal(k, needle) {
                        return Ok(v.clone());
                    }
                }
                return Ok(args.get(1).map(|(_, v)| v.clone()).unwrap_or(Value::Nil));
            }
            (Value::String(s), "len") => return Ok(Value::Int(s.len() as i64)),
            (Value::String(s), "contains") => {
                if let Some((_, Value::String(needle))) = args.first() {
                    return Ok(Value::Bool(s.contains(needle.as_str())));
                }
                return Err(RuntimeError {
                    message: "String.contains() requires a String argument".to_string(),
                });
            }
            (Value::String(s), "split") => {
                if let Some((_, Value::String(sep))) = args.first() {
                    let parts: Vec<Value> = s.split(sep.as_str()).map(|p| Value::String(p.to_string())).collect();
                    return Ok(Value::List(parts));
                }
                return Err(RuntimeError {
                    message: "String.split() requires a String argument".to_string(),
                });
            }
            (Value::String(s), "trim") => return Ok(Value::String(s.trim().to_string())),
            (Value::String(s), "to_upper") => return Ok(Value::String(s.to_uppercase())),
            (Value::String(s), "to_lower") => return Ok(Value::String(s.to_lowercase())),
            (Value::String(s), "starts_with") => {
                if let Some((_, Value::String(prefix))) = args.first() {
                    return Ok(Value::Bool(s.starts_with(prefix.as_str())));
                }
                return Err(RuntimeError {
                    message: "starts_with() requires a String argument".to_string(),
                });
            }
            (Value::String(s), "ends_with") => {
                if let Some((_, Value::String(suffix))) = args.first() {
                    return Ok(Value::Bool(s.ends_with(suffix.as_str())));
                }
                return Err(RuntimeError {
                    message: "ends_with() requires a String argument".to_string(),
                });
            }
            (Value::String(s), "replace") => {
                if args.len() != 2 {
                    return Err(RuntimeError {
                        message: "replace() takes 2 arguments".to_string(),
                    });
                }
                if let (Some((_, Value::String(from))), Some((_, Value::String(to)))) =
                    (args.first(), args.get(1))
                {
                    return Ok(Value::String(s.replace(from.as_str(), to.as_str())));
                }
                return Err(RuntimeError {
                    message: "replace() requires String arguments".to_string(),
                });
            }
            (Value::String(s), "chars") => {
                let chars: Vec<Value> = s.chars().map(Value::Char).collect();
                return Ok(Value::List(chars));
            }
            (Value::String(s), "repeat") => {
                if let Some((_, Value::Int(n))) = args.first() {
                    return Ok(Value::String(s.repeat(*n as usize)));
                }
                return Err(RuntimeError {
                    message: "repeat() requires an Int argument".to_string(),
                });
            }
            (Value::String(s), "is_empty") => return Ok(Value::Bool(s.is_empty())),
            (Value::String(s), "index_of") => {
                if let Some((_, Value::String(needle))) = args.first() {
                    return Ok(s.find(needle.as_str())
                        .map(|i| Value::Int(i as i64))
                        .unwrap_or(Value::Int(-1)));
                }
                return Err(RuntimeError {
                    message: "index_of() requires a String argument".to_string(),
                });
            }
            (Value::String(s), "substring") => {
                if args.len() != 2 {
                    return Err(RuntimeError {
                        message: "substring() takes 2 arguments (start, end)".to_string(),
                    });
                }
                if let (Value::Int(start), Value::Int(end)) = (&args[0].1, &args[1].1) {
                    let chars: Vec<char> = s.chars().collect();
                    let si = self.resolve_index(*start, chars.len());
                    let ei = self.resolve_index(*end, chars.len()).min(chars.len());
                    return Ok(Value::String(chars[si..ei].iter().collect()));
                }
                return Err(RuntimeError {
                    message: "substring() requires Int arguments".to_string(),
                });
            }
            (Value::String(s), "rev") => {
                return Ok(Value::String(s.chars().rev().collect()));
            }
            _ => {}
        }

        // Struct methods
        if let Value::Struct { name: type_name, .. } = &object {
            let method_def = {
                let mut found = None;
                for scope in self.scopes.iter().rev() {
                    if let Some(sdef) = scope.structs.get(type_name) {
                        if let Some(m) = sdef.methods.get(method) {
                            found = Some(m.clone());
                            break;
                        }
                    }
                }
                found
            };

            if let Some(fdef) = method_def {
                let params = fdef.params.clone();
                let body = fdef.body.clone();
                self.scopes.push(Scope::new());
                // Bind self
                self.define_var("sf", object, false);
                // Bind remaining params (skip sf param)
                let non_self_params: Vec<_> = params.into_iter().filter(|p| p.name != "sf").collect();
                self.bind_args(&non_self_params, args)?;
                let result = self.eval_block(&body)?;
                self.scopes.pop();
                return match result {
                    Value::Return(v) => Ok(*v),
                    other => Ok(other),
                };
            }
        }

        Err(RuntimeError {
            message: format!("No method '{method}' on {}", object.type_name()),
        })
    }

    fn call_value(
        &mut self,
        callee: Value,
        args: Vec<(Option<String>, Value)>,
    ) -> Result<Value, RuntimeError> {
        match callee {
            Value::Lambda {
                params,
                body,
                closure,
            } => {
                self.scopes.push(Scope::new());
                // Restore closure with original mutability
                for (name, val, mutable) in closure {
                    self.define_var(&name, val, mutable);
                }
                // Bind params
                for (i, param) in params.iter().enumerate() {
                    let val = args
                        .get(i)
                        .map(|(_, v)| v.clone())
                        .unwrap_or(Value::Nil);
                    self.define_var(&param.name, val, false);
                }
                let result = self.eval_expr(&body)?;
                self.scopes.pop();
                Ok(result)
            }
            Value::Function {
                params,
                body,
                closure,
                ..
            } => {
                self.scopes.push(Scope::new());
                for (name, val, mutable) in closure {
                    self.define_var(&name, val, mutable);
                }
                self.bind_args(&params, args)?;
                let result = self.eval_block(&body)?;
                self.scopes.pop();
                match result {
                    Value::Return(v) => Ok(*v),
                    other => Ok(other),
                }
            }
            _ => Err(RuntimeError {
                message: format!("Cannot call {}", callee.type_name()),
            }),
        }
    }

    // --- Variable management ---

    fn define_var(&mut self, name: &str, value: Value, mutable: bool) {
        let scope = self.scopes.last_mut().unwrap();
        scope
            .variables
            .insert(name.to_string(), Variable { value, mutable });
    }

    /// Resolve a possibly-negative index against a collection length.
    /// Negative indices count from the end: -1 = last element.
    fn resolve_index(&self, idx: i64, len: usize) -> usize {
        if idx < 0 {
            let pos = len as i64 + idx;
            if pos < 0 { 0 } else { pos as usize }
        } else {
            idx as usize
        }
    }

    fn get_var(&self, name: &str) -> Result<Value, RuntimeError> {
        for scope in self.scopes.iter().rev() {
            if let Some(var) = scope.variables.get(name) {
                return Ok(var.value.clone());
            }
        }
        // Fall back to functions — enables first-class function references
        for scope in self.scopes.iter().rev() {
            if let Some(fdef) = scope.functions.get(name) {
                return Ok(Value::Function {
                    name: name.to_string(),
                    params: fdef.params.clone(),
                    body: fdef.body.clone(),
                    closure: self.capture_closure(),
                });
            }
        }
        Err(RuntimeError {
            message: format!("Undefined variable: '{name}'"),
        })
    }

    fn set_var(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(var) = scope.variables.get_mut(name) {
                if !var.mutable {
                    return Err(RuntimeError {
                        message: format!("Cannot assign to immutable variable '{name}'"),
                    });
                }
                var.value = value;
                return Ok(());
            }
        }
        Err(RuntimeError {
            message: format!("Undefined variable: '{name}'"),
        })
    }

    fn find_function(&self, name: &str) -> Result<FnDef, RuntimeError> {
        for scope in self.scopes.iter().rev() {
            if let Some(fdef) = scope.functions.get(name) {
                return Ok(fdef.clone());
            }
        }
        Err(RuntimeError {
            message: format!("Undefined function: '{name}'"),
        })
    }

    fn find_enum(&self, name: &str) -> Option<EnumDef> {
        for scope in self.scopes.iter().rev() {
            if let Some(edef) = scope.enums.get(name) {
                return Some(edef.clone());
            }
        }
        None
    }

    fn capture_closure(&self) -> Vec<(String, Value, bool)> {
        let mut captured = Vec::new();
        for scope in &self.scopes {
            for (name, var) in &scope.variables {
                captured.push((name.clone(), var.value.clone(), var.mutable));
            }
        }
        captured
    }

    fn bind_args(
        &mut self,
        params: &[Param],
        args: Vec<(Option<String>, Value)>,
    ) -> Result<(), RuntimeError> {
        // Count required params (no default value, not variadic)
        let required = params.iter().filter(|p| p.default.is_none() && !p.is_variadic).count();
        let positional_args = args.iter().filter(|(n, _)| n.is_none()).count();

        if positional_args > params.len() && !params.iter().any(|p| p.is_variadic) {
            return Err(RuntimeError {
                message: format!(
                    "Expected {} argument{}, got {}",
                    params.len(),
                    if params.len() == 1 { "" } else { "s" },
                    positional_args
                ),
            });
        }

        // Handle named + positional args
        let mut positional_idx = 0;
        let mut used = vec![false; args.len()];

        for param in params {
            let val = if let Some(pos) = args.iter().position(|(name, _)| {
                name.as_deref() == Some(&param.name)
            }) {
                used[pos] = true;
                args[pos].1.clone()
            } else {
                while positional_idx < args.len() && used[positional_idx] {
                    positional_idx += 1;
                }
                if positional_idx < args.len() {
                    used[positional_idx] = true;
                    positional_idx += 1;
                    args[positional_idx - 1].1.clone()
                } else if let Some(default) = &param.default {
                    self.eval_expr(default)?
                } else {
                    return Err(RuntimeError {
                        message: format!(
                            "Missing argument '{}' (expected at least {})",
                            param.name, required
                        ),
                    });
                }
            };
            self.define_var(&param.name, val, false);
        }

        Ok(())
    }

    fn pattern_matches(&self, pattern: &Pattern, value: &Value) -> Result<bool, RuntimeError> {
        match pattern {
            Pattern::Wildcard => Ok(true),
            Pattern::Binding(_) => Ok(true),
            Pattern::Literal(expr) => {
                // Compare the literal expression against the value
                match expr {
                    Expr::IntLit(n) => {
                        if let Value::Int(v) = value {
                            Ok(n == v)
                        } else {
                            Ok(false)
                        }
                    }
                    Expr::FloatLit(f) => {
                        if let Value::Float(v) = value {
                            Ok(f == v)
                        } else {
                            Ok(false)
                        }
                    }
                    Expr::BoolLit(b) => {
                        if let Value::Bool(v) = value {
                            Ok(b == v)
                        } else {
                            Ok(false)
                        }
                    }
                    Expr::CharLit(c) => {
                        if let Value::Char(v) = value {
                            Ok(c == v)
                        } else {
                            Ok(false)
                        }
                    }
                    Expr::StringLit(s) => {
                        if let Value::String(v) = value {
                            // Compare simple (non-interpolated) strings
                            if s.parts.len() == 1 {
                                if let crate::lexer::StringPart::Literal(text) = &s.parts[0] {
                                    return Ok(text == v);
                                }
                            }
                            Ok(false)
                        } else {
                            Ok(false)
                        }
                    }
                    Expr::Nil => Ok(matches!(value, Value::Nil)),
                    _ => Ok(false),
                }
            }
            Pattern::Range(start, end, inclusive) => {
                // For now just handle Int ranges
                if let (Expr::IntLit(s), Expr::IntLit(e), Value::Int(v)) = (start.as_ref(), end.as_ref(), value) {
                    if *inclusive {
                        Ok(v >= s && v <= e)
                    } else {
                        Ok(v >= s && v < e)
                    }
                } else {
                    Ok(false)
                }
            }
            Pattern::Variant(pat_name, sub_patterns) => {
                if let Value::EnumVariant {
                    enum_name,
                    variant,
                    fields,
                } = value
                {
                    // pat_name is "EnumName.Variant" or just "Variant"
                    let full_name = format!("{enum_name}.{variant}");
                    if *pat_name == full_name || *pat_name == *variant {
                        // Check sub-pattern count matches
                        if sub_patterns.len() == fields.len() || sub_patterns.is_empty() {
                            // Check each sub-pattern matches
                            for (sp, fv) in sub_patterns.iter().zip(fields.iter()) {
                                if !self.pattern_matches(sp, fv)? {
                                    return Ok(false);
                                }
                            }
                            return Ok(true);
                        }
                    }
                    Ok(false)
                } else {
                    Ok(false)
                }
            }
        }
    }

    fn bind_pattern(&mut self, pattern: &Pattern, value: &Value) {
        match pattern {
            Pattern::Binding(name) => {
                self.define_var(name, value.clone(), false);
            }
            Pattern::Variant(_, sub_patterns) => {
                if let Value::EnumVariant { fields, .. } = value {
                    for (sp, fv) in sub_patterns.iter().zip(fields.iter()) {
                        self.bind_pattern(sp, fv);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    /// Run source and return the REPL result (last expression value)
    fn eval(source: &str) -> Option<Value> {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        let mut interp = Interpreter::new();
        interp.execute_interactive(&program).unwrap()
    }

    fn eval_val(source: &str) -> Value {
        eval(source).expect("Expected a value")
    }

    fn eval_err(source: &str) -> String {
        let tokens = lexer::tokenize(source).unwrap();
        let program = parser::parse(tokens).unwrap();
        let mut interp = Interpreter::new();
        interp.execute(&program).unwrap_err().message
    }

    // --- Arithmetic ---

    #[test]
    fn test_int_arithmetic() {
        assert!(matches!(eval_val("2 + 3"), Value::Int(5)));
        assert!(matches!(eval_val("10 - 4"), Value::Int(6)));
        assert!(matches!(eval_val("3 * 7"), Value::Int(21)));
        assert!(matches!(eval_val("15 / 4"), Value::Int(3)));
        assert!(matches!(eval_val("17 % 5"), Value::Int(2)));
        assert!(matches!(eval_val("2 ** 10"), Value::Int(1024)));
    }

    #[test]
    fn test_float_arithmetic() {
        if let Value::Float(f) = eval_val("1.5 + 2.5") {
            assert!((f - 4.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Float");
        }
    }

    #[test]
    fn test_float_modulo() {
        if let Value::Float(f) = eval_val("10.5 % 3.0") {
            assert!((f - 1.5).abs() < f64::EPSILON);
        } else {
            panic!("Expected Float");
        }
        // Mixed types
        if let Value::Float(f) = eval_val("10 % 3.0") {
            assert!((f - 1.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Float");
        }
    }

    #[test]
    fn test_mixed_numeric() {
        if let Value::Float(f) = eval_val("1 + 2.0") {
            assert!((f - 3.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Float");
        }
    }

    #[test]
    fn test_division_by_zero() {
        let err = eval_err("lt x = 1 / 0");
        assert!(err.contains("Division by zero"));
    }

    #[test]
    fn test_unary_neg() {
        assert!(matches!(eval_val("-42"), Value::Int(-42)));
    }

    #[test]
    fn test_unary_not() {
        assert!(matches!(eval_val("not tt"), Value::Bool(false)));
        assert!(matches!(eval_val("not ff"), Value::Bool(true)));
    }

    // --- Strings ---

    #[test]
    fn test_string_concat() {
        if let Value::String(s) = eval_val(r#""hello" + " " + "world""#) {
            assert_eq!(s, "hello world");
        } else {
            panic!("Expected String");
        }
    }

    #[test]
    fn test_string_repeat() {
        if let Value::String(s) = eval_val(r#""ab" * 3"#) {
            assert_eq!(s, "ababab");
        } else {
            panic!("Expected String");
        }
    }

    #[test]
    fn test_string_interpolation() {
        if let Value::String(s) = eval_val(r#"lt name = "Nebula"
"Hello, {name}!""#) {
            assert_eq!(s, "Hello, Nebula!");
        } else {
            panic!("Expected String");
        }
    }

    #[test]
    fn test_string_interpolation_expression() {
        if let Value::String(s) = eval_val(r#"lt x = 10
"result: {x + 5}""#) {
            assert_eq!(s, "result: 15");
        } else {
            panic!("Expected String");
        }
    }

    #[test]
    fn test_string_interpolation_function_call() {
        if let Value::String(s) = eval_val(r#"fn double(n) = n * 2
"val: {double(21)}""#) {
            assert_eq!(s, "val: 42");
        } else {
            panic!("Expected String");
        }
    }

    // --- Variables ---

    #[test]
    fn test_let_binding() {
        assert!(matches!(eval_val("lt x = 42\nx"), Value::Int(42)));
    }

    #[test]
    fn test_mut_binding() {
        assert!(matches!(
            eval_val("mt x = 1\nx += 10\nx"),
            Value::Int(11)
        ));
    }

    #[test]
    fn test_immutable_assignment_error() {
        let err = eval_err("lt x = 1\nx = 2");
        assert!(err.contains("immutable"));
    }

    // --- Comparisons & Logic ---

    #[test]
    fn test_comparisons() {
        assert!(matches!(eval_val("1 < 2"), Value::Bool(true)));
        assert!(matches!(eval_val("2 > 1"), Value::Bool(true)));
        assert!(matches!(eval_val("5 == 5"), Value::Bool(true)));
        assert!(matches!(eval_val("5 != 6"), Value::Bool(true)));
        assert!(matches!(eval_val("3 <= 3"), Value::Bool(true)));
        assert!(matches!(eval_val("4 >= 5"), Value::Bool(false)));
    }

    #[test]
    fn test_logical_and_or() {
        assert!(matches!(eval_val("tt and tt"), Value::Bool(true)));
        assert!(matches!(eval_val("tt and ff"), Value::Bool(false)));
        assert!(matches!(eval_val("ff or tt"), Value::Bool(true)));
    }

    // --- Control Flow ---

    #[test]
    fn test_if_expression() {
        assert!(matches!(
            eval_val("if tt { 1 } el { 2 }"),
            Value::Int(1)
        ));
        assert!(matches!(
            eval_val("if ff { 1 } el { 2 }"),
            Value::Int(2)
        ));
    }

    #[test]
    fn test_while_loop() {
        assert!(matches!(
            eval_val("mt x = 0\nwl x < 5 { x += 1 }\nx"),
            Value::Int(5)
        ));
    }

    #[test]
    fn test_for_loop() {
        // For loop over range, summing
        assert!(matches!(
            eval_val("mt s = 0\nfo x in [1, 2, 3] { s += x }\ns"),
            Value::Int(6)
        ));
    }

    #[test]
    fn test_loop_break() {
        assert!(matches!(
            eval_val("mt i = 0\nlp {\n  i += 1\n  if i == 10 { bk }\n}\ni"),
            Value::Int(10)
        ));
    }

    // --- Functions ---

    #[test]
    fn test_function_call() {
        assert!(matches!(
            eval_val("fn double(n) { n * 2 }\ndouble(21)"),
            Value::Int(42)
        ));
    }

    #[test]
    fn test_recursive_function() {
        assert!(matches!(
            eval_val("fn fact(n) { if n <= 1 { 1 } el { n * fact(n - 1) } }\nfact(5)"),
            Value::Int(120)
        ));
    }

    #[test]
    fn test_lambda() {
        assert!(matches!(
            eval_val("lt f = |x| x + 1\nf(9)"),
            Value::Int(10)
        ));
    }

    #[test]
    fn test_higher_order_function() {
        assert!(matches!(
            eval_val("fn apply(f, x) { f(x) }\nlt sq = |n| n * n\napply(sq, 6)"),
            Value::Int(36)
        ));
    }

    #[test]
    fn test_default_params() {
        assert!(matches!(
            eval_val("fn greet(name, prefix = \"Hello\") { prefix }\ngreet(\"x\")"),
            Value::String(s) if s == "Hello"
        ));
    }

    // --- Collections ---

    #[test]
    fn test_list_operations() {
        // map
        if let Value::List(items) = eval_val("[1,2,3].map(|x| x * 10)") {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0], Value::Int(10)));
            assert!(matches!(items[2], Value::Int(30)));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_list_filter() {
        if let Value::List(items) = eval_val("[1,2,3,4,5].filter(|x| x > 3)") {
            assert_eq!(items.len(), 2);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_list_fold() {
        assert!(matches!(
            eval_val("[1,2,3,4].fold(0, |a, b| a + b)"),
            Value::Int(10)
        ));
    }

    #[test]
    fn test_list_index() {
        assert!(matches!(eval_val("[10, 20, 30][1]"), Value::Int(20)));
    }

    #[test]
    fn test_range_to_list() {
        if let Value::List(items) = eval_val("1..4") {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0], Value::Int(1)));
            assert!(matches!(items[2], Value::Int(3)));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_range_inclusive() {
        if let Value::List(items) = eval_val("1..=3") {
            assert_eq!(items.len(), 3);
        } else {
            panic!("Expected List");
        }
    }

    // --- Structs ---

    #[test]
    fn test_struct_creation() {
        if let Value::Struct { name, fields } =
            eval_val("st Pt { x: Int\n y: Int }\nPt { x: 1, y: 2 }")
        {
            assert_eq!(name, "Pt");
            assert!(matches!(fields.get("x"), Some(Value::Int(1))));
        } else {
            panic!("Expected Struct");
        }
    }

    #[test]
    fn test_struct_method() {
        assert!(matches!(
            eval_val(
                "st V { x: Int }\nim V {\n  fn doubled(sf) -> Int { sf.x * 2 }\n}\nlt v = V { x: 21 }\nv.doubled()"
            ),
            Value::Int(42)
        ));
    }

    #[test]
    fn test_struct_spread() {
        if let Value::Struct { fields, .. } =
            eval_val("st P { a: Int\n b: Int }\nlt p = P { a: 1, b: 2 }\nP { ...p, a: 99 }")
        {
            assert!(matches!(fields.get("a"), Some(Value::Int(99))));
            assert!(matches!(fields.get("b"), Some(Value::Int(2))));
        } else {
            panic!("Expected Struct");
        }
    }

    // --- Match ---

    #[test]
    fn test_match_literal() {
        assert!(matches!(
            eval_val("mc 1 {\n  0 => \"zero\"\n  1 => \"one\"\n  _ => \"other\"\n}"),
            Value::String(s) if s == "one"
        ));
    }

    #[test]
    fn test_match_wildcard() {
        assert!(matches!(
            eval_val("mc 999 {\n  0 => \"zero\"\n  _ => \"other\"\n}"),
            Value::String(s) if s == "other"
        ));
    }

    #[test]
    fn test_match_range() {
        assert!(matches!(
            eval_val("mc 5 {\n  1..=9 => \"digit\"\n  _ => \"other\"\n}"),
            Value::String(s) if s == "digit"
        ));
    }

    #[test]
    fn test_match_binding() {
        assert!(matches!(
            eval_val("mc 42 {\n  n => n * 2\n}"),
            Value::Int(84)
        ));
    }

    // --- Nil coalescing ---

    #[test]
    fn test_nil_coalesce() {
        assert!(matches!(eval_val("nil ?? 42"), Value::Int(42)));
        assert!(matches!(eval_val("10 ?? 42"), Value::Int(10)));
    }

    // --- Built-in functions ---

    #[test]
    fn test_len() {
        assert!(matches!(eval_val("len([1,2,3])"), Value::Int(3)));
        assert!(matches!(eval_val(r#"len("hello")"#), Value::Int(5)));
    }

    #[test]
    fn test_type_of() {
        assert!(matches!(eval_val("type_of(42)"), Value::String(s) if s == "Int"));
        assert!(matches!(eval_val("type_of(tt)"), Value::String(s) if s == "Bool"));
    }

    // --- String methods ---

    #[test]
    fn test_string_methods() {
        assert!(matches!(eval_val(r#""hello".to_upper()"#), Value::String(s) if s == "HELLO"));
        assert!(matches!(eval_val(r#""  hi  ".trim()"#), Value::String(s) if s == "hi"));
        assert!(matches!(eval_val(r#""hello".contains("ell")"#), Value::Bool(true)));
    }

    // --- Enums ---

    #[test]
    fn test_enum_simple_variant() {
        if let Value::EnumVariant { enum_name, variant, fields } =
            eval_val("en Color { Red, Green, Blue }\nColor.Red")
        {
            assert_eq!(enum_name, "Color");
            assert_eq!(variant, "Red");
            assert!(fields.is_empty());
        } else {
            panic!("Expected EnumVariant");
        }
    }

    #[test]
    fn test_enum_equality() {
        assert!(matches!(
            eval_val("en D { A, B }\nD.A == D.A"),
            Value::Bool(true)
        ));
        assert!(matches!(
            eval_val("en D { A, B }\nD.A == D.B"),
            Value::Bool(false)
        ));
    }

    #[test]
    fn test_enum_variant_with_fields() {
        if let Value::EnumVariant { variant, fields, .. } =
            eval_val("en S { Circle(r: Float) }\nS.Circle(5.0)")
        {
            assert_eq!(variant, "Circle");
            assert_eq!(fields.len(), 1);
            assert!(matches!(fields[0], Value::Float(f) if (f - 5.0).abs() < f64::EPSILON));
        } else {
            panic!("Expected EnumVariant");
        }
    }

    #[test]
    fn test_enum_pattern_matching() {
        assert!(matches!(
            eval_val("en C { R, G, B }\nmc C.R {\n  C.R => \"red\"\n  _ => \"other\"\n}"),
            Value::String(s) if s == "red"
        ));
    }

    #[test]
    fn test_enum_destructuring() {
        assert!(matches!(
            eval_val("en S { V(x: Int) }\nmc S.V(42) {\n  S.V(n) => n\n  _ => 0\n}"),
            Value::Int(42)
        ));
    }

    // --- Standard library ---

    #[test]
    fn test_abs() {
        assert!(matches!(eval_val("abs(-5)"), Value::Int(5)));
        assert!(matches!(eval_val("abs(5)"), Value::Int(5)));
    }

    #[test]
    fn test_sqrt() {
        if let Value::Float(f) = eval_val("sqrt(16.0)") {
            assert!((f - 4.0).abs() < f64::EPSILON);
        } else {
            panic!("Expected Float");
        }
    }

    #[test]
    fn test_floor_ceil_round() {
        assert!(matches!(eval_val("floor(3.7)"), Value::Int(3)));
        assert!(matches!(eval_val("ceil(3.2)"), Value::Int(4)));
        assert!(matches!(eval_val("round(3.5)"), Value::Int(4)));
    }

    #[test]
    fn test_min_max() {
        assert!(matches!(eval_val("min(3, 7)"), Value::Int(3)));
        assert!(matches!(eval_val("max(3, 7)"), Value::Int(7)));
    }

    #[test]
    fn test_list_sort() {
        if let Value::List(items) = eval_val("[3, 1, 2].sort()") {
            assert!(matches!(items[0], Value::Int(1)));
            assert!(matches!(items[1], Value::Int(2)));
            assert!(matches!(items[2], Value::Int(3)));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_list_join() {
        assert!(matches!(
            eval_val(r#"["a", "b", "c"].join("-")"#),
            Value::String(s) if s == "a-b-c"
        ));
    }

    #[test]
    fn test_range_builtin() {
        if let Value::List(items) = eval_val("range(0, 3)") {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0], Value::Int(0)));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_zip() {
        if let Value::List(items) = eval_val("zip([1, 2], [3, 4])") {
            assert_eq!(items.len(), 2);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_string_replace() {
        assert!(matches!(
            eval_val(r#""hello world".replace("world", "neb")"#),
            Value::String(s) if s == "hello neb"
        ));
    }

    #[test]
    fn test_string_chars() {
        if let Value::List(items) = eval_val(r#""abc".chars()"#) {
            assert_eq!(items.len(), 3);
            assert!(matches!(&items[0], Value::Char('a')));
        } else {
            panic!("Expected List");
        }
    }

    // --- Maps ---

    #[test]
    fn test_map_literal() {
        if let Value::Map(entries) = eval_val(r#"{"a": 1, "b": 2}"#) {
            assert_eq!(entries.len(), 2);
        } else {
            panic!("Expected Map");
        }
    }

    #[test]
    fn test_map_index() {
        assert!(matches!(
            eval_val(r#"{"x": 42}["x"]"#),
            Value::Int(42)
        ));
    }

    #[test]
    fn test_empty_map() {
        if let Value::Map(entries) = eval_val("{}") {
            assert!(entries.is_empty());
        } else {
            panic!("Expected Map");
        }
    }

    #[test]
    fn test_map_missing_key() {
        // Missing key returns Nil, which execute_interactive filters
        assert!(eval(r#"{"a": 1}["b"]"#).is_none());
    }

    // --- Try operator ---

    #[test]
    fn test_try_passes_value() {
        assert!(matches!(
            eval_val("fn ok() { 42 }\nfn use_it() { lt x = ok()?\n x }\nuse_it()"),
            Value::Int(42)
        ));
    }

    #[test]
    fn test_try_propagates_nil() {
        // fn that returns nil, caller uses ? — should propagate nil
        assert!(eval("fn fail() { nil }\nfn use_it() { lt x = fail()?\n x }\nuse_it()").is_none());
    }

    #[test]
    fn test_map_keys() {
        if let Value::List(keys) = eval_val(r#"lt m = {"a": 1, "b": 2}
m.keys()"#) {
            assert_eq!(keys.len(), 2);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_map_values() {
        if let Value::List(vals) = eval_val(r#"lt m = {"a": 1, "b": 2}
m.values()"#) {
            assert_eq!(vals.len(), 2);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_map_contains_key() {
        assert!(matches!(
            eval_val(r#"lt m = {"x": 10}
m.contains_key("x")"#),
            Value::Bool(true)
        ));
        assert!(matches!(
            eval_val(r#"lt m = {"x": 10}
m.contains_key("y")"#),
            Value::Bool(false)
        ));
    }

    #[test]
    fn test_continue_in_for_loop() {
        // continue should skip the rest of the loop body
        if let Value::List(items) = eval_val(
            "mt results = []\nfo x in [1, 2, 3, 4, 5] {\n  if x == 3 { ct }\n  results = results.push(x)\n}\nresults",
        ) {
            let ints: Vec<i64> = items
                .iter()
                .map(|v| if let Value::Int(n) = v { *n } else { panic!() })
                .collect();
            assert_eq!(ints, vec![1, 2, 4, 5]);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_continue_in_while_loop() {
        if let Value::Int(n) = eval_val(
            "mt i = 0\nmt sum = 0\nwl i < 10 {\n  i += 1\n  if i % 2 == 0 { ct }\n  sum += i\n}\nsum",
        ) {
            assert_eq!(n, 25); // 1+3+5+7+9
        } else {
            panic!("Expected Int");
        }
    }

    // --- Slicing and negative indices ---

    #[test]
    fn test_list_slice() {
        if let Value::List(items) = eval_val("lt x = [10, 20, 30, 40, 50]\nx[1..3]") {
            let vals: Vec<i64> = items.iter().map(|v| if let Value::Int(n) = v { *n } else { panic!() }).collect();
            assert_eq!(vals, vec![20, 30]);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_list_slice_inclusive() {
        if let Value::List(items) = eval_val("lt x = [10, 20, 30, 40, 50]\nx[1..=3]") {
            let vals: Vec<i64> = items.iter().map(|v| if let Value::Int(n) = v { *n } else { panic!() }).collect();
            assert_eq!(vals, vec![20, 30, 40]);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_string_slice() {
        if let Value::String(s) = eval_val(r#"lt s = "hello"
s[1..4]"#) {
            assert_eq!(s, "ell");
        } else {
            panic!("Expected String");
        }
    }

    #[test]
    fn test_negative_index_list() {
        assert!(matches!(eval_val("lt x = [10, 20, 30]\nx[-1]"), Value::Int(30)));
        assert!(matches!(eval_val("lt x = [10, 20, 30]\nx[-2]"), Value::Int(20)));
    }

    #[test]
    fn test_negative_index_string() {
        assert!(matches!(eval_val(r#"lt s = "abc"
s[-1]"#), Value::Char('c')));
    }

    // --- Map iteration ---

    #[test]
    fn test_map_iteration_with_index() {
        // fo k, v in map — k is key, v is value
        eval("lt m = {\"a\": 1, \"b\": 2}\nfo k, v in m {\n  print(k)\n}");
        // Just ensure it doesn't error; output tested by example files
    }

    #[test]
    fn test_map_iteration_single_binding() {
        // fo entry in map — entry is (key, value) tuple
        if let Some(Value::Tuple(pair)) = eval("lt m = {\"x\": 42}\nfo entry in m {\n  entry\n}") {
            assert_eq!(pair.len(), 2);
        }
    }

    // --- Collection convenience methods ---

    #[test]
    fn test_list_first_last() {
        assert!(matches!(eval_val("[10, 20, 30].first()"), Value::Int(10)));
        assert!(matches!(eval_val("[10, 20, 30].last()"), Value::Int(30)));
        assert!(eval("[].first()").is_none()); // nil shows as None in REPL
    }

    #[test]
    fn test_list_take_skip() {
        if let Value::List(items) = eval_val("[1, 2, 3, 4, 5].take(3)") {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[2], Value::Int(3)));
        } else { panic!("Expected List"); }
        if let Value::List(items) = eval_val("[1, 2, 3, 4, 5].skip(2)") {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0], Value::Int(3)));
        } else { panic!("Expected List"); }
    }

    #[test]
    fn test_list_flatten() {
        if let Value::List(items) = eval_val("[[1, 2], [3, 4], [5]].flatten()") {
            assert_eq!(items.len(), 5);
        } else { panic!("Expected List"); }
    }

    #[test]
    fn test_list_sum_min_max() {
        assert!(matches!(eval_val("[1, 2, 3, 4, 5].sum()"), Value::Int(15)));
        assert!(matches!(eval_val("[3, 1, 4, 1, 5].min()"), Value::Int(1)));
        assert!(matches!(eval_val("[3, 1, 4, 1, 5].max()"), Value::Int(5)));
    }

    #[test]
    fn test_range_single_arg() {
        // range(n) should produce [0, 1, ..., n-1]
        if let Value::List(items) = eval_val("range(5)") {
            assert_eq!(items.len(), 5);
            assert!(matches!(items[0], Value::Int(0)));
            assert!(matches!(items[4], Value::Int(4)));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_string_index_of() {
        assert!(matches!(eval_val(r#""hello world".index_of("world")"#), Value::Int(6)));
        assert!(matches!(eval_val(r#""hello".index_of("xyz")"#), Value::Int(-1)));
    }

    #[test]
    fn test_string_rev() {
        if let Value::String(s) = eval_val(r#""abcde".rev()"#) {
            assert_eq!(s, "edcba");
        } else { panic!("Expected String"); }
    }

    #[test]
    fn test_map_get() {
        assert!(matches!(
            eval_val("{\"a\": 1}.get(\"a\")"),
            Value::Int(1)
        ));
        assert!(matches!(
            eval_val("{\"a\": 1}.get(\"z\", 99)"),
            Value::Int(99)
        ));
        // get with missing key and no default returns nil
        assert!(eval("{\"a\": 1}.get(\"z\")").is_none() ||
            matches!(eval("{\"a\": 1}.get(\"z\")"), Some(Value::Nil)));
    }

    // --- Destructuring ---

    #[test]
    fn test_tuple_destructure() {
        assert!(matches!(
            eval_val("lt (a, b) = (10, 20)\na + b"),
            Value::Int(30)
        ));
    }

    #[test]
    fn test_list_destructure() {
        assert!(matches!(
            eval_val("lt (x, y, z) = [1, 2, 3]\nx + y + z"),
            Value::Int(6)
        ));
    }

    #[test]
    fn test_destructure_from_function() {
        assert!(matches!(
            eval_val("fn pair() = (42, 99)\nlt (a, b) = pair()\nb - a"),
            Value::Int(57)
        ));
    }

    // --- Index assignment ---

    #[test]
    fn test_list_index_assign() {
        if let Value::List(items) = eval_val("mt a = [1, 2, 3]\na[1] = 99\na") {
            assert!(matches!(items[1], Value::Int(99)));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_list_index_assign_negative() {
        if let Value::List(items) = eval_val("mt a = [1, 2, 3]\na[-1] = 42\na") {
            assert!(matches!(items[2], Value::Int(42)));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_map_index_assign() {
        // Update existing key
        assert!(matches!(
            eval_val("mt m = {\"x\": 1}\nm[\"x\"] = 99\nm[\"x\"]"),
            Value::Int(99)
        ));
    }

    #[test]
    fn test_map_index_insert() {
        // Insert new key
        assert!(matches!(
            eval_val("mt m = {\"x\": 1}\nm[\"y\"] = 42\nm[\"y\"]"),
            Value::Int(42)
        ));
    }

    #[test]
    fn test_list_compound_index_assign() {
        assert!(matches!(
            eval_val("mt a = [10, 20, 30]\na[1] += 5\na[1]"),
            Value::Int(25)
        ));
    }

    // --- Lambda block body ---

    #[test]
    fn test_lambda_block_body_with_let() {
        // Lambda with block body containing let bindings
        assert!(matches!(
            eval_val("lt f = |x| {\n  lt y = x * 2\n  y + 1\n}\nf(5)"),
            Value::Int(11)
        ));
    }

    #[test]
    fn test_lambda_block_body_side_effects() {
        // Lambda block with multiple statements
        eval("[1, 2, 3].each(|x| {\n  lt msg = to_string(x)\n  print(msg)\n})");
    }

    #[test]
    fn test_mixed_type_comparison() {
        assert!(matches!(eval_val("3.14 > 3"), Value::Bool(true)));
        assert!(matches!(eval_val("2 < 2.5"), Value::Bool(true)));
        assert!(matches!(eval_val("5.0 >= 5"), Value::Bool(true)));
        assert!(matches!(eval_val("3 <= 3.0"), Value::Bool(true)));
    }

    #[test]
    fn test_nested_string_in_interpolation() {
        if let Value::String(s) = eval_val(r#"lt m = {"k": 42}
"val={m["k"]}""#) {
            assert_eq!(s, "val=42");
        } else {
            panic!("Expected String");
        }
    }

    #[test]
    fn test_closure_captures_mutability() {
        // Closure should be able to mutate captured mt vars within a single call
        assert!(matches!(
            eval_val("mt x = 10\nlt f = |n| { x += n\n x }\nf(5)"),
            Value::Int(15)
        ));
    }

    // --- Short-circuit evaluation ---

    #[test]
    fn test_and_short_circuit() {
        // Should not evaluate right side if left is false
        // This would crash without short-circuit (index out of bounds)
        assert!(matches!(
            eval_val("lt a = [1, 2]\nff and a[99] > 0"),
            Value::Bool(false)
        ));
    }

    #[test]
    fn test_or_short_circuit() {
        assert!(matches!(
            eval_val("tt or (1 / 0 > 0)"),
            Value::Bool(true)
        ));
    }

    // --- Char pattern matching ---

    #[test]
    fn test_char_match_pattern() {
        assert!(matches!(
            eval_val("lt c = '+'\nmc c {\n  '+' => 1\n  '-' => 2\n  _ => 0\n}"),
            Value::Int(1)
        ));
    }

    #[test]
    fn test_char_comparison() {
        assert!(matches!(eval_val("'a' < 'z'"), Value::Bool(true)));
        assert!(matches!(eval_val("'z' > 'a'"), Value::Bool(true)));
        assert!(matches!(eval_val("'m' == 'm'"), Value::Bool(true)));
    }

    // --- String pattern matching ---

    #[test]
    fn test_string_match_pattern() {
        assert!(matches!(
            eval_val(r#"lt s = "hello"
mc s {
  "hello" => 1
  "world" => 2
  _ => 0
}"#),
            Value::Int(1)
        ));
    }

    // --- Collection equality ---

    #[test]
    fn test_list_equality() {
        assert!(matches!(eval_val("[1, 2, 3] == [1, 2, 3]"), Value::Bool(true)));
        assert!(matches!(eval_val("[1, 2] == [1, 3]"), Value::Bool(false)));
        assert!(matches!(eval_val("[] == []"), Value::Bool(true)));
    }

    #[test]
    fn test_tuple_equality() {
        assert!(matches!(eval_val("(1, 2) == (1, 2)"), Value::Bool(true)));
        assert!(matches!(eval_val("(1, 2) == (1, 3)"), Value::Bool(false)));
    }

    #[test]
    fn test_nested_list_equality() {
        assert!(matches!(
            eval_val("[[1, 2], [3]] == [[1, 2], [3]]"),
            Value::Bool(true)
        ));
    }

    #[test]
    fn test_list_unique() {
        if let Value::List(items) = eval_val("[1, 2, 2, 3, 3, 1].unique()") {
            assert_eq!(items.len(), 3);
        } else { panic!("Expected List"); }
    }

    #[test]
    fn test_list_count() {
        assert!(matches!(
            eval_val("[1, 2, 3, 4, 5, 6].count(|x| x % 2 == 0)"),
            Value::Int(3)
        ));
    }

    #[test]
    fn test_list_reduce() {
        assert!(matches!(
            eval_val("[1, 2, 3, 4, 5].reduce(|a, b| a * b)"),
            Value::Int(120)
        ));
    }

    #[test]
    fn test_map_equality() {
        assert!(matches!(
            eval_val("{\"a\": 1, \"b\": 2} == {\"a\": 1, \"b\": 2}"),
            Value::Bool(true)
        ));
        assert!(matches!(
            eval_val("{\"a\": 1} == {\"a\": 2}"),
            Value::Bool(false)
        ));
    }

    // --- Arity checking ---

    #[test]
    fn test_too_many_args_error() {
        let err = eval_err("fn foo(x) = x\nfoo(1, 2, 3)");
        assert!(err.contains("Expected 1 argument"));
    }

    #[test]
    fn test_too_few_args_error() {
        let err = eval_err("fn add(a, b) = a + b\nadd(1)");
        assert!(err.contains("Missing argument"));
    }

    #[test]
    fn test_default_param_not_required() {
        assert!(matches!(
            eval_val("fn f(x, y = 10) = x + y\nf(5)"),
            Value::Int(15)
        ));
    }

    // --- Try/catch ---

    #[test]
    fn test_try_catch_catches_error() {
        if let Value::String(s) = eval_val("try {\n  1 / 0\n} ch e {\n  e\n}") {
            assert!(s.contains("Division by zero"));
        } else {
            panic!("Expected error string");
        }
    }

    #[test]
    fn test_try_catch_no_error() {
        assert!(matches!(
            eval_val("try { 42 } ch e { -1 }"),
            Value::Int(42)
        ));
    }

    #[test]
    fn test_try_catch_nested() {
        // Inner catch handles the error, outer continues
        eval("try {\n  try { 1 / 0 } ch e { print(e) }\n  print(\"ok\")\n} ch e { print(\"outer\") }");
    }

    // --- First-class functions ---

    #[test]
    fn test_fn_as_value() {
        assert!(matches!(
            eval_val("fn double(x) = x * 2\n[1, 2, 3].map(double)"),
            Value::List(_)
        ));
    }
}
