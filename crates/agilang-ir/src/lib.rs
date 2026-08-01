use agilang_source::Span;
use agilang_symbols::Symbol;
use agilang_types::Type;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirProgram {
    pub structs: Vec<HirStruct>,
    pub functions: Vec<HirFunction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirStruct {
    pub name: String,
    pub fields: Vec<HirStructField>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirStructField {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<HirParameter>,
    pub return_type: Type,
    pub body: Vec<HirStmt>,
    pub local_symbols: Vec<Symbol>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirParameter {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HirStmt {
    Let {
        name: String,
        ty: Type,
        value: HirExpr,
        mutable: bool,
        span: Span,
    },
    Assign {
        target: HirExpr,
        value: HirExpr,
        span: Span,
    },
    Return {
        value: Option<HirExpr>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    If {
        condition: HirExpr,
        then_body: Vec<HirStmt>,
        else_body: Vec<HirStmt>,
        span: Span,
    },
    While {
        condition: HirExpr,
        body: Vec<HirStmt>,
        span: Span,
    },
    ForIn {
        key_name: String,
        value_name: Option<String>,
        iterable: HirExpr,
        body: Vec<HirStmt>,
        span: Span,
    },
    Expr(HirExpr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HirBinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HirExpr {
    Identifier(String, Type, Span),
    Integer(i64, Type, Span),
    Float(f64, Type, Span),
    String(String, Type, Span),
    Bool(bool, Type, Span),
    ListLiteral(Vec<HirExpr>, Type, Span),
    ObjectLiteral(Vec<(String, HirExpr)>, Type, Span),
    MemberAccess {
        object: Box<HirExpr>,
        member: String,
        ty: Type,
        span: Span,
    },
    Index {
        object: Box<HirExpr>,
        index: Box<HirExpr>,
        ty: Type,
        span: Span,
    },
    Call {
        callee: Box<HirExpr>,
        args: Vec<HirExpr>,
        ty: Type,
        span: Span,
    },
    Binary {
        left: Box<HirExpr>,
        op: HirBinaryOp,
        right: Box<HirExpr>,
        ty: Type,
        span: Span,
    },
}

impl HirExpr {
    pub fn ty(&self) -> &Type {
        match self {
            Self::Identifier(_, ty, _) => ty,
            Self::Integer(_, ty, _) => ty,
            Self::Float(_, ty, _) => ty,
            Self::String(_, ty, _) => ty,
            Self::Bool(_, ty, _) => ty,
            Self::ListLiteral(_, ty, _) => ty,
            Self::ObjectLiteral(_, ty, _) => ty,
            Self::MemberAccess { ty, .. } => ty,
            Self::Index { ty, .. } => ty,
            Self::Call { ty, .. } => ty,
            Self::Binary { ty, .. } => ty,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Self::Identifier(_, _, span) => *span,
            Self::Integer(_, _, span) => *span,
            Self::Float(_, _, span) => *span,
            Self::String(_, _, span) => *span,
            Self::Bool(_, _, span) => *span,
            Self::ListLiteral(_, _, span) => *span,
            Self::ObjectLiteral(_, _, span) => *span,
            Self::MemberAccess { span, .. } => *span,
            Self::Index { span, .. } => *span,
            Self::Call { span, .. } => *span,
            Self::Binary { span, .. } => *span,
        }
    }
}
