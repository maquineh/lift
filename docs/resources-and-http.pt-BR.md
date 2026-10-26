# Recursos SQLite owned e HTTP síncrono

[English](resources-and-http.md)

O protocolo SQLite limitado a notas abaixo permanece por compatibilidade.
Sua evolução é a [API geral de banco](../stdlib/lift_db/README.pt-BR.md): DbPool,
DbTransaction, DbRows e DbValue permitem SQL parametrizado SQLite/PostgreSQL.
Handlers HTTP agora podem receber DbPool emprestado e
`http.serve(endereço,limite,pool)` consome o pool. As restrições de ownership
descritas aqui continuam aplicáveis.

A aplicação de notas agora é escrita em Lift: interfaces, handlers, registro de
rotas e decisões de banco estão em `examples/notes-service/main.lift`.
`lift_web::http` é um adaptador independente da aplicação; bridges geradas
traduzem os DTOs Lift para os DTOs Rust.

## Protocolo fechado de recursos

`ResolvedType::Resource(ResourceKind)` distingue `SqliteConnection` e
`SqliteTransaction` das structs comuns. Handles não têm campos públicos,
igualdade nem Clone. `HirImport.parameter_modes` registra a assinatura do
adaptador; cada `HirCallArgument` registra valor, empréstimo imutável ou mutável.
`Move` representa transferências de locais owned. Uma análise semântica de fluxo
rejeita uso após consumo, consumo de parâmetro emprestado e clone de handle.

| Import de `lift_db::resource` | Contrato |
| --- | --- |
| `open(string)` | `Result<SqliteConnection, string>` |
| `begin(SqliteConnection)` | Consome conexão; `Result<SqliteTransaction, string>` |
| `insert(SqliteTransaction, string)` | Empresta transação mutavelmente; `Result<int, string>` |
| `list(SqliteConnection)` | Empresta conexão imutavelmente; `Result<string, string>` |
| `commit/rollback(SqliteTransaction)` | Consome transação; `Result<SqliteConnection, string>` |
| `close(SqliteConnection)` | Consome e descarta conexão; `void` |

Esses imports seguros são um protocolo local fechado; FFI externa arbitrária
não aceita recursos. As assinaturas continuam explícitas e Rust verifica os
ponteiros de função safe gerados. A transação possui a conexão, evitando expor
um lifetime Rust. Commit/rollback consomem a transação; uma transação inacabada
executa rollback ao sair do escopo. Falhas também descartam o handle consumido.
`close` permite descarte explícito, e o fim do escopo fornece RAII.
`result.is_ok` empresta recursos; `result.unwrap` e `unwrap_err` consomem todo
o Result. `unwrap` ainda causa panic na variante errada; verifique primeiro.

As regras de fluxo são conservadoras: recursos não podem ser campos, elementos
de array/opcional, payload de erro, captura de closure nem export de pacote.
Results aninhados com recursos ficam fora. Um move em qualquer ramo torna o
recurso indisponível depois, mesmo quando o ramo retorna. Loops não podem
consumir recursos externos. Reatribuição de recurso é rejeitada; use novo nome.
Uma expressão de chamada não pode emprestar e consumir o mesmo recurso,
inclusive em argumentos aninhados (análise conservadora).
Encaminhamento de empréstimos funciona quando a inferência existente de
parâmetros consegue prová-lo. Helpers que consomem parâmetros emprestados são
rejeitados; anotações gerais de consumo e empréstimos que escapam ficam para
outro incremento. Apenas o protocolo legado `lift_db::resource` contém operações
fixas de notas; sua evolução `lift_db::api` oferece a interface geral SQL CRUD/DDL
documentada.

## DTOs e rotas em Lift

```lift
interface Request { method: string, path: string, query: string, body: string }
interface Response { status: int, body: string }
function health(request: Request): Response { return {status: 200, body: "ok"}; }
http.route("GET", "/health", health);
const server = http.serve("127.0.0.1:3000", 16384);
```

Os nomes das interfaces podem variar; seus campos devem corresponder exatamente
a esses formatos. Handlers são funções Lift nomeadas com um parâmetro request
imutável/owned e resposta owned. Closures com captura, callbacks FFI, request
mutável, headers e formatos arbitrários de DTO ficam fora deste incremento.
A bridge gerada não captura estado e pode ser chamada repetidamente.
O registro deve ser um statement direto de topo, antes do único `http.serve`.
Pares método/caminho duplicados geram erro semântico. Métodos aceitos: GET,
POST, PUT, DELETE e PATCH. Caminhos usam letras/dígitos ASCII e `/ _ - . ~`.
Axum também atende HEAD nas rotas GET. Método e caminho são strings literais.

`http.serve` retorna `Result<void, string>`. `LIFT_HTTP_ADDR` substitui o endereço;
o exemplo usa loopback por padrão. O limite de corpo vai de 1 a 1048576 bytes,
inclusive (16384 no exemplo). UTF-8 inválido retorna 400; limite/falha de leitura,
413. Rotas desconhecidas retornam 404 e métodos incompatíveis, 405.
O adaptador define text/plain UTF-8, aceita status de resposta 200..599 e retorna
500 para status inválido/panic do handler. Oito handlers bloqueantes podem
executar simultaneamente; acima disso, retorna 503. Não há timeout prometido.
Axum/Tokio e async continuam internos ao adaptador.

Os handlers atuais emprestam o DbPool pertencente ao servidor e adquirem conexões
do pool; transações mantêm conexão exclusiva até completion/drop. Concorrência
SQLite também segue os locks do driver. Falhas de banco retornam texto genérico
com 500. `LIFT_DB_DRIVER`/`LIFT_DB_URL` escolhem o backend; `LIFT_NOTES_DB` continua
como caminho SQLite de fallback.
O import antigo `lift_web::notes` continua disponível por compatibilidade;
`main.lift` usa o adaptador genérico.

## Aceite repetível

```text
cargo test --locked --workspace
python scripts/verify_notes.py
```

O script portátil usa a biblioteca padrão Python e compila os programas nativos.
Verifica as três rotas, parâmetros SQL, persistência após reiniciar o processo,
UTF-8/corpo vazio, limites exatos de 16384/16385 bytes, 404/405/500 e falha de bind
mantendo a porta concorrente reservada. O exemplo de recursos verifica commit,
rollback explícito, rollback por drop e close, usando bancos temporários.
O workflow `notes-contract.yml` executa os comandos em Windows e Ubuntu;
configurar CI é diferente de registrar uma execução real em cada plataforma.
