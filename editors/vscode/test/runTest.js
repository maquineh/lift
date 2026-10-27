'use strict';

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { runTests } = require('@vscode/test-electron');

async function main() {
    const repository = path.resolve(__dirname, '../../..');
    const binary = process.platform === 'win32' ? 'lift.exe' : 'lift';
    const serverPath = path.resolve(
        process.env.LIFT_TEST_SERVER_PATH || path.join(repository, 'target', 'debug', binary)
    );
    if (!fs.existsSync(serverPath)) {
        throw new Error(`Lift server not found at ${serverPath}; build lift_cli first`);
    }

    const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'lift-vscode-test-'));
    try {
        fs.mkdirSync(path.join(workspace, '.vscode'));
        fs.writeFileSync(
            path.join(workspace, '.vscode', 'settings.json'),
            JSON.stringify({ 'lift.serverPath': serverPath, 'lift.entry': 'main.lift' }, null, 2)
        );
        fs.writeFileSync(
            path.join(workspace, 'main.lift'),
            'import module "dep.lift";\nconsole.log(1);\n'
        );
        fs.writeFileSync(
            path.join(workspace, 'dep.lift'),
            'function answer(): int { return true; }\n'
        );
        process.env.LIFT_TEST_SERVER_PATH = serverPath;
        await runTests({
            version: process.env.VSCODE_TEST_VERSION || 'stable',
            extensionDevelopmentPath: path.resolve(__dirname, '..'),
            extensionTestsPath: path.resolve(__dirname, 'suite', 'index.js'),
            launchArgs: [workspace, '--disable-workspace-trust']
        });
    } finally {
        fs.rmSync(workspace, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
    }
}

main().catch(error => {
    console.error(error);
    process.exitCode = 1;
});
