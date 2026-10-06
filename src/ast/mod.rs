//! Raskell Rust AST — a simplified, analysis-friendly representation of supported Rust.

use std::fmt;

/// Source span for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// A complete Raskell program (one crate / file for now).
#[derive(Debug, Clone)]
pub struct Program {
    pub filename: String,
    pub items: Vec<Item>,
}

/// Top-level item.
#[derive(Debug, Clone)]
pub enum Item {
    Function(Function),
    Struct(StructDef),
    Enum(EnumDef),
    Const(ConstDef),
    Use(UseItem),
    Trait(TraitDef),
    Impl(ImplBlock),
}

#[derive(Debug, Clone)]
pub struct UseItem {
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstDef {
    pub name: String,
    pub ty: Type,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Block,
    pub is_pub: bool,
    /// Type parameter names, e.g. `T`, `E`.
    pub generics: Vec<String>,
    /// Trait bounds: `(type_param, [Trait, ...])`.
    pub bounds: Vec<(String, Vec<String>)>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub is_mut: bool,
    pub by_ref: bool,
    /// True when this is a `self` / `&self` / `&mut self` receiver.
    pub is_self: bool,
    pub span: Span,
}

/// `trait Foo { fn bar(&self) -> …; }`
#[derive(Debug, Clone)]
pub struct TraitDef {
    pub name: String,
    pub generics: Vec<String>,
    pub methods: Vec<TraitMethod>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub generics: Vec<String>,
    pub bounds: Vec<(String, Vec<String>)>,
    pub default_body: Option<Block>,
    pub span: Span,
}

/// `impl Foo { … }` or `impl Trait for Type { … }`
#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub trait_name: Option<String>,
    pub for_type: Type,
    pub methods: Vec<Function>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<Field>,
    pub generics: Vec<String>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<Variant>,
    pub generics: Vec<String>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub name: String,
    pub fields: VariantFields,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum VariantFields {
    Unit,
    Tuple(Vec<Type>),
    Struct(Vec<Field>),
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub expr: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let {
        name: String,
        is_mut: bool,
        ty: Option<Type>,
        value: Option<Expr>,
        span: Span,
    },
    Expr(Expr),
    Return(Option<Expr>, Span),
    Break(Span),
    Continue(Span),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Lit(Lit, Span),
    Path(String, Span),
    Field {
        base: Box<Expr>,
        field: String,
        span: Span,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    MethodCall {
        receiver: Box<Expr>,
        method: String,
        args: Vec<Expr>,
        span: Span,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
        span: Span,
    },
    If {
        cond: Box<Expr>,
        then_branch: Block,
        else_branch: Option<Box<Expr>>,
        span: Span,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    Block(Block),
    Closure {
        params: Vec<String>,
        body: Box<Expr>,
        span: Span,
    },
    Tuple(Vec<Expr>, Span),
    Array(Vec<Expr>, Span),
    Struct {
        name: String,
        fields: Vec<(String, Expr)>,
        span: Span,
    },
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    AssignOp {
        op: BinOp,
        target: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    Reference {
        is_mut: bool,
        expr: Box<Expr>,
        span: Span,
    },
    Deref {
        expr: Box<Expr>,
        span: Span,
    },
    For {
        pat: String,
        iter: Box<Expr>,
        body: Block,
        span: Span,
    },
    While {
        cond: Box<Expr>,
        body: Block,
        span: Span,
    },
    Loop {
        body: Block,
        span: Span,
    },
    Return(Option<Box<Expr>>, Span),
    Cast {
        expr: Box<Expr>,
        ty: Type,
        span: Span,
    },
    Try(Box<Expr>, Span),
    /// Placeholder for unsupported expressions that we still need to reject cleanly.
    Unsupported {
        description: String,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Lit(_, s)
            | Expr::Path(_, s)
            | Expr::Field { span: s, .. }
            | Expr::Index { span: s, .. }
            | Expr::Call { span: s, .. }
            | Expr::MethodCall { span: s, .. }
            | Expr::Binary { span: s, .. }
            | Expr::Unary { span: s, .. }
            | Expr::If { span: s, .. }
            | Expr::Match { span: s, .. }
            | Expr::Closure { span: s, .. }
            | Expr::Tuple(_, s)
            | Expr::Array(_, s)
            | Expr::Struct { span: s, .. }
            | Expr::Assign { span: s, .. }
            | Expr::AssignOp { span: s, .. }
            | Expr::Reference { span: s, .. }
            | Expr::Deref { span: s, .. }
            | Expr::For { span: s, .. }
            | Expr::While { span: s, .. }
            | Expr::Loop { span: s, .. }
            | Expr::Return(_, s)
            | Expr::Cast { span: s, .. }
            | Expr::Try(_, s)
            | Expr::Unsupported { span: s, .. } => *s,
            Expr::Block(b) => b.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard,
    Lit(Lit),
    Ident(String),
    Tuple(Vec<Pattern>),
    Struct {
        name: String,
        fields: Vec<(String, Pattern)>,
    },
    TupleStruct {
        name: String,
        elems: Vec<Pattern>,
    },
    Path(String),
    /// Option::Some / Result::Ok style
    Variant {
        enum_name: Option<String>,
        variant: String,
        elems: Vec<Pattern>,
    },
    Ref {
        is_mut: bool,
        inner: Box<Pattern>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Char(char),
    Unit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// Named type, optionally applied to type arguments (`Foo`, `Foo<T, U>`).
    Named(String, Vec<Type>),
    Path(Vec<String>),
    Ref {
        is_mut: bool,
        inner: Box<Type>,
    },
    Tuple(Vec<Type>),
    Array(Box<Type>),
    Vec(Box<Type>),
    Option(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Fun {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    /// A type parameter such as `T`.
    Generic(String),
    SelfType,
    Infer,
    Unit,
}

impl Type {
    pub fn named(name: &str) -> Self {
        match name {
            "()" => Type::Unit,
            "Self" => Type::SelfType,
            _ => Type::Named(name.to_string(), vec![]),
        }
    }
}

pub mod mod_reexport {
    pub use super::*;
}
