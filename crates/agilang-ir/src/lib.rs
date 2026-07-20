use agilang_source::Span;
use agilang_symbols::Symbol;
use agilang_types::Type;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HirProgram {
    pub functions: Vec<HirFunction>,
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
    Return {
        value: Option<HirExpr>,
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
            Self::Call { span, .. } => *span,
            Self::Binary { span, .. } => *span,
        }
    }
}
