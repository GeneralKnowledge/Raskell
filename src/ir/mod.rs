//! Translation IR — captures program *semantics*, not Rust syntax.
//!
//! Multiple Rust constructs can lower into the same IR nodes so the Haskell
//! backend can optimise them consistently (e.g. `for`+`push` and `.map()`).

use crate::ast::Span;
use indexmap::IndexMap;

/// A complete IR module.
#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub decls: Vec<Decl>,
    pub explanations: Vec<TransNote>,
}

/// A note about a translation decision (used by `raskell explain`).
#[derive(Debug, Clone)]
pub struct TransNote {
    pub function: String,
    pub detected: Vec<String>,
    pub translation: Vec<String>,
    pub generated_summary: String,
}

#[derive(Debug, Clone)]
pub enum Decl {
    Func(Func),
    Data(DataType),
    Const { name: String, ty: Ty, value: Exp },
}

#[derive(Debug, Clone)]
pub struct Func {
    pub name: String,
    pub params: Vec<(String, Ty)>,
    pub return_ty: Ty,
    pub body: Exp,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DataType {
    pub name: String,
    pub generics: Vec<String>,
    pub constructors: Vec<Constructor>,
    /// If true, emit as Haskell record.
    pub is_record: bool,
}

#[derive(Debug, Clone)]
pub struct Constructor {
    pub name: String,
    pub fields: Vec<DataField>,
}

#[derive(Debug, Clone)]
pub struct DataField {
    pub name: Option<String>,
    pub ty: Ty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Unit,
    Bool,
    Int,
    Integer, // arbitrary precision if needed
    Word32,
    Word64,
    Double,
    Float,
    Char,
    String,
    List(Box<Ty>),
    Maybe(Box<Ty>),
    Either(Box<Ty>, Box<Ty>), // Either e a  (note: error first, like Haskell)
    Tuple(Vec<Ty>),
    Named(String, Vec<Ty>),
    Fun(Vec<Ty>, Box<Ty>),
    Var(String),
}

#[derive(Debug, Clone)]
pub enum Exp {
    Lit(Lit),
    Var(String),
    App(Box<Exp>, Vec<Exp>),
    Lam(Vec<String>, Box<Exp>),
    Let(Vec<Binding>, Box<Exp>),
    If(Box<Exp>, Box<Exp>, Box<Exp>),
    Case(Box<Exp>, Vec<Arm>),
    Tuple(Vec<Exp>),
    List(Vec<Exp>),
    Record {
        name: String,
        fields: IndexMap<String, Exp>,
    },
    Field(Box<Exp>, String),
    BinOp(BinOp, Box<Exp>, Box<Exp>),
    UnOp(UnOp, Box<Exp>),
    /// Idiomatic collection transforms recognised from Rust patterns.
    Map(Box<Exp>, Box<Exp>),       // map f xs
    Filter(Box<Exp>, Box<Exp>),    // filter p xs
    Fold(Box<Exp>, Box<Exp>, Box<Exp>), // foldl f z xs
    Sum(Box<Exp>),
    /// Sequence of statements turned into nested lets / do-notation later.
    Seq(Vec<Exp>),
    Do(Vec<DoStmt>, Box<Exp>),
    Pure(Box<Exp>),
    /// Placeholder rejected earlier — should not reach pretty printer.
    Error(String),
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub name: String,
    pub value: Exp,
}

#[derive(Debug, Clone)]
pub struct Arm {
    pub pattern: Pat,
    pub guard: Option<Exp>,
    pub body: Exp,
}

#[derive(Debug, Clone)]
pub enum Pat {
    Wildcard,
    Lit(Lit),
    Var(String),
    Tuple(Vec<Pat>),
    Constr {
        name: String,
        args: Vec<Pat>,
    },
    Record {
        name: String,
        fields: IndexMap<String, Pat>,
    },
}

#[derive(Debug, Clone)]
pub enum DoStmt {
    Bind { name: String, exp: Exp },
    Exp(Exp),
    Let { name: String, exp: Exp },
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
    Append, // ++
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

impl Module {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            decls: Vec::new(),
            explanations: Vec::new(),
        }
    }
}

/// Helper constructors.
impl Exp {
    pub fn var(name: impl Into<String>) -> Self {
        Exp::Var(name.into())
    }

    pub fn app(f: Exp, args: Vec<Exp>) -> Self {
        Exp::App(Box::new(f), args)
    }

    pub fn int(n: i64) -> Self {
        Exp::Lit(Lit::Int(n))
    }

    pub fn span_dummy() -> Span {
        Span::default()
    }
}
