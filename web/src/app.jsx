// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

import { useEffect, useState } from 'react';
import { LayoutDashboard, Package, FileDown, ShieldCheck, ClipboardList, WifiOff } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { Footer } from '@/components/footer';
import { Mark } from '@/components/logo';
import { RichText } from '@/components/rich-text';
import { api } from '@/lib/api';
import { localeCodes } from '@/lib/i18n';
import { useLocale, useLocaleSwitch, useT } from '@/lib/use-i18n';
import { Dashboard } from '@/views/dashboard';
import { Consignments } from '@/views/consignments';
import { Exports } from '@/views/exports';
import { Packs } from '@/views/packs';
import { Review } from '@/views/review';

/**
 * Personas (R47). The role decides which views exist; a role with no matching
 * view simply hides that tab, so adding a persona is a data change here.
 */
const ROLES = [
  { id: 'importer', labelKey: 'roleImporter', descKey: 'roleImporterDesc', views: ['dashboard', 'consignments', 'exports'] },
  { id: 'exporter', labelKey: 'roleExporter', descKey: 'roleExporterDesc', views: ['packs'] },
  { id: 'trader', labelKey: 'roleTrader', descKey: 'roleTraderDesc', views: ['dashboard', 'packs'] },
  { id: 'verifier', labelKey: 'roleVerifier', descKey: 'roleVerifierDesc', views: ['review'] },
];

const VIEWS = {
  dashboard: { icon: LayoutDashboard, key: 'dash', Component: Dashboard },
  consignments: { icon: Package, key: 'cons', Component: Consignments },
  exports: { icon: FileDown, key: 'exp', Component: Exports },
  packs: { icon: ShieldCheck, key: 'packsTab', Component: Packs },
  review: { icon: ClipboardList, key: 'reviewTab', Component: Review },
};

const ROLE_STORAGE_KEY = 'kaimeter.role';

function readStoredRole() {
  try {
    const stored = globalThis.localStorage?.getItem(ROLE_STORAGE_KEY);
    return ROLES.some((r) => r.id === stored) ? stored : 'importer';
  } catch {
    return 'importer';
  }
}

function LanguagePicker() {
  const locale = useLocale();
  const switchLocale = useLocaleSwitch();
  const codes = localeCodes();

  // Nothing to choose between: a single embedded locale means no picker.
  if (codes.length < 2) return null;

  return (
    <Select value={locale} onValueChange={switchLocale}>
      <SelectTrigger size="sm" className="w-[8.5rem]" aria-label="Language">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {codes.map((code) => (
          <SelectItem key={code} value={code}>
            {code === 'zh-CN' ? '简体中文' : 'English'}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function OfflineBadge() {
  const t = useT();
  // The artifact is offline by construction (R22) when opened from file://, and
  // there is no server to probe in that mode.
  if (api.served) return null;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant="secondary" className="gap-1.5 font-normal">
          <WifiOff className="size-3" />
          {t('offline')}
        </Badge>
      </TooltipTrigger>
      <TooltipContent className="max-w-xs">
        <RichText text={t('etsPlainP4')} />
      </TooltipContent>
    </Tooltip>
  );
}

/** Id of the primer card, so the footer can jump to it. */
const PRIMER_ID = 'kaimeter-primer';

export function App() {
  const t = useT();
  const [role, setRole] = useState(readStoredRole);
  const [view, setView] = useState(() => ROLES.find((r) => r.id === readStoredRole())?.views[0]);
  const [serverPrice, setServerPrice] = useState(null);

  useEffect(() => {
    try {
      globalThis.localStorage?.setItem(ROLE_STORAGE_KEY, role);
    } catch {
      // Private mode: the role simply does not persist.
    }
    const first = ROLES.find((r) => r.id === role)?.views[0];
    setView(first);
  }, [role]);

  useEffect(() => {
    if (!api.served) return;
    let cancelled = false;
    api.get.price().then((res) => {
      if (!cancelled && res.ok) setServerPrice(res.data);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // The primer lives on the dashboard, so reaching it may mean switching view.
  const showPrimer = () => {
    setView('dashboard');
    requestAnimationFrame(() => {
      document.getElementById(PRIMER_ID)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    });
  };

  const activeRole = ROLES.find((r) => r.id === role) ?? ROLES[0];
  const enabledViews = activeRole.views;
  const ActiveView = VIEWS[view]?.Component ?? Dashboard;

  return (
    <div id="top" className="bg-background text-foreground flex min-h-svh flex-col">
      <header className="bg-background/95 supports-[backdrop-filter]:bg-background/60 sticky top-0 z-20 border-b backdrop-blur">
        <div className="mx-auto flex h-16 max-w-7xl items-center gap-4 px-6">
          <div className="flex items-center gap-2.5">
            <Mark className="size-9" />
            <span className="text-lg font-semibold tracking-tight">{t('brand')}</span>
          </div>

          <Separator orientation="vertical" className="!h-6" />

          <div className="flex flex-1 items-center gap-2">
            <Select value={role} onValueChange={setRole}>
              <SelectTrigger size="sm" className="w-[10.5rem]" aria-label={t('consignmentLbl')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ROLES.map((r) => (
                  <SelectItem key={r.id} value={r.id}>
                    {t(r.labelKey)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <OfflineBadge />
          <LanguagePicker />
        </div>
      </header>

      <main className="mx-auto w-full max-w-7xl flex-1 px-6 py-8">
        <div className="mb-6">
          <h1 className="text-2xl font-semibold tracking-tight">{t(activeRole.labelKey)}</h1>
          <p className="text-muted-foreground mt-1 text-sm">{t(activeRole.descKey)}</p>
        </div>

        <Tabs value={view} onValueChange={setView}>
          <TabsList>
            {enabledViews.map((id) => {
              const { icon: Icon, key } = VIEWS[id];
              return (
                <TabsTrigger key={id} value={id} className="gap-1.5">
                  <Icon />
                  {t(key)}
                </TabsTrigger>
              );
            })}
          </TabsList>

          {enabledViews.map((id) => {
            const { Component } = VIEWS[id];
            return (
              <TabsContent key={id} value={id} className="pt-6">
                <Component serverPrice={serverPrice} />
              </TabsContent>
            );
          })}
        </Tabs>
      </main>

      <Footer onShowPrimer={showPrimer} />
    </div>
  );
}

export { ROLES, VIEWS };
