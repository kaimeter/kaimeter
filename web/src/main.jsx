// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import './index.css';
import { App } from './app';
import { detectInitialLocale, setLocale } from '@/lib/locale';
import { TooltipProvider } from '@/components/ui/tooltip';

setLocale(detectInitialLocale());

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <TooltipProvider>
      <App />
    </TooltipProvider>
  </StrictMode>,
);
