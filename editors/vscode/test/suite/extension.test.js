'use strict';

const assert = require('node:assert/strict');
const vscode = require('vscode');

function waitForDiagnostics(uri, predicate, description) {
    return new Promise((resolve, reject) => {
        let subscription;
        const timer = setTimeout(() => {
            subscription?.dispose();
            reject(new Error(
                `Timed out waiting for ${description}; current diagnostics: ${JSON.stringify(vscode.languages.getDiagnostics(uri))}`
            ));
        }, 20000);
        subscription = vscode.languages.onDidChangeDiagnostics(event => {
            if (!event.uris.some(changed => changed.toString() === uri.toString())) return;
            const diagnostics = vscode.languages.getDiagnostics(uri);
            if (predicate(diagnostics)) {
                clearTimeout(timer);
                subscription.dispose();
                resolve(diagnostics);
            }
        });
    });
}

suite('Lift VS Code extension', () => {
    test('tracks diagnostics across unsaved edits and an import path change', async () => {
        const serverPath = process.env.LIFT_TEST_SERVER_PATH;
        assert.ok(serverPath, 'test runner must provide LIFT_TEST_SERVER_PATH');
        assert.ok(vscode.workspace.workspaceFolders?.length, 'test workspace must be open');
        assert.equal(
            vscode.workspace.getConfiguration('lift').get('serverPath'),
            serverPath,
            'the test workspace must configure the built Lift server'
        );
        const workspace = vscode.workspace.workspaceFolders[0].uri;
        const mainUri = vscode.Uri.joinPath(workspace, 'main.lift');
        const dependencyUri = vscode.Uri.joinPath(workspace, 'dep.lift');
        const errorPublished = waitForDiagnostics(
            dependencyUri,
            diagnostics => diagnostics.some(diagnostic => diagnostic.code === 'sema.TypeMismatch'),
            'the imported Lift type error'
        );
        const mainDocument = await vscode.workspace.openTextDocument(mainUri);
        await vscode.window.showTextDocument(mainDocument);
        await errorPublished;

        const diagnostics = vscode.languages.getDiagnostics(dependencyUri);
        const typeError = diagnostics.find(diagnostic => diagnostic.code === 'sema.TypeMismatch');
        assert.ok(typeError, 'the language client must forward the semantic diagnostic');
        assert.match(typeError.message, /boolean.*int|int.*boolean/i);

        const dependency = await vscode.workspace.openTextDocument(dependencyUri);
        const dependencyEditor = await vscode.window.showTextDocument(dependency);
        const cleared = waitForDiagnostics(
            dependencyUri,
            diagnostics => diagnostics.length === 0,
            'diagnostic clearing after an unsaved dependency edit'
        );
        await dependencyEditor.edit(edit => {
            edit.replace(
                new vscode.Range(dependency.positionAt(0), dependency.positionAt(dependency.getText().length)),
                'function answer(): int { return 42; }\n'
            );
        });
        await cleared;
        assert.equal(dependency.isDirty, true, 'analysis must use the unsaved dependency buffer');
        assert.deepEqual(vscode.languages.getDiagnostics(dependencyUri), []);

        await dependency.save();
        await vscode.commands.executeCommand('workbench.action.closeActiveEditor');

        const movedDirectory = vscode.Uri.joinPath(workspace, 'nested');
        const movedDependency = vscode.Uri.joinPath(movedDirectory, 'dep.lift');
        await vscode.workspace.fs.createDirectory(movedDirectory);
        await vscode.workspace.fs.rename(dependencyUri, movedDependency);
        await assert.rejects(
            () => vscode.workspace.fs.stat(dependencyUri),
            'the original dependency path must be removed by the rename'
        );
        await vscode.workspace.fs.stat(movedDependency);

        const mainEditor = await vscode.window.showTextDocument(mainDocument);
        const missingDependency = waitForDiagnostics(
            mainUri,
            values => values.some(diagnostic => diagnostic.code === 'project.io'),
            'a missing import after the dependency path changes'
        );
        await mainEditor.edit(edit => {
            edit.replace(
                new vscode.Range(mainDocument.positionAt(0), mainDocument.positionAt(mainDocument.getText().length)),
                'import module "missing.lift";\n'
            );
        });
        assert.match(mainDocument.getText(), /missing\.lift/);
        await missingDependency;

        const restored = waitForDiagnostics(
            mainUri,
            values => values.length === 0,
            'diagnostic clearing after updating the import path'
        );
        await mainEditor.edit(edit => {
            edit.replace(
                new vscode.Range(mainDocument.positionAt(0), mainDocument.positionAt(mainDocument.getText().length)),
                'import module "nested/dep.lift";\nconsole.log(1);\n'
            );
        });
        await restored;
        assert.equal(mainDocument.isDirty, true, 'the updated import must be analyzed from the editor buffer');
        assert.deepEqual(vscode.languages.getDiagnostics(mainUri), []);
    });
});
