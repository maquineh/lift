//! Stable public diagnostic codes, independent of Rust enum Debug formatting.
//! PT-BR: Códigos públicos estáveis, independentes do Debug de enums Rust.
use lift_sema::SemaErrorKind;

pub(crate) const fn semantic_code(kind: SemaErrorKind) -> &'static str {
    match kind {
        SemaErrorKind::InvalidPackage => "sema.InvalidPackage",
        SemaErrorKind::PrivatePackageMember => "sema.PrivatePackageMember",
        SemaErrorKind::InvalidExport => "sema.InvalidExport",
        SemaErrorKind::InvalidImport => "sema.InvalidImport",
        SemaErrorKind::DuplicateDeclaration => "sema.DuplicateDeclaration",
        SemaErrorKind::DuplicateField => "sema.DuplicateField",
        SemaErrorKind::DuplicateParameter => "sema.DuplicateParameter",
        SemaErrorKind::UnknownType => "sema.UnknownType",
        SemaErrorKind::UnknownVariable => "sema.UnknownVariable",
        SemaErrorKind::UnknownFunction => "sema.UnknownFunction",
        SemaErrorKind::UnknownField => "sema.UnknownField",
        SemaErrorKind::TypeMismatch => "sema.TypeMismatch",
        SemaErrorKind::InvalidOperator => "sema.InvalidOperator",
        SemaErrorKind::InvalidCondition => "sema.InvalidCondition",
        SemaErrorKind::InvalidAssignmentTarget => "sema.InvalidAssignmentTarget",
        SemaErrorKind::ImmutableBinding => "sema.ImmutableBinding",
        SemaErrorKind::ArgumentCountMismatch => "sema.ArgumentCountMismatch",
        SemaErrorKind::NotCallable => "sema.NotCallable",
        SemaErrorKind::MissingObjectField => "sema.MissingObjectField",
        SemaErrorKind::UnexpectedObjectField => "sema.UnexpectedObjectField",
        SemaErrorKind::DuplicateObjectField => "sema.DuplicateObjectField",
        SemaErrorKind::ObjectLiteralNeedsContext => "sema.ObjectLiteralNeedsContext",
        SemaErrorKind::EmptyArrayNeedsContext => "sema.EmptyArrayNeedsContext",
        SemaErrorKind::HeterogeneousArray => "sema.HeterogeneousArray",
        SemaErrorKind::InvalidReturn => "sema.InvalidReturn",
        SemaErrorKind::MissingReturnPath => "sema.MissingReturnPath",
        SemaErrorKind::ReturnTypeRequired => "sema.ReturnTypeRequired",
        SemaErrorKind::InvalidIntegerLiteral => "sema.InvalidIntegerLiteral",
        SemaErrorKind::UnsupportedType => "sema.UnsupportedType",
        SemaErrorKind::UnsupportedExpression => "sema.UnsupportedExpression",
        SemaErrorKind::RecursiveValueType => "sema.RecursiveValueType",
        SemaErrorKind::InvalidNumberLiteral => "sema.InvalidNumberLiteral",
        SemaErrorKind::BorrowConflict => "sema.BorrowConflict",
        SemaErrorKind::BorrowMutRequiresMutablePlace => "sema.BorrowMutRequiresMutablePlace",
        SemaErrorKind::UnsupportedOwnershipPattern => "sema.UnsupportedOwnershipPattern",
    }
}
