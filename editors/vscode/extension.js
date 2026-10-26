// EN: Small client; all Truxt analysis lives in the Rust project API.
// PT-BR: Cliente mínimo; toda análise Truxt pertence à API Rust de projeto.
'use strict';
const vscode = require('vscode');
const path = require('node:path');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');
let client;
let watcher;
let restartQueue = Promise.resolve();

function settings() {
    const config = vscode.workspace.getConfiguration('truxt');
    const folder = vscode.workspace.workspaceFolders?.[0];
    const configured = config.get('entry', '');
    const entry = configured ? path.resolve(folder?.uri.fsPath || process.cwd(), configured) : undefined;
    return { command: config.get('serverPath', 'truxt'), folder, entry };
}

async function restart(context) {
    if (client) { await client.stop(); await client.dispose(); client = undefined; }
    if (watcher) { watcher.dispose(); }
    const { command, folder, entry } = settings();
    watcher = vscode.workspace.createFileSystemWatcher('**/{*.truxt,truxt.toml}');
    const next = new LanguageClient('truxt', 'Truxt', {
        command, args: ['lsp'], transport: TransportKind.stdio,
        options: { cwd: folder?.uri.fsPath }
    }, {
        documentSelector: [{ scheme: 'file', language: 'truxt' }],
        initializationOptions: { entry: entry ? vscode.Uri.file(entry).toString() : undefined },
        synchronize: { fileEvents: watcher }
    });
    client = next;
    await next.start();
}

function task(definition, folder) {
    if (!['check', 'build', 'lint', 'run'].includes(definition.command) || typeof definition.file !== 'string') { return undefined; }
    const { command } = settings();
    const file = path.resolve(folder?.uri.fsPath || process.cwd(), definition.file);
    const execution = new vscode.ProcessExecution(command, [definition.command, file], { cwd: folder?.uri.fsPath });
    const result = new vscode.Task(definition, folder || vscode.TaskScope.Workspace, `${definition.command} ${path.basename(file)}`, 'Truxt', execution, []);
    if (definition.command === 'build') { result.group = vscode.TaskGroup.Build; }
    return result;
}

function activate(context) {
    const queueRestart = () => {
        restartQueue = restartQueue.then(() => restart(context)).catch(error => {
            vscode.window.showErrorMessage(`Truxt server: ${error.message}. Set truxt.serverPath to a built Truxt executable.`);
        });
        return restartQueue;
    };
    context.subscriptions.push(
        vscode.commands.registerCommand('truxt.restartServer', queueRestart),
        vscode.workspace.onDidChangeConfiguration(event => { if (event.affectsConfiguration('truxt')) { void queueRestart(); } }),
        vscode.tasks.registerTaskProvider('truxt', {
            provideTasks() {
                const { folder, entry } = settings();
                const active = vscode.window.activeTextEditor?.document;
                const file = entry || (active?.languageId === 'truxt' && active.uri.scheme === 'file' ? active.uri.fsPath : undefined);
                return file ? ['check', 'build', 'lint', 'run'].map(command => task({ type: 'truxt', command, file }, folder)) : [];
            },
            resolveTask(unresolved) {
                const folder = typeof unresolved.scope === 'object' ? unresolved.scope : settings().folder;
                return task(unresolved.definition, folder);
            }
        })
    );
    return queueRestart();
}

async function deactivate() {
    await restartQueue;
    if (client) { await client.stop(); await client.dispose(); client = undefined; }
    if (watcher) { watcher.dispose(); watcher = undefined; }
}
module.exports = { activate, deactivate };
