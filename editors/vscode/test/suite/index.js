'use strict';

const path = require('node:path');
const Mocha = require('mocha');

function run() {
    const mocha = new Mocha({ ui: 'tdd', color: true, timeout: 30000 });
    mocha.addFile(path.resolve(__dirname, 'extension.test.js'));
    return new Promise((resolve, reject) => {
        try {
            mocha.run(failures => {
                if (failures > 0) reject(new Error(`${failures} extension-host test(s) failed`));
                else resolve();
            });
        } catch (error) {
            reject(error);
        }
    });
}

module.exports = { run };
