//! Lift semantic analysis and the validated typed HIR contract.
//!
//! Resolves names and types, validates statements, and records parameter modes,
//! argument evaluation, and cloning decisions for code generation.
//!
//! PT-BR: Análise semântica da AST e contrato do HIR tipado validado do Lift.
//! Resolve nomes e tipos, valida statements e registra modos de passagem,
//! avaliação de argumentos e decisões de clonagem necessárias ao codegen.

mod analyzer;
mod interop;
mod liveness;
mod resources;

use std::{error::Error, fmt};

use lift_ast::{BinaryOperator, BindingKind, Span, UnaryOperator};

pub use analyzer::analyze;
pub use packages::analyze_project;
mod packages;

/// Tipo Lift resolvido. Referências Rust são representadas separadamente por
/// [`ParameterPassMode`] e [`ArgPassMode`], nunca como tipos de expressão.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResolvedType {
    Closure(Vec<ResolvedType>, Box<ResolvedType>),
    Int,
    Number,
    String,
    Boolean,
    Void,
    /// Owned, cloneable scalar SQL value; only the local database protocol constructs it.
    DbValue,
    /// Adapter-owned handle. It has no fields, Clone, equality, or numeric identity.
    Resource(ResourceKind),
    Struct(String),
    /// Parâmetro de tipo genérico (`T`); sempre instanciado em chamadas.
    TypeParam(String),
    Array(Box<ResolvedType>),
    Optional(Box<ResolvedType>),
    Result(Box<ResolvedType>, Box<ResolvedType>),
}

impl ResolvedType {
    pub fn needs_db_runtime(&self) -> bool {
        match self {
            Self::DbValue | Self::Resource(_) => true,
            Self::Array(t) | Self::Optional(t) => t.needs_db_runtime(),
            Self::Result(a, b) => a.needs_db_runtime() || b.needs_db_runtime(),
            Self::Closure(parameters, result) => {
                parameters.iter().any(Self::needs_db_runtime) || result.needs_db_runtime()
            }
            _ => false,
        }
    }
    pub fn contains_resource(&self) -> bool {
        match self {
            Self::Resource(_) => true,
            Self::Array(t) | Self::Optional(t) => t.contains_resource(),
            Self::Result(a, b) => a.contains_resource() || b.contains_resource(),
            Self::Closure(p, r) => p.iter().any(Self::contains_resource) || r.contains_resource(),
            _ => false,
        }
    }
    /// Valor de função boxed (`Box<dyn Fn>`): não é `Copy` nem `Clone`.
    /// PT-BR: valor de função boxed; move-only.
    pub fn contains_closure(&self) -> bool {
        match self {
            Self::Closure(..) => true,
            Self::Array(t) | Self::Optional(t) => t.contains_closure(),
            Self::Result(a, b) => a.contains_closure() || b.contains_closure(),
            _ => false,
        }
    }
    /// Recursos opacos exigem move; callbacks são `Rc` e seguem a cópia Lift.
    /// PT-BR: apenas recursos são afins; callbacks usam Rc clonável.
    pub fn is_affine(&self) -> bool {
        self.contains_resource()
    }
    pub const fn is_copy(&self) -> bool {
        matches!(self, Self::Int | Self::Number | Self::Boolean)
    }

    pub fn is_value_type(&self) -> bool {
        !matches!(self, Self::Void | Self::Closure(..))
            && match self {
                Self::Array(element) => element.is_value_type(),
                Self::Optional(element) => {
                    element.is_value_type() && !matches!(**element, Self::Optional(_))
                }
                Self::Result(ok, error) => {
                    (**ok == Self::Void || ok.is_value_type()) && error.is_value_type()
                }
                _ => true,
            }
    }
}

