// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

import { useEffect, useState } from 'react';
import { AlertTriangle, ArrowUpRight, Euro, Gauge, Info, Scale } from 'lucide-react';

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Progress } from '@/components/ui/progress';
import { Separator } from '@/components/ui/separator';
import { api } from '@/lib/api';
import { tonnes } from '@/lib/format';
import { RichText } from '@/components/rich-text';
import { CbamPrimer, EtsExplainer, GlossaryTerm, Hint } from '@/components/glossary';
import { useT } from '@/lib/use-i18n';

/** The statutory exemption line, in tonnes (R1, Reg (EU) 2025/2083 Art 2a). */
const LINE_TONNES = 50;

/** The declaration year the dashboard is scoped to. */
const YEAR = new Date().getFullYear();

function Stat({ label, value, unit, hint, tipKey, icon: Icon }) {
  return (
    <Card>
      <CardHeader className="pb-3">
        <CardDescription className="flex items-center gap-1.5">
          {Icon ? <Icon className="size-3.5" /> : null}
          {label}
          {tipKey ? <Hint labelKey={tipKey} /> : null}
        </CardDescription>
        <CardTitle className="text-3xl font-semibold tabular-nums">
          {value}
          {unit ? (
            <span className="text-muted-foreground ml-1 text-base font-normal">{unit}</span>
          ) : null}
        </CardTitle>
      </CardHeader>
      {hint ? (
        <CardContent className="pt-0">
          <p className="text-muted-foreground text-xs leading-relaxed">{hint}</p>
        </CardContent>
      ) : null}
    </Card>
  );
}

export function Dashboard({ serverPrice }) {
  const t = useT();
  const [state, setState] = useState({
    status: 'loading',
    priceNeeded: false,
    deMinimis: null,
    exposure: null,
  });

  useEffect(() => {
    if (!api.served) {
      setState({ status: 'offline', priceNeeded: false, deMinimis: null, exposure: null });
      return;
    }
    let cancelled = false;
    // The exemption is per calendar year, and the core wants an explicit price
    // when its cache is cold, so fetch the price first and pass it through.
    (async () => {
      const priceRes = await api.get.price();
      const price = priceRes.ok ? priceRes.data?.price?.eur : undefined;
      const d = await api.get.deminimis(YEAR);
      // Only ask for exposure when a price exists: the core refuses to guess
      // one, and a call that can only fail is not worth making.
      const e = price != null
        ? await api.get.exposure('72083800', YEAR, price)
        : { ok: false, status: 409 };
      if (cancelled) return;
      setState({
        // `priceNeeded` is a supported state, not an error: the core refuses to
        // guess a carbon price, so without one there is nothing to project.
        status: 'ready',
        priceNeeded: !e.ok && (e.status === 409 || e.status === 400),
        deMinimis: d.ok ? d.data : null,
        exposure: e.ok ? e.data : null,
      });
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const netMassKg = Number(state.deMinimis?.ytd_net_mass_kg ?? 0);
  const netT = netMassKg / 1000;
  const pct = Math.min(100, (netT / LINE_TONNES) * 100);
  const over = netT > LINE_TONNES;
  const near = !over && pct >= 80;
  const price = state.exposure?.price?.eur ?? serverPrice?.price?.eur ?? null;

  const lineNote = over ? t('lineNoteOver') : near ? t('lineNoteNear') : t('lineNoteBelow');

  return (
    <div className="space-y-6">
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Stat
          label={t('massT')}
          value={tonnes(netT)}
          unit="t"
          icon={Scale}
          tipKey="tipMass"
          hint={`${t('netMass')}: ${netMassKg.toLocaleString()} kg`}
        />
        <Stat
          label={<GlossaryTerm termKey="tipExposure" labelKey="certCost" />}
          value={
            state.exposure?.totals?.net_eur != null
              ? `€${Number(state.exposure.totals.net_eur).toLocaleString()}`
              : '—'
          }
          icon={Euro}
          tipKey="tipExposure"
          hint={
            price != null
              ? t('exposureNote', { p: Number(price).toFixed(2) })
              : null
          }
        />
        <Stat
          label={<GlossaryTerm termKey="tipEts" labelKey="etsCached" />}
          value={price != null ? `€${Number(price).toFixed(2)}` : '—'}
          unit="/tCO₂e"
          icon={Gauge}
          tipKey="tipEts"
          hint={serverPrice?.stale ? t('unverified') : null}
        />
        <Stat
          label={<GlossaryTerm termKey="tipExposure" labelKey="factorLbl" />}
          value={
            state.exposure?.factor != null
              ? `${(state.exposure.factor * 100).toFixed(1)}%`
              : '—'
          }
          icon={ArrowUpRight}
        />
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base"><GlossaryTerm termKey="tipEmissions" labelKey="massLineTitle" /></CardTitle>
          <CardDescription className="flex items-center gap-2">
            <Badge variant="outline" className="font-mono text-[11px]">
              {t('lineLabel')}
            </Badge>
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-baseline justify-between gap-4">
            <span className="text-2xl font-semibold tabular-nums">
              {tonnes(netT)}
              <span className="text-muted-foreground ml-1 text-base font-normal">t</span>
            </span>
            <span className="text-muted-foreground text-sm tabular-nums">
              {t('pctOfLine', { p: pct.toFixed(1) })}
            </span>
          </div>
          <Progress value={pct} />
          <p className="text-sm leading-relaxed">{lineNote}</p>
          <Separator />
          <p className="text-muted-foreground text-xs leading-relaxed">
            {t('alwaysLiable', { n: 0 })}
          </p>
        </CardContent>
      </Card>

      <CbamPrimer />
      <EtsExplainer />

      {state.priceNeeded ? (
        <Alert>
          <Info />
          <AlertTitle>{t('etsCached')}</AlertTitle>
          <AlertDescription>{t('tipEts')}</AlertDescription>
        </Alert>
      ) : null}

      {state.status === 'offline' ? (
        <Alert>
          <AlertTriangle />
          <AlertTitle>{t('offline')}</AlertTitle>
          <AlertDescription>
            <RichText text={t('etsPlainP4')} />
          </AlertDescription>
        </Alert>
      ) : null}
    </div>
  );
}
