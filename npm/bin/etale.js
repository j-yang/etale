#!/usr/bin/env node
'use strict';

const { spawn } = require('child_process');
const path = require('path');

const ext = process.platform === 'win32' ? '.exe' : '';
const bin = path.join(__dirname, 'etale-bin' + ext);

const child = spawn(bin, process.argv.slice(2), { stdio: 'inherit' });
child.on('exit', function (code) { process.exit(code == null ? 1 : code); });
child.on('error', function (err) {
  console.error('etale: ' + err.message);
  process.exit(1);
});