impl fmt::Display for ResolvedType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closure(parameters, result) => {
                let parameters = parameters
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(formatter, "({parameters}) => {result}")
            }
            Self::Int => formatter.write_str("int"),
            Self::Number => formatter.write_str("number"),
            Self::String => formatter.write_str("string"),
            Self::Boolean => formatter.write_str("boolean"),
            Self::Void => formatter.write_str("void"),
            Self::DbValue => formatter.write_str("DbValue"),
            Self::Resource(ResourceKind::DbPool) => formatter.write_str("DbPool"),
            Self::Resource(ResourceKind::DbTransaction) => formatter.write_str("DbTransaction"),
            Self::Resource(ResourceKind::DbRows) => formatter.write_str("DbRows"),
            Self::Resource(ResourceKind::SqliteConnection) => {
                formatter.write_str("SqliteConnection")
            }
            Self::Resource(ResourceKind::SqliteTransaction) => {
                formatter.write_str("SqliteTransaction")
            }
            Self::Struct(name) | Self::TypeParam(name) => formatter.write_str(name),
            Self::Array(element) => write!(formatter, "{element}[]"),
            Self::Optional(element) => write!(formatter, "{element}?"),
            Self::Result(ok, error) => write!(formatter, "Result<{ok}, {error}>"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    DbPool,
    DbTransaction,
    DbRows,
    SqliteConnection,
    SqliteTransaction,
}

/// Modo da assinatura inferido para cada parâmetro.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterPassMode {
    /// Valor `Copy`, passado por valor.
    ByValue,
    /// Parâmetro não `Copy` apenas lido (`&T`; string vira `&str` no codegen).
    BorrowImm,
    /// Parâmetro não `Copy` cuja projeção sofre mutação (`&mut T`).
    BorrowMut,
    /// O parâmetro escapa e a função possui seu valor (`T`).
    OwnedMove,
}

