//! Ariadne rendering for source diagnostics with UTF-8 byte-indexed spans.
//!
//! PT-BR: Renderização Ariadne de diagnósticos com spans indexados em bytes UTF-8.

use std::{
    fmt,
    io::{self, IsTerminal, Write},
    ops::Range,
};

use ariadne::{Color, Config, IndexType, Label, Report, ReportKind};
use lift_ast::{SourceId, Span};
use lift_project::{Diagnostic, Severity, SourceMap};

// EN: Path text is presentation only; SourceId prevents lossy-path collisions.
// PT-BR: Caminho é apresentação; SourceId impede colisões de textos com perda.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SourceKey {
    source: SourceId,
    display: String,
}

impl fmt::Display for SourceKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.display)
    }
}

pub(crate) fn report_diagnostic(sources: &SourceMap, diagnostic: &Diagnostic) {
    let Some(primary) = diagnostic.span else {
        eprintln!("lift: {}: {}", diagnostic.code, diagnostic.message);
        return;
    };
    let stderr = io::stderr();
    let color = stderr.is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let mut stderr = stderr.lock();
    let kind = match diagnostic.severity {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
    };
    let message = format!("[{}] {}", diagnostic.code, diagnostic.message);
    if write_report(
        &mut stderr,
        sources,
        primary,
        diagnostic.secondary_span,
        &message,
        color,
        kind,
    )
    .is_err()
    {
        let path = sources
            .get(primary.source)
            .map(|source| source.path.display().to_string())
            .unwrap_or_else(|| "<fonte desconhecida>".to_owned());
        let _ = writeln!(stderr, "{path}: erro: {message}");
    }
}

#[cfg(test)]
fn write_diagnostic(
    writer: impl Write,
    sources: &SourceMap,
    primary: Span,
    secondary: Option<Span>,
    message: &str,
    color: bool,
) -> io::Result<()> {
    write_report(
        writer,
        sources,
        primary,
        secondary,
        message,
        color,
        ReportKind::Error,
    )
}

fn write_report(
    writer: impl Write,
    sources: &SourceMap,
    primary: Span,
    secondary: Option<Span>,
    message: &str,
    color: bool,
    kind: ReportKind<'_>,
) -> io::Result<()> {
    let primary_source = sources
        .get(primary.source)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown diagnostic source"))?;
    let key = SourceKey {
        source: primary.source,
        display: primary_source.path.display().to_string(),
    };
    let primary = source_range(&primary_source.text, primary);
    let mut report = Report::build(kind, (key.clone(), primary.clone()))
        .with_config(
            Config::default()
                .with_index_type(IndexType::Byte)
                .with_color(color),
        )
        .with_message(message)
        .with_label(
            Label::new((key, primary))
                .with_message(message)
                .with_color(Color::Red)
                .with_order(0),
        );
    if let Some(secondary) = secondary {
        let secondary_source = sources.get(secondary.source).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "unknown secondary source")
        })?;
        report = report.with_label(
            Label::new((
                SourceKey {
                    source: secondary.source,
                    display: secondary_source.path.display().to_string(),
                },
                source_range(&secondary_source.text, secondary),
            ))
            .with_message("local relacionado")
            .with_color(Color::Blue)
            .with_order(1),
        );
    }
    report.finish().write(
        ariadne::sources(sources.iter().map(|(id, source)| {
            (
                SourceKey {
                    source: id,
                    display: source.path.display().to_string(),
                },
                source.text.as_str(),
            )
        })),
        writer,
    )
}

