import { Minus, Square, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';

export function TitleBar() {
  const appWindow = getCurrentWindow();

  return (
    <div data-tauri-drag-region className="titlebar flex items-center justify-between h-10 px-3 bg-dark-950 border-b border-dark-700">
      <div data-tauri-drag-region className="flex items-center gap-2">
        <div className="w-4 h-4 rounded-full bg-gradient-to-br from-blue-400 to-purple-500" />
        <span className="text-sm font-medium text-dark-200">Tapo Controller</span>
      </div>
      <div className="flex items-center">
        <button onClick={() => appWindow.minimize()} className="w-10 h-10 flex items-center justify-center hover:bg-dark-800"><Minus size={14} /></button>
        <button onClick={() => appWindow.toggleMaximize()} className="w-10 h-10 flex items-center justify-center hover:bg-dark-800"><Square size={12} /></button>
        <button onClick={() => appWindow.close()} className="w-10 h-10 flex items-center justify-center hover:bg-red-500"><X size={14} /></button>
      </div>
    </div>
  );
}
