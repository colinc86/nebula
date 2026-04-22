use crate::lexer::StringLiteral;

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Let {
        name: String,
        type_ann: Option<TypeExpr>,
        value: Expr,
    },
    Mut {
        name: String,
        type_ann: Option<TypeExpr>,
        value: Expr,
    },
    Const {
        name: String,
        type_ann: Option<TypeExpr>,
        value: Expr,
    },
    FnDecl {
        name: String,
        params: Vec<Param>,
        return_type: Option<TypeExpr>,
        body: Block,
        is_pub: bool,
        is_test: bool,
        decorators: Vec<(String, Vec<Expr>)>,
    },
    StructDecl {
        name: String,
        generics: Vec<String>,
        fields: Vec<Field>,
        is_pub: bool,
        extends: Option<String>,
    },
    EnumDecl {
        name: String,
        generics: Vec<String>,
        variants: Vec<Variant>,
        is_pub: bool,
    },
    TraitDecl {
        name: String,
        generics: Vec<String>,
        methods: Vec<TraitMethod>,
        is_pub: bool,
    },
    ImplBlock {
        trait_name: Option<String>,
        target: String,
        methods: Vec<Statement>,
    },
    Expr(Expr),
    Return(Option<Expr>),
    Break(Option<Expr>),
    Continue,
    Throw(Option<Expr>),
    Assignment {
        target: Expr,
        op: AssignOp,
        value: Expr,
    },
    LetDestructure {
        names: Vec<String>,
        value: Expr,
        mutable: bool,
    },
    TypeAlias {
        name: String,
        type_expr: TypeExpr,
    },
    Use {
        path: Vec<String>,
        items: Option<Vec<String>>,
        alias: Option<String>,
    },
    Module {
        path: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub type_ann: Option<TypeExpr>,
    pub default: Option<Expr>,
    pub is_variadic: bool,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub type_ann: TypeExpr,
    pub is_pub: bool,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<VariantField>,
}

#[derive(Debug, Clone)]
pub struct VariantField {
    pub name: Option<String>,
    pub type_ann: TypeExpr,
}

#[derive(Debug, Clone)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Option<Block>,
}

#[derive(Debug, Clone)]
pub enum TypeExpr {
    Named(String),
    Generic(String, Vec<TypeExpr>),
    Function(Vec<TypeExpr>, Box<TypeExpr>),
    Tuple(Vec<TypeExpr>),
    Option(Box<TypeExpr>),
}

#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub expr: Option<Box<Expr>>,
}

#[derive(Debug, Clone)]
pub enum Expr {
    IntLit(i64),
    FloatLit(f64),
    BoolLit(bool),
    StringLit(StringLiteral),
    CharLit(char),
    Nil,

    Ident(String),

    BinaryOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
    },
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<CallArg>,
    },
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    /// Slice expression `xs[start:end]`. Either bound may be omitted
    /// (`xs[:end]`, `xs[start:]`, `xs[:]`). Codegen emits the closest native
    /// form per target: Python/Go use `xs[a:b]` directly; JS/TS use
    /// `.slice(a, b)`; Swift uses `xs[a..<b]`; Rust uses `&xs[a..b]`; Kotlin
    /// uses `xs.subList(a, b)`; C has no clean form.
    Slice {
        object: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Lambda {
        params: Vec<Param>,
        body: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then_block: Block,
        else_block: Option<Block>,
    },
    Match {
        subject: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Block(Block),
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    Tuple(Vec<Expr>),
    StructLit {
        name: String,
        fields: Vec<(String, Expr)>,
        spread: Option<Box<Expr>>,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
    },
    Pipe {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Try(Box<Expr>),
    NilCoalesce {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    For {
        binding: String,
        index_binding: Option<String>,
        iterable: Box<Expr>,
        body: Block,
    },
    While {
        condition: Box<Expr>,
        body: Block,
    },
    Loop {
        body: Block,
    },
    TryCatch {
        try_block: Block,
        error_binding: String,
        catch_block: Block,
    },
}

#[derive(Debug, Clone)]
pub struct CallArg {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard,
    Literal(Expr),
    Binding(String),
    Variant(String, Vec<Pattern>),
    Range(Box<Expr>, Box<Expr>, bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    NotEq,
    Less,
    Greater,
    LessEq,
    GreaterEq,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}
