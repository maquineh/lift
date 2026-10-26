//! Initial LSP: synchronous full-text diagnostics using the shared project API.
//! PT-BR: LSP inicial: diagnósticos síncronos com overlays e API de projeto.

use lift_ast::Span;
use lift_project::{source_path, Diagnostic, ProjectSession, Severity, SourceMap};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

const MAX_MESSAGE: usize = 16 * 1024 * 1024;

pub(crate) fn serve() -> io::Result<u8> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run(&mut stdin.lock(), &mut stdout.lock())
}

struct Document {
    path: PathBuf,
    version: i64,
    revision: u64,
    text: String,
}

#[derive(Default)]
struct Server {
    session: ProjectSession,
    documents: HashMap<String, Document>,
    published: HashSet<String>,
    entry: Option<PathBuf>,
    initialized: bool,
    shutdown: bool,
    revision: u64,
}

fn run(reader: &mut impl BufRead, writer: &mut impl Write) -> io::Result<u8> {
    let mut server = Server::default();
    while let Some(bytes) = read_frame(reader)? {
        let message: Value = match serde_json::from_slice(&bytes) {
            Ok(message) => message,
            Err(_) => {
                write_message(
                    writer,
                    &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
                )?;
                continue;
            }
        };
        let valid_id = message
            .get("id")
            .is_none_or(|id| id.is_string() || id.is_i64() || id.is_u64() || id.is_null());
        if !message.is_object()
            || message["jsonrpc"] != "2.0"
            || !message["method"].is_string()
            || !valid_id
            || message
                .get("params")
                .is_some_and(|params| !params.is_object() && !params.is_array())
        {
            let id = message
                .get("id")
                .filter(|_| valid_id)
                .cloned()
                .unwrap_or(Value::Null);
            write_message(
                writer,
                &json!({"jsonrpc":"2.0","id":id,"error":{"code":-32600,"message":"Invalid Request"}}),
            )?;
            continue;
        }
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let id = message.get("id");
        let params = &message["params"];
        if method == "exit" {
            return Ok(u8::from(!server.shutdown));
        }
        if let Some(id) = id {
            let result = if method == "initialize" && !server.initialized {
                match params["initializationOptions"]["entry"]
                    .as_str()
                    .map(uri_or_path)
                    .transpose()
                {
                    Ok(entry) => {
                        server.entry = entry;
                        server.initialized = true;
                        Ok(
                            json!({"capabilities":{"positionEncoding":"utf-16","textDocumentSync":{"openClose":true,"change":1,"save":{"includeText":false}}},"serverInfo":{"name":"lift","version":env!("CARGO_PKG_VERSION")}}),
                        )
                    }
                    Err(error) => Err((-32602, error.to_string())),
                }
            } else if !server.initialized {
                Err((-32002, "Server not initialized".to_owned()))
            } else if server.shutdown {
                Err((-32600, "Server has shut down".to_owned()))
            } else if method == "shutdown" {
                server.shutdown = true;
                Ok(Value::Null)
            } else {
                Err((-32601, format!("Method not supported: {method}")))
            };
            let response = match result {
                Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                Err((code, message)) => {
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
                }
            };
            write_message(writer, &response)?;
            continue;
        }
        if !server.initialized || server.shutdown {
            continue;
        }
        if let Err(error) = server.notification(method, params, writer) {
            write_message(
                writer,
                &json!({"jsonrpc":"2.0","method":"window/logMessage","params":{"type":1,"message":error.to_string()}}),
            )?;
        }
    }
    Ok(u8::from(!server.shutdown))
}

