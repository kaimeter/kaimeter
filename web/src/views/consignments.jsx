// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

import { useState } from 'react';
import { PackagePlus, Upload } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import {
  Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from '@/components/ui/table';
import { api } from '@/lib/api';
import { GlossaryTerm, Hint } from '@/components/glossary';
import { useT } from '@/lib/use-i18n';

/** Sample CN codes. The full catalogue is reference data (see /api/reference). */
const SAMPLE_CN = ['72083800', '73181500', '76041010', '25232100', '31021000'];
const ORIGINS = ['CN', 'IN', 'TR', 'UA', 'ZA'];

export function Consignments() {
  const t = useT();
  const [rows, setRows] = useState([]);
  const [cn, setCn] = useState(SAMPLE_CN[0]);
  const [origin, setOrigin] = useState(ORIGINS[0]);
  const [basis, setBasis] = useState('DEFAULT');
  const [pending, setPending] = useState(false);

  const add = async () => {
    setPending(true);
    const draft = {
      cn_code: cn,
      net_mass_kg: 1000,
      country_of_origin: origin,
      production_country: origin,
      determination_basis: basis,
    };
    // Served: the row goes through the core's own validation, so the UI shows
    // exactly what the engine would accept or reject. file://: local only.
    const res = api.served
      ? await api.consignments.importSad({ rows: [draft] })
      : { ok: false, offline: true };
    setRows((prev) => [{ ...draft, saved: res.ok }, ...prev]);
    setPending(false);
  };

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <PackagePlus className="size-4" />
            {t('newConsLbl')}
          </CardTitle>
          <CardDescription className="flex items-center gap-1.5">
            <GlossaryTerm termKey="tipDossier" labelKey="consSub" />
            <Hint labelKey="tipDossier" />
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-5">
          <div className="grid gap-4 sm:grid-cols-3">
            <div className="grid gap-2">
              <Label htmlFor="cn" className="gap-1.5">
                <GlossaryTerm termKey="tipCn" labelKey="cnCode" />
                <Hint labelKey="tipCn" />
              </Label>
              <Select value={cn} onValueChange={setCn}>
                <SelectTrigger id="cn"><SelectValue /></SelectTrigger>
                <SelectContent>
                  {SAMPLE_CN.map((code) => (
                    <SelectItem key={code} value={code}>
                      <span className="font-mono text-xs">{code}</span>
                      <span className="text-muted-foreground ml-2">{t(`cn.${code}`)}</span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-2">
              <Label htmlFor="origin">{t('origin')}</Label>
              <Select value={origin} onValueChange={setOrigin}>
                <SelectTrigger id="origin"><SelectValue /></SelectTrigger>
                <SelectContent>
                  {ORIGINS.map((iso) => (
                    <SelectItem key={iso} value={iso}>{t(`country.${iso}`)}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-2">
              <Label htmlFor="basis">{t('basis')}</Label>
              <Select value={basis} onValueChange={setBasis}>
                <SelectTrigger id="basis"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="DEFAULT">{t('default')}</SelectItem>
                  <SelectItem value="ACTUAL">{t('actual')}</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>
          <p className="text-muted-foreground text-xs">
            {basis === 'ACTUAL' ? t('basisActualHint') : t('basisDefaultHint')}
          </p>
          <Button onClick={add} disabled={pending}>
            <Upload />
            {t('importSad')}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t('cons')}</CardTitle>
        </CardHeader>
        <CardContent>
          {rows.length === 0 ? (
            <p className="text-muted-foreground py-10 text-center text-sm">{t('noCons')}</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('cnCode')}</TableHead>
                  <TableHead className="text-right">{t('netMass')}</TableHead>
                  <TableHead>{t('origin')}</TableHead>
                  <TableHead>{t('basisLbl')}</TableHead>
                  <TableHead>{t('statusHdr')}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((r, i) => (
                  <TableRow key={`${r.cn_code}-${i}`}>
                    <TableCell>
                      <span className="font-mono text-xs">{r.cn_code}</span>
                      <span className="text-muted-foreground ml-2 text-xs">{t(`cn.${r.cn_code}`)}</span>
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {r.net_mass_kg.toLocaleString()}
                    </TableCell>
                    <TableCell>{t(`country.${r.country_of_origin}`)}</TableCell>
                    <TableCell>
                      <Badge variant="outline">
                        {t(r.determination_basis === 'ACTUAL' ? 'actual' : 'default')}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <Badge variant={r.saved ? 'default' : 'secondary'}>
                        {r.saved ? t('verified') : t('unverified')}
                      </Badge>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
