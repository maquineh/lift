//! Untyped Truxt syntax-tree data structures.
//!
//! This crate stores syntax and half-open UTF-8 byte spans; it performs no
//! parsing or semantic analysis.
//!
//! PT-BR: Tipos de dados da AST não tipada do Truxt. Este crate não faz parsing
//! nem análise semântica. Todo nó sintático mantém
//! um [`Span`] com offsets de bytes semiabertos na fonte original.

mod span;

pub use span::{SourceId, Span};

/// Programa fonte após parsing.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgramAst {
    pub declarations: Vec<Declaration>,
    pub span: Span,
}

impl ProgramAst {
    pub fn new(declarations: Vec<Declaration>, span: Span) -> Self {
        Self { declarations, span }
    }
}

/// Declaração de topo. A expressão discriminante não armazena span próprio;
/// cada nó concreto contém o seu.
#[derive(Clone, Debug, PartialEq)]
pub enum Declaration {
    Impl(ImplDeclaration),
    Interface(InterfaceDeclaration),
    Function(FunctionDeclaration),
    Import(ImportFunctionDeclaration),
    ModuleImport(ModuleImportDeclaration),
    PackageImport(PackageImportDeclaration),
    ExportFunction(FunctionDeclaration),
    Statement(Statement),
}

impl Declaration {
    pub fn span(&self) -> Span {
        match self {
            Self::Impl(node) => node.span,
            Self::Interface(node) => node.span,
            Self::Function(node) => node.span,
            Self::Import(node) => node.span,
            Self::ModuleImport(node) => node.span,
            Self::PackageImport(node) => node.span,
            Self::ExportFunction(node) => node.span,
            Self::Statement(node) => node.span(),
        }
    }
}

/// Package identity within one contextual analysis, assigned by the resolver.
/// PT-BR: Identidade de pacote numa análise contextual, atribuída pelo resolvedor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageId(pub usize);

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectAst {
    pub root: PackageId,
    pub packages: Vec<PackageAst>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackageAst {
    pub id: PackageId,
    pub program: ProgramAst,
    pub is_library: bool,
    pub imports: Vec<PackageBinding>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackageBinding {
    pub alias: Identifier,
    pub target: PackageId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackageImportDeclaration {
    pub alias: Identifier,
    pub dependency: String,
    pub dependency_span: Span,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImplDeclaration {
    pub target: Identifier,
    pub methods: Vec<FunctionDeclaration>,
    pub span: Span,
}

/// Relative source import; PT-BR: import de fonte relativo, resolvido pela CLI.
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleImportDeclaration {
    pub path: String,
    pub path_span: Span,
    pub span: Span,
}

/// Assinatura explícita de uma função Rust externa; o destino ainda é texto fonte.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportFunctionDeclaration {
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeExpression,
    pub source: String,
    pub source_span: Span,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InterfaceDeclaration {
    pub name: Identifier,
    pub fields: Vec<InterfaceField>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InterfaceField {
    pub name: Identifier,
    pub ty: TypeExpression,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDeclaration {
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeExpression>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: Identifier,
    pub ty: TypeExpression,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Identifier {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeExpression {
    pub kind: TypeExpressionKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeExpressionKind {
    Named(Identifier),
    Array(Box<TypeExpression>),
    /// Builtin Result type; PT-BR: tipo Result embutido, sem generics gerais.
    Result(Box<TypeExpression>, Box<TypeExpression>),
    Optional(Box<TypeExpression>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Variable(VariableDeclaration),
    Assignment(AssignmentStatement),
    If(IfStatement),
    While(WhileStatement),
    Return(ReturnStatement),
    Expression(ExpressionStatement),
}

impl Statement {
    pub fn span(&self) -> Span {
        match self {
            Self::Variable(node) => node.span,
            Self::Assignment(node) => node.span,
            Self::If(node) => node.span,
            Self::While(node) => node.span,
            Self::Return(node) => node.span,
            Self::Expression(node) => node.span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingKind {
    Const,
    Let,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariableDeclaration {
    pub binding: BindingKind,
    pub name: Identifier,
    pub annotation: Option<TypeExpression>,
    pub initializer: Expression,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssignmentStatement {
    pub target: Expression,
    pub value: Expression,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IfStatement {
    pub condition: Expression,
    pub then_branch: Block,
    pub else_branch: Option<ElseBranch>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ElseBranch {
    pub kind: ElseBranchKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ElseBranchKind {
    Block(Block),
    ElseIf(Box<IfStatement>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct WhileStatement {
    pub condition: Expression,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReturnStatement {
    pub value: Option<Expression>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExpressionStatement {
    pub expression: Expression,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExpressionKind {
    Closure(ClosureExpression),
    Template(Vec<Expression>),
    /// Magnitude não negativa. A semântica valida o intervalo `i64`, levando
    /// em conta um operador unário de negação que possa precedê-la.
    IntegerLiteral(u64),
    NumberLiteral(f64),
    /// Conteúdo da string sem aspas; escapes continuam no formato-fonte.
    StringLiteral(String),
    BooleanLiteral(bool),
    NullLiteral,
    Identifier(Identifier),
    ObjectLiteral(ObjectLiteral),
    ArrayLiteral(ArrayLiteral),
    Unary(UnaryExpression),
    Binary(BinaryExpression),
    FieldAccess(FieldAccessExpression),
    Call(CallExpression),
    Index(IndexExpression),
    Grouping(Box<Expression>),
}

/// Explicitly typed arrow; PT-BR: arrow com assinatura explícita.
#[derive(Clone, Debug, PartialEq)]
pub struct ClosureExpression {
    pub parameters: Vec<Parameter>,
    pub return_type: TypeExpression,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectLiteral {
    pub fields: Vec<ObjectField>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectField {
    pub name: Identifier,
    pub value: Expression,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayLiteral {
    pub elements: Vec<Expression>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnaryExpression {
    pub operator: UnaryOperator,
    pub operand: Box<Expression>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOperator {
    Negate,
    Not,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BinaryExpression {
    pub left: Box<Expression>,
    pub operator: BinaryOperator,
    pub right: Box<Expression>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOperator {
    Coalesce,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldAccessExpression {
    pub object: Box<Expression>,
    pub field: Identifier,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CallExpression {
    pub callee: Box<Expression>,
    pub arguments: Vec<Expression>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndexExpression {
    pub object: Box<Expression>,
    pub index: Box<Expression>,
    pub span: Span,
}