/// Como um argumento de chamada deve ser preparado para a assinatura HIR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgPassMode {
    ByValue,
    ByRef,
    ByMutRef,
    ByClone,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirProgram {
    pub declarations: Vec<HirDeclaration>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HirDeclaration {
    Struct(HirStruct),
    /// Enum nominal com variantes posicionais e payloads owned.
    Enum(HirEnum),
    /// Tipo externo validado; o codegen não gera `struct`, só asserções de layout.
    /// PT-BR: tipo externo, sem geração de struct.
    ExternStruct(HirExternStruct),
    Function(HirFunction),
    Import(HirImport),
    Statement(HirStatement),
}

/// Função externa validada com parâmetros scalars por valor e agregados por empréstimo.
#[derive(Clone, Debug, PartialEq)]
pub struct HirImport {
    pub name: String,
    pub parameters: Vec<ResolvedType>,
    pub parameter_modes: Vec<ParameterPassMode>,
    pub return_type: ResolvedType,
    pub dependency: RustDependency,
    /// Caminho ASCII validado, incluindo o nome Rust da crate.
    pub rust_path: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RustDependency {
    LocalCore,
    LocalDb,
    LocalWeb,
    Registry { package: String, version: String },
}

/// Nome reservado de chamada HIR; não é um identificador fonte Lift.
pub const CONSOLE_LOG_BUILTIN: &str = "console.log";
/// Reserved HIR names for Result inspection; PT-BR: nomes reservados de Result.
pub const RESULT_IS_OK_BUILTIN: &str = "result.is_ok";
pub const RESULT_UNWRAP_BUILTIN: &str = "result.unwrap";
pub const RESULT_UNWRAP_ERR_BUILTIN: &str = "result.unwrap_err";
pub const OPTION_IS_SOME_BUILTIN: &str = "option.is_some";
pub const OPTION_UNWRAP_BUILTIN: &str = "option.unwrap";

#[derive(Clone, Debug, PartialEq)]
pub struct HirStruct {
    pub name: String,
    pub fields: Vec<HirStructField>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirEnum {
    pub name: String,
    pub variants: Vec<HirEnumVariant>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirEnumVariant {
    pub name: String,
    pub fields: Vec<ResolvedType>,
    /// Um flag de indireção (`Box`) por campo posicional.
    pub field_boxed: Vec<bool>,
    pub named_fields: Vec<HirEnumNamedField>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirEnumNamedField {
    pub name: String,
    pub ty: ResolvedType,
    /// Campo nomeado exige `Box` para quebrar um ciclo de tamanho.
    pub boxed: bool,
    pub span: Span,
}

/// Declaração de layout de um tipo Rust externo (`import interface ... from`).
/// O compilador assume `Clone + Debug + PartialEq`, campos públicos e nomes de
/// campo idênticos; o Rust gerado contém asserções que o `rustc` verifica.
#[derive(Clone, Debug, PartialEq)]
pub struct HirExternStruct {
    pub name: String,
    pub fields: Vec<HirStructField>,
    pub dependency: RustDependency,
    /// Caminho Rust completo do tipo, já validado.
    pub rust_path: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirStructField {
    pub name: String,
    pub ty: ResolvedType,
    /// Declarado como `name?: T`: o object literal pode omiti-lo (`None`).
    /// PT-BR: campo omitível; o tipo já é `Optional`.
    pub optional: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirFunction {
    pub name: String,
    /// Parâmetros de tipo genéricos declarados na função.
    pub type_parameters: Vec<String>,
    pub parameters: Vec<HirParameter>,
    pub return_type: ResolvedType,
    pub body: HirBlock,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirParameter {
    pub id: BindingId,
    pub name: String,
    pub ty: ResolvedType,
    pub pass_mode: ParameterPassMode,
    /// Emite binding Rust mutável quando a função altera o valor owned.
    pub requires_mut_binding: bool,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BindingId(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub struct HirBlock {
    pub statements: Vec<HirStatement>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HirStatement {
    Variable(HirVariableDeclaration),
    /// Binding derivado somente leitura que materializa o payload de um
    /// opcional estreitado. `id` só existe no bloco em que `Narrow` aparece.
    /// PT-BR: binding derivado de narrowing; lê o payload owned do opcional.
    Narrow(HirNarrowing),
    Assignment(HirAssignment),
    If(HirIf),
    While(HirWhile),
    /// `match` exaustivo de `Option<T>`/`Result<T, E>`.
    Match(HirMatch),
    /// `for (const item of values) { ... }` com cópia owned do array.
    ForOf(HirForOf),
    Break(HirBreak),
    Continue(HirContinue),
    Return(HirReturn),
    /// `throw valor;`: diverge para o `catch` mais próximo ou para o retorno.
    Throw(HirThrow),
    /// `try { ... } catch (erro: E) { ... }`.
    TryCatch(HirTryCatch),
    Expression(HirExpressionStatement),
}

impl HirStatement {
    pub fn span(&self) -> Span {
        match self {
            Self::Variable(node) => node.span,
            Self::Narrow(node) => node.span,
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

/// Identidade de um bloco `try`, usada pelo `break` do Rust gerado.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TryId(pub u32);

/// Identidade de um laço, usada pelos labels `break`/`continue` do Rust gerado.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LoopId(pub u32);

/// Destino de `?`/`throw`: o `try` mais próximo com erro compatível ou o
/// retorno `Result<_, E>` da função corrente.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropagateTarget {
    Function,
    Try(TryId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirThrow {
    pub value: HirExpr,
    pub target: PropagateTarget,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirTryCatch {
    pub label: TryId,
    pub body: HirBlock,
    pub catch_id: BindingId,
    pub catch_name: String,
    pub catch_ty: ResolvedType,
    /// O binding do catch precisa de `mut` no padrão Rust gerado.
    pub catch_mut: bool,
    /// Todas as rotas do try/catch retornam da função; o codegen fecha a
    /// divergência com `unreachable!()` para satisfazer o tipo de retorno.
    pub returns: bool,
    pub catch_body: HirBlock,
    pub span: Span,
}

/// Extração de payload de `T?` provada por um teste de fluxo. O codegen clona
/// o payload; mutações através do binding derivado não são aceitas.
/// PT-BR: payload de `T?` provado pelo fluxo; codegen clona e não aceita mutação.
#[derive(Clone, Debug, PartialEq)]
pub struct HirNarrowing {
    pub id: BindingId,
    pub source: BindingId,
    pub ty: ResolvedType,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirVariableDeclaration {
    pub id: BindingId,
    pub binding: BindingKind,
    pub name: String,
    pub ty: ResolvedType,
    pub initializer: HirExpr,
    /// `const` pode precisar virar `let mut` no Rust para mutar uma projeção;
    /// a semântica Lift continua proibindo rebinding do binding `const`.
    pub requires_mut_binding: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirAssignment {
    pub target: HirExpr,
    pub value: HirExpr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirIf {
    pub condition: HirExpr,
    pub then_branch: HirBlock,
    pub else_branch: Option<HirElseBranch>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HirElseBranch {
    Block(HirBlock),
    ElseIf(Box<HirIf>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirWhile {
    pub label: LoopId,
    pub condition: HirExpr,
    pub body: HirBlock,
    pub span: Span,
}

/// `match` de `Option<T>`/`Result<T, E>` com braços exaustivos.
#[derive(Clone, Debug, PartialEq)]
pub struct HirMatch {
    pub value: HirExpr,
    pub arms: Vec<HirMatchArm>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirMatchArm {
    pub pattern: HirMatchPattern,
    pub body: HirBlock,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HirMatchPattern {
    Some(HirMatchBinding),
    None,
    /// `None` representa `ok()` para sucesso `void`.
    Ok(Option<HirMatchBinding>),
    Err(HirMatchBinding),
    /// `Enum.Variant(binding, ...)`; `named` indica payload por campos.
    EnumVariant {
        enum_name: String,
        variant: String,
        bindings: Vec<HirMatchBinding>,
        named: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirMatchBinding {
    pub id: BindingId,
    pub name: String,
    pub ty: ResolvedType,
    /// Campos do payload foram mutados no braço; o padrão Rust usa `mut`.
    pub requires_mut: bool,
    pub span: Span,
}

/// Iteração `for...of`; o array já foi materializado/clonado pela semântica.
#[derive(Clone, Debug, PartialEq)]
pub struct HirForOf {
    pub label: LoopId,
    pub id: BindingId,
    pub name: String,
    pub binding: BindingKind,
    pub element_ty: ResolvedType,
    pub iterable: HirExpr,
    pub body: HirBlock,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirBreak {
    pub target: LoopId,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirContinue {
    pub target: LoopId,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirReturn {
    pub value: Option<HirExpr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirExpressionStatement {
    pub expression: HirExpr,
    pub discard_mode: ExprDiscardMode,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExprDiscardMode {
    /// O resultado é um valor owned/Copy que pode ser avaliado e descartado.
    Value,
    /// O resultado é um place não `Copy`; avalie sem movê-lo.
    BorrowPlace,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub ty: ResolvedType,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HirExprKind {
    /// Captures are owned snapshots evaluated at construction, outside the body CFG.
    Closure {
        captures: Vec<(BindingId, HirExpr)>,
        parameters: Vec<HirParameter>,
        body: HirBlock,
    },
    /// Referência a uma função nomeada convertida em valor `Box<dyn Fn>`.
    /// PT-BR: função nomeada como valor de função.
    FunctionValue {
        name: String,
        parameters: Vec<(ResolvedType, ParameterPassMode)>,
        return_type: ResolvedType,
    },
    ClosureCall {
        callee: Box<HirExpr>,
        arguments: Vec<HirExpr>,
    },
    /// Métodos higher-order de array, monomorfizados pelo tipo do elemento.
    ArrayMethod {
        method: ArrayMethodKind,
        array: Box<HirExpr>,
        callback: Box<HirExpr>,
        initial: Option<Box<HirExpr>>,
    },
    /// `Enum.Variant(arg, ...)`, `Enum.Variant { campo: valor }` ou unit.
    EnumConstruct {
        enum_name: String,
        variant: String,
        fields: Vec<HirExpr>,
        named_fields: Vec<HirStructInitField>,
    },
    IntegerLiteral(i64),
    NumberLiteral(f64),
    StringLiteral(String),
    BooleanLiteral(bool),
    OptionNone,
    /// Payload is already materialized as owned; PT-BR: payload já é owned.
    OptionSome(Box<HirExpr>),
    /// `result.ok(valor)`; `None` representa `Ok(())` para sucesso `void`.
    ResultOk(Option<Box<HirExpr>>),
    /// `result.err(valor)` com o payload de erro owned.
    ResultErr(Box<HirExpr>),
    /// `expressão?`: lê o sucesso e propaga o erro para o alvo.
    TryPropagate {
        expression: Box<HirExpr>,
        target: PropagateTarget,
    },
    /// Both operands are owned; fallback executes only for None.
    /// PT-BR: Ambos são owned; fallback executa somente para None.
    Coalesce {
        option: Box<HirExpr>,
        fallback: Box<HirExpr>,
        mode: CoalesceMode,
    },
    Local {
        id: BindingId,
        name: String,
    },
    StructLiteral {
        struct_name: String,
        fields: Vec<HirStructInitField>,
    },
    ArrayLiteral(Vec<HirExpr>),
    Unary {
        operator: UnaryOperator,
        operand: Box<HirExpr>,
    },
    Binary {
        left: Box<HirExpr>,
        operator: BinaryOperator,
        right: Box<HirExpr>,
    },
    FieldAccess {
        object: Box<HirExpr>,
        field: String,
    },
    Call {
        function: String,
        arguments: Vec<HirCallArgument>,
    },
    Index {
        object: Box<HirExpr>,
        index: Box<HirExpr>,
    },
    Grouping(Box<HirExpr>),
    /// Clona um place não `Copy` ao produzir um valor owned. O clone fica
    /// explícito no HIR para que o codegen não refaça análise de ownership.
    Clone {
        expression: Box<HirExpr>,
        mode: CloneMode,
    },
    /// Proven last use of a direct owned local; codegen emits its binding.
    /// PT-BR: Último uso comprovado de local owned; codegen emite o binding.
    Move(Box<HirExpr>),
}

/// Métodos higher-order disponíveis em arrays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrayMethodKind {
    Map,
    Filter,
    ForEach,
    Find,
    Some,
    Every,
    Reduce,
}

impl ArrayMethodKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Map => "map",
            Self::Filter => "filter",
            Self::ForEach => "forEach",
            Self::Find => "find",
            Self::Some => "some",
            Self::Every => "every",
            Self::Reduce => "reduce",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "map" => Some(Self::Map),
            "filter" => Some(Self::Filter),
            "forEach" => Some(Self::ForEach),
            "find" => Some(Self::Find),
            "some" => Some(Self::Some),
            "every" => Some(Self::Every),
            "reduce" => Some(Self::Reduce),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoalesceMode {
    Value,
    Optional,
}

/// Forma de materializar um place não `Copy` em um valor Lift owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneMode {
    /// O place já é owned (`value.clone()`).
    Value,
    /// O place está atrás de `&T`/`&mut T` (`(*value).clone()`).
    BorrowedAggregate,
    /// Parâmetro Lift `string` mapeado como Rust `&str` (`value.to_owned()`).
    BorrowedString,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirStructInitField {
    pub name: String,
    pub value: HirExpr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HirCallArgument {
    pub expression: HirExpr,
    pub pass_mode: ArgPassMode,
    /// Forma de produzir o valor owned quando `pass_mode` é `ByClone`.
    pub clone_mode: Option<CloneMode>,
    /// Posterga `&T`/`&mut T` até que os argumentos com efeitos sejam avaliados.
    pub defer_borrow_until_call: bool,
    /// Quando presente, o codegen avalia este argumento em um temporário
    /// antes de emitir os empréstimos da chamada. O índice é local à chamada.
    pub pre_evaluation_temp: Option<CallTempId>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallTempId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemaErrorKind {
    InvalidPackage,
    PrivatePackageMember,
    InvalidExport,
    InvalidImport,
    DuplicateDeclaration,
    DuplicateField,
    DuplicateParameter,
    UnknownType,
    UnknownVariable,
    UnknownFunction,
    UnknownField,
    TypeMismatch,
    InvalidOperator,
    InvalidCondition,
    InvalidAssignmentTarget,
    ImmutableBinding,
    ArgumentCountMismatch,
    NotCallable,
    MissingObjectField,
    UnexpectedObjectField,
    DuplicateObjectField,
    ObjectLiteralNeedsContext,
    EmptyArrayNeedsContext,
    HeterogeneousArray,
    InvalidReturn,
    MissingReturnPath,
    ReturnTypeRequired,
    InvalidIntegerLiteral,
    UnsupportedType,
    UnsupportedExpression,
    RecursiveValueType,
    InvalidNumberLiteral,
    BorrowConflict,
    BorrowMutRequiresMutablePlace,
    UnsupportedOwnershipPattern,
    /// `?`/`throw` sem `try` compatível nem retorno `Result<_, E>`.
    InvalidPropagation,
    /// `break`/`continue` fora de `while`/`for`.
    InvalidLoopControl,
    /// `match` não exaustivo, com braço duplicado ou padrão incompatível.
    InvalidMatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemaError {
    pub kind: SemaErrorKind,
    pub message: String,
    pub span: Span,
    pub secondary_span: Option<Span>,
}

impl SemaError {
    pub(crate) fn new(kind: SemaErrorKind, message: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            message: message.into(),
            span,
            secondary_span: None,
        }
    }

    pub(crate) fn with_secondary(mut self, span: Span) -> Self {
        self.secondary_span = Some(span);
        self
    }
}

impl fmt::Display for SemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} no span {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl Error for SemaError {}
