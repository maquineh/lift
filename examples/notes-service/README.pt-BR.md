# Serviço local de notas

O exemplo agora usa a [API SQL geral](../../stdlib/lift_db/README.pt-BR.md).
`LIFT_DB_DRIVER` aceita `sqlite` (padrão) ou `postgres`; `LIFT_DB_URL` usa
`sqlite:lift-notes.sqlite3` por padrão. Pool: `LIFT_DB_MIN_IDLE` (1),
`LIFT_DB_MAX_CONNECTIONS` (4), `LIFT_DB_ACQUIRE_TIMEOUT_MS` (1000).
`LIFT_NOTES_DB` continua como caminho SQLite quando LIFT_DB_URL não foi definido.
URLs PostgreSQL exigem `sslmode=require` com verificação real ou
`sslmode=disable` explícito para plaintext local/CI. DDL, binds `$1`, getters e
transações estão em Lift. `http.serve(...,pool)` possui o pool; handlers recebem
empréstimo imutável. Resultados são fechados nos caminhos normais e descartados
automaticamente em retornos antecipados; transações inacabadas fazem rollback.
Falhas de configuração/schema encerram o processo no início; falhas durante
requisições retornam 500 genérico. O aceite `--postgres` exige banco descartável,
pois a fixture substitui/remove a tabela notes.

Esta fatia de PoC compila `main.lift` para um binário nativo e inicia um servidor
local usando os adaptadores Rust `lift_web` e `lift_db`.

```powershell
cargo run -p lift_cli -- run examples/notes-service/main.lift
```

O serviço usa `127.0.0.1:3000` por padrão. Configure `LIFT_HTTP_ADDR` para
usar outro endereço local e `LIFT_NOTES_DB` para selecionar o arquivo SQLite.
Por padrão, o arquivo é `lift-notes.sqlite3` no diretório de trabalho do comando.
SQLite é compilado junto do adaptador.

Para verificar as operações SQLite diretamente pela FFI segura existente, em
vez do serviço HTTP, execute:

```powershell
cargo run -p lift_cli -- run examples/notes-service/db-smoke.lift
```

| Requisição | Resultado |
| --- | --- |
| `GET /health` | `200`, texto `ok`. |
| `GET /notes` | `200`, uma nota por linha no formato `id<TAB>texto`; newline, tab e barra invertida no texto são escapados. |
| `POST /notes` | Armazena o corpo UTF-8 como uma nota e retorna `201` com o ID. Corpo vazio retorna `400`. |
| Rota ausente | `404` do Axum. |
| Método não permitido | `405` do Axum. |
| Corpo maior que 16 KiB | `413`. UTF-8 inválido retorna `400`. |

Exemplo de uso em outro terminal:

```powershell
Invoke-WebRequest http://127.0.0.1:3000/health
Invoke-WebRequest -Method Post -Uri http://127.0.0.1:3000/notes -Body 'Minha primeira nota' -ContentType 'text/plain; charset=utf-8'
Invoke-WebRequest http://127.0.0.1:3000/notes
```

As interfaces, os handlers síncronos nomeados e os registros `http.route`
vivem em `main.lift`. O compilador gera bridges para o adaptador genérico
`lift_web::http`. `lift_db::api` expõe pools/transações/resultados owned e não
clonáveis com moves, empréstimos imutáveis/mutáveis, commit/rollback e close.
Os handlers adquirem conexões do pool e retornam 500 genérico para
falhas de banco. Há capacidade para oito handlers bloqueantes; excesso retorna 503.

Execute `python scripts/verify_notes.py` para compilar e verificar persistência
após reinício, erros HTTP, limites exatos de corpo, porta ocupada, parâmetros SQL
e commit/rollback/drop. Execute `resources.lift` para observar recursos owned.
Consulte [o contrato e os limites](../../docs/resources-and-http.pt-BR.md).

A API SQL oferece CRUD/DDL e tipos escalares documentados em SQLite/PostgreSQL;
não promete todos os tipos, versões PostgreSQL nem suporte Oracle.
