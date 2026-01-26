#!/usr/bin/env node
import { spawn } from 'child_process';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const rootDir = join(__dirname, '..');
const backendDir = join(rootDir, 'backend');

const isWindows = process.platform === 'win32';
const pythonCmd = isWindows ? 'py' : 'python3';
const pythonArgs = isWindows ? ['-3', '-m', 'uvicorn'] : ['-m', 'uvicorn'];

console.log('🚀 Starting Tapo Controller Dev Environment...\n');

// Start backend
console.log('📦 Starting backend...');
const backend = spawn(pythonCmd, [...pythonArgs, 'main:app', '--reload', '--host', '127.0.0.1', '--port', '8742'], {
  cwd: backendDir,
  stdio: 'pipe',
  shell: isWindows,
});

backend.stdout.on('data', (data) => {
  process.stdout.write(`[backend] ${data}`);
});
backend.stderr.on('data', (data) => {
  process.stderr.write(`[backend] ${data}`);
});

// Wait for backend to be ready
const waitForBackend = () => new Promise((resolve) => {
  const check = async () => {
    try {
      const res = await fetch('http://localhost:8742/health');
      if (res.ok) {
        console.log('✅ Backend ready!\n');
        resolve();
        return;
      }
    } catch {}
    setTimeout(check, 500);
  };
  check();
});

// Start Vite
console.log('⚡ Starting Vite...');
const vite = spawn('npx', ['vite'], {
  cwd: rootDir,
  stdio: 'pipe',
  shell: isWindows,
});

vite.stdout.on('data', (data) => {
  process.stdout.write(`[vite] ${data}`);
});
vite.stderr.on('data', (data) => {
  process.stderr.write(`[vite] ${data}`);
});

// Wait for Vite to be ready
const waitForVite = () => new Promise((resolve) => {
  const check = async () => {
    try {
      const res = await fetch('http://localhost:5173');
      if (res.ok) {
        console.log('✅ Vite ready!\n');
        resolve();
        return;
      }
    } catch {}
    setTimeout(check, 500);
  };
  check();
});

// Wait for both and start Electron
Promise.all([waitForBackend(), waitForVite()]).then(() => {
  console.log('🖥️  Starting Electron...\n');
  const electron = spawn('npx', ['electron', '.'], {
    cwd: rootDir,
    stdio: 'inherit',
    shell: isWindows,
  });

  electron.on('close', (code) => {
    console.log('\n👋 Electron closed, shutting down...');
    backend.kill();
    vite.kill();
    process.exit(code);
  });
});

// Handle Ctrl+C
process.on('SIGINT', () => {
  console.log('\n🛑 Shutting down...');
  backend.kill();
  vite.kill();
  process.exit(0);
});
