const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  minimize: () => ipcRenderer.send('window:minimize'),
  maximize: () => ipcRenderer.send('window:maximize'),
  close: () => ipcRenderer.send('window:close'),
  platform: process.platform,
  getAutoStart: () => ipcRenderer.invoke('get-auto-start'),
  setAutoStart: (enabled) => ipcRenderer.invoke('set-auto-start', enabled),
  getSavedCredentials: () => ipcRenderer.invoke('get-saved-credentials'),
  saveCredentials: (creds) => ipcRenderer.invoke('save-credentials', creds),
  clearCredentials: () => ipcRenderer.invoke('clear-credentials'),
});
