import { contextBridge, ipcRenderer } from 'electron';
contextBridge.exposeInMainWorld('revueDesktop', { chooseRepo: () => ipcRenderer.invoke('choose-repo') });
