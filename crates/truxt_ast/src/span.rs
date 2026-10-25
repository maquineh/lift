//! Shared half-open byte ranges for source syntax and diagnostics.
//!
//! PT-BR: Intervalos de bytes semiabertos compartilhados por sintaxe e diagnósticos.

use std::ops::Range;

/// Intervalo semiaberto de offsets em bytes na fonte UTF-8 (`start..end`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn join(self, other: Self) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
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
    use super::Span;

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
}
