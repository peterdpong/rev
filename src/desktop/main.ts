import { app, BrowserWindow, dialog, ipcMain } from 'electron';
import path from 'node:path';
import { startServer } from '../server';

app.setName('Revue');
let service: Awaited<ReturnType<typeof startServer>>;
let window: BrowserWindow | null;
async function launch() {
  const repoArg = process.argv.indexOf('--repo');
  const baseArg = process.argv.indexOf('--base');
  const headArg = process.argv.indexOf('--head');
  service = await startServer({ uiDir: path.join(__dirname, '../ui'), initial: repoArg >= 0 ? { repo: process.argv[repoArg + 1], base: baseArg >= 0 ? process.argv[baseArg + 1] : undefined, head: headArg >= 0 ? process.argv[headArg + 1] : undefined, mode: process.argv.includes('--working') ? 'working' : 'branch' } : undefined });
  window = new BrowserWindow({ width: 1510, height: 960, minWidth: 1050, minHeight: 700, title: 'Revue', backgroundColor: '#111314', titleBarStyle: 'hiddenInset', trafficLightPosition: { x: 18, y: 18 }, webPreferences: { preload: path.join(__dirname, 'preload.cjs'), contextIsolation: true, nodeIntegration: false, sandbox: true } });
  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  window.webContents.on('will-navigate', (event, url) => { if (new URL(url).origin !== service.origin) event.preventDefault(); });
  window.webContents.session.setPermissionRequestHandler((_wc, _permission, callback) => callback(false));
  ipcMain.handle('choose-repo', async event => {
    if (event.sender !== window?.webContents || new URL(event.senderFrame?.url || 'about:blank').origin !== service.origin) throw new Error('Invalid window.');
    const result = await dialog.showOpenDialog(window!, { properties: ['openDirectory'], title: 'Open a Git repository' });
    return result.canceled ? null : result.filePaths[0];
  });
  await window.loadURL(service.url);
  window.on('closed', () => { window = null; });
}
app.whenReady().then(launch).catch(error => { dialog.showErrorBox('Could not open Revue', error.message); app.quit(); });
app.on('window-all-closed', () => app.quit());
app.on('before-quit', () => service?.close());
