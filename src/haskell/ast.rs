//! Structured Haskell AST — never build Haskell by string concatenation in the pipeline.

use indexmap::IndexMap;

#[derive(Debug, Clone)]
pub struct HsModule {
    pub name: String,
    pub pragmas: Vec<String>,
    pub exports: Option<Vec<String>>,
    pub imports: Vec<HsImport>,
    pub decls: Vec<HsDecl>,
}

impl HsModule {
    pub fn empty(name: &str) -> Self {
        Self {
            name: name.to_string(),
            pragmas: vec![],
            exports: None,
            imports: vec![],
            decls: vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct HsImport {
    pub module: String,
    pub qualified: bool,
    pub alias: Option<String>,
    pub items: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum HsDecl {
    Data {
        name: String,
        generics: Vec<String>,
        ctors: Vec<HsCtor>,
        deriving: Vec<String>,
    },
    TypeSig {
        name: String,
        /// Optional constraints rendered as `Display a => …`
        constraints: Vec<(String, String)>,
        ty: HsType,
    },
    FunBind {
        name: String,
        equations: Vec<HsEquation>,
    },
    PatBind {
        name: String,
        body: HsExp,
    },
    Class {
        name: String,
        type_var: String,
        methods: Vec<(String, HsType)>,
    },
    Instance {
        class: String,
        ty: HsType,
        methods: Vec<(String, Vec<HsPat>, HsExp)>,
    },
}

#[derive(Debug, Clone)]
pub struct HsCtor {
    pub name: String,
    /// Record fields if Some
    pub record: Option<IndexMap<String, HsType>>,
    pub fields: Vec<HsType>,
}

#[derive(Debug, Clone)]
pub struct HsEquation {
    pub pats: Vec<HsPat>,
    pub body: HsExp,
    pub guards: Vec<(HsExp, HsExp)>,
}

#[derive(Debug, Clone)]
pub enum HsType {
    Var(String),
    Con(String),
    App(Box<HsType>, Box<HsType>),
    Fun(Box<HsType>, Box<HsType>),
    Tuple(Vec<HsType>),
    List(Box<HsType>),
    Paren(Box<HsType>),
    /// `Ctx => ty` — stored on TypeSig preferably; also allowed inline.
    Constrained(Vec<(String, String)>, Box<HsType>),
}

#[derive(Debug, Clone)]
pub enum HsExp {
    Var(String),
    Con(String),
    Lit(HsLit),
    App(Box<HsExp>, Box<HsExp>),
    Infix(String, Box<HsExp>, Box<HsExp>),
    Lam(Vec<String>, Box<HsExp>),
    Let(Vec<(String, HsExp)>, Box<HsExp>),
    If(Box<HsExp>, Box<HsExp>, Box<HsExp>),
    Case(Box<HsExp>, Vec<(HsPat, HsExp)>),
    Tuple(Vec<HsExp>),
    List(Vec<HsExp>),
    ListComp {
        expr: Box<HsExp>,
        quals: Vec<HsQual>,
    },
    Record {
        name: String,
        fields: IndexMap<String, HsExp>,
    },
    /// `base { field = value, … }`
    RecordUpdate {
        base: Box<HsExp>,
        fields: IndexMap<String, HsExp>,
    },
    Field(Box<HsExp>, String),
    Paren(Box<HsExp>),
    Neg(Box<HsExp>),
    Do(Vec<HsDoStmt>),
}

#[derive(Debug, Clone)]
pub enum HsQual {
    Gen(HsPat, HsExp),
    Filter(HsExp),
}

#[derive(Debug, Clone)]
pub enum HsDoStmt {
    Bind(HsPat, HsExp),
    Let(String, HsExp),
    Exp(HsExp),
}

#[derive(Debug, Clone)]
pub enum HsPat {
    Wildcard,
    Var(String),
    Lit(HsLit),
    Con(String, Vec<HsPat>),
    Tuple(Vec<HsPat>),
    List(Vec<HsPat>),
    Record(String, IndexMap<String, HsPat>),
    As(String, Box<HsPat>),
    Paren(Box<HsPat>),
}

#[derive(Debug, Clone)]
pub enum HsLit {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Char(char),
}
