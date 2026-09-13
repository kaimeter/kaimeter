// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

import { useState } from 'react';
import { PackageOpen, ShieldCheck } from 'lucide-react';

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Separator } from '@/components/ui/separator';
import { api } from '@/lib/api';
import { RichText } from '@/components/rich-text';
import { useT } from '@/lib/use-i18n';

/**
 * Sealed data packs (R21): a Merkle-rooted, Ed25519-signed payload a counterparty
 * can verify offline, with no Kaimeter server in the loop.
 */
export function Packs() {
  const t = useT();
  const [cn, setCn] = useState('73181500');
  const [installation, setInstallation] = useState('');
  const [factor, setFactor] = useState('2.1');
  const [emissions, setEmissions] = useState('');
  const [pack, setPack] = useState(
    /** @type {import('@generated/PackSealResponse').PackSealResponse | { offline: true } | null} */ (
      null
    ),
  );
  const [pending, setPending] = useState(false);

  const seal = async () => {
    setPending(true);
    // The core validates every field and refuses rather than guessing, so the
    // whole payload is collected here: an unsealable pack never ships
    // (R16/R21).
    const res = api.served
      ? await api.sealPack({
          installation_ref: installation,
          cn_code: cn,
          emission_factor_tco2e_per_t: Number(factor),
          embedded_emissions_tco2e: Number(emissions),
          evidence_leaves: [],
          valid_until_iso: null,
        })
      : null;
    setPack(res && res.ok ? res.data : { offline: true });
    setPending(false);
  };

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <PackageOpen className="size-4" />
            {t('newPackLbl')}
          </CardTitle>
          <CardDescription>{t('packsSub')}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-5">
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="grid gap-2">
              <Label htmlFor="pack-cn">{t('cnCode')}</Label>
              <Input id="pack-cn" value={cn} onChange={(e) => setCn(e.target.value)} className="font-mono" />
              <p className="text-muted-foreground text-xs">{t(`cn.${cn}`)}</p>
            </div>
            <div className="grid gap-2">
              <Label htmlFor="pack-installation">{t('installation')}</Label>
              <Input
                id="pack-installation"
                value={installation}
                onChange={(e) => setInstallation(e.target.value)}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="pack-factor">{t('emissionsPerT')}</Label>
              <Input
                id="pack-factor"
                type="number"
                step="0.01"
                value={factor}
                onChange={(e) => setFactor(e.target.value)}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="pack-emissions">{t('emissionsTotal')}</Label>
              <Input
                id="pack-emissions"
                type="number"
                step="0.01"
                value={emissions}
                onChange={(e) => setEmissions(e.target.value)}
              />
            </div>
          </div>
          <Button onClick={seal} disabled={pending}>
            <ShieldCheck />
            {t('verifyPackFileBtn')}
          </Button>
          <p className="text-muted-foreground text-xs leading-relaxed">{t('packNotIncluded')}</p>
        </CardContent>
      </Card>

      {pack ? (
        <Alert>
          <ShieldCheck />
          <AlertTitle>{'offline' in pack ? t('offline') : t('packSealedNote')}</AlertTitle>
          <AlertDescription>
            {'offline' in pack ? (
              <RichText text={t('etsPlainP4')} />
            ) : (
              <div className="mt-3 space-y-3 text-xs">
                <div className="flex items-center gap-2">
                  <span className="text-muted-foreground">{t('verifyStatus')}</span>
                  <Badge variant="outline">{t('verified')}</Badge>
                </div>
                <Separator />
                <div className="space-y-1">
                  <div className="text-muted-foreground">{t('cnCode')}</div>
                  <code className="block break-all">{pack.pack.content.cn_code}</code>
                </div>
                <div className="space-y-1">
                  <div className="text-muted-foreground">{t('dateHdr')}</div>
                  <code className="block break-all">{pack.pack.content.issued_iso}</code>
                </div>
                <div className="space-y-1">
                  <div className="text-muted-foreground">{t('verifierCheck')}</div>
                  <code className="block break-all">{pack.pack.public_key_hex}</code>
                </div>
              </div>
            )}
          </AlertDescription>
        </Alert>
      ) : null}
    </div>
  );
}