impl Server {
    fn notification(
        &mut self,
        method: &str,
        params: &Value,
        writer: &mut impl Write,
    ) -> io::Result<()> {
        if method == "workspace/didChangeWatchedFiles" {
            return self.publish(writer);
        }
        if !matches!(
            method,
            "textDocument/didOpen"
                | "textDocument/didChange"
                | "textDocument/didClose"
                | "textDocument/didSave"
        ) {
            return Ok(());
        }
        let document = &params["textDocument"];
        let uri = document["uri"]
            .as_str()
            .ok_or_else(|| invalid("Missing file URI"))?;
        let path = uri_to_path(uri)?;
        match method {
            "textDocument/didOpen" => {
                let text = document["text"]
                    .as_str()
                    .ok_or_else(|| invalid("Missing document text"))?;
                let version = document["version"]
                    .as_i64()
                    .ok_or_else(|| invalid("Missing document version"))?;
                self.revision = self.revision.saturating_add(1);
                self.documents.insert(
                    uri.to_owned(),
                    Document {
                        path,
                        version,
                        revision: self.revision,
                        text: text.to_owned(),
                    },
                );
            }
            "textDocument/didChange" => {
                let Some(open) = self.documents.get_mut(uri) else {
                    return Ok(());
                };
                let version = document["version"]
                    .as_i64()
                    .ok_or_else(|| invalid("Missing document version"))?;
                if version <= open.version {
                    return Ok(());
                }
                let changes = params["contentChanges"]
                    .as_array()
                    .ok_or_else(|| invalid("Missing contentChanges"))?;
                if changes.iter().any(|change| change.get("range").is_some()) {
                    return Err(invalid("Lift requires full text synchronization"));
                }
                let Some(change) = changes.last() else {
                    return Ok(());
                };
                let text = change["text"]
                    .as_str()
                    .ok_or_else(|| invalid("Missing changed text"))?;
                self.revision = self.revision.saturating_add(1);
                open.version = version;
                open.revision = self.revision;
                open.text = text.to_owned();
            }
            "textDocument/didClose" => {
                self.documents.remove(uri);
            }
            _ => {}
        }
        // Rebuild in update order, including URIs with identical decoded paths.
        // Closing an authoritative alias restores the last remaining buffer.
        // PT-BR: Reconstrói por ordem de edição, inclusive caminhos decodificados
        // iguais. Fechar alias autoritativo restaura o último buffer restante.
        let mut ordered: Vec<_> = self.documents.values().collect();
        ordered.sort_by_key(|doc| doc.revision);
        self.session = ProjectSession::new();
        for document in ordered {
            self.session
                .set_overlay(&document.path, document.text.clone())?;
        }
        self.publish(writer)
    }

    fn uri(&self, path: &Path) -> io::Result<String> {
        self.documents
            .iter()
            .filter(|(_, doc)| source_path(&doc.path).is_ok_and(|canonical| canonical == path))
            .max_by_key(|(_, doc)| doc.revision)
            .map(|(uri, _)| Ok(uri.clone()))
            .unwrap_or_else(|| path_to_uri(path))
    }

