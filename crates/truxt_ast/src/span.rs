//! Shared half-open byte ranges for source syntax and diagnostics.
//!
//! PT-BR: Intervalos de bytes semiabertos compartilhados por sintaxe e diagnósticos.

use std::ops::Range;

/// File identity within one compilation; PT-BR: identidade de fonte numa compilação.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SourceId(pub usize);

/// Half-open byte offsets in one source; PT-BR: offsets de bytes em uma fonte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub source: SourceId,
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self::in_source(SourceId(0), start, end)
    }

    pub const fn in_source(source: SourceId, start: usize, end: usize) -> Self {
        Self { source, start, end }
    }

    pub const fn with_source(self, source: SourceId) -> Self {
        Self { source, ..self }
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn join(self, other: Self) -> Self {
        assert_eq!(
            self.source, other.source,
            "cannot join spans from different sources"
        );
        Self::in_source(
            self.source,
            self.start.min(other.start),
            self.end.max(other.end),
        )
    }

    pub fn as_range(self) -> Range<usize> {
        self.start..self.end
    }
}

impl From<Range<usize>> for Span {
    fn from(range: Range<usize>) -> Self {
        Self::new(range.start, range.end)
    }
}

impl From<Span> for Range<usize> {
    fn from(span: Span) -> Self {
        span.as_range()
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceId, Span};

    #[test]
    fn joins_spans_and_converts_to_range() {
        let joined = Span::new(8, 12).join(Span::new(2, 5));

        assert_eq!(joined, Span::new(2, 12));
        assert_eq!(joined.as_range(), 2..12);
        assert_eq!(joined.len(), 10);
    }

    #[test]
    fn zero_width_span_is_valid_for_eof() {
        let eof = Span::new(17, 17);

        assert!(eof.is_empty());
        assert_eq!(eof.len(), 0);
    }

    #[test]
    fn source_identity_is_preserved_when_joining() {
        let source = SourceId(7);
        let span = Span::in_source(source, 2, 5).join(Span::in_source(source, 9, 11));
        assert_eq!(span, Span::in_source(source, 2, 11));
    }

    #[test]
    #[should_panic(expected = "cannot join spans from different sources")]
    fn spans_from_different_files_cannot_be_joined() {
        Span::new(0, 1).join(Span::in_source(SourceId(1), 0, 1));
    }
}
