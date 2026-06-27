#!/usr/bin/env node
'use strict';

const path = require('path');
const { spawn } = require('child_process');

const PLATFORM_MAP = {
  'darwin-arm64': 'search-proxy',
  'darwin-x64': 'search-proxy',
  'linux-x64': 'search-proxy',
  'linux-arm64': 'search-proxy',
  'win32-x64': 'search-proxy.exe',
};

const key = `${process.platform}-${process.arch}`;
const binName = PLATFORM_MAP[key];
if (!binName) {
  console.error(`search-proxy: unsupported platform ${key}`);
  process.exit(1);
}

const binPath = path.join(__dirname, 'vendor', binName);
const child = spawn(binPath, process.argv.slice(2), { stdio: 'inherit' });
child.on('exit', (code, sig) => {
  if (code != null) process.exit(code);
  if (sig) process.exit(128 + 1); // SIGTERM-ish
  process.exit(1);
});
child.on('error', (e) => {
  console.error('search-proxy: failed to launch binary:', e.message);
  process.exit(1);
});
