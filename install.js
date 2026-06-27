#!/usr/bin/env node
'use strict';

const https = require('https');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const PLATFORM_MAP = {
  'darwin-arm64': { asset: 'search-proxy-mac-arm64.tar.gz', bin: 'search-proxy' },
  'darwin-x64': { asset: 'search-proxy-mac-x64.tar.gz', bin: 'search-proxy' },
  'linux-x64': { asset: 'search-proxy-linux-x64.tar.gz', bin: 'search-proxy' },
  'linux-arm64': { asset: 'search-proxy-linux-arm64.tar.gz', bin: 'search-proxy' },
  'win32-x64': { asset: 'search-proxy-windows-x64.zip', bin: 'search-proxy.exe' },
};

const key = `${process.platform}-${process.arch}`;
const plat = PLATFORM_MAP[key];
if (!plat) {
  console.error(`search-proxy: unsupported platform ${key}`);
  process.exit(1);
}

const repo = process.env.SEARCH_PROXY_REPO || 'baicai-1145/Search-Proxy';
const version = process.env.SEARCH_PROXY_VERSION || 'latest';
const vendorDir = path.join(__dirname, 'vendor');
fs.mkdirSync(vendorDir, { recursive: true });

const assetUrl =
  version === 'latest'
    ? `https://github.com/${repo}/releases/latest/download/${plat.asset}`
    : `https://github.com/${repo}/releases/download/${version}/${plat.asset}`;

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const f = fs.createWriteStream(dest);
    const req = (u) =>
      https
        .get(u, (res) => {
          if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
            res.resume();
            req(res.headers.location);
            return;
          }
          if (res.statusCode !== 200) {
            reject(new Error(`HTTP ${res.statusCode} for ${u}`));
            return;
          }
          res.pipe(f);
          f.on('finish', () => f.close(resolve));
        })
        .on('error', reject);
    req(url);
  });
}

(async () => {
  const archivePath = path.join(vendorDir, plat.asset);
  console.log(`search-proxy: downloading ${assetUrl}`);
  await download(assetUrl, archivePath);
  if (plat.asset.endsWith('.tar.gz')) {
    execSync(`tar -xzf "${archivePath}" -C "${vendorDir}"`, { stdio: 'inherit' });
  } else {
    execSync(
      `powershell -NoProfile -Command "Expand-Archive -Path '${archivePath}' -DestinationPath '${vendorDir}' -Force"`,
      { stdio: 'inherit' }
    );
  }
  const binPath = path.join(vendorDir, plat.bin);
  if (process.platform !== 'win32') fs.chmodSync(binPath, 0o755);
  console.log(`search-proxy: installed ${plat.bin} -> ${binPath}`);
})().catch((e) => {
  console.error('search-proxy: install failed:', e.message || e);
  process.exit(1);
});
