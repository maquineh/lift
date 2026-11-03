//! Untyped Lift syntax-tree data structures.
//!
//! This crate stores syntax and half-open UTF-8 byte spans; it performs no
//! parsing or semantic analysis.
//!
//! PT-BR: Tipos de dados da AST não tipada do Lift. Este crate não faz parsing
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
    /// Enum nominal com variantes posicionais e payloads owned.
    Enum(EnumDeclaration),
    /// `type Nome = A | B;`: união nominal desaçucarada para enum.
    Union(UnionDeclaration),
    Function(FunctionDeclaration),
    Import(ImportFunctionDeclaration),
    /// Interface externa mapeada para um tipo Rust declarado em outra crate.
    /// PT-BR: interface externa mapeada para um tipo Rust de outra crate.
    ImportInterface(ImportInterfaceDeclaration),
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
            Self::Enum(node) => node.span,
            Self::Union(node) => node.span,
            Self::Function(node) => node.span,
            Self::Import(node) => node.span,
            Self::ImportInterface(node) => node.span,
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
    /// Parâmetros de tipo de um impl genérico (`impl Box<T>`).
    pub type_parameters: Vec<Identifier>,
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

/// Interface externa: layout declarado em Lift para um tipo Rust de outra crate.
/// Os campos precisam existir no tipo Rust, ser públicos e ter exatamente os
/// tipos mapeados pelo contrato de FFI.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportInterfaceDeclaration {
    pub name: Identifier,
    /// Parâmetros de tipo do layout externo genérico (`import interface Box<T>`).
    pub type_parameters: Vec<Identifier>,
    /// Argumentos de tipo concretos da instância sintética (`Box<int>`).
    pub type_arguments: Vec<TypeExpression>,
    pub fields: Vec<InterfaceField>,
    pub source: String,
    pub source_span: Span,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnionDeclaration {
    pub name: Identifier,
    pub members: Vec<TypeExpression>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDeclaration {
    pub name: Identifier,
    /// Parâmetros de tipo de um enum genérico (`enum Option<T>`).
    pub type_parameters: Vec<Identifier>,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumVariant {
    pub name: Identifier,
    pub fields: Vec<TypeExpression>,
    /// Payload com campos nomeados; mutuamente exclusivo com `fields`.
    pub named_fields: Vec<EnumNamedField>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumNamedField {
    pub name: Identifier,
    pub ty: TypeExpression,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InterfaceDeclaration {
    pub name: Identifier,
    /// Parâmetros de tipo de um container genérico (`interface Box<T>`).
    pub type_parameters: Vec<Identifier>,
    pub fields: Vec<InterfaceField>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InterfaceField {
    pub name: Identifier,
    /// Campo declarado com `?` após o nome (`name?: T`): pode ser omitido em
    /// um object literal e vira `None`. PT-BR: campo opcional omitível.
    pub optional: bool,
    pub ty: TypeExpression,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDeclaration {
    pub name: Identifier,
    /// Parâmetros de tipo genéricos (`function f<T>(...)`).
    pub type_parameters: Vec<Identifier>,
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
    /// Aplicação de tipo genérico (`Box<int>`, `Map<string, int>`).
    Generic {
        name: Identifier,
        arguments: Vec<TypeExpression>,
    },
    Array(Box<TypeExpression>),
    /// Builtin Result type; PT-BR: tipo Result embutido, sem generics gerais.
    Result(Box<TypeExpression>, Box<TypeExpression>),
    Optional(Box<TypeExpression>),
    /// Tipo de função `(T, U) => R`, usado por callbacks e valores de função.
    Function(Vec<TypeExpression>, Box<TypeExpression>),
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
    /// `match valor { some(x) => {...} none => {...} }` para Option/Result.
    Match(MatchStatement),
    /// `for (const item of values) { ... }`; itera uma cópia owned do array.
    ForOf(ForOfStatement),
    Break(BreakStatement),
    Continue(ContinueStatement),
    Return(ReturnStatement),
    /// `throw expressão;`: produz `Err` para o `catch` mais próximo ou para o
    /// retorno `Result<_, E>` da função.
    Throw(ThrowStatement),
    /// `try { ... } catch (erro: E) { ... }`.
    TryCatch(TryCatchStatement),
    Expression(ExpressionStatement),
}

impl Statement {
    pub fn span(&self) -> Span {
        match self {
            Self::Variable(node) => node.span,
            Self::Assignment(node) => node.span,
            Self::If(node) => node.span,
            Self::While(node) => node.span,
            Self::Match(node) => node.span,
            Self::ForOf(node) => node.span,
            Self::Break(node) => node.span,
            Self::Continue(node) => node.span,
            Self::Return(node) => node.span,
            Self::Throw(node) => node.span,
            Self::TryCatch(node) => node.span,
            Self::Expression(node) => node.span,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThrowStatement {
    pub value: Expression,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ForOfStatement {
    pub binding: BindingKind,
    pub name: Identifier,
    pub iterable: Expression,
    pub body: Block,
    pub span: Span,
}

/// `match` de Option/Result com braços exaustivos.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchStatement {
    pub value: Expression,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchPattern {
    pub kind: MatchPatternKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MatchPatternKind {
    /// `some(nome)`.
    Some(Identifier),
    /// `none`.
    None,
    /// `ok(nome)` ou `ok()` quando o sucesso é `void`.
    Ok(Option<Identifier>),
    /// `err(nome)`.
    Err(Identifier),
    /// `Enum.Variant` ou `Enum.Variant(nome, ...)`.
    EnumVariant {
        enum_name: Identifier,
        variant: Identifier,
        bindings: Vec<Identifier>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct BreakStatement {
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContinueStatement {
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TryCatchStatement {
    pub body: Block,
    pub catch_name: Identifier,
    pub catch_type: TypeExpression,
    pub catch_body: Block,
    pub span: Span,
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
    /// Operador `?`: propaga `Err` para o `try` mais próximo ou para o retorno
    /// `Result<_, E>` da função, produzindo o payload de sucesso.
    TryPropagate(Box<Expression>),
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
    /// `f::<T>(...)`: argumentos de tipo explícitos apenas no callee.
    TypeApplication {
        callee: Box<Expression>,
        arguments: Vec<TypeExpression>,
        span: Span,
    },
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