// EN: Keep byte spans in bounds, preserving valid UTF-8 boundaries and empty EOF.
// PT-BR: Mantém spans nos limites e fronteiras UTF-8, preservando EOF vazio.
fn source_range(source: &str, span: Span) -> Range<usize> {
    let mut start = span.start.min(source.len());
    let mut end = span.end.min(source.len()).max(start);
    while !source.is_char_boundary(start) {
        start -= 1;
    }
    while !source.is_char_boundary(end) {
        end += 1;
    }
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(any(unix, windows))]
    fn lossy_path_display_does_not_merge_diagnostic_sources() {
        use std::ffi::OsString;
        #[cfg(unix)]
        let paths = {
            use std::os::unix::ffi::OsStringExt;
            [
                OsString::from_vec(vec![b'a', 0xff]),
                OsString::from_vec(vec![b'a', 0xfe]),
            ]
        };
        #[cfg(windows)]
        let paths = {
            use std::os::windows::ffi::OsStringExt;
            [
                OsString::from_wide(&[0x61, 0xd800]),
                OsString::from_wide(&[0x61, 0xd801]),
            ]
        };
        let paths = paths.map(std::path::PathBuf::from);
        assert_eq!(
            paths[0].display().to_string(),
            paths[1].display().to_string()
        );
        let mut sources = SourceMap::default();
        let first = sources.add(paths[0].clone(), "const first = 1;".to_owned());
        let second = sources.add(paths[1].clone(), "const second = 2;".to_owned());
        let mut output = Vec::new();
        write_diagnostic(
            &mut output,
            &sources,
            Span::in_source(second, 6, 12),
            Some(Span::in_source(first, 6, 11)),
            "two files",
            false,
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("const first = 1;"), "{output}");
        assert!(output.contains("const second = 2;"), "{output}");
    }

    fn render(source: &str, primary: Span, secondary: Option<Span>) -> String {
        let mut output = Vec::new();
        let mut sources = SourceMap::default();
        sources.add("sample.lift".into(), source.to_owned());
        write_diagnostic(
            &mut output,
            &sources,
            primary,
            secondary,
            "mensagem de teste",
            false,
        )
        .unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn utf8_bytes_are_rendered_at_the_correct_character_column() {
        let source = "const texto = \"á🙂\"; const value: int = true;";
        let start = source.find("true").unwrap();
        let column = source[..start].chars().count() + 1;
        let output = render(source, Span::new(start, start + 4), None);
        assert!(
            output.contains(&format!("sample.lift:1:{column}")),
            "{output}"
        );
        assert!(output.contains(source));
        assert!(output.contains("mensagem de teste"));
        assert!(!output.contains('\u{1b}'));
    }

    #[test]
    fn empty_eof_spans_render_for_empty_and_nonempty_sources() {
        for source in ["", "const text = \"á🙂\";\nfunction unfinished(): void {"] {
            let output = render(source, Span::new(source.len(), source.len()), None);
            assert!(output.contains("sample.lift:"), "{output}");
            assert!(output.contains("mensagem de teste"));
        }
    }

    #[test]
    fn related_spans_share_one_report() {
        let source = "const item = 1;\nconst item = 2;";
        let primary = source.rfind("item").unwrap();
        let secondary = source.find("item").unwrap();
        let output = render(
            source,
            Span::new(primary, primary + 4),
            Some(Span::new(secondary, secondary + 4)),
        );
        assert_eq!(output.matches("Error:").count(), 1, "{output}");
        assert!(output.contains("local relacionado"));
        assert!(output.contains("const item = 1;"));
        assert!(output.contains("const item = 2;"));
    }

    #[test]
    fn related_labels_can_resolve_different_source_files() {
        let mut sources = SourceMap::default();
        let first = sources.add("first.lift".into(), "function same(): void {}".to_owned());
        let second = sources.add(
            "second.lift".into(),
            "// á🙂\nfunction same(): void {}".to_owned(),
        );
        let second_start = sources.get(second).unwrap().text.find("same").unwrap();
        let mut output = Vec::new();
        write_diagnostic(
            &mut output,
            &sources,
            Span::in_source(second, second_start, second_start + 4),
            Some(Span::in_source(first, 9, 13)),
            "duplicate",
            false,
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("first.lift"), "{output}");
        assert!(output.contains("second.lift"), "{output}");
        assert!(output.contains("local relacionado"), "{output}");
        assert_eq!(output.matches("Error:").count(), 1);
    }
}
