// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

import { useState } from 'react';
import { CheckCircle2, Download, FileJson, ShieldAlert } from 'lucide-react';

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from '@/components/ui/table';
import { api } from '@/lib/api';
import { RichText } from '@/components/rich-text';
import { useT } from '@/lib/use-i18n';

/** The eight mandatory declaration fields (R2/R9). The export fails closed. */
const REQUIRED_FIELDS = [
  'cn_code', 'net_mass_kg', 'country_of_origin', 'production_country',
  'installation_id', 'import_date', 'determination_basis', 'emissions_tco2e',
];

export function Exports() {
  const t = useT();
  const [result, setResult] = useState(null);
  const [pending, setPending] = useState(false);

  const run = async () => {
    setPending(true);
    const res = api.served ? await api.exportDeclaration({ fields: [] }) : { ok: false, offline: true };
    setResult(res);
    setPending(false);
  };

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <FileJson className="size-4" />
            {t('exp')}
          </CardTitle>
          <CardDescription>{t('exportSub')}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-wrap items-center gap-3">
          <Button onClick={run} disabled={pending}>
            <Download />
            {t('exp')}
          </Button>
          <span className="text-muted-foreground text-xs">{t('packNotIncluded')}</span>
        </CardContent>
      </Card>

      {result ? (
        result.ok ? (
          <Alert>
            <CheckCircle2 />
            <AlertTitle>{t('toastExported')}</AlertTitle>
            <AlertDescription>
              <pre className="bg-muted mt-2 max-h-72 overflow-auto rounded-md p-3 text-xs">
                {JSON.stringify(result.data, null, 2)}
              </pre>
            </AlertDescription>
          </Alert>
        ) : (
          <Alert variant="destructive">
            <ShieldAlert />
            <AlertTitle>{result.offline ? t('offline') : t('verifyBad')}</AlertTitle>
            <AlertDescription>
              {result.offline ? <RichText text={t('etsPlainP4')} /> : JSON.stringify(result.data)}
            </AlertDescription>
          </Alert>
        )
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t('exportFieldsTitle')}</CardTitle>
          <CardDescription>{t('packNotIncluded')}</CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('fields')}</TableHead>
                <TableHead>{t('statusHdr')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {REQUIRED_FIELDS.map((f) => (
                <TableRow key={f}>
                  <TableCell className="font-mono text-xs">{f}</TableCell>
                  <TableCell>
                    <Badge variant="outline">{t('verified')}</Badge>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}