    fn publish(&mut self, writer: &mut impl Write) -> io::Result<()> {
        let mut entries: Vec<_> = self
            .entry
            .iter()
            .cloned()
            .chain(if self.entry.is_none() {
                self.documents
                    .values()
                    .map(|doc| doc.path.clone())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            })
            .collect();
        entries.sort();
        let mut grouped: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        // Clear files from the previous graph and open files with no diagnostics.
        for uri in &self.published {
            grouped.entry(uri.clone()).or_default();
        }
        for uri in self.documents.keys() {
            grouped.entry(uri.clone()).or_default();
        }
        for entry in entries {
            let mut analysis = self.session.analyze(&entry);
            if let Some(hir) = &analysis.hir {
                analysis.diagnostics.extend(lift_project::lint::lint(hir));
            }
            for (_, source) in analysis.sources.iter() {
                grouped.entry(self.uri(&source.path)?).or_default();
            }
            for diagnostic in &analysis.diagnostics {
                let (uri, value) = match diagnostic.span {
                    Some(span) => {
                        let Some(source) = analysis.sources.get(span.source) else {
                            continue;
                        };
                        (
                            self.uri(&source.path)?,
                            self.diagnostic(diagnostic, &analysis.sources),
                        )
                    }
                    None => (
                        self.uri(&analysis.entry)?,
                        json!({"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"severity":1,"source":"lift","code":diagnostic.code,"message":diagnostic.message}),
                    ),
                };
                let values = grouped.entry(uri).or_default();
                if !values.contains(&value) {
                    values.push(value);
                }
            }
        }
        self.published.clear();
        for (uri, diagnostics) in grouped {
            if !diagnostics.is_empty() {
                self.published.insert(uri.clone());
            }
            let mut params = json!({"uri":uri,"diagnostics":diagnostics});
            if let Some(doc) = self.documents.get(&uri) {
                params["version"] = json!(doc.version);
            }
            write_message(
                writer,
                &json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":params}),
            )?;
        }
        Ok(())
    }

    fn diagnostic(&self, diagnostic: &Diagnostic, sources: &SourceMap) -> Value {
        let mut value = json!({"range":range(sources, diagnostic.span.unwrap()),"severity":if diagnostic.severity == Severity::Error { 1 } else { 2 },"code":diagnostic.code,"source":"lift","message":diagnostic.message});
        if let Some(secondary) = diagnostic.secondary_span {
            if let Some(source) = sources.get(secondary.source) {
                if let Ok(uri) = self.uri(&source.path) {
                    value["relatedInformation"] = json!([{"location":{"uri":uri,"range":range(sources,secondary)},"message":"local relacionado"}]);
                }
            }
        }
        value
    }
}

fn range(sources: &SourceMap, span: Span) -> Value {
    let source = sources.get(span.source).unwrap();
    let start = source.position_utf16(span.start);
    let end = source.position_utf16(span.end.max(span.start));
    json!({"start":{"line":start.line,"character":start.character},"end":{"line":end.line,"character":end.character}})
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn read_frame(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut length = None;
    let mut header_bytes = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return if header_bytes == 0 {
                Ok(None)
            } else {
                Err(invalid("Truncated LSP header"))
            };
        }
        header_bytes += line.len();
        if header_bytes > 8192 {
            return Err(invalid("LSP header too large"));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                if length.is_some() {
                    return Err(invalid("Duplicate Content-Length"));
                }
                length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| invalid("Invalid Content-Length"))?,
                );
            }
        }
    }
    let length = length
        .filter(|n| *n <= MAX_MESSAGE)
        .ok_or_else(|| invalid("Missing or excessive Content-Length"))?;
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Ok(Some(bytes))
}

#[cfg(test)]
fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    read_frame(reader)?
        .map(|bytes| serde_json::from_slice(&bytes).map_err(|error| invalid(error.to_string())))
        .transpose()
}

fn write_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", bytes.len())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

fn uri_or_path(value: &str) -> io::Result<PathBuf> {
    if value.starts_with("file:") {
        uri_to_path(value)
    } else {
        source_path(Path::new(value))
    }
}

fn uri_to_path(uri: &str) -> io::Result<PathBuf> {
    let rest = uri
        .strip_prefix("file://")
        .ok_or_else(|| invalid("Only file:// URIs are supported"))?;
    if rest.contains(['?', '#']) {
        return Err(invalid("File URI must escape query/fragment characters"));
    }
    let (host, path) = rest
        .split_once('/')
        .ok_or_else(|| invalid("Invalid file URI"))?;
    let mut bytes = Vec::new();
    let encoded = path.as_bytes();
    let mut i = 0;
    while i < encoded.len() {
        if encoded[i] == b'%' {
            let hex = encoded
                .get(i + 1..i + 3)
                .and_then(|s| std::str::from_utf8(s).ok())
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or_else(|| invalid("Invalid URI escape"))?;
            bytes.push(hex);
            i += 3;
        } else {
            bytes.push(encoded[i]);
            i += 1;
        }
    }
    let decoded = String::from_utf8(bytes).map_err(|_| invalid("File URI must encode UTF-8"))?;
    if decoded.contains('\0') {
        return Err(invalid("NUL in file URI"));
    }
    #[cfg(windows)]
    let path = if !host.is_empty() && host != "localhost" {
        PathBuf::from(format!("//{host}/{decoded}"))
    } else if decoded.as_bytes().get(1) == Some(&b':') {
        PathBuf::from(decoded)
    } else {
        return Err(invalid(
            "Windows file URI requires an absolute drive or UNC path",
        ));
    };
    #[cfg(not(windows))]
    let path = {
        if !host.is_empty() && host != "localhost" {
            return Err(invalid("Remote file URI is unsupported on this host"));
        }
        PathBuf::from(format!("/{decoded}"))
    };
    Ok(path)
}

