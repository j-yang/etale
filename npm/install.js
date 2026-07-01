#!/usr/bin/env node
'use strict';

const https = require('https');
const fs = require('fs');
const path = require('path');

const PLATFORM_MAP = {
  'darwin-arm64': 'etale-darwin-arm64',
  'darwin-x64': 'etale-darwin-x64',
  'linux-x64': 'etale-linux-x64',
  'win32-x64': 'etale-win32-x64.exe',
};

const key = process.platform + '-' + process.arch;
const asset = PLATFORM_MAP[key];

if (!asset) {
  console.error('etale: no prebuilt binary for ' + key);
  console.error('available: ' + Object.keys(PLATFORM_MAP).join(', '));
  process.exit(1);
}

const pkg = require('./package.json');
const version = pkg.version;
const binDir = path.join(__dirname, 'bin');
const ext = process.platform === 'win32' ? '.exe' : '';
const dest = path.join(binDir, 'etale-bin' + ext);

const url = 'https://github.com/j-yang/etale/releases/download/v' + version + '/' + asset;

function download(url, dest, redirects) {
  redirects = redirects || 0;
  if (redirects > 5) {
    console.error('etale: too many redirects');
    process.exit(1);
  }
  https.get(url, function (res) {
    if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
      res.resume();
      return download(res.headers.location, dest, redirects + 1);
    }
    if (res.statusCode !== 200) {
      console.error('etale: download failed (HTTP ' + res.statusCode + ')');
      console.error('  ' + url);
      process.exit(1);
    }
    var file = fs.createWriteStream(dest);
    res.pipe(file);
    file.on('finish', function () {
      file.close(function () {
        if (process.platform !== 'win32') {
          fs.chmodSync(dest, 0o755);
        }
        console.log('etale v' + version + ' installed');
      });
    });
    file.on('error', function (err) {
      console.error('etale: ' + err.message);
      process.exit(1);
    });
  }).on('error', function (err) {
    console.error('etale: ' + err.message);
    process.exit(1);
  });
}

download(url, dest);
