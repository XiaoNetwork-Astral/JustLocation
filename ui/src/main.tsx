import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { moduleExec } from './platform';
import { App } from './App';
import { createClient } from './control';
import './style.css';

const client = createClient(moduleExec);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App client={client} />
  </StrictMode>,
);
