use std::{
    env,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{header::CONTENT_TYPE, HeaderName, HeaderValue, Request as HttpRequest, StatusCode},
    response::Response as HttpResponse,
    routing::get,
    Router,
};
use lift_db::{Note, NoteStore};
use tokio::{sync::Semaphore, task};

const MAX_BODY_BYTES: usize = 16 * 1024;
const MAX_BLOCKING_REQUESTS: usize = 8;

/// Owned request DTO passed to a synchronous named route handler.
#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Owned text response DTO returned by a synchronous named route handler.
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

type SharedStore = Arc<Mutex<NoteStore>>;
type SyncHandler = fn(Request, SharedStore) -> Response;

#[derive(Clone)]
struct AppState {
    store: SharedStore,
    blocking_slots: Arc<Semaphore>,
}

/// Starts the local notes service. Override its address and SQLite file with
/// `LIFT_NOTES_ADDR` and `LIFT_NOTES_DB`.
pub fn serve() -> Result<(), String> {
    let address = env::var("LIFT_NOTES_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_owned())
        .parse::<SocketAddr>()
        .map_err(|error| format!("invalid LIFT_NOTES_ADDR: {error}"))?;
    let database_path =
        env::var("LIFT_NOTES_DB").unwrap_or_else(|_| "lift-notes.sqlite3".to_owned());
    let store = NoteStore::open(&database_path)
        .map_err(|_| "could not open the notes database".to_owned())?;
    let state = AppState {
        store: Arc::new(Mutex::new(store)),
        blocking_slots: Arc::new(Semaphore::new(MAX_BLOCKING_REQUESTS)),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|error| format!("could not start the HTTP runtime: {error}"))?;

    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|error| format!("could not bind {address}: {error}"))?;
        let app = router(state);
        axum::serve(listener, app)
            .await
            .map_err(|error| format!("HTTP server failed: {error}"))
    })
}

fn router(state: AppState) -> Router {
    Router::new()
        .route(
            "/health",
            get(
                move |State(state): State<AppState>, request: HttpRequest<Body>| async move {
                    dispatch(state, request, health_handler).await
                },
            ),
        )
        .route(
            "/notes",
            get(
                move |State(state): State<AppState>, request: HttpRequest<Body>| async move {
                    dispatch(state, request, list_notes_handler).await
                },
            )
            .post(
                move |State(state): State<AppState>, request: HttpRequest<Body>| async move {
                    dispatch(state, request, create_note_handler).await
                },
            ),
        )
        .with_state(state)
}

async fn dispatch(
    state: AppState,
    request: HttpRequest<Body>,
    handler: SyncHandler,
) -> HttpResponse<Body> {
    let (parts, body) = request.into_parts();
    let bytes = match to_bytes(body, MAX_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return response(413, "request body exceeds 16384 bytes"),
    };
    let body = match String::from_utf8(bytes.to_vec()) {
        Ok(body) => body,
        Err(_) => return response(400, "request body must be valid UTF-8"),
    };
    let dto = Request {
        method: parts.method.to_string(),
        path: parts.uri.path().to_owned(),
        query: parts.uri.query().unwrap_or_default().to_owned(),
        headers: repeated_headers(&parts.headers),
        body,
    };
    let permit = match state.blocking_slots.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return response(503, "service is busy"),
    };
    let store = state.store.clone();
    let result = task::spawn_blocking(move || {
        let _permit = permit;
        handler(dto, store)
    })
    .await;
    match result {
        Ok(dto) => into_http_response(dto),
        Err(_) => response(500, "request handler failed"),
    }
}

fn health_handler(_request: Request, _store: SharedStore) -> Response {
    text_response(200, "ok".to_owned())
}

fn list_notes_handler(_request: Request, store: SharedStore) -> Response {
    let notes = match store.lock() {
        Ok(store) => store.list_notes(),
        Err(_) => return response_dto(500, "notes database is unavailable"),
    };
    match notes {
        Ok(notes) => text_response(200, format_notes(&notes)),
        Err(_) => response_dto(500, "notes database is unavailable"),
    }
}

fn create_note_handler(request: Request, store: SharedStore) -> Response {
    if request.body.trim().is_empty() {
        return response_dto(400, "note body must not be empty");
    }
    let id = match store.lock() {
        Ok(mut store) => store.save_note(&request.body),
        Err(_) => return response_dto(500, "notes database is unavailable"),
    };
    match id {
        Ok(id) => text_response(201, format!("created note {id}")),
        Err(_) => response_dto(500, "notes database is unavailable"),
    }
}

fn format_notes(notes: &[Note]) -> String {
    notes
        .iter()
        .map(|note| format!("{}\t{}", note.id, escape_lines(&note.body)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_lines(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            other => vec![other],
        })
        .collect()
}

fn text_response(status: u16, body: String) -> Response {
    Response {
        status,
        headers: vec![(
            "content-type".to_owned(),
            "text/plain; charset=utf-8".to_owned(),
        )],
        body,
    }
}

fn response_dto(status: u16, body: &str) -> Response {
    text_response(status, body.to_owned())
}

fn response(status: u16, body: &str) -> HttpResponse<Body> {
    into_http_response(response_dto(status, body))
}

fn into_http_response(dto: Response) -> HttpResponse<Body> {
    let status = StatusCode::from_u16(dto.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut builder = HttpResponse::builder().status(status);
    for (name, value) in dto.headers {
        let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) else {
            return HttpResponse::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(Body::from("invalid response header"))
                .expect("static HTTP response is valid");
        };
        builder = builder.header(name, value);
    }
    builder.body(Body::from(dto.body)).unwrap_or_else(|_| {
        HttpResponse::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("invalid response"))
            .expect("static HTTP response is valid")
    })
}

fn repeated_headers(headers: &axum::http::HeaderMap) -> Vec<(String, String)> {
    headers
        .keys()
        .flat_map(|name| {
            headers.get_all(name).iter().map(move |value| {
                (
                    name.as_str().to_owned(),
                    value.to_str().unwrap_or("").to_owned(),
                )
            })
        })
        .collect()
}
