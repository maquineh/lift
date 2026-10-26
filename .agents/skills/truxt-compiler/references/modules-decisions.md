# Incremento de módulos explícitos Truxt

Contratos bilíngues completos: `docs/modules.md` / `docs/modules.pt-BR.md`.

- Sintaxe exclusiva de topo: `import module "relative.truxt";`. `module` é contextual, não keyword global. Imports FFI anteriores não mudam.
- Caminho relativo ao importador, extensão .truxt obrigatória (case-insensitive), sem interpolação/decodificação de escapes. Use `/` para portabilidade. Caminhos com raiz/drive são rejeitados; `..` é aceito.
- Canonicalização e marcação antes da recursão deduplicam fontes e permitem ciclos, inclusive pela entrada. Import não executa statements e o root entra uma vez.
- Somente root admite Declaration::Statement, sujeito às regras semânticas existentes (return exige função); módulos aceitam funções/interfaces/import FFI/outros módulos. O loader rejeita cada statement importado antes de gerar HIR.
- O grafo é achatado em uma AST para a tabela global existente. Duplicatas preservam os dois spans. Não há visibilidade por arquivo, aliases, namespaces ou manifesto.
- Span ganha SourceId e mantém offsets em bytes. Span::new/tokenize legados usam zero; tokenize_with_source e parse_with_source recebem ID. parse_with_source normaliza spans de todos os tokens e parse herda o ID de sua entrada de fonte única. Join cross-file é rejeitado.
- Compatibilidade: adicionar o campo público source é breaking para consumidores que construíam Span { start, end } diretamente. Construtores/APIs legadas preservam comportamento single-source, sem promessa de compatibilidade total.
- A CLI mantém textos/caminhos e Ariadne resolve labels por SourceId, com display de path separado da identidade. Spans AST/HIR não perdem o ID. Erros em fonte importada apontam para ela; falhas de abertura/UTF-8 apontam para o importador.
- Semântica não faz I/O: analyze rejeita ModuleImport não resolvido. Codegen recebe o mesmo HIR e produz um binário único.
- O grafo é recarregado a cada comando; preservação de arquivos gerados idênticos não é cache completo de dependências. Mudar módulo transitivo afeta a próxima compilação.
- Testes cobrem resolução relativa/transitiva, repetição/ciclos/root único, FFI/tipo/funções cruzados, erro importado/UTF-8/EOF, duplicatas entre arquivos, statement proibido e caminhos inválidos. Sem novos testes de rede.
