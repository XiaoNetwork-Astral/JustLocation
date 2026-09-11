import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { exec } from 'kernelsu';
import { App } from './App';
import { createClient } from './control';
import './style.css';

const client = createClient(async command => {
  if (!('ksu' in window)) throw new Error('请从 KernelSU 管理器打开模块面板');
  return exec(command);
});

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App client={client} />
  </StrictMode>,
);