fn path_to_uri(path: &Path) -> io::Result<String> {
    let text = path
        .to_str()
        .ok_or_else(|| invalid("LSP requires UTF-8 file paths"))?;
    #[cfg(windows)]
    let text = text
        .strip_prefix("\\\\?\\UNC\\")
        .map(|rest| format!("//{rest}"))
        .unwrap_or_else(|| text.strip_prefix("\\\\?\\").unwrap_or(text).to_owned())
        .replace('\\', "/");
    #[cfg(not(windows))]
    let text = text.to_owned();
    let prefix = if text.starts_with("//") {
        "file:"
    } else if text.starts_with('/') {
        "file://"
    } else {
        "file:///"
    };
    let mut uri = prefix.to_owned();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b':' | b'-' | b'_' | b'.' | b'~') {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn exchange(messages: &[Value]) -> (u8, Vec<Value>) {
        let mut input = Vec::new();
        for message in messages {
            write_message(&mut input, message).unwrap();
        }
        let mut output = Vec::new();
        let status = run(&mut Cursor::new(input), &mut output).unwrap();
        let mut reader = Cursor::new(output);
        let mut messages = Vec::new();
        while let Some(message) = read_message(&mut reader).unwrap() {
            messages.push(message);
        }
        (status, messages)
    }

    fn uri(name: &str) -> String {
        path_to_uri(
            &std::env::temp_dir()
                .join(format!("lift-lsp-unsaved-{}", std::process::id()))
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn protocol_versions_utf16_multifile_overlays_and_diagnostic_clearing() {
        let main = uri("main.lift");
        let dep = uri("dep á🙂.lift");
        let bad = "function answer(): int { const _label = \"á🙂\"; return true; }";
        let good = "function answer(): int { return 7; }";
        let (status, output) = exchange(&[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"initializationOptions":{"entry":main}}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":main,"version":1,"text":"import module \"dep á🙂.lift\"; console.log(answer());"}}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":dep,"version":2,"text":bad}}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":dep,"version":3},"contentChanges":[{"text":good}]}}),
            // Stale notification must not replace the accepted version-3 snapshot.
            json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":dep,"version":2},"contentChanges":[{"text":bad}]}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{"textDocument":{"uri":dep}}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":dep,"version":4},"contentChanges":[{"text":bad}]}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":dep}}}),
            json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
            json!({"jsonrpc":"2.0","method":"exit"}),
        ]);
        assert_eq!(status, 0);
        assert_eq!(
            output[0]["result"]["capabilities"]["positionEncoding"],
            "utf-16"
        );
        let diagnostics: Vec<_> = output
            .iter()
            .filter(|message| message["method"] == "textDocument/publishDiagnostics")
            .map(|message| &message["params"])
            .collect();
        let error = diagnostics
            .iter()
            .find(|params| params["uri"] == dep && params["version"] == 2)
            .unwrap();
        assert_eq!(error["diagnostics"][0]["severity"], 1);
        assert_eq!(error["diagnostics"][0]["code"], "sema.TypeMismatch");
        let start = bad[..bad.find("true").unwrap()].encode_utf16().count();
        assert_eq!(
            error["diagnostics"][0]["range"]["start"],
            json!({"line":0,"character":start})
        );
        assert!(diagnostics
            .iter()
            .filter(|params| params["uri"] == dep && params["version"] == 3)
            .all(|params| params["diagnostics"] == json!([])));
        assert_eq!(
            diagnostics
                .iter()
                .filter(|params| params["uri"] == dep && params["version"] == 2)
                .count(),
            1
        );
        assert!(diagnostics.iter().any(|params| params["uri"] == dep
            && params.get("version").is_none()
            && params["diagnostics"] == json!([])));
        assert!(diagnostics.iter().any(|params| params["uri"] == main
            && params["version"] == 1
            && params["diagnostics"][0]["code"] == "project.io"));
        assert_eq!(
            output.last().unwrap(),
            &json!({"jsonrpc":"2.0","id":2,"result":null})
        );
    }

    #[test]
    fn secondary_diagnostics_and_lint_are_published_as_related_information_and_warnings() {
        let main = uri("duplicate.lift");
        let dep = uri("duplicate-dep.lift");
        let (_, output) = exchange(&[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"initializationOptions":{"entry":main}}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":dep,"version":1,"text":"function duplicate(): void {}"}}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":main,"version":1,"text":"import module \"duplicate-dep.lift\"; function duplicate(): void {}"}}}),
            json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":main,"version":2},"contentChanges":[{"text":"const unused = 2;"}]}}),
            json!({"jsonrpc":"2.0","id":2,"method":"shutdown"}),
            json!({"jsonrpc":"2.0","method":"exit"}),
        ]);
        let related = output
            .iter()
            .find_map(|message| {
                message["params"]["diagnostics"]
                    .as_array()?
                    .iter()
                    .find(|d| d.get("relatedInformation").is_some())
            })
            .unwrap();
        let related_uri = related["relatedInformation"][0]["location"]["uri"]
            .as_str()
            .unwrap();
        assert!(related_uri == main || related_uri == dep);
        assert!(output
            .iter()
            .any(
                |message| message["params"]["diagnostics"][0]["code"] == "unused-binding"
                    && message["params"]["diagnostics"][0]["severity"] == 2
            ));
    }

    #[test]
    fn handshake_unknown_requests_shutdown_and_abnormal_exit() {
        let (status, output) = exchange(&[
            json!({"jsonrpc":"2.0","id":0,"method":"textDocument/hover"}),
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover"}),
            json!({"jsonrpc":"2.0","id":3,"method":"shutdown"}),
            json!({"jsonrpc":"2.0","id":4,"method":"textDocument/hover"}),
            json!({"jsonrpc":"2.0","method":"exit"}),
        ]);
        assert_eq!(status, 0);
        assert_eq!(output[0]["error"]["code"], -32002);
        assert_eq!(output[2]["error"]["code"], -32601);
        assert_eq!(output[4]["error"]["code"], -32600);
        assert_eq!(exchange(&[json!({"jsonrpc":"2.0","method":"exit"})]).0, 1);
    }

    #[test]
    fn uri_roundtrip_and_invalid_framing() {
        let path = std::env::temp_dir().join("space á🙂#%.lift");
        let uri = path_to_uri(&path).unwrap();
        assert_eq!(
            source_path(&uri_to_path(&uri).unwrap()).unwrap(),
            source_path(&path).unwrap()
        );
        for uri in [
            "untitled:x",
            "https://example.com/a",
            "file:///bad%GG",
            "file:///bad%00",
            "file:///bad%FF",
            "file:///a?b",
        ] {
            assert!(uri_to_path(uri).is_err(), "{uri}");
        }
        for bytes in [
            b"Content-Length: 2\r\n".as_slice(),
            b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}",
            b"Content-Length: 999999999\r\n\r\n",
            b"Content-Length: 2\r\n\r\n{",
        ] {
            assert!(read_message(&mut Cursor::new(bytes)).is_err());
        }
    }

    #[test]
    fn malformed_json_and_invalid_requests_report_errors_and_keep_session_alive() {
        let mut input = b"Content-Length: 1\r\n\r\n{".to_vec();
        for invalid_request in [
            json!({}),
            json!({"jsonrpc":"1.0","id":3,"method":"initialize"}),
            json!({"jsonrpc":"2.0","id":true,"method":"initialize"}),
            json!({"jsonrpc":"2.0","id":4,"method":"initialize","params":"bad"}),
            json!([]),
        ] {
            write_message(&mut input, &invalid_request).unwrap();
        }
        for message in [
            json!({"jsonrpc":"2.0","id":10,"method":"initialize","params":{}}),
            json!({"jsonrpc":"2.0","id":11,"method":"textDocument/hover"}),
            json!({"jsonrpc":"2.0","id":12,"method":"shutdown"}),
            json!({"jsonrpc":"2.0","method":"exit"}),
        ] {
            write_message(&mut input, &message).unwrap();
        }
        let mut output = Vec::new();
        assert_eq!(run(&mut Cursor::new(input), &mut output).unwrap(), 0);
        let mut reader = Cursor::new(output);
        let mut responses = Vec::new();
        while let Some(response) = read_message(&mut reader).unwrap() {
            responses.push(response);
        }
        assert_eq!(responses[0]["error"]["code"], -32700);
        assert!(responses[0]["id"].is_null());
        assert!(responses[1..6]
            .iter()
            .all(|response| response["error"]["code"] == -32600));
        assert!(responses[6]["result"]["capabilities"].is_object());
        assert_eq!(responses[7]["id"], 11);
        assert_eq!(responses[7]["error"]["code"], -32601);
        assert_eq!(responses[8], json!({"jsonrpc":"2.0","id":12,"result":null}));
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn document_uri_survives_first_save_through_directory_alias() {
        let directory =
            std::env::temp_dir().join(format!("lift-lsp-junction-{}", std::process::id()));
        let real = directory.join("real");
        let alias = directory.join("alias");
        std::fs::create_dir_all(&real).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        #[cfg(windows)]
        {
            let output = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", "New-Item -ItemType Junction -Path $env:LIFT_TEST_LINK -Target $env:LIFT_TEST_TARGET | Out-Null"])
                .env("LIFT_TEST_LINK", &alias).env("LIFT_TEST_TARGET", &real).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let path = alias.join("new.lift");
        let uri = path_to_uri(&path).unwrap();
        let mut server = Server::default();
        let mut output = Vec::new();
        server
            .notification(
                "textDocument/didOpen",
                &json!({"textDocument":{"uri":uri,"version":1,"text":"const _valid = 1;"}}),
                &mut output,
            )
            .unwrap();
        std::fs::write(&path, "const _disk = 1;").unwrap();
        output.clear();
        server.notification("textDocument/didChange", &json!({"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"const bad: int = true;"}]}), &mut output).unwrap();
        let mut reader = Cursor::new(output);
        let diagnostic = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(diagnostic["params"]["uri"], uri);
        assert_eq!(diagnostic["params"]["version"], 2);
        assert_eq!(
            diagnostic["params"]["diagnostics"][0]["code"],
            "sema.TypeMismatch"
        );
        server
            .notification(
                "textDocument/didClose",
                &json!({"textDocument":{"uri":uri}}),
                &mut Vec::new(),
            )
            .unwrap();
        assert!(server.session.analyze(&path).hir.is_some());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn simultaneous_aliases_use_latest_update_uri_and_restore_remaining_buffer() {
        let first = uri("aliases.lift");
        let second = first.replace("aliases.lift", "%61liases.lift");
        let mut server = Server::default();
        let mut output = Vec::new();
        server
            .notification(
                "textDocument/didOpen",
                &json!({"textDocument":{"uri":first,"version":10,"text":"const bad: int = true;"}}),
                &mut output,
            )
            .unwrap();
        output.clear();
        server.notification("textDocument/didOpen", &json!({"textDocument":{"uri":second,"version":1,"text":"const bad: int = false;"}}), &mut output).unwrap();
        let published = |bytes: Vec<u8>| {
            let mut reader = Cursor::new(bytes);
            let mut values = Vec::new();
            while let Some(value) = read_message(&mut reader).unwrap() {
                values.push(value["params"].clone());
            }
            values
        };
        let check = |values: Vec<Value>, authoritative: &str, version: i64, cleared: &str| {
            assert!(
                values.iter().any(|value| value["uri"] == authoritative
                    && value["version"] == version
                    && value["diagnostics"][0]["code"] == "sema.TypeMismatch"),
                "{values:?}"
            );
            assert!(
                values
                    .iter()
                    .any(|value| value["uri"] == cleared && value["diagnostics"] == json!([])),
                "{values:?}"
            );
        };
        check(published(std::mem::take(&mut output)), &second, 1, &first);
        server.notification("textDocument/didChange", &json!({"textDocument":{"uri":first,"version":11},"contentChanges":[{"text":"const bad: int = true;"}]}), &mut output).unwrap();
        check(published(std::mem::take(&mut output)), &first, 11, &second);
        server
            .notification(
                "textDocument/didClose",
                &json!({"textDocument":{"uri":first}}),
                &mut output,
            )
            .unwrap();
        check(published(output), &second, 1, &first);
        let path = uri_to_path(&second).unwrap();
        assert!(server.session.analyze(path).has_errors());
    }

    #[test]
    fn manifested_documents_publish_dependency_overlays_and_outside_graph_errors() {
        let directory =
            std::env::temp_dir().join(format!("lift-lsp-packages-{}", std::process::id()));
        let app = directory.join("app");
        let lib = directory.join("lib");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(app.join("lift.toml"),"[package]\nname='app'\nversion='1'\nkind='bin'\nentry='main.lift'\n[dependencies]\nlib={path='../lib'}").unwrap();
        std::fs::write(
            lib.join("lift.toml"),
            "[package]\nname='lib'\nversion='1'\nkind='lib'\nentry='main.lift'",
        )
        .unwrap();
        let main = app.join("main.lift");
        let source = lib.join("main.lift");
        let extra = app.join("extra.lift");
        std::fs::write(
            &main,
            "import * as lib from \"lib\"; console.log(lib.answer());",
        )
        .unwrap();
        std::fs::write(&source, "export function answer():int {return 7;}").unwrap();
        std::fs::write(&extra, "").unwrap();
        let uri = path_to_uri(&source).unwrap();
        let mut server = Server {
            entry: Some(main),
            ..Server::default()
        };
        let mut output = Vec::new();
        server.notification("textDocument/didOpen",&json!({"textDocument":{"uri":uri,"version":5,"text":"export function answer():int {return true;}"}}),&mut output).unwrap();
        let mut reader = Cursor::new(output);
        let mut found = false;
        while let Some(message) = read_message(&mut reader).unwrap() {
            if message["params"]["uri"] == uri && message["params"]["version"] == 5 {
                found = message["params"]["diagnostics"][0]["code"] == "sema.TypeMismatch";
            }
        }
        assert!(found);
        let mut standalone = Server::default();
        let mut output = Vec::new();
        let extra_uri = path_to_uri(&extra).unwrap();
        standalone.notification("textDocument/didOpen",&json!({"textDocument":{"uri":extra_uri,"version":1,"text":"const bad:int = true;"}}),&mut output).unwrap();
        let mut reader = Cursor::new(output);
        let mut found = false;
        while let Some(message) = read_message(&mut reader).unwrap() {
            if message["params"]["uri"] == extra_uri {
                found = message["params"]["diagnostics"][0]["code"] == "package.outside-graph";
            }
        }
        assert!(found);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
